//! Authenticated HTTP adapter for the shared server-Agent protocol.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::{fmt::Write as _, sync::Arc, time::SystemTime};

use axum::{
  Json, Router,
  body::Body,
  extract::{DefaultBodyLimit, FromRequest, Path, Request, State, rejection::JsonRejection},
  http::{HeaderMap, StatusCode, header},
  middleware,
  response::{IntoResponse, Response},
  routing::post,
};
use octacity_protocol::{
  AcquireLeaseRequest, AcquireLeaseResponse, AgentCredentialToken, AppendEventsRequest, AppendEventsResponse,
  AuthorizeToolActionRequest, AuthorizeToolActionResponse, COORDINATOR_PROTOCOL_VERSION, CompleteLeaseRequest,
  CompleteLeaseResponse, CoordinatorErrorResponse, HeartbeatDirective, HeartbeatRequest, HeartbeatResponse,
  LeaseAssignment, MAX_AGENT_TELEMETRY_REQUEST_BYTES, RegisterAgentRequest, RegisterAgentResponse,
};
use octacity_server_application::{
  AcquireAgentLeaseInput, AgentArtifactError, AgentArtifactTransferUseCases, AgentCacheSessionUseCases,
  AgentExecutionError, AgentExecutionUseCases, AgentHeartbeatError, AgentHeartbeatInput, AgentHeartbeatUseCases,
  AgentLeaseError, AgentLeaseOutcome, AgentLeaseUseCases, AgentRegistrationError, AgentRegistrationInput,
  AgentRegistrationUseCases, AgentToolActionError, AgentToolActionInput, AgentToolActionUseCases,
  AppendAgentEventsInput, AuthorizeProtectedInputsInput, CompleteAgentLeaseInput, FailAgentLeaseAssignmentInput,
  LeaseHeartbeatOutcome, Timestamp,
};

mod artifact;
mod cache;
mod ready_job;
mod request_observability;
mod telemetry;

pub use ready_job::ReadyJobNotificationHub;
pub use telemetry::{AgentTelemetryIngress, AgentTelemetryPolicy};

const IDEMPOTENCY_KEY: &str = "idempotency-key";
const MAX_AGENT_BODY_BYTES: usize = 512 * 1024;

/// Explicit policy returned to an Agent after successful registration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AgentApiConfig {
  max_retry_delay_ms: u64,
  no_work_retry_delay_ms: u64,
}

impl AgentApiConfig {
  /// Validates the retry ceiling advertised on the coordinator protocol.
  pub const fn new(max_retry_delay_ms: u64) -> Option<Self> {
    if max_retry_delay_ms == 0 {
      None
    } else {
      Some(Self {
        max_retry_delay_ms,
        no_work_retry_delay_ms: max_retry_delay_ms,
      })
    }
  }
}

/// Complete application dependencies required by the Agent protocol surface.
#[derive(Clone)]
pub struct AgentRouterDependencies {
  registrations: Arc<dyn AgentRegistrationUseCases>,
  leases: Arc<dyn AgentLeaseUseCases>,
  heartbeats: Arc<dyn AgentHeartbeatUseCases>,
  telemetry: AgentTelemetryIngress,
  execution: Arc<dyn AgentExecutionUseCases>,
  artifacts: Arc<dyn AgentArtifactTransferUseCases>,
  cache: Arc<dyn AgentCacheSessionUseCases>,
  tool_actions: Option<Arc<dyn AgentToolActionUseCases>>,
}

impl AgentRouterDependencies {
  /// Creates the complete dependency set for all registered Agent routes.
  pub fn new(
    registrations: Arc<dyn AgentRegistrationUseCases>,
    leases: Arc<dyn AgentLeaseUseCases>,
    heartbeats: Arc<dyn AgentHeartbeatUseCases>,
    telemetry: AgentTelemetryIngress,
    execution: Arc<dyn AgentExecutionUseCases>,
    artifacts: Arc<dyn AgentArtifactTransferUseCases>,
    cache: Arc<dyn AgentCacheSessionUseCases>,
  ) -> Self {
    Self {
      registrations,
      leases,
      heartbeats,
      telemetry,
      execution,
      artifacts,
      cache,
      tool_actions: None,
    }
  }

  /// Replaces the fail-closed default with the configured server-side policy.
  #[must_use]
  pub fn with_tool_actions(mut self, tool_actions: Arc<dyn AgentToolActionUseCases>) -> Self {
    self.tool_actions = Some(tool_actions);
    self
  }
}

