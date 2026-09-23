use super::*;

pub(super) async fn accept_manual_trigger(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  headers: HeaderMap,
  payload: Result<Json<AcceptManualTriggerRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  if key.as_str() != body.deduplication_identity {
    return Err(ApiError::new(
      StatusCode::CONFLICT,
      ErrorCode::IdempotencyConflict,
      "manual Trigger Idempotency-Key must equal deduplication_identity",
      &request_id,
    ));
  }
  let command = application
    .inputs
    .accept_manual_trigger(
      ManualTriggerInput {
        trigger_id: body.trigger_id,
        trigger_version: body.trigger_version,
        configuration_id: body.configuration_id,
        configuration_version: body.configuration_version,
        deduplication_identity: body.deduplication_identity,
        source: encode(body.source, &request_id)?,
        parameters: body.parameters,
        priority: body.priority,
      },
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .manual_triggers
    .0
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(trigger_response(outcome))))
}

pub(super) async fn create_manual_trigger_definition(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  headers: HeaderMap,
  payload: Result<Json<CreateManualTriggerDefinitionRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let command = application
    .inputs
    .create_manual_trigger_definition(
      ManualTriggerDefinitionInput {
        id: Uuid::new_v4(),
        configuration_id: body.configuration_id,
        configuration_version: body.configuration_version,
        enabled: body.enabled,
        definition: body.definition,
      },
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .definitions
    .create_trigger
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((
    StatusCode::CREATED,
    Json(MutationResponse {
      disposition: mutation_disposition(outcome.disposition),
      resource: TriggerDefinitionResource {
        id: outcome.trigger_id.to_string(),
        version: outcome.version.get(),
      },
    }),
  ))
}
