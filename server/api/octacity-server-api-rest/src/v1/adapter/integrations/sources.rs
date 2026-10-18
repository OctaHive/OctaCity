//! Protected, credential-free authoring over the same immutable input/group journal.
use super::*;
use octacity_server_application::{
  GetWorkSourceConfigurationQuery, ListWorkSourceConfigurationsQuery, ListWorkSourceInputChoicesQuery,
  MAX_WORK_SOURCE_CONFIGURATION_PAGE_SIZE, PublishWorkSourceConfigurationCommand, ValidateWorkSourceConfigurationQuery,
  WorkSourceConfigurationIntent, WorkSourceScanTarget,
};
pub(super) struct SourceConfigurationEndpoints {
  publish: Arc<dyn AuthorizedManagementCommandHandler<PublishWorkSourceConfigurationCommand, Error = ApplicationError>>,
  validate: Arc<dyn AuthorizedManagementQueryHandler<ValidateWorkSourceConfigurationQuery, Error = ApplicationError>>,
  choices: Arc<dyn AuthorizedManagementQueryHandler<ListWorkSourceInputChoicesQuery, Error = ApplicationError>>,
  get: Arc<dyn AuthorizedManagementQueryHandler<GetWorkSourceConfigurationQuery, Error = ApplicationError>>,
  list: Arc<dyn AuthorizedManagementQueryHandler<ListWorkSourceConfigurationsQuery, Error = ApplicationError>>,
}
impl IntegrationManagementApplication {
  /// Installs independently authorized input authoring; no credential or provider execution capability is added.
  pub fn with_source_configuration<C, Q>(mut self, commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<PublishWorkSourceConfigurationCommand, Error = ApplicationError> + 'static,
    Q: AuthorizedManagementQueryHandler<ValidateWorkSourceConfigurationQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListWorkSourceInputChoicesQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<GetWorkSourceConfigurationQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListWorkSourceConfigurationsQuery, Error = ApplicationError>
      + 'static,
  {
    self.sources = Some(SourceConfigurationEndpoints {
      publish: commands,
      validate: queries.clone(),
      choices: queries.clone(),
      get: queries.clone(),
      list: queries,
    });
    self
  }
}
fn services<'a>(
  application: &'a IntegrationManagementApplication,
  request_id: &RequestId,
) -> Result<&'a SourceConfigurationEndpoints, ApiError> {
  application
    .sources
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, request_id))
}
fn intent(body: SafeIntent, project: &str, request_id: &RequestId) -> Result<WorkSourceConfigurationIntent, ApiError> {
  let intent: WorkSourceConfigurationIntent =
    serde_json::from_value(body.intent).map_err(|_| application_error(ApplicationFailure::Invalid, request_id))?;
  if intent.project_id.to_string() != project {
    return Err(application_error(ApplicationFailure::Invalid, request_id));
  }
  Ok(intent)
}
async fn publish(
  application: Arc<IntegrationManagementApplication>,
  request_id: RequestId,
  context: ManagementRequestContext,
  path: (String, String),
  headers: HeaderMap,
  payload: Result<Json<SafeIntent>, JsonRejection>,
  grouped: bool,
) -> Result<Json<Value>, ApiError> {
  let intent = intent(body(payload, &request_id)?, &path.0, &request_id)?;
  let expected = if grouped {
    let group = intent
      .group
      .as_ref()
      .filter(|g| g.id.to_string() == path.1)
      .ok_or_else(|| application_error(ApplicationFailure::Invalid, &request_id))?;
    group.expected_version.map_or(0, |v| v.get())
  } else {
    if intent.group.is_some() || intent.inputs.len() != 1 || intent.inputs[0].id.to_string() != path.1 {
      return Err(application_error(ApplicationFailure::Invalid, &request_id));
    }
    intent.inputs[0].expected_version.map_or(0, |v| v.get())
  };
  check_precondition(&headers, expected, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let outcome = services(&application, &request_id)?
    .publish
    .handle_authorized_command(
      &context,
      PublishWorkSourceConfigurationCommand {
        intent,
        idempotency_key: key
          .as_str()
          .parse()
          .map_err(|_| application_error(ApplicationFailure::Invalid, &request_id))?,
        published_at: Timestamp::from_unix_millis(now_unix_ms(&request_id)?)
          .map_err(|_| application_error(ApplicationFailure::Internal, &request_id))?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &request_id))?;
  Ok(Json(
    json!({"disposition":encode(outcome.disposition,&request_id)?,"publication":encode(outcome.publication,&request_id)?,"published_at_unix_ms":outcome.published_at.unix_millis()}),
  ))
}
pub(crate) async fn publish_source_input(
  State(app): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(id, ctx)): Extension<crate::ManagementRequest>,
  Path(path): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<SafeIntent>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
  publish(app, id, ctx, path, headers, payload, false).await
}
pub(crate) async fn publish_source_group(
  State(app): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(id, ctx)): Extension<crate::ManagementRequest>,
  Path(path): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<SafeIntent>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
  publish(app, id, ctx, path, headers, payload, true).await
}
pub(crate) async fn validate_source_configuration(
  State(app): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(id, ctx)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  payload: Result<Json<SafeIntent>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
  let intent = intent(body(payload, &id)?, &project, &id)?;
  let publication = services(&app, &id)?
    .validate
    .handle_authorized_query(&ctx, ValidateWorkSourceConfigurationQuery { intent })
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(Json(json!({"publication":encode(publication,&id)?})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceChoices {
  connection_id: String,
  connection_version: u64,
}
pub(crate) async fn source_choices(
  State(app): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(id, ctx)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  query: Result<Query<SourceChoices>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, ApiError> {
  let Query(query) = query.map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let query = ListWorkSourceInputChoicesQuery {
    project_id: project
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    connection_id: query
      .connection_id
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    connection_version: serde_json::from_value(json!(query.connection_version))
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
  };
  let result = services(&app, &id)?
    .choices
    .handle_authorized_query(&ctx, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(Json(encode(result, &id)?))
}
async fn get(
  app: Arc<IntegrationManagementApplication>,
  id: RequestId,
  ctx: ManagementRequestContext,
  path: (String, String, u64),
  kind: &str,
) -> Result<Json<Value>, ApiError> {
  let query = GetWorkSourceConfigurationQuery {
    project_id: path
      .0
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    target: serde_json::from_value(json!({"kind":kind,"id":path.1,"version":path.2}))
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
  };
  let result = services(&app, &id)?
    .get
    .handle_authorized_query(&ctx, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(Json(encode(result, &id)?))
}
pub(crate) async fn get_source_input(
  State(app): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(id, ctx)): Extension<crate::ManagementRequest>,
  Path(path): Path<(String, String, u64)>,
) -> Result<Json<Value>, ApiError> {
  get(app, id, ctx, path, "input").await
}
pub(crate) async fn get_source_group(
  State(app): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(id, ctx)): Extension<crate::ManagementRequest>,
  Path(path): Path<(String, String, u64)>,
) -> Result<Json<Value>, ApiError> {
  get(app, id, ctx, path, "group").await
}
const MAX_SOURCE_CURSOR_BYTES: usize = 256;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceList {
  after: Option<String>,
  limit: Option<u16>,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct SourceCursor {
  project_id: String,
  target: WorkSourceScanTarget,
}
pub(crate) async fn list_source_configurations(
  State(app): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(id, ctx)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  query: Result<Query<SourceList>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, ApiError> {
  let Query(query) = query.map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let after = query
    .after
    .map(|after| {
      if after.len() > MAX_SOURCE_CURSOR_BYTES {
        return Err(application_error(ApplicationFailure::Invalid, &id));
      }
      let cursor: SourceCursor =
        serde_json::from_str(&after).map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
      if cursor.project_id != project {
        return Err(application_error(ApplicationFailure::Invalid, &id));
      }
      Ok(cursor.target)
    })
    .transpose()?;
  let limit = query.limit.unwrap_or(DEFAULT_PAGE_LIMIT);
  if !(1..=MAX_WORK_SOURCE_CONFIGURATION_PAGE_SIZE).contains(&limit) {
    return Err(application_error(ApplicationFailure::Invalid, &id));
  }
  let result = services(&app, &id)?
    .list
    .handle_authorized_query(
      &ctx,
      ListWorkSourceConfigurationsQuery {
        project_id: project
          .parse()
          .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
        after,
        limit,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  let next_cursor = result
    .next_cursor
    .map(|target| {
      serde_json::to_string(&SourceCursor {
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

pub(super) struct SourceReadEndpoints {
  preview: Arc<
    dyn AuthorizedManagementQueryHandler<octacity_server_application::PreviewWorkSourcesQuery, Error = ApplicationError>,
  >,
  diagnostics: Arc<
    dyn AuthorizedManagementQueryHandler<
        octacity_server_application::GetWorkSourceDiagnosticsQuery,
        Error = ApplicationError,
      >,
  >,
}
impl IntegrationManagementApplication {
  /// Installs independently authorized read-only Source previews and retained diagnostics.
  pub fn with_source_reads<Q>(mut self, queries: Arc<Q>) -> Self
  where
    Q: AuthorizedManagementQueryHandler<octacity_server_application::PreviewWorkSourcesQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<
        octacity_server_application::GetWorkSourceDiagnosticsQuery,
        Error = ApplicationError,
      > + 'static,
  {
    self.source_reads = Some(SourceReadEndpoints {
      preview: queries.clone(),
      diagnostics: queries,
    });
    self
  }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourcePreviewBody {
  intent: Value,
  limit: u16,
}
pub(crate) async fn preview_sources(
  State(app): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(id, ctx)): Extension<crate::ManagementRequest>,
  Path(project): Path<String>,
  payload: Result<Json<SourcePreviewBody>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
  let payload = body(payload, &id)?;
  if !(1..=octacity_server_application::MAX_WORK_SOURCE_PREVIEW_ITEMS).contains(&payload.limit) {
    return Err(application_error(ApplicationFailure::Invalid, &id));
  }
  let query = octacity_server_application::PreviewWorkSourcesQuery {
    intent: intent(SafeIntent { intent: payload.intent }, &project, &id)?,
    limit: payload.limit,
    observed_at: Timestamp::from_unix_millis(now_unix_ms(&id)?)
      .map_err(|_| application_error(ApplicationFailure::Internal, &id))?,
  };
  let result = app
    .source_reads
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?
    .preview
    .handle_authorized_query(&ctx, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(Json(encode(result, &id)?))
}
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct SourceDiagnosticCursor {
  project_id: String,
  target: WorkSourceScanTarget,
  position: String,
}
async fn diagnostics(
  app: Arc<IntegrationManagementApplication>,
  id: RequestId,
  ctx: ManagementRequestContext,
  path: (String, String, u64),
  query: Result<Query<SourceList>, axum::extract::rejection::QueryRejection>,
  kind: &str,
) -> Result<Json<Value>, ApiError> {
  let Query(query) = query.map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let target: WorkSourceScanTarget = serde_json::from_value(json!({"kind":kind,"id":path.1,"version":path.2}))
    .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
  let limit = query.limit.unwrap_or(DEFAULT_PAGE_LIMIT);
  if !(1..=64).contains(&limit) {
    return Err(application_error(ApplicationFailure::Invalid, &id));
  }
  let after = query
    .after
    .map(|after| {
      if after.len() > 512 {
        return Err(application_error(ApplicationFailure::Invalid, &id));
      }
      let cursor: SourceDiagnosticCursor =
        serde_json::from_str(&after).map_err(|_| application_error(ApplicationFailure::Invalid, &id))?;
      if cursor.project_id != path.0 || cursor.target != target {
        return Err(application_error(ApplicationFailure::Invalid, &id));
      }
      cursor
        .position
        .parse()
        .map_err(|_| application_error(ApplicationFailure::Invalid, &id))
    })
    .transpose()?;
  let query = octacity_server_application::GetWorkSourceDiagnosticsQuery {
    project_id: path
      .0
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &id))?,
    target,
    after,
    limit,
  };
  let result = app
    .source_reads
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?
    .diagnostics
    .handle_authorized_query(&ctx, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  let cursor = result
    .next_cursor
    .map(|position| {
      serde_json::to_string(&SourceDiagnosticCursor {
        project_id: path.0,
        target,
        position: position.to_string(),
      })
    })
    .transpose()
    .map_err(|_| application_error(ApplicationFailure::Internal, &id))?;
  let mut value = encode(result, &id)?;
  value["next_cursor"] = json!(cursor);
  Ok(Json(value))
}
pub(crate) async fn source_input_diagnostics(
  State(app): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(id, ctx)): Extension<crate::ManagementRequest>,
  Path(path): Path<(String, String, u64)>,
  query: Result<Query<SourceList>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, ApiError> {
  diagnostics(app, id, ctx, path, query, "input").await
}
pub(crate) async fn source_group_diagnostics(
  State(app): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(id, ctx)): Extension<crate::ManagementRequest>,
  Path(path): Path<(String, String, u64)>,
  query: Result<Query<SourceList>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, ApiError> {
  diagnostics(app, id, ctx, path, query, "group").await
}
