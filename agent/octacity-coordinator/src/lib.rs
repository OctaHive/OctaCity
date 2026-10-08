//! Outbound coordinator transport and fenced lease supervision.
//!
//! The public boundary contains no HTTP values. Callers provide validated
//! protocol DTOs and cancellation tokens; the HTTPS adapter owns credentials,
//! framing, deadlines, idempotency headers, and retry policy. Lease polling
//! verifies the signed job before exposing it to the future job state machine.

#![warn(missing_docs)]

use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

use async_trait::async_trait;
use ed25519_dalek::VerifyingKey;
use octacity_protocol::{
  AcquireLeaseResponse, AgentInventory, AgentTelemetrySample, AppendEventsResponse, AttemptEventEnvelope,
  AuthorizeToolActionRequest, AuthorizeToolActionResponse, BeginCacheSessionRequest, BeginCacheSessionResponse,
  BeginOutputUploadRequest, BeginOutputUploadResponse, CompleteLeaseRequest, CompleteOutputUploadRequest,
  HeartbeatDirective, HostCapacity, HostSnapshot, IngestAgentTelemetryResponse, JobSpecError, LeaseAssignment,
  RevokeCacheSessionRequest, VerifiedJobSpec,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

mod http;
mod lease;
mod retry;

pub use http::{HttpCoordinatorClient, HttpCoordinatorConfig, load_additional_root_certificates};
pub use lease::{LeaseMonitor, LeaseMonitorOutcome, LeaseMonitorPolicy, LeasePollOutcome, LeasePoller, VerifiedLease};
pub use retry::RetryPolicy;

/// Maximum accepted size of one operator-provided additional CA bundle.
pub const MAX_ADDITIONAL_CA_CERTIFICATE_BYTES: u64 = 1024 * 1024;

/// Successful registration epoch and server retry policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Registration {
  /// Stable agent identity used in endpoint paths.
  pub agent_id: String,
  /// Opaque server-issued registration epoch.
  pub registration_id: String,
  /// Execution-contract revision negotiated with the server.
  pub execution_contract_version: u16,
  /// Server-provided ceiling for retry and idle-poll delays.
  pub max_retry_delay: Duration,
}

impl Registration {
  /// Validates values later used in endpoint paths and retry calculations.
  pub fn validate(&self) -> Result<(), CoordinatorError> {
    for (name, value) in [
      ("registered agent_id", self.agent_id.as_str()),
      ("registration_id", self.registration_id.as_str()),
    ] {
      if value.is_empty()
        || value.len() > octacity_protocol::MAX_COORDINATOR_IDENTIFIER_BYTES
        || value.chars().any(char::is_control)
      {
        return Err(invalid(format!("{name} is invalid")));
      }
    }
    if self.max_retry_delay.is_zero() {
      return Err(invalid("registration retry delay must be greater than zero"));
    }
    if self.execution_contract_version == 0 {
      return Err(invalid(
        "registration execution-contract revision must be greater than zero",
      ));
    }
    Ok(())
  }
}

