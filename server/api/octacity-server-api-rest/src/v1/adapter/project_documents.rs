//! Named context documents use the shared Project input publisher, independently of execution.
use super::*;
use octacity_server_application::{
  ApplicationError, AuthorizedManagementCommandHandler, AuthorizedManagementQueryHandler,
  ListProjectInputDocumentsQuery, PublishProjectDocumentCommand, ReadProjectInputDocumentQuery,
};

pub(super) struct Endpoints {
  publish: Arc<dyn AuthorizedManagementCommandHandler<PublishProjectDocumentCommand, Error = ApplicationError>>,
  list: Arc<dyn AuthorizedManagementQueryHandler<ListProjectInputDocumentsQuery, Error = ApplicationError>>,
  read: Arc<dyn AuthorizedManagementQueryHandler<ReadProjectInputDocumentQuery, Error = ApplicationError>>,
}
impl ManagementApplication {
  /// Connects existing Project input services; stores no private documents in the HTTP adapter.
  pub fn with_project_documents<C, Q>(mut self, commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<PublishProjectDocumentCommand, Error = ApplicationError> + 'static,
    Q: AuthorizedManagementQueryHandler<ListProjectInputDocumentsQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ReadProjectInputDocumentQuery, Error = ApplicationError>
      + 'static,
  {
    self.projects.documents = Some(Endpoints {
      publish: commands,
      list: queries.clone(),
      read: queries,
    });
    self
  }
}
pub(super) async fn read(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, artifact)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
  let query = app
    .inputs
    .read_project_input_document(&project, &artifact)
    .map_err(|e| invalid_input(e, &id))?;
  let result = endpoints(&app, &id)?
    .read
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(result)))
}
fn endpoints<'a>(app: &'a ManagementApplication, id: &RequestId) -> Result<&'a Endpoints, ApiError> {
  app
    .projects
    .documents
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, id))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Publication {
  name: String,
  content: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Page {
  after: Option<String>,
  #[serde(default = "default_page_limit")]
  limit: u16,
}
pub(super) async fn publish(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<Publication>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &id)?;
  let key = idempotency_key(&headers, &id)?;
  let command = app
    .inputs
    .publish_project_document(&project, body.name, body.content, key.as_str())
    .map_err(|e| invalid_input(e, &id))?;
  let resource = endpoints(&app, &id)?
    .publish
    .handle_authorized_command(&context, command)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok((StatusCode::CREATED, [("cache-control", "no-store")], Json(resource)))
}
pub(super) async fn list(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  parameters: Result<Query<Page>, QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let Query(page) = parameters.map_err(|e| query_error(e, &id))?;
  let query = app
    .inputs
    .list_project_input_documents(&project, page.after.as_deref(), page.limit)
    .map_err(|e| invalid_input(e, &id))?;
  let result = endpoints(&app, &id)?
    .list
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(result)))
}
