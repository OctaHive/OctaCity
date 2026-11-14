//! Issue documents through HTTP and the shared immutable publication owner.
use super::collaboration::*;
use super::*;
use octacity_server_application::testing::MutationEvidenceProbe;
use octacity_server_application::*;

fn path(issue: &str) -> String {
  format!("{}/{issue}/input-documents", root())
}
fn publication(content: &str) -> String {
  serde_json::json!({"name":"Investigation notes", "content":content}).to_string()
}

#[tokio::test]
async fn issue_documents_http_publication_has_exact_owner_without_editing_issue() {
  let f = Fixture::new().await;
  let router = f.router(Arc::new(TrustedNetworkManagementPolicy));
  let issue = create(&router, "issue").await;
  let before = f.issues.mutation_evidence_counts().await;
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &path(&issue),
      "document",
      None,
      &publication("Private Issue notes"),
    ))
    .await
    .unwrap();
  let status = response.status();
  let headers = response.headers().clone();
  let published = document(response).await;
  assert_eq!(status, StatusCode::CREATED, "{published}");
  assert_eq!(headers["cache-control"], "no-store");
  assert_eq!(published["project_id"], PROJECT);
  assert_eq!(published["issue_id"], issue);
  assert_eq!(published["name"], "Investigation notes");
  assert_eq!(published["artifact"]["encoded_size"], 19);
  assert!(published.get("content").is_none());
  assert_json_matches_component(
    &octacity_server_api_rest::v1::openapi_document(),
    "IssueDocumentResource",
    &published,
  );
  assert_eq!(
    f.issues.mutation_evidence_counts().await,
    before,
    "publication does not edit, select material or execute Issue work"
  );
}