/// Builds the independently authenticated Agent ingress routes.
pub fn agent_router(dependencies: AgentRouterDependencies, config: AgentApiConfig) -> Router {
  let AgentRouterDependencies {
    registrations,
    leases,
    heartbeats,
    telemetry,
    execution,
    artifacts,
    cache,
    tool_actions,
  } = dependencies;
  let state = AgentState {
    registrations,
    leases,
    heartbeats,
    telemetry,
    execution,
    artifacts,
    cache,
    tool_actions,
    config,
  };
  Router::new()
    .route("/api/v1/agents/register", post(register_agent))
    .route("/api/v1/agents/{agent_id}/leases:acquire", post(acquire_lease))
    .route("/api/v1/leases/{lease_id}/heartbeat", post(heartbeat))
    .route(
      "/api/v1/agents/telemetry:ingest",
      post(telemetry::ingest).layer(DefaultBodyLimit::max(MAX_AGENT_TELEMETRY_REQUEST_BYTES)),
    )
    .route("/api/v1/leases/{lease_id}/events:append", post(append_events))
    .route(
      "/api/v1/leases/{lease_id}/tool-actions:authorize",
      post(authorize_tool_action),
    )
    .route("/api/v1/leases/{lease_id}/complete", post(complete_lease))
    .route(
      "/api/v1/leases/{lease_id}/artifacts:begin",
      post(artifact::begin_upload),
    )
    .route(
      "/api/v1/leases/{lease_id}/artifacts:complete",
      post(artifact::complete_upload),
    )
    .route("/api/v1/leases/{lease_id}/cache:begin", post(cache::begin_session))
    .route("/api/v1/leases/{lease_id}/cache:revoke", post(cache::revoke_session))
    .layer(DefaultBodyLimit::max(MAX_AGENT_BODY_BYTES))
    .with_state(state)
    .layer(middleware::from_fn(request_observability::request_context))
}

#[derive(Clone)]
struct AgentState {
  registrations: Arc<dyn AgentRegistrationUseCases>,
  leases: Arc<dyn AgentLeaseUseCases>,
  heartbeats: Arc<dyn AgentHeartbeatUseCases>,
  telemetry: AgentTelemetryIngress,
  execution: Arc<dyn AgentExecutionUseCases>,
  artifacts: Arc<dyn AgentArtifactTransferUseCases>,
  cache: Arc<dyn AgentCacheSessionUseCases>,
  tool_actions: Option<Arc<dyn AgentToolActionUseCases>>,
  config: AgentApiConfig,
}

async fn authorize_tool_action(
  State(state): State<AgentState>,
  Path(lease_id): Path<String>,
  request: Request<Body>,
) -> Response {
  let headers = request.headers().clone();
  let parsed = match Json::<AuthorizeToolActionRequest>::from_request(request, &()).await {
    Ok(Json(request)) => request,
    Err(rejection) => return rejection_response(rejection),
  };
  let request_id = parsed.request_id.clone();
  let binding_request = parsed.clone();
  let Some(tool_actions) = state.tool_actions.as_ref() else {
    return tool_action_error(&request_id, AgentToolActionError::Unavailable);
  };
  let (credential, observed_at_unix_ms) = match operation_context(&headers, &request_id) {
    Ok(context) => context,
    Err(error) => return operation_context_error(&request_id, error),
  };
  match tool_actions
    .authorize_tool_action(AgentToolActionInput {
      route_lease_id: lease_id,
      request: parsed,
      credential,
      observed_at_unix_ms,
    })
    .await
  {
    Ok(decision) => match AuthorizeToolActionResponse::new(&binding_request, decision) {
      Ok(response) => Json(response).into_response(),
      Err(_) => tool_action_error(&request_id, AgentToolActionError::Unavailable),
    },
    Err(error) => tool_action_error(&request_id, error),
  }
}

