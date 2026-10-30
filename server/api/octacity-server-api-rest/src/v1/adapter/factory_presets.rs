//! Preset discovery and resolution remain authorized queries, never publication commands.
use super::*;
use octacity_server_application::{
  ApplicationError, AuthorizedManagementQueryHandler, FactoryPresetResolutionRequest, ListFactoryPresetsQuery,
  ResolveFactoryPresetQuery,
};

/// Installed shared preset setup queries, independent of execution/provider availability.
pub struct FactoryPresetManagementApplication {
  list: Arc<dyn AuthorizedManagementQueryHandler<ListFactoryPresetsQuery, Error = ApplicationError>>,
  resolve: Arc<dyn AuthorizedManagementQueryHandler<ResolveFactoryPresetQuery, Error = ApplicationError>>,
}
impl FactoryPresetManagementApplication {
  /// Erases the common application owner behind authorized management queries.
  pub fn new<
    Q: AuthorizedManagementQueryHandler<ListFactoryPresetsQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ResolveFactoryPresetQuery, Error = ApplicationError>
      + 'static,
  >(
    queries: Arc<Q>,
  ) -> Self {
    Self {
      list: queries.clone(),
      resolve: queries,
    }
  }
}
pub(super) async fn resolve(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  payload: Result<Json<FactoryPresetResolutionRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let selection = body(payload, &id)?;
  let project_id = project
    .parse()
    .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let service = app
    .factory_presets
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  let result = service
    .resolve
    .handle_authorized_query(&context, ResolveFactoryPresetQuery { project_id, selection })
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok((
    [("cache-control", "no-store")],
    Json(super::factory_schema_wire::preset(encode(result, &id)?)),
  ))
}
impl ManagementApplication {
  /// Enables read-only setup without enabling intake or granting execution authority.
  pub fn with_factory_presets(mut self, presets: FactoryPresetManagementApplication) -> Self {
    self.factory_presets = Some(presets);
    self
  }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PageParameters {
  after: Option<String>,
  limit: Option<u16>,
}
pub(super) async fn list(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  parameters: Result<Query<PageParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let Query(p) = parameters.map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let page = app
    .inputs
    .factory_role_page(&project, p.after.as_deref(), p.limit.unwrap_or(DEFAULT_PAGE_LIMIT))
    .map_err(|e| invalid_input(e, &id))?;
  let service = app
    .factory_presets
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  let result = service
    .list
    .handle_authorized_query(&context, ListFactoryPresetsQuery { page })
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(result)))
}