#[tokio::test]
async fn issue_documents_http_history_is_bounded_metadata_and_body_reads_are_exact() {
  let f = Fixture::new().await;
  let router = f.router(Arc::new(TrustedNetworkManagementPolicy));
  let issue = create(&router, "history").await;
  let mut publications = Vec::new();
  for (key, content) in [("v1", "Original private notes\r\n"), ("v2", "Reviewed private notes")] {
    let response = router
      .clone()
      .oneshot(json_request("POST", &path(&issue), key, None, &publication(content)))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    publications.push(document(response).await);
  }
  assert_ne!(publications[0]["artifact"], publications[1]["artifact"]);
  let response = router
    .clone()
    .oneshot(empty_request("GET", &format!("{}?limit=1", path(&issue)), None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let first = document(response).await;
  assert_eq!(first["items"].as_array().unwrap().len(), 1);
  let cursor = first["next_cursor"].as_str().unwrap();
  let response = router
    .clone()
    .oneshot(empty_request(
      "GET",
      &format!("{}?limit=1&after={cursor}", path(&issue)),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let second = document(response).await;
  assert_eq!(second["items"].as_array().unwrap().len(), 1);
  assert!(second["next_cursor"].is_null());
  let mut observed = vec![first["items"][0].clone(), second["items"][0].clone()];
  observed.sort_by_key(|p| p["artifact"]["artifact_id"].as_str().unwrap().to_owned());
  let mut expected = publications.clone();
  expected.sort_by_key(|p| p["artifact"]["artifact_id"].as_str().unwrap().to_owned());
  assert_eq!(observed, expected, "history retains both versions without content");
  for (published, content) in publications
    .iter()
    .zip(["Original private notes\r\n", "Reviewed private notes"])
  {
    let response = router
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!(
          "{}/{}",
          path(&issue),
          published["artifact"]["artifact_id"].as_str().unwrap()
        ),
        None,
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let read = document(response).await;
    assert_eq!(read, serde_json::json!({"document":published,"content":content}));
    assert_json_matches_component(&openapi_document(), "IssueDocumentContent", &read);
  }
  assert_json_matches_component(&openapi_document(), "IssueDocumentPage", &first);
  for query in ["limit=0", "limit=201", "after=not-an-artifact", "content=true"] {
    let response = router
      .clone()
      .oneshot(empty_request("GET", &format!("{}?{query}", path(&issue)), None))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{query}");
  }
}

#[tokio::test]
async fn issue_documents_http_replay_keeps_original_actor_deadline_and_issue_security_scope() {
  let f = Fixture::new().await;
  let router = f.router(Arc::new(Rights(Arc::new(Mutex::new(BTreeSet::new())))));
  let issue = create(&router, "original").await;
  let other = create(&router, "other").await;
  let send = |issue: &str, key: &str, content: &str, actor: &str, scope: &str| {
    verified(
      json_request("POST", &path(issue), key, None, &publication(content)),
      actor,
      scope,
    )
  };
  let response = router
    .clone()
    .oneshot(send(&issue, "same-key", "original", "author", "team-a"))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let original = document(response).await;
  assert_eq!(original["actor"]["identity"], "author");
  assert_eq!(original["published_at"], 1000);
  f.clock.0.store(2000, Ordering::SeqCst);
  let response = router
    .clone()
    .oneshot(send(&issue, "new-key", "newer version", "editor", "team-a"))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let newer = document(response).await;
  assert_ne!(newer["artifact"], original["artifact"]);
  let evidence = f.store.mutation_evidence_counts().await;
  let response = router
    .clone()
    .oneshot(send(&issue, "same-key", "original", "editor", "team-a"))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  assert_eq!(document(response).await, original);
  let response = router
    .clone()
    .oneshot(send(&issue, "same-key", "tampered", "author", "team-a"))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CONFLICT);
  assert_eq!(f.store.mutation_evidence_counts().await, evidence);
  for (owner, scope) in [(&other, "team-a"), (&issue, "team-b")] {
    let response = router
      .clone()
      .oneshot(send(owner, "same-key", "original", "other-author", scope))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let fresh = document(response).await;
    assert_eq!(fresh["issue_id"], *owner);
    assert_eq!(fresh["actor"]["identity"], "other-author");
    assert_eq!(fresh["published_at"], 2000);
    assert_ne!(
      fresh["artifact"], original["artifact"],
      "replay partitions cannot disclose another receipt"
    );
  }
  f.clock
    .0
    .store(original["expires_at"].as_i64().unwrap(), Ordering::SeqCst);
  let response = router
    .clone()
    .oneshot(empty_request(
      "GET",
      &format!(
        "{}/{}",
        path(&issue),
        original["artifact"]["artifact_id"].as_str().unwrap()
      ),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    StatusCode::NOT_FOUND,
    "expiry cannot select the newer version"
  );
}

#[tokio::test]
async fn issue_documents_http_authority_is_current_and_body_rights_are_independent() {
  use ManagementResourceKind as Kind;
  let f = Fixture::new().await;
  let denied = Arc::new(Mutex::new(BTreeSet::new()));
  let router = f.router(Arc::new(Rights(denied.clone())));
  let issue = create(&router, "authority").await;
  // Issue editors need neither Project publication nor Project administration rights.
  denied.lock().unwrap().extend([
    (Kind::Project, ManagementAction::Publish),
    (Kind::Project, ManagementAction::Administer),
  ]);
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &path(&issue),
      "notes",
      None,
      &publication("private"),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let original = document(response).await;
  let body_path = format!(
    "{}/{}",
    path(&issue),
    original["artifact"]["artifact_id"].as_str().unwrap()
  );
  denied
    .lock()
    .unwrap()
    .insert((Kind::Artifact, ManagementAction::Download));
  let response = router
    .clone()
    .oneshot(empty_request("GET", &path(&issue), None))
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    StatusCode::OK,
    "metadata View does not require Download"
  );
  assert_eq!(document(response).await["items"], serde_json::json!([original]));
  assert_eq!(
    router
      .clone()
      .oneshot(empty_request("GET", &body_path, None))
      .await
      .unwrap()
      .status(),
    StatusCode::FORBIDDEN
  );
  denied.lock().unwrap().clear();
  let evidence = f.store.mutation_evidence_counts().await;
  for denial in [
    (Kind::Issue, ManagementAction::Update),
    (Kind::Issue, ManagementAction::View),
    (Kind::Project, ManagementAction::View),
  ] {
    denied.lock().unwrap().insert(denial);
    for key in ["notes", "unseen"] {
      let response = router
        .clone()
        .oneshot(json_request("POST", &path(&issue), key, None, &publication("private")))
        .await
        .unwrap();
      let expected = if denial == (Kind::Issue, ManagementAction::Update) {
        StatusCode::FORBIDDEN
      } else {
        StatusCode::BAD_REQUEST
      };
      assert_eq!(response.status(), expected, "{denial:?} {key}");
      assert!(!document(response).await.to_string().contains("private"));
    }
    if denial.1 == ManagementAction::View {
      for path in [&path(&issue), &body_path] {
        let response = router.clone().oneshot(empty_request("GET", path, None)).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "hidden parent reference");
      }
    }
    assert_eq!(
      f.store.mutation_evidence_counts().await,
      evidence,
      "deny before allocation or replay"
    );
    denied.lock().unwrap().clear();
  }
}

#[tokio::test]
async fn issue_documents_http_rejects_forged_publications_and_foreign_owner_routes() {
  let f = Fixture::new().await;
  let router = f.router(Arc::new(TrustedNetworkManagementPolicy));
  let issue = create(&router, "owner").await;
  let other = create(&router, "other").await;
  let evidence = f.store.mutation_evidence_counts().await;
  for (field, value) in [
    ("project_id", serde_json::json!(PROJECT)),
    ("issue_id", serde_json::json!(other)),
    (
      "actor",
      serde_json::json!({"kind":"authenticated_management","identity":"forged"}),
    ),
    ("artifact", serde_json::json!({})),
    ("publication_id", serde_json::json!(issue)),
    ("purpose", serde_json::json!("criteria")),
    ("published_at", serde_json::json!(1)),
  ] {
    let mut body: serde_json::Value = serde_json::from_str(&publication("private")).unwrap();
    body[field] = value;
    let response = router
      .clone()
      .oneshot(json_request("POST", &path(&issue), "notes", None, &body.to_string()))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{field}");
  }
  for content in [
    String::new(),
    " \n ".into(),
    "é".repeat(MAX_PROJECT_DOCUMENT_BYTES as usize / 2 + 1),
  ] {
    let response = router
      .clone()
      .oneshot(json_request(
        "POST",
        &path(&issue),
        "notes",
        None,
        &publication(&content),
      ))
      .await
      .unwrap();
    assert_eq!(
      response.status(),
      StatusCode::BAD_REQUEST,
      "bounded nonblank UTF-8 text"
    );
  }
  let mut request = json_request("POST", &path(&issue), "notes", None, &publication("private"));
  request.headers_mut().remove("idempotency-key");
  assert_eq!(
    router.clone().oneshot(request).await.unwrap().status(),
    StatusCode::BAD_REQUEST
  );
  let missing = "20000000-0000-4000-8000-000000000099";
  let foreign = path(&issue).replace(PROJECT, missing);
  for route in [path(missing), foreign.clone()] {
    for method in ["POST", "GET"] {
      let request = if method == "POST" {
        json_request(method, &route, "notes", None, &publication("private"))
      } else {
        empty_request(method, &route, None)
      };
      assert_eq!(
        router.clone().oneshot(request).await.unwrap().status(),
        StatusCode::NOT_FOUND,
        "{route}"
      );
    }
  }
  assert_eq!(f.store.mutation_evidence_counts().await, evidence);
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &path(&issue),
      "notes",
      None,
      &publication("private"),
    ))
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    StatusCode::CREATED,
    "rejections cannot reserve the key"
  );
  let published = document(response).await;
  let artifact = published["artifact"]["artifact_id"].as_str().unwrap();
  for route in [format!("{}/{artifact}", path(&other)), format!("{foreign}/{artifact}")] {
    let response = router
      .clone()
      .oneshot(empty_request("GET", &route, None))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(!document(response).await.to_string().contains("private"));
  }
  let response = router
    .clone()
    .oneshot(empty_request("GET", &path(&other), None))
    .await
    .unwrap();
  assert_eq!(document(response).await["items"], serde_json::json!([]));
}

