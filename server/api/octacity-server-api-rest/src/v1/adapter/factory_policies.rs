//! HTTP transport for Project policies; installation and execution remain separate owners.
use super::*;
use octacity_server_application::{
  ApplicationError, AuthorizedManagementCommandHandler, AuthorizedManagementQueryHandler, FactoryLegacyPolicyReference,
  FactoryProjectPolicyDefaultsDraft, FactoryProjectPolicyDraft, GetFactoryProjectPolicyDefaultsQuery,
  GetFactoryProjectPolicyQuery, ListFactoryProjectPoliciesQuery, PublishFactoryProjectPolicyCommand,
  ResolveFactoryLegacyPolicyQuery, SelectFactoryProjectPolicyDefaultsCommand, ValidateFactoryProjectPolicyQuery,
};

/// Authorized policy handlers, independent of optional Factory execution providers.
pub struct FactoryPolicyManagementApplication {
  legacy: Arc<dyn AuthorizedManagementQueryHandler<ResolveFactoryLegacyPolicyQuery, Error = ApplicationError>>,
  list: Arc<dyn AuthorizedManagementQueryHandler<ListFactoryProjectPoliciesQuery, Error = ApplicationError>>,
  defaults: Arc<dyn AuthorizedManagementQueryHandler<GetFactoryProjectPolicyDefaultsQuery, Error = ApplicationError>>,
  select_defaults:
    Arc<dyn AuthorizedManagementCommandHandler<SelectFactoryProjectPolicyDefaultsCommand, Error = ApplicationError>>,
  validate: Arc<dyn AuthorizedManagementQueryHandler<ValidateFactoryProjectPolicyQuery, Error = ApplicationError>>,
  publish: Arc<dyn AuthorizedManagementCommandHandler<PublishFactoryProjectPolicyCommand, Error = ApplicationError>>,
  detail: Arc<dyn AuthorizedManagementQueryHandler<GetFactoryProjectPolicyQuery, Error = ApplicationError>>,
}
impl FactoryPolicyManagementApplication {
  /// Shares ordinary management authorization for every read and write.
  pub fn new<C, Q>(commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<PublishFactoryProjectPolicyCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<SelectFactoryProjectPolicyDefaultsCommand, Error = ApplicationError>
      + 'static,
    Q: AuthorizedManagementQueryHandler<ResolveFactoryLegacyPolicyQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListFactoryProjectPoliciesQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<GetFactoryProjectPolicyDefaultsQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<GetFactoryProjectPolicyQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ValidateFactoryProjectPolicyQuery, Error = ApplicationError>
      + 'static,
  {
    Self {
      legacy: queries.clone(),
      select_defaults: commands.clone(),
      list: queries.clone(),
      defaults: queries.clone(),
      publish: commands,
      validate: queries.clone(),
      detail: queries,
    }
  }
}
impl ManagementApplication {
  /// Installs policy administration without requiring workers or a model provider.
  pub fn with_factory_policies(mut self, policies: FactoryPolicyManagementApplication) -> Self {
    self.factory_policies = Some(policies);
    self
  }
}
fn endpoints<'a>(
  app: &'a ManagementApplication,
  id: &RequestId,
) -> Result<&'a FactoryPolicyManagementApplication, ApiError> {
  app
    .factory_policies
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, id))
}
pub(super) async fn publish(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<FactoryProjectPolicyDraft>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let draft = body(payload, &id)?;
  let key = idempotency_key(&headers, &id)?;
  let command = app
    .inputs
    .publish_factory_project_policy(&project, draft, key.as_str(), now_unix_ms(&id)?)
    .map_err(|e| invalid_input(e, &id))?;
  let saved = endpoints(&app, &id)?
    .publish
    .handle_authorized_command(&context, command)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(saved, &id)?)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExactParameters {
  digest: String,
}
pub(super) async fn detail(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, kind, policy, version)): Path<(String, String, String, u64)>,
  parameters: Result<Query<ExactParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let Query(parameters) = parameters.map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let query = app
    .inputs
    .get_factory_project_policy(&project, &kind, &policy, version, &parameters.digest)
    .map_err(|e| invalid_input(e, &id))?;
  let policy = endpoints(&app, &id)?
    .detail
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok((
    [("cache-control", "no-store")],
    Json(json!({"policy": encode(policy, &id)?})),
  ))
}

pub(super) async fn validate(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  payload: Result<Json<FactoryProjectPolicyDraft>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let draft = body(payload, &id)?;
  let query = app
    .inputs
    .validate_factory_project_policy(&project, draft)
    .map_err(|e| invalid_input(e, &id))?;
  let validation = endpoints(&app, &id)?
    .validate
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(validation, &id)?)))
}

/// Catalog continuation is the serialized typed reference from the previous page.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CatalogParameters {
  after: Option<String>,
  limit: Option<u16>,
}
pub(super) async fn list(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  parameters: Result<Query<CatalogParameters>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let Query(parameters) = parameters.map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let query = app
    .inputs
    .list_factory_project_policies(
      &project,
      parameters.after.as_deref(),
      parameters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
    )
    .map_err(|e| invalid_input(e, &id))?;
  let page = endpoints(&app, &id)?
    .list
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(page, &id)?)))
}
pub(super) async fn defaults(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
  let query = app
    .inputs
    .get_factory_project_policy_defaults(&project)
    .map_err(|e| invalid_input(e, &id))?;
  let defaults = endpoints(&app, &id)?
    .defaults
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok((
    [("cache-control", "no-store")],
    Json(json!({"defaults": encode(defaults, &id)?})),
  ))
}
pub(super) async fn select_defaults(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<FactoryProjectPolicyDefaultsDraft>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let draft = body(payload, &id)?;
  let key = idempotency_key(&headers, &id)?;
  let command = app
    .inputs
    .select_factory_project_policy_defaults(&project, draft, key.as_str(), now_unix_ms(&id)?)
    .map_err(|e| invalid_input(e, &id))?;
  let result = endpoints(&app, &id)?
    .select_defaults
    .handle_authorized_command(&context, command)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(result, &id)?)))
}

pub(super) async fn legacy(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  payload: Result<Json<FactoryLegacyPolicyReference>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let reference = body(payload, &id)?;
  let query = app
    .inputs
    .resolve_factory_legacy_policy(&project, reference)
    .map_err(|e| invalid_input(e, &id))?;
  let mapping = endpoints(&app, &id)?
    .legacy
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok((
    [("cache-control", "no-store")],
    Json(json!({"mapping": encode(mapping, &id)?})),
  ))
}