async fn append_events(
  State(state): State<AgentState>,
  Path(lease_id): Path<String>,
  request: Request<Body>,
) -> Response {
  let headers = request.headers().clone();
  let parsed = match Json::<AppendEventsRequest>::from_request(request, &()).await {
    Ok(Json(request)) => request,
    Err(rejection) => return rejection_response(rejection),
  };
  let request_id = parsed.request_id.clone();
  let (credential, observed_at_unix_ms) = match operation_context(&headers, &request_id) {
    Ok(context) => context,
    Err(error) => return operation_context_error(&request_id, error),
  };
  match state
    .execution
    .append_events(AppendAgentEventsInput {
      route_lease_id: lease_id,
      request: parsed,
      credential,
      observed_at_unix_ms,
    })
    .await
  {
    Ok(acknowledged_sequence) => Json(AppendEventsResponse {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id,
      acknowledged_sequence,
    })
    .into_response(),
    Err(error) => execution_error(&request_id, error),
  }
}

async fn complete_lease(
  State(state): State<AgentState>,
  Path(lease_id): Path<String>,
  request: Request<Body>,
) -> Response {
  let headers = request.headers().clone();
  let parsed = match Json::<CompleteLeaseRequest>::from_request(request, &()).await {
    Ok(Json(request)) => request,
    Err(rejection) => return rejection_response(rejection),
  };
  let request_id = parsed.request_id.clone();
  let completion_id = parsed.completion_id.clone();
  let (credential, observed_at_unix_ms) = match operation_context(&headers, &request_id) {
    Ok(context) => context,
    Err(error) => return operation_context_error(&request_id, error),
  };
  match state
    .execution
    .complete_lease(CompleteAgentLeaseInput {
      route_lease_id: lease_id,
      request: parsed,
      credential,
      observed_at_unix_ms,
    })
    .await
  {
    Ok(()) => Json(CompleteLeaseResponse {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id,
      completion_id,
    })
    .into_response(),
    Err(error) => execution_error(&request_id, error),
  }
}

async fn heartbeat(State(state): State<AgentState>, Path(lease_id): Path<String>, request: Request<Body>) -> Response {
  let headers = request.headers().clone();
  let parsed = match Json::<HeartbeatRequest>::from_request(request, &()).await {
    Ok(Json(request)) => request,
    Err(rejection) => return rejection_response(rejection),
  };
  let request_id = parsed.request_id.clone();
  let (credential, observed_at_unix_ms) = match operation_context(&headers, &request_id) {
    Ok(context) => context,
    Err(error) => return operation_context_error(&request_id, error),
  };
  match state
    .heartbeats
    .heartbeat(AgentHeartbeatInput {
      route_lease_id: lease_id,
      request: parsed,
      credential,
      observed_at_unix_ms,
    })
    .await
  {
    Ok(outcome) => match heartbeat_directive(outcome) {
      Some(directive) => Json(HeartbeatResponse {
        protocol_version: COORDINATOR_PROTOCOL_VERSION,
        request_id,
        directive,
      })
      .into_response(),
      None => heartbeat_error(&request_id, AgentHeartbeatError::Unavailable),
    },
    Err(error) => heartbeat_error(&request_id, error),
  }
}

fn heartbeat_directive(outcome: LeaseHeartbeatOutcome) -> Option<HeartbeatDirective> {
  match outcome {
    LeaseHeartbeatOutcome::Continue { expires_at } => Some(HeartbeatDirective::Continue {
      expires_at: u64::try_from(expires_at.unix_millis().div_euclid(1_000)).ok()?,
    }),
    LeaseHeartbeatOutcome::Cancel => Some(HeartbeatDirective::Cancel),
    LeaseHeartbeatOutcome::Fenced => Some(HeartbeatDirective::Fenced),
    LeaseHeartbeatOutcome::Drain { expires_at } => Some(HeartbeatDirective::Drain {
      expires_at: u64::try_from(expires_at.unix_millis().div_euclid(1_000)).ok()?,
    }),
  }
}

async fn register_agent(State(state): State<AgentState>, request: Request<Body>) -> Response {
  let headers = request.headers().clone();
  let parsed = match Json::<RegisterAgentRequest>::from_request(request, &()).await {
    Ok(Json(request)) => request,
    Err(rejection) => return rejection_response(rejection),
  };
  let request_id = parsed.request_id.clone();
  let (credential, observed_at_unix_ms) = match operation_context(&headers, &request_id) {
    Ok(context) => context,
    Err(error) => return operation_context_error(&request_id, error),
  };
  match state
    .registrations
    .register(AgentRegistrationInput {
      request: parsed,
      credential,
      observed_at_unix_ms,
    })
    .await
  {
    Ok(outcome) => Json(RegisterAgentResponse {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id,
      registration_id: outcome.registration_id,
      execution_contract_version: (outcome.execution_contract_version > octacity_protocol::EXECUTION_CONTRACT_V1)
        .then_some(outcome.execution_contract_version),
      max_retry_delay_ms: state.config.max_retry_delay_ms,
    })
    .into_response(),
    Err(error) => application_error(&request_id, error),
  }
}

