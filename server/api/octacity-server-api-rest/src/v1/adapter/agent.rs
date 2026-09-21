use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AgentListParameters {
  after: Option<Cursor>,
  #[serde(default = "default_page_limit")]
  limit: u16,
}

pub(super) async fn issue_agent_enrollment(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  headers: HeaderMap,
  payload: Result<Json<IssueAgentEnrollmentRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let IssueAgentEnrollmentRequest {
    pool_id,
    pool_version,
    expected_platform,
  } = body;
  let (operating_system, architecture) = expected_platform
    .map(|platform| (Some(platform.operating_system), Some(platform.architecture)))
    .unwrap_or((None, None));
  let command = application
    .inputs
    .issue_agent_enrollment(
      &pool_id,
      pool_version,
      operating_system,
      architecture,
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .agents
    .issue_enrollment
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  let credential = outcome.credential.encode().to_string();
  Ok((
    StatusCode::CREATED,
    Json(IssueAgentEnrollmentResponse {
      disposition: mutation_disposition(outcome.disposition),
      credential,
      pool_id: outcome.pool_id.to_string(),
      pool_version: outcome.pool_version.get(),
      expires_at_unix_ms: outcome.expires_at.unix_millis(),
    }),
  ))
}

pub(super) async fn get_agent(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(agent_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
  let query = application
    .inputs
    .get_agent(&agent_id)
    .map_err(|error| invalid_input(error, &request_id))?;
  let projection = application
    .agents
    .get
    .handle_query(query)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(agent_resource(projection, &request_id)?)))
}

pub(super) async fn list_agents(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  parameters: Result<Query<AgentListParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let Query(parameters) = parameters.map_err(|error| query_error(error, &request_id))?;
  let query = application
    .inputs
    .list_agents(parameters.after.as_ref().map(Cursor::as_str), parameters.limit)
    .map_err(|error| invalid_input(error, &request_id))?;
  let page = application
    .agents
    .list
    .handle_query(query)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(agent_page(page, &request_id)?)))
}

pub(super) async fn reassign_agent_pool(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(agent_id): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<ReassignAgentPoolRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let expected = precondition(&headers, &request_id)?;
  let command = application
    .inputs
    .reassign_agent_pool(
      &agent_id,
      expected.version(),
      &body.pool_id,
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .agents
    .reassign
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(agent_mutation(outcome, &request_id)?)))
}

pub(super) async fn drain_agent(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(agent_id): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<DrainAgentRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let expected = precondition(&headers, &request_id)?;
  let command = application
    .inputs
    .drain_agent(
      &agent_id,
      expected.version(),
      matches!(body.mode, AgentDrainMode::Forced),
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .agents
    .drain
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(agent_mutation(outcome, &request_id)?)))
}
