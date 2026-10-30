use super::*;
use octacity_server_application::{FactoryConfigurationPageProjection, ListProjectFactoryConfigurationsQuery};

#[async_trait]
impl ManagementQueryUseCase<octacity_server_application::ValidateFactoryFlowDataSchemaQuery> for RecordingApplication {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: octacity_server_application::ValidateFactoryFlowDataSchemaQuery,
  ) -> Result<
    <octacity_server_application::ValidateFactoryFlowDataSchemaQuery as octacity_server_application::Query>::Outcome,
    Self::Error,
  > {
    self.record("validate_factory_data_schema");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    let store = Arc::new(octacity_server_application::testing::InMemoryFactoryConfigurationStore::new());
    store.seed_project(query.project_id).unwrap();
    store
      .enable_project(
        query.project_id,
        octacity_server_application::testing::installed_preset_choices(query.project_id).0,
      )
      .unwrap();
    octacity_server_application::FactoryFlowDefinitionHandlers::new(
      store,
      Arc::new(octacity_server_application::TrustedNetworkManagementPolicy),
    )
    .execute_management_query(context, grant, query)
    .await
  }
}

#[tokio::test]
async fn normalizes_a_project_scoped_data_schema_without_a_caller_digest_or_publication() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application.clone()),
  );
  let response = routes.oneshot(json_request("POST", "/api/v1/flow-data-schemas/validate", "unused-validation-key", None, r#"{"project_id":"11111111-1111-4111-8111-111111111111","identity":"operator.boolean","version":"v1","shape":{"type":"boolean"}}"#)).await.unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(
    body["reference"]["digest"],
    "843fd3f60f5da95afe13dac9521ccdc83dbda0c664d61917c1b2dd321b350fc3"
  );
  assert_eq!(*application.calls.lock().unwrap(), ["validate_factory_data_schema"]);
  assert_json_matches_component(&openapi_document(), "FactoryFlowDataSchema", &body);
}
#[async_trait]
impl ManagementCommandUseCase<octacity_server_application::ConfirmFactoryNodeOperationCommand>
  for RecordingApplication
{
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    command: octacity_server_application::ConfirmFactoryNodeOperationCommand,
  ) -> Result<octacity_server_application::FactoryRunControlCommandOutcome, Self::Error> {
    self.record("confirm_factory_node_operation");
    assert_eq!(
      command.node_attempt_id.to_string(),
      "44444444-4444-4444-8444-444444444444"
    );
    assert_eq!(command.reason.as_str(), "operator confirmation");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    Ok(octacity_server_application::FactoryRunControlCommandOutcome {
      disposition: octacity_server_application::MutationDisposition::Applied,
      run_id: command.run_id,
      version: serde_json::from_value(serde_json::json!(2)).unwrap(),
      state: "running".parse().unwrap(),
      cancellation_requested: false,
    })
  }
}
#[async_trait]
impl ManagementCommandUseCase<octacity_server_application::RetryFactoryNodeCommand> for RecordingApplication {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    command: octacity_server_application::RetryFactoryNodeCommand,
  ) -> Result<octacity_server_application::FactoryRunControlCommandOutcome, Self::Error> {
    self.record("retry_factory_node");
    assert_eq!(
      command.node_attempt_id.to_string(),
      "44444444-4444-4444-8444-444444444444"
    );
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    Ok(octacity_server_application::FactoryRunControlCommandOutcome {
      disposition: octacity_server_application::MutationDisposition::Applied,
      run_id: command.run_id,
      version: serde_json::from_value(serde_json::json!(2)).unwrap(),
      state: "running".parse().unwrap(),
      cancellation_requested: false,
    })
  }
}

#[async_trait]
impl ManagementCommandUseCase<octacity_server_application::SubmitFactoryNodeResponseCommand> for RecordingApplication {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    command: octacity_server_application::SubmitFactoryNodeResponseCommand,
  ) -> Result<octacity_server_application::FactoryRunControlCommandOutcome, Self::Error> {
    self.record("submit_factory_node_response");
    assert_eq!(
      command.node_attempt_id.to_string(),
      "44444444-4444-4444-8444-444444444444"
    );
    assert_eq!(command.outcome.as_str(), "operator-chosen");
    assert_eq!(command.value, serde_json::json!(true));
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    Ok(octacity_server_application::FactoryRunControlCommandOutcome {
      disposition: octacity_server_application::MutationDisposition::Applied,
      run_id: command.run_id,
      version: serde_json::from_value(serde_json::json!(2)).unwrap(),
      state: "running".parse().unwrap(),
      cancellation_requested: false,
    })
  }
}

