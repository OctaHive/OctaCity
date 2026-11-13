//! Classification is Issue data: HTTP reads need no producer or execution service.
use super::*;
use octacity_server_application::testing::MutationEvidenceProbe;
use octacity_server_application::{IssueStore, ManagementResourceKind, ManagementVisibility};

struct ClassificationRights {
  visible: Mutex<bool>,
  edit: Mutex<bool>,
  producer_access: AtomicUsize,
}
#[async_trait]
impl ManagementAuthorizationPolicy for ClassificationRights {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if !matches!(
      resource.kind(),
      ManagementResourceKind::Issue | ManagementResourceKind::Project
    ) {
      self.producer_access.fetch_add(1, Ordering::SeqCst);
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    if action == ManagementAction::Administer || (action == ManagementAction::Update && !*self.edit.lock().unwrap()) {
      return Err(ManagementAuthorizationDenial::forbidden());
    }
    Ok(ManagementAuthorizationGrant::new(if *self.visible.lock().unwrap() {
      ManagementVisibility::all()
    } else {
      ManagementVisibility::none()
    }))
  }
}

#[tokio::test]
async fn issue_http_classification_reads_need_no_private_producer_and_recheck_issue_authority() {
  let rights = Arc::new(ClassificationRights {
    visible: Mutex::new(true),
    edit: Mutex::new(true),
    producer_access: AtomicUsize::new(0),
  });
  let (router, store) = issue_router_with_store(rights.clone()).await;
  let mut request = draft();
  request["priority"] = serde_json::json!("high");
  let first = document(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &root(),
        "private-producer",
        None,
        &request.to_string(),
      ))
      .await
      .unwrap(),
  )
  .await["resource"]
    .clone();
  let issue_id = first["id"].as_str().unwrap().parse().unwrap();
  // Restore trusted retained provenance through the existing fixture port. A client
  // never supplies this document, and no Factory/transcript reader is installed.
  let mut retained = store.issue(PROJECT.parse().unwrap(), issue_id, None).await.unwrap();
  retained.version = retained.version.next().unwrap();
  retained.classification.fields[0].origin = serde_json::from_value(serde_json::json!({"kind":"pipeline",
    "actor":{"kind":"unauthenticated_management","identity":null},"work_id":"60000000-0000-4000-8000-000000000002",
    "run_id":"60000000-0000-4000-8000-000000000003","record":"ab".repeat(32),"policy":"triage","operation_id":"cd".repeat(32)})).unwrap();
  let origin = serde_json::to_value(&retained.classification.fields[0].origin).unwrap();
  let subject = serde_json::json!({"kind":"exact","value":{"project_id":PROJECT,"repository_id":"60000000-0000-4000-8000-000000000004","base_revision":"original-base"}});
  retained.classification.fields[0].inputs = serde_json::from_value(serde_json::json!({"subject":subject})).unwrap();
  retained.classification.fields[0].evidence = vec!["ef".repeat(32).parse().unwrap()];
  retained.classification.evidence = serde_json::from_value(serde_json::json!([{
    "work_id":"60000000-0000-4000-8000-000000000002","run_id":"60000000-0000-4000-8000-000000000003",
    "record":"ef".repeat(32),"subject":subject,"environment":{"identity":"test.environment","version":"v1","digest":"aa".repeat(32)},
    "criterion":{"identity":"test.criterion","version":"v1","digest":"bb".repeat(32)},"observed_at":1,"fresh_until":2
  }])).unwrap();
  store.seed_retained_issue_revision(retained).unwrap();
  let path = format!("{}/{issue_id}", root());
  let before = store.mutation_evidence_counts().await;
  let resource = get_json(&router, &path).await;
  assert_eq!(resource["classification"]["fields"][0]["origin"], origin);
  let review = get_json(&router, &format!("{path}/classification")).await;
  assert_eq!(
    review["fields"][0]["problems"],
    serde_json::json!([
    {"code":"code_subject_unavailable"},{"code":"evidence_subject_unavailable","record":"ef".repeat(32)},
    {"code":"evidence_expired","record":"ef".repeat(32)},{"code":"evidence_unavailable","record":"ef".repeat(32)}])
  );
  for (schema, value) in [("IssueResource", &resource), ("IssueClassificationReview", &review)] {
    assert_json_matches_component(&openapi_document(), schema, value);
    for private in ["payload", "transcript", "context", "credentials", "report_body"] {
      assert!(!value.to_string().contains(private));
    }
  }
  assert_eq!(rights.producer_access.load(Ordering::SeqCst), 0);
  *rights.edit.lock().unwrap() = false;
  request["priority"] = serde_json::json!("low");
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "PATCH",
        &path,
        "denied",
        Some("\"2\""),
        &request.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::FORBIDDEN
  );
  *rights.visible.lock().unwrap() = false;
  for suffix in ["", "/classification", "/revisions/1", "/revisions/2/classification"] {
    assert_eq!(
      router
        .clone()
        .oneshot(empty_request("GET", &format!("{path}{suffix}"), None))
        .await
        .unwrap()
        .status(),
      StatusCode::NOT_FOUND
    );
  }
  assert_eq!(store.mutation_evidence_counts().await, before);
}

