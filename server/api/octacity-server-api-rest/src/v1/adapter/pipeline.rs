use super::*;

pub(super) async fn create_pipeline(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  headers: HeaderMap,
  payload: Result<Json<CreatePipelineRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let command = application
    .inputs
    .create_pipeline(
      Uuid::new_v4(),
      &body.project_id,
      body.name,
      pipeline_document(body.dag),
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .pipelines
    .create
    .handle_authorized_command(&context, command)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok((StatusCode::CREATED, Json(pipeline_mutation(outcome))))
}

pub(super) async fn publish_pipeline(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(pipeline_id): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<PublishPipelineVersionRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let expected = precondition(&headers, &request_id)?;
  let command = application
    .inputs
    .publish_pipeline(
      &pipeline_id,
      expected.version(),
      pipeline_document(body.dag),
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .pipelines
    .publish
    .handle_authorized_command(&context, command)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok((StatusCode::OK, Json(pipeline_mutation(outcome))))
}

pub(super) async fn get_pipeline(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path((pipeline_id, version)): Path<(String, u64)>,
) -> Result<impl IntoResponse, ApiError> {
  let query = application
    .inputs
    .get_pipeline(&pipeline_id, version)
    .map_err(|error| invalid_input(error, &request_id))?;
  let projection = application
    .pipelines
    .get
    .handle_authorized_query(&context, query)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok((StatusCode::OK, Json(pipeline_resource(projection))))
}
