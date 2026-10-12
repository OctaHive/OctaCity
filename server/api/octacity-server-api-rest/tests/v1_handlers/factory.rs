use super::*;
use octacity_server_application::{FactoryConfigurationPageProjection, ListProjectFactoryConfigurationsQuery};

#[async_trait]
impl ManagementQueryUseCase<ListProjectFactoryConfigurationsQuery> for RecordingApplication {
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    query: ListProjectFactoryConfigurationsQuery,
  ) -> Result<FactoryConfigurationPageProjection, Self::Error> {
    self.record("list_project_factory_configurations");
    assert_eq!(
      query.page().project_id().to_string(),
      "11111111-1111-4111-8111-111111111111"
    );
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    Ok(FactoryConfigurationPageProjection {
      total: 0,
      items: vec![],
      next_cursor: None,
    })
  }
}

#[tokio::test]
async fn lists_an_empty_factory_configuration_page_through_the_typed_management_route() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application.clone()),
  );
  let response = routes
    .oneshot(empty_request(
      "GET",
      "/api/v1/projects/11111111-1111-4111-8111-111111111111/factory-configurations?limit=1",
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(body, serde_json::json!({"total":0,"items":[],"next_cursor":null}));
  assert_eq!(
    *application.calls.lock().unwrap(),
    ["list_project_factory_configurations"]
  );
  assert_json_matches_component(&openapi_document(), "FactoryConfigurationSummaryPage", &body);
}

#[tokio::test]
async fn rejects_invalid_factory_configuration_pages_before_dispatch() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application.clone()),
  );
  for parameters in [
    "limit=0",
    "limit=201",
    "limit=1&offset=1",
    "limit=1&after=invalid",
    "limit=1&after=2:17:22222222-2222-4222-8222-222222222222",
    "limit=1&after=1:017:22222222-2222-4222-8222-222222222222",
    "limit=1&after=1:17:22222222-2222-4222-8222-222222222222:extra",
  ] {
    let response = routes
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("/api/v1/projects/11111111-1111-4111-8111-111111111111/factory-configurations?{parameters}"),
        None,
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{parameters}");
  }
  assert!(application.calls.lock().unwrap().is_empty());
}

#[async_trait]
impl ManagementQueryUseCase<octacity_server_application::GetCurrentFactoryConfigurationQuery> for RecordingApplication {
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    _query: octacity_server_application::GetCurrentFactoryConfigurationQuery,
  ) -> Result<octacity_server_application::FactoryConfigurationProjection, Self::Error> {
    self.record("get_current_factory_configuration");
    Err(ApplicationError::unavailable())
  }
}

#[async_trait]
impl ManagementQueryUseCase<octacity_server_application::GetFactoryConfigurationQuery> for RecordingApplication {
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    _query: octacity_server_application::GetFactoryConfigurationQuery,
  ) -> Result<octacity_server_application::FactoryConfigurationProjection, Self::Error> {
    self.record("get_factory_configuration");
    Err(ApplicationError::unavailable())
  }
}

#[tokio::test]
async fn factory_configuration_detail_queries_are_authorized_and_decode_identity_and_version() {
  let application = Arc::new(RecordingApplication::default());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application.clone()),
  );
  for path in [
    "/api/v1/factory-configurations/22222222-2222-4222-8222-222222222222",
    "/api/v1/factory-configurations/22222222-2222-4222-8222-222222222222/versions/1",
  ] {
    let response = routes.clone().oneshot(empty_request("GET", path, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE, "{path}");
  }
  assert_eq!(
    *application.calls.lock().unwrap(),
    ["get_current_factory_configuration", "get_factory_configuration"]
  );
  let response = routes
    .oneshot(empty_request(
      "GET",
      "/api/v1/factory-configurations/invalid/versions/1",
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  assert_eq!(application.calls.lock().unwrap().len(), 2);
}

#[async_trait]
impl ManagementQueryUseCase<octacity_server_application::ListFactoryConfigurationChoicesQuery>
  for RecordingApplication
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    query: octacity_server_application::ListFactoryConfigurationChoicesQuery,
  ) -> Result<octacity_server_application::FactoryConfigurationChoicePageProjection, Self::Error> {
    self.record("list_factory_configuration_choices");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    assert_eq!(query.project_id().to_string(), "11111111-1111-4111-8111-111111111111");
    assert_eq!(query.kind().unwrap().as_str(), "criterion_pack");
    Ok(octacity_server_application::FactoryConfigurationChoicePageProjection {
      total: 1,
      items: vec![octacity_server_application::FactoryConfigurationChoiceProjection {
        identity: format!("{}:criterion_pack:quality", query.project_id())
          .parse()
          .unwrap(),
        reference: octacity_server_application::FactoryChoiceReferenceProjection::Immutable(
          serde_json::from_value(serde_json::json!({"identity":"quality","version":"v1","digest":"11".repeat(32)}))
            .unwrap(),
        ),
        metadata: Some(
          serde_json::from_value(serde_json::json!({
            "node_kinds":["reasoning"],"signal_purposes":[],"enforcement":["network_hosts"],"compatible_choices":[]
          }))
          .unwrap(),
        ),
      }],
      next_cursor: None,
    })
  }
}

