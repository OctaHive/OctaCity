use super::*;
use octacity_server_api_rest::v1::IssueManagementApplication;
use octacity_server_application::testing::{InMemoryProjectStore, manual_issue_store};
use octacity_server_application::{
  AuthorizedCommandHandler, AuthorizedQueryHandler, IssueHandlers, TrustedNetworkManagementPolicy,
};

const PROJECT: &str = "0199a6f4-d56c-7440-9aa2-6a320f862795";
fn root() -> String {
  format!("/api/v1/projects/{PROJECT}/issues")
}
async fn issue_router(policy: Arc<dyn ManagementAuthorizationPolicy>) -> axum::Router {
  issue_router_with_store(policy).await.0
}
async fn issue_router_with_store(
  policy: Arc<dyn ManagementAuthorizationPolicy>,
) -> (axum::Router, Arc<InMemoryProjectStore>) {
  issue_router_with_directory(policy, None).await
}
async fn issue_router_with_directory(
  policy: Arc<dyn ManagementAuthorizationPolicy>,
  directory: Option<Arc<dyn octacity_server_application::IssueAssigneeDirectory>>,
) -> (axum::Router, Arc<InMemoryProjectStore>) {
  let store = manual_issue_store(PROJECT.parse().unwrap()).await;
  let workflows = Arc::new(octacity_server_application::IssueWorkflowHandlers::new(store.clone()));
  let workflow_commands = Arc::new(AuthorizedCommandHandler::new(policy.clone(), workflows.clone()));
  let workflow_queries = Arc::new(AuthorizedQueryHandler::new(policy.clone(), workflows));
  let mut service = IssueHandlers::new(store.clone(), policy.clone());
  if let Some(directory) = directory {
    service = service.with_assignee_directory(directory);
  }
  let service = Arc::new(service);
  let discussion = Arc::new(octacity_server_application::IssueDiscussionHandlers::new(
    store.clone(),
    policy.clone(),
    Arc::new(octacity_server_application::SystemFactoryExecutionClock),
  ));
  let discussion_commands = Arc::new(AuthorizedCommandHandler::new(policy.clone(), discussion.clone()));
  let discussion_queries = Arc::new(AuthorizedQueryHandler::new(policy.clone(), discussion));
  let peer = Arc::new(RecordingApplication::default());
  let router = management_router_with_application(
    || true,
    recording_management_application(peer.clone(), peer)
      .with_issues(IssueManagementApplication::new(
        Arc::new(AuthorizedCommandHandler::new(policy.clone(), service.clone())),
        Arc::new(AuthorizedQueryHandler::new(policy, service)),
      ))
      .with_issue_discussion(octacity_server_api_rest::v1::IssueDiscussionManagementApplication::new(
        discussion_commands,
        discussion_queries,
      ))
      .with_issue_workflows(octacity_server_api_rest::v1::IssueWorkflowManagementApplication::new(
        workflow_commands,
        workflow_queries,
      )),
  );
  (router, store)
}
async fn document(response: axum::response::Response) -> serde_json::Value {
  serde_json::from_slice(&to_bytes(response.into_body(), 256 * 1024).await.unwrap()).unwrap()
}
fn draft() -> serde_json::Value {
  serde_json::json!({"title":"Manual draft","description":"","acceptance":null,"priority":null})
}