#[tokio::test]
async fn issue_http_classification_rejects_forged_origins_and_source_owned_edits_atomically() {
  let (router, store) = issue_router_with_store(Arc::new(TrustedNetworkManagementPolicy)).await;
  let request = draft();
  let created = document(
    router
      .clone()
      .oneshot(json_request("POST", &root(), "safe", None, &request.to_string()))
      .await
      .unwrap(),
  )
  .await["resource"]
    .clone();
  let path = format!("{}/{}", root(), created["id"].as_str().unwrap());
  let plan = serde_json::json!({"definition_version":0,"assignee":null,"labels":[],"values":{},"due_date":null,"parent_id":null,"size":"small"});
  let before = store.mutation_evidence_counts().await;
  for field in [
    "classification",
    "origin",
    "actor",
    "evidence",
    "accepted",
    "dependencies",
  ] {
    for (method, route, mut body, version) in [
      ("POST", root(), request.clone(), None),
      ("PATCH", path.clone(), request.clone(), Some("\"1\"")),
      ("PATCH", format!("{path}/planning"), plan.clone(), Some("\"1\"")),
    ] {
      body[field] = serde_json::json!({"kind":"pipeline","verified":true});
      assert_eq!(
        router
          .clone()
          .oneshot(json_request(method, &route, field, version, &body.to_string()))
          .await
          .unwrap()
          .status(),
        StatusCode::BAD_REQUEST,
        "{method} {route} {field}"
      );
    }
  }
  store
    .seed_issue_field_policy(
      PROJECT.parse().unwrap(),
      created["id"].as_str().unwrap().parse().unwrap(),
      octacity_server_application::IssueFieldPolicy {
        source_owned: [octacity_server_application::IssueField::Size].into(),
        ..Default::default()
      },
    )
    .unwrap();
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "PATCH",
        &format!("{path}/planning"),
        "owned-size",
        Some("\"1\""),
        &plan.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::CONFLICT
  );
  assert_eq!(get_json(&router, &path).await, created);
  assert_eq!(store.mutation_evidence_counts().await, before);
}

async fn get_json(router: &axum::Router, path: &str) -> serde_json::Value {
  let response = router.clone().oneshot(empty_request("GET", path, None)).await.unwrap();
  let status = response.status();
  assert_eq!(status, StatusCode::OK, "{path}");
  assert_eq!(response.headers()["cache-control"], "no-store");
  document(response).await
}

