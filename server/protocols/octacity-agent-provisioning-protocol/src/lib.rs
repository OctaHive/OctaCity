//! Strict provider-neutral agent-provisioning adapter process protocol.
//!
//! See the [language-neutral v1 specification](https://github.com/OctaHive/OctaCity/blob/main/docs/protocols/agent-provisioning-v1.md).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Protocol version implemented by this crate.
pub const AGENT_PROVISIONING_PROTOCOL_VERSION: u16 = 1;
/// Maximum bytes in one opaque protocol identifier.
pub const MAX_IDENTIFIER_BYTES: usize = 256;
/// Maximum bytes in one secret-free diagnostic.
pub const MAX_DIAGNOSTIC_BYTES: usize = 4096;
/// Maximum delay an adapter may suggest before retry.
pub const MAX_RETRY_AFTER_MS: u64 = 60 * 60 * 1000;
/// Maximum encoded bytes accepted for one complete request or response.
pub const MAX_AGENT_PROVISIONING_MESSAGE_BYTES: usize = 64 * 1024;

/// Inclusive range of agent-provisioning protocol versions.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolRange {
  /// Oldest supported version.
  pub min: u16,
  /// Newest supported version.
  pub max: u16,
}

impl ProtocolRange {
  /// Selects the newest mutually supported version.
  pub fn negotiate(self, other: Self) -> Result<u16, ProtocolError> {
    if self.min == 0 || other.min == 0 || self.min > self.max || other.min > other.max {
      return Err(ProtocolError::Invalid("invalid protocol range"));
    }
    let selected = self.max.min(other.max);
    if selected < self.min.max(other.min) {
      return Err(ProtocolError::IncompatibleVersion);
    }
    Ok(selected)
  }
}

/// Agent-provisioning operations advertised by one future adapter.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
  /// Adapter can provision machines idempotently.
  pub provision: bool,
  /// Adapter can observe normalized lifecycle state.
  pub observe: bool,
  /// Adapter can terminate machines idempotently.
  pub terminate: bool,
}

/// Identity and compatibility declaration for one installed adapter.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterManifest {
  /// Operator-configured adapter identity.
  pub adapter_id: String,
  /// Lowercase SHA-256 of the immutable executable.
  pub executable_sha256: String,
  /// Supported protocol range.
  pub protocol: ProtocolRange,
  /// Supported lifecycle operations.
  pub capabilities: Capabilities,
}

impl AdapterManifest {
  /// Validates adapter identity, digest, protocol, and minimum lifecycle support.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    identifier("adapter_id", &self.adapter_id)?;
    sha256(&self.executable_sha256)?;
    self.protocol.negotiate(self.protocol)?;
    if !(self.capabilities.provision && self.capabilities.observe && self.capabilities.terminate) {
      return Err(ProtocolError::Invalid(
        "agent-provisioning adapter must implement the complete lifecycle",
      ));
    }
    Ok(())
  }
}

/// One strict agent-provisioning request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Request {
  /// Exact negotiated protocol version.
  pub protocol_version: u16,
  /// Caller-generated response-correlation identity.
  pub request_id: String,
  /// Requested lifecycle operation.
  pub command: Command,
}

/// One strict agent-provisioning response.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Response {
  /// Exact negotiated protocol version.
  pub protocol_version: u16,
  /// Echo of the request identity.
  pub request_id: String,
  /// Lifecycle result or classified failure.
  pub outcome: Outcome,
}

