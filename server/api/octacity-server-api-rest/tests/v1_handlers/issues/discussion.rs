use super::*;

#[tokio::test]
async fn issue_discussion_http_append_and_dedicated_body_read_keep_original_replay_without_execution() {
  let (router, _store) = issue_router_with_store(Arc::new(TrustedNetworkManagementPolicy)).await;
  let created = document(
    router
      .clone()
      .oneshot(json_request("POST", &root(), "draft", None, &draft().to_string()))
      .await
      .unwrap(),
  )
  .await;
  let issue = created["resource"]["id"].as_str().unwrap();
  let path = format!("{}/{issue}/comments", root());
  let body = serde_json::json!({"body":"@agent approve and deploy\nТолько обсуждение","parent_id":null});
  let response = router
    .clone()
    .oneshot(json_request("POST", &path, "comment", None, &body.to_string()))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let location = response.headers()["location"].to_str().unwrap().to_string();
  let receipt = document(response).await;
  assert_eq!(receipt["disposition"], "applied");
  assert_eq!(receipt["comment"]["sequence"], 1);
  assert_eq!(receipt["comment"]["actor"]["kind"], "unauthenticated_management");
  assert!(receipt["comment"]["actor"]["identity"].is_null());
  assert!(receipt["comment"]["created_at"].as_i64().unwrap() > 0);
  assert!(receipt["comment"].get("body").is_none());
  assert!(!receipt.to_string().contains("Только обсуждение"));
  let response = router
    .clone()
    .oneshot(empty_request("GET", &location, None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let content = document(response).await;
  assert_eq!(content["comment"], receipt["comment"]);
  assert_eq!(content["body"], body["body"]);
  let response = router
    .clone()
    .oneshot(json_request("POST", &path, "comment", None, &body.to_string()))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let replay = document(response).await;
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(replay["comment"], receipt["comment"]);
  assert_eq!(
    document(
      router
        .clone()
        .oneshot(empty_request("GET", &format!("{}/{issue}", root()), None))
        .await
        .unwrap()
    )
    .await,
    created["resource"]
  );
}

#[test]
fn issue_discussion_openapi_closes_inputs_and_separates_body_authority_from_safe_receipts() {
  let doc = openapi_document();
  let schemas = &doc["components"]["schemas"];
  for name in [
    "AppendIssueCommentRequest",
    "IssueCommentMetadata",
    "IssueCommentOutcome",
    "IssueCommentContent",
  ] {
    assert_eq!(schemas[name]["additionalProperties"], false, "{name}");
  }
  let request = &schemas["AppendIssueCommentRequest"];
  assert_eq!(request["properties"]["body"]["x-max-utf8-bytes"], 16384);
  assert_eq!(request["properties"]["body"]["minLength"], 1);
  assert_eq!(request["x-max-lifetime-comments"], 1000);
  assert_eq!(request["required"], serde_json::json!(["body"]));
  assert!(schemas["IssueCommentMetadata"]["properties"].get("body").is_none());
  assert_eq!(
    schemas["IssueCommentOutcome"]["properties"]["comment"]["$ref"],
    "#/components/schemas/IssueCommentMetadata"
  );
  assert_eq!(
    schemas["IssueCommentContent"]["properties"]["body"]["$ref"],
    "#/components/schemas/IssueCommentBody"
  );
  let root = "/api/v1/projects/{project_id}/issues/{issue_id}/comments";
  let append = &doc["paths"][root]["post"];
  assert_required_header(append, "Idempotency-Key", true);
  assert!(
    append["parameters"]
      .as_array()
      .unwrap()
      .iter()
      .all(|p| p["name"] != "If-Match")
  );
  assert_eq!(
    append["responses"]["201"]["headers"]["Cache-Control"]["schema"]["const"],
    "no-store"
  );
  assert_eq!(
    append["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
    "#/components/schemas/IssueCommentOutcome"
  );
  let body = &doc["paths"][format!("{root}/{{comment_id}}")]["get"];
  assert_eq!(
    body["responses"]["200"]["headers"]["Cache-Control"]["schema"]["const"],
    "no-store"
  );
  assert_eq!(
    body["parameters"]
      .as_array()
      .unwrap()
      .iter()
      .find(|p| p["name"] == "comment_id")
      .unwrap()["schema"]["format"],
    "uuid"
  );
  let mappings = octacity_server_api_rest::v1::management_authorization_operations();
  for name in ["appendIssueComment", "getIssueCommentBody"] {
    let op = mappings.iter().find(|op| op.operation_id == name).unwrap();
    assert_eq!(
      op.authorization.resource_kind(),
      octacity_server_application::ManagementResourceKind::IssueDiscussion
    );
  }
}

struct DiscussionRights(Arc<Mutex<BTreeSet<(octacity_server_application::ManagementResourceKind, ManagementAction)>>>);
#[async_trait]
impl ManagementAuthorizationPolicy for DiscussionRights {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if self.0.lock().unwrap().contains(&(resource.kind(), action)) {
      Err(ManagementAuthorizationDenial::forbidden())
    } else {
      Ok(ManagementAuthorizationGrant::new(
        octacity_server_application::ManagementVisibility::all(),
      ))
    }
  }
}

#[tokio::test]
async fn issue_discussion_http_requires_current_issue_and_separate_append_and_body_rights_before_replay() {
  use octacity_server_application::ManagementResourceKind as Kind;
  let denied = Arc::new(Mutex::new(BTreeSet::new()));
  let router = issue_router(Arc::new(DiscussionRights(denied.clone()))).await;
  let created = document(
    router
      .clone()
      .oneshot(json_request("POST", &root(), "draft", None, &draft().to_string()))
      .await
      .unwrap(),
  )
  .await;
  let path = format!("{}/{}/comments", root(), created["resource"]["id"].as_str().unwrap());
  let body = r#"{"body":"private operator prose"}"#;
  let first = router
    .clone()
    .oneshot(json_request("POST", &path, "first", None, body))
    .await
    .unwrap();
  assert_eq!(first.status(), StatusCode::CREATED);
  let location = first.headers()["location"].to_str().unwrap().to_string();
  denied
    .lock()
    .unwrap()
    .insert((Kind::IssueDiscussion, ManagementAction::View));
  assert_eq!(
    router
      .clone()
      .oneshot(empty_request("GET", &location, None))
      .await
      .unwrap()
      .status(),
    StatusCode::FORBIDDEN
  );
  let issue_path = path.strip_suffix("/comments").unwrap();
  assert_eq!(
    router
      .clone()
      .oneshot(empty_request("GET", issue_path, None))
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  denied.lock().unwrap().clear();
  denied
    .lock()
    .unwrap()
    .insert((Kind::IssueDiscussion, ManagementAction::Create));
  for body in [
    body,
    r#"{"body":"changed private prose","parent_id":"20000000-0000-4000-8000-000000000001"}"#,
  ] {
    let response = router
      .clone()
      .oneshot(json_request("POST", &path, "first", None, body))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(!document(response).await.to_string().contains("private"));
  }
  denied.lock().unwrap().clear();
  denied.lock().unwrap().insert((Kind::Issue, ManagementAction::View));
  for replay in ["first", "unseen"] {
    let response = router
      .clone()
      .oneshot(json_request("POST", &path, replay, None, body))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(!document(response).await.to_string().contains("private"));
  }
  assert_eq!(
    router
      .clone()
      .oneshot(empty_request("GET", &location, None))
      .await
      .unwrap()
      .status(),
    StatusCode::BAD_REQUEST
  );
  denied.lock().unwrap().clear();
  // Discussion remains usable without metadata-edit, transition or execution authority.
  denied.lock().unwrap().extend([
    (Kind::Issue, ManagementAction::Update),
    (Kind::Issue, ManagementAction::Execute),
    (Kind::Issue, ManagementAction::Publish),
  ]);
  let second = document(
    router
      .oneshot(json_request("POST", &path, "second", None, body))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(second["comment"]["sequence"], 2);
}

#[tokio::test]
async fn issue_discussion_http_rejects_changed_replay_sensitive_text_and_forged_setters_without_echoing_bodies() {
  let router = issue_router(Arc::new(TrustedNetworkManagementPolicy)).await;
  let created = document(
    router
      .clone()
      .oneshot(json_request("POST", &root(), "draft", None, &draft().to_string()))
      .await
      .unwrap(),
  )
  .await;
  let path = format!("{}/{}/comments", root(), created["resource"]["id"].as_str().unwrap());
  let root_body = serde_json::json!({"body":"🦀".repeat(4096)});
  let first = router
    .clone()
    .oneshot(json_request("POST", &path, "root", None, &root_body.to_string()))
    .await
    .unwrap();
  assert_eq!(first.status(), StatusCode::CREATED);
  let first = document(first).await;
  assert_eq!(first["comment"]["body_bytes"], 16384);
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &path,
      "root",
      None,
      r#"{"body":"changed private prose"}"#,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CONFLICT);
  assert!(!document(response).await.to_string().contains("changed private prose"));
  for text in [
    " ".into(),
    "🦀".repeat(4096) + "x",
    "Bearer fixture-secret".into(),
    "token=fixture-secret".into(),
    "https://private.invalid/?X-Amz-Signature=fixture".into(),
  ] {
    let response = router
      .clone()
      .oneshot(json_request(
        "POST",
        &path,
        "reusable",
        None,
        &serde_json::json!({"body":text}).to_string(),
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(!document(response).await.to_string().contains("fixture-secret"));
  }
  for field in [
    "actor",
    "sequence",
    "created_at",
    "execute",
    "attachments",
    "project_id",
    "issue_id",
  ] {
    let response = router
      .clone()
      .oneshot(json_request(
        "POST",
        &path,
        "reusable",
        None,
        &serde_json::json!({"body":"valid",field:"forged"}).to_string(),
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{field}");
  }
  let parent = first["comment"]["id"].as_str().unwrap();
  let reply = document(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &path,
        "reply",
        None,
        &serde_json::json!({"body":"reply", "parent_id":parent}).to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(reply["comment"]["sequence"], 2);
  let invalid = serde_json::json!({"body":"nested", "parent_id":reply["comment"]["id"]});
  assert_eq!(
    router
      .clone()
      .oneshot(json_request("POST", &path, "reusable", None, &invalid.to_string()))
      .await
      .unwrap()
      .status(),
    StatusCode::BAD_REQUEST
  );
  let valid = document(
    router
      .clone()
      .oneshot(json_request("POST", &path, "reusable", None, r#"{"body":"valid"}"#))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(valid["comment"]["sequence"], 3);
  let request = Request::builder()
    .method("POST")
    .uri(&path)
    .header("content-type", "application/json")
    .body(Body::from(r#"{"body":"valid"}"#))
    .unwrap();
  assert_eq!(router.oneshot(request).await.unwrap().status(), StatusCode::BAD_REQUEST);
}