/// Coordinator operation failure independent of the HTTP implementation.
#[derive(Debug, Error)]
pub enum CoordinatorError {
  /// Local client configuration is invalid.
  #[error("invalid coordinator client configuration: {0}")]
  Invalid(String),
  /// Coordinator credentials could not be read or atomically updated.
  #[error("failed to access coordinator credential '{path}': {source}")]
  Credential {
    /// Credential file path.
    path: PathBuf,
    /// Underlying filesystem error.
    source: std::io::Error,
  },
  /// An operation was cancelled by local shutdown.
  #[error("coordinator operation was cancelled")]
  Cancelled,
  /// An operation exceeded its explicit deadline.
  #[error("coordinator operation '{operation}' timed out")]
  TimedOut {
    /// Operation active when the deadline expired.
    operation: &'static str,
  },
  /// The HTTPS stack failed before a valid response was available.
  #[error("coordinator transport failed while {operation}: {source}")]
  Transport {
    /// Logical operation being attempted.
    operation: &'static str,
    /// Underlying HTTP-client error.
    source: Box<reqwest::Error>,
  },
  /// A response exceeded the configured memory bound.
  #[error("coordinator response to '{operation}' exceeds the {maximum}-byte limit")]
  ResponseTooLarge {
    /// Logical operation being attempted.
    operation: &'static str,
    /// Configured response byte limit.
    maximum: usize,
  },
  /// A serialized request exceeded the configured memory bound.
  #[error("coordinator request for '{operation}' exceeds the {maximum}-byte limit")]
  RequestTooLarge {
    /// Logical operation being attempted.
    operation: &'static str,
    /// Configured request byte limit.
    maximum: usize,
  },
  /// A request DTO could not be serialized as protocol JSON.
  #[error("coordinator request for '{operation}' cannot be encoded as JSON: {source}")]
  Encode {
    /// Logical operation being attempted.
    operation: &'static str,
    /// JSON encoding error.
    source: Box<serde_json::Error>,
  },
  /// A successful response did not contain a valid protocol document.
  #[error("coordinator response to '{operation}' contains invalid JSON: {source}")]
  Json {
    /// Logical operation being attempted.
    operation: &'static str,
    /// JSON decoding error.
    source: Box<serde_json::Error>,
  },
  /// A decoded request, response, lease, or snapshot violates protocol rules.
  #[error(transparent)]
  Protocol(#[from] octacity_protocol::CoordinatorProtocolError),
  /// A leased JobSpec failed signature or semantic verification.
  #[error("leased JobSpec verification failed: {0}")]
  JobSpec(#[source] Box<JobSpecError>),
  /// The server rejected an authenticated protocol request.
  #[error("coordinator rejected '{operation}' with HTTP {status}: {code}: {message}")]
  Rejected {
    /// Logical operation being attempted.
    operation: &'static str,
    /// Numeric HTTP response status.
    status: u16,
    /// Stable server error code.
    code: String,
    /// Bounded human-readable diagnostic.
    message: String,
    /// Whether the server explicitly permits retrying the same operation.
    retryable: bool,
  },
  /// Retryable failures exhausted the bounded attempt policy.
  #[error("coordinator operation '{operation}' failed after {attempts} attempts: {last}")]
  RetriesExhausted {
    /// Logical operation being attempted.
    operation: &'static str,
    /// Total attempts made with the same idempotency key.
    attempts: usize,
    /// Last retryable transport or server failure.
    last: Box<CoordinatorError>,
  },
}

impl CoordinatorError {
  /// Returns whether repeating an idempotent coordinator operation can make progress.
  ///
  /// HTTP adapters already exhaust their bounded per-request retry policy. A
  /// long-lived owner such as the event spool may begin another bounded retry
  /// cycle after transport failures, server-approved rejections, or an
  /// exhausted cycle. Validation, authentication, fencing, and protocol errors
  /// are permanent and must be surfaced instead of retried forever.
  pub fn is_retryable(&self) -> bool {
    match self {
      Self::TimedOut { .. } | Self::Transport { .. } => true,
      Self::Rejected { retryable, .. } => *retryable,
      Self::RetriesExhausted { last, .. } => last.is_retryable(),
      _ => false,
    }
  }

  /// Returns the authoritative lease-loss reason encoded by a fenced endpoint.
  ///
  /// The coordinator protocol reserves these exact stable codes. Callers must
  /// not infer lease ownership from human-readable messages or HTTP status.
  pub fn lease_loss(&self) -> Option<LeaseMonitorOutcome> {
    match self {
      Self::Rejected { code, .. } if code == "lease_fenced" => Some(LeaseMonitorOutcome::Fenced),
      Self::Rejected { code, .. } if code == "lease_expired" => Some(LeaseMonitorOutcome::Expired),
      Self::RetriesExhausted { last, .. } => last.lease_loss(),
      _ => None,
    }
  }
}

/// Transport-independent operations required by the agent lifecycle.
#[async_trait]
pub trait CoordinatorClient: Send + Sync {
  /// Registers validated inventory and starts a new registration epoch.
  async fn register(
    &self,
    inventory: &AgentInventory,
    cancellation: CancellationToken,
  ) -> Result<Registration, CoordinatorError>;

  /// Performs one cancellable long poll for work.
  ///
  /// Implementations must validate the response version and correlate its
  /// request identifier before returning it. Signature verification remains
  /// the poller's responsibility so no unverified job reaches orchestration.
  async fn acquire_lease(
    &self,
    registration: &Registration,
    wait: Duration,
    lease_safety_margin: Duration,
    accept_jobs: bool,
    snapshot: &HostSnapshot,
    cancellation: CancellationToken,
  ) -> Result<AcquireLeaseResponse, CoordinatorError>;

  /// Renews one fenced lease and receives an explicit directive.
  ///
  /// Implementations must validate and correlate the response before exposing
  /// the directive to the lease monitor.
  async fn heartbeat(
    &self,
    registration: &Registration,
    lease: &LeaseAssignment,
    snapshot: &HostSnapshot,
    capacity: &HostCapacity,
    lease_safety_margin: Duration,
    cancellation: CancellationToken,
  ) -> Result<HeartbeatDirective, CoordinatorError>;

  /// Durably appends a contiguous batch and returns the server's contiguous cursor.
  async fn append_events(
    &self,
    registration: &Registration,
    lease: &LeaseAssignment,
    events: &[AttemptEventEnvelope],
    cancellation: CancellationToken,
  ) -> Result<AppendEventsResponse, CoordinatorError>;

  /// Records the terminal result after events and cleanup are complete.
  async fn complete_lease(
    &self,
    registration: &Registration,
    lease: &LeaseAssignment,
    completion: &CompleteLeaseRequest,
    cancellation: CancellationToken,
  ) -> Result<(), CoordinatorError>;
}

/// Narrow coordinator boundary used only by the lease-bound tool-action broker.
///
/// Keeping this operation outside [`CoordinatorClient`] avoids forcing ordinary
/// CI/CD test doubles and agents without Factory tool control to implement it.
#[async_trait]
pub trait ToolActionCoordinator: Send + Sync {
  /// Requests a disposition for one exact action blocked under the current lease.
  async fn authorize_tool_action(
    &self,
    registration: &Registration,
    lease: &LeaseAssignment,
    request: &AuthorizeToolActionRequest,
    cancellation: CancellationToken,
  ) -> Result<AuthorizeToolActionResponse, CoordinatorError>;
}

/// Narrow coordinator interface used only by immutable output publication.
///
/// Keeping these operations separate prevents lease polling, heartbeat, and
/// event-only adapters from inheriting upload behavior they cannot implement.
#[async_trait]
pub trait OutputUploadCoordinator: Send + Sync {
  /// Authorizes one immutable output and returns a short-lived upload target.
  async fn begin_output_upload(
    &self,
    registration: &Registration,
    lease: &LeaseAssignment,
    request: &BeginOutputUploadRequest,
    cancellation: CancellationToken,
  ) -> Result<BeginOutputUploadResponse, CoordinatorError>;

  /// Publishes an uploaded object after server-side size and digest checks.
  async fn complete_output_upload(
    &self,
    registration: &Registration,
    lease: &LeaseAssignment,
    request: &CompleteOutputUploadRequest,
    cancellation: CancellationToken,
  ) -> Result<(), CoordinatorError>;
}

/// Narrow coordinator boundary for short-lived runner cache authority.
#[async_trait]
pub trait CacheSessionCoordinator: Send + Sync {
  /// Returns a fenced physical scope and optional remote L2 credential.
  async fn begin_cache_session(
    &self,
    registration: &Registration,
    lease: &LeaseAssignment,
    request: &BeginCacheSessionRequest,
    cancellation: CancellationToken,
  ) -> Result<BeginCacheSessionResponse, CoordinatorError>;

  /// Revokes server-side authority after the runner can no longer read it.
  async fn revoke_cache_session(
    &self,
    registration: &Registration,
    lease: &LeaseAssignment,
    request: &RevokeCacheSessionRequest,
    cancellation: CancellationToken,
  ) -> Result<(), CoordinatorError>;
}

/// Narrow best-effort boundary kept outside lease and event coordination.
#[async_trait]
pub trait AgentTelemetryCoordinator: Send + Sync {
  /// Uploads a bounded diagnostic sample batch and returns explicit loss accounting.
  async fn ingest_agent_telemetry(
    &self,
    registration: &Registration,
    samples: &[AgentTelemetrySample],
    cancellation: CancellationToken,
  ) -> Result<IngestAgentTelemetryResponse, CoordinatorError>;
}

pub(crate) fn verify_assignment(
  lease: LeaseAssignment,
  keys: &BTreeMap<String, VerifyingKey>,
  now: u64,
  safety_margin: Duration,
  negotiated_execution_contract: u16,
) -> Result<VerifiedLease, CoordinatorError> {
  lease.validate(now, safety_margin.as_secs())?;
  let spec = octacity_protocol::verify_compatible_job_spec(
    &lease.signed_job_spec,
    keys,
    octacity_protocol::JobBinding {
      job_id: &lease.job_id,
      attempt: lease.attempt,
      now,
    },
  )
  .map_err(|error| CoordinatorError::JobSpec(Box::new(error)))?;
  if spec.protocol_version() > negotiated_execution_contract {
    return Err(invalid(format!(
      "leased JobSpec v{} exceeds negotiated execution contract v{negotiated_execution_contract}",
      spec.protocol_version()
    )));
  }
  match &spec {
    VerifiedJobSpec::V3(value) => {
      let transferred = lease
        .protected_inputs
        .iter()
        .map(|transfer| transfer.input.clone())
        .collect::<Vec<_>>();
      if transferred != value.protected_inputs.inputs {
        return Err(invalid(
          "lease protected-input transfers differ from signed JobSpec v3 intent",
        ));
      }
    }
    VerifiedJobSpec::V1(_) | VerifiedJobSpec::V2(_) if !lease.protected_inputs.is_empty() => {
      return Err(invalid(
        "ordinary JobSpec lease must not carry protected-input transfers",
      ));
    }
    VerifiedJobSpec::V1(_) | VerifiedJobSpec::V2(_) => {}
  }
  Ok(VerifiedLease { lease, spec })
}

pub(crate) fn unix_now() -> Result<u64, CoordinatorError> {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|duration| duration.as_secs())
    .map_err(|_| CoordinatorError::Invalid("system clock is before the Unix epoch".to_owned()))
}

pub(crate) fn invalid(message: impl Into<String>) -> CoordinatorError {
  CoordinatorError::Invalid(message.into())
}

pub(crate) type SharedCoordinator = Arc<dyn CoordinatorClient>;

#[cfg(test)]
mod tests {
  use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
  use ed25519_dalek::{Signer as _, SigningKey};
  use octacity_protocol::{
    ArtifactTransferCapability, EXECUTION_CONTRACT_V2, EXECUTION_CONTRACT_V3, JobSpecV3, LeaseAssignment,
    ProtectedInputTransferV3, SIGNATURE_ALGORITHM, SignedEnvelope,
  };