impl Response {
  /// Validates version, bounds, and normalized lifecycle data.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    if self.protocol_version != AGENT_PROVISIONING_PROTOCOL_VERSION {
      return Err(ProtocolError::IncompatibleVersion);
    }
    identifier("request_id", &self.request_id)?;
    match &self.outcome {
      Outcome::Machine(value) => value.validate(),
      Outcome::Terminated { machine_id } => identifier("machine_id", machine_id),
      Outcome::Acknowledged { operation_id } => identifier("operation_id", operation_id),
      Outcome::Failure(value) => value.validate(),
    }
  }

  /// Validates the response and its version and identifier correlation.
  pub fn validate_for(&self, request: &Request) -> Result<(), ProtocolError> {
    request.validate()?;
    self.validate()?;
    if self.protocol_version != request.protocol_version || self.request_id != request.request_id {
      return Err(ProtocolError::CorrelationMismatch);
    }
    Ok(())
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestWire {
  protocol_version: u16,
  request_id: String,
  command: Command,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResponseWire {
  protocol_version: u16,
  request_id: String,
  outcome: Outcome,
}

/// Decodes and validates one size-bounded agent-provisioning request.
pub fn decode_request(message: &[u8]) -> Result<Request, ProtocolError> {
  bounded_message(message)?;
  let wire: RequestWire = serde_json::from_slice(message).map_err(|_| ProtocolError::MalformedMessage)?;
  let request = Request {
    protocol_version: wire.protocol_version,
    request_id: wire.request_id,
    command: wire.command,
  };
  request.validate()?;
  Ok(request)
}

/// Decodes a size-bounded response and verifies correlation to `request`.
pub fn decode_response(message: &[u8], request: &Request) -> Result<Response, ProtocolError> {
  bounded_message(message)?;
  let wire: ResponseWire = serde_json::from_slice(message).map_err(|_| ProtocolError::MalformedMessage)?;
  let response = Response {
    protocol_version: wire.protocol_version,
    request_id: wire.request_id,
    outcome: wire.outcome,
  };
  response.validate_for(request)?;
  Ok(response)
}

/// Provider-neutral agent-provisioning operation outcomes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", content = "payload", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
  /// Current normalized machine state.
  Machine(Machine),
  /// Provider machine is confirmed absent.
  Terminated {
    /// Opaque provider machine identity.
    machine_id: String,
  },
  /// Cancellation or another idempotent request completed.
  Acknowledged {
    /// Stable operation identity.
    operation_id: String,
  },
  /// Classified provider failure.
  Failure(Failure),
}

impl Request {
  /// Validates version, bounds, and lifecycle invariants.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    if self.protocol_version != AGENT_PROVISIONING_PROTOCOL_VERSION {
      return Err(ProtocolError::IncompatibleVersion);
    }
    identifier("request_id", &self.request_id)?;
    match &self.command {
      Command::Provision(value) => value.validate(),
      Command::Observe(value) => {
        identifier("operation_id", &value.operation_id)?;
        identifier("machine_id", &value.machine_id)
      }
      Command::Terminate(value) => {
        identifier("operation_id", &value.operation_id)?;
        identifier("idempotency_key", &value.idempotency_key)?;
        identifier("machine_id", &value.machine_id)
      }
      Command::Cancel(value) => identifier("target_operation_id", &value.target_operation_id),
    }
  }
}

/// Agent-provisioning operations.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
  tag = "operation",
  content = "payload",
  rename_all = "snake_case",
  deny_unknown_fields
)]
pub enum Command {
  /// Idempotently provision one machine for a pool.
  Provision(Provision),
  /// Observe normalized state for one provider machine.
  Observe(Observe),
  /// Idempotently terminate one provider machine.
  Terminate(Terminate),
  /// Cooperatively cancel an in-flight operation.
  Cancel(CancelOperation),
}

/// Provider-neutral operating system and architecture.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Platform {
  /// Stable operating-system identifier.
  pub os: String,
  /// Stable CPU architecture identifier.
  pub architecture: String,
}

impl Platform {
  fn validate(&self) -> Result<(), ProtocolError> {
    identifier("platform os", &self.os)?;
    identifier("platform architecture", &self.architecture)
  }
}

/// Requested pool intent without provider template, cluster, or network types.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PoolIntent {
  /// Server-owned target pool identity.
  pub pool_id: String,
  /// Platform that the enrolling agent must report.
  pub expected_platform: Platform,
  /// Minimum logical processors requested.
  pub logical_cpu_count: u32,
  /// Minimum physical memory requested.
  pub memory_bytes: u64,
  /// Minimum workspace disk capacity requested.
  pub work_disk_bytes: u64,
}

impl PoolIntent {
  fn validate(&self) -> Result<(), ProtocolError> {
    identifier("pool_id", &self.pool_id)?;
    self.expected_platform.validate()?;
    if self.logical_cpu_count == 0 || self.memory_bytes == 0 || self.work_disk_bytes == 0 {
      return Err(ProtocolError::Invalid("capacity resources must be greater than zero"));
    }
    Ok(())
  }
}

/// Host-owned short-lived single-use enrollment bootstrap.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentBootstrap {
  /// Handle resolved only inside the selected adapter host.
  pub enrollment_credential_handle: String,
  /// Absolute Unix millisecond after which enrollment must fail.
  pub expires_at_unix_ms: u64,
}