#[async_trait]
impl ManagementCommandUseCase<octacity_server_application::CancelFactoryRunCommand> for RecordingApplication {
  type Error = ApplicationError;
  async fn execute_management_command(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    command: octacity_server_application::CancelFactoryRunCommand,
  ) -> Result<octacity_server_application::FactoryRunControlCommandOutcome, Self::Error> {
    self.record("cancel_factory_run");
    assert_eq!(command.expected_version.get(), 1);
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    Ok(octacity_server_application::FactoryRunControlCommandOutcome {
      disposition: octacity_server_application::MutationDisposition::Applied,
      run_id: command.run_id,
      version: serde_json::from_value(serde_json::json!(2)).unwrap(),
      state: "running".parse().unwrap(),
      cancellation_requested: true,
    })
  }
}

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
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    let mut details: serde_json::Value = serde_json::from_str(CONFIGURATION_DETAILS).unwrap();
    details["configuration"]["flow"]["pools"] = serde_json::json!({"operator-chosen-queue": {
      "policy": {"phase":"operator-chosen-queue", "order":["severity","project_priority","age"], "max_wip":2, "max_project_wip":1, "budget":details["configuration"]["flow"]["limits"]["budget"]},
      "project_priority":0, "capabilities":[], "selection":{"value":{"mapping":"constant","value":{}}}
    }});
    Ok(octacity_server_application::FactoryConfigurationProjection {
      configuration: serde_json::from_value(details["configuration"].clone()).unwrap(),
      published_at: Timestamp::from_unix_millis(1).unwrap(),
    })
  }
}

