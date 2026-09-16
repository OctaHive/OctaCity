//! Backend-neutral artifact transfer messages shared by server and agent.
//!
//! See the [language-neutral v1 specification](https://github.com/OctaHive/OctaCity/blob/main/docs/protocols/artifact-transfer-v1.md).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Artifact transfer protocol implemented by this crate.
pub const ARTIFACT_PROTOCOL_VERSION: u16 = 1;
/// Maximum bytes in an opaque identifier or logical output name.
pub const MAX_ARTIFACT_FIELD_BYTES: usize = 256;
/// Maximum entries in artifact metadata or required transfer headers.
pub const MAX_ARTIFACT_METADATA_ENTRIES: usize = 32;
/// Maximum bytes in one metadata or header value.
pub const MAX_ARTIFACT_METADATA_VALUE_BYTES: usize = 4096;
/// Maximum bytes in one opaque short-lived transfer URL.
pub const MAX_ARTIFACT_CAPABILITY_BYTES: usize = 8192;
/// Maximum encoded bytes accepted for one complete artifact message.
pub const MAX_ARTIFACT_MESSAGE_BYTES: usize = 256 * 1024;

/// Inclusive range of artifact protocol versions supported by one peer.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactProtocolRange {
  /// Oldest supported version.
  pub min: u16,
  /// Newest supported version.
  pub max: u16,
}

impl ArtifactProtocolRange {
  /// Selects the newest mutually supported protocol version.
  pub fn negotiate(self, other: Self) -> Result<u16, ArtifactProtocolError> {
    if self.min == 0 || other.min == 0 || self.min > self.max || other.min > other.max {
      return Err(ArtifactProtocolError::Invalid("invalid protocol range"));
    }
    let selected = self.max.min(other.max);
    if selected < self.min.max(other.min) {
      return Err(ArtifactProtocolError::IncompatibleVersion);
    }
    Ok(selected)
  }
}

/// One strict artifact operation request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ArtifactProtocolRequest {
  /// Exact negotiated protocol version.
  pub protocol_version: u16,
  /// Caller-generated response-correlation identifier.
  pub request_id: String,
  /// Requested operation.
  pub command: ArtifactCommand,
}

/// One strict artifact operation response.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ArtifactProtocolResponse {
  /// Exact negotiated protocol version.
  pub protocol_version: u16,
  /// Echo of the request identity.
  pub request_id: String,
  /// Operation result or classified failure.
  pub outcome: ArtifactOutcome,
}

impl ArtifactProtocolResponse {
  /// Validates version, bounds, and result invariants.
  pub fn validate(&self) -> Result<(), ArtifactProtocolError> {
    if self.protocol_version != ARTIFACT_PROTOCOL_VERSION {
      return Err(ArtifactProtocolError::IncompatibleVersion);
    }
    identifier("request_id", &self.request_id)?;
    match &self.outcome {
      ArtifactOutcome::Transfer(value) => value.validate(),
      ArtifactOutcome::Published { artifact_id }
      | ArtifactOutcome::Aborted { artifact_id }
      | ArtifactOutcome::Cancelled { artifact_id } => identifier("artifact_id", artifact_id),
      ArtifactOutcome::Failure(value) => value.validate(),
    }
  }

