use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CurrentDefinitionListParameters {
  after: Option<Cursor>,
  #[serde(default = "default_page_limit")]
  limit: u16,
}

async fn current_definition_page<Q, P, H>(
  request_id: &RequestId,
  context: &octacity_server_application::ManagementRequestContext,
  query: Result<Q, ManagementInputError>,
  handler: &H,
  map_page: impl FnOnce(Q::Outcome, &RequestId) -> Result<P, ApiError>,
) -> Result<(StatusCode, Json<P>), ApiError>
where
  Q: octacity_server_application::Query + 'static,
  P: Serialize,
  H: octacity_server_application::AuthorizedManagementQueryHandler<
      Q,
      Error = octacity_server_application::ApplicationError,
    > + ?Sized,
{
  let query = query.map_err(|error| invalid_input(error, request_id))?;
  let page = handler
    .handle_authorized_query(context, query)
    .await
    .map_err(|error| authorized_handler_error(error, request_id))?;
  Ok((StatusCode::OK, Json(map_page(page, request_id)?)))
}

pub(super) async fn list_project_pipelines(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(project_id): Path<String>,
  parameters: Result<Query<CurrentDefinitionListParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let Query(parameters) = parameters.map_err(|error| query_error(error, &request_id))?;
  current_definition_page(
    &request_id,
    &context,
    application.inputs.list_project_pipelines(
      &project_id,
      parameters.after.as_ref().map(Cursor::as_str),
      parameters.limit,
    ),
    application.pipelines.list.as_ref(),
    pipeline_summary_page,
  )
  .await
}

pub(super) async fn list_project_repositories(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(project_id): Path<String>,
  parameters: Result<Query<CurrentDefinitionListParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let Query(parameters) = parameters.map_err(|error| query_error(error, &request_id))?;
  current_definition_page(
    &request_id,
    &context,
    application.inputs.list_project_repositories(
      &project_id,
      parameters.after.as_ref().map(Cursor::as_str),
      parameters.limit,
    ),
    application.configurations.list_repositories.as_ref(),
    repository_summary_page,
  )
  .await
}

pub(super) async fn list_project_build_configurations(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(project_id): Path<String>,
  parameters: Result<Query<CurrentDefinitionListParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let Query(parameters) = parameters.map_err(|error| query_error(error, &request_id))?;
  current_definition_page(
    &request_id,
    &context,
    application.inputs.list_project_build_configurations(
      &project_id,
      parameters.after.as_ref().map(Cursor::as_str),
      parameters.limit,
    ),
    application.configurations.list_configurations.as_ref(),
    build_configuration_summary_page,
  )
  .await
}

pub(super) async fn list_project_trigger_definitions(
  State(application): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(project_id): Path<String>,
  parameters: Result<Query<CurrentDefinitionListParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let Query(parameters) = parameters.map_err(|error| query_error(error, &request_id))?;
  current_definition_page(
    &request_id,
    &context,
    application.inputs.list_project_trigger_definitions(
      &project_id,
      parameters.after.as_ref().map(Cursor::as_str),
      parameters.limit,
    ),
    application.definitions.list_triggers.as_ref(),
    trigger_definition_summary_page,
  )
  .await
}