#[tokio::test]
async fn factory_configuration_detail_exposes_server_owned_named_pool_identities() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application),
  );
  let response = routes
    .oneshot(empty_request(
      "GET",
      "/api/v1/factory-configurations/22222222-2222-4222-8222-222222222222",
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(body["pool_policy_digests"].as_object().unwrap().len(), 1);
  assert_eq!(
    body["pool_policy_digests"]["operator-chosen-queue"],
    "a053c331ce602228de4bcae35cfe895f350210f15404976dedac9c30446fe7fb"
  );
  assert_json_matches_component(&openapi_document(), "FactoryConfigurationDetails", &body);
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
    query: octacity_server_application::ValidateFactoryFlowDefinitionQuery,
  ) -> Result<octacity_server_application::FactoryFlowDefinitionValidation, Self::Error> {
    if let Some(binding) = query
      .draft
      .root
      .nodes
      .iter()
      .filter_map(|node| node.action())
      .find(|binding| binding.action().as_str() == "transition_issue")
    {
      assert_eq!(
        binding.parameters(),
        &serde_json::json!({"workflow_id":"10000000-0000-4000-8000-000000000010","workflow_version":3,"transition_id":"10000000-0000-4000-8000-000000000011","outcome":"succeeded"})
      );
      self.record("validate_local_issue_action");
    }
    self.record("validate_factory_flow_definition");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    let configuration: serde_json::Value = serde_json::from_str(CREATE_CONFIGURATION).unwrap();
    Ok(serde_json::from_value(configuration["definition"]["flow"].clone()).unwrap())
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
    self
      .factory_admission_overrides
      .lock()
      .unwrap()
      .push(serde_json::to_value(&command.profile_overrides).unwrap());
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
      return Ok(octacity_server_application::FactoryRunPageProjection {
        total: 1,
        next_cursor: None,
        items: vec![factory_run_summary_fixture()],
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

#[tokio::test]
async fn factory_run_detail_returns_safe_current_facts_and_a_strong_precondition() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application),
  );
  let response = routes
    .oneshot(empty_request(
      "GET",
      "/api/v1/factory-runs/22222222-2222-4222-8222-222222222222",
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["etag"], "\"1\"");
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(body["id"], "22222222-2222-4222-8222-222222222222");
  assert_eq!(body["diagnostics"]["latest_decision"]["outcome"], "accept");
  assert!(body.get("task").is_none());
  assert!(body.get("context").is_none());
  assert_json_matches_component(&openapi_document(), "FactoryRunSummaryResource", &body);
}

fn factory_run_summary_fixture() -> octacity_server_application::FactoryRunSummaryProjection {
  let exact = serde_json::json!({"project_id":"11111111-1111-4111-8111-111111111111", "repository_id":"33333333-3333-4333-8333-333333333333", "base_revision":"exact-base"});
  let candidate =
    serde_json::json!({"exact":exact,"candidate_revision":"exact-candidate","changeset_digest":"ab".repeat(32)});
  octacity_server_application::FactoryRunSummaryProjection {
    issue_association: None,
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
        }
}
#[async_trait]
impl ManagementQueryUseCase<octacity_server_application::GetFactoryRunQuery> for RecordingApplication {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    query: octacity_server_application::GetFactoryRunQuery,
  ) -> Result<octacity_server_application::FactoryRunSummaryProjection, ApplicationError> {
    self.record("get_factory_run");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    let fixture = factory_run_summary_fixture();
    assert_eq!(query.run_id, fixture.id);
    Ok(fixture)
  }
}

#[tokio::test]
async fn factory_flow_collection_is_bounded_and_omits_execution_inputs() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application),
  );
  let response = routes
    .oneshot(empty_request(
      "GET",
      "/api/v1/factory-runs/22222222-2222-4222-8222-222222222222/flow-runs?limit=1",
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(body["items"][0]["kind"], "flow_run");
  assert_eq!(body["items"][0]["record"]["definition"]["version"], 1);
  assert!(body["items"][0]["record"].get("input").is_none());
  assert_json_matches_component(&openapi_document(), "FactoryDiagnosticPage", &body);
}

#[async_trait]
impl ManagementQueryUseCase<octacity_server_application::ListFactoryDiagnosticsQuery> for RecordingApplication {
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    query: octacity_server_application::ListFactoryDiagnosticsQuery,
  ) -> Result<octacity_server_application::FactoryDiagnosticPage, ApplicationError> {
    self.record("list_factory_diagnostics");
    if !self.successful_workflow {
      return Err(ApplicationError::unavailable());
    }
    assert_eq!(query.limit(), 1);
    if let Some(facts) = factory_domain_fact_fixture(query.collection()) {
      return Ok(octacity_server_application::FactoryDiagnosticPage {
        items: vec![
          serde_json::from_value(serde_json::json!({"kind":"domain_fact","record":{
            "id":"ab".repeat(32), "node_attempt_id":"44444444-4444-4444-8444-444444444444",
            "flow_run_id":"22222222-2222-4222-8222-222222222222", "cycle_id":"22222222-2222-4222-8222-222222222222",
            "input_digest":"ac".repeat(32), "observed_at":2, "facts":facts
          }}))
          .unwrap(),
        ],
        next_cursor: None,
      });
    }
    if query.collection() == octacity_server_application::FactoryDiagnosticCollection::Calls {
      return Ok(octacity_server_application::FactoryDiagnosticPage {
        items: vec![serde_json::from_value(serde_json::json!({"kind":"call","record":{
          "node":{"id":"44444444-4444-4444-8444-444444444444","factory_run_id":"22222222-2222-4222-8222-222222222222",
            "flow_run_id":"22222222-2222-4222-8222-222222222222","cycle_id":"22222222-2222-4222-8222-222222222222",
            "node_key":"arbitrary","node_kind":"reasoning","number":1,"input_digest":"ab".repeat(32),
            "execution":{"kind":"external","identity":{"identity":"installed","version":"v1","digest":"ab".repeat(32)}},
            "started_at":1,"deadline":10000,"retry_of":null},
          "profile":{"operation_id":"ab".repeat(32),"context_id":"88888888-8888-4888-8888-888888888888","context_digest":"ac".repeat(32),
            "tool":{"identity":"harness","version":"v1","digest":"ab".repeat(32)},
            "plugin":{"identity":"runner","version":"v1","digest":"ab".repeat(32)},
            "model_or_tool":{"identity":"model","version":"v1","digest":"ab".repeat(32)},
            "task_digest":"ad".repeat(32),"permission_digest":"ae".repeat(32),
            "build_configuration":{"id":"55555555-5555-4555-8555-555555555555","version":1,"project_id":"11111111-1111-4111-8111-111111111111","definition_digest":"ab".repeat(32)}}
        }})).unwrap()], next_cursor: None,
      });
    }
    let terminal = match query.collection() {
      octacity_server_application::FactoryDiagnosticCollection::Work => {
        Some(serde_json::json!({"kind":"work","record":{
          "id":"44444444-4444-4444-8444-444444444444",
          "configuration":{"id":"55555555-5555-4555-8555-555555555555","version":1,"project_id":"11111111-1111-4111-8111-111111111111","definition_digest":"ab".repeat(32)},
          "external_identity":"operator/work", "subject":factory_domain_fact_fixture(octacity_server_application::FactoryDiagnosticCollection::Escalations).unwrap()["subject"],
          "priority":10,"risk":"medium"
        }}))
      }
      octacity_server_application::FactoryDiagnosticCollection::NodeCompletions => {
        Some(serde_json::json!({"kind":"node_completion","record":{
          "id":"ab".repeat(32),"node_attempt_id":"44444444-4444-4444-8444-444444444444",
          "flow_run_id":"22222222-2222-4222-8222-222222222222","cycle_id":"22222222-2222-4222-8222-222222222222",
          "node_key":"arbitrary","outcome":"declared-result","output_schema":{"identity":"result","version":"v1","digest":"ab".repeat(32)},
          "output_digest":"ac".repeat(32),"usage":{"attempts":0,"elapsed_millis":0,"tokens":0,"cost_micro_units":0,"output_bytes":0},"observed_at":20
        }}))
      }
      octacity_server_application::FactoryDiagnosticCollection::NodeStops => {
        Some(serde_json::json!({"kind":"node_stop","record":{
          "id":"ab".repeat(32),"node_attempt_id":"44444444-4444-4444-8444-444444444444","reason":{"kind":"execution_failed"},
          "usage":{"attempts":0,"elapsed_millis":0,"tokens":0,"cost_micro_units":0,"output_bytes":0},"execution":null,"observed_at":20
        }}))
      }
      _ => None,
    };
    if let Some(record) = terminal {
      return Ok(octacity_server_application::FactoryDiagnosticPage {
        items: vec![serde_json::from_value(record).unwrap()],
        next_cursor: None,
      });
    }
    let other = match query.collection() {
      octacity_server_application::FactoryDiagnosticCollection::Builds => {
        Some(serde_json::json!({"kind":"build","record":{
          "id":"ab".repeat(32),"operation_id":"ac".repeat(32),"node_attempt_id":"44444444-4444-4444-8444-444444444444",
          "build_id":"55555555-5555-4555-8555-555555555555","attempt_id":"66666666-6666-4666-8666-666666666666",
          "jobs":["77777777-7777-4777-8777-777777777777"]
        }}))
      }
      octacity_server_application::FactoryDiagnosticCollection::Budgets => {
        Some(serde_json::json!({"kind":"budget","record":{
          "id":"ab".repeat(32),"run_version":1,"usage":{"attempts":0,"elapsed_millis":0,"tokens":0,"cost_micro_units":0,"output_bytes":0},"recorded_at":1
        }}))
      }
      octacity_server_application::FactoryDiagnosticCollection::Audit => {
        Some(serde_json::json!({"kind":"audit","record":{
          "id":"ab".repeat(32),"run_id":"22222222-2222-4222-8222-222222222222","actor_kind":"unauthenticated_management",
          "actor_identity_digest":null,"operation":"factory.admitted","request_identity_digest":"ac".repeat(32),"outcome":"applied","recorded_at":1
        }}))
      }
      _ => None,
    };
    if let Some(record) = other {
      return Ok(octacity_server_application::FactoryDiagnosticPage {
        items: vec![serde_json::from_value(record).unwrap()],
        next_cursor: None,
      });
    }
    if query.collection() == octacity_server_application::FactoryDiagnosticCollection::NodeAttempts {
      return Ok(octacity_server_application::FactoryDiagnosticPage {
        items: vec![
          serde_json::from_value(serde_json::json!({"kind":"node_attempt","record":{
            "id":"44444444-4444-4444-8444-444444444444","factory_run_id":"22222222-2222-4222-8222-222222222222",
            "flow_run_id":"22222222-2222-4222-8222-222222222222","cycle_id":"22222222-2222-4222-8222-222222222222",
            "node_key":"arbitrary","node_kind":"human_gate","number":1,"input_digest":"ab".repeat(32),
            "execution":{"kind":"built_in"},"started_at":1,"deadline":10000,"retry_of":null,"retryable":false,"active":true
          }}))
          .unwrap(),
        ],
        next_cursor: None,
      });
    }
    if query.collection() == octacity_server_application::FactoryDiagnosticCollection::WorkflowCycles {
      return Ok(octacity_server_application::FactoryDiagnosticPage {
        items: vec![
          serde_json::from_value(serde_json::json!({"kind":"workflow_cycle","record":{
            "id":"22222222-2222-4222-8222-222222222222","flow_run_id":"22222222-2222-4222-8222-222222222222",
            "number":1,"predecessor":null
          }}))
          .unwrap(),
        ],
        next_cursor: None,
      });
    }
    Ok(octacity_server_application::FactoryDiagnosticPage {
      items: vec![
        serde_json::from_value(serde_json::json!({"kind":"flow_run","record":{
          "id":"22222222-2222-4222-8222-222222222222","factory_run_id":"22222222-2222-4222-8222-222222222222",
          "definition":{"id":"33333333-3333-4333-8333-333333333333","version":1,"digest":"ab".repeat(32)},"parent":null
        }}))
        .unwrap(),
      ],
      next_cursor: None,
    })
  }
}

