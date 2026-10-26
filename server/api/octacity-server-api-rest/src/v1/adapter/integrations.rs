//! Protected integration administration; verified ingress is required even in trusted-network deployments.
use super::*;
use axum::{
  Router,
  extract::Request,
  middleware::{self, Next},
  response::Response,
};
use octacity_server_application::{
  ApplicationError, AuthorizedManagementCommandHandler, BeginIntegrationAdministration,
  IntegrationAdministrationIntent, ManageIntegrationCredentialCommand, ManagementActorKind, ManagementIngress,
  ManagementRequestContext, ManagementRequestId, ProviderCredentialValue, Timestamp,
};
use octacity_server_application::{
  AuthorizedManagementQueryHandler, CleanupIntegrationPreparationsCommand, GetIntegrationAdministrationQuery,
  GetIntegrationConnectionQuery, PublishIntegrationConnectionCommand, SetIntegrationConnectionStateCommand,
};

mod check;
mod dependencies;
pub(super) use dependencies::list_dependencies;
mod discovery;
mod project_access;
mod project_imports;
mod project_sources;
pub(super) use project_imports::{
  get_current_project_import, get_project_import, list_project_imports, publish_project_import,
};
pub(super) use project_sources::preview_project_sources;
mod sources;
pub(super) use check::check_connection;
pub(super) use discovery::*;
use octacity_server_application::CheckIntegrationConnectionQuery;
use octacity_server_application::{
  GetIntegrationProjectAccessQuery, ListIntegrationProjectResourcesQuery, PublishIntegrationProjectAccessCommand,
};
pub(super) use project_access::{get_project_access, list_project_resources, publish_project_access};
pub(super) use sources::*;
type Credentials = dyn AuthorizedManagementCommandHandler<ManageIntegrationCredentialCommand, Error = ApplicationError>;
/// Endpoint capabilities are supplied by trusted composition, with application authorization.
pub struct IntegrationManagementApplication {
  pub(super) source_management_required: bool,
  dependencies: Option<
    Arc<
      dyn AuthorizedManagementQueryHandler<
          octacity_server_application::ListIntegrationDependenciesQuery,
          Error = ApplicationError,
        >,
    >,
  >,
  credentials: Option<Arc<Credentials>>,
  metadata: Option<MetadataEndpoints>,
  administration: Option<AdministrationEndpoints>,
  project_access: Option<ProjectAccessEndpoints>,
  sources: Option<SourceConfigurationEndpoints>,
  source_reads: Option<SourceReadEndpoints>,
  project_source_choices: Option<
    Arc<
      dyn AuthorizedManagementQueryHandler<
          octacity_server_application::ListProjectIssueSourceChoicesQuery,
          Error = ApplicationError,
        >,
    >,
  >,
  project_source_preview: Option<
    Arc<
      dyn AuthorizedManagementQueryHandler<
          octacity_server_application::PreviewProjectIssueSourcesQuery,
          Error = ApplicationError,
        >,
    >,
  >,
  project_source_imports: Option<project_imports::ProjectSourceImportEndpoints>,
  discovery: Option<DiscoveryEndpoints>,
  checks: Option<Arc<dyn AuthorizedManagementQueryHandler<CheckIntegrationConnectionQuery, Error = ApplicationError>>>,
}
struct ProjectAccessEndpoints {
  publish:
    Arc<dyn AuthorizedManagementCommandHandler<PublishIntegrationProjectAccessCommand, Error = ApplicationError>>,
  get: Arc<dyn AuthorizedManagementQueryHandler<GetIntegrationProjectAccessQuery, Error = ApplicationError>>,
  list: Arc<dyn AuthorizedManagementQueryHandler<ListIntegrationProjectResourcesQuery, Error = ApplicationError>>,
}
struct MetadataEndpoints {
  publish: Arc<dyn AuthorizedManagementCommandHandler<PublishIntegrationConnectionCommand, Error = ApplicationError>>,
  get: Arc<dyn AuthorizedManagementQueryHandler<GetIntegrationConnectionQuery, Error = ApplicationError>>,
}
struct AdministrationEndpoints {
  set: Arc<dyn AuthorizedManagementCommandHandler<SetIntegrationConnectionStateCommand, Error = ApplicationError>>,
  get: Arc<dyn AuthorizedManagementQueryHandler<GetIntegrationAdministrationQuery, Error = ApplicationError>>,
  cleanup: Arc<dyn AuthorizedManagementCommandHandler<CleanupIntegrationPreparationsCommand, Error = ApplicationError>>,
}
impl IntegrationManagementApplication {
  pub(super) fn unavailable() -> Self {
    Self {
      source_management_required: false,
      dependencies: None,
      credentials: None,
      metadata: None,
      administration: None,
      project_access: None,
      checks: None,
      sources: None,
      source_reads: None,
      project_source_choices: None,
      project_source_preview: None,
      project_source_imports: None,
      discovery: None,
    }
  }
  /// Installs the separately authorized write-only credential service.
  pub fn new<
    C: AuthorizedManagementCommandHandler<ManageIntegrationCredentialCommand, Error = ApplicationError> + 'static,
  >(
    credentials: Arc<C>,
  ) -> Self {
    Self {
      source_management_required: false,
      dependencies: None,
      credentials: Some(credentials),
      metadata: None,
      administration: None,
      project_access: None,
      checks: None,
      sources: None,
      source_reads: None,
      project_source_choices: None,
      project_source_preview: None,
      project_source_imports: None,
      discovery: None,
    }
  }
  /// Installs separately authorized, bounded read-only diagnostics without activating inputs.
  pub fn with_checks<Q>(mut self, queries: Arc<Q>) -> Self
  where
    Q: AuthorizedManagementQueryHandler<CheckIntegrationConnectionQuery, Error = ApplicationError> + 'static,
  {
    self.checks = Some(queries);
    self
  }
  /// Installs independently authorized Project grants and filtered use-resource discovery.
  pub fn with_project_access<C, Q>(mut self, commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<PublishIntegrationProjectAccessCommand, Error = ApplicationError> + 'static,
    Q: AuthorizedManagementQueryHandler<GetIntegrationProjectAccessQuery, Error = ApplicationError>
      + AuthorizedManagementQueryHandler<ListIntegrationProjectResourcesQuery, Error = ApplicationError>
      + 'static,
  {
    self.project_access = Some(ProjectAccessEndpoints {
      publish: commands,
      get: queries.clone(),
      list: queries,
    });
    self
  }
  /// Installs metadata publication/inspection without granting Project use or exposing tokens.
  pub fn with_metadata<C, Q>(mut self, commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<PublishIntegrationConnectionCommand, Error = ApplicationError> + 'static,
    Q: AuthorizedManagementQueryHandler<GetIntegrationConnectionQuery, Error = ApplicationError> + 'static,
  {
    self.metadata = Some(MetadataEndpoints {
      publish: commands,
      get: queries,
    });
    self
  }
  /// Installs independently authorized lifecycle and bounded private cleanup capabilities.
  pub fn with_administration<C, Q>(mut self, commands: Arc<C>, queries: Arc<Q>) -> Self
  where
    C: AuthorizedManagementCommandHandler<SetIntegrationConnectionStateCommand, Error = ApplicationError>
      + AuthorizedManagementCommandHandler<CleanupIntegrationPreparationsCommand, Error = ApplicationError>
      + 'static,
    Q: AuthorizedManagementQueryHandler<GetIntegrationAdministrationQuery, Error = ApplicationError> + 'static,
  {
    self.administration = Some(AdministrationEndpoints {
      set: commands.clone(),
      cleanup: commands,
      get: queries,
    });
    self
  }
}
/// Trusted ingress proof. Only a server identity/transport verifier may install
/// this extension, after authenticating the caller over TLS or an authenticated
/// local proxy channel. Client headers/forwarded protocol are never proof.
#[derive(Clone)]
pub struct ProtectedIntegrationRequest(ManagementRequestContext);
impl ProtectedIntegrationRequest {
  /// Accepts verified identity context only. The caller is responsible for the
  /// protected transport assertion; this is a composition API, not a wire DTO.
  pub fn new(context: ManagementRequestContext) -> Result<Self, &'static str> {
    if context.actor().kind() != ManagementActorKind::AuthenticatedManagement
      || context.attributes().ingress() != ManagementIngress::VerifiedIdentity
    {
      return Err("protected authenticated integration ingress required");
    }
    Ok(Self(context))
  }
  /// Preserve transport authority across ordinary and protected operations, with fresh correlation.
  pub(crate) fn context_for_request(&self, request_id: ManagementRequestId) -> ManagementRequestContext {
    ManagementRequestContext::new(
      self.0.actor().clone(),
      self.0.security_scope().clone(),
      request_id,
      self.0.attributes(),
    )
    .expect("verified context with generated request identity")
  }
}
/// Builds opt-in protected routes. Without a verified ingress extension every
/// request fails closed. Ordinary/manual management routes require no integration.
pub fn integration_management_routes(application: IntegrationManagementApplication) -> Router {
  super::routes::integration_router(application).layer(middleware::from_fn(crate::request_context))
}
pub(super) async fn protected_ingress(mut request: Request, next: Next) -> Response {
  let request_id = request
    .extensions()
    .get::<RequestId>()
    .cloned()
    .expect("request context precedes protected ingress");
  let mut response = if let Some(proof) = request.extensions_mut().remove::<ProtectedIntegrationRequest>() {
    let context = proof.context_for_request(
      request_id
        .0
        .parse::<ManagementRequestId>()
        .expect("generated request identity"),
    );
    request
      .extensions_mut()
      .insert(crate::ManagementRequest(request_id, context));
    next.run(request).await
  } else {
    ApiError::new(
      StatusCode::FORBIDDEN,
      ErrorCode::Forbidden,
      crate::v1::MANAGEMENT_FORBIDDEN_MESSAGE,
      &request_id,
    )
    .into_response()
  };
  response.headers_mut().insert(
    axum::http::header::CACHE_CONTROL,
    axum::http::HeaderValue::from_static("no-store"),
  );
  response
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CredentialSubmission {
  intent: Value,
  #[serde(default)]
  value: Option<ProviderCredentialValue>,
}
fn administration_request(
  id: &str,
  intent: Value,
  key: &IdempotencyKey,
  headers: &HeaderMap,
  request_id: &RequestId,
) -> Result<BeginIntegrationAdministration, ApiError> {
  let intent: IntegrationAdministrationIntent =
    serde_json::from_value(intent).map_err(|_| application_error(ApplicationFailure::Invalid, request_id))?;
  if intent.id.to_string() != id {
    return Err(application_error(ApplicationFailure::Invalid, request_id));
  }
  check_precondition(headers, intent.expected_revision, request_id)?;
  Ok(BeginIntegrationAdministration {
    intent,
    idempotency_key: key
      .as_str()
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, request_id))?,
    requested_at: Timestamp::from_unix_millis(now_unix_ms(request_id)?)
      .map_err(|_| application_error(ApplicationFailure::Internal, request_id))?,
  })
}
pub(super) async fn submit_credentials(
  State(application): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(id): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<CredentialSubmission>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let command = ManageIntegrationCredentialCommand {
    request: administration_request(&id, body.intent, &key, &headers, &request_id)?,
    value: body.value,
  };
  let service = application
    .credentials
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &request_id))?;
  let receipt = service
    .handle_authorized_command(&context, command)
    .await
    .map_err(|error| authorized_handler_error(error, &request_id))?;
  Ok(Json(receipt_representation(receipt, &request_id)?))
}
fn check_precondition(headers: &HeaderMap, expected: u64, request_id: &RequestId) -> Result<(), ApiError> {
  if expected == 0 {
    if headers.contains_key(axum::http::header::IF_MATCH) {
      return Err(application_error(ApplicationFailure::Invalid, request_id));
    }
    if headers
      .get(axum::http::header::IF_NONE_MATCH)
      .is_none_or(|value| value != "*")
    {
      return Err(ApiError::new(
        StatusCode::PRECONDITION_REQUIRED,
        ErrorCode::PreconditionRequired,
        "If-None-Match: * is required",
        request_id,
      ));
    }
  } else {
    if headers.contains_key(axum::http::header::IF_NONE_MATCH) {
      return Err(application_error(ApplicationFailure::Invalid, request_id));
    }
    if precondition(headers, request_id)?.version() != expected {
      return Err(application_error(ApplicationFailure::PreconditionFailed, request_id));
    }
  }
  Ok(())
}
fn receipt_representation(
  receipt: octacity_server_application::IntegrationAdministrationReceipt,
  request_id: &RequestId,
) -> Result<Value, ApiError> {
  let operation: String = receipt.operation.iter().map(|byte| format!("{byte:02x}")).collect();
  Ok(
    json!({"operation_id": operation, "integration_id": receipt.intent.id.to_string(), "revision": receipt.revision,
    "metadata_version":receipt.intent.expected_metadata_version.get(), "status":encode(receipt.status, request_id)?,
    "requested_at_unix_ms":receipt.requested_at.unix_millis()}),
  )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SafeIntent {
  intent: Value,
}
pub(super) async fn publish_metadata(
  State(application): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(id): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<SafeIntent>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
  let body = body(payload, &request_id)?;
  let intent: octacity_server_application::IntegrationConnectionIntent =
    serde_json::from_value(body.intent).map_err(|_| application_error(ApplicationFailure::Invalid, &request_id))?;
  if intent.id.to_string() != id {
    return Err(application_error(ApplicationFailure::Invalid, &request_id));
  }
  check_precondition(
    &headers,
    intent.expected_current_version.map_or(0, |v| v.get()),
    &request_id,
  )?;
  let key = idempotency_key(&headers, &request_id)?;
  let command = PublishIntegrationConnectionCommand {
    intent,
    idempotency_key: key
      .as_str()
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &request_id))?,
    published_at: Timestamp::from_unix_millis(now_unix_ms(&request_id)?)
      .map_err(|_| application_error(ApplicationFailure::Internal, &request_id))?,
  };
  let service = application
    .metadata
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &request_id))?;
  let outcome = service
    .publish
    .handle_authorized_command(&context, command)
    .await
    .map_err(|e| authorized_handler_error(e, &request_id))?;
  Ok(Json(
    json!({"disposition":encode(outcome.disposition, &request_id)?, "connection":connection_representation(outcome.connection, &request_id)?}),
  ))
}
pub(super) fn connection_representation(
  connection: octacity_server_application::PublishedIntegrationConnection,
  request_id: &RequestId,
) -> Result<Value, ApiError> {
  Ok(
    json!({"id":connection.id.to_string(),"version":connection.version.get(),"digest":connection.digest.to_string(),
    "definition":encode(connection.definition, request_id)?,"published_at_unix_ms":connection.published_at.unix_millis()}),
  )
}
async fn metadata_response(
  application: &IntegrationManagementApplication,
  context: &ManagementRequestContext,
  request_id: &RequestId,
  id: &str,
  version: Option<u64>,
) -> Result<Json<Value>, ApiError> {
  let query = GetIntegrationConnectionQuery {
    id: id
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, request_id))?,
    version: version
      .map(|v| serde_json::from_value(json!(v)))
      .transpose()
      .map_err(|_| application_error(ApplicationFailure::Invalid, request_id))?,
  };
  let service = application
    .metadata
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, request_id))?;
  let connection = service
    .get
    .handle_authorized_query(context, query)
    .await
    .map_err(|e| authorized_handler_error(e, request_id))?;
  Ok(Json(connection_representation(connection, request_id)?))
}
pub(super) async fn get_metadata(
  State(application): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
  metadata_response(&application, &context, &request_id, &id, None).await
}
pub(super) async fn get_metadata_version(
  State(application): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path((id, version)): Path<(String, u64)>,
) -> Result<Json<Value>, ApiError> {
  metadata_response(&application, &context, &request_id, &id, Some(version)).await
}
pub(super) async fn set_state(
  State(application): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(id): Path<String>,
  headers: HeaderMap,
  payload: Result<Json<SafeIntent>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
  let body = body(payload, &request_id)?;
  let key = idempotency_key(&headers, &request_id)?;
  let command = SetIntegrationConnectionStateCommand {
    request: administration_request(&id, body.intent, &key, &headers, &request_id)?,
  };
  let service = application
    .administration
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &request_id))?;
  let receipt = service
    .set
    .handle_authorized_command(&context, command)
    .await
    .map_err(|e| authorized_handler_error(e, &request_id))?;
  Ok(Json(receipt_representation(receipt, &request_id)?))
}
pub(super) async fn get_state(
  State(application): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
  let query = GetIntegrationAdministrationQuery {
    id: id
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &request_id))?,
  };
  let service = application
    .administration
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &request_id))?;
  let state = service
    .get
    .handle_authorized_query(&context, query)
    .await
    .map_err(|e| authorized_handler_error(e, &request_id))?;
  Ok(Json(
    json!({"id":state.id.to_string(), "revision":state.revision,"enabled":state.enabled,"active_version":state.active_version.map(|v| v.get()),
    "credentials":encode(state.credentials, &request_id)?,"preparation_pending":state.pending.is_some()}),
  ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CleanupRequest {
  limit: u16,
}
pub(super) async fn cleanup_preparations(
  State(application): State<Arc<IntegrationManagementApplication>>,
  Extension(crate::ManagementRequest(request_id, context)): Extension<crate::ManagementRequest>,
  Path(id): Path<String>,
  payload: Result<Json<CleanupRequest>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
  let body = body(payload, &request_id)?;
  if body.limit == 0 || body.limit > 100 {
    return Err(application_error(ApplicationFailure::Invalid, &request_id));
  }
  // Clients cannot expire another in-flight operation by submitting a future cutoff.
  let now = now_unix_ms(&request_id)?;
  let cutoff = now.saturating_sub(15 * 60 * 1000);
  let command = CleanupIntegrationPreparationsCommand {
    requested_at: Timestamp::from_unix_millis(now)
      .map_err(|_| application_error(ApplicationFailure::Internal, &request_id))?,
    id: id
      .parse()
      .map_err(|_| application_error(ApplicationFailure::Invalid, &request_id))?,
    older_than: Timestamp::from_unix_millis(cutoff)
      .map_err(|_| application_error(ApplicationFailure::Internal, &request_id))?,
    limit: body.limit,
  };
  let service = application
    .administration
    .as_ref()
    .ok_or_else(|| application_error(ApplicationFailure::Unavailable, &request_id))?;
  let cleaned = service
    .cleanup
    .handle_authorized_command(&context, command)
    .await
    .map_err(|e| authorized_handler_error(e, &request_id))?;
  Ok(Json(json!({"cleaned":cleaned})))
}