/// Idempotent provision request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Provision {
  /// Stable identity of this in-flight operation.
  pub operation_id: String,
  /// Stable identity reused after lost responses.
  pub idempotency_key: String,
  /// Provider-neutral requested capacity.
  pub pool: PoolIntent,
  /// Single-use outbound-agent bootstrap.
  pub bootstrap: EnrollmentBootstrap,
}

impl Provision {
  fn validate(&self) -> Result<(), ProtocolError> {
    identifier("operation_id", &self.operation_id)?;
    identifier("idempotency_key", &self.idempotency_key)?;
    self.pool.validate()?;
    identifier(
      "enrollment_credential_handle",
      &self.bootstrap.enrollment_credential_handle,
    )?;
    if self.bootstrap.expires_at_unix_ms == 0 {
      return Err(ProtocolError::Invalid("bootstrap expiry must be non-zero"));
    }
    Ok(())
  }
}

/// Machine observation request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Observe {
  /// Stable identity of this in-flight operation.
  pub operation_id: String,
  /// Opaque provider machine identity.
  pub machine_id: String,
}

/// Idempotent machine termination request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Terminate {
  /// Stable identity of this in-flight operation.
  pub operation_id: String,
  /// Stable identity reused after lost responses.
  pub idempotency_key: String,
  /// Opaque provider machine identity.
  pub machine_id: String,
}

/// Cooperative cancellation request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancelOperation {
  /// Operation to cancel; repeated cancellation is safe.
  pub target_operation_id: String,
}

/// Normalized provider-independent machine lifecycle.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MachineLifecycle {
  /// Provision request is accepted but no machine is observable yet.
  Pending,
  /// Provider is creating or booting the machine.
  Provisioning,
  /// Machine exists and may enroll through the normal agent protocol.
  Running,
  /// Termination has begun.
  Terminating,
  /// Machine is confirmed absent.
  Terminated,
  /// Machine reached a permanent failed state.
  Failed,
}

/// Normalized machine state returned by provision and observe.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Machine {
  /// Opaque provider machine identity.
  pub machine_id: String,
  /// Server-owned pool identity.
  pub pool_id: String,
  /// Normalized lifecycle state.
  pub lifecycle: MachineLifecycle,
  /// Observed platform when known.
  pub platform: Option<Platform>,
  /// Last provider observation time in Unix milliseconds.
  pub observed_at_unix_ms: u64,
}

impl Machine {
  /// Validates normalized lifecycle data without provider-specific fields.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    identifier("machine_id", &self.machine_id)?;
    identifier("pool_id", &self.pool_id)?;
    if let Some(platform) = &self.platform {
      platform.validate()?;
    }
    if self.observed_at_unix_ms == 0 {
      return Err(ProtocolError::Invalid("observation time must be non-zero"));
    }
    Ok(())
  }
}

/// Stable agent-provisioning failure classes.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
  /// Request is malformed or violates configured policy.
  InvalidRequest,
  /// Adapter does not support the operation.
  Unsupported,
  /// Provider or configuration cannot succeed on retry.
  Permanent,
  /// Provider failure permits bounded idempotent retry.
  Transient,
  /// Operation was cooperatively cancelled.
  Cancelled,
  /// Peer violated the negotiated protocol.
  ProtocolFault,
}

/// Bounded secret-free agent-provisioning failure.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Failure {
  /// Stable semantic class.
  pub class: FailureClass,
  /// Stable machine-readable code.
  pub code: String,
  /// Bounded diagnostic without credentials or infrastructure configuration.
  pub diagnostic: String,
  /// Optional retry delay, valid only for transient failures.
  pub retry_after_ms: Option<u64>,
}

impl Failure {
  /// Validates bounded diagnostics and retry semantics.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    identifier("failure code", &self.code)?;
    if self.diagnostic.chars().any(char::is_control) {
      return Err(ProtocolError::Invalid("failure diagnostic"));
    }
    if self.diagnostic.len() > MAX_DIAGNOSTIC_BYTES {
      return Err(ProtocolError::LimitExceeded("failure diagnostic"));
    }
    if let Some(delay) = self.retry_after_ms
      && (self.class != FailureClass::Transient || delay > MAX_RETRY_AFTER_MS)
    {
      return Err(ProtocolError::Invalid(
        "retry delay requires a transient bounded failure",
      ));
    }
    Ok(())
  }
}