#[tokio::test]
async fn issue_http_classification_contracts_are_closed_bounded_and_reuse_project_definitions() {
  let router = issue_router(Arc::new(TrustedNetworkManagementPolicy)).await;
  let definition = serde_json::json!({"labels":[],"components":[{"id":"compiler","name":"Compiler","retired":false}],
    "properties":[{"id":"reproduction","name":"Reproduction","retired":false,"kind":{"type":"choice","choices":[
      {"id":"unknown","name":"Unknown","retired":false},{"id":"reproduced","name":"Reproduced","retired":false}]}}],
    "classification":{"reproducibility":"reproduction"}});
  let settings = format!("/api/v1/projects/{PROJECT}/issue-property-definitions");
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      &settings,
      "catalog",
      None,
      &serde_json::json!({"expected_version":0,"definition":definition}).to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let original_catalog = document(response).await["resource"].clone();
  let mut request = draft();
  request["priority"] = serde_json::json!("high");
  request["planning"] = serde_json::json!({"definition_version":1,"component":"compiler","type":"defect","size":"medium",
    "assignee":null,"labels":[],"values":{"reproduction":{"type":"choice","value":"unknown"}},"due_date":null,"parent_id":null});
  let response = router
    .clone()
    .oneshot(json_request("POST", &root(), "typed", None, &request.to_string()))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let receipt = document(response).await;
  let path = format!("{}/{}", root(), receipt["resource"]["id"].as_str().unwrap());
  let review = get_json(&router, &format!("{path}/classification")).await;
  let contract = openapi_document();
  for (schema, value) in [
    ("IssueMutationResponse", &receipt),
    ("IssueClassificationReview", &review),
  ] {
    assert_json_matches_component(&contract, schema, value);
  }
  for name in [
    "IssueClassificationMetadata",
    "IssueClassificationAttribution",
    "IssueClassificationReview",
    "IssueClassificationFieldReview",
    "IssueClassificationEvidenceReview",
    "IssueClassificationAssessmentInputs",
    "IssueClassificationEvidenceReference",
    "IssueClassificationDependencyReference",
  ] {
    assert_eq!(
      contract["components"]["schemas"][name]["additionalProperties"], false,
      "{name}"
    );
  }
  assert_eq!(
    contract["components"]["schemas"]["IssueClassificationMetadata"]["properties"]["fields"]["maxItems"],
    37
  );
  assert_eq!(
    contract["components"]["schemas"]["IssueClassificationMetadata"]["properties"]["evidence"]["maxItems"],
    64
  );
  assert_eq!(
    contract["components"]["schemas"]["IssueClassificationReview"]["properties"]["fields"]["maxItems"],
    37
  );
  for kind in ["priority", "component", "type"] {
    let fields = receipt["resource"]["classification"]["fields"].as_array().unwrap();
    assert_eq!(
      fields.iter().find(|f| f["field"]["kind"] == kind).unwrap()["field"]["definition_version"],
      1
    );
  }
  let mut renamed = definition;
  renamed["properties"][0]["name"] = serde_json::json!("Current label");
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "POST",
        &settings,
        "rename",
        None,
        &serde_json::json!({"expected_version":1,"definition":renamed}).to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  assert_eq!(get_json(&router, &format!("{settings}/1")).await, original_catalog);
  assert_eq!(
    get_json(&router, &format!("{path}/revisions/1")).await,
    receipt["resource"]
  );
  assert_eq!(get_json(&router, &format!("{path}/classification")).await, review);
  for operation in ["reviewIssueClassification", "reviewIssueRevisionClassification"] {
    let entry = management_authorization_operations()
      .into_iter()
      .find(|entry| entry.operation_id == operation)
      .unwrap();
    assert_eq!(entry.authorization.action(), ManagementAction::View);
  }
}

