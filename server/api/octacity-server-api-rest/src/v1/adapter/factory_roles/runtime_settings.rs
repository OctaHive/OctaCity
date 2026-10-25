//! Optional Runtime transport facade over the same Project role/profile handlers.
use super::*;
use octacity_server_application::{
  GetFactoryRuntimeSettingsQuery, ListFactoryRuntimeChoicesQuery, PublishFactoryRuntimeSettingsCommand,
  ValidateFactoryRuntimeSettingsQuery,
};

pub(super) struct RuntimeSettingsApplication {
  read: Arc<dyn AuthorizedManagementQueryHandler<GetFactoryRuntimeSettingsQuery, Error = ApplicationError>>,
  validate: Arc<dyn AuthorizedManagementQueryHandler<ValidateFactoryRuntimeSettingsQuery, Error = ApplicationError>>,
  choices: Arc<dyn AuthorizedManagementQueryHandler<ListFactoryRuntimeChoicesQuery, Error = ApplicationError>>,
  publish: Arc<dyn AuthorizedManagementCommandHandler<PublishFactoryRuntimeSettingsCommand, Error = ApplicationError>>,
}
impl FactoryRoleManagementApplication {
  /// Adds Runtime administration without making ordinary AI-agent authoring depend on it.
  pub fn with_runtime_settings<C, Q>(mut self, commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<PublishFactoryRuntimeSettingsCommand, Error = ApplicationError> + 'static,
    Q: AuthorizedManagementQueryHandler<GetFactoryRuntimeSettingsQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ValidateFactoryRuntimeSettingsQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListFactoryRuntimeChoicesQuery, Error = ApplicationError>
      + 'static,
  {
    self.runtime_settings = Some(RuntimeSettingsApplication {
      read: queries.clone(),
      choices: queries.clone(),
      validate: queries,
      publish: commands,
    });
    self
  }
}
fn runtime_endpoints<'a>(
  app: &'a ManagementApplication,
  id: &RequestId,
) -> Result<&'a RuntimeSettingsApplication, ApiError> {
  endpoints(app, id)?
    .runtime_settings
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, id))
}
async fn settings_response(
  app: &ManagementApplication,
  context: &octacity_server_application::ManagementRequestContext,
  project: &str,
  alias: &str,
  version: Option<u64>,
  id: &RequestId,
) -> Result<axum::response::Response, ApiError> {
  let query = app
    .inputs
    .get_factory_runtime_settings(project, alias, version)
    .map_err(|e| invalid_input(e, id))?;
  let settings = runtime_endpoints(app, id)?
    .read
    .handle_authorized_query(context, query)
    .await
    .map_err(|e| authorized_handler_error(e, id))?;
  if version.is_some() && settings.is_none() {
    return Err(application_error(ApplicationFailure::NotFound, id));
  }
  Ok(
    (
      [("cache-control", "no-store")],
      Json(serde_json::json!({"settings":encode(settings,id)?})),
    )
      .into_response(),
  )
}
pub(crate) async fn current(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, alias)): Path<(String, String)>,
) -> Result<axum::response::Response, ApiError> {
  settings_response(&app, &context, &project, &alias, None, &id).await
}
pub(crate) async fn version(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, alias, version)): Path<(String, String, u64)>,
) -> Result<axum::response::Response, ApiError> {
  settings_response(&app, &context, &project, &alias, Some(version), &id).await
}
pub(crate) async fn publish(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, alias)): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<crate::v1::FactoryRuntimeSettingsRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &id)?;
  let key = idempotency_key(&headers, &id)?;
  let command = app
    .inputs
    .publish_factory_runtime_settings(
      &project,
      &alias,
      body.expected_version,
      body.definition,
      key.as_str(),
      now_unix_ms(&id)?,
    )
    .map_err(|e| invalid_input(e, &id))?;
  let saved = runtime_endpoints(&app, &id)?
    .publish
    .handle_authorized_command(&context, command)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(saved, &id)?)))
}

pub(crate) async fn validate(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, alias)): Path<(String, String)>,
  payload: Result<Json<crate::v1::FactoryRuntimeSettingsRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &id)?;
  let query = app
    .inputs
    .validate_factory_runtime_settings(&project, &alias, body.expected_version, body.definition)
    .map_err(|e| invalid_input(e, &id))?;
  let settings = runtime_endpoints(&app, &id)?
    .validate
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok((
    [("cache-control", "no-store")],
    Json(serde_json::json!({"settings":encode(settings, &id)?})),
  ))
}

pub(crate) async fn choices(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  parameters: Result<Query<PageParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let Query(p) = parameters.map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let page = app
    .inputs
    .factory_role_page(&project, p.after.as_deref(), p.limit.unwrap_or(DEFAULT_PAGE_LIMIT))
    .map_err(|e| invalid_input(e, &id))?;
  read(
    &runtime_endpoints(&app, &id)?.choices,
    &context,
    ListFactoryRuntimeChoicesQuery { page },
    &id,
  )
  .await
}