  /// Validates the response and its version and identifier correlation.
  pub fn validate_for(&self, request: &ArtifactProtocolRequest) -> Result<(), ArtifactProtocolError> {
    request.validate()?;
    self.validate()?;
    if self.protocol_version != request.protocol_version || self.request_id != request.request_id {
      return Err(ArtifactProtocolError::CorrelationMismatch);
    }
    Ok(())
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactProtocolRequestWire {
  protocol_version: u16,
  request_id: String,
  command: ArtifactCommand,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactProtocolResponseWire {
  protocol_version: u16,
  request_id: String,
  outcome: ArtifactOutcome,
}

/// Decodes and validates one size-bounded artifact request.
pub fn decode_artifact_request(message: &[u8]) -> Result<ArtifactProtocolRequest, ArtifactProtocolError> {
  bounded_message(message)?;
  let wire: ArtifactProtocolRequestWire =
    serde_json::from_slice(message).map_err(|_| ArtifactProtocolError::MalformedMessage)?;
  let request = ArtifactProtocolRequest {
    protocol_version: wire.protocol_version,
    request_id: wire.request_id,
    command: wire.command,
  };
  request.validate()?;
  Ok(request)
}

/// Decodes a size-bounded response and verifies correlation to `request`.
pub fn decode_artifact_response(
  message: &[u8],
  request: &ArtifactProtocolRequest,
) -> Result<ArtifactProtocolResponse, ArtifactProtocolError> {
  bounded_message(message)?;
  let wire: ArtifactProtocolResponseWire =
    serde_json::from_slice(message).map_err(|_| ArtifactProtocolError::MalformedMessage)?;
  let response = ArtifactProtocolResponse {
    protocol_version: wire.protocol_version,
    request_id: wire.request_id,
    outcome: wire.outcome,
  };
  response.validate_for(request)?;
  Ok(response)
}

/// Provider-neutral artifact operation outcomes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", content = "payload", rename_all = "snake_case", deny_unknown_fields)]
pub enum ArtifactOutcome {
  /// Short-lived authority for an upload or download.
  Transfer(ArtifactTransfer),
  /// Immutable bytes were verified and published.
  Published {
    /// Opaque logical artifact identity.
    artifact_id: String,
  },
  /// Pending bytes were revoked and hidden.
  Aborted {
    /// Opaque logical artifact identity.
    artifact_id: String,
  },
  /// Cancellation was accepted or the operation was already terminal.
  Cancelled {
    /// Opaque logical artifact identity.
    artifact_id: String,
  },
  /// Classified operation failure.
  Failure(ArtifactFailure),
}

impl ArtifactProtocolRequest {
  /// Validates protocol version, bounds, and operation invariants.
  pub fn validate(&self) -> Result<(), ArtifactProtocolError> {
    if self.protocol_version != ARTIFACT_PROTOCOL_VERSION {
      return Err(ArtifactProtocolError::IncompatibleVersion);
    }
    identifier("request_id", &self.request_id)?;
    match &self.command {
      ArtifactCommand::BeginUpload(value) => value.validate(),
      ArtifactCommand::CompleteUpload(value) => {
        identifier("operation_id", &value.operation_id)?;
        identifier("upload_id", &value.upload_id)
      }
      ArtifactCommand::AbortUpload(value) => {
        identifier("operation_id", &value.operation_id)?;
        identifier("upload_id", &value.upload_id)
      }
      ArtifactCommand::BeginDownload(value) => {
        identifier("operation_id", &value.operation_id)?;
        identifier("artifact_id", &value.artifact_id)
      }
      ArtifactCommand::Cancel(value) => identifier("target_operation_id", &value.target_operation_id),
    }
  }
}

/// Supported artifact protocol operations.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
  tag = "operation",
  content = "payload",
  rename_all = "snake_case",
  deny_unknown_fields
)]
pub enum ArtifactCommand {
  /// Reserve and authorize one immutable upload.
  BeginUpload(BeginArtifactUpload),
  /// Verify and publish previously uploaded bytes.
  CompleteUpload(CompleteArtifactUpload),
  /// Revoke and discard a pending upload.
  AbortUpload(AbortArtifactUpload),
  /// Obtain a short-lived read capability for a published artifact.
  BeginDownload(BeginArtifactDownload),
  /// Cooperatively cancel another in-flight protocol operation.
  Cancel(CancelArtifactOperation),
}

/// Logical output kind without storage-provider semantics.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
  /// User-visible produced file or archive.
  Artifact,
  /// Machine-readable report with an open format identifier.
  Report,
  /// Immutable redacted stdout or stderr chunk.
  LogChunk,
}

/// Immutable byte identity independent of ETag or object-store generation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactContentIdentity {
  /// Exact byte count.
  pub size_bytes: u64,
  /// Lowercase SHA-256 of the exact bytes.
  pub sha256: String,
}

