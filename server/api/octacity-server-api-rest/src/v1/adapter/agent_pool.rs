use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AgentPoolListParameters {
  after: Option<Cursor>,
  #[serde(default = "default_page_limit")]
  limit: u16,
}

pub(super) async fn create_agent_pool(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  headers: HeaderMap,
  payload: Result<Json<CreateAgentPoolRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let command = application
    .inputs
    .create_agent_pool(
      Uuid::new_v4(),
      body.name,
      agent_pool_document(body.definition, &request_id)?,
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .agent_pools
    .create
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::CREATED, Json(agent_pool_mutation(outcome))))
}

pub(super) async fn publish_agent_pool(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(pool_id): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<PublishAgentPoolVersionRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let expected = precondition(&headers, &request_id)?;
  let command = application
    .inputs
    .publish_agent_pool(
      &pool_id,
      expected.version(),
      agent_pool_document(body.definition, &request_id)?,
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .agent_pools
    .publish
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(agent_pool_mutation(outcome))))
}

pub(super) async fn get_agent_pool(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path((pool_id, version)): Path<(String, u64)>,
) -> Result<impl IntoResponse, ApiError> {
  let query = application
    .inputs
    .get_agent_pool(&pool_id, version)
    .map_err(|error| invalid_input(error, &request_id))?;
  let projection = application
    .agent_pools
    .get
    .handle_query(query)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(agent_pool_resource(projection))))
}

pub(super) async fn list_agent_pools(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  parameters: Result<Query<AgentPoolListParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let Query(parameters) = parameters.map_err(|error| query_error(error, &request_id))?;
  let query = application
    .inputs
    .list_agent_pools(parameters.after.as_ref().map(Cursor::as_str), parameters.limit)
    .map_err(|error| invalid_input(error, &request_id))?;
  let page = application
    .agent_pools
    .list
    .handle_query(query)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(agent_pool_page(page, &request_id)?)))
}

pub(super) async fn delete_agent_pool(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(pool_id): Path<String>,
  headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
  let key = idempotency_key(&headers, &request_id)?;
  let expected = precondition(&headers, &request_id)?;
  let command = application
    .inputs
    .delete_agent_pool(&pool_id, expected.version(), key.as_str(), now_unix_ms(&request_id)?)
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .agent_pools
    .delete
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(delete_agent_pool_response(outcome))))
}
