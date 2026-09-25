//! Bounded best-effort Agent telemetry, isolated from coordination paths.

use std::sync::Arc;

use async_trait::async_trait;
use octacity_protocol::{AgentCredentialToken, AgentTelemetrySample, IngestAgentTelemetryRequest};
use thiserror::Error;

use crate::{AgentOperation, AgentRegistrationError, AgentRegistrationUseCases, AuthorizeAgentInput};

/// Complete transport-independent input for one diagnostic upload.
#[derive(Clone, Debug)]
pub struct AgentTelemetryInput {
  /// Shared versioned wire request.
  pub request: IngestAgentTelemetryRequest,
  /// Current registration bearer.
  pub credential: AgentCredentialToken,
  /// Server-observed Unix time in milliseconds.
  pub observed_at_unix_ms: i64,
}

/// Authenticated batch handed to the diagnostic exporter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentTelemetryBatch {
  request_id: String,
  samples: Vec<AgentTelemetrySample>,
  sample_count: u16,
}

impl AgentTelemetryBatch {
  /// Validates a wire request and hides its bounded-batch invariant.
  pub fn try_from_request(request: IngestAgentTelemetryRequest) -> Result<Self, AgentTelemetryError> {
    request.validate().map_err(|_| AgentTelemetryError::InvalidRequest)?;
    let sample_count = u16::try_from(request.samples.len()).map_err(|_| AgentTelemetryError::InvalidRequest)?;
    Ok(Self {
      request_id: request.request_id,
      samples: request.samples,
      sample_count,
    })
  }

  /// Request identity available for exporter-side deduplication.
  pub fn request_id(&self) -> &str {
    &self.request_id
  }

  /// Bounded, protocol-validated resource samples.
  pub fn samples(&self) -> &[AgentTelemetrySample] {
    &self.samples
  }

  /// Number of samples, already proven to fit the wire response field.
  pub const fn sample_count(&self) -> u16 {
    self.sample_count
  }

  /// Consumes the batch into the request identity and validated samples.
  pub fn into_parts(self) -> (String, Vec<AgentTelemetrySample>) {
    (self.request_id, self.samples)
  }
}

/// Application boundary used by the independently authenticated telemetry adapter.
#[async_trait]
pub trait AgentTelemetryUseCases: Send + Sync {
  /// Validates and authenticates one bounded batch before diagnostic export.
  async fn authorize_ingest(&self, input: AgentTelemetryInput) -> Result<AgentTelemetryBatch, AgentTelemetryError>;
}

/// Diagnostic sink for authenticated, protocol-bounded telemetry batches.
#[async_trait]
pub trait AgentTelemetryExporter: Send + Sync {
  /// Exports one authenticated batch.
  ///
  /// Adapters must deduplicate repeated [`AgentTelemetryBatch::request_id`]
  /// values for at least the coordinator retry window.
  async fn export(&self, batch: AgentTelemetryBatch) -> Result<(), AgentTelemetryExportError>;
}

/// Opaque exporter failure that cannot leak backend details to the Agent.
#[derive(Clone, Copy, Debug, Error)]
#[error("agent telemetry exporter unavailable")]
pub struct AgentTelemetryExportError;

/// Registration-backed authorization for diagnostic telemetry.
pub struct AgentTelemetryService {
  registrations: Arc<dyn AgentRegistrationUseCases>,
}

impl AgentTelemetryService {
  /// Creates the application service without transport or exporter concerns.
  pub fn new(registrations: Arc<dyn AgentRegistrationUseCases>) -> Self {
    Self { registrations }
  }
}

#[async_trait]
impl AgentTelemetryUseCases for AgentTelemetryService {
  async fn authorize_ingest(&self, input: AgentTelemetryInput) -> Result<AgentTelemetryBatch, AgentTelemetryError> {
    let registration_id = input.request.registration_id.clone();
    let batch = AgentTelemetryBatch::try_from_request(input.request)?;
    let authorized = self
      .registrations
      .authorize(AuthorizeAgentInput {
        operation: AgentOperation::Telemetry,
        registration_id,
        credential: input.credential,
        observed_at_unix_ms: input.observed_at_unix_ms,
      })
      .await
      .map_err(AgentTelemetryError::from)?;

    debug_assert_eq!(authorized.operation(), AgentOperation::Telemetry);
    Ok(batch)
  }
}

/// Stable request failures; exporter failure is intentionally not represented.
#[derive(Debug, Error)]
pub enum AgentTelemetryError {
  /// The shared request is invalid.
  #[error("invalid agent telemetry request")]
  InvalidRequest,
  /// The registration bearer was rejected without disclosing why.
  #[error("agent telemetry credential rejected")]
  CredentialRejected,
  /// A superseded registration attempted to upload telemetry.
  #[error("agent telemetry registration is fenced")]
  Fenced,
  /// Current registration authority could not be checked.
  #[error("agent telemetry authorization unavailable")]
  Unavailable,
}

impl From<AgentRegistrationError> for AgentTelemetryError {
  fn from(value: AgentRegistrationError) -> Self {
    match value {
      AgentRegistrationError::InvalidRequest => Self::InvalidRequest,
      AgentRegistrationError::CredentialRejected => Self::CredentialRejected,
      AgentRegistrationError::Fenced => Self::Fenced,
      AgentRegistrationError::Unavailable => Self::Unavailable,
    }
  }
}
