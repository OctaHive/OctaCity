//! Project role/profile HTTP operations over the existing audited publication authority.
use super::*;
use octacity_server_application::{
  ApplicationError, AuthorizedManagementCommandHandler, AuthorizedManagementQueryHandler, GetFactoryRoleProfileQuery,
  GetFactoryRoleQuery, ListFactoryRoleProfilesQuery, ListFactoryRolesQuery, PublishFactoryRoleProfilesCommand,
};
use octacity_server_application::{
  CopyFactoryRoleTemplateCommand, GetFactoryRoleTemplateQuery, ListFactoryRoleTemplatesQuery, ValidateFactoryRoleQuery,
};
use octacity_server_application::{
  GetFactoryRoleProfileCapabilityQuery, ListFactoryRoleProfileCapabilitiesQuery, ValidateFactoryRoleProfileQuery,
};
use octacity_server_application::{ImportProjectSkill, ListProjectSkillsQuery};
/// Optional AI-agent authoring, independent of worker enrollment and execution providers.
pub struct FactoryRoleManagementApplication {
  capabilities:
    Arc<dyn AuthorizedManagementQueryHandler<ListFactoryRoleProfileCapabilitiesQuery, Error = ApplicationError>>,
  capability: Arc<dyn AuthorizedManagementQueryHandler<GetFactoryRoleProfileCapabilityQuery, Error = ApplicationError>>,
  validate_profile:
    Arc<dyn AuthorizedManagementQueryHandler<ValidateFactoryRoleProfileQuery, Error = ApplicationError>>,
  import_skill: Option<Arc<dyn AuthorizedManagementCommandHandler<ImportProjectSkill, Error = ApplicationError>>>,
  skills: Option<Arc<dyn AuthorizedManagementQueryHandler<ListProjectSkillsQuery, Error = ApplicationError>>>,
  copy: Arc<dyn AuthorizedManagementCommandHandler<CopyFactoryRoleTemplateCommand, Error = ApplicationError>>,
  validate_role: Arc<dyn AuthorizedManagementQueryHandler<ValidateFactoryRoleQuery, Error = ApplicationError>>,
  templates: Arc<dyn AuthorizedManagementQueryHandler<ListFactoryRoleTemplatesQuery, Error = ApplicationError>>,
  template: Arc<dyn AuthorizedManagementQueryHandler<GetFactoryRoleTemplateQuery, Error = ApplicationError>>,
  publish: Arc<dyn AuthorizedManagementCommandHandler<PublishFactoryRoleProfilesCommand, Error = ApplicationError>>,
  roles: Arc<dyn AuthorizedManagementQueryHandler<ListFactoryRolesQuery, Error = ApplicationError>>,
  profiles: Arc<dyn AuthorizedManagementQueryHandler<ListFactoryRoleProfilesQuery, Error = ApplicationError>>,
  profile: Arc<dyn AuthorizedManagementQueryHandler<GetFactoryRoleProfileQuery, Error = ApplicationError>>,
  role: Arc<dyn AuthorizedManagementQueryHandler<GetFactoryRoleQuery, Error = ApplicationError>>,
}
impl FactoryRoleManagementApplication {
  /// Uses the existing retained-input publisher and authorized catalog; no second file store.
  pub fn with_skills<C, Q>(mut self, commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<ImportProjectSkill, Error = ApplicationError> + 'static,
    Q: AuthorizedManagementQueryHandler<ListProjectSkillsQuery, Error = ApplicationError> + 'static,
  {
    self.import_skill = Some(commands);
    self.skills = Some(queries);
    self
  }
  /// Shares the same Project authorization and immutable catalog services.
  pub fn new<C, Q>(commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<PublishFactoryRoleProfilesCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<CopyFactoryRoleTemplateCommand, Error = ApplicationError>
      + 'static,
    Q: AuthorizedManagementQueryHandler<GetFactoryRoleQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<GetFactoryRoleProfileQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListFactoryRolesQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListFactoryRoleProfilesQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ValidateFactoryRoleQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListFactoryRoleTemplatesQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<GetFactoryRoleTemplateQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListFactoryRoleProfileCapabilitiesQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<GetFactoryRoleProfileCapabilityQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ValidateFactoryRoleProfileQuery, Error = ApplicationError>
      + 'static,
  {
    Self {
      capabilities: queries.clone(),
      capability: queries.clone(),
      validate_profile: queries.clone(),
      import_skill: None,
      skills: None,
      copy: commands.clone(),
      validate_role: queries.clone(),
      templates: queries.clone(),
      template: queries.clone(),
      publish: commands,
      roles: queries.clone(),
      profiles: queries.clone(),
      profile: queries.clone(),
      role: queries,
    }
  }
}
impl ManagementApplication {
  /// Installs authoring without requiring Factory admission, workers or a provider.
  pub fn with_factory_roles(mut self, roles: FactoryRoleManagementApplication) -> Self {
    self.factory_roles = Some(roles);
    self
  }
}
fn endpoints<'a>(
  app: &'a ManagementApplication,
  id: &RequestId,
) -> Result<&'a FactoryRoleManagementApplication, ApiError> {
  app
    .factory_roles
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, id))
}
pub(super) async fn publish(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<crate::v1::PublishFactoryRoleProfilesRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &id)?;
  let key = idempotency_key(&headers, &id)?;
  let command = app
    .inputs
    .publish_factory_role_profiles(&project, body.role, body.profiles, key.as_str(), now_unix_ms(&id)?)
    .map_err(|e| invalid_input(e, &id))?;
  let result = endpoints(&app, &id)?
    .publish
    .handle_authorized_command(&context, command)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(result, &id)?)))
}
pub(super) async fn role(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, role)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
  let query = app
    .inputs
    .get_factory_role(&project, &role, None)
    .map_err(|e| invalid_input(e, &id))?;
  let result = endpoints(&app, &id)?
    .role
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(result, &id)?)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PageParameters {
  after: Option<String>,
  limit: Option<u16>,
}
async fn read<Q: octacity_server_application::Query + 'static>(
  handler: &Arc<dyn AuthorizedManagementQueryHandler<Q, Error = ApplicationError>>,
  context: &octacity_server_application::ManagementRequestContext,
  query: Q,
  id: &RequestId,
) -> Result<axum::response::Response, ApiError>
where
  Q::Outcome: Serialize,
{
  let result = handler
    .handle_authorized_query(context, query)
    .await
    .map_err(|e| authorized_handler_error(e, id))?;
  Ok(([("cache-control", "no-store")], Json(encode(result, id)?)).into_response())
}
pub(super) async fn roles(
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
    &endpoints(&app, &id)?.roles,
    &context,
    ListFactoryRolesQuery { page },
    &id,
  )
  .await
}
pub(super) async fn profiles(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, role)): Path<(String, String)>,
  parameters: Result<Query<PageParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let Query(p) = parameters.map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let page = app
    .inputs
    .factory_role_page(&project, p.after.as_deref(), p.limit.unwrap_or(DEFAULT_PAGE_LIMIT))
    .map_err(|e| invalid_input(e, &id))?;
  let query = ListFactoryRoleProfilesQuery {
    page,
    role_id: role
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
  };
  read(&endpoints(&app, &id)?.profiles, &context, query, &id).await
}
pub(super) async fn role_version(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, role, version)): Path<(String, String, u64)>,
) -> Result<axum::response::Response, ApiError> {
  let query = app
    .inputs
    .get_factory_role(&project, &role, Some(version))
    .map_err(|e| invalid_input(e, &id))?;
  read(&endpoints(&app, &id)?.role, &context, query, &id).await
}
pub(super) async fn profile(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, profile)): Path<(String, String)>,
) -> Result<axum::response::Response, ApiError> {
  let query = app
    .inputs
    .get_factory_role_profile(&project, &profile, None)
    .map_err(|e| invalid_input(e, &id))?;
  read(&endpoints(&app, &id)?.profile, &context, query, &id).await
}
pub(super) async fn profile_version(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, profile, version)): Path<(String, String, u64)>,
) -> Result<axum::response::Response, ApiError> {
  let query = app
    .inputs
    .get_factory_role_profile(&project, &profile, Some(version))
    .map_err(|e| invalid_input(e, &id))?;
  read(&endpoints(&app, &id)?.profile, &context, query, &id).await
}

