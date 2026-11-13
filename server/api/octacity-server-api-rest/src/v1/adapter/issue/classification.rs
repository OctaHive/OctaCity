//! Read-only derived applicability; clock and subject authority stay on the server.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in super::super) struct ReviewParameters {}

pub(in super::super) async fn current(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  parameters: Result<Query<ReviewParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  parameters.map_err(|e| query_error(e, &id))?;
  read(&app, &context, &id, &project, &issue, None).await
}

pub(in super::super) async fn revision(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue, version)): Path<(String, String, String)>,
  parameters: Result<Query<ReviewParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  parameters.map_err(|e| query_error(e, &id))?;
  let version = version
    .parse()
    .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  read(&app, &context, &id, &project, &issue, Some(version)).await
}

async fn read(
  app: &ManagementApplication,
  context: &octacity_server_application::ManagementRequestContext,
  id: &RequestId,
  project: &str,
  issue: &str,
  version: Option<u64>,
) -> Result<axum::response::Response, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, id);
  let review = endpoints(app, id)?
    .classification
    .handle_authorized_query(
      context,
      octacity_server_application::ReviewIssueClassificationQuery {
        project_id: project.parse().map_err(|_| invalid())?,
        id: issue.parse().map_err(|_| invalid())?,
        version: version.map(IssueVersion::new).transpose().map_err(|_| invalid())?,
        // No target is selected here. Code-dependent claims remain explicitly unresolved;
        // execution review independently resolves its repository subject before admission.
        subject: None,
        checked_at: Timestamp::from_unix_millis(now_unix_ms(id)?).map_err(|_| invalid())?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, id))?;
  Ok(response(encode(&review, id)?, review.version.get(), StatusCode::OK))
}