#[tokio::test]
async fn factory_cycle_collection_matches_the_published_openapi_contract() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application),
  );
  let response = routes
    .oneshot(empty_request(
      "GET",
      "/api/v1/factory-runs/22222222-2222-4222-8222-222222222222/workflow-cycles?limit=1",
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(body["items"][0]["kind"], "workflow_cycle");
  assert_json_matches_component(&openapi_document(), "FactoryDiagnosticPage", &body);
}

#[tokio::test]
async fn factory_node_collection_has_an_explicit_safe_openapi_projection() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application),
  );
  let response = routes
    .oneshot(empty_request(
      "GET",
      "/api/v1/factory-runs/22222222-2222-4222-8222-222222222222/node-attempts?limit=1",
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(body["items"][0]["kind"], "node_attempt");
  assert_json_matches_component(&openapi_document(), "FactoryDiagnosticPage", &body);
}

#[tokio::test]
async fn factory_build_budget_and_audit_collections_match_safe_openapi_schemas() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application),
  );
  for (collection, kind) in [("builds", "build"), ("budgets", "budget"), ("audit", "audit")] {
    let response = routes
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("/api/v1/factory-runs/22222222-2222-4222-8222-222222222222/{collection}?limit=1"),
        None,
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "{collection}");
    let body: serde_json::Value =
      serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["items"][0]["kind"], kind);
    assert_json_matches_component(&openapi_document(), "FactoryDiagnosticPage", &body);
  }
}