#[tokio::test]
async fn issue_http_classification_review_is_read_only_and_reports_exact_revision_applicability() {
  let (router, store) = issue_router_with_store(Arc::new(TrustedNetworkManagementPolicy)).await;
  let mut request = draft();
  request["priority"] = serde_json::json!("high");
  let created = document(
    router
      .clone()
      .oneshot(json_request("POST", &root(), "review", None, &request.to_string()))
      .await
      .unwrap(),
  )
  .await;
  let path = format!("{}/{}", root(), created["resource"]["id"].as_str().unwrap());
  let before = store.mutation_evidence_counts().await;
  let original = get_json(&router, &format!("{path}/classification")).await;
  assert_eq!(original["version"], 1);
  let fields = original["fields"].as_array().unwrap();
  assert_eq!(
    fields.iter().find(|f| f["field"]["kind"] == "priority").unwrap()["problems"],
    serde_json::json!([])
  );
  assert_eq!(
    fields.iter().find(|f| f["field"]["kind"] == "size").unwrap()["problems"],
    serde_json::json!([{"code":"missing_value"}])
  );
  assert_eq!(store.mutation_evidence_counts().await, before);
  request["acceptance"] = serde_json::json!("Changed expected behavior");
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "PATCH",
        &path,
        "changed",
        Some("\"1\""),
        &request.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  let before = store.mutation_evidence_counts().await;
  let current = get_json(&router, &format!("{path}/classification")).await;
  assert_eq!(current["version"], 2);
  assert_eq!(
    current["fields"]
      .as_array()
      .unwrap()
      .iter()
      .find(|f| f["field"]["kind"] == "priority")
      .unwrap()["problems"],
    serde_json::json!([{"code":"requirements_changed"}])
  );
  assert_eq!(
    get_json(&router, &format!("{path}/revisions/1/classification")).await,
    original
  );
  for query in ["checked_at=0", "subject=forged", "accepted=true"] {
    assert_eq!(
      router
        .clone()
        .oneshot(empty_request("GET", &format!("{path}/classification?{query}"), None))
        .await
        .unwrap()
        .status(),
      StatusCode::BAD_REQUEST
    );
  }
  assert_eq!(
    get_json(&router, &path).await["classification"],
    created["resource"]["classification"]
  );
  assert_eq!(store.mutation_evidence_counts().await, before);
}

#[tokio::test]
async fn issue_http_classification_retains_field_origins_history_and_original_edit_receipts() {
  let (router, store) = issue_router_with_store(Arc::new(TrustedNetworkManagementPolicy)).await;
  let mut request = draft();
  request["priority"] = serde_json::json!("high");
  request["planning"] = serde_json::json!({"definition_version":0,"assignee":null,"labels":[],
    "due_date":null,"parent_id":null,"values":{},"type":"defect","size":"small"});
  let response = router
    .clone()
    .oneshot(json_request("POST", &root(), "classified", None, &request.to_string()))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let first = document(response).await["resource"].clone();
  let fields = first["classification"]["fields"]
    .as_array()
    .expect("Issue reads expose committed field attribution");
  assert_eq!(fields.len(), 3);
  for kind in ["priority", "type", "size"] {
    let field = fields.iter().find(|field| field["field"]["kind"] == kind).unwrap();
    assert_eq!(
      field["origin"],
      serde_json::json!({"kind":"manual","actor":{"kind":"unauthenticated_management","identity":null}})
    );
    assert_eq!(field["assessed_version"], 1);
    assert_eq!(field["requirements_version"], 1);
    assert_eq!(field["assessed_at"], first["updated_at_unix_ms"]);
  }
  let path = format!("{}/{}", root(), first["id"].as_str().unwrap());
  let mut plan = request["planning"].clone();
  plan["size"] = serde_json::json!("large");
  let response = router
    .clone()
    .oneshot(json_request(
      "PATCH",
      &format!("{path}/planning"),
      "large",
      Some("\"1\""),
      &plan.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let second = document(response).await["resource"].clone();
  assert_eq!(second["planning"]["size"], "large");
  let mut content = request.clone();
  content.as_object_mut().unwrap().remove("planning");
  content["acceptance"] = serde_json::json!("Different requirements");
  assert_eq!(
    router
      .clone()
      .oneshot(json_request(
        "PATCH",
        &path,
        "acceptance",
        Some("\"2\""),
        &content.to_string()
      ))
      .await
      .unwrap()
      .status(),
    StatusCode::OK
  );
  let before = store.mutation_evidence_counts().await;
  for (suffix, expected) in [("/revisions/1", &first), ("/revisions/2", &second)] {
    let response = router
      .clone()
      .oneshot(empty_request("GET", &format!("{path}{suffix}"), None))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(document(response).await, *expected);
  }
  let replay = document(
    router
      .clone()
      .oneshot(json_request(
        "PATCH",
        &format!("{path}/planning"),
        "large",
        Some("\"1\""),
        &plan.to_string(),
      ))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(replay["resource"], second);
  assert_eq!(replay["disposition"], "replayed");
  assert_eq!(store.mutation_evidence_counts().await, before);
}
