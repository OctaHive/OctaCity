//! Project source preview accepts reviewed public pins, never private adapter or credential settings.
use super::*;
use octacity_server_application::{ListWorkSourceInputChoicesQuery, PreviewProjectIssueSourcesQuery};
impl IntegrationManagementApplication {
  /// Installs bounded read-only task-source preview without an Issue, import worker or Factory target.
  pub fn with_project_source_preview<Q>(mut self, queries: Arc<Q>) -> Self
  where
    Q: AuthorizedManagementQueryHandler<PreviewProjectIssueSourcesQuery, Error = ApplicationError> + 'static,
  {
    self.project_source_preview = Some(queries);
    self
  }
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectSourcePreviewBody {
  connection_id: octacity_server_application::IntegrationId,
  connection_version: octacity_server_application::IntegrationVersion,
  access_version: octacity_server_application::IntegrationVersion,
  source: octacity_server_application::WorkSourceBinding,
  limit: u16,
}
pub(crate) async fn preview_project_sources(
  State(app): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(id, ctx)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  payload: Result<Json<ProjectSourcePreviewBody>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
  let payload = body(payload, &id)?;
  let result = app
    .project_source_preview
    .as_ref()
    .ok_or_else(|| sources::source_unavailable(&app, &id))?
    .handle_authorized_query(
      &ctx,
      PreviewProjectIssueSourcesQuery {
        selection: ListWorkSourceInputChoicesQuery {
          project_id: project
            .parse()
            .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
          connection_id: payload.connection_id,
          connection_version: payload.connection_version,
        },
        access_version: payload.access_version,
        source: payload.source,
        limit: payload.limit,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(Json(encode(result, &id)?))
}
