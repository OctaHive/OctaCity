//! Optional Source HTTP composition over authorized application contracts.
use super::*;
use octacity_server_application::{
  ApplicationError, AttachIssueSourceCommand, AuthorizedManagementCommandHandler, AuthorizedManagementQueryHandler,
  GetIssueSourceChoicesQuery, GetIssueSourceMappingQuery, GetIssueSynchronizationQuery,
  ListIssueSourceProjectionsQuery, ObserveIssueSourceCommand, PreviewIssueSourceCommand,
  PublishIssueSourceMappingCommand, RefreshIssueSourceCommand, ResolveIssueSourceConflictCommand,
  SetIssueSourcePullCommand,
};

/// Installs task Source review independently of admission and provider availability.
pub struct IssueSourceManagementApplication {
  pub(super) protected_ingress: bool,
  projections:
    Option<Arc<dyn AuthorizedManagementQueryHandler<ListIssueSourceProjectionsQuery, Error = ApplicationError>>>,
  review: Arc<dyn AuthorizedManagementQueryHandler<GetIssueSynchronizationQuery, Error = ApplicationError>>,
  choices: Arc<dyn AuthorizedManagementQueryHandler<GetIssueSourceChoicesQuery, Error = ApplicationError>>,
  mapping: Arc<dyn AuthorizedManagementQueryHandler<GetIssueSourceMappingQuery, Error = ApplicationError>>,
  commands: Option<IssueSourceCommands>,
  resolution:
    Option<Arc<dyn AuthorizedManagementCommandHandler<ResolveIssueSourceConflictCommand, Error = ApplicationError>>>,
}
struct IssueSourceCommands {
  attach: Arc<dyn AuthorizedManagementCommandHandler<AttachIssueSourceCommand, Error = ApplicationError>>,
  preview: Arc<dyn AuthorizedManagementCommandHandler<PreviewIssueSourceCommand, Error = ApplicationError>>,
  mapping: Arc<dyn AuthorizedManagementCommandHandler<PublishIssueSourceMappingCommand, Error = ApplicationError>>,
  refresh: Arc<dyn AuthorizedManagementCommandHandler<ObserveIssueSourceCommand, Error = ApplicationError>>,
  pull: Arc<dyn AuthorizedManagementCommandHandler<SetIssueSourcePullCommand, Error = ApplicationError>>,
}
impl IssueSourceManagementApplication {
  /// Uses retained reads, even when no provider capability is installed.
  pub fn new<Q>(queries: Arc<Q>) -> Self
  where
    Q: AuthorizedManagementQueryHandler<GetIssueSynchronizationQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<GetIssueSourceChoicesQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<GetIssueSourceMappingQuery, Error = ApplicationError>
      + 'static,
  {
    Self {
      protected_ingress: false,
      projections: None,
      review: queries.clone(),
      choices: queries.clone(),
      mapping: queries,
      commands: None,
      resolution: None,
    }
  }
  /// Requires the same authenticated transport proof as Project Source controls when installed.
  pub fn with_protected_ingress(mut self, required: bool) -> Self {
    self.protected_ingress = required;
    self
  }
  /// Installs retained Reporter reads independently of provider and manual-task capabilities.
  pub fn with_projection_queries(
    mut self,
    handler: Arc<dyn AuthorizedManagementQueryHandler<ListIssueSourceProjectionsQuery, Error = ApplicationError>>,
  ) -> Self {
    self.projections = Some(handler);
    self
  }
  /// Installs reviewed conflict commands independently of ordinary manual-task services.
  pub fn with_resolution(
    mut self,
    handler: Arc<dyn AuthorizedManagementCommandHandler<ResolveIssueSourceConflictCommand, Error = ApplicationError>>,
  ) -> Self {
    self.resolution = Some(handler);
    self
  }
  /// Installs explicit reviewed Source mutations; constructing services starts no read or worker.
  pub fn with_commands<C>(mut self, commands: Arc<C>) -> Self
  where
    C: AuthorizedManagementCommandHandler<AttachIssueSourceCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<PreviewIssueSourceCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<PublishIssueSourceMappingCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<ObserveIssueSourceCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<SetIssueSourcePullCommand, Error = ApplicationError>
      + 'static,
  {
    self.commands = Some(IssueSourceCommands {
      attach: commands.clone(),
      preview: commands.clone(),
      mapping: commands.clone(),
      refresh: commands.clone(),
      pull: commands,
    });
    self
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProjectionParameters {
  after: Option<String>,
  limit: Option<u16>,
}
pub(super) async fn projections(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  parameters: Result<Query<ProjectionParameters>, QueryRejection>,
) -> Result<axum::response::Response, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let Query(parameters) = parameters.map_err(|_| invalid())?;
  let query = ListIssueSourceProjectionsQuery::try_new(
    project.parse().map_err(|_| invalid())?,
    issue.parse().map_err(|_| invalid())?,
    parameters.after,
    parameters.limit.unwrap_or(DEFAULT_PAGE_LIMIT),
  )
  .map_err(|e| application_error(e.classification(), &id))?;
  let handler = app
    .issue_sources
    .as_ref()
    .and_then(|s| s.projections.as_ref())
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  let page = handler
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([(axum::http::header::CACHE_CONTROL, "no-store")], Json(page)).into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PullRequest {
  expected_generation: u64,
  enabled: bool,
}
pub(super) async fn pull(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<PullRequest>, JsonRejection>,
) -> Result<axum::response::Response, ApiError> {
  let key = idempotency_key(&headers, &id)?;
  let request = body(payload, &id)?;
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let services = app
    .issue_sources
    .as_ref()
    .and_then(|s| s.commands.as_ref())
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  let result = services
    .pull
    .handle_authorized_command(
      &context,
      SetIssueSourcePullCommand {
        project_id: project.parse().map_err(|_| invalid())?,
        issue_id: issue.parse().map_err(|_| invalid())?,
        expected_generation: request.expected_generation,
        enabled: request.enabled,
        idempotency_key: key.as_str().parse().map_err(|_| invalid())?,
        changed_at: octacity_server_application::Timestamp::from_unix_millis(now_unix_ms(&id)?)
          .map_err(|_| invalid())?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(result, &id)?)).into_response())
}

fn read_request(
  project_id: octacity_server_application::ProjectId,
  issue_id: octacity_server_application::IssueId,
  source: octacity_server_application::WorkSourceBinding,
  key: &IdempotencyKey,
  scan_id: Option<octacity_server_application::FactoryDigest>,
  continuation: Option<String>,
) -> octacity_server_application::ReadWorkSourceObservations {
  octacity_server_application::ReadWorkSourceObservations {
    connection: None,
    project_id,
    source,
    window: None,
    continuation,
    scan_id: scan_id.unwrap_or_else(|| {
      octacity_server_application::FactoryDigest::content_sha256(
        format!("issue-source-read:{project_id}:{issue_id}:{}", key.as_str()).as_bytes(),
      )
    }),
    limit: octacity_server_application::MAX_SOURCE_POLL_PAGE_ITEMS,
  }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RefreshRequest {
  source: octacity_server_application::WorkSourceBinding,
  continuation: Option<String>,
}
pub(super) async fn refresh(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<RefreshRequest>, JsonRejection>,
) -> Result<axum::response::Response, ApiError> {
  let key = idempotency_key(&headers, &id)?;
  let request = body(payload, &id)?;
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let project_id = project.parse().map_err(|_| invalid())?;
  let issue_id = issue.parse().map_err(|_| invalid())?;
  let services = app
    .issue_sources
    .as_ref()
    .and_then(|s| s.commands.as_ref())
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  let result = services
    .refresh
    .handle_authorized_command(
      &context,
      ObserveIssueSourceCommand {
        refresh: RefreshIssueSourceCommand {
          project_id,
          issue_id,
          read: read_request(project_id, issue_id, request.source, &key, None, request.continuation),
          idempotency_key: key.as_str().parse().map_err(|_| invalid())?,
          refreshed_at: octacity_server_application::Timestamp::from_unix_millis(now_unix_ms(&id)?)
            .map_err(|_| invalid())?,
        },
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(result, &id)?)).into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MappingRequest {
  expected_mapping: Option<octacity_server_application::ImmutableReference>,
  definition: octacity_server_application::IssueSourceMappingDefinition,
}
pub(super) async fn publish_mapping(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<MappingRequest>, JsonRejection>,
) -> Result<axum::response::Response, ApiError> {
  use octacity_server_application::{IssueVersion, PublishIssueSourceMappingIntent, Timestamp};
  let expected = precondition(&headers, &id)?;
  let key = idempotency_key(&headers, &id)?;
  let request = body(payload, &id)?;
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let services = app
    .issue_sources
    .as_ref()
    .and_then(|s| s.commands.as_ref())
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  let result = services
    .mapping
    .handle_authorized_command(
      &context,
      PublishIssueSourceMappingCommand {
        intent: PublishIssueSourceMappingIntent {
          project_id: project.parse().map_err(|_| invalid())?,
          issue_id: issue.parse().map_err(|_| invalid())?,
          expected_issue_version: IssueVersion::new(expected.version()).map_err(|_| invalid())?,
          expected_mapping: request.expected_mapping,
          definition: request.definition,
        },
        idempotency_key: key.as_str().parse().map_err(|_| invalid())?,
        published_at: Timestamp::from_unix_millis(now_unix_ms(&id)?).map_err(|_| invalid())?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  mapping_response(result, &id)
}
fn mapping_response(
  result: <PublishIssueSourceMappingCommand as octacity_server_application::Command>::Outcome,
  id: &RequestId,
) -> Result<axum::response::Response, ApiError> {
  use octacity_server_application::StoreMutationDisposition;
  let status = if result.disposition == StoreMutationDisposition::Applied {
    StatusCode::CREATED
  } else {
    StatusCode::OK
  };
  let mut response = json!({"disposition":encode(result.disposition,id)?,"mapping_reference":encode(result.mapping.reference(),id)?,"resource":encode(result.mapping,id)?});
  if let Some(projection) = result.projection {
    response["projection"] = encode(projection, id)?;
  }
  Ok((status, [("cache-control", "no-store")], Json(response)).into_response())
}
pub(super) async fn mapping_version(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue, version)): Path<(String, String, String)>,
) -> Result<axum::response::Response, ApiError> {
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let services = app
    .issue_sources
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  let result = services
    .mapping
    .handle_authorized_query(
      &context,
      GetIssueSourceMappingQuery {
        project_id: project.parse().map_err(|_| invalid())?,
        issue_id: issue.parse().map_err(|_| invalid())?,
        version: octacity_server_application::IssueSourceMappingVersion::new(version.parse().map_err(|_| invalid())?)
          .map_err(|_| invalid())?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(result, &id)?)).into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PreviewRequest {
  source: octacity_server_application::WorkSourceBinding,
  scan_id: Option<octacity_server_application::FactoryDigest>,
  continuation: Option<String>,
}
pub(super) async fn preview(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  payload: Result<Json<PreviewRequest>, JsonRejection>,
) -> Result<axum::response::Response, ApiError> {
  let request = body(payload, &id)?;
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let services = app
    .issue_sources
    .as_ref()
    .and_then(|s| s.commands.as_ref())
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  let result = services
    .preview
    .handle_authorized_command(
      &context,
      PreviewIssueSourceCommand {
        project_id: project.parse().map_err(|_| invalid())?,
        issue_id: issue.parse().map_err(|_| invalid())?,
        source: request.source,
        scan_id: request.scan_id,
        continuation: request.continuation,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(&result, &id)?)).into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AttachRequest {
  source: octacity_server_application::WorkSourceBinding,
  item: octacity_server_application::WorkSourceItem,
  expected_field_policy: octacity_server_application::IssueFieldPolicy,
  scan_id: Option<octacity_server_application::FactoryDigest>,
  continuation: Option<String>,
}
pub(super) async fn attach(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<AttachRequest>, JsonRejection>,
) -> Result<axum::response::Response, ApiError> {
  use octacity_server_application::{
    AttachIssueSourceIntent, IssueSourceOutcome, IssueSourceSelection, IssueVersion, StoreMutationDisposition,
    Timestamp,
  };
  let expected = precondition(&headers, &id)?;
  let key = idempotency_key(&headers, &id)?;
  let request = body(payload, &id)?;
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let project_id = project.parse().map_err(|_| invalid())?;
  let issue_id = issue.parse().map_err(|_| invalid())?;
  let read = read_request(
    project_id,
    issue_id,
    request.source.clone(),
    &key,
    request.scan_id,
    request.continuation,
  );
  let services = app
    .issue_sources
    .as_ref()
    .and_then(|s| s.commands.as_ref())
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  let outcome = services
    .attach
    .handle_authorized_command(
      &context,
      AttachIssueSourceCommand {
        intent: AttachIssueSourceIntent {
          selection: IssueSourceSelection {
            connection: None,
            project_id,
            source: request.source,
            item: request.item,
          },
          issue_id,
          expected_version: IssueVersion::new(expected.version()).map_err(|_| invalid())?,
          expected_field_policy: request.expected_field_policy,
        },
        read,
        idempotency_key: key.as_str().parse().map_err(|_| invalid())?,
        attached_at: Timestamp::from_unix_millis(now_unix_ms(&id)?).map_err(|_| invalid())?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  let status = match &outcome {
    IssueSourceOutcome::Linked {
      disposition: StoreMutationDisposition::Applied,
      ..
    } => StatusCode::CREATED,
    IssueSourceOutcome::Linked { .. } => StatusCode::OK,
    IssueSourceOutcome::Conflict { .. } => StatusCode::OK,
  };
  Ok((status, [("cache-control", "no-store")], Json(encode(&outcome, &id)?)).into_response())
}
pub(super) async fn choices(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
) -> Result<axum::response::Response, ApiError> {
  let services = app
    .issue_sources
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let result = services
    .choices
    .handle_authorized_query(
      &context,
      GetIssueSourceChoicesQuery {
        project_id: project.parse().map_err(|_| invalid())?,
        issue_id: issue.parse().map_err(|_| invalid())?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(&result, &id)?)).into_response())
}
impl ManagementApplication {
  /// Adds optional Source services without changing manual task handlers.
  pub fn with_issue_sources(mut self, sources: IssueSourceManagementApplication) -> Self {
    self.issue_sources = Some(sources);
    self
  }
}
pub(super) async fn review(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
) -> Result<axum::response::Response, ApiError> {
  let services = app
    .issue_sources
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let result = services
    .review
    .handle_authorized_query(
      &context,
      GetIssueSynchronizationQuery {
        project_id: project.parse().map_err(|_| invalid())?,
        issue_id: issue.parse().map_err(|_| invalid())?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(&result, &id)?)).into_response())
}
pub(super) async fn link(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
) -> Result<axum::response::Response, ApiError> {
  let services = app
    .issue_sources
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let result = services
    .review
    .handle_authorized_query(
      &context,
      GetIssueSynchronizationQuery {
        project_id: project.parse().map_err(|_| invalid())?,
        issue_id: issue.parse().map_err(|_| invalid())?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  Ok(([("cache-control", "no-store")], Json(encode(&result.link, &id)?)).into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResolutionRequest {
  mapping: MappingRequest,
  review: octacity_server_application::IssueSourceResolutionReview,
}
pub(super) async fn resolve(
  State(app): State<Arc<ManagementApplication>>,
  Extension(crate::ManagementRequest(id, context)): Extension<crate::ManagementRequest>,
  Path((project, issue)): Path<(String, String)>,
  headers: HeaderMap,
  payload: Result<Json<ResolutionRequest>, JsonRejection>,
) -> Result<axum::response::Response, ApiError> {
  use octacity_server_application::{
    IssueVersion, PublishIssueSourceMappingIntent, ResolveIssueSourceConflictIntent, Timestamp,
  };
  let expected = precondition(&headers, &id)?;
  let key = idempotency_key(&headers, &id)?;
  let request = body(payload, &id)?;
  let invalid = || application_error(ApplicationFailure::Invalid, &id);
  let handler = app
    .issue_sources
    .as_ref()
    .and_then(|s| s.resolution.as_ref())
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &id))?;
  let result = handler
    .handle_authorized_command(
      &context,
      ResolveIssueSourceConflictCommand {
        intent: ResolveIssueSourceConflictIntent {
          review: request.review,
          mapping: PublishIssueSourceMappingIntent {
            project_id: project.parse().map_err(|_| invalid())?,
            issue_id: issue.parse().map_err(|_| invalid())?,
            expected_issue_version: IssueVersion::new(expected.version()).map_err(|_| invalid())?,
            expected_mapping: request.mapping.expected_mapping,
            definition: request.mapping.definition,
          },
        },
        idempotency_key: key.as_str().parse().map_err(|_| invalid())?,
        resolved_at: Timestamp::from_unix_millis(now_unix_ms(&id)?).map_err(|_| invalid())?,
      },
    )
    .await
    .map_err(|e| authorized_handler_error(e, &id))?;
  if result.projection.is_none() {
    return Err(application_error(ApplicationFailure::Unavailable, &id));
  }
  mapping_response(result, &id)
}
