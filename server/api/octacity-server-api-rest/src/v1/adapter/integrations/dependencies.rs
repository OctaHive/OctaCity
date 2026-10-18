//! Bounded history of visible connection dependents; no provider or use-grant requirement.
use super::*;
use octacity_server_application::{
  ListIntegrationDependenciesQuery, MAX_WORK_SOURCE_CONFIGURATION_PAGE_SIZE, WorkSourceScanTarget,
};
impl IntegrationManagementApplication {
  /// Adds retained dependency review independently of source activation and credential administration.
  pub fn with_dependencies<Q>(mut self, queries: Arc<Q>) -> Self
  where
    Q: AuthorizedManagementQueryHandler<ListIntegrationDependenciesQuery, Error = ApplicationError> + 'static,
  {
    self.dependencies = Some(queries);
    self
  }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Page {
  after: Option<String>,
  limit: Option<u16>,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
  connection_id: String,
  project_id: String,
  target: WorkSourceScanTarget,
}
pub(crate) async fn list_dependencies(
  State(app): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((connection, project)): Path<(String, String)>,
  query: Result<Query<Page>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let Query(query) = query.map_err(|_| invalid())?;
  let limit = query.limit.unwrap_or(DEFAULT_PAGE_LIMIT);
  if !(1..=MAX_WORK_SOURCE_CONFIGURATION_PAGE_SIZE).contains(&limit) {
    return Err(invalid());
  }
  let after = query
    .after
    .map(|s| {
      if s.len() > 256 {
        return Err(invalid());
      }
      let cursor: Cursor = serde_json::from_str(&s).map_err(|_| invalid())?;
      if cursor.connection_id != connection
        || cursor.project_id != project
        || !matches!(cursor.target, WorkSourceScanTarget::Input { .. })
      {
        return Err(invalid());
      }
      Ok(cursor.target)
    })
    .transpose()?;
  let result = app
    .dependencies
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?
    .handle_authorized_query(
      &context,
      ListIntegrationDependenciesQuery {
        connection_id: connection.parse().map_err(|_| invalid())?,
        project_id: project.parse().map_err(|_| invalid())?,
        after,
        limit,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  let next_cursor = result
    .next_cursor
    .map(|target| {
      serde_json::to_string(&Cursor {
        connection_id: connection,
        project_id: project,
        target,
      })
    })
    .transpose()
    .map_err(|_| application_error(ApplicationFailure::Internal, &id))?;
  Ok(Json(
    json!({"items":encode(result.items,&id)?,"next_cursor":next_cursor}),
  ))
}