async fn acquire_lease(
  State(state): State<AgentState>,
  Path(agent_id): Path<String>,
  request: Request<Body>,
) -> Response {
  let headers = request.headers().clone();
  let parsed = match Json::<AcquireLeaseRequest>::from_request(request, &()).await {
    Ok(Json(request)) => request,
    Err(rejection) => return rejection_response(rejection),
  };
  let request_id = parsed.request_id.clone();
  let (credential, observed_at_unix_ms) = match operation_context(&headers, &request_id) {
    Ok(context) => context,
    Err(error) => return operation_context_error(&request_id, error),
  };
  match state
    .leases
    .acquire(AcquireAgentLeaseInput {
      route_agent_id: agent_id,
      request: parsed,
      credential,
      observed_at_unix_ms,
    })
    .await
  {
    Ok(AgentLeaseOutcome::Lease(grant)) => {
      let Some(assignment_observed_at_unix_ms) = unix_now_millis() else {
        return protocol_error(
          StatusCode::SERVICE_UNAVAILABLE,
          &request_id,
          "unavailable",
          "lease assignment clock is unavailable",
          true,
        );
      };
      let assignment_observed_at_unix_ms = assignment_observed_at_unix_ms.max(grant.claimed_at.unix_millis());
      match assignment(&state, &grant, assignment_observed_at_unix_ms).await {
        Ok(lease) => Json(AcquireLeaseResponse::Lease {
          protocol_version: COORDINATOR_PROTOCOL_VERSION,
          request_id,
          lease: Box::new(lease),
        })
        .into_response(),
        Err(error) if error.requires_compensation() => {
          if state
            .leases
            .fail_assignment(FailAgentLeaseAssignmentInput {
              grant: *grant,
              observed_at_unix_ms: assignment_observed_at_unix_ms,
            })
            .await
            .is_err()
          {
            protocol_error(
              StatusCode::SERVICE_UNAVAILABLE,
              &request_id,
              "unavailable",
              "lease assignment recovery is temporarily unavailable",
              true,
            )
          } else {
            assignment_error(&request_id, error)
          }
        }
        Err(error) => assignment_error(&request_id, error),
      }
    }
    Ok(AgentLeaseOutcome::Drain) => Json(AcquireLeaseResponse::Drain {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id,
    })
    .into_response(),
    Ok(AgentLeaseOutcome::NoWork) => Json(AcquireLeaseResponse::NoWork {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id,
      retry_after_ms: state.config.no_work_retry_delay_ms,
    })
    .into_response(),
    Err(error) => lease_error(&request_id, error),
  }
}

async fn assignment(
  state: &AgentState,
  grant: &octacity_server_application::LeaseGrant,
  observed_at_unix_ms: i64,
) -> Result<LeaseAssignment, AssignmentError> {
  let issued_at = wire_timestamp_seconds(grant.claimed_at)?;
  let (expires_at, capability_deadline) = wire_capability_deadline(grant.expires_at)?;
  let protected_inputs = match &grant.protected_inputs {
    Some(manifest) => state
      .artifacts
      .authorize_protected_inputs(AuthorizeProtectedInputsInput {
        manifest: manifest.clone(),
        observed_at_unix_ms,
        // The wire Lease carries whole seconds. Capabilities must be bounded by
        // that same observable deadline, not by discarded sub-second precision.
        lease_expires_at: capability_deadline,
      })
      .await
      .map_err(AssignmentError::ProtectedInputs)?,
    None => Vec::new(),
  };
  Ok(LeaseAssignment {
    lease_id: grant.lease_id.to_string(),
    job_id: grant.job_id.to_string(),
    attempt: u32::try_from(grant.attempt.get()).map_err(|_| AssignmentError::InvalidGrant)?,
    fencing_token: encode_fence(grant.fence.expose()),
    issued_at,
    expires_at,
    signed_job_spec: grant.signed_job_spec.clone(),
    protected_inputs,
  })
}

#[derive(Debug)]
enum AssignmentError {
  InvalidGrant,
  ProtectedInputs(AgentArtifactError),
}