#[tokio::test]
async fn discovers_factory_choices_and_rejects_an_unknown_category_before_dispatch() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application.clone()),
  );
  let response = routes
    .clone()
    .oneshot(empty_request(
      "GET",
      "/api/v1/projects/11111111-1111-4111-8111-111111111111/factory-configuration-choices?kind=criterion_pack&limit=1",
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(body["total"], 1);
  assert_eq!(body["items"][0]["alias"], "quality");
  assert_eq!(
    body["items"][0]["metadata"],
    serde_json::json!({
      "node_kinds":["reasoning"],"signal_purposes":[],"enforcement":["network_hosts"],"compatible_choices":[]
    })
  );
  assert_json_matches_component(&openapi_document(), "FactoryConfigurationChoicePage", &body);
  let response = routes
    .oneshot(empty_request(
      "GET",
      "/api/v1/projects/11111111-1111-4111-8111-111111111111/factory-configuration-choices?kind=unknown",
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  assert_eq!(
    *application.calls.lock().unwrap(),
    ["list_factory_configuration_choices"]
  );
}

pub(super) const CREATE_CONFIGURATION: &str =
  include_str!("../../fixtures/v1/create-factory-configuration-request.json");
const CONFIGURATION_DETAILS: &str = include_str!("../../fixtures/v1/factory-configuration-details.json");

#[async_trait]
impl ManagementQueryUseCase<octacity_server_application::ValidateFactoryConfigurationQuery> for RecordingApplication {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    query: octacity_server_application::ValidateFactoryConfigurationQuery,
  ) -> Result<octacity_server_application::FactoryConfigurationValidation, Self::Error> {
    self.record("validate_factory_configuration");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    assert_eq!(query.project_id.to_string(), "11111111-1111-4111-8111-111111111111");
    if query.draft.admission_policy.as_str() == "unavailable" {
      return Err(ApplicationError::FactoryValidation(Box::new(
        octacity_server_application::FactoryValidationDiagnostics {
          errors: vec![octacity_server_application::FactoryValidationIssue {
            field: "/admission_policy".to_owned(),
            code: "unavailable_reference",
            definition: None,
            node: None,
          }],
          unavailable_references: vec!["/admission_policy".to_owned()],
          ..Default::default()
        },
      )));
    }
    let details: serde_json::Value = serde_json::from_str(CONFIGURATION_DETAILS).unwrap();
    Ok(octacity_server_application::FactoryConfigurationValidation {
      configuration: serde_json::from_value(details["configuration"].clone()).unwrap(),
      diagnostics: Default::default(),
    })
  }
}

#[tokio::test]
async fn validates_a_factory_configuration_without_publishing_a_version() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application.clone()),
  );
  let response = routes
    .oneshot(json_request(
      "POST",
      "/api/v1/factory-configurations/validate",
      "unused",
      None,
      CREATE_CONFIGURATION,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(body["diagnostics"]["errors"], serde_json::json!([]));
  assert!(body["diagnostics"]["required_backend_capabilities"].is_array());
  assert_eq!(
    body["configuration"],
    serde_json::from_str::<serde_json::Value>(CONFIGURATION_DETAILS).unwrap()["configuration"]
  );
  assert_json_matches_component(&openapi_document(), "FactoryConfigurationValidation", &body);
  assert_eq!(*application.calls.lock().unwrap(), ["validate_factory_configuration"]);
}

#[async_trait]
impl ManagementCommandUseCase<octacity_server_application::CreateFactoryConfigurationCommand> for RecordingApplication {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    command: octacity_server_application::CreateFactoryConfigurationCommand,
  ) -> Result<octacity_server_application::FactoryConfigurationCommandOutcome, Self::Error> {
    self.record("create_factory_configuration");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    assert_eq!(command.id.to_string(), "22222222-2222-4222-8222-222222222222");
    let details: serde_json::Value = serde_json::from_str(CONFIGURATION_DETAILS).unwrap();
    Ok(octacity_server_application::FactoryConfigurationCommandOutcome {
      disposition: octacity_server_application::MutationDisposition::Applied,
      configuration: octacity_server_application::FactoryConfigurationProjection {
        configuration: serde_json::from_value(details["configuration"].clone()).unwrap(),
        published_at: Timestamp::from_unix_millis(1).unwrap(),
      },
    })
  }
}