fn factory_domain_fact_fixture(
  collection: octacity_server_application::FactoryDiagnosticCollection,
) -> Option<serde_json::Value> {
  use octacity_server_application::FactoryDiagnosticCollection::*;
  let candidate = serde_json::to_value(factory_run_summary_fixture().diagnostics.candidate.unwrap()).unwrap();
  let exact = candidate["exact"].clone();
  let id = "88888888-8888-4888-8888-888888888888";
  let reference = serde_json::json!({"identity":"installed","version":"v1","digest":"ab".repeat(32)});
  Some(match collection {
    Candidates => serde_json::json!({"kind":"candidate","subject":candidate}),
    ChangeSets => {
      serde_json::json!({"kind":"changeset","subject":candidate,"id":id,"node_attempt_id":"44444444-4444-4444-8444-444444444444"})
    }
    Evidence => {
      serde_json::json!({"kind":"evidence","subject":candidate,"id":id,"changeset_id":id,"digest":"ab".repeat(32),"item_count":1})
    }
    ReviewPlans => {
      serde_json::json!({"kind":"review_plan","subject":candidate,"id":id,"evidence_id":id,"digest":"ab".repeat(32),"purpose":"verification","required_quorum":1,"deadline":1000})
    }
    Assessments => {
      serde_json::json!({"kind":"assessment","subject":candidate,"id":id,"plan_id":id,"digest":"ab".repeat(32),"evaluator":reference,"model":reference,"prompt_digest":"ac".repeat(32),"outcome":"satisfied"})
    }
    Decisions => serde_json::json!({"kind":"decision","subject":candidate,"id":id,"outcome":"rework",
      "explanation":{"plan_id":"77777777-7777-4777-8777-777777777777", "assessment_ids":[id],"signal_receipt_ids":[],
        "reasons":[{"RequiredEvidenceFailed":"contract_tests"}],"input_digest":"ac".repeat(32),"policy_version":1,"policy_digest":"ad".repeat(32)}}),
    Delivery => serde_json::json!({"kind":"delivery","subject":candidate,"id":id,"decision_id":id,"state":"succeeded"}),
    Escalations => serde_json::json!({"kind":"escalation","subject":exact,"id":id}),
    Reporting => serde_json::json!({"kind":"reporting","subject":exact,"id":id,"number":1,"state":"succeeded"}),
    Promotion => {
      serde_json::json!({"kind":"promotion","subject":candidate,"plan_digest":"ab".repeat(32),"input_digest":"ac".repeat(32),"environment_contract":reference,"deployment":reference,"outcome":"passed","started_at":1,"finished_at":100})
    }
    DecisionSignals => {
      serde_json::json!({"kind":"decision_signal","subject":exact,"id":id,"request_id":id,"receipt_digest":"ab".repeat(32),"input_digest":"ac".repeat(32),"frozen_input_digest":"ad".repeat(32),"question_set":reference,"probability_semantics":reference,"provider":reference,"adapter":reference,"model":reference,"policy":reference,"purpose":"routing","mode":"advisory","fallback":"escalate","answers":[{"question":reference,"value":{"FiniteChoice":{"selected":"fast","selected_probability":900000,"runner_up_probability":100000}}}],"state":"completed",
      "consumption":{"baseline":{"Route":"baseline"},"recommendation":{"Route":"fast"},"final_disposition":{"Route":"baseline"},"kind":"Advisory"}})
    }
    _ => return None,
  })
}

