//! Optional review HTTP translation over existing authorized observation/application contracts.
use super::*;
use octacity_server_application::{
  ApplicationError, AttachIssueCodeReviewCommand, AttachIssueCodeReviewIntent, AuthorizedManagementCommandHandler,
  AuthorizedManagementQueryHandler, IssueCodeReviewBinding, IssueCodeReviewSelection, IssueVersion,
  ListIssueCodeReviewObservationsQuery, ListIssueCodeReviewsQuery, ListIssueExternalChecksQuery,
  RefreshIssueCodeReviewCommand,
};

/// Independently installed review endpoints; manual task management requires none of them.
pub struct IssueCodeReviewManagementApplication {
  checks: Option<Arc<dyn AuthorizedManagementQueryHandler<ListIssueExternalChecksQuery, Error = ApplicationError>>>,
  attach: Arc<dyn AuthorizedManagementCommandHandler<AttachIssueCodeReviewCommand, Error = ApplicationError>>,
  refresh: Arc<dyn AuthorizedManagementCommandHandler<RefreshIssueCodeReviewCommand, Error = ApplicationError>>,
  list: Arc<dyn AuthorizedManagementQueryHandler<ListIssueCodeReviewsQuery, Error = ApplicationError>>,
  observations:
    Arc<dyn AuthorizedManagementQueryHandler<ListIssueCodeReviewObservationsQuery, Error = ApplicationError>>,
}
impl IssueCodeReviewManagementApplication {
  /// Adds progressive retained CI reads under the existing review observation authority.
  pub fn with_checks<
    Q: AuthorizedManagementQueryHandler<ListIssueExternalChecksQuery, Error = ApplicationError> + 'static,
  >(
    mut self,
    queries: Arc<Q>,
  ) -> Self {
    self.checks = Some(queries);
    self
  }
  /// Composes existing authorized owners without contacting a provider or starting a worker.
  pub fn new<C, Q>(commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<AttachIssueCodeReviewCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<RefreshIssueCodeReviewCommand, Error = ApplicationError>
      + 'static,
    Q: AuthorizedManagementQueryHandler<ListIssueCodeReviewsQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListIssueCodeReviewObservationsQuery, Error = ApplicationError>
      + 'static,
  {
    Self {
      checks: None,
      attach: commands.clone(),
      refresh: commands,
      list: queries.clone(),
      observations: queries,
    }
  }
}
impl ManagementApplication {
  /// Adds optional review reads/commands independently from Source, Reporter and execution.
  pub fn with_issue_code_reviews(mut self, reviews: IssueCodeReviewManagementApplication) -> Self {
    self.issue_code_reviews = Some(reviews);
    self
  }
}
fn endpoints<'a>(
  app: &'a ManagementApplication,
  id: &RequestId,
) -> Result<&'a IssueCodeReviewManagementApplication, ApiError> {
  app
    .issue_code_reviews
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, id))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AttachRequest {
  selection: IssueCodeReviewSelection,
  binding: Option<IssueCodeReviewBinding>,
}
pub(super) async fn attach(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<AttachRequest>, JsonRejection>,
) -> Result<axum::response::Response, ApiError> {
  let expected = precondition(&headers, &id)?;
  let key = idempotency_key(&headers, &id)?;
  let request = body(payload, &id)?;
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let result = endpoints(&app, &id)?
    .attach
    .handle_authorized_command(
      &context,
      AttachIssueCodeReviewCommand {
        intent: AttachIssueCodeReviewIntent {
          project_id: project.parse().map_err(|_| invalid())?,
          issue_id: issue.parse().map_err(|_| invalid())?,
          expected_version: IssueVersion::new(expected.version()).map_err(|_| invalid())?,
          selection: request.selection,
          binding: request.binding,
        },
        idempotency_key: key.as_str().parse().map_err(|_| invalid())?,
      },
    )
    .await
    .map_err(|error| authorized_handler_error(error, &id))?;
  let status = if result.disposition == octacity_server_application::StoreMutationDisposition::Applied {
    StatusCode::CREATED
  } else {
    StatusCode::OK
  };
  Ok((status, [("cache-control", "no-store")], Json(encode(result, &id)?)).into_response())
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
  Path((project, issue)): Path<(String, String)>,
  parameters: Result<Query<PageParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let Query(p) = parameters.map_err(|_| invalid())?;
  let query = ListIssueCodeReviewsQuery::try_new(
    project.parse().map_err(|_| invalid())?,
    issue.parse().map_err(|_| invalid())?,
    p.after,
    p.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
  )
  .map_err(|e| application_error(e.classification(), &id))?;
  let page = endpoints(&app, &id)?
    .list
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(page, &id)?)).into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RefreshRequest {}
pub(super) async fn refresh(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue, review)): Path<(String, String, String)>,
  headers: HeaderMap,
  payload: Result<Json<RefreshRequest>, JsonRejection>,
) -> Result<axum::response::Response, ApiError> {
  let key = idempotency_key(&headers, &id)?;
  let RefreshRequest {} = body(payload, &id)?;
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let result = endpoints(&app, &id)?
    .refresh
    .handle_authorized_command(
      &context,
      RefreshIssueCodeReviewCommand {
        project_id: project.parse().map_err(|_| invalid())?,
        issue_id: issue.parse().map_err(|_| invalid())?,
        association_id: review.parse().map_err(|_| invalid())?,
        idempotency_key: key.as_str().parse().map_err(|_| invalid())?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(result, &id)?)).into_response())
}
pub(super) async fn observations(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue, review)): Path<(String, String, String)>,
  parameters: Result<Query<PageParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let Query(p) = parameters.map_err(|_| invalid())?;
  let query = ListIssueCodeReviewObservationsQuery::try_new(
    project.parse().map_err(|_| invalid())?,
    issue.parse().map_err(|_| invalid())?,
    review.parse().map_err(|_| invalid())?,
    p.after,
    p.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
  )
  .map_err(|e| application_error(e.classification(), &id))?;
  let page = endpoints(&app, &id)?
    .observations
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(page, &id)?)).into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CheckParameters {
  after: Option<String>,
}
pub(super) async fn checks(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue, review)): Path<(String, String, String)>,
  parameters: Result<Query<CheckParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let Query(p) = parameters.map_err(|_| invalid())?;
  let query = ListIssueExternalChecksQuery::try_new(
    project.parse().map_err(|_| invalid())?,
    issue.parse().map_err(|_| invalid())?,
    review.parse().map_err(|_| invalid())?,
    p.after,
  )
  .map_err(|e| application_error(e.classification(), &id))?;
  let page = endpoints(&app, &id)?
    .checks
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(page, &id)?)).into_response())
}