#[tokio::test]
async fn creates_a_factory_configuration_through_an_idempotent_typed_command() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application.clone()),
  );
  let response = routes
    .oneshot(json_request(
      "POST",
      "/api/v1/factory-configurations",
      "factory-create",
      None,
      CREATE_CONFIGURATION,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(
    body,
    serde_json::json!({"disposition":"applied","resource":serde_json::from_str::<serde_json::Value>(CONFIGURATION_DETAILS).unwrap()})
  );
  assert_eq!(*application.calls.lock().unwrap(), ["create_factory_configuration"]);
  assert_json_matches_component(&openapi_document(), "FactoryConfigurationMutationResponse", &body);
}

#[test]
fn factory_configuration_documents_publish_a_closed_graph_contract_for_management_clients() {
  let document = openapi_document();
  let schemas = &document["components"]["schemas"];
  assert_eq!(schemas["FactoryConfigurationDefinition"]["additionalProperties"], false);
  assert_eq!(
    schemas["FactoryConfigurationDefinition"]["properties"]["flow"]["$ref"],
    "#/components/schemas/FactoryFlowConfiguration"
  );
  assert_eq!(schemas["FactoryFlowConfiguration"]["additionalProperties"], false);
  assert_eq!(
    schemas["FactoryFlowConfiguration"]["properties"]["closure"]["$ref"],
    "#/components/schemas/FactoryFlowDefinitionClosure"
  );
  assert_eq!(schemas["FactoryFlowNodeDefinition"]["additionalProperties"], false);
  assert_eq!(
    schemas["FactoryFlowNodeDefinition"]["properties"]["kind"]["$ref"],
    "#/components/schemas/FactoryFlowNodeKind"
  );
  let request: serde_json::Value = serde_json::from_str(CREATE_CONFIGURATION).unwrap();
  assert_json_matches_component(&document, "CreateFactoryConfigurationRequest", &request);
  let details: serde_json::Value = serde_json::from_str(CONFIGURATION_DETAILS).unwrap();
  assert_json_matches_component(&document, "FactoryConfigurationDetails", &details);
}

#[async_trait]
impl ManagementCommandUseCase<octacity_server_application::ReplaceFactoryConfigurationCommand>
  for RecordingApplication
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    command: octacity_server_application::ReplaceFactoryConfigurationCommand,
  ) -> Result<octacity_server_application::FactoryConfigurationCommandOutcome, Self::Error> {
    self.record("replace_factory_configuration");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    assert_eq!(command.expected_current_version.get(), 1);
    let mut details: serde_json::Value = serde_json::from_str(CONFIGURATION_DETAILS).unwrap();
    details["configuration"]["reference"]["version"] = serde_json::json!(2);
    details["configuration"]["reference"]["definition_digest"] =
      serde_json::json!(command.definition_digest.to_string());
    Ok(octacity_server_application::FactoryConfigurationCommandOutcome {
      disposition: octacity_server_application::MutationDisposition::Applied,
      configuration: octacity_server_application::FactoryConfigurationProjection {
        configuration: serde_json::from_value(details["configuration"].clone()).unwrap(),
        published_at: Timestamp::from_unix_millis(2).unwrap(),
      },
    })
  }
}