impl AssignmentError {
  fn requires_compensation(&self) -> bool {
    matches!(
      self,
      Self::InvalidGrant
        | Self::ProtectedInputs(
          AgentArtifactError::InvalidRequest
            | AgentArtifactError::Integrity
            | AgentArtifactError::Conflict
            | AgentArtifactError::NotFound
        )
    )
  }
}

fn wire_timestamp_seconds(timestamp: Timestamp) -> Result<u64, AssignmentError> {
  u64::try_from(timestamp.unix_millis().div_euclid(1_000)).map_err(|_| AssignmentError::InvalidGrant)
}

fn wire_capability_deadline(timestamp: Timestamp) -> Result<(u64, Timestamp), AssignmentError> {
  let seconds = wire_timestamp_seconds(timestamp)?;
  let milliseconds = i64::try_from(seconds)
    .ok()
    .and_then(|value| value.checked_mul(1_000))
    .ok_or(AssignmentError::InvalidGrant)?;
  let deadline = Timestamp::from_unix_millis(milliseconds).map_err(|_| AssignmentError::InvalidGrant)?;
  Ok((seconds, deadline))
}

fn assignment_error(request_id: &str, error: AssignmentError) -> Response {
  let (status, code, message, retryable) = match error {
    AssignmentError::InvalidGrant | AssignmentError::ProtectedInputs(AgentArtifactError::InvalidRequest) => (
      StatusCode::UNPROCESSABLE_ENTITY,
      "invalid_assignment",
      "committed lease cannot be represented by the Agent protocol",
      false,
    ),
    AssignmentError::ProtectedInputs(
      AgentArtifactError::Integrity | AgentArtifactError::NotFound | AgentArtifactError::Conflict,
    ) => (
      StatusCode::CONFLICT,
      "protected_input_unavailable",
      "a protected input cannot satisfy its committed immutable identity",
      false,
    ),
    AssignmentError::ProtectedInputs(AgentArtifactError::CredentialRejected) => (
      StatusCode::UNAUTHORIZED,
      "credential_rejected",
      "agent credential rejected",
      false,
    ),
    AssignmentError::ProtectedInputs(AgentArtifactError::Fenced) => (
      StatusCode::CONFLICT,
      "lease_fenced",
      "lease is no longer current",
      false,
    ),
    AssignmentError::ProtectedInputs(AgentArtifactError::Expired) => {
      (StatusCode::CONFLICT, "lease_expired", "lease has expired", false)
    }
    AssignmentError::ProtectedInputs(AgentArtifactError::Unavailable) => (
      StatusCode::SERVICE_UNAVAILABLE,
      "unavailable",
      "protected input authorization is temporarily unavailable",
      true,
    ),
  };
  protocol_error(status, request_id, code, message, retryable)
}

fn encode_fence(bytes: [u8; 32]) -> String {
  let mut encoded = String::with_capacity(bytes.len() * 2);
  for byte in bytes {
    write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
  }
  encoded
}

fn bearer(headers: &HeaderMap) -> Option<String> {
  headers
    .get(header::AUTHORIZATION)?
    .to_str()
    .ok()?
    .strip_prefix("Bearer ")
    .filter(|value| !value.is_empty())
    .map(str::to_owned)
}

fn idempotency_key(headers: &HeaderMap) -> Option<&str> {
  headers.get(IDEMPOTENCY_KEY)?.to_str().ok()
}

#[derive(Clone, Copy)]
enum OperationContextError {
  IdempotencyKey,
  Credential,
  Clock,
}

fn operation_context(
  headers: &HeaderMap,
  request_id: &str,
) -> Result<(AgentCredentialToken, i64), OperationContextError> {
  if idempotency_key(headers) != Some(request_id) {
    return Err(OperationContextError::IdempotencyKey);
  }
  let credential = bearer(headers)
    .and_then(|value| AgentCredentialToken::parse(&value).ok())
    .ok_or(OperationContextError::Credential)?;
  let observed_at_unix_ms = unix_now_millis().ok_or(OperationContextError::Clock)?;
  Ok((credential, observed_at_unix_ms))
}

fn operation_context_error(request_id: &str, error: OperationContextError) -> Response {
  match error {
    OperationContextError::IdempotencyKey => protocol_error(
      StatusCode::BAD_REQUEST,
      request_id,
      "invalid_request",
      "idempotency-key must equal request_id",
      false,
    ),
    OperationContextError::Credential => protocol_error(
      StatusCode::UNAUTHORIZED,
      request_id,
      "credential_rejected",
      "agent credential rejected",
      false,
    ),
    OperationContextError::Clock => protocol_error(
      StatusCode::SERVICE_UNAVAILABLE,
      request_id,
      "unavailable",
      "agent operation unavailable",
      true,
    ),
  }
}

