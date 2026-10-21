//! Optional thin task-scoped adapters over the existing authorized retained readers.
use super::*;
use octacity_server_application::{
  ApplicationError, AuthorizedManagementQueryHandler, GetIssueWorkExecutionQuery, IssueArtifactReadMode,
  ListIssueArtifactsQuery, ListIssueChecksQuery, ReadIssueArtifactContentQuery,
};
/// Read-only result composition; manual Issues need neither Factory nor a provider.
pub struct IssueResultPanelManagementApplication {
  checks: Arc<dyn AuthorizedManagementQueryHandler<ListIssueChecksQuery, Error = ApplicationError>>,
  artifacts: Arc<dyn AuthorizedManagementQueryHandler<ListIssueArtifactsQuery, Error = ApplicationError>>,
  content: Arc<dyn AuthorizedManagementQueryHandler<ReadIssueArtifactContentQuery, Error = ApplicationError>>,
}
impl IssueResultPanelManagementApplication {
  /// Reuses independent execution/Artifact authorities without installing an execution owner.
  pub fn new<C, A>(checks: Arc<C>, artifacts: Arc<A>) -> Self
  where
    C: AuthorizedManagementQueryHandler<ListIssueChecksQuery, Error = ApplicationError> + 'static,
    A: AuthorizedManagementQueryHandler<ListIssueArtifactsQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ReadIssueArtifactContentQuery, Error = ApplicationError>
      + 'static,
  {
    Self {
      checks,
      artifacts: artifacts.clone(),
      content: artifacts,
    }
  }
}
impl ManagementApplication {
  /// Installs optional inspection without changing task creation or publication.
  pub fn with_issue_result_panels(mut self, panels: IssueResultPanelManagementApplication) -> Self {
    self.issue_result_panels = Some(panels);
    self
  }
}
fn endpoints<'a>(
  app: &'a ManagementApplication,
  id: &RequestId,
) -> Result<&'a IssueResultPanelManagementApplication, ApiError> {
  app
    .issue_result_panels
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, id))
}
fn execution(project: &str, issue: &str, work: &str, id: &RequestId) -> Result<GetIssueWorkExecutionQuery, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, id);
  Ok(GetIssueWorkExecutionQuery {
    project_id: project.parse().map_err(|_| invalid())?,
    issue_id: issue.parse().map_err(|_| invalid())?,
    work_id: work.parse().map_err(|_| invalid())?,
  })
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CheckParameters {
  work_id: String,
  record: String,
  after: Option<String>,
  limit: Option<u16>,
}
pub(super) async fn checks(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  parameters: Result<Query<CheckParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let Query(p) = parameters.map_err(|_| invalid())?;
  let query = ListIssueChecksQuery::try_new(
    execution(&project, &issue, &p.work_id, &id)?,
    p.record.parse().map_err(|_| invalid())?,
    p.after,
    p.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
  )
  .map_err(|e| application_error(e.classification(), &id))?;
  let page = endpoints(&app, &id)?
    .checks
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(page, &id)?)).into_response())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ArtifactParameters {
  work_id: String,
  after: Option<String>,
  limit: Option<u16>,
}
pub(super) async fn artifacts(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  parameters: Result<Query<ArtifactParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let Query(p) = parameters.map_err(|_| invalid())?;
  let query = ListIssueArtifactsQuery::try_new(
    execution(&project, &issue, &p.work_id, &id)?,
    p.after,
    p.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
  )
  .map_err(|e| application_error(e.classification(), &id))?;
  let page = endpoints(&app, &id)?
    .artifacts
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(page, &id)?)).into_response())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ContentParameters {
  work_id: String,
  mode: IssueArtifactReadMode,
}
pub(super) async fn content(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue, artifact)): Path<(String, String, String)>,
  parameters: Result<Query<ContentParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let Query(p) = parameters.map_err(|_| invalid())?;
  let query = ReadIssueArtifactContentQuery {
    execution: execution(&project, &issue, &p.work_id, &id)?,
    artifact_id: artifact.parse().map_err(|_| invalid())?,
    mode: p.mode,
  };
  let content = endpoints(&app, &id)?
    .content
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  let disposition = if p.mode == IssueArtifactReadMode::Download {
    "attachment"
  } else {
    "inline"
  };
  Ok(
    (
      [
        ("cache-control", "no-store"),
        ("content-type", content.media_type.as_str()),
        ("content-disposition", disposition),
        ("x-content-type-options", "nosniff"),
        ("content-security-policy", "default-src 'none'; sandbox"),
      ],
      content.bytes,
    )
      .into_response(),
  )
}