#[tokio::test]
async fn factory_input_sources_document_the_actual_retained_record_views() {
  let document = openapi_document();
  assert_eq!(
    document["components"]["schemas"]["FactoryFlowRecordView"]["enum"],
    serde_json::json!(["input_payload", "payload", "metadata"])
  );
  for view in ["input_payload", "payload", "metadata"] {
    assert_json_matches_component(
      &document,
      "FactoryFlowInputSource",
      &serde_json::json!({"source":"predecessor","view":view,"path":""}),
    );
  }
}

#[tokio::test]
async fn factory_domain_collections_publish_safe_typed_openapi_facts() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application),
  );
  for collection in octacity_server_application::FactoryDiagnosticCollection::ALL {
    let Some(expected) = factory_domain_fact_fixture(collection) else {
      continue;
    };
    let response = routes
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!(
          "/api/v1/factory-runs/22222222-2222-4222-8222-222222222222/{}?limit=1",
          collection.as_str()
        ),
        None,
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "{collection:?}");
    let body: serde_json::Value =
      serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["items"][0]["record"]["facts"], expected);
    assert_json_matches_component(&openapi_document(), "FactoryDiagnosticPage", &body);
  }
}

#[tokio::test]
async fn factory_calls_publish_frozen_task_metadata_without_context_entries() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application),
  );
  let response = routes
    .oneshot(empty_request(
      "GET",
      "/api/v1/factory-runs/22222222-2222-4222-8222-222222222222/calls?limit=1",
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(body["items"][0]["kind"], "call");
  assert!(body["items"][0]["record"]["profile"].get("entries").is_none());
  assert_json_matches_component(&openapi_document(), "FactoryDiagnosticPage", &body);
}

#[tokio::test]
async fn factory_work_completions_and_stops_have_distinct_safe_openapi_records() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application),
  );
  for (collection, kind) in [
    ("work", "work"),
    ("node-completions", "node_completion"),
    ("node-stops", "node_stop"),
  ] {
    let response = routes
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("/api/v1/factory-runs/22222222-2222-4222-8222-222222222222/{collection}?limit=1"),
        None,
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
      serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["items"][0]["kind"], kind);
    assert!(body["items"][0]["record"].get("ownership").is_none());
    assert_json_matches_component(&openapi_document(), "FactoryDiagnosticPage", &body);
  }
}

