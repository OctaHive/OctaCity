//! Exact Issue-owned publications over the shared immutable document owner.
use super::*;
use octacity_server_application::{
  ApplicationError, AuthorizedManagementCommandHandler, AuthorizedManagementQueryHandler, IssueDocumentSubmission,
  ListIssueDocumentsQuery, PublishIssueDocumentCommand, ReadIssueDocumentQuery,
};

pub(super) struct Endpoints {
  list: Arc<dyn AuthorizedManagementQueryHandler<ListIssueDocumentsQuery, Error = ApplicationError>>,
  read: Arc<dyn AuthorizedManagementQueryHandler<ReadIssueDocumentQuery, Error = ApplicationError>>,
  publish: Arc<dyn AuthorizedManagementCommandHandler<PublishIssueDocumentCommand, Error = ApplicationError>>,
}
impl ManagementApplication {
  /// Enables task-owned publication independently of shared Project publication authority.
  pub fn with_issue_documents<C, Q>(mut self, commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<PublishIssueDocumentCommand, Error = ApplicationError> + 'static,
    Q: AuthorizedManagementQueryHandler<ListIssueDocumentsQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ReadIssueDocumentQuery, Error = ApplicationError>
      + 'static,
  {
    self.issue_documents = Some(Endpoints {
      publish: commands,
      list: queries.clone(),
      read: queries,
    });
    self
  }
}
fn endpoints<'a>(app: &'a ManagementApplication, id: &RequestId) -> Result<&'a Endpoints, ApiError> {
  app
    .issue_documents
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, id))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Publication {
  name: String,
  content: String,
}
pub(super) async fn publish(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<Publication>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let body = body(payload, &id)?;
  let key = idempotency_key(&headers, &id)?;
  let resource = endpoints(&app, &id)?
    .publish
    .handle_authorized_command(
      &context,
      PublishIssueDocumentCommand {
        submission: IssueDocumentSubmission {
          project_id: project.parse().map_err(|_| invalid())?,
          issue_id: issue.parse().map_err(|_| invalid())?,
          name: body.name.parse().map_err(|_| invalid())?,
          content: body.content,
          idempotency_key: key.as_str().parse().map_err(|_| invalid())?,
        },
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok((StatusCode::CREATED, [("cache-control", "no-store")], Json(resource)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Page {
  after: Option<String>,
  #[serde(default = "default_page_limit")]
  limit: u16,
}
pub(super) async fn list(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  parameters: Result<Query<Page>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let Query(page) = parameters.map_err(|e| query_error(e, &id))?;
  let query = ListIssueDocumentsQuery::try_new(
    project.parse().map_err(|_| invalid())?,
    issue.parse().map_err(|_| invalid())?,
    page.after.map(|a| a.parse()).transpose().map_err(|_| invalid())?,
    page.limit,
  )
  .map_err(|_| invalid())?;
  let result = endpoints(&app, &id)?
    .list
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(result)))
}
pub(super) async fn read(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue, artifact)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let query = ReadIssueDocumentQuery {
    project_id: project.parse().map_err(|_| invalid())?,
    issue_id: issue.parse().map_err(|_| invalid())?,
    artifact_id: artifact.parse().map_err(|_| invalid())?,
  };
  let result = endpoints(&app, &id)?
    .read
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(Json(result))
}