#[tokio::test]
async fn replaces_a_factory_configuration_only_with_a_strong_current_version_precondition() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application.clone()),
  );
  let request: serde_json::Value = serde_json::from_str(CREATE_CONFIGURATION).unwrap();
  let body = serde_json::json!({"definition":request["definition"]}).to_string();
  let path = "/api/v1/factory-configurations/22222222-2222-4222-8222-222222222222/versions";
  let missing = routes
    .clone()
    .oneshot(json_request("POST", path, "factory-replace", None, &body))
    .await
    .unwrap();
  assert_eq!(missing.status(), StatusCode::PRECONDITION_REQUIRED);
  assert!(application.calls.lock().unwrap().is_empty());
  let response = routes
    .oneshot(json_request("POST", path, "factory-replace", Some("\"1\""), &body))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(body["resource"]["configuration"]["reference"]["version"], 2);
  assert_eq!(*application.calls.lock().unwrap(), ["replace_factory_configuration"]);
  assert_json_matches_component(&openapi_document(), "FactoryConfigurationMutationResponse", &body);
}

#[tokio::test]
async fn flow_validation_rejects_an_undeclared_primitive() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application),
  );
  let configuration: serde_json::Value = serde_json::from_str(CREATE_CONFIGURATION).unwrap();
  let flow = &configuration["definition"]["flow"];
  let mut root = flow["closure"]["definitions"][0].clone();
  root.as_object_mut().unwrap().remove("reference");
  root["nodes"][0]["kind"] = serde_json::json!("repository_script");
  let request = serde_json::json!({"id":"33333333-3333-4333-8333-333333333333","project_id":"11111111-1111-4111-8111-111111111111","definition":{"root":root,"limits":flow["limits"],"data_schemas":flow["data_schemas"]}});
  let response = routes
    .oneshot(json_request(
      "POST",
      "/api/v1/flow-definitions/validate",
      "unused",
      None,
      &request.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[async_trait]
impl ManagementQueryUseCase<octacity_server_application::ValidateFactoryFlowDefinitionQuery> for RecordingApplication {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    _query: octacity_server_application::ValidateFactoryFlowDefinitionQuery,
  ) -> Result<octacity_server_application::FactoryFlowDefinitionValidation, Self::Error> {
    self.record("validate_factory_flow_definition");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    Err(ApplicationError::unavailable())
  }
}

pub(super) fn flow_draft_request() -> serde_json::Value {
  let configuration: serde_json::Value = serde_json::from_str(CREATE_CONFIGURATION).unwrap();
  let flow = &configuration["definition"]["flow"];
  let mut root = flow["closure"]["definitions"][0].clone();
  root.as_object_mut().unwrap().remove("reference");
  serde_json::json!({"id":"33333333-3333-4333-8333-333333333333","project_id":"11111111-1111-4111-8111-111111111111","definition":{"root":root,"limits":flow["limits"],"data_schemas":flow["data_schemas"]}})
}

#[tokio::test]
async fn publishes_and_reads_a_typed_flow_definition_with_idempotency_and_version_preconditions() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application),
  );
  let request = flow_draft_request();
  assert_json_matches_component(&openapi_document(), "CreateFactoryFlowDefinitionRequest", &request);
  let response = routes
    .clone()
    .oneshot(json_request(
      "POST",
      "/api/v1/flow-definitions",
      "flow-first",
      None,
      &request.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(body["disposition"], "applied");
  assert_eq!(body["resource"]["flow"]["closure"]["root"]["id"], request["id"]);
  assert_json_matches_component(&openapi_document(), "FactoryFlowDefinitionMutationResponse", &body);
  for path in [
    "/api/v1/flow-definitions/33333333-3333-4333-8333-333333333333",
    "/api/v1/flow-definitions/33333333-3333-4333-8333-333333333333/versions/1",
  ] {
    let response = routes.clone().oneshot(empty_request("GET", path, None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let detail: serde_json::Value =
      serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(detail, body["resource"]);
  }
  let response = routes
    .clone()
    .oneshot(empty_request(
      "GET",
      "/api/v1/projects/11111111-1111-4111-8111-111111111111/flow-definitions?limit=1",
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let page: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(page["total"], 1);
  assert_eq!(page["items"][0], body["resource"]);
  assert_json_matches_component(&openapi_document(), "FactoryFlowDefinitionPage", &page);
  let replacement = serde_json::json!({"definition":request["definition"]}).to_string();
  let path = "/api/v1/flow-definitions/33333333-3333-4333-8333-333333333333/versions";
  assert_eq!(
    routes
      .clone()
      .oneshot(json_request("POST", path, "flow-next", None, &replacement))
      .await
      .unwrap()
      .status(),
    StatusCode::PRECONDITION_REQUIRED
  );
  let response = routes
    .oneshot(json_request("POST", path, "flow-next", Some("\"1\""), &replacement))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let next: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(next["resource"]["flow"]["closure"]["root"]["version"], 2);
}

fn published_flow_fixture(version: u64) -> octacity_server_application::PublishedFactoryFlowDefinition {
  let mut fixture: serde_json::Value = serde_json::from_str(CONFIGURATION_DETAILS).unwrap();
  let root = &mut fixture["configuration"]["flow"]["closure"]["definitions"][0];
  root["reference"]["id"] = serde_json::json!("33333333-3333-4333-8333-333333333333");
  root["reference"]["version"] = serde_json::json!(version);
  fixture["configuration"]["flow"]["closure"]["root"] = root["reference"].clone();
  octacity_server_application::PublishedFactoryFlowDefinition {
    project_id: "11111111-1111-4111-8111-111111111111".parse().unwrap(),
    flow: serde_json::from_value(fixture["configuration"]["flow"].clone()).unwrap(),
    published_at: Timestamp::from_unix_millis(1).unwrap(),
  }
}
#[async_trait]
impl ManagementCommandUseCase<octacity_server_application::CreateFactoryFlowDefinitionCommand>
  for RecordingApplication
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    command: octacity_server_application::CreateFactoryFlowDefinitionCommand,
  ) -> Result<octacity_server_application::FactoryFlowDefinitionCommandOutcome, Self::Error> {
    self.record("create_factory_flow_definition");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    assert_eq!(command.id.to_string(), "33333333-3333-4333-8333-333333333333");
    Ok(octacity_server_application::FactoryFlowDefinitionCommandOutcome {
      disposition: octacity_server_store_disposition(),
      publication: published_flow_fixture(1),
    })
  }
}
fn octacity_server_store_disposition() -> octacity_server_application::StoreMutationDisposition {
  octacity_server_application::StoreMutationDisposition::Applied
}
#[async_trait]
impl ManagementCommandUseCase<octacity_server_application::ReplaceFactoryFlowDefinitionCommand>
  for RecordingApplication
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    command: octacity_server_application::ReplaceFactoryFlowDefinitionCommand,
  ) -> Result<octacity_server_application::FactoryFlowDefinitionCommandOutcome, Self::Error> {
    self.record("replace_factory_flow_definition");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    assert_eq!(command.expected_current_version.get(), 1);
    Ok(octacity_server_application::FactoryFlowDefinitionCommandOutcome {
      disposition: octacity_server_store_disposition(),
      publication: published_flow_fixture(2),
    })
  }
}
#[async_trait]
impl ManagementQueryUseCase<octacity_server_application::GetFactoryFlowDefinitionQuery> for RecordingApplication {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    _query: octacity_server_application::GetFactoryFlowDefinitionQuery,
  ) -> Result<octacity_server_application::PublishedFactoryFlowDefinition, Self::Error> {
    self.record("get_factory_flow_definition");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    Ok(published_flow_fixture(1))
  }
}
#[async_trait]
impl ManagementQueryUseCase<octacity_server_application::ListProjectFactoryFlowDefinitionsQuery>
  for RecordingApplication
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    query: octacity_server_application::ListProjectFactoryFlowDefinitionsQuery,
  ) -> Result<octacity_server_application::FactoryFlowDefinitionPage, Self::Error> {
    self.record("list_project_factory_flow_definitions");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    assert_eq!(query.limit, 1);
    Ok(octacity_server_application::FactoryFlowDefinitionPage {
      total: 1,
      items: vec![published_flow_fixture(1)],
      next_cursor: None,
    })
  }
}