#[tokio::test]
async fn diagnostic_pages_reject_unbounded_or_foreign_cursors_before_dispatch() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application.clone()),
  );
  for parameters in [
    "limit=0",
    "limit=101",
    "limit=1&offset=1",
    "limit=1&after=invalid",
    "limit=1&after=1:33333333-3333-4333-8333-333333333333:flow-runs:44444444-4444-4444-8444-444444444444",
    "limit=1&after=1:22222222-2222-4222-8222-222222222222:workflow-cycles:44444444-4444-4444-8444-444444444444",
    "limit=1&after=1:22222222-2222-4222-8222-222222222222:flow-runs:bad-identity",
  ] {
    let response = routes
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("/api/v1/factory-runs/22222222-2222-4222-8222-222222222222/flow-runs?{parameters}"),
        None,
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{parameters}");
  }
  assert!(application.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn cancellation_requires_a_precondition_and_returns_a_typed_correlated_receipt() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application.clone()),
  );
  let path = "/api/v1/factory-runs/22222222-2222-4222-8222-222222222222/cancellations";
  let response = routes
    .clone()
    .oneshot(json_request("POST", path, "cancel-run", Some("\"1\""), "{}"))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers().get("etag").unwrap(), "\"2\"");
  assert!(response.headers().contains_key("x-request-id"));
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(body["cancellation_requested"], true);
  assert_eq!(body["run_id"], "22222222-2222-4222-8222-222222222222");
  assert_json_matches_component(&openapi_document(), "FactoryRunControlResponse", &body);
  let response = routes
    .oneshot(json_request("POST", path, "cancel-run", None, "{}"))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::PRECONDITION_REQUIRED);
  assert_eq!(*application.calls.lock().unwrap(), ["cancel_factory_run"]);
}

#[tokio::test]
async fn human_response_addresses_an_exact_attempt_without_a_caller_supplied_schema() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application.clone()),
  );
  let path =
    "/api/v1/factory-runs/22222222-2222-4222-8222-222222222222/nodes/44444444-4444-4444-8444-444444444444/responses";
  let response = routes
    .clone()
    .oneshot(json_request(
      "POST",
      path,
      "node-response",
      Some("\"1\""),
      r#"{"outcome":"operator-chosen","value":true}"#,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(body["cancellation_requested"], false);
  assert_json_matches_component(&openapi_document(), "FactoryRunControlResponse", &body);
  let response = routes
    .oneshot(json_request(
      "POST",
      path,
      "node-response",
      Some("\"1\""),
      r#"{"outcome":"operator-chosen","value":true,"state":"completed"}"#,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  assert_eq!(*application.calls.lock().unwrap(), ["submit_factory_node_response"]);
}

#[tokio::test]
async fn retry_confirms_the_exact_attempt_and_rejects_a_caller_defined_next_state() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application.clone()),
  );
  let path =
    "/api/v1/factory-runs/22222222-2222-4222-8222-222222222222/nodes/44444444-4444-4444-8444-444444444444/retries";
  let response = routes
    .clone()
    .oneshot(json_request("POST", path, "retry-node", Some("\"1\""), "{}"))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_json_matches_component(&openapi_document(), "FactoryRunControlResponse", &body);
  let response = routes
    .oneshot(json_request(
      "POST",
      path,
      "retry-node",
      Some("\"1\""),
      r#"{"state":"running"}"#,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  assert_eq!(*application.calls.lock().unwrap(), ["retry_factory_node"]);
}

#[tokio::test]
async fn operator_requests_expose_only_closed_dispositions_and_a_bounded_reason() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application.clone()),
  );
  let path =
    "/api/v1/factory-runs/22222222-2222-4222-8222-222222222222/nodes/44444444-4444-4444-8444-444444444444/requests";
  for (operation, disposition) in [
    ("escalation", "resume"),
    ("delivery", "confirm"),
    ("promotion", "confirm"),
  ] {
    let request = serde_json::json!({"operation":operation,"disposition":disposition,"reason":"operator confirmation"});
    let response = routes
      .clone()
      .oneshot(json_request(
        "POST",
        path,
        operation,
        Some("\"1\""),
        &request.to_string(),
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "{operation}");
    assert_json_matches_component(&openapi_document(), "FactoryNodeOperationRequest", &request);
    let body: serde_json::Value =
      serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_json_matches_component(&openapi_document(), "FactoryRunControlResponse", &body);
  }
  for invalid in [
    serde_json::json!({"operation":"delivery","disposition":"resume","reason":"operator confirmation"}),
    serde_json::json!({"operation":"delivery","disposition":"confirm","reason":"operator confirmation","candidate_digest":"ab".repeat(32)}),
    serde_json::json!({"operation":"future","disposition":"confirm","reason":"operator confirmation"}),
    serde_json::json!({"operation":"delivery","disposition":"confirm","reason":""}),
  ] {
    let response = routes
      .clone()
      .oneshot(json_request(
        "POST",
        path,
        "bad-request",
        Some("\"1\""),
        &invalid.to_string(),
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  }
  assert_eq!(
    *application.calls.lock().unwrap(),
    ["confirm_factory_node_operation"; 3]
  );
}

#[test]
fn operator_gate_bindings_are_part_of_the_closed_flow_authoring_grammar() {
  let mut node = flow_draft_request()["definition"]["root"]["nodes"][0].clone();
  node["kind"] = serde_json::json!("human_gate");
  for operation in ["delivery", "promotion", "escalation"] {
    let disposition = if operation == "escalation" { "resume" } else { "confirm" };
    node["operator_requests"] =
      serde_json::json!({"operation":operation,"outcomes":{disposition:"caller-selected-result"}});
    assert_json_matches_component(&openapi_document(), "FactoryFlowNodeDefinition", &node);
  }
}

#[tokio::test]
async fn flow_validation_adds_safe_capability_and_permission_diagnostics_without_publishing() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application.clone()),
  );
  let response = routes
    .oneshot(json_request(
      "POST",
      "/api/v1/flow-definitions/validate",
      "validation-only",
      None,
      &flow_draft_request().to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert!(body["closure"]["root"].is_object());
  assert_eq!(body["diagnostics"]["errors"], serde_json::json!([]));
  assert_eq!(
    body["diagnostics"]["required_node_capabilities"][0]["identity"],
    "contract.capability"
  );
  assert_eq!(
    body["diagnostics"]["effective_permissions"][0]["permissions"]["resources"]["cpu_millis"],
    0
  );
  assert_eq!(*application.calls.lock().unwrap(), ["validate_factory_flow_definition"]);
  assert_json_matches_component(&openapi_document(), "FactoryFlowDefinitionValidation", &body);
}

#[test]
fn local_issue_status_parameters_have_a_closed_provider_free_http_contract() {
  let document = openapi_document();
  let parameters = serde_json::json!({
    "workflow_id":"10000000-0000-4000-8000-000000000010", "workflow_version":3,
    "transition_id":"10000000-0000-4000-8000-000000000011", "outcome":"published"
  });
  assert_json_matches_component(&document, "FactoryIssueStatusActionParameters", &parameters);
  let schema = &document["components"]["schemas"]["FactoryIssueStatusActionParameters"];
  assert_eq!(schema["additionalProperties"], false);
  assert_eq!(schema["required"].as_array().unwrap().len(), 4);
  assert!(schema.to_string().contains("workflow_version"));
  assert!(!schema.to_string().contains("connection"));
}

#[tokio::test]
async fn existing_flow_validation_forwards_exact_local_issue_action_without_provider_fields() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(application.clone(), application.clone()),
  );
  let mut request = flow_draft_request();
  request["definition"]["root"]["nodes"][0]["action"]["action"] = serde_json::json!("transition_issue");
  request["definition"]["root"]["nodes"][0]["action"]["parameters"] = serde_json::json!({"workflow_id":"10000000-0000-4000-8000-000000000010","workflow_version":3,"transition_id":"10000000-0000-4000-8000-000000000011","outcome":"succeeded"});
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
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(
    *application.calls.lock().unwrap(),
    ["validate_local_issue_action", "validate_factory_flow_definition"]
  );
}

