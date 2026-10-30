//! Read-only action discovery; resolution reuses existing Factory endpoints unchanged.
use super::*;
use octacity_server_application::{
  ApplicationError, AuthorizedManagementQueryHandler, GetFactoryNodeActionQuery, ListFactoryActionsQuery,
  ListIssueFactoryActionsQuery,
};
/// Optional shared action index, independent of manual Issue services and providers.
pub struct FactoryActionManagementApplication {
  observation: Option<Arc<dyn AuthorizedManagementQueryHandler<GetFactoryNodeActionQuery, Error = ApplicationError>>>,
  list: Arc<dyn AuthorizedManagementQueryHandler<ListFactoryActionsQuery, Error = ApplicationError>>,
  issue: Arc<dyn AuthorizedManagementQueryHandler<ListIssueFactoryActionsQuery, Error = ApplicationError>>,
}
impl FactoryActionManagementApplication {
  /// Uses the authoritative existing Run reader for recorded/consumed state.
  pub fn with_observations<
    Q: AuthorizedManagementQueryHandler<GetFactoryNodeActionQuery, Error = ApplicationError> + 'static,
  >(
    mut self,
    queries: Arc<Q>,
  ) -> Self {
    self.observation = Some(queries);
    self
  }

  /// Installs the same authorized reader for shared and Issue-scoped collections.
  pub fn new<Q>(queries: Arc<Q>) -> Self
  where
    Q: AuthorizedManagementQueryHandler<ListFactoryActionsQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListIssueFactoryActionsQuery, Error = ApplicationError>
      + 'static,
  {
    Self {
      list: queries.clone(),
      issue: queries,
      observation: None,
    }
  }
}
impl ManagementApplication {
  /// Adds discovery without replacing ordinary controls or requiring execution admission.
  pub fn with_factory_actions(mut self, actions: FactoryActionManagementApplication) -> Self {
    self.factory_actions = Some(actions);
    self
  }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Parameters {
  after: Option<String>,
  limit: Option<u16>,
}
async fn read(
  app: Arc<ManagementApplication>,
  id: RequestId,
  context: octacity_server_application::ManagementRequestContext,
  project: Option<String>,
  issue: Option<String>,
  parameters: Result<Query<Parameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let Query(p) = parameters.map_err(|_| invalid())?;
  let project = project.map(|v| v.parse()).transpose().map_err(|_| invalid())?;
  let issue = issue.map(|v| v.parse()).transpose().map_err(|_| invalid())?;
  let endpoint = app
    .factory_actions
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  let limit = p.limit.unwrap_or(DEFAULT_PAGE_LIMIT);
  let result = match issue {
    Some(issue) => {
      endpoint
        .issue
        .handle_authorized_query(
          &context,
          ListIssueFactoryActionsQuery::try_new(project.ok_or_else(invalid)?, issue, p.after, limit)
            .map_err(|_| invalid())?,
        )
        .await
    }
    None => {
      endpoint
        .list
        .handle_authorized_query(
          &context,
          ListFactoryActionsQuery::try_new(project, p.after, limit).map_err(|_| invalid())?,
        )
        .await
    }
  }
  .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(
    (
      [("cache-control", "no-store")],
      Json(super::factory_schema_wire::actions(encode(result, &id)?)),
    )
      .into_response(),
  )
}
pub(super) async fn global(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  parameters: Result<Query<Parameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  read(app, id, context, None, None, parameters).await
}
pub(super) async fn project(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  parameters: Result<Query<Parameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  read(app, id, context, Some(project), None, parameters).await
}
pub(super) async fn issue(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  parameters: Result<Query<Parameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  read(app, id, context, Some(project), Some(issue), parameters).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ObservationParameters {
  project_id: Option<String>,
  issue_id: Option<String>,
}
pub(super) async fn observe(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((run, attempt)): Path<(String, String)>,
  parameters: Result<Query<ObservationParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let Query(p) = parameters.map_err(|_| invalid())?;
  if p.issue_id.is_some() && p.project_id.is_none() {
    return Err(invalid());
  }
  let query = GetFactoryNodeActionQuery {
    project_id: p.project_id.map(|v| v.parse()).transpose().map_err(|_| invalid())?,
    issue_id: p.issue_id.map(|v| v.parse()).transpose().map_err(|_| invalid())?,
    run_id: run.parse().map_err(|_| invalid())?,
    node_attempt_id: attempt.parse().map_err(|_| invalid())?,
  };
  let endpoint = app
    .factory_actions
    .as_ref()
    .and_then(|a| a.observation.as_ref())
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  let result = endpoint
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(
    (
      [("cache-control", "no-store")],
      Json(super::factory_schema_wire::actions(encode(result, &id)?)),
    )
      .into_response(),
  )
}
