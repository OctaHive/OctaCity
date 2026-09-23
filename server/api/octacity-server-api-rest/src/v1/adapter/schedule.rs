use super::*;

pub(super) async fn create_scheduled_trigger_definition(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  headers: HeaderMap,
  payload: Result<Json<CreateScheduledTriggerDefinitionRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let command = application
    .inputs
    .create_scheduled_trigger_definition(
      ScheduledTriggerDefinitionInput {
        id: Uuid::new_v4(),
        configuration_id: body.configuration_id,
        configuration_version: body.configuration_version,
        enabled: body.enabled,
        schedule: encode(body.schedule, &request_id)?,
        build: encode(body.build, &request_id)?,
      },
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .schedules
    .create
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

pub(super) async fn get_schedule(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path((trigger_id, version)): Path<(String, u64)>,
) -> Result<impl IntoResponse, ApiError> {
  let query = application
    .inputs
    .get_schedule(&trigger_id, version)
    .map_err(|error| invalid_input(error, &request_id))?;
  let projection = application
    .schedules
    .get
    .handle_query(query)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(schedule_resource(projection, &request_id)?)))
}