pub(super) async fn templates(
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
    &endpoints(&app, &id)?.templates,
    &context,
    ListFactoryRoleTemplatesQuery { page },
    &id,
  )
  .await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TemplateParameters {
  digest: String,
}
pub(super) async fn template(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, identity, version)): Path<(String, String, String)>,
  parameters: Result<Query<TemplateParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let Query(p) = parameters.map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let query = app
    .inputs
    .get_factory_role_template(&project, &identity, &version, &p.digest)
    .map_err(|e| invalid_input(e, &id))?;
  read(&endpoints(&app, &id)?.template, &context, query, &id).await
}
pub(super) async fn validate_role(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  payload: Result<Json<crate::v1::ValidateFactoryRoleRequest>, JsonRejection>,
) -> Result<axum::response::Response, ApiError> {
  let body = body(payload, &id)?;
  let query = app
    .inputs
    .validate_factory_role(&project, &body.id, body.definition)
    .map_err(|e| invalid_input(e, &id))?;
  read(&endpoints(&app, &id)?.validate_role, &context, query, &id).await
}
pub(super) async fn copy_template(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<crate::v1::CopyFactoryRoleTemplateRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &id)?;
  let key = idempotency_key(&headers, &id)?;
  let command = app
    .inputs
    .copy_factory_role_template(
      &project,
      serde_json::to_value(body).map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
      key.as_str(),
      now_unix_ms(&id)?,
    )
    .map_err(|e| invalid_input(e, &id))?;
  let result = endpoints(&app, &id)?
    .copy
    .handle_authorized_command(&context, command)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(result, &id)?)))
}

