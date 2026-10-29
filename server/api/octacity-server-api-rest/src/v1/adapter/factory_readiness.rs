//! Exact-subject readiness uses authorized queries and grants no future dispatch permission.
use super::*;
use octacity_server_application::{
  ApplicationError, AuthorizedManagementQueryHandler, FactoryReadinessSelection, GetFactoryReadinessQuery,
};
/// Installed read-only exact Configuration/Repository readiness owner.
pub struct FactoryReadinessManagementApplication {
  query: Arc<dyn AuthorizedManagementQueryHandler<GetFactoryReadinessQuery, Error = ApplicationError>>,
}
impl FactoryReadinessManagementApplication {
  /// Requires the existing management authorization decorator.
  pub fn new<Q: AuthorizedManagementQueryHandler<GetFactoryReadinessQuery, Error = ApplicationError> + 'static>(
    query: Arc<Q>,
  ) -> Self {
    Self { query }
  }
}
impl ManagementApplication {
  /// Adds read-only diagnosis without enabling intake or execution.
  pub fn with_factory_readiness(mut self, readiness: FactoryReadinessManagementApplication) -> Self {
    self.factory_readiness = Some(readiness);
    self
  }
}
pub(super) async fn readiness(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((configuration, version)): Path<(String, u64)>,
  payload: Result<Json<FactoryReadinessSelection>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let selected = app
    .inputs
    .get_factory_configuration(&configuration, version)
    .map_err(|e| invalid_input(e, &id))?;
  let query = GetFactoryReadinessQuery {
    id: selected.id,
    version: selected.version,
    selection: body(payload, &id)?,
  };
  let service = app
    .factory_readiness
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  let result = service
    .query
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(result)))
}
