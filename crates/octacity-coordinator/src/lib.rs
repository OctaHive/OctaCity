//! Outbound coordinator transport and fenced lease supervision.
//!
//! The public boundary contains no HTTP values. Callers provide validated
//! protocol DTOs and cancellation tokens; the HTTPS adapter owns credentials,
//! framing, deadlines, idempotency headers, and retry policy. Lease polling
//! verifies the signed job before exposing it to the future job state machine.

use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

use async_trait::async_trait;
use ed25519_dalek::VerifyingKey;
use octacity_protocol::{
  AcquireLeaseResponse, AgentInventory, AppendEventsResponse, AttemptEventEnvelope, CompleteLeaseRequest,
  HeartbeatDirective, HostCapacity, HostSnapshot, JobSpecError, LeaseAssignment,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

mod http;
mod lease;
mod retry;

pub use http::{HttpCoordinatorClient, HttpCoordinatorConfig};
pub use lease::{LeaseMonitor, LeaseMonitorOutcome, LeaseMonitorPolicy, LeasePollOutcome, LeasePoller, VerifiedLease};
pub use retry::RetryPolicy;

/// Successful registration epoch and server retry policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Registration {
  /// Stable agent identity used in endpoint paths.
  pub agent_id: String,
  /// Opaque server-issued registration epoch.
  pub registration_id: String,
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
    Ok(())
  }
}

/// Coordinator operation failure independent of the HTTP implementation.
#[derive(Debug, Error)]
pub enum CoordinatorError {
  /// Local client configuration is invalid.
  #[error("invalid coordinator client configuration: {0}")]
  Invalid(String),
  /// Enrollment credentials could not be inspected or read.
  #[error("failed to read coordinator credential '{path}': {source}")]
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

pub(crate) fn verify_assignment(
  lease: LeaseAssignment,
  keys: &BTreeMap<String, VerifyingKey>,
  now: u64,
  safety_margin: Duration,
) -> Result<VerifiedLease, CoordinatorError> {
  lease.validate(now, safety_margin.as_secs())?;
  let spec = octacity_protocol::verify_job_spec(
    &lease.signed_job_spec,
    keys,
    octacity_protocol::JobBinding {
      job_id: &lease.job_id,
      attempt: lease.attempt,
      now,
    },
  )
  .map_err(|error| CoordinatorError::JobSpec(Box::new(error)))?;
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
}