pub(super) async fn import_skill(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<crate::v1::ImportProjectSkillRequest>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &id)?;
  let key = idempotency_key(&headers, &id)?;
  let command = app
    .inputs
    .import_project_skill(
      &project,
      &body.identity,
      serde_json::to_value(body.files).map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
      key.as_str(),
    )
    .map_err(|e| invalid_input(e, &id))?;
  let endpoint = endpoints(&app, &id)?
    .import_skill
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  let result = endpoint
    .handle_authorized_command(&context, command)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(result, &id)?)))
}
pub(super) async fn skills(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  parameters: Result<Query<PageParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let Query(p) = parameters.map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let query = app
    .inputs
    .list_project_skills(&project, p.after.as_deref(), p.limit.unwrap_or(DEFAULT_PAGE_LIMIT))
    .map_err(|e| invalid_input(e, &id))?;
  let endpoint = endpoints(&app, &id)?
    .skills
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  read(endpoint, &context, query, &id).await
}

pub(super) async fn capabilities(
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
    &endpoints(&app, &id)?.capabilities,
    &context,
    ListFactoryRoleProfileCapabilitiesQuery { page },
    &id,
  )
  .await
}
pub(super) async fn capability(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, alias)): Path<(String, String)>,
) -> Result<axum::response::Response, ApiError> {
  let query = app
    .inputs
    .get_factory_role_profile_capability(&project, &alias)
    .map_err(|e| invalid_input(e, &id))?;
  read(&endpoints(&app, &id)?.capability, &context, query, &id).await
}
pub(super) async fn validate_profile(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  payload: Result<Json<crate::v1::ValidateFactoryRoleProfileRequest>, JsonRejection>,
) -> Result<axum::response::Response, ApiError> {
  let body = body(payload, &id)?;
  let query = app
    .inputs
    .validate_factory_role_profile(&project, &body.id, body.role, &body.capability, body.definition)
    .map_err(|e| invalid_input(e, &id))?;
  read(&endpoints(&app, &id)?.validate_profile, &context, query, &id).await
}
