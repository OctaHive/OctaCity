//! References only; the Issue/source application owners enforce scope, freshness and replay.
use super::*;
use octacity_server_application::{EditIssueMaterialsCommand, EditIssueMaterialsIntent, IssueMaterialReference};
impl IssueManagementApplication {
  /// Adds independently authorized source selection without execution/provider dependencies.
  pub fn with_materials<C>(mut self, commands: Arc<C>) -> Self
  where
    C: AuthorizedManagementCommandHandler<EditIssueMaterialsCommand, Error = ApplicationError> + 'static,
  {
    self.materials = Some(commands);
    self
  }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in super::super) struct EditRequest {
  references: Vec<IssueMaterialReference>,
}
pub(in super::super) async fn edit(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<EditRequest>, JsonRejection>,
) -> Result<axum::response::Response, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let expected = precondition(&headers, &id)?;
  let key = idempotency_key(&headers, &id)?;
  let body = body(payload, &id)?;
  let outcome = endpoints(&app, &id)?
    .materials
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?
    .handle_authorized_command(
      &context,
      EditIssueMaterialsCommand {
        intent: EditIssueMaterialsIntent {
          project_id: project.parse().map_err(|_| invalid())?,
          id: issue.parse().map_err(|_| invalid())?,
          expected_version: IssueVersion::new(expected.version()).map_err(|_| invalid())?,
          references: body.references,
        },
        idempotency_key: key.as_str().parse().map_err(|_| invalid())?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(response(
    json!({"disposition":encode(outcome.disposition,&id)?,"resource":representation(&outcome.issue,&id)?}),
    outcome.issue.version.get(),
    StatusCode::OK,
  ))
}