fn unix_now_millis() -> Option<i64> {
  let millis = SystemTime::now()
    .duration_since(SystemTime::UNIX_EPOCH)
    .ok()?
    .as_millis();
  i64::try_from(millis).ok()
}

fn rejection_response(rejection: JsonRejection) -> Response {
  let status = if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
    StatusCode::PAYLOAD_TOO_LARGE
  } else {
    StatusCode::BAD_REQUEST
  };
  protocol_error(
    status,
    "unknown-request",
    "invalid_request",
    "request body is not a valid agent protocol document",
    false,
  )
}

fn heartbeat_error(request_id: &str, error: AgentHeartbeatError) -> Response {
  match error {
    AgentHeartbeatError::InvalidRequest => protocol_error(
      StatusCode::BAD_REQUEST,
      request_id,
      "invalid_request",
      "lease heartbeat request is invalid",
      false,
    ),
    AgentHeartbeatError::Unavailable => protocol_error(
      StatusCode::SERVICE_UNAVAILABLE,
      request_id,
      "unavailable",
      "lease heartbeat unavailable",
      true,
    ),
  }
}

fn execution_error(request_id: &str, error: AgentExecutionError) -> Response {
  match error {
    AgentExecutionError::InvalidRequest => protocol_error(
      StatusCode::BAD_REQUEST,
      request_id,
      "invalid_request",
      "Agent execution request is invalid",
      false,
    ),
    AgentExecutionError::CredentialRejected => protocol_error(
      StatusCode::UNAUTHORIZED,
      request_id,
      "credential_rejected",
      "agent credential rejected",
      false,
    ),
    AgentExecutionError::Fenced => protocol_error(
      StatusCode::CONFLICT,
      request_id,
      "lease_fenced",
      "lease is no longer current",
      false,
    ),
    AgentExecutionError::Expired => protocol_error(
      StatusCode::CONFLICT,
      request_id,
      "lease_expired",
      "lease has expired",
      false,
    ),
    AgentExecutionError::EventGap { .. } => protocol_error(
      StatusCode::CONFLICT,
      request_id,
      "event_gap",
      "event batch starts after the next durable sequence",
      false,
    ),
    AgentExecutionError::EventsMissing { .. } => protocol_error(
      StatusCode::TOO_EARLY,
      request_id,
      "events_missing",
      "completion requires events that are not durable yet",
      true,
    ),
    AgentExecutionError::Conflict => protocol_error(
      StatusCode::CONFLICT,
      request_id,
      "execution_conflict",
      "Agent execution request conflicts with durable state",
      false,
    ),
    AgentExecutionError::Unavailable => protocol_error(
      StatusCode::SERVICE_UNAVAILABLE,
      request_id,
      "unavailable",
      "Agent execution service unavailable",
      true,
    ),
  }
}

fn tool_action_error(request_id: &str, error: AgentToolActionError) -> Response {
  match error {
    AgentToolActionError::InvalidRequest => protocol_error(
      StatusCode::BAD_REQUEST,
      request_id,
      "invalid_request",
      "protected tool-action request is invalid",
      false,
    ),
    AgentToolActionError::CredentialRejected => protocol_error(
      StatusCode::UNAUTHORIZED,
      request_id,
      "credential_rejected",
      "agent credential rejected",
      false,
    ),
    AgentToolActionError::Fenced => protocol_error(
      StatusCode::CONFLICT,
      request_id,
      "lease_fenced",
      "lease is no longer current",
      false,
    ),
    AgentToolActionError::Expired => protocol_error(
      StatusCode::CONFLICT,
      request_id,
      "lease_expired",
      "lease has expired",
      false,
    ),
    AgentToolActionError::Unavailable => protocol_error(
      StatusCode::SERVICE_UNAVAILABLE,
      request_id,
      "unavailable",
      "protected tool-action authorization unavailable",
      true,
    ),
  }
}