pub(super) fn manual_work_request() -> serde_json::Value {
  serde_json::json!({"configuration_id":"22222222-2222-4222-8222-222222222222","configuration_version":1,"repository_id":"33333333-3333-4333-8333-333333333333","repository_version":1,"external_identity":"operator.issue-14","source":{"kind":"default_reference"},"artifacts":{"task":{"artifact_id":"44444444-4444-4444-8444-444444444444","content_digest":"ab".repeat(32),"encoded_size":1},"acceptance":{"artifact_id":"55555555-5555-4555-8555-555555555555","content_digest":"cd".repeat(32),"encoded_size":1},"specifications":[]},"classification":{"priority":1,"risk":"low","metadata":{}}})
}
#[tokio::test]
async fn admits_manual_work_through_a_bounded_idempotent_command() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application),
  );
  let path = "/api/v1/projects/11111111-1111-4111-8111-111111111111/factory-work";
  let request = manual_work_request();
  let response = routes
    .clone()
    .oneshot(json_request("POST", path, "manual-work", None, &request.to_string()))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(body["factory_run_id"], "66666666-6666-4666-8666-666666666666");
  assert_eq!(body["state"], "admitted");
  assert_json_matches_component(&openapi_document(), "FactoryAdmissionResponse", &body);
  let mut invalid = request;
  invalid["artifacts"]["acceptance"] = invalid["artifacts"]["task"].clone();
  assert_eq!(
    routes
      .oneshot(json_request("POST", path, "bad-work", None, &invalid.to_string()))
      .await
      .unwrap()
      .status(),
    StatusCode::BAD_REQUEST
  );
}
#[async_trait]
impl ManagementCommandUseCase<octacity_server_application::AdmitManualFactoryWorkCommand> for RecordingApplication {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    command: octacity_server_application::AdmitManualFactoryWorkCommand,
  ) -> Result<octacity_server_application::FactoryAdmissionOutcome, Self::Error> {
    self.record("admit_manual_factory_work");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    assert_eq!(command.external_identity.as_str(), "operator.issue-14");
    Ok(octacity_server_application::FactoryAdmissionOutcome {
      disposition: octacity_server_application::MutationDisposition::Applied,
      work_envelope_id: "77777777-7777-4777-8777-777777777777".parse().unwrap(),
      factory_run_id: "66666666-6666-4666-8666-666666666666".parse().unwrap(),
      base_revision: "0123456789abcdef0123456789abcdef01234567".parse().unwrap(),
      state: "admitted".parse().unwrap(),
      admitted_at: Timestamp::from_unix_millis(1).unwrap(),
    })
  }
}

