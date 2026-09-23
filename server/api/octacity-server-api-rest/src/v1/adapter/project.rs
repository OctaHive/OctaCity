use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProjectListParameters {
  parent_id: Option<String>,
  after: Option<Cursor>,
  #[serde(default = "default_page_limit")]
  limit: u16,
}

pub(super) async fn create_project(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  headers: HeaderMap,
  payload: Result<Json<CreateProjectRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let idempotency_key = idempotency_key(&headers, &request_id)?;
  let command = application
    .inputs
    .create_project(
      Uuid::new_v4(),
      body.parent_id.as_deref(),
      body.name,
      idempotency_key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .projects
    .create
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::CREATED, Json(project_mutation(outcome))))
}

pub(super) async fn rename_project(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(project_id): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<RenameProjectRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let expected = precondition(&headers, &request_id)?;
  let command = application
    .inputs
    .rename_project(
      &project_id,
      expected.version(),
      body.name,
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .projects
    .rename
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(project_mutation(outcome))))
}

pub(super) async fn move_project(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(project_id): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<MoveProjectRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let expected = precondition(&headers, &request_id)?;
  let command = application
    .inputs
    .move_project(
      &project_id,
      expected.version(),
      body.parent_id.as_deref(),
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .projects
    .move_project
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(project_mutation(outcome))))
}

pub(super) async fn delete_project(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(project_id): Path<String>,
  headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
  let key = idempotency_key(&headers, &request_id)?;
  let expected = precondition(&headers, &request_id)?;
  let command = application
    .inputs
    .delete_project(&project_id, expected.version(), key.as_str(), now_unix_ms(&request_id)?)
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .projects
    .delete
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(delete_project_response(outcome))))
}

pub(super) async fn publish_project_policy(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(project_id): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<PublishProjectPolicyRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let expected = optional_precondition(&headers, &request_id)?.map(VersionPrecondition::version);
  let command = application
    .inputs
    .publish_project_policy(
      &project_id,
      expected,
      body.policy,
      key.as_str(),
      now_unix_ms(&request_id)?,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let outcome = application
    .definitions
    .publish_project_policy
    .handle_command(command)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((
    StatusCode::CREATED,
    Json(MutationResponse {
      disposition: mutation_disposition(outcome.disposition),
      resource: ProjectPolicyResource {
        project_id: outcome.project_id.to_string(),
        version: outcome.version.get(),
      },
    }),
  ))
}

pub(super) async fn get_project(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  Path(project_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
  let query = application
    .inputs
    .get_project(&project_id)
    .map_err(|error| invalid_input(error, &request_id))?;
  let projection = application
    .projects
    .get
    .handle_query(query)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(project_details(projection))))
}

pub(super) async fn list_projects(
  State(application): State<Arc<ManagementApplication>>,
  Extension(request_id): Extension<RequestId>,
  parameters: Result<Query<ProjectListParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let Query(parameters) = parameters.map_err(|error| query_error(error, &request_id))?;
  let query = application
    .inputs
    .list_projects(
      parameters.parent_id.as_deref(),
      parameters.after.as_ref().map(Cursor::as_str),
      parameters.limit,
    )
    .map_err(|error| invalid_input(error, &request_id))?;
  let page = application
    .projects
    .list
    .handle_query(query)
    .await
    .map_err(|error| application_error(error.classification(), &request_id))?;
  Ok((StatusCode::OK, Json(project_page(page, &request_id)?)))
}
