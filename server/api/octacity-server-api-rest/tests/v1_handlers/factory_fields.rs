//! Selected Issue fields use the ordinary schema, publication and diagnostic HTTP owners.
use super::*;
use octacity_server_application::*;

async fn router(policy: Arc<dyn ManagementAuthorizationPolicy>) -> (axum::Router, ProjectId) {
  let project = ProjectId::generate();
  let peer = Arc::new(RecordingApplication::successful_workflow());
  (
    management_router_with_application(
      || true,
      support::recording_management_application_with_policy(peer.clone(), peer, policy),
    ),
    project,
  )
}

#[tokio::test]
async fn selected_issue_fields_generate_the_schema_without_a_copied_shape() {
  let (router, project) = router(Arc::new(TrustedNetworkManagementPolicy)).await;
  let request = serde_json::json!({"project_id":project,"identity":"analysis.fields","version":"v1",
  "issue_fields":{"project_id":project,"definition_version":0,"fields":[
    {"field":{"kind":"priority"}}, {"field":{"kind":"size"},"required":false}
  ]}});
  let response = router
    .oneshot(json_request(
      "POST",
      "/api/v1/flow-data-schemas/validate",
      "no-publication",
      None,
      &request.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    StatusCode::OK,
    "the server must derive the result from the selected fields"
  );
  assert_eq!(response.headers()["cache-control"], "no-store");
  let schema: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(
    schema["shape"],
    serde_json::json!({"type":"object","fields":{
      "priority":{"required":true,"value":{"type":"enum","values":["blocker","high","medium","low"]}},
      "size":{"required":false,"value":{"type":"enum","values":["small","medium","large"]}}
    }})
  );
  assert_json_matches_component(&openapi_document(), "FactoryFlowDataSchema", &schema);
  assert_json_matches_component(&openapi_document(), "ValidateFactoryFlowDataSchemaRequest", &request);
}

#[tokio::test]
async fn selected_issue_field_schema_rejects_competing_sources_and_spoofed_scope() {
  let (router, project) = router(Arc::new(TrustedNetworkManagementPolicy)).await;
  let base = serde_json::json!({"project_id":project,"identity":"analysis.fields","version":"v1",
    "issue_fields":{"project_id":project,"definition_version":0,"fields":[{"field":{"kind":"priority"}}]}});
  for (name, pointer, value) in [
    (
      "foreign_project",
      "/issue_fields/project_id",
      serde_json::json!(ProjectId::generate()),
    ),
    (
      "unknown_target",
      "/issue_fields/fields/0/field/kind",
      serde_json::json!("status"),
    ),
    ("old_catalog", "/issue_fields/definition_version", serde_json::json!(9)),
    ("empty_selection", "/issue_fields/fields", serde_json::json!([])),
    (
      "forged_provenance",
      "/issue_fields/fields/0",
      serde_json::json!({"field":{"kind":"priority"},"actor":"operator"}),
    ),
  ] {
    let mut invalid = base.clone();
    *invalid.pointer_mut(pointer).unwrap() = value;
    let response = router
      .clone()
      .oneshot(json_request(
        "POST",
        "/api/v1/flow-data-schemas/validate",
        "unused",
        None,
        &invalid.to_string(),
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{name}");
  }
  for shape in [serde_json::json!({"type":"boolean"}), serde_json::Value::Null] {
    let mut invalid = base.clone();
    invalid["shape"] = shape;
    let response = router
      .clone()
      .oneshot(json_request(
        "POST",
        "/api/v1/flow-data-schemas/validate",
        "unused",
        None,
        &invalid.to_string(),
      ))
      .await
      .unwrap();
    assert_eq!(
      response.status(),
      StatusCode::BAD_REQUEST,
      "a second source, including null, is outside the closed form request"
    );
  }
}

#[tokio::test]
async fn selected_issue_field_discovery_advertises_all_scalar_types_and_exact_write_keys() {
  let project = ProjectId::generate();
  let properties = testing::manual_issue_store(project).await;
  let policy = Arc::new(TrustedNetworkManagementPolicy);
  AuthorizedCommandHandler::new(policy.clone(), Arc::new(IssueHandlers::new(properties.clone(), policy.clone())))
    .handle_command(&ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap()), PublishIssuePropertiesCommand {
      intent: PublishIssuePropertiesIntent { project_id:project, expected_version:0, definition:serde_json::from_value(serde_json::json!({
        "components":[{"id":"billing","name":"Billing","retired":false}],"labels":[],"properties":[
          {"id":"note","name":"Note","retired":false,"kind":{"type":"text","max_bytes":31}},
          {"id":"route","name":"Route","retired":false,"kind":{"type":"choice","choices":[{"id":"ready","name":"Ready for work","retired":false}]}},
          {"id":"flag","name":"Flag","retired":false,"kind":{"type":"boolean"}},
          {"id":"effort","name":"Effort","retired":false,"kind":{"type":"number","minimum":-2,"maximum":8}},
          {"id":"due","name":"Due","retired":false,"kind":{"type":"date"}},
          {"id":"retired","name":"Retired","retired":true,"kind":{"type":"boolean"}}
        ]})).unwrap() }, idempotency_key:"definitions".parse().unwrap(), published_at:Timestamp::from_unix_millis(2).unwrap(),
    }).await.unwrap();
  let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
  store.seed_project(project).unwrap();
  let handlers =
    Arc::new(FactoryRoleProfileHandlers::with_policy(store, policy.clone()).with_issue_properties(properties));
  let peer = Arc::new(RecordingApplication::default());
  let app = recording_management_application(peer.clone(), peer).with_factory_roles(
    octacity_server_api_rest::v1::FactoryRoleManagementApplication::new(
      Arc::new(AuthorizedCommandHandler::new(policy.clone(), handlers.clone())),
      Arc::new(AuthorizedQueryHandler::new(policy, handlers)),
    ),
  );
  let response = management_router_with_application(|| true, app)
    .oneshot(empty_request(
      "GET",
      &format!("/api/v1/projects/{project}/factory-authoring-capabilities"),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  let fields = body["issue_fields"]["fields"].as_array().unwrap();
  for (id, shape) in [
    (
      "note",
      serde_json::json!({"type":"string","min_bytes":0,"max_bytes":31}),
    ),
    ("route", serde_json::json!({"type":"enum","values":["ready"]})),
    ("flag", serde_json::json!({"type":"boolean"})),
    ("effort", serde_json::json!({"type":"integer","minimum":-2,"maximum":8})),
    (
      "due",
      serde_json::json!({"type":"string","min_bytes":10,"max_bytes":10}),
    ),
  ] {
    let field = fields
      .iter()
      .find(|f| f["source"] == serde_json::json!({"kind":"property","id":id}))
      .unwrap();
    assert_eq!(field["write_target"], serde_json::json!({"kind":"property","id":id}));
    assert_eq!(field["value_key"], format!("property.{id}"));
    assert_eq!(field["shape"], shape);
  }
  assert_eq!(
    fields.iter().find(|f| f["source"]["id"] == "due").unwrap()["value_type"],
    serde_json::json!({"type":"date"}),
    "a date must be distinguishable from ten-byte free text"
  );
  assert!(!fields.iter().any(|f| f["source"]["id"] == "retired"));
  assert!(
    fields
      .iter()
      .find(|f| f["source"]["kind"] == "title")
      .unwrap()
      .get("write_target")
      .is_none(),
    "requirements keep their separate owner"
  );
  assert_eq!(
    fields.iter().find(|f| f["source"]["id"] == "route").unwrap()["choice_labels"]["ready"],
    "Ready for work"
  );
  assert_json_matches_component(&openapi_document(), "FactoryAuthoringCapabilities", &body);
}

struct DenyFormAuthoring;
#[async_trait]
impl ManagementAuthorizationPolicy for DenyFormAuthoring {
  async fn authorize(
    &self,
    _: &ManagementRequestContext,
    _: ManagementAction,
    _: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    Err(ManagementAuthorizationDenial::forbidden())
  }
}
#[tokio::test]
async fn selected_issue_field_schema_requires_current_authoring_authority() {
  let (router, project) = router(Arc::new(DenyFormAuthoring)).await;
  let response=router.oneshot(json_request("POST","/api/v1/flow-data-schemas/validate","unused",None,&serde_json::json!({
    "project_id":project,"identity":"denied.fields","version":"1","issue_fields":{"project_id":project,"definition_version":0,"fields":[{"field":{"kind":"size"}}]}
  }).to_string())).await.unwrap();
  assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn accepted_issue_field_analysis_exposes_pending_save_separately_without_result_values() {
  let peer = Arc::new(RecordingApplication::successful_workflow());
  let router = management_router_with_application(|| true, recording_management_application(peer.clone(), peer));
  let response = router
    .oneshot(empty_request(
      "GET",
      "/api/v1/factory-runs/22222222-2222-4222-8222-222222222222/node-completions?limit=1",
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  let record = &body["items"][0]["record"];
  assert_eq!(record["issue_fields"]["status"], "pending");
  assert_eq!(record["issue_fields"]["operation_id"], "ad".repeat(32));
  assert!(record.get("public_result").is_none());
  for private in ["values", "actor", "owner", "claim", "input", "context", "credentials"] {
    assert!(record["issue_fields"].get(private).is_none(), "{private}");
  }
  assert_json_matches_component(&openapi_document(), "FactoryDiagnosticPage", &body);
}
