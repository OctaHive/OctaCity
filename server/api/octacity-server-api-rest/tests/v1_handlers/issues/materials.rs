//! Explicit material selection through HTTP and the existing Issue/source owners.
use super::collaboration::{Fixture, create};
use super::*;
use octacity_server_application::*;
fn issue_path(issue: &str) -> String {
  format!("{}/{issue}", root())
}
fn selection(references: &[serde_json::Value]) -> String {
  serde_json::json!({"references":references}).to_string()
}
async fn get(router: &axum::Router, path: &str) -> serde_json::Value {
  let response = router.clone().oneshot(empty_request("GET", path, None)).await.unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  document(response).await
}
async fn comment(router: &axum::Router, issue: &str, key: &str) -> serde_json::Value {
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &format!("{}/comments", issue_path(issue)),
      key,
      None,
      r#"{"body":"Private selected discussion"}"#,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let receipt = document(response).await;
  let comment = &receipt["comment"];
  serde_json::json!({"kind":"comment","project_id":PROJECT,"issue_id":issue,"comment_id":comment["id"],"body_digest":comment["body_digest"]})
}
#[tokio::test]
async fn issue_materials_http_select_remove_history_and_original_replay() {
  let f = Fixture::new().await;
  let router = f.router(Arc::new(TrustedNetworkManagementPolicy));
  let issue = create(&router, "material-issue").await;
  f.set_time(SystemFactoryExecutionClock.now().unwrap().unix_millis() + 1000);
  let reference = comment(&router, &issue, "comment").await;
  let path = issue_path(&issue);
  let before = get(&router, &path).await;
  assert!(before.get("materials").is_none(), "posting never selects input");
  let body = selection(std::slice::from_ref(&reference));
  let response = router
    .clone()
    .oneshot(json_request(
      "PATCH",
      &format!("{path}/materials"),
      "select",
      Some("\"1\""),
      &body,
    ))
    .await
    .unwrap();
  let status = response.status();
  let headers = response.headers().clone();
  let original = document(response).await;
  assert_eq!(status, StatusCode::OK, "{original}");
  assert_eq!(headers["etag"], "\"2\"");
  assert_eq!(headers["cache-control"], "no-store");
  assert_eq!(original["disposition"], "applied");
  assert_eq!(original["resource"]["materials"][0]["reference"], reference);
  assert_eq!(
    original["resource"]["materials"][0]["selected_by"]["kind"],
    "unauthenticated_management"
  );
  assert!(original["resource"]["materials"][0]["selected_at"].as_i64().unwrap() > 0);
  assert!(!original.to_string().contains("Private selected discussion"));
  assert_eq!(original["resource"]["content"], before["content"]);
  assert_eq!(
    original["resource"]["applicability_version"],
    before["applicability_version"]
  );
  assert_eq!(get(&router, &path).await, original["resource"]);
  assert_eq!(get(&router, &format!("{path}/revisions/2")).await, original["resource"]);
  assert_json_matches_component(&openapi_document(), "IssueMutationResponse", &original);
  let response = router
    .clone()
    .oneshot(json_request(
      "PATCH",
      &format!("{path}/materials"),
      "remove",
      Some("\"2\""),
      &selection(&[]),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let removed = document(response).await;
  assert_eq!(removed["resource"]["version"], 3);
  assert!(removed["resource"].get("materials").is_none());
  let response = router
    .clone()
    .oneshot(json_request(
      "PATCH",
      &format!("{path}/materials"),
      "select",
      Some("\"1\""),
      &body,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["etag"], "\"2\"");
  let replay = document(response).await;
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(replay["resource"], original["resource"]);
  assert_eq!(get(&router, &path).await, removed["resource"]);
  assert_eq!(get(&router, &format!("{path}/revisions/2")).await, original["resource"]);
  assert_eq!(
    get(
      &router,
      &format!("{path}/comments/{}", reference["comment_id"].as_str().unwrap())
    )
    .await["body"],
    "Private selected discussion"
  );
}

async fn edit(
  router: &axum::Router,
  issue: &str,
  key: &str,
  version: &str,
  references: &[serde_json::Value],
) -> axum::response::Response {
  router
    .clone()
    .oneshot(json_request(
      "PATCH",
      &format!("{}/materials", issue_path(issue)),
      key,
      Some(version),
      &selection(references),
    ))
    .await
    .unwrap()
}
async fn publish(router: &axum::Router, path: &str, key: &str, content: &str) -> serde_json::Value {
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      path,
      key,
      None,
      &serde_json::json!({"name":"Notes","content":content}).to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  document(response).await
}
fn owned(issue: &str, doc: &serde_json::Value) -> serde_json::Value {
  serde_json::json!({"kind":"issue_document","project_id":PROJECT,"issue_id":issue,"artifact":doc["artifact"]})
}
fn shared(doc: &serde_json::Value) -> serde_json::Value {
  serde_json::json!({"kind":"project_document","project_id":PROJECT,"artifact":doc["artifact"]})
}

#[tokio::test]
async fn issue_materials_http_pins_both_document_owners_and_preserves_selection_attribution() {
  use super::collaboration::{Rights, verified};
  let f = Fixture::new().await;
  let router = f.router(Arc::new(Rights(Arc::new(Mutex::new(BTreeSet::new())))));
  let issue = create(&router, "document-materials").await;
  let now = SystemFactoryExecutionClock.now().unwrap().unix_millis() + 1000;
  f.set_time(now);
  let path = issue_path(&issue);
  let comment = comment(&router, &issue, "comment").await;
  let document_path = format!("{path}/input-documents");
  let project_path = format!("/api/v1/projects/{PROJECT}/input-documents");
  let original_doc = publish(&router, &document_path, "v1", "Private Issue document").await;
  let project_doc = publish(&router, &project_path, "shared", "Private Project document").await;
  let mut references = vec![comment.clone(), owned(&issue, &original_doc), shared(&project_doc)];
  assert!(get(&router, &path).await.get("materials").is_none());
  let response = router
    .clone()
    .oneshot(verified(
      json_request(
        "PATCH",
        &format!("{path}/materials"),
        "select",
        Some("\"1\""),
        &selection(&references),
      ),
      "selector",
      "team",
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let original = document(response).await;
  let entries = original["resource"]["materials"].as_array().unwrap();
  assert_eq!(entries.len(), 3);
  for (entry, reference) in entries.iter().zip(&references) {
    assert_eq!(entry["reference"], *reference);
    assert_eq!(entry["selected_by"]["identity"], "selector");
    assert_eq!(entry["selected_at"], now);
  }
  assert!(!original.to_string().contains("Private"));
  assert_json_matches_component(&openapi_document(), "IssueMutationResponse", &original);
  f.set_time(now + 1000);
  let newer = publish(&router, &document_path, "v2", "Newer private Issue document").await;
  assert_eq!(
    get(&router, &path).await,
    original["resource"],
    "publication never retargets the selected Artifact"
  );
  references[1] = owned(&issue, &newer);
  let response = router
    .clone()
    .oneshot(verified(
      json_request(
        "PATCH",
        &format!("{path}/materials"),
        "replace",
        Some("\"2\""),
        &selection(&references),
      ),
      "editor",
      "team",
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let replaced = document(response).await;
  assert_eq!(replaced["resource"]["version"], 3);
  assert_eq!(replaced["resource"]["materials"][0], entries[0]);
  assert_eq!(replaced["resource"]["materials"][2], entries[2]);
  assert_eq!(
    replaced["resource"]["materials"][1]["selected_by"]["identity"],
    "editor"
  );
  assert_eq!(replaced["resource"]["materials"][1]["selected_at"], now + 1000);
  assert_eq!(get(&router, &format!("{path}/revisions/2")).await, original["resource"]);
  let response = edit(&router, &issue, "remove", "\"3\"", &[]).await;
  assert_eq!(response.status(), StatusCode::OK);
  assert!(document(response).await["resource"].get("materials").is_none());
  assert_eq!(
    get(
      &router,
      &format!(
        "{document_path}/{}",
        original_doc["artifact"]["artifact_id"].as_str().unwrap()
      )
    )
    .await["content"],
    "Private Issue document"
  );
  assert_eq!(
    get(
      &router,
      &format!(
        "{project_path}/{}",
        project_doc["artifact"]["artifact_id"].as_str().unwrap()
      )
    )
    .await["content"],
    "Private Project document"
  );
}

#[tokio::test]
async fn issue_materials_http_rejects_forged_owners_bodies_digests_and_excess_without_effects() {
  use octacity_server_application::testing::MutationEvidenceProbe;
  let f = Fixture::new().await;
  let router = f.router(Arc::new(TrustedNetworkManagementPolicy));
  let issue = create(&router, "guards").await;
  let other = create(&router, "other").await;
  f.set_time(SystemFactoryExecutionClock.now().unwrap().unix_millis() + 1000);
  let comment = comment(&router, &issue, "comment").await;
  let original_doc = publish(
    &router,
    &format!("{}/input-documents", issue_path(&issue)),
    "v1",
    "Private original",
  )
  .await;
  let other_doc = publish(
    &router,
    &format!("{}/input-documents", issue_path(&other)),
    "other",
    "Private other",
  )
  .await;
  let project_doc = publish(
    &router,
    &format!("/api/v1/projects/{PROJECT}/input-documents"),
    "shared",
    "Private shared",
  )
  .await;
  let before = get(&router, &issue_path(&issue)).await;
  let evidence = (
    f.issues.mutation_evidence_counts().await,
    f.store.mutation_evidence_counts().await,
  );
  let mut invalid = vec![
    owned(&other, &other_doc),
    owned(&issue, &other_doc),
    shared(&original_doc),
    owned(&issue, &project_doc),
  ];
  for (source, field, value) in [
    (comment.clone(), "body_digest", serde_json::json!("11".repeat(32))),
    (comment.clone(), "issue_id", serde_json::json!(other)),
    (
      comment.clone(),
      "project_id",
      serde_json::json!("20000000-0000-4000-8000-000000000099"),
    ),
    (
      shared(&project_doc),
      "project_id",
      serde_json::json!("20000000-0000-4000-8000-000000000099"),
    ),
    (comment.clone(), "body", serde_json::json!("forged body")),
    (
      owned(&issue, &original_doc),
      "selected_by",
      serde_json::json!({"kind":"authenticated_management","identity":"forged"}),
    ),
    (owned(&issue, &original_doc), "selected_at", serde_json::json!(1)),
  ] {
    let mut reference = source;
    reference[field] = value;
    invalid.push(reference);
  }
  let mut wrong_digest = owned(&issue, &original_doc);
  wrong_digest["artifact"]["content_digest"] = serde_json::json!("11".repeat(32));
  invalid.push(wrong_digest);
  let mut wrong_size = owned(&issue, &original_doc);
  wrong_size["artifact"]["encoded_size"] = serde_json::json!(1);
  invalid.push(wrong_size);
  let mut cases: Vec<Vec<serde_json::Value>> = invalid.into_iter().map(|v| vec![v]).collect();
  cases.push(vec![comment.clone(), comment.clone()]);
  cases.push(vec![owned(&issue, &original_doc), shared(&original_doc)]);
  // Distinct IDs keep the count rejection independent of duplicate rejection.
  cases.push(
    (1..=33)
      .map(|id| {
        let mut r = comment.clone();
        r["comment_id"] = serde_json::json!(format!("20000000-0000-4000-8000-{id:012}"));
        r
      })
      .collect(),
  );
  for references in cases {
    let response = edit(&router, &issue, "reusable-after-rejection", "\"1\"", &references).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{references:?}");
    assert!(!document(response).await.to_string().contains("Private"));
    assert_eq!(get(&router, &issue_path(&issue)).await, before);
    assert_eq!(
      (
        f.issues.mutation_evidence_counts().await,
        f.store.mutation_evidence_counts().await
      ),
      evidence
    );
  }
  for body in [
    serde_json::json!({}),
    serde_json::json!({"references":[],"actor":"forged"}),
    serde_json::json!({"references":[],"project_id":PROJECT}),
    serde_json::json!({"references":[],"expected_version":1}),
    serde_json::json!({"references":[],"issue_id":other}),
  ] {
    let response = router
      .clone()
      .oneshot(json_request(
        "PATCH",
        &format!("{}/materials", issue_path(&issue)),
        "invalid",
        Some("\"1\""),
        &body.to_string(),
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  }
  for route in [
    issue_path("20000000-0000-4000-8000-000000000099"),
    issue_path(&issue).replace(PROJECT, "20000000-0000-4000-8000-000000000099"),
  ] {
    let response = router
      .clone()
      .oneshot(json_request(
        "PATCH",
        &format!("{route}/materials"),
        "foreign-route",
        Some("\"1\""),
        &selection(&[]),
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
      router
        .clone()
        .oneshot(empty_request("GET", &route, None))
        .await
        .unwrap()
        .status(),
      StatusCode::NOT_FOUND
    );
  }
  let response = edit(&router, &issue, "reusable-after-rejection", "\"1\"", &[comment]).await;
  assert_eq!(response.status(), StatusCode::OK);
  let response = edit(&router, &other, "reusable-after-rejection", "\"1\"", &[]).await;
  assert_eq!(
    response.status(),
    StatusCode::CONFLICT,
    "another Issue cannot obtain the original receipt"
  );
  let response = edit(&router, &issue, "reusable-after-rejection", "\"1\"", &[]).await;
  assert_eq!(
    response.status(),
    StatusCode::CONFLICT,
    "changed intent cannot use an old key"
  );
}

#[tokio::test]
async fn issue_materials_http_requires_review_and_current_authority_without_restoring_bodies_on_replay() {
  use super::collaboration::{Rights, verified};
  use ManagementResourceKind as Kind;
  use octacity_server_application::testing::MutationEvidenceProbe;
  let f = Fixture::new().await;
  let denied = Arc::new(Mutex::new(BTreeSet::new()));
  let router = f.router(Arc::new(Rights(denied.clone())));
  let issue = create(&router, "authorization").await;
  f.set_time(SystemFactoryExecutionClock.now().unwrap().unix_millis() + 1000);
  let path = issue_path(&issue);
  let comment = comment(&router, &issue, "comment").await;
  let doc = publish(
    &router,
    &format!("{path}/input-documents"),
    "document",
    "Private document",
  )
  .await;
  let refs = vec![comment.clone(), owned(&issue, &doc)];
  for (version, expected) in [
    (None, StatusCode::PRECONDITION_REQUIRED),
    (Some("W/\"1\""), StatusCode::BAD_REQUEST),
    (Some("*"), StatusCode::BAD_REQUEST),
    (Some("\"2\""), StatusCode::CONFLICT),
  ] {
    let response = router
      .clone()
      .oneshot(json_request(
        "PATCH",
        &format!("{path}/materials"),
        "select",
        version,
        &selection(&refs),
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), expected);
  }
  let mut missing_key = json_request(
    "PATCH",
    &format!("{path}/materials"),
    "select",
    Some("\"1\""),
    &selection(&refs),
  );
  missing_key.headers_mut().remove("idempotency-key");
  assert_eq!(
    router.clone().oneshot(missing_key).await.unwrap().status(),
    StatusCode::BAD_REQUEST
  );
  let evidence = f.issues.mutation_evidence_counts().await;
  for denial in [
    (Kind::Issue, ManagementAction::Update),
    (Kind::Issue, ManagementAction::View),
    (Kind::Project, ManagementAction::View),
    (Kind::IssueDiscussion, ManagementAction::View),
    (Kind::Artifact, ManagementAction::Download),
  ] {
    denied.lock().unwrap().insert(denial);
    let response = edit(&router, &issue, "select", "\"1\"", &refs).await;
    let expected = if denial == (Kind::Issue, ManagementAction::Update) {
      StatusCode::FORBIDDEN
    } else {
      StatusCode::BAD_REQUEST
    };
    assert_eq!(response.status(), expected, "{denial:?}");
    assert!(!document(response).await.to_string().contains("Private"));
    assert_eq!(f.issues.mutation_evidence_counts().await, evidence);
    denied.lock().unwrap().clear();
  }
  let request = || {
    verified(
      json_request(
        "PATCH",
        &format!("{path}/materials"),
        "select",
        Some("\"1\""),
        &selection(&refs),
      ),
      "selector",
      "team-a",
    )
  };
  let response = router.clone().oneshot(request()).await.unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let original = document(response).await;
  let response = router
    .clone()
    .oneshot(verified(
      json_request(
        "PATCH",
        &format!("{path}/materials"),
        "select",
        Some("\"1\""),
        &selection(&refs),
      ),
      "other",
      "team-b",
    ))
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    StatusCode::CONFLICT,
    "a different security scope cannot replay an old receipt"
  );
  denied.lock().unwrap().extend([
    (Kind::IssueDiscussion, ManagementAction::View),
    (Kind::Artifact, ManagementAction::Download),
  ]);
  assert_eq!(
    get(&router, &path).await,
    original["resource"],
    "Issue View grants safe retained references, not body rights"
  );
  assert_eq!(get(&router, &format!("{path}/revisions/2")).await, original["resource"]);
  for body_path in [
    format!("{path}/comments/{}", comment["comment_id"].as_str().unwrap()),
    format!(
      "{path}/input-documents/{}",
      doc["artifact"]["artifact_id"].as_str().unwrap()
    ),
  ] {
    assert_eq!(
      router
        .clone()
        .oneshot(empty_request("GET", &body_path, None))
        .await
        .unwrap()
        .status(),
      StatusCode::FORBIDDEN
    );
  }
  let response = router.clone().oneshot(request()).await.unwrap();
  assert_eq!(
    response.status(),
    StatusCode::OK,
    "receipt replay reads no source bodies"
  );
  assert_eq!(document(response).await["resource"], original["resource"]);
  for denial in [
    (Kind::Issue, ManagementAction::Update),
    (Kind::Issue, ManagementAction::View),
    (Kind::Project, ManagementAction::View),
  ] {
    denied.lock().unwrap().insert(denial);
    let response = router.clone().oneshot(request()).await.unwrap();
    assert_eq!(
      response.status(),
      if denial.0 == Kind::Issue && denial.1 == ManagementAction::Update {
        StatusCode::FORBIDDEN
      } else {
        StatusCode::BAD_REQUEST
      }
    );
    denied.lock().unwrap().remove(&denial);
  }
  let response = edit(&router, &issue, "remove", "\"2\"", &[]).await;
  assert_eq!(
    response.status(),
    StatusCode::OK,
    "removal does not need access to source bodies"
  );
  denied.lock().unwrap().clear();
  f.set_time(doc["expires_at"].as_i64().unwrap());
  let response = edit(&router, &issue, "expired", "\"3\"", &[owned(&issue, &doc)]).await;
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  let response = router.clone().oneshot(request()).await.unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(document(response).await["resource"], original["resource"]);
  assert_eq!(
    router
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!(
          "{path}/input-documents/{}",
          doc["artifact"]["artifact_id"].as_str().unwrap()
        ),
        None
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::NOT_FOUND
  );
  assert!(
    get(&router, &path).await.get("materials").is_none(),
    "replay never reinstates selection or expired bytes"
  );
  denied.lock().unwrap().insert((Kind::Issue, ManagementAction::View));
  for route in [&path, &format!("{path}/revisions/2")] {
    assert_eq!(
      router
        .clone()
        .oneshot(empty_request("GET", route, None))
        .await
        .unwrap()
        .status(),
      StatusCode::FORBIDDEN
    );
  }
}

#[tokio::test]
async fn issue_materials_http_accepts_the_shared_32_reference_boundary() {
  let f = Fixture::new().await;
  let router = f.router(Arc::new(TrustedNetworkManagementPolicy));
  let issue = create(&router, "count-boundary").await;
  f.set_time(SystemFactoryExecutionClock.now().unwrap().unix_millis() + 1000);
  let mut references = Vec::new();
  for index in 0..32 {
    references.push(comment(&router, &issue, &format!("comment-{index}")).await);
  }
  let response = edit(&router, &issue, "select-32", "\"1\"", &references).await;
  assert_eq!(response.status(), StatusCode::OK);
  let resource = document(response).await;
  assert_eq!(resource["resource"]["materials"].as_array().unwrap().len(), 32);
  assert_json_matches_component(&openapi_document(), "IssueMutationResponse", &resource);
}

#[test]
fn issue_materials_http_contract_closes_all_source_variants_and_retains_legacy_empty_encoding() {
  let doc = openapi_document();
  let schemas = &doc["components"]["schemas"];
  let request = &schemas["EditIssueMaterialsRequest"];
  assert_eq!(request["additionalProperties"], false);
  assert_eq!(request["required"], serde_json::json!(["references"]));
  assert_eq!(request["properties"].as_object().unwrap().len(), 1);
  assert_eq!(request["properties"]["references"]["maxItems"], 32);
  assert_eq!(
    request["properties"]["references"]["items"]["$ref"],
    "#/components/schemas/IssueMaterialReference"
  );
  let variants = schemas["IssueMaterialReference"]["oneOf"].as_array().unwrap();
  assert_eq!(variants.len(), 3);
  for (variant, kind) in variants.iter().zip(["comment", "issue_document", "project_document"]) {
    assert_eq!(variant["additionalProperties"], false);
    assert_eq!(variant["properties"]["kind"]["const"], kind);
    assert!(variant["properties"].get("body").is_none());
    assert_eq!(
      variant["properties"].get("issue_id").is_some(),
      kind != "project_document"
    );
  }
  assert_eq!(schemas["IssueMaterialSelection"]["additionalProperties"], false);
  assert_eq!(schemas["IssueResource"]["properties"]["materials"]["maxItems"], 32);
  assert!(
    !schemas["IssueResource"]["required"]
      .as_array()
      .unwrap()
      .contains(&serde_json::json!("materials"))
  );
  assert!(
    schemas["IssueResource"]["properties"]["materials"]
      .get("default")
      .is_none()
  );
  let path = "/api/v1/projects/{project_id}/issues/{issue_id}/materials";
  let op = &doc["paths"][path]["patch"];
  for name in ["Idempotency-Key", "If-Match"] {
    assert_required_header(op, name, true);
  }
  assert_eq!(
    op["responses"]["200"]["headers"]["Cache-Control"]["schema"]["const"],
    "no-store"
  );
  let mappings = management_authorization_operations();
  let mapping = mappings
    .iter()
    .find(|m| m.operation_id == "editIssueMaterials")
    .unwrap();
  assert_eq!(mapping.authorization.action(), ManagementAction::Update);
  assert_eq!(mapping.authorization.resource_kind(), ManagementResourceKind::Issue);
}
