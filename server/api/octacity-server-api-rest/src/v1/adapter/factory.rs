use super::*;
pub(super) async fn confirm_factory_node_operation(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path((run, node)): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<crate::v1::FactoryNodeOperationRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let expected = precondition(&headers, &request_id)?;
  let command = application
    .inputs
    .confirm_factory_node_operation(
      octacity_server_application::FactoryNodeControlInput {
        run_id: &run,
        node_attempt_id: &node,
        expected_version: expected.version(),
        idempotency_key: key.as_str(),
        requested_at: now_unix_ms(&request_id)?,
      },
      &body.operation,
      &body.disposition,
      &body.reason,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .factory
    .confirm_node
    .handle_authorized_command(&context, command)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  factory_control_response(outcome, &request_id)
}
pub(super) async fn retry_factory_node(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path((run, node)): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<FactoryConfirmationRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let _ = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let expected = precondition(&headers, &request_id)?;
  let command = application
    .inputs
    .retry_factory_node(octacity_server_application::FactoryNodeControlInput {
      run_id: &run,
      node_attempt_id: &node,
      expected_version: expected.version(),
      idempotency_key: key.as_str(),
      requested_at: now_unix_ms(&request_id)?,
    })
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .factory
    .retry_node
    .handle_authorized_command(&context, command)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  factory_control_response(outcome, &request_id)
}
pub(super) async fn submit_factory_node_response(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path((run, node)): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<crate::v1::FactoryNodeResponseRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let expected = precondition(&headers, &request_id)?;
  let command = application
    .inputs
    .submit_factory_node_response(
      octacity_server_application::FactoryNodeControlInput {
        run_id: &run,
        node_attempt_id: &node,
        expected_version: expected.version(),
        idempotency_key: key.as_str(),
        requested_at: now_unix_ms(&request_id)?,
      },
      &body.outcome,
      body.value,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .factory
    .respond_node
    .handle_authorized_command(&context, command)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  factory_control_response(outcome, &request_id)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FactoryConfirmationRequest {}

pub(super) async fn cancel_factory_run(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(id): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<FactoryConfirmationRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let _ = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let expected = precondition(&headers, &request_id)?;
  let command = application
    .inputs
    .cancel_factory_run(&id, expected.version(), key.as_str(), now_unix_ms(&request_id)?)
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .factory
    .cancel_run
    .handle_authorized_command(&context, command)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  factory_control_response(outcome, &request_id)
}

fn factory_control_response(
  outcome: octacity_server_application::FactoryRunControlCommandOutcome,
  request_id: &RequestId,
) -> Result<axum::response::Response, ApiError> {
  let etag = VersionPrecondition::new(outcome.version.get()).map_err(|_| internal_conversion(request_id))?;
  Ok(
    (
      [(axum::http::header::ETAG, etag.to_header_value())],
      Json(crate::v1::FactoryRunControlResponse {
        disposition: mutation_disposition(outcome.disposition),
        run_id: outcome.run_id.to_string(),
        version: outcome.version.get(),
        state: outcome.state.as_str().to_owned(),
        cancellation_requested: outcome.cancellation_requested,
      }),
    )
      .into_response(),
  )
}
use crate::v1::{
  CreateFactoryConfigurationRequest, FactoryConfigurationDetails, FactoryConfigurationMutationResponse,
  ReplaceFactoryConfigurationRequest,
};
use crate::v1::{
  FactoryAdmissionResponse, FactoryFlowDefinitionDetails, FactoryFlowDefinitionMutationResponse,
  FactoryFlowDefinitionPage, FactoryPhaseReadyPage, FactoryPhaseReadyResource, FactoryRunSummaryPage,
  FactoryRunSummaryResource,
};
use crate::v1::{FactoryConfigurationSummaryPage, FactoryConfigurationSummaryResource};

pub(super) async fn create_factory_configuration(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  headers: HeaderMap,
  payload: Result<Json<CreateFactoryConfigurationRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let id = Uuid::parse_str(&body.id)
    .map_err(|_| invalid_input(ManagementInputError::Invalid("factory configuration id"), &request_id))?;
  let command = application
    .inputs
    .create_factory_configuration(
      id,
      &body.project_id,
      body.definition,
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .factory
    .create_configuration
    .handle_authorized_command(&context, command)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok((
    StatusCode::CREATED,
    Json(factory_configuration_mutation(outcome, &request_id)?),
  ))
}

pub(super) async fn replace_factory_configuration(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(id): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<ReplaceFactoryConfigurationRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let expected = precondition(&headers, &request_id)?;
  let command = application
    .inputs
    .replace_factory_configuration(
      &id,
      expected.version(),
      body.definition,
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .factory
    .replace_configuration
    .handle_authorized_command(&context, command)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok((
    StatusCode::CREATED,
    Json(factory_configuration_mutation(outcome, &request_id)?),
  ))
}

fn factory_configuration_mutation(
  outcome: octacity_server_application::FactoryConfigurationCommandOutcome,
  request_id: &RequestId,
) -> Result<FactoryConfigurationMutationResponse, ApiError> {
  Ok(FactoryConfigurationMutationResponse {
    disposition: mutation_disposition(outcome.disposition),
    resource: factory_configuration_resource(outcome.configuration, request_id)?,
  })
}
use crate::v1::{
  FactoryBuildConfigurationReferenceResource, FactoryChoiceReferenceResource, FactoryConfigurationChoicePage,
  FactoryConfigurationChoiceResource, FactoryImmutableReferenceResource,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FactoryConfigurationChoiceParameters {
  kind: Option<String>,
  after: Option<Cursor>,
  #[serde(default = "default_page_limit")]
  limit: u16,
}

pub(super) async fn list_factory_configuration_choices(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(project_id): Path<String>,
  parameters: Result<Query<FactoryConfigurationChoiceParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let Query(parameters) = parameters.map_err(|error| query_error(error, &request_id))?;
  let query = application
    .inputs
    .list_factory_configuration_choices(
      &project_id,
      parameters.kind.as_deref(),
      parameters.after.as_ref().map(Cursor::as_str),
      parameters.limit,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let page = application
    .factory
    .list_choices
    .handle_authorized_query(&context, query)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok(Json(FactoryConfigurationChoicePage {
    total: page.total,
    items: page
      .items
      .into_iter()
      .map(|item| FactoryConfigurationChoiceResource {
        metadata: item.metadata,
        identity: item.identity.to_string(),
        kind: item.identity.kind.as_str().to_owned(),
        alias: item.identity.alias.to_string(),
        reference: match item.reference {
          octacity_server_application::FactoryChoiceReferenceProjection::Immutable(reference) => {
            FactoryChoiceReferenceResource::Immutable(FactoryImmutableReferenceResource {
              identity: reference.identity().to_string(),
              version: reference.version().to_string(),
              digest: reference.digest().to_string(),
            })
          }
          octacity_server_application::FactoryChoiceReferenceProjection::BuildConfiguration(reference) => {
            FactoryChoiceReferenceResource::BuildConfiguration(FactoryBuildConfigurationReferenceResource {
              id: reference.id().to_string(),
              version: reference.version().get(),
              project_id: reference.project_id().to_string(),
              definition_digest: reference.definition_digest().to_string(),
            })
          }
        },
      })
      .collect(),
    next_cursor: page
      .next_cursor
      .map(|cursor| Cursor::new(cursor.to_string()))
      .transpose()
      .map_err(|_| internal_conversion(&request_id))?,
  }))
}

pub(super) async fn get_factory_configuration(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path((id, version)): Path<(String, u64)>,
) -> Result<impl IntoResponse, ApiError> {
  let query = application
    .inputs
    .get_factory_configuration(&id, version)
    .map_err(|error| invalid_input(error, &request_id))?;
  let projection = application
    .factory
    .get_configuration
    .handle_authorized_query(&context, query)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  factory_configuration_details(projection, &request_id)
}

pub(super) async fn get_current_factory_configuration(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
  let query = application
    .inputs
    .get_current_factory_configuration(&id)
    .map_err(|error| invalid_input(error, &request_id))?;
  let projection = application
    .factory
    .get_current_configuration
    .handle_authorized_query(&context, query)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  factory_configuration_details(projection, &request_id)
}

fn factory_configuration_details(
  projection: octacity_server_application::FactoryConfigurationProjection,
  request_id: &RequestId,
) -> Result<Json<FactoryConfigurationDetails>, ApiError> {
  factory_configuration_resource(projection, request_id).map(Json)
}

fn factory_configuration_resource(
  projection: octacity_server_application::FactoryConfigurationProjection,
  request_id: &RequestId,
) -> Result<FactoryConfigurationDetails, ApiError> {
  Ok(FactoryConfigurationDetails {
    configuration: encode(projection.configuration, request_id)?,
    published_at_unix_ms: projection.published_at.unix_millis(),
  })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FactoryConfigurationListParameters {
  after: Option<Cursor>,
  #[serde(default = "default_page_limit")]
  limit: u16,
}

pub(super) async fn list_project_factory_configurations(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(project_id): Path<String>,
  parameters: Result<Query<FactoryConfigurationListParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let Query(parameters) = parameters.map_err(|error| query_error(error, &request_id))?;
  let query = application
    .inputs
    .list_project_factory_configurations(
      &project_id,
      parameters.after.as_ref().map(Cursor::as_str),
      parameters.limit,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let page = application
    .factory
    .list_configurations
    .handle_authorized_query(&context, query)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok(Json(FactoryConfigurationSummaryPage {
    total: page.total,
    items: page
      .items
      .into_iter()
      .map(|item| FactoryConfigurationSummaryResource {
        id: item.id.to_string(),
        project_id: item.project_id.to_string(),
        version: item.version.get(),
        enabled: item.enabled,
        created_at_unix_ms: item.created_at.unix_millis(),
        published_at_unix_ms: item.published_at.unix_millis(),
      })
      .collect(),
    next_cursor: page
      .next_cursor
      .map(|position| {
        Cursor::new(format!(
          "1:{}:{}",
          position.created_at.unix_millis(),
          position.configuration_id
        ))
      })
      .transpose()
      .map_err(|_| internal_conversion(&request_id))?,
  }))
}

pub(super) async fn validate_factory_configuration(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  payload: Result<Json<CreateFactoryConfigurationRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let query = application
    .inputs
    .validate_factory_configuration(&body.id, &body.project_id, body.definition)
    .map_err(|error| invalid_input(error, &request_id))?;
  let validation = application
    .factory
    .validate_configuration
    .handle_authorized_query(&context, query)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok(Json(
    serde_json::json!({"configuration": encode(validation.configuration, &request_id)?, "diagnostics": encode(validation.diagnostics, &request_id)?}),
  ))
}

pub(super) async fn validate_factory_flow_definition(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  payload: Result<Json<crate::v1::CreateFactoryFlowDefinitionRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let query = application
    .inputs
    .validate_factory_flow_definition(&body.id, &body.project_id, body.definition)
    .map_err(|error| invalid_input(error, &request_id))?;
  let flow = application
    .factory
    .validate_flow
    .handle_authorized_query(&context, query)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok(Json(encode(flow, &request_id)?))
}

fn flow_details(
  publication: octacity_server_application::PublishedFactoryFlowDefinition,
  request_id: &RequestId,
) -> Result<FactoryFlowDefinitionDetails, ApiError> {
  Ok(FactoryFlowDefinitionDetails {
    project_id: publication.project_id.to_string(),
    flow: encode(publication.flow, request_id)?,
    published_at_unix_ms: publication.published_at.unix_millis(),
  })
}
fn flow_mutation(
  outcome: octacity_server_application::FactoryFlowDefinitionCommandOutcome,
  request_id: &RequestId,
) -> Result<FactoryFlowDefinitionMutationResponse, ApiError> {
  Ok(FactoryFlowDefinitionMutationResponse {
    disposition: mutation_disposition(outcome.disposition.into()),
    resource: flow_details(outcome.publication, request_id)?,
  })
}
pub(super) async fn create_factory_flow_definition(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  headers: HeaderMap,
  payload: Result<Json<crate::v1::CreateFactoryFlowDefinitionRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let command = application
    .inputs
    .create_factory_flow_definition(
      &body.id,
      &body.project_id,
      body.definition,
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .factory
    .create_flow
    .handle_authorized_command(&context, command)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok((StatusCode::CREATED, Json(flow_mutation(outcome, &request_id)?)))
}
pub(super) async fn replace_factory_flow_definition(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(id): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<crate::v1::ReplaceFactoryFlowDefinitionRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let expected = precondition(&headers, &request_id)?;
  let command = application
    .inputs
    .replace_factory_flow_definition(
      &id,
      expected.version(),
      body.definition,
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .factory
    .replace_flow
    .handle_authorized_command(&context, command)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok((StatusCode::CREATED, Json(flow_mutation(outcome, &request_id)?)))
}
pub(super) async fn get_current_factory_flow_definition(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
  get_flow(&application, &context, &request_id, &id, None).await
}
pub(super) async fn get_factory_flow_definition(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path((id, version)): Path<(String, u64)>,
) -> Result<impl IntoResponse, ApiError> {
  get_flow(&application, &context, &request_id, &id, Some(version)).await
}
async fn get_flow(
  application: &ManagementApplication,
  context: &octacity_server_application::ManagementRequestContext,
  request_id: &RequestId,
  id: &str,
  version: Option<u64>,
) -> Result<Json<FactoryFlowDefinitionDetails>, ApiError> {
  let query = application
    .inputs
    .get_factory_flow_definition(id, version)
    .map_err(|error| invalid_input(error, request_id))?;
  let outcome = application
    .factory
    .get_flow
    .handle_authorized_query(context, query)
    .await
    .map_err(|error| authorized_handler_error(error, request_id))?;
  Ok(Json(flow_details(outcome, request_id)?))
}
pub(super) async fn list_project_factory_flow_definitions(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  parameters: Result<Query<FactoryConfigurationListParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let Query(parameters) = parameters.map_err(|error| query_error(error, &request_id))?;
  let query = application
    .inputs
    .list_project_factory_flow_definitions(
      &project,
      parameters.after.as_ref().map(Cursor::as_str),
      parameters.limit,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let page = application
    .factory
    .list_flows
    .handle_authorized_query(&context, query)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  let items = page
    .items
    .into_iter()
    .map(|publication| flow_details(publication, &request_id))
    .collect::<Result<Vec<_>, _>>()?;
  let cursor = page
    .next_cursor
    .map(|p| Cursor::new(format!("1:{}:{}", p.created_at.unix_millis(), p.id)))
    .transpose()
    .map_err(|_| internal_conversion(&request_id))?;
  Ok(Json(FactoryFlowDefinitionPage {
    total: page.total,
    items,
    next_cursor: cursor,
  }))
}

pub(super) async fn admit_manual_factory_work(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(project_id): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<crate::v1::AdmitManualFactoryWorkRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let command = application
    .inputs
    .admit_manual_factory_work(
      octacity_server_application::ManualFactoryWorkInput {
        project_id,
        configuration_id: body.configuration_id,
        configuration_version: body.configuration_version,
        repository_id: body.repository_id,
        repository_version: body.repository_version,
        external_identity: body.external_identity,
        source: body.source,
        artifacts: body.artifacts,
        classification: body.classification,
      },
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .factory
    .admit_work
    .handle_authorized_command(&context, command)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok((
    StatusCode::CREATED,
    Json(FactoryAdmissionResponse {
      disposition: mutation_disposition(outcome.disposition),
      work_envelope_id: outcome.work_envelope_id.to_string(),
      factory_run_id: outcome.factory_run_id.to_string(),
      base_revision: outcome.base_revision.as_str().to_owned(),
      state: outcome.state.as_str().to_owned(),
      admitted_at_unix_ms: outcome.admitted_at.unix_millis(),
    }),
  ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FactoryRunListParameters {
  configuration_id: Option<String>,
  source: Option<String>,
  state: Option<String>,
  decision: Option<String>,
  delivery_state: Option<String>,
  admitted_from_unix_ms: Option<i64>,
  admitted_before_unix_ms: Option<i64>,
  after: Option<Cursor>,
  #[serde(default = "default_page_limit")]
  limit: u16,
}
pub(super) async fn list_factory_runs(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  parameters: Result<Query<FactoryRunListParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  list_run_page(&application, &context, &request_id, None, parameters).await
}
pub(super) async fn list_project_factory_runs(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  parameters: Result<Query<FactoryRunListParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  list_run_page(&application, &context, &request_id, Some(project), parameters).await
}
async fn list_run_page(
  application: &ManagementApplication,
  context: &octacity_server_application::ManagementRequestContext,
  request_id: &RequestId,
  project_id: Option<String>,
  parameters: Result<Query<FactoryRunListParameters>, QueryRejection>,
) -> Result<Json<FactoryRunSummaryPage>, ApiError> {
  let Query(p) = parameters.map_err(|error| query_error(error, request_id))?;
  let query = application
    .inputs
    .list_factory_runs(octacity_server_application::FactoryRunListInput {
      project_id,
      configuration_id: p.configuration_id,
      source: p.source,
      state: p.state,
      decision: p.decision,
      delivery_state: p.delivery_state,
      admitted_from_unix_ms: p.admitted_from_unix_ms,
      admitted_before_unix_ms: p.admitted_before_unix_ms,
      after: p.after.map(|cursor| cursor.as_str().to_owned()),
      limit: p.limit,
    })
    .map_err(|error| invalid_input(error, request_id))?;
  let page = application
    .factory
    .list_runs
    .handle_authorized_query(context, query)
    .await
    .map_err(|error| authorized_handler_error(error, request_id))?;
  let items = page
    .items
    .into_iter()
    .map(|item| run_summary_resource(item, request_id))
    .collect::<Result<Vec<_>, ApiError>>()?;
  let next_cursor = page
    .next_cursor
    .map(|position| Cursor::new(format!("1:{}:{}", position.admitted_at.unix_millis(), position.run_id)))
    .transpose()
    .map_err(|_| internal_conversion(request_id))?;
  Ok(Json(FactoryRunSummaryPage {
    total: page.total,
    items,
    next_cursor,
  }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FactoryPhaseReadyParameters {
  policy_digest: String,
  after: Option<Cursor>,
  #[serde(default = "default_page_limit")]
  limit: u16,
}
pub(super) async fn list_factory_phase_ready(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  parameters: Result<Query<FactoryPhaseReadyParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  list_phase_page(&application, &context, &request_id, None, parameters).await
}
pub(super) async fn list_project_factory_phase_ready(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  parameters: Result<Query<FactoryPhaseReadyParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  list_phase_page(&application, &context, &request_id, Some(project), parameters).await
}
async fn list_phase_page(
  application: &ManagementApplication,
  context: &octacity_server_application::ManagementRequestContext,
  request_id: &RequestId,
  project: Option<String>,
  parameters: Result<Query<FactoryPhaseReadyParameters>, QueryRejection>,
) -> Result<Json<FactoryPhaseReadyPage>, ApiError> {
  let Query(p) = parameters.map_err(|error| query_error(error, request_id))?;
  let query = application
    .inputs
    .list_factory_phase_ready(
      project.as_deref(),
      &p.policy_digest,
      p.after.as_ref().map(Cursor::as_str),
      p.limit,
    )
    .map_err(|error| invalid_input(error, request_id))?;
  let page = application
    .factory
    .list_phase_ready
    .handle_authorized_query(context, query)
    .await
    .map_err(|error| authorized_handler_error(error, request_id))?;
  let items = page
    .items
    .into_iter()
    .map(|entry| FactoryPhaseReadyResource {
      id: entry.id.to_string(),
      policy_digest: entry.policy_digest.to_string(),
      work_id: entry.work_id.to_string(),
      project_id: entry.project_id.to_string(),
      run_id: entry.input.run_id.to_string(),
      run_version: entry.input.run_version.get(),
      flow_run_id: entry.input.flow_run_id.to_string(),
      cycle_id: entry.input.cycle_id.to_string(),
      node: entry.input.node.to_string(),
      generation: entry.input.generation,
      severity: entry.input.severity.as_str().to_owned(),
      project_priority: entry.input.project_priority,
      admitted_at_unix_ms: entry.admitted_at.unix_millis(),
    })
    .collect();
  let cursor = page
    .next_cursor
    .map(|p| Cursor::new(p.to_string()))
    .transpose()
    .map_err(|_| internal_conversion(request_id))?;
  Ok(Json(FactoryPhaseReadyPage {
    total: page.total,
    items,
    next_cursor: cursor,
  }))
}

pub(super) async fn get_factory_run(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
  let query = application
    .inputs
    .get_factory_run(&id)
    .map_err(|error| invalid_input(error, &request_id))?;
  let item = application
    .factory
    .get_run
    .handle_authorized_query(&context, query)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  let etag = VersionPrecondition::new(item.version.get()).map_err(|_| internal_conversion(&request_id))?;
  Ok((
    [(axum::http::header::ETAG, etag.to_header_value())],
    Json(run_summary_resource(item, &request_id)?),
  ))
}
fn run_summary_resource(
  item: octacity_server_application::FactoryRunSummaryProjection,
  request_id: &RequestId,
) -> Result<FactoryRunSummaryResource, ApiError> {
  Ok(FactoryRunSummaryResource {
    diagnostics: encode(item.diagnostics, request_id)?,
    id: item.id.to_string(),
    project_id: item.project_id.to_string(),
    work_id: item.work_id.to_string(),
    configuration_id: item.configuration_id.to_string(),
    configuration_version: item.configuration_version.get(),
    source: item.source.to_string(),
    external_identity: item.external_identity.to_string(),
    state: item.state.as_str().to_owned(),
    version: item.version.get(),
    admitted_at_unix_ms: item.admitted_at.unix_millis(),
    updated_at_unix_ms: item.updated_at.unix_millis(),
  })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FactoryDiagnosticParameters {
  after: Option<Cursor>,
  #[serde(default = "default_page_limit")]
  limit: u16,
}
pub(super) async fn list_factory_diagnostics(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path((run, collection)): Path<(String, String)>,
  parameters: Result<Query<FactoryDiagnosticParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let Query(parameters) = parameters.map_err(|error| query_error(error, &request_id))?;
  let query = application
    .inputs
    .list_factory_diagnostics(
      &run,
      &collection,
      parameters.after.as_ref().map(Cursor::as_str),
      parameters.limit,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let page = application
    .factory
    .diagnostics
    .handle_authorized_query(&context, query)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok(Json(encode(page, &request_id)?))
}
