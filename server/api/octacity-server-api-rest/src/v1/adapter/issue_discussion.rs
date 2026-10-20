//! Dedicated operator discussion transport; never an execution or provider dispatcher.
use super::*;
use octacity_server_application::{
  AppendIssueCommentCommand, AppendIssueCommentIntent, ApplicationError, AuthorizedManagementCommandHandler,
  AuthorizedManagementQueryHandler, GetIssueCommentBodyQuery, IssueCommentBody,
};

/// Optional discussion services with independently authorized body access.
pub struct IssueDiscussionManagementApplication {
  append: Arc<dyn AuthorizedManagementCommandHandler<AppendIssueCommentCommand, Error = ApplicationError>>,
  body: Arc<dyn AuthorizedManagementQueryHandler<GetIssueCommentBodyQuery, Error = ApplicationError>>,
}
impl IssueDiscussionManagementApplication {
  /// Composes existing authorized application contracts without execution dependencies.
  pub fn new<C, Q>(commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<AppendIssueCommentCommand, Error = ApplicationError> + 'static,
    Q: AuthorizedManagementQueryHandler<GetIssueCommentBodyQuery, Error = ApplicationError> + 'static,
  {
    Self {
      append: commands,
      body: queries,
    }
  }
}
impl ManagementApplication {
  /// Enables discussion independently of manual task creation and execution configuration.
  pub fn with_issue_discussion(mut self, discussion: IssueDiscussionManagementApplication) -> Self {
    self.issue_discussion = Some(discussion);
    self
  }
}
fn endpoints<'a>(
  app: &'a ManagementApplication,
  id: &RequestId,
) -> Result<&'a IssueDiscussionManagementApplication, ApiError> {
  app
    .issue_discussion
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, id))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AppendRequest {
  body: IssueCommentBody,
  parent_id: Option<String>,
}
pub(super) async fn append(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<AppendRequest>, JsonRejection>,
) -> Result<axum::response::Response, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let key = idempotency_key(&headers, &id)?;
  let body = body(payload, &id)?;
  let outcome = endpoints(&app, &id)?
    .append
    .handle_authorized_command(
      &context,
      AppendIssueCommentCommand {
        intent: AppendIssueCommentIntent {
          project_id: project.parse().map_err(|_| invalid())?,
          issue_id: issue.parse().map_err(|_| invalid())?,
          parent_id: body.parent_id.map(|p| p.parse()).transpose().map_err(|_| invalid())?,
          body: body.body,
        },
        idempotency_key: key.as_str().parse().map_err(|_| invalid())?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  let status = match outcome.disposition {
    octacity_server_application::StoreMutationDisposition::Applied => StatusCode::CREATED,
    octacity_server_application::StoreMutationDisposition::Replayed => StatusCode::OK,
  };
  Ok(
    (
      status,
      [
        ("cache-control", "no-store".to_owned()),
        (
          "location",
          format!(
            "/api/v1/projects/{project}/issues/{issue}/comments/{}",
            outcome.comment.id
          ),
        ),
      ],
      Json(encode(outcome, &id)?),
    )
      .into_response(),
  )
}
pub(super) async fn get_body(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue, comment)): Path<(String, String, String)>,
) -> Result<axum::response::Response, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let content = endpoints(&app, &id)?
    .body
    .handle_authorized_query(
      &context,
      GetIssueCommentBodyQuery {
        project_id: project.parse().map_err(|_| invalid())?,
        issue_id: issue.parse().map_err(|_| invalid())?,
        comment_id: comment.parse().map_err(|_| invalid())?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(content, &id)?)).into_response())
}
