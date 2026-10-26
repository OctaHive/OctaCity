//! Import lifecycle publishes metadata only; Source consumers, pull and Factory inputs remain independent.
use super::*;
use octacity_server_application::{
  GetIssueSourceImportQuery, ListIssueSourceImportsQuery, PublishIssueSourceImportCommand,
};
pub(super) struct ProjectSourceImportEndpoints {
  publish: Arc<dyn AuthorizedManagementCommandHandler<PublishIssueSourceImportCommand, Error = ApplicationError>>,
  get: Arc<dyn AuthorizedManagementQueryHandler<GetIssueSourceImportQuery, Error = ApplicationError>>,
  list: Arc<dyn AuthorizedManagementQueryHandler<ListIssueSourceImportsQuery, Error = ApplicationError>>,
}
impl IntegrationManagementApplication {
  /// Installs protected reviewed import metadata, independent of task creation or execution.
  pub fn with_project_source_imports<C, Q>(mut self, commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<PublishIssueSourceImportCommand, Error = ApplicationError> + 'static,
    Q: AuthorizedManagementQueryHandler<GetIssueSourceImportQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListIssueSourceImportsQuery, Error = ApplicationError>
      + 'static,
  {
    self.project_source_imports = Some(ProjectSourceImportEndpoints {
      publish: commands,
      get: queries.clone(),
      list: queries,
    });
    self
  }
}
fn services<'a>(
  app: &'a IntegrationManagementApplication,
  id: &RequestId,
) -> Result<&'a ProjectSourceImportEndpoints, ApiError> {
  app
    .project_source_imports
    .as_ref()
    .ok_or_else(|| sources::source_unavailable(app, id))
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ImportPublication {
  intent: octacity_server_application::IssueSourceImportIntent,
}
pub(crate) async fn publish_project_import(
  State(app): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(id, ctx)): Extension<crate::ManagementRequest>,
  Path((project, import)): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<ImportPublication>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
  let intent = body(payload, &id)?.intent;
  if intent.project_id.to_string() != project || intent.id.to_string() != import {
    return Err(application_error(ApplicationFailure::Invalid, &id));
  }
  check_precondition(&headers, intent.expected_current_version.map_or(0, |v| v.get()), &id)?;
  let result = services(&app, &id)?
    .publish
    .handle_authorized_command(
      &ctx,
      PublishIssueSourceImportCommand {
        intent,
        idempotency_key: idempotency_key(&headers, &id)?
          .as_str()
          .parse()
          .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
        published_at: Timestamp::from_unix_millis(now_unix_ms(&id)?)
          .map_err(|_| application_error(ApplicationFailure::Internal, &id))?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(Json(encode(result, &id)?))
}
async fn get(
  app: Arc<IntegrationManagementApplication>,
  id: RequestId,
  ctx: ManagementRequestContext,
  project: String,
  import: String,
  version: Option<u64>,
) -> Result<Json<Value>, ApiError> {
  let result = services(&app, &id)?
    .get
    .handle_authorized_query(
      &ctx,
      GetIssueSourceImportQuery {
        project_id: project
          .parse()
          .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
        id: import
          .parse()
          .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
        version: version
          .map(|v| serde_json::from_value(json!(v)))
          .transpose()
          .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(Json(json!({"configuration":encode(result,&id)?})))
}
pub(crate) async fn get_current_project_import(
  State(app): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(id, ctx)): Extension<crate::ManagementRequest>,
  Path((project, import)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
  get(app, id, ctx, project, import, None).await
}
pub(crate) async fn get_project_import(
  State(app): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(id, ctx)): Extension<crate::ManagementRequest>,
  Path((project, import, version)): Path<(String, String, u64)>,
) -> Result<Json<Value>, ApiError> {
  get(app, id, ctx, project, import, Some(version)).await
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ImportList {
  after: Option<String>,
  limit: Option<u16>,
}
pub(crate) async fn list_project_imports(
  State(app): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(id, ctx)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  query: Result<Query<ImportList>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, ApiError> {
  let Query(query) = query.map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let result = services(&app, &id)?
    .list
    .handle_authorized_query(
      &ctx,
      ListIssueSourceImportsQuery {
        project_id: project
          .parse()
          .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
        after: query
          .after
          .map(|v| v.parse())
          .transpose()
          .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
        limit: query.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(Json(encode(result, &id)?))
}