#[tokio::test]
async fn issue_documents_http_excludes_task_documents_from_project_catalog_and_content() {
  let f = Fixture::new().await;
  let router = f.router(Arc::new(TrustedNetworkManagementPolicy));
  let issue = create(&router, "isolation").await;
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &path(&issue),
      "notes",
      None,
      &publication("Issue private"),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let published = document(response).await;
  let project = format!("/api/v1/projects/{PROJECT}/input-documents");
  let response = router
    .clone()
    .oneshot(empty_request("GET", &project, None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(
    document(response).await["items"],
    serde_json::json!([]),
    "Project settings and selectors share this catalog"
  );
  let response = router
    .clone()
    .oneshot(empty_request(
      "GET",
      &format!("{project}/{}", published["artifact"]["artifact_id"].as_str().unwrap()),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &project,
      "notes",
      None,
      &publication("Shared Project guidance"),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let shared = document(response).await;
  assert_json_matches_component(&openapi_document(), "ProjectDocumentResource", &shared);
  assert!(shared.get("issue_id").is_none());
  let response = router
    .clone()
    .oneshot(empty_request("GET", &project, None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(document(response).await["items"], serde_json::json!([shared]));
  let response = router
    .clone()
    .oneshot(empty_request(
      "GET",
      &format!("{project}/{}", shared["artifact"]["artifact_id"].as_str().unwrap()),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(document(response).await["content"], "Shared Project guidance");
  let response = router
    .clone()
    .oneshot(empty_request(
      "GET",
      &format!(
        "{}/{}",
        path(&issue),
        shared["artifact"]["artifact_id"].as_str().unwrap()
      ),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    StatusCode::NOT_FOUND,
    "linking is a separate reviewed operation"
  );
}

#[test]
fn issue_documents_http_contract_is_closed_bounded_and_owner_aware() {
  let doc = openapi_document();
  let schemas = &doc["components"]["schemas"];
  for name in [
    "PublishIssueDocumentRequest",
    "IssueDocumentResource",
    "IssueDocumentPage",
    "IssueDocumentContent",
  ] {
    assert_eq!(schemas[name]["additionalProperties"], false, "{name}");
  }
  assert_eq!(
    schemas["PublishIssueDocumentRequest"]["required"],
    serde_json::json!(["name", "content"])
  );
  assert_eq!(
    schemas["PublishIssueDocumentRequest"]["properties"]
      .as_object()
      .unwrap()
      .len(),
    2
  );
  assert_eq!(
    schemas["PublishIssueDocumentRequest"]["properties"]["content"]["x-max-utf8-bytes"],
    1048576
  );
  assert_eq!(schemas["IssueDocumentPage"]["properties"]["items"]["maxItems"], 200);
  let base = "/api/v1/projects/{project_id}/issues/{issue_id}/input-documents";
  for (route, method, status, operation, action, kind) in [
    (
      base.to_owned(),
      "post",
      "201",
      "publishIssueDocument",
      ManagementAction::Update,
      ManagementResourceKind::Issue,
    ),
    (
      base.to_owned(),
      "get",
      "200",
      "listIssueDocuments",
      ManagementAction::View,
      ManagementResourceKind::Artifact,
    ),
    (
      format!("{base}/{{artifact_id}}"),
      "get",
      "200",
      "readIssueDocument",
      ManagementAction::Download,
      ManagementResourceKind::Artifact,
    ),
  ] {
    let contract = &doc["paths"][&route][method];
    assert_eq!(contract["operationId"], operation);
    assert_eq!(
      contract["responses"][status]["headers"]["Cache-Control"]["schema"]["const"],
      "no-store"
    );
    assert!(
      !contract["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["name"] == "If-Match")
    );
    let mappings = management_authorization_operations();
    let mapping = mappings.iter().find(|op| op.operation_id == operation).unwrap();
    assert_eq!(mapping.authorization.action(), action);
    assert_eq!(mapping.authorization.resource_kind(), kind);
  }
}

#[tokio::test]
async fn issue_document_body_http_is_no_store_on_success_and_all_error_paths() {
  let f = Fixture::new().await;
  let denied = Arc::new(Mutex::new(BTreeSet::new()));
  let router = f.router(Arc::new(Rights(denied.clone())));
  let issue = create(&router, "body-cache-policy").await;
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &path(&issue),
      "body",
      None,
      &publication("Private exact document body"),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let published = document(response).await;
  let body_path = format!(
    "{}/{}",
    path(&issue),
    published["artifact"]["artifact_id"].as_str().unwrap()
  );
  let response = router
    .clone()
    .oneshot(empty_request("GET", &body_path, None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(
    response.headers().get("cache-control").and_then(|v| v.to_str().ok()),
    Some("no-store")
  );
  assert_eq!(document(response).await["content"], "Private exact document body");

  denied
    .lock()
    .unwrap()
    .insert((ManagementResourceKind::Artifact, ManagementAction::Download));
  let forbidden = router
    .clone()
    .oneshot(empty_request("GET", &body_path, None))
    .await
    .unwrap();
  denied.lock().unwrap().clear();
  f.set_time(published["expires_at"].as_i64().unwrap());
  let expired = router
    .clone()
    .oneshot(empty_request("GET", &body_path, None))
    .await
    .unwrap();
  let invalid_id = router
    .clone()
    .oneshot(empty_request("GET", &format!("{}/invalid", path(&issue)), None))
    .await
    .unwrap();
  let invalid_path = router
    .clone()
    .oneshot(empty_request("GET", &format!("{}/%FF", path(&issue)), None))
    .await
    .unwrap();
  let peer = Arc::new(RecordingApplication::default());
  let unavailable_router =
    management_router_with_application(|| true, recording_management_application(peer.clone(), peer));
  let unavailable = unavailable_router
    .oneshot(empty_request("GET", &body_path, None))
    .await
    .unwrap();
  for (label, response, expected) in [
    ("download denied", forbidden, StatusCode::FORBIDDEN),
    ("expired publication", expired, StatusCode::NOT_FOUND),
    ("invalid identity", invalid_id, StatusCode::BAD_REQUEST),
    ("path extraction rejected", invalid_path, StatusCode::BAD_REQUEST),
    ("unavailable owner", unavailable, StatusCode::SERVICE_UNAVAILABLE),
  ] {
    assert_eq!(response.status(), expected, "{label}");
    assert_eq!(
      response.headers().get("cache-control").and_then(|v| v.to_str().ok()),
      Some("no-store"),
      "{label}"
    );
    let body = to_bytes(response.into_body(), 256 * 1024).await.unwrap();
    assert!(
      !String::from_utf8_lossy(&body).contains("Private exact document body"),
      "{label}"
    );
  }
}