fn application_error(request_id: &str, error: AgentRegistrationError) -> Response {
  match error {
    AgentRegistrationError::InvalidRequest => protocol_error(
      StatusCode::BAD_REQUEST,
      request_id,
      "invalid_request",
      "agent registration request is invalid",
      false,
    ),
    AgentRegistrationError::CredentialRejected => protocol_error(
      StatusCode::UNAUTHORIZED,
      request_id,
      "credential_rejected",
      "agent credential rejected",
      false,
    ),
    AgentRegistrationError::Fenced => protocol_error(
      StatusCode::CONFLICT,
      request_id,
      "lease_fenced",
      "agent registration is no longer current",
      false,
    ),
    AgentRegistrationError::Unavailable => protocol_error(
      StatusCode::SERVICE_UNAVAILABLE,
      request_id,
      "unavailable",
      "registration service unavailable",
      true,
    ),
  }
}

fn lease_error(request_id: &str, error: AgentLeaseError) -> Response {
  match error {
    AgentLeaseError::InvalidRequest => protocol_error(
      StatusCode::BAD_REQUEST,
      request_id,
      "invalid_request",
      "lease acquisition request is invalid",
      false,
    ),
    AgentLeaseError::CredentialRejected => protocol_error(
      StatusCode::UNAUTHORIZED,
      request_id,
      "credential_rejected",
      "agent credential rejected",
      false,
    ),
    AgentLeaseError::Fenced => protocol_error(
      StatusCode::CONFLICT,
      request_id,
      "lease_fenced",
      "agent registration is no longer current",
      false,
    ),
    AgentLeaseError::Unavailable => protocol_error(
      StatusCode::SERVICE_UNAVAILABLE,
      request_id,
      "unavailable",
      "lease placement unavailable",
      true,
    ),
  }
}

fn protocol_error(status: StatusCode, request_id: &str, code: &str, message: &str, retryable: bool) -> Response {
  (
    status,
    Json(CoordinatorErrorResponse {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: request_id.to_owned(),
      code: code.to_owned(),
      message: message.to_owned(),
      retryable,
      retry_after_ms: None,
    }),
  )
    .into_response()
}

#[cfg(test)]
mod tests {
  use axum::body::to_bytes;
  use octacity_server_application::LeaseHeartbeatOutcome;
  use serde_json::json;

  use super::*;

  #[test]
  fn every_authoritative_heartbeat_outcome_has_one_wire_directive() {
    let continuing: LeaseHeartbeatOutcome = serde_json::from_value(json!({
      "Continue": {"expires_at": 61_999}
    }))
    .unwrap();
    let draining: LeaseHeartbeatOutcome = serde_json::from_value(json!({
      "Drain": {"expires_at": 61_999}
    }))
    .unwrap();
    assert_eq!(
      heartbeat_directive(continuing),
      Some(HeartbeatDirective::Continue { expires_at: 61 })
    );
    assert_eq!(
      heartbeat_directive(LeaseHeartbeatOutcome::Cancel),
      Some(HeartbeatDirective::Cancel)
    );
    assert_eq!(
      heartbeat_directive(LeaseHeartbeatOutcome::Fenced),
      Some(HeartbeatDirective::Fenced)
    );
    assert_eq!(
      heartbeat_directive(draining),
      Some(HeartbeatDirective::Drain { expires_at: 61 })
    );
  }

  #[test]
  fn assignment_capability_deadline_uses_the_same_floor_as_the_wire_lease() {
    let authoritative = Timestamp::from_unix_millis(61_999).unwrap();
    let (wire_seconds, capability_deadline) = wire_capability_deadline(authoritative).unwrap();

    assert_eq!(wire_seconds, 61);
    assert_eq!(capability_deadline.unix_millis(), 61_000);
    assert!(capability_deadline <= authoritative);
  }

  #[tokio::test]
  async fn assignment_failures_preserve_permanent_and_retryable_classification() {
    for (error, expected_status, expected_code, expected_retryable) in [
      (
        AgentArtifactError::Integrity,
        StatusCode::CONFLICT,
        "protected_input_unavailable",
        false,
      ),
      (
        AgentArtifactError::Unavailable,
        StatusCode::SERVICE_UNAVAILABLE,
        "unavailable",
        true,
      ),
    ] {
      let response = assignment_error("assignment-1", AssignmentError::ProtectedInputs(error));
      assert_eq!(response.status(), expected_status);
      let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
      let body: CoordinatorErrorResponse = serde_json::from_slice(&body).unwrap();
      assert_eq!(body.code, expected_code);
      assert_eq!(body.retryable, expected_retryable);
    }
  }
}