#[tokio::test]
async fn manual_factory_admission_preserves_exact_optional_profile_overrides() {
  let peer = Arc::new(RecordingApplication::successful_workflow());
  let router =
    management_router_with_application(|| true, recording_management_application(peer.clone(), peer.clone()));
  let mut request = manual_work_request();
  let overrides = serde_json::json!([{"stage":{"root":{"id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","version":1,"digest":"ab".repeat(32)},"calls":["nested_call"],"node":"implementation"},"profile":{"project_id":"11111111-1111-4111-8111-111111111111","id":"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb","version":2,"digest":"cd".repeat(32)}}]);
  request["profile_overrides"] = overrides.clone();
  let path = "/api/v1/projects/11111111-1111-4111-8111-111111111111/factory-work";
  let response = router
    .clone()
    .oneshot(json_request(
      "POST",
      path,
      "manual-override",
      None,
      &request.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::CREATED);
  assert_eq!(peer.factory_admission_overrides.lock().unwrap().as_slice(), [overrides]);
  request["profile_overrides"][0]["stage"]["node"] = serde_json::json!("*");
  let response = router
    .oneshot(json_request(
      "POST",
      path,
      "invalid-override",
      None,
      &request.to_string(),
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  assert_eq!(peer.factory_admission_overrides.lock().unwrap().len(), 1);
}