/// Request to reserve one logical upload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BeginArtifactUpload {
  /// Stable identity of this in-flight operation.
  pub operation_id: String,
  /// Caller-owned idempotency identity for the logical upload.
  pub idempotency_key: String,
  /// Opaque server-owned logical artifact identity.
  pub artifact_id: String,
  /// User-visible bounded output name.
  pub name: String,
  /// Output semantics.
  pub kind: ArtifactKind,
  /// Transport media type.
  pub media_type: String,
  /// Optional open report-format identifier.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub report_format: Option<String>,
  /// Immutable expected byte identity.
  pub content: ArtifactContentIdentity,
  /// Bounded opaque product metadata.
  #[serde(default)]
  pub metadata: BTreeMap<String, String>,
}

impl BeginArtifactUpload {
  fn validate(&self) -> Result<(), ArtifactProtocolError> {
    for (field, value) in [
      ("operation_id", &self.operation_id),
      ("idempotency_key", &self.idempotency_key),
      ("artifact_id", &self.artifact_id),
      ("name", &self.name),
      ("media_type", &self.media_type),
    ] {
      identifier(field, value)?;
    }
    if let Some(format) = &self.report_format {
      identifier("report_format", format)?;
    }
    if self.content.sha256.len() != 64
      || !self
        .content
        .sha256
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
      return Err(ArtifactProtocolError::Invalid("sha256 must be lowercase hexadecimal"));
    }
    bounded_map(&self.metadata)
  }
}

/// Request to verify and publish a pending upload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteArtifactUpload {
  /// Stable identity of this in-flight operation.
  pub operation_id: String,
  /// Opaque pending-upload identity.
  pub upload_id: String,
}

/// Request to revoke and discard a pending upload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AbortArtifactUpload {
  /// Stable identity of this in-flight operation.
  pub operation_id: String,
  /// Opaque pending-upload identity.
  pub upload_id: String,
}

/// Request for authorized access to one published artifact.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BeginArtifactDownload {
  /// Stable identity of this in-flight operation.
  pub operation_id: String,
  /// Opaque logical artifact identity.
  pub artifact_id: String,
}

/// Cooperative cancellation of one in-flight operation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancelArtifactOperation {
  /// Operation to cancel; cancellation itself is idempotent.
  pub target_operation_id: String,
}

/// Opaque, short-lived byte-transfer authority.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactTransferCapability {
  /// Opaque URL whose physical layout is server-private.
  pub url: String,
  /// Required signed request headers.
  #[serde(default)]
  pub required_headers: BTreeMap<String, String>,
  /// Unix millisecond at which this capability expires.
  pub expires_at_unix_ms: u64,
}

impl ArtifactTransferCapability {
  /// Validates opaque capability and signed-header bounds.
  pub fn validate(&self) -> Result<(), ArtifactProtocolError> {
    if self.url.is_empty() || self.url.chars().any(char::is_control) {
      return Err(ArtifactProtocolError::Invalid("transfer capability"));
    }
    if self.url.len() > MAX_ARTIFACT_CAPABILITY_BYTES {
      return Err(ArtifactProtocolError::LimitExceeded("transfer capability"));
    }
    if self.expires_at_unix_ms == 0 {
      return Err(ArtifactProtocolError::Invalid("capability expiry must be non-zero"));
    }
    bounded_map(&self.required_headers)
  }
}

/// Logical transfer identity paired with an opaque short-lived capability.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactTransfer {
  /// Opaque logical artifact identity.
  pub artifact_id: String,
  /// Opaque pending-upload identity when this is an upload capability.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub upload_id: Option<String>,
  /// Short-lived backend-neutral transfer authority.
  pub capability: ArtifactTransferCapability,
}

impl ArtifactTransfer {
  fn validate(&self) -> Result<(), ArtifactProtocolError> {
    identifier("artifact_id", &self.artifact_id)?;
    if let Some(upload_id) = &self.upload_id {
      identifier("upload_id", upload_id)?;
    }
    self.capability.validate()
  }
}