#[tokio::test]
async fn factory_run_collections_support_project_and_global_scope_and_reject_unbounded_inputs() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application.clone()),
  );
  for prefix in [
    "/api/v1/factory-runs",
    "/api/v1/projects/11111111-1111-4111-8111-111111111111/factory-runs",
  ] {
    let response = routes
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("{prefix}?state=admitted&source=manual&admitted_from_unix_ms=0&admitted_before_unix_ms=100&limit=1"),
        None,
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
      serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body, serde_json::json!({"total":0,"items":[],"next_cursor":null}));
    assert_json_matches_component(&openapi_document(), "FactoryRunSummaryPage", &body);
    for parameters in [
      "state=triage",
      "limit=0",
      "limit=201",
      "source=manual%20secret",
      "offset=1",
      "after=1:01:66666666-6666-4666-8666-666666666666",
      "admitted_from_unix_ms=100&admitted_before_unix_ms=100",
    ] {
      assert_eq!(
        routes
          .clone()
          .oneshot(empty_request("GET", &format!("{prefix}?{parameters}"), None))
          .await
          .unwrap()
          .status(),
        StatusCode::BAD_REQUEST,
        "{parameters}"
      );
    }
  }
}

#[async_trait]
impl ManagementQueryUseCase<octacity_server_application::ListFactoryRunsQuery> for RecordingApplication {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    query: octacity_server_application::ListFactoryRunsQuery,
  ) -> Result<octacity_server_application::FactoryRunPageProjection, Self::Error> {
    self.record("list_factory_runs");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    assert_eq!(query.page().filter().state.unwrap().as_str(), "admitted");
    assert_eq!(query.page().limit().get(), 1);
    if query
      .page()
      .filter()
      .source
      .as_ref()
      .is_some_and(|source| source.as_str() == "filter-contract")
    {
      assert_eq!(query.page().filter().decision.unwrap().as_str(), "accept");
      assert_eq!(query.page().filter().delivery_state.unwrap().as_str(), "failed");
    }

    if query
      .page()
      .filter()
      .source
      .as_ref()
      .is_some_and(|source| source.as_str() == "summary-contract")
    {
      let exact = serde_json::json!({"project_id":"11111111-1111-4111-8111-111111111111", "repository_id":"33333333-3333-4333-8333-333333333333", "base_revision":"exact-base"});
      let candidate =
        serde_json::json!({"exact":exact,"candidate_revision":"exact-candidate","changeset_digest":"ab".repeat(32)});
      return Ok(octacity_server_application::FactoryRunPageProjection {
        total:1, next_cursor:None, items:vec![octacity_server_application::FactoryRunSummaryProjection {
          id:"22222222-2222-4222-8222-222222222222".parse().unwrap(),
          project_id:"11111111-1111-4111-8111-111111111111".parse().unwrap(),
          work_id:"44444444-4444-4444-8444-444444444444".parse().unwrap(),
          configuration_id:"55555555-5555-4555-8555-555555555555".parse().unwrap(),
          configuration_version:serde_json::from_value(serde_json::json!(1)).unwrap(),
          source:"manual".parse().unwrap(), external_identity:"operator/work".parse().unwrap(),
          state:"admitted".parse().unwrap(), version:serde_json::from_value(serde_json::json!(1)).unwrap(),
          admitted_at:serde_json::from_value(serde_json::json!(1)).unwrap(), updated_at:serde_json::from_value(serde_json::json!(1)).unwrap(),
          diagnostics:serde_json::from_value(serde_json::json!({"base_revision":"exact-base","candidate":candidate,"current_nodes":[],"active_node_count":0,
            "budget":{"usage":{"attempts":1,"elapsed_millis":0,"tokens":0,"cost_micro_units":0,"output_bytes":0},"limit":{"max_attempts":4,"max_elapsed_millis":1000,"max_tokens":0,"max_cost_micro_units":0,"max_output_bytes":0}},
            "latest_decision":{"id":"88888888-8888-4888-8888-888888888888","subject":candidate,"outcome":"accept"},
            "delivery":null,"escalation":null})).unwrap(),
        }],
      });
    }
    Ok(octacity_server_application::FactoryRunPageProjection {
      total: 0,
      items: vec![],
      next_cursor: None,
    })
  }
}

