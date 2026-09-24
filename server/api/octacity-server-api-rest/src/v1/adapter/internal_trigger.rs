use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InternalTriggerListParameters {
  after: Option<Cursor>,
  #[serde(default = "default_page_limit")]
  limit: u16,
}

pub(super) async fn create_internal_trigger(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  headers: HeaderMap,
  payload: Result<Json<InternalTriggerDefinitionRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let request = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let command = application
    .inputs
    .create_internal_trigger_definition(
      Uuid::new_v4(),
      definition_input(request, &request_id)?,
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .internal_triggers
    .create
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((
    StatusCode::CREATED,
    Json(MutationResponse {
      disposition: mutation_disposition(outcome.disposition),
      resource: internal_trigger_resource(outcome.trigger),
    }),
  ))
}

pub(super) async fn publish_internal_trigger(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(trigger_id): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<InternalTriggerDefinitionRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let request = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let expected = precondition(&headers, &request_id)?;
  let command = application
    .inputs
    .publish_internal_trigger_version(
      &trigger_id,
      expected.version(),
      definition_input(request, &request_id)?,
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .internal_triggers
    .publish
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((
    StatusCode::OK,
    Json(MutationResponse {
      disposition: mutation_disposition(outcome.disposition),
      resource: internal_trigger_resource(outcome.trigger),
    }),
  ))
}

pub(super) async fn get_internal_trigger(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path((trigger_id, version)): Path<(String, u64)>,
) -> Result<impl IntoResponse, ApiError> {
  let query = application
    .inputs
    .get_internal_trigger(&trigger_id, version)
    .map_err(|error| invalid_input(error, &request_id))?;
  let projection = application
    .internal_triggers
    .get
    .handle_query(query)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(internal_trigger_resource(projection))))
}

pub(super) async fn list_internal_triggers(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  parameters: Result<Query<InternalTriggerListParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let Query(parameters) = parameters.map_err(|error| query_error(error, &request_id))?;
  let query = application
    .inputs
    .list_internal_triggers(parameters.after.as_ref().map(Cursor::as_str), parameters.limit)
    .map_err(|error| invalid_input(error, &request_id))?;
  let page = application
    .internal_triggers
    .list
    .handle_query(query)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(internal_trigger_page(page, &request_id)?)))
}

fn definition_input(
  request: InternalTriggerDefinitionRequest,
  request_id: &RequestId,
) -> Result<InternalTriggerDefinitionInput, ApiError> {
  let event = octacity_server_application::TerminalBuildEvent::from(request.outcome);
  Ok(InternalTriggerDefinitionInput {
    upstream_configuration_id: request.upstream_configuration_id,
    upstream_configuration_version: request.upstream_configuration_version,
    configuration_id: request.configuration_id,
    configuration_version: request.configuration_version,
    event_kind: event.as_str().to_owned(),
    source: encode(request.source, request_id)?,
    parameters: request.parameters,
    priority: request.priority,
    enabled: request.enabled,
  })
}

fn internal_trigger_page(
  page: InternalTriggerPageProjection,
  request_id: &RequestId,
) -> Result<CursorPage<InternalTriggerResource>, ApiError> {
  Ok(CursorPage {
    items: page.items.into_iter().map(internal_trigger_resource).collect(),
    next_cursor: page
      .next_after
      .map(|cursor| Cursor::new(cursor.to_string()))
      .transpose()
      .map_err(|_| internal_projection_error(request_id))?,
  })
}

fn internal_trigger_resource(projection: InternalTriggerProjection) -> InternalTriggerResource {
  let outcome = projection.definition.event_kind.into();
  let source = match projection.definition.source {
    ApplicationInternalTriggerSource::InheritRevision => InternalTriggerSource::InheritRevision,
    ApplicationInternalTriggerSource::ResolveTarget(source) => {
      InternalTriggerSource::ResolveTarget(decode_application_source(source))
    }
  };
  InternalTriggerResource {
    id: projection.trigger.id.to_string(),
    version: projection.trigger.version.get(),
    upstream_configuration_id: projection.definition.upstream.configuration_id.to_string(),
    upstream_configuration_version: projection.definition.upstream.configuration_version.get(),
    configuration_id: projection.target.configuration_id.to_string(),
    configuration_version: projection.target.configuration_version.get(),
    outcome,
    source,
    parameters: projection.definition.parameters,
    priority: projection.definition.priority,
    enabled: projection.enabled,
    created_at_unix_ms: projection.created_at.unix_millis(),
  }
}

impl From<InternalTriggerOutcome> for octacity_server_application::TerminalBuildEvent {
  fn from(value: InternalTriggerOutcome) -> Self {
    match value {
      InternalTriggerOutcome::Succeeded => Self::Succeeded,
      InternalTriggerOutcome::Failed => Self::Failed,
      InternalTriggerOutcome::Cancelled => Self::Cancelled,
    }
  }
}

impl From<octacity_server_application::TerminalBuildEvent> for InternalTriggerOutcome {
  fn from(value: octacity_server_application::TerminalBuildEvent) -> Self {
    match value {
      octacity_server_application::TerminalBuildEvent::Succeeded => Self::Succeeded,
      octacity_server_application::TerminalBuildEvent::Failed => Self::Failed,
      octacity_server_application::TerminalBuildEvent::Cancelled => Self::Cancelled,
    }
  }
}

fn decode_application_source(source: octacity_server_application::ManualSourceSelection) -> ManualSource {
  match source {
    octacity_server_application::ManualSourceSelection::DefaultReference => ManualSource::DefaultReference,
    octacity_server_application::ManualSourceSelection::Reference(reference) => {
      ManualSource::Reference(reference.to_string())
    }
    octacity_server_application::ManualSourceSelection::ExactRevision(revision) => {
      ManualSource::ExactRevision(revision.to_string())
    }
  }
}

fn internal_projection_error(request_id: &RequestId) -> ApiError {
  ApiError::new(
    StatusCode::INTERNAL_SERVER_ERROR,
    ErrorCode::Internal,
    "internal Trigger state cannot be represented safely",
    request_id,
  )
}