impl std::fmt::Debug for ArtifactTransferCapability {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("ArtifactTransferCapability")
      .field("url", &"<redacted>")
      .field(
        "required_header_names",
        &self.required_headers.keys().collect::<Vec<_>>(),
      )
      .field("expires_at_unix_ms", &self.expires_at_unix_ms)
      .finish()
  }
}

/// Stable failure classes used by every artifact implementation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactFailureClass {
  /// Request was invalid and must be changed.
  InvalidRequest,
  /// Operation is not implemented by this peer.
  Unsupported,
  /// Logical object or authorization state cannot succeed on retry.
  Permanent,
  /// Backend failed transiently and an idempotent retry is allowed.
  Transient,
  /// Operation was cooperatively cancelled.
  Cancelled,
  /// Stored bytes failed immutable identity verification.
  Integrity,
  /// Peer violated the negotiated protocol.
  ProtocolFault,
}

/// Bounded provider-neutral artifact failure.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactFailure {
  /// Stable semantic class.
  pub class: ArtifactFailureClass,
  /// Stable machine-readable code.
  pub code: String,
  /// Bounded secret-free diagnostic.
  pub diagnostic: String,
  /// Optional retry delay, valid only for transient failures.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub retry_after_ms: Option<u64>,
}

impl ArtifactFailure {
  /// Validates bounded diagnostics and retry classification.
  pub fn validate(&self) -> Result<(), ArtifactProtocolError> {
    identifier("failure code", &self.code)?;
    if self.diagnostic.is_empty() || self.diagnostic.chars().any(char::is_control) {
      return Err(ArtifactProtocolError::Invalid("failure diagnostic"));
    }
    if self.diagnostic.len() > MAX_ARTIFACT_METADATA_VALUE_BYTES {
      return Err(ArtifactProtocolError::LimitExceeded("failure diagnostic"));
    }
    if self.retry_after_ms.is_some() && self.class != ArtifactFailureClass::Transient {
      return Err(ArtifactProtocolError::Invalid(
        "only transient failures may request retry",
      ));
    }
    Ok(())
  }
}