#[tokio::test]
async fn issue_http_manual_create_read_and_replay_need_no_factory_or_provider() {
  let router = issue_router(Arc::new(TrustedNetworkManagementPolicy)).await;
  let response = router
    .clone()
    .oneshot(json_request("POST", &root(), "draft", None, &draft().to_string()))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  assert_eq!(response.headers()["etag"], "\"1\"");
  assert_eq!(response.headers()["cache-control"], "no-store");
  let receipt = document(response).await;
  assert_eq!(receipt["resource"]["display_key"], "ISS-1");
  assert_eq!(receipt["resource"]["content"], draft());
  let id = receipt["resource"]["id"].as_str().unwrap();
  let response = router
    .clone()
    .oneshot(empty_request("GET", &format!("{}/{id}", root()), None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  assert_eq!(document(response).await, receipt["resource"]);
  let response = router
    .oneshot(json_request("POST", &root(), "draft", None, &draft().to_string()))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let replay = document(response).await;
  assert_eq!(replay["resource"], receipt["resource"]);
  assert_eq!(replay["disposition"], "replayed");
}

#[tokio::test]
async fn issue_http_edit_preserves_exact_revision_replay_and_rejects_implicit_status_setters() {
  let router = issue_router(Arc::new(TrustedNetworkManagementPolicy)).await;
  let created = document(
    router
      .clone()
      .oneshot(json_request("POST", &root(), "draft", None, &draft().to_string()))
      .await
      .unwrap(),
  )
  .await;
  let path = format!("{}/{}", root(), created["resource"]["id"].as_str().unwrap());
  let mut edited = draft();
  edited["description"] = serde_json::json!("Reviewed requirements");
  let response = router
    .clone()
    .oneshot(json_request("PATCH", &path, "edit", Some("\"1\""), &edited.to_string()))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["etag"], "\"2\"");
  let original = document(response).await;
  let mut later = edited.clone();
  later["priority"] = serde_json::json!(42);
  assert_eq!(
    router
      .clone()
      .oneshot(json_request("PATCH", &path, "later", Some("\"2\""), &later.to_string()))
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  let replay = document(
    router
      .clone()
      .oneshot(json_request("PATCH", &path, "edit", Some("\"1\""), &edited.to_string()))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(replay["resource"], original["resource"]);
  assert_eq!(replay["disposition"], "replayed");
  let response = router
    .clone()
    .oneshot(empty_request("GET", &format!("{path}/revisions/1"), None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  assert_eq!(document(response).await, created["resource"]);
  for (version, expected) in [
    (None, StatusCode::PRECONDITION_REQUIRED),
    (Some("W/\"3\""), StatusCode::BAD_REQUEST),
    (Some("*"), StatusCode::BAD_REQUEST),
    (Some("\"1\""), StatusCode::CONFLICT),
  ] {
    assert_eq!(
      router
        .clone()
        .oneshot(json_request("PATCH", &path, "another", version, &later.to_string()))
        .await
        .unwrap()
        .status(),
      expected
    );
  }
  for field in [
    "status_id",
    "workflow_id",
    "actor",
    "run_state",
    "source_owned",
    "credentials",
  ] {
    let mut invalid = later.clone();
    invalid[field] = serde_json::json!("injected");
    assert_eq!(
      router
        .clone()
        .oneshot(json_request(
          "PATCH",
          &path,
          "invalid",
          Some("\"3\""),
          &invalid.to_string()
        ))
        .await
        .unwrap()
        .status(),
      StatusCode::BAD_REQUEST,
      "{field}"
    );
  }
}

struct IssueScope(Arc<Mutex<Option<Vec<ManagementResource>>>>);
#[async_trait]
impl ManagementAuthorizationPolicy for IssueScope {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    let scope = self.0.lock().unwrap().clone();
    let visibility = if action == ManagementAction::View
      && resource.kind() == octacity_server_application::ManagementResourceKind::Issue
    {
      match scope {
        None => octacity_server_application::ManagementVisibility::all(),
        Some(ids) if ids.is_empty() => octacity_server_application::ManagementVisibility::none(),
        Some(ids) => octacity_server_application::ManagementVisibility::restricted(ids).unwrap(),
      }
    } else {
      octacity_server_application::ManagementVisibility::all()
    };
    Ok(ManagementAuthorizationGrant::new(visibility))
  }
}
#[tokio::test]
async fn issue_http_list_filters_visibility_before_limit_cursor_and_content() {
  let scope = Arc::new(Mutex::new(None));
  let router = issue_router(Arc::new(IssueScope(scope.clone()))).await;
  let mut cards = Vec::new();
  for key in ["one", "two", "three"] {
    let mut content = draft();
    content["title"] = serde_json::json!(key);
    let receipt = document(
      router
        .clone()
        .oneshot(json_request("POST", &root(), key, None, &content.to_string()))
        .await
        .unwrap(),
    )
    .await;
    cards.push(receipt["resource"].clone());
  }
  cards.sort_by_key(|c| c["id"].as_str().unwrap().to_string());
  let visible = |c: &serde_json::Value| {
    ManagementResource::instance(
      octacity_server_application::ManagementResourceKind::Issue,
      octacity_server_application::ManagementResourceIdentity::new(c["id"].as_str().unwrap()).unwrap(),
    )
    .unwrap()
  };
  *scope.lock().unwrap() = Some(vec![visible(&cards[1]), visible(&cards[2])]);
  let response = router
    .clone()
    .oneshot(empty_request("GET", &format!("{}?limit=1", root()), None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let page = document(response).await;
  assert_eq!(page["items"].as_array().unwrap().len(), 1);
  assert_eq!(page["items"][0]["id"], cards[1]["id"]);
  assert!(page["items"][0].get("content").is_none());
  assert!(page["items"][0].get("description").is_none());
  assert_eq!(page["next_cursor"], cards[1]["id"]);
  let next = format!("{}?limit=1&after={}", root(), page["next_cursor"].as_str().unwrap());
  let last = document(router.clone().oneshot(empty_request("GET", &next, None)).await.unwrap()).await;
  assert_eq!(last["items"][0]["id"], cards[2]["id"]);
  assert!(last["next_cursor"].is_null());
  for query in [
    "limit=0".to_string(),
    "limit=201".into(),
    "provider=github".into(),
    format!("after={}", cards[0]["id"].as_str().unwrap()),
  ] {
    assert_eq!(
      router
        .clone()
        .oneshot(empty_request("GET", &format!("{}?{query}", root()), None))
        .await
        .unwrap()
        .status(),
      StatusCode::BAD_REQUEST
    );
  }
  *scope.lock().unwrap() = Some(vec![visible(&cards[2])]);
  assert_eq!(
    router
      .clone()
      .oneshot(empty_request("GET", &next, None))
      .await
      .unwrap()
      .status(),
    StatusCode::BAD_REQUEST
  );
  assert_eq!(
    router
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("{}/{}", root(), cards[0]["id"].as_str().unwrap()),
        None
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::NOT_FOUND
  );
}

#[tokio::test]
async fn issue_http_revision_history_is_bounded_and_bodies_have_dedicated_no_store_reads() {
  let router = issue_router(Arc::new(TrustedNetworkManagementPolicy)).await;
  let receipt = document(
    router
      .clone()
      .oneshot(json_request("POST", &root(), "draft", None, &draft().to_string()))
      .await
      .unwrap(),
  )
  .await;
  let path = format!("{}/{}", root(), receipt["resource"]["id"].as_str().unwrap());
  let mut content = draft();
  content["description"] = serde_json::json!("Confidential requirements");
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "PATCH",
        &path,
        "edit",
        Some("\"1\""),
        &content.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  let response = router
    .clone()
    .oneshot(empty_request("GET", &format!("{path}/revisions?limit=1"), None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let page = document(response).await;
  assert_eq!(page["items"][0]["version"], 1);
  assert!(page["items"][0].get("content").is_none());
  assert_eq!(page["next_cursor"], 1);
  let last = document(
    router
      .clone()
      .oneshot(empty_request("GET", &format!("{path}/revisions?limit=1&after=1"), None))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(last["items"][0]["version"], 2);
  assert!(last["next_cursor"].is_null());
  for query in ["limit=0", "limit=201", "after=0", "after=invalid", "fields=description"] {
    assert_eq!(
      router
        .clone()
        .oneshot(empty_request("GET", &format!("{path}/revisions?{query}"), None))
        .await
        .unwrap()
        .status(),
      StatusCode::BAD_REQUEST
    );
  }
}

async fn install_policy(router: &axum::Router) -> serde_json::Value {
  let mut definition = support::issue_workflow_definition();
  let initial = definition["statuses"][0]["id"].clone();
  let ready = definition["statuses"][1]["id"].clone();
  let declined = definition["statuses"][2]["id"].clone();
  definition["transitions"] = serde_json::json!([
    {"id":"30000000-0000-4000-8000-000000000001","name":"Organizational move","from":initial,"to":ready,"origins":["manual"],"evidence":[]},
    {"id":"30000000-0000-4000-8000-000000000002","name":"Guarded move","from":initial,"to":declined,"origins":["manual"],"evidence":["accepted_execution"]},
    {"id":"30000000-0000-4000-8000-000000000003","name":"Accepted pipeline action","from":initial,"to":declined,"origins":["pipeline"],"evidence":[]}
  ]);
  let path = format!("/api/v1/projects/{PROJECT}/issue-workflows");
  let request = serde_json::json!({"id":"20000000-0000-4000-8000-000000000001","definition":definition});
  let response = router
    .clone()
    .oneshot(json_request("POST", &path, "workflow", None, &request.to_string()))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let request = serde_json::json!({"workflow_id":"20000000-0000-4000-8000-000000000001","workflow_version":1});
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("/api/v1/projects/{PROJECT}/issue-workflow-default"),
      "select",
      Some("\"1\""),
      &request.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  definition
}
#[tokio::test]
async fn issue_http_manual_moves_use_exact_policy_status_and_history_without_execution() {
  let (router, store) = issue_router_with_store(Arc::new(TrustedNetworkManagementPolicy)).await;
  let policy = install_policy(&router).await;
  let created = document(
    router
      .clone()
      .oneshot(json_request("POST", &root(), "draft", None, &draft().to_string()))
      .await
      .unwrap(),
  )
  .await;
  let card = &created["resource"];
  let path = format!("{}/{}", root(), card["id"].as_str().unwrap());
  let response = router
    .clone()
    .oneshot(empty_request("GET", &format!("{path}/transitions"), None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let choices = document(response).await;
  assert_eq!(choices["issue_version"], 1);
  assert_eq!(choices["items"].as_array().unwrap().len(), 3);
  assert_eq!(choices["items"][0]["available"], true);
  assert_eq!(choices["items"][1]["reason"], "evidence_required");
  assert_eq!(choices["items"][2]["reason"], "origin_not_allowed");
  let request = serde_json::json!({"workflow_id":card["workflow_id"],"workflow_version":1,"expected_status_id":card["status_id"],"transition_id":policy["transitions"][0]["id"],"reason":"Organizational only"});
  let owned = octacity_server_application::IssueFieldPolicy {
    source_owned: [octacity_server_application::IssueField::Status].into(),
    ..Default::default()
  };
  store
    .seed_issue_field_policy(
      PROJECT.parse().unwrap(),
      card["id"].as_str().unwrap().parse().unwrap(),
      owned,
    )
    .unwrap();
  let blocked = document(
    router
      .clone()
      .oneshot(empty_request("GET", &format!("{path}/transitions"), None))
      .await
      .unwrap(),
  )
  .await;
  assert!(
    blocked["items"]
      .as_array()
      .unwrap()
      .iter()
      .all(|i| i["available"] == false && i["reason"] == "source_owned")
  );
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("{path}/transitions"),
        "owned",
        Some("\"1\""),
        &request.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::CONFLICT
  );
  store
    .seed_issue_field_policy(
      PROJECT.parse().unwrap(),
      card["id"].as_str().unwrap().parse().unwrap(),
      Default::default(),
    )
    .unwrap();
  for index in [1, 2] {
    let mut invalid = request.clone();
    invalid["transition_id"] = policy["transitions"][index]["id"].clone();
    assert_eq!(
      router
        .clone()
        .oneshot(json_request(
          "POST",
          &format!("{path}/transitions"),
          "forbidden",
          Some("\"1\""),
          &invalid.to_string()
        ))
        .await
        .unwrap()
        .status(),
      StatusCode::CONFLICT
    );
  }
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{path}/transitions"),
      "move",
      Some("\"1\""),
      &request.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["etag"], "\"2\"");
  let moved = document(response).await;
  assert_eq!(moved["resource"]["status_id"], policy["statuses"][1]["id"]);
  assert_eq!(moved["transition"]["from"], card["status_id"]);
  assert_eq!(moved["transition"]["origin"], "manual");
  let response = router
    .clone()
    .oneshot(empty_request("GET", &format!("{path}/status-history?limit=1"), None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let first = document(response).await;
  assert_eq!(first["items"][0]["version"], 1);
  assert_eq!(first["next_cursor"], 1);
  let next = document(
    router
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("{path}/status-history?after=1&limit=1"),
        None,
      ))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(next["items"][0], moved["transition"]);
  assert!(next["next_cursor"].is_null());
  let replay = document(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("{path}/transitions"),
        "move",
        Some("\"1\""),
        &request.to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(replay["resource"], moved["resource"]);
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(
    router
      .oneshot(json_request(
        "POST",
        &format!("{path}/transitions"),
        "stale",
        Some("\"2\""),
        &request.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::CONFLICT
  );
}

struct IssueRights(Arc<Mutex<BTreeSet<ManagementAction>>>);
#[async_trait]
impl ManagementAuthorizationPolicy for IssueRights {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    action: ManagementAction,
    _: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if self.0.lock().unwrap().contains(&action) {
      Err(ManagementAuthorizationDenial::forbidden())
    } else {
      Ok(ManagementAuthorizationGrant::new(
        octacity_server_application::ManagementVisibility::all(),
      ))
    }
  }
}
#[tokio::test]
async fn issue_http_adoption_requires_review_current_policy_and_separate_admin_rights() {
  let denied = Arc::new(Mutex::new(BTreeSet::new()));
  let (router, store) = issue_router_with_store(Arc::new(IssueRights(denied.clone()))).await;
  let receipt = document(
    router
      .clone()
      .oneshot(json_request("POST", &root(), "draft", None, &draft().to_string()))
      .await
      .unwrap(),
  )
  .await;
  let card = &receipt["resource"];
  let path = format!("{}/{}", root(), card["id"].as_str().unwrap());
  let target = install_policy(&router).await;
  let review = document(
    router
      .clone()
      .oneshot(empty_request("GET", &format!("{path}/transitions"), None))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(review["workflow_id"], card["workflow_id"]);
  let mut correspondence = serde_json::Map::new();
  correspondence.insert(
    card["status_id"].as_str().unwrap().into(),
    target["initial_status_id"].clone(),
  );
  let request = serde_json::json!({"expected_workflow_id":card["workflow_id"],"expected_workflow_version":card["workflow_version"],"expected_status_id":card["status_id"],"target_workflow_id":"20000000-0000-4000-8000-000000000001","target_workflow_version":1,"status_correspondence":correspondence,"expected_field_policy":review["field_policy"]});
  let route = format!("{path}/workflow-adoption");
  denied.lock().unwrap().insert(ManagementAction::Administer);
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &route,
        "adopt",
        Some("\"1\""),
        &request.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::FORBIDDEN
  );
  denied.lock().unwrap().clear();
  for field in ["actor", "run_state", "workflow_id"] {
    let mut invalid = request.clone();
    invalid[field] = serde_json::json!("forged");
    assert_eq!(
      router
        .clone()
        .oneshot(json_request(
          "POST",
          &route,
          "invalid",
          Some("\"1\""),
          &invalid.to_string()
        ))
        .await
        .unwrap()
        .status(),
      StatusCode::BAD_REQUEST
    );
  }
  let mut invalid = request.clone();
  invalid["expected_field_policy"]["execute"] = serde_json::json!(true);
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &route,
        "invalid-policy",
        Some("\"1\""),
        &invalid.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::BAD_REQUEST
  );
  let mut invalid = request.clone();
  invalid["status_correspondence"] = serde_json::json!({});
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &route,
        "empty",
        Some("\"1\""),
        &invalid.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::BAD_REQUEST
  );
  store
    .seed_issue_field_policy(
      PROJECT.parse().unwrap(),
      card["id"].as_str().unwrap().parse().unwrap(),
      octacity_server_application::IssueFieldPolicy {
        source_owned: [octacity_server_application::IssueField::Status].into(),
        ..Default::default()
      },
    )
    .unwrap();
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &route,
        "owned",
        Some("\"1\""),
        &request.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::CONFLICT
  );
  store
    .seed_issue_field_policy(
      PROJECT.parse().unwrap(),
      card["id"].as_str().unwrap().parse().unwrap(),
      Default::default(),
    )
    .unwrap();
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &route,
      "adopt",
      Some("\"1\""),
      &request.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["etag"], "\"2\"");
  let adopted = document(response).await;
  assert_eq!(adopted["resource"]["workflow_id"], request["target_workflow_id"]);
  assert_eq!(adopted["transition"]["previous_workflow"]["id"], card["workflow_id"]);
  assert_eq!(adopted["resource"]["content"], card["content"]);
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "PATCH",
        &path,
        "later",
        Some("\"2\""),
        &draft().to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  let replay = document(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &route,
        "adopt",
        Some("\"1\""),
        &request.to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(replay["resource"], adopted["resource"]);
  assert_eq!(replay["transition"], adopted["transition"]);
  assert_eq!(replay["disposition"], "replayed");
}

#[test]
fn issue_openapi_describes_closed_bounded_no_store_and_replay_contracts() {
  let doc = openapi_document();
  let schemas = &doc["components"]["schemas"];
  let paths = &doc["paths"];
  let base = "/api/v1/projects/{project_id}/issues";
  assert_eq!(
    paths[base]["post"]["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
    "#/components/schemas/IssueMutationResponse"
  );
  for (suffix, method) in [
    ("", "get"),
    ("/{issue_id}", "get"),
    ("/{issue_id}/revisions/{version}", "get"),
    ("/{issue_id}/transitions", "get"),
    ("/{issue_id}/status-history", "get"),
    ("/{issue_id}/revisions", "get"),
  ] {
    let op = &paths[format!("{base}{suffix}")][method];
    assert_eq!(
      op["responses"]["200"]["headers"]["Cache-Control"]["schema"]["const"],
      "no-store"
    );
    if suffix.contains("{issue_id}") {
      let parameter = op["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "issue_id")
        .unwrap();
      assert_eq!(parameter["schema"]["format"], "uuid");
    }
  }
  for schema in [
    "IssueContent",
    "TransitionIssueRequest",
    "AdoptIssueWorkflowRequest",
    "IssueFieldPolicy",
    "IssuePushPolicy",
    "IssueResource",
    "IssueSummary",
    "IssueRevisionHeader",
    "IssueTransitionChoices",
    "IssueStatusHistoryEntry",
  ] {
    assert_eq!(schemas[schema]["additionalProperties"], false, "{schema}");
  }
  assert_eq!(
    schemas["IssueContent"]["properties"]["description"]["x-max-utf8-bytes"],
    65536
  );
  assert_eq!(schemas["IssueContent"]["properties"]["title"]["x-max-utf8-bytes"], 128);
  for schema in ["IssueSummaryPage", "IssueRevisionPage", "IssueStatusHistoryPage"] {
    assert_eq!(schemas[schema]["properties"]["items"]["maxItems"], 200);
  }
  assert_eq!(
    schemas["IssueTransitionChoices"]["properties"]["items"]["maxItems"],
    256
  );
  assert_eq!(
    schemas["AdoptIssueWorkflowRequest"]["properties"]["status_correspondence"]["maxProperties"],
    64
  );
  for (suffix, method) in [
    ("/{issue_id}", "patch"),
    ("/{issue_id}/transitions", "post"),
    ("/{issue_id}/workflow-adoption", "post"),
  ] {
    let op = &paths[format!("{base}{suffix}")][method];
    assert_required_header(op, "If-Match", true);
    assert_required_header(op, "Idempotency-Key", true);
  }
}

#[tokio::test]
async fn issue_http_text_edit_rights_do_not_offer_or_authorize_manual_moves_or_policy_administration() {
  let denied = Arc::new(Mutex::new(BTreeSet::new()));
  let router = issue_router(Arc::new(IssueRights(denied.clone()))).await;
  let policy = install_policy(&router).await;
  let created = document(
    router
      .clone()
      .oneshot(json_request("POST", &root(), "draft", None, &draft().to_string()))
      .await
      .unwrap(),
  )
  .await;
  let card = &created["resource"];
  let path = format!("{}/{}", root(), card["id"].as_str().unwrap());
  *denied.lock().unwrap() = [
    ManagementAction::Publish,
    ManagementAction::Administer,
    ManagementAction::Execute,
  ]
  .into();
  let choices = document(
    router
      .clone()
      .oneshot(empty_request("GET", &format!("{path}/transitions"), None))
      .await
      .unwrap(),
  )
  .await;
  assert!(
    choices["items"]
      .as_array()
      .unwrap()
      .iter()
      .all(|c| c["available"] == false && c["reason"] == "permission_required")
  );
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "PATCH",
        &path,
        "edit",
        Some("\"1\""),
        &draft().to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  let movement = serde_json::json!({"workflow_id":card["workflow_id"],"workflow_version":1,"expected_status_id":card["status_id"],"transition_id":policy["transitions"][0]["id"],"reason":null});
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &format!("{path}/transitions"),
        "move",
        Some("\"2\""),
        &movement.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::FORBIDDEN
  );
  let response = router
    .oneshot(json_request(
      "POST",
      &format!("/api/v1/projects/{PROJECT}/issue-workflows"),
      "publish",
      None,
      &serde_json::json!({"id":"20000000-0000-4000-8000-000000000002","definition":policy}).to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn issue_http_reason_limits_are_explicit_utf8_bytes_in_openapi_and_runtime() {
  let doc = openapi_document();
  let schemas = &doc["components"]["schemas"];
  assert_eq!(
    schemas["TransitionIssueRequest"]["properties"]["reason"]["anyOf"][0]["x-max-utf8-bytes"],
    128
  );
  assert_eq!(
    schemas["IssueStatusHistoryEntry"]["properties"]["reason"]["anyOf"][0]["x-max-utf8-bytes"],
    128
  );
  let router = issue_router(Arc::new(TrustedNetworkManagementPolicy)).await;
  let policy = install_policy(&router).await;
  let created = document(
    router
      .clone()
      .oneshot(json_request("POST", &root(), "draft", None, &draft().to_string()))
      .await
      .unwrap(),
  )
  .await;
  let card = &created["resource"];
  let path = format!("{}/{}/transitions", root(), card["id"].as_str().unwrap());
  let mut request = serde_json::json!({"workflow_id":card["workflow_id"],"workflow_version":1,"expected_status_id":card["status_id"],"transition_id":policy["transitions"][0]["id"],"reason":"я".repeat(65)});
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &path,
      "oversized-reason",
      Some("\"1\""),
      &request.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  request["reason"] = serde_json::json!("я".repeat(64));
  let response = router
    .oneshot(json_request(
      "POST",
      &path,
      "bounded-reason",
      Some("\"1\""),
      &request.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(document(response).await["transition"]["reason"], "я".repeat(64));
}

#[path = "issues/planning.rs"]
mod planning;

#[path = "issues/executions.rs"]
mod executions;

#[path = "issues/sources.rs"]
mod sources;

#[path = "issues/discussion.rs"]
mod discussion;