  use super::*;

  #[test]
  fn exposes_only_failures_safe_for_an_outer_idempotent_retry_cycle() {
    let rejection = |retryable| CoordinatorError::Rejected {
      operation: "append job events",
      status: 503,
      code: "unavailable".to_owned(),
      message: "try again".to_owned(),
      retryable,
    };
    assert!(rejection(true).is_retryable());
    assert!(!rejection(false).is_retryable());
    assert!(CoordinatorError::TimedOut { operation: "test" }.is_retryable());
    assert!(
      CoordinatorError::RetriesExhausted {
        operation: "test",
        attempts: 1,
        last: Box::new(rejection(true)),
      }
      .is_retryable()
    );
    assert!(
      !CoordinatorError::RetriesExhausted {
        operation: "test",
        attempts: 1,
        last: Box::new(rejection(false)),
      }
      .is_retryable()
    );
    assert!(!CoordinatorError::Invalid("invalid request".to_owned()).is_retryable());
  }

  #[test]
  fn rejects_a_signed_job_newer_than_the_negotiated_registration() {
    let spec: JobSpecV3 = serde_json::from_str(include_str!(
      "../../../shared/protocol-fixtures/job-spec/job-spec-v3.json"
    ))
    .unwrap();
    let signing_key = SigningKey::from_bytes(&[9; 32]);
    let payload = serde_json::to_vec(&spec).unwrap();
    let lease = LeaseAssignment {
      lease_id: "lease-v3".to_owned(),
      job_id: spec.job_id.clone(),
      attempt: spec.attempt,
      fencing_token: "fence-v3".to_owned(),
      issued_at: 99,
      expires_at: 200,
      signed_job_spec: SignedEnvelope {
        key_id: "primary".to_owned(),
        algorithm: SIGNATURE_ALGORITHM.to_owned(),
        payload: BASE64.encode(&payload),
        signature: BASE64.encode(signing_key.sign(&payload).to_bytes()),
      },
      protected_inputs: spec
        .protected_inputs
        .inputs
        .iter()
        .cloned()
        .map(|input| ProtectedInputTransferV3 {
          input,
          capability: ArtifactTransferCapability {
            url: "https://objects.example/protected?signature=redacted".to_owned(),
            required_headers: BTreeMap::new(),
            expires_at_unix_ms: 150_000,
          },
        })
        .collect(),
    };
    let keys = BTreeMap::from([("primary".to_owned(), signing_key.verifying_key())]);

    assert!(matches!(
      verify_assignment(lease.clone(), &keys, 100, Duration::from_secs(10), EXECUTION_CONTRACT_V2),
      Err(CoordinatorError::Invalid(message)) if message.contains("exceeds negotiated")
    ));
    let mut mismatched = lease.clone();
    mismatched.protected_inputs[0].input.destination = "/octacity/protected/other.json".to_owned();
    assert!(matches!(
      verify_assignment(mismatched, &keys, 100, Duration::from_secs(10), EXECUTION_CONTRACT_V3),
      Err(CoordinatorError::Invalid(message)) if message.contains("differ from signed")
    ));
    let mut expired = lease.clone();
    expired.protected_inputs[0].capability.expires_at_unix_ms = 100_000;
    assert!(matches!(
      verify_assignment(expired, &keys, 100, Duration::from_secs(10), EXECUTION_CONTRACT_V3),
      Err(CoordinatorError::Protocol(_))
    ));
    assert_eq!(
      verify_assignment(lease, &keys, 100, Duration::from_secs(10), EXECUTION_CONTRACT_V3)
        .unwrap()
        .spec
        .protocol_version(),
      EXECUTION_CONTRACT_V3
    );
  }
}