#[tokio::test]
async fn lists_project_and_global_phase_ready_pools_with_bounded_exact_policy_filters() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application),
  );
  for prefix in [
    "/api/v1/factory-phase-ready",
    "/api/v1/projects/11111111-1111-4111-8111-111111111111/factory-phase-ready",
  ] {
    let response = routes
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("{prefix}?policy_digest={}&limit=1", "ab".repeat(32)),
        None,
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
      serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body, serde_json::json!({"total":0,"items":[],"next_cursor":null}));
    assert_json_matches_component(&openapi_document(), "FactoryPhaseReadyPage", &body);
    for parameters in [
      "limit=1",
      "policy_digest=bad",
      "policy_digest=AB",
      "policy_digest=ab&limit=0",
      "policy_digest=ab&after=invalid",
    ] {
      assert_eq!(
        routes
          .clone()
          .oneshot(empty_request("GET", &format!("{prefix}?{parameters}"), None))
          .await
          .unwrap()
          .status(),
        StatusCode::BAD_REQUEST
      );
    }
  }
}

#[async_trait]
impl ManagementQueryUseCase<octacity_server_application::ListFactoryPhaseReadyQuery> for RecordingApplication {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    query: octacity_server_application::ListFactoryPhaseReadyQuery,
  ) -> Result<octacity_server_application::FactoryPhaseReadyPageProjection, Self::Error> {
    self.record("list_factory_phase_ready");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    assert_eq!(query.page().limit(), 1);
    Ok(octacity_server_application::FactoryPhaseReadyPageProjection::default())
  }
}