/// Artifact protocol validation failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ArtifactProtocolError {
  /// Peers do not share a protocol version.
  #[error("artifact protocol versions are incompatible")]
  IncompatibleVersion,
  /// A semantic invariant is invalid.
  #[error("invalid artifact protocol value: {0}")]
  Invalid(&'static str),
  /// A bounded field exceeds its documented limit.
  #[error("artifact protocol limit exceeded: {0}")]
  LimitExceeded(&'static str),
  /// Encoded JSON is malformed or contains unknown fields.
  #[error("malformed artifact protocol message")]
  MalformedMessage,
  /// A response does not match the request version or identifier.
  #[error("artifact protocol response does not correlate to its request")]
  CorrelationMismatch,
}

fn bounded_message(message: &[u8]) -> Result<(), ArtifactProtocolError> {
  if message.len() > MAX_ARTIFACT_MESSAGE_BYTES {
    return Err(ArtifactProtocolError::LimitExceeded("encoded message"));
  }
  Ok(())
}

fn identifier(field: &'static str, value: &str) -> Result<(), ArtifactProtocolError> {
  if value.is_empty() || value.chars().any(char::is_control) {
    return Err(ArtifactProtocolError::Invalid(field));
  }
  if value.len() > MAX_ARTIFACT_FIELD_BYTES {
    return Err(ArtifactProtocolError::LimitExceeded(field));
  }
  Ok(())
}

fn bounded_map(values: &BTreeMap<String, String>) -> Result<(), ArtifactProtocolError> {
  if values.len() > MAX_ARTIFACT_METADATA_ENTRIES {
    return Err(ArtifactProtocolError::LimitExceeded("metadata entries"));
  }
  for (key, value) in values {
    identifier("metadata key", key)?;
    if value.chars().any(char::is_control) {
      return Err(ArtifactProtocolError::Invalid("metadata value"));
    }
    if value.len() > MAX_ARTIFACT_METADATA_VALUE_BYTES {
      return Err(ArtifactProtocolError::LimitExceeded("metadata value"));
    }
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  const FIXTURE: &str = include_str!("../../protocol-fixtures/artifact/begin-upload-v1.json");

  #[test]
  fn golden_fixture_is_strict_and_valid() {
    let request = decode_artifact_request(FIXTURE.as_bytes()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(serde_json::to_value(request).unwrap(), expected);

    let mut unknown = expected;
    unknown["unexpected"] = serde_json::json!(true);
    assert!(decode_artifact_request(&serde_json::to_vec(&unknown).unwrap()).is_err());
  }

  #[test]
  fn canonical_decoders_enforce_total_size_semantics_and_correlation() {
    assert_eq!(
      decode_artifact_request(&vec![b' '; MAX_ARTIFACT_MESSAGE_BYTES + 1]),
      Err(ArtifactProtocolError::LimitExceeded("encoded message"))
    );
    let request = decode_artifact_request(FIXTURE.as_bytes()).unwrap();
    let response = ArtifactProtocolResponse {
      protocol_version: ARTIFACT_PROTOCOL_VERSION,
      request_id: "different-request".to_owned(),
      outcome: ArtifactOutcome::Cancelled {
        artifact_id: "artifact-01".to_owned(),
      },
    };
    assert_eq!(
      response.validate_for(&request),
      Err(ArtifactProtocolError::CorrelationMismatch)
    );
  }

  #[test]
  fn version_cancellation_limits_and_failure_classification_are_explicit() {
    assert_eq!(
      ArtifactProtocolRange { min: 1, max: 2 }
        .negotiate(ArtifactProtocolRange { min: 2, max: 3 })
        .unwrap(),
      2
    );
    assert_eq!(
      ArtifactProtocolRange { min: 1, max: 1 }.negotiate(ArtifactProtocolRange { min: 2, max: 2 }),
      Err(ArtifactProtocolError::IncompatibleVersion)
    );
    let cancel = ArtifactProtocolRequest {
      protocol_version: 1,
      request_id: "cancel-request".to_owned(),
      command: ArtifactCommand::Cancel(CancelArtifactOperation {
        target_operation_id: "upload-operation".to_owned(),
      }),
    };
    cancel.validate().unwrap();
    let failure = ArtifactFailure {
      class: ArtifactFailureClass::Permanent,
      code: "object_missing".to_owned(),
      diagnostic: "not found".to_owned(),
      retry_after_ms: Some(10),
    };
    assert!(failure.validate().is_err());
  }

  #[test]
  fn transfer_capability_debug_is_redacted() {
    let capability = ArtifactTransferCapability {
      url: "https://objects.example/item?signature=secret".to_owned(),
      required_headers: BTreeMap::from([("authorization".to_owned(), "secret".to_owned())]),
      expires_at_unix_ms: 1,
    };
    let debug = format!("{capability:?}");
    assert!(!debug.contains("signature=secret"));
    assert!(!debug.contains("authorization: secret"));
  }

  #[test]
  fn validation_distinguishes_invalid_values_from_size_limits() {
    assert_eq!(
      identifier("artifact_id", ""),
      Err(ArtifactProtocolError::Invalid("artifact_id"))
    );
    assert_eq!(
      identifier("artifact_id", "line\nbreak"),
      Err(ArtifactProtocolError::Invalid("artifact_id"))
    );
    assert_eq!(
      identifier("artifact_id", &"x".repeat(MAX_ARTIFACT_FIELD_BYTES + 1)),
      Err(ArtifactProtocolError::LimitExceeded("artifact_id"))
    );
    let invalid_metadata = BTreeMap::from([("key".to_owned(), "line\nbreak".to_owned())]);
    assert_eq!(
      bounded_map(&invalid_metadata),
      Err(ArtifactProtocolError::Invalid("metadata value"))
    );
  }
}