/// Agent-provisioning protocol validation failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProtocolError {
  /// Peers have no compatible version.
  #[error("agent-provisioning protocol versions are incompatible")]
  IncompatibleVersion,
  /// A semantic invariant is invalid.
  #[error("invalid agent-provisioning protocol value: {0}")]
  Invalid(&'static str),
  /// A bounded value exceeds its limit.
  #[error("agent-provisioning protocol limit exceeded: {0}")]
  LimitExceeded(&'static str),
  /// Encoded JSON is malformed or contains unknown fields.
  #[error("malformed agent-provisioning protocol message")]
  MalformedMessage,
  /// A response does not match the request version or identifier.
  #[error("agent-provisioning response does not correlate to its request")]
  CorrelationMismatch,
}

fn bounded_message(message: &[u8]) -> Result<(), ProtocolError> {
  if message.len() > MAX_AGENT_PROVISIONING_MESSAGE_BYTES {
    return Err(ProtocolError::LimitExceeded("encoded message"));
  }
  Ok(())
}

fn identifier(field: &'static str, value: &str) -> Result<(), ProtocolError> {
  if value.is_empty() || value.chars().any(char::is_control) {
    return Err(ProtocolError::Invalid(field));
  }
  if value.len() > MAX_IDENTIFIER_BYTES {
    return Err(ProtocolError::LimitExceeded(field));
  }
  Ok(())
}

fn sha256(value: &str) -> Result<(), ProtocolError> {
  if value.len() != 64
    || !value
      .bytes()
      .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
  {
    return Err(ProtocolError::Invalid("executable digest must be lowercase SHA-256"));
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  const FIXTURE: &str = include_str!("../fixtures/provision-v1.json");

  #[test]
  fn golden_fixture_round_trips_and_nested_unknown_fields_are_rejected() {
    let request = decode_request(FIXTURE.as_bytes()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(serde_json::to_value(request).unwrap(), expected);
    let mut unknown = expected;
    unknown["command"]["payload"]["pool"]["vsphere_template"] = serde_json::json!("forbidden");
    assert!(decode_request(&serde_json::to_vec(&unknown).unwrap()).is_err());
  }

  #[test]
  fn canonical_decoders_enforce_total_size_semantics_and_correlation() {
    assert_eq!(
      decode_request(&vec![b' '; MAX_AGENT_PROVISIONING_MESSAGE_BYTES + 1]),
      Err(ProtocolError::LimitExceeded("encoded message"))
    );
    let request = decode_request(FIXTURE.as_bytes()).unwrap();
    let response = Response {
      protocol_version: AGENT_PROVISIONING_PROTOCOL_VERSION,
      request_id: "different-request".to_owned(),
      outcome: Outcome::Acknowledged {
        operation_id: "operation-01".to_owned(),
      },
    };
    assert_eq!(response.validate_for(&request), Err(ProtocolError::CorrelationMismatch));
  }

  #[test]
  fn incompatible_versions_cancellation_limits_and_failure_classes_are_enforced() {
    assert_eq!(
      ProtocolRange { min: 1, max: 1 }.negotiate(ProtocolRange { min: 2, max: 2 }),
      Err(ProtocolError::IncompatibleVersion)
    );
    Request {
      protocol_version: 1,
      request_id: "cancel-01".to_owned(),
      command: Command::Cancel(CancelOperation {
        target_operation_id: "operation-01".to_owned(),
      }),
    }
    .validate()
    .unwrap();
    assert!(
      Failure {
        class: FailureClass::Permanent,
        code: "quota".to_owned(),
        diagnostic: "quota exceeded".to_owned(),
        retry_after_ms: Some(1),
      }
      .validate()
      .is_err()
    );
  }

  #[test]
  fn validation_distinguishes_invalid_values_from_size_limits() {
    assert_eq!(identifier("agent_id", ""), Err(ProtocolError::Invalid("agent_id")));
    assert_eq!(
      identifier("agent_id", "line\nbreak"),
      Err(ProtocolError::Invalid("agent_id"))
    );
    assert_eq!(
      identifier("agent_id", &"x".repeat(MAX_IDENTIFIER_BYTES + 1)),
      Err(ProtocolError::LimitExceeded("agent_id"))
    );
  }
}