#[test]
fn factory_discovery_schemas_accept_advisory_severity_and_preserve_work_input_bounds() {
  let document = openapi_document();
  let entry = serde_json::json!({"id":"ab".repeat(32),"policy_digest":"cd".repeat(32),"work_id":"77777777-7777-4777-8777-777777777777","project_id":"11111111-1111-4111-8111-111111111111","run_id":"66666666-6666-4666-8666-666666666666","run_version":1,"flow_run_id":"33333333-3333-4333-8333-333333333333","cycle_id":"55555555-5555-4555-8555-555555555555","node":"operator.step","generation":0,"severity":"advisory","project_priority":10,"admitted_at_unix_ms":1});
  assert_json_matches_component(&document, "FactoryPhaseReadyResource", &entry);
  let schemas = &document["components"]["schemas"];
  assert_eq!(
    schemas["FactoryWorkClassification"]["properties"]["metadata"]["maxProperties"],
    32
  );
  assert_eq!(
    schemas["FactoryWorkClassification"]["properties"]["metadata"]["additionalProperties"]["maxLength"],
    512
  );
  assert_eq!(
    schemas["AdmitManualFactoryWorkRequest"]["properties"]["external_identity"]["maxLength"],
    256
  );
}

#[tokio::test]
async fn factory_validation_errors_expose_safe_field_diagnostics() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application),
  );
  let mut request: serde_json::Value = serde_json::from_str(CREATE_CONFIGURATION).unwrap();
  request["definition"]["admission_policy"] = "unavailable".into();
  let response = routes
    .oneshot(json_request(
      "POST",
      "/api/v1/factory-configurations/validate",
      "unused",
      None,
      &request.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(
    body["diagnostics"]["errors"],
    serde_json::json!([{"field":"/admission_policy","code":"unavailable_reference"}])
  );
  assert_json_matches_component(&openapi_document(), "ErrorResponse", &body);
}

#[tokio::test]
async fn factory_run_collection_decodes_closed_decision_and_delivery_filters() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application),
  );
  for prefix in [
    "/api/v1/factory-runs",
    "/api/v1/projects/11111111-1111-4111-8111-111111111111/factory-runs",
  ] {
    let response = routes
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("{prefix}?state=admitted&source=filter-contract&decision=accept&delivery_state=failed&limit=1"),
        None,
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    for invalid in ["decision=advisory", "delivery_state=future"] {
      let response = routes
        .clone()
        .oneshot(empty_request(
          "GET",
          &format!("{prefix}?state=admitted&{invalid}&limit=1"),
          None,
        ))
        .await
        .unwrap();
      assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
  }
}

#[tokio::test]
async fn factory_run_summary_returns_only_safe_exact_diagnostics() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application),
  );
  let response = routes
    .oneshot(empty_request(
      "GET",
      "/api/v1/factory-runs?state=admitted&source=summary-contract&limit=1",
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  let diagnostics = &body["items"][0]["diagnostics"];
  assert_eq!(diagnostics["base_revision"], "exact-base");
  assert_eq!(diagnostics["candidate"]["candidate_revision"], "exact-candidate");
  assert_eq!(diagnostics["latest_decision"]["outcome"], "accept");
  assert_eq!(diagnostics["budget"]["usage"]["attempts"], 1);
  assert_json_matches_component(&openapi_document(), "FactoryRunSummaryPage", &body);
}

#[tokio::test]
async fn malformed_factory_limits_return_field_diagnostics_before_dispatch() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application.clone()),
  );
  let mut request: serde_json::Value = serde_json::from_str(CREATE_CONFIGURATION).unwrap();
  request["definition"]["wip_limits"]["max_active_runs"] = 0.into();
  let response = routes
    .oneshot(json_request(
      "POST",
      "/api/v1/factory-configurations/validate",
      "unused",
      None,
      &request.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(body["diagnostics"]["errors"][0]["field"], "/wip_limits/max_active_runs");
  assert_eq!(body["diagnostics"]["errors"][0]["code"], "invalid_contract");
  assert!(application.calls.lock().unwrap().is_empty());
}
