//! Independent metadata reads; optional technical results reuse authorized common readers.
use super::*;
use octacity_server_application::{
  ApplicationError, AuthorizedManagementQueryHandler, FactoryDiagnosticCollection, GetIssueWorkExecutionQuery,
  IssueTimelineKind, ListIssueResultsQuery, ListIssueTimelineQuery,
};
/// Card activity needs no Factory or provider. Results can be installed independently.
pub struct IssueActivityManagementApplication {
  timeline: Arc<dyn AuthorizedManagementQueryHandler<ListIssueTimelineQuery, Error = ApplicationError>>,
  results: Option<Arc<dyn AuthorizedManagementQueryHandler<ListIssueResultsQuery, Error = ApplicationError>>>,
}
impl IssueActivityManagementApplication {
  /// Installs the independently authorized local metadata reader.
  pub fn new<Q: AuthorizedManagementQueryHandler<ListIssueTimelineQuery, Error = ApplicationError> + 'static>(
    queries: Arc<Q>,
  ) -> Self {
    Self {
      timeline: queries,
      results: None,
    }
  }
  /// Installs exact Work/Run result reads without enabling admission or execution.
  pub fn with_results<
    Q: AuthorizedManagementQueryHandler<ListIssueResultsQuery, Error = ApplicationError> + 'static,
  >(
    mut self,
    queries: Arc<Q>,
  ) -> Self {
    self.results = Some(queries);
    self
  }
}
impl ManagementApplication {
  /// Adds optional activity without changing manual task composition.
  pub fn with_issue_activity(mut self, activity: IssueActivityManagementApplication) -> Self {
    self.issue_activity = Some(activity);
    self
  }
}
fn endpoints<'a>(
  app: &'a ManagementApplication,
  id: &RequestId,
) -> Result<&'a IssueActivityManagementApplication, ApiError> {
  app
    .issue_activity
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, id))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TimelineParameters {
  after: Option<String>,
  limit: Option<u16>,
  kinds: Option<String>,
}
pub(super) async fn timeline(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  parameters: Result<Query<TimelineParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let Query(p) = parameters.map_err(|_| invalid())?;
  let kinds = p
    .kinds
    .map(|s| {
      s.split(',')
        .map(|s| serde_json::from_value::<IssueTimelineKind>(json!(s)).map_err(|_| invalid()))
        .collect::<Result<_, _>>()
    })
    .transpose()?;
  let query = ListIssueTimelineQuery::try_new(
    project.parse().map_err(|_| invalid())?,
    issue.parse().map_err(|_| invalid())?,
    kinds,
    p.after,
    p.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
  )
  .map_err(|e| application_error(e.classification(), &id))?;
  let page = endpoints(&app, &id)?
    .timeline
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(page, &id)?)).into_response())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResultParameters {
  work_id: String,
  collection: FactoryDiagnosticCollection,
  after: Option<String>,
  limit: Option<u16>,
}
pub(super) async fn results(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  parameters: Result<Query<ResultParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let Query(p) = parameters.map_err(|_| invalid())?;
  let query = ListIssueResultsQuery::try_new(
    GetIssueWorkExecutionQuery {
      project_id: project.parse().map_err(|_| invalid())?,
      issue_id: issue.parse().map_err(|_| invalid())?,
      work_id: p.work_id.parse().map_err(|_| invalid())?,
    },
    p.collection,
    p.after,
    p.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
  )
  .map_err(|e| application_error(e.classification(), &id))?;
  let page = endpoints(&app, &id)?
    .results
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(page, &id)?)).into_response())
}
