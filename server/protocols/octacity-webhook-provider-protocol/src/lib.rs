//! Strict provider-neutral webhook adapter process protocol.
//!
//! See the [language-neutral v1 specification](https://github.com/OctaHive/OctaCity/blob/main/docs/protocols/webhook-provider-v1.md).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::BTreeMap;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use octacity_adapter_protocol::{self as adapter_core, ValidationFailure};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Protocol version implemented by this crate.
pub const WEBHOOK_PROTOCOL_VERSION: u16 = 1;
/// Maximum bytes in an opaque identifier.
pub const MAX_IDENTIFIER_BYTES: usize = 256;
/// Maximum decoded bytes in one raw delivery.
pub const MAX_DELIVERY_BYTES: usize = 1024 * 1024;
/// Maximum base64 characters that can encode one bounded delivery.
pub const MAX_DELIVERY_BASE64_BYTES: usize = MAX_DELIVERY_BYTES.div_ceil(3) * 4;
/// Maximum allowlisted headers supplied to an adapter.
pub const MAX_HEADERS: usize = 64;
/// Maximum entries in normalized opaque metadata.
pub const MAX_METADATA_ENTRIES: usize = 32;
/// Maximum bytes in a header or metadata value.
pub const MAX_VALUE_BYTES: usize = 4096;
/// Maximum bytes in a secret-free diagnostic.
pub const MAX_DIAGNOSTIC_BYTES: usize = 4096;
/// Maximum encoded bytes accepted for one complete request or response.
pub const MAX_WEBHOOK_MESSAGE_BYTES: usize = 2 * 1024 * 1024;

/// Inclusive range of supported protocol versions.
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
    adapter_core::negotiate_version(self.min, self.max, other.min, other.max).map_err(|failure| match failure {
      ValidationFailure::InvalidRange => ProtocolError::Invalid("invalid protocol range"),
      ValidationFailure::IncompatibleVersion => ProtocolError::IncompatibleVersion,
      _ => unreachable!("version negotiation returns only range failures"),
    })
  }
}

/// Adapter operations advertised before any credential or delivery is sent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
  /// Adapter can authenticate and normalize raw deliveries.
  pub verify_delivery: bool,
  /// Adapter can create remote webhook registrations.
  pub create_registration: bool,
  /// Adapter can observe remote webhook registrations.
  pub observe_registration: bool,
  /// Adapter can rotate remote webhook verification material.
  pub rotate_registration: bool,
  /// Adapter can delete remote webhook registrations.
  pub delete_registration: bool,
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
  /// Supported operations.
  pub capabilities: Capabilities,
}

impl AdapterManifest {
  /// Validates identity, digest, and protocol bounds.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    identifier("adapter_id", &self.adapter_id)?;
    sha256(&self.executable_sha256)?;
    self.protocol.negotiate(self.protocol)?;
    if !self.capabilities.verify_delivery {
      return Err(ProtocolError::Invalid("webhook adapter must verify deliveries"));
    }
    Ok(())
  }
}

/// One strict webhook adapter request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Request {
  /// Exact negotiated version.
  pub protocol_version: u16,
  /// Caller-generated response-correlation identity.
  pub request_id: String,
  /// Requested operation.
  pub command: Command,
}

/// One strict webhook adapter response.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Response {
  /// Exact negotiated version.
  pub protocol_version: u16,
  /// Echo of the request identity.
  pub request_id: String,
  /// Operation result or classified failure.
  pub outcome: Outcome,
}

impl Response {
  /// Validates version, bounds, and normalized response data.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    if self.protocol_version != WEBHOOK_PROTOCOL_VERSION {
      return Err(ProtocolError::IncompatibleVersion);
    }
    identifier("request_id", &self.request_id)?;
    match &self.outcome {
      Outcome::AuthenticatedEvent(value) => value.validate(),
      Outcome::Registration(value) => value.validate(),
      Outcome::Acknowledged { operation_id } => identifier("operation_id", operation_id),
      Outcome::Failure(value) => value.validate(),
    }
  }

  /// Validates the response and its version and identifier correlation.
  pub fn validate_for(&self, request: &Request) -> Result<(), ProtocolError> {
    request.validate()?;
    self.validate()?;
    adapter_core::require_correlation(
      self.protocol_version,
      &self.request_id,
      request.protocol_version,
      &request.request_id,
    )
    .map_err(|_| ProtocolError::CorrelationMismatch)
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

/// Decodes and validates one size-bounded webhook request.
pub fn decode_request(message: &[u8]) -> Result<Request, ProtocolError> {
  bounded_message(message)?;
  let wire: RequestWire = adapter_core::decode_json(message).map_err(|_| ProtocolError::MalformedMessage)?;
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
  let wire: ResponseWire = adapter_core::decode_json(message).map_err(|_| ProtocolError::MalformedMessage)?;
  let response = Response {
    protocol_version: wire.protocol_version,
    request_id: wire.request_id,
    outcome: wire.outcome,
  };
  response.validate_for(request)?;
  Ok(response)
}

/// Provider-neutral webhook operation outcomes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", content = "payload", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
  /// Exact delivery was authenticated before event normalization.
  AuthenticatedEvent(AuthenticatedRepositoryEvent),
  /// Current normalized managed-registration state.
  Registration(ManagedRegistration),
  /// Idempotent operation completed without additional data.
  Acknowledged {
    /// Stable operation identity.
    operation_id: String,
  },
  /// Classified adapter failure.
  Failure(Failure),
}

impl Request {
  /// Validates version, bounds, and operation semantics.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    if self.protocol_version != WEBHOOK_PROTOCOL_VERSION {
      return Err(ProtocolError::IncompatibleVersion);
    }
    identifier("request_id", &self.request_id)?;
    match &self.command {
      Command::VerifyDelivery(value) => value.validate(),
      Command::CreateRegistration(value)
      | Command::ObserveRegistration(value)
      | Command::RotateRegistration(value)
      | Command::DeleteRegistration(value) => value.validate(),
      Command::Cancel(value) => identifier("target_operation_id", &value.target_operation_id),
    }
  }
}

/// Webhook protocol operations.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
  tag = "operation",
  content = "payload",
  rename_all = "snake_case",
  deny_unknown_fields
)]
pub enum Command {
  /// Authenticate the exact bytes and normalize the event.
  VerifyDelivery(VerifyDelivery),
  /// Idempotently create a managed remote webhook.
  CreateRegistration(ManagedRegistrationOperation),
  /// Observe a managed remote webhook.
  ObserveRegistration(ManagedRegistrationOperation),
  /// Idempotently rotate verification material.
  RotateRegistration(ManagedRegistrationOperation),
  /// Idempotently delete a managed remote webhook.
  DeleteRegistration(ManagedRegistrationOperation),
  /// Cooperatively cancel an in-flight operation.
  Cancel(CancelOperation),
}

/// Exact bounded raw delivery presented to the provider adapter.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyDelivery {
  /// Stable identity of this in-flight operation.
  pub operation_id: String,
  /// Server-owned integration identity.
  pub integration_id: String,
  /// Host-owned handle through which the adapter receives protected material.
  pub verification_material_handle: String,
  /// Allowlisted transport headers normalized to lowercase names.
  pub headers: BTreeMap<String, String>,
  /// Base64 encoding of the exact request body bytes.
  pub body_base64: String,
}

impl std::fmt::Debug for VerifyDelivery {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("VerifyDelivery")
      .field("operation_id", &self.operation_id)
      .field("integration_id", &self.integration_id)
      .field("verification_material_handle", &"<redacted>")
      .field("header_names", &self.headers.keys().collect::<Vec<_>>())
      .field("body_base64", &"<redacted>")
      .finish()
  }
}

impl VerifyDelivery {
  fn validate(&self) -> Result<(), ProtocolError> {
    for (field, value) in [
      ("operation_id", &self.operation_id),
      ("integration_id", &self.integration_id),
      ("verification_material_handle", &self.verification_material_handle),
    ] {
      identifier(field, value)?;
    }
    bounded_map(&self.headers, MAX_HEADERS)?;
    if self.headers.keys().any(|name| {
      name
        .bytes()
        .any(|byte| !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'))
    }) {
      return Err(ProtocolError::Invalid("header names must be lowercase ASCII tokens"));
    }
    if self.body_base64.len() > MAX_DELIVERY_BASE64_BYTES {
      return Err(ProtocolError::LimitExceeded("delivery body"));
    }
    let decoded = STANDARD
      .decode(&self.body_base64)
      .map_err(|_| ProtocolError::Invalid("delivery body is not valid base64"))?;
    if decoded.len() > MAX_DELIVERY_BYTES {
      return Err(ProtocolError::LimitExceeded("delivery body"));
    }
    Ok(())
  }
}

/// Common idempotent managed-registration operation.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedRegistrationOperation {
  /// Stable identity of this in-flight operation.
  pub operation_id: String,
  /// Server-owned integration identity.
  pub integration_id: String,
  /// Stable idempotency identity.
  pub idempotency_key: String,
  /// Provider-neutral callback URL.
  pub callback_url: String,
  /// Host-owned provider-administration credential handle.
  pub credential_handle: String,
}

impl std::fmt::Debug for ManagedRegistrationOperation {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("ManagedRegistrationOperation")
      .field("operation_id", &self.operation_id)
      .field("integration_id", &self.integration_id)
      .field("idempotency_key", &self.idempotency_key)
      .field("callback_url", &self.callback_url)
      .field("credential_handle", &"<redacted>")
      .finish()
  }
}

impl ManagedRegistrationOperation {
  fn validate(&self) -> Result<(), ProtocolError> {
    for (field, value) in [
      ("operation_id", &self.operation_id),
      ("integration_id", &self.integration_id),
      ("idempotency_key", &self.idempotency_key),
      ("credential_handle", &self.credential_handle),
    ] {
      identifier(field, value)?;
    }
    bounded_text("callback_url", &self.callback_url, MAX_VALUE_BYTES)
  }
}

/// Cooperative cancellation request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancelOperation {
  /// Operation to cancel; repeated cancellation is safe.
  pub target_operation_id: String,
}

/// Normalized managed-registration lifecycle state.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationStatus {
  /// Remote registration exists and is enabled.
  Active,
  /// Remote registration exists but is disabled.
  Disabled,
  /// Remote registration is absent.
  Missing,
}

/// Provider-neutral managed webhook registration state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedRegistration {
  /// Server-owned integration identity.
  pub integration_id: String,
  /// Opaque remote registration identity.
  pub registration_id: String,
  /// Current normalized lifecycle state.
  pub status: RegistrationStatus,
  /// Callback URL observed by the adapter.
  pub callback_url: String,
}

impl ManagedRegistration {
  fn validate(&self) -> Result<(), ProtocolError> {
    identifier("integration_id", &self.integration_id)?;
    identifier("registration_id", &self.registration_id)?;
    bounded_text("callback_url", &self.callback_url, MAX_VALUE_BYTES)
  }
}

/// Provider-neutral authenticated repository event.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthenticatedRepositoryEvent {
  /// Server-owned integration identity.
  pub integration_id: String,
  /// Provider delivery identity scoped to the integration.
  pub delivery_id: String,
  /// Normalized event kind such as `push` or `merge_request`.
  pub event_kind: String,
  /// Provider-neutral repository identity.
  pub repository_id: String,
  /// Optional changed reference.
  pub reference: Option<String>,
  /// Optional immutable revision reported by the provider.
  pub revision: Option<String>,
  /// Provider event time in Unix milliseconds.
  pub provider_time_unix_ms: Option<u64>,
  /// Optional display-only actor name.
  pub actor_display_name: Option<String>,
  /// Bounded provider metadata not used as identity or authorization.
  #[serde(default)]
  pub metadata: BTreeMap<String, String>,
}

impl AuthenticatedRepositoryEvent {
  /// Validates all normalized fields after provider authentication.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    for (field, value) in [
      ("integration_id", &self.integration_id),
      ("delivery_id", &self.delivery_id),
      ("event_kind", &self.event_kind),
      ("repository_id", &self.repository_id),
    ] {
      identifier(field, value)?;
    }
    for (field, value) in [
      ("reference", self.reference.as_deref()),
      ("revision", self.revision.as_deref()),
      ("actor_display_name", self.actor_display_name.as_deref()),
    ] {
      if let Some(value) = value {
        bounded_text(field, value, MAX_VALUE_BYTES)?;
      }
    }
    bounded_map(&self.metadata, MAX_METADATA_ENTRIES)
  }
}

/// Stable provider-neutral failure classes.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
  /// Request is malformed or violates configured policy.
  InvalidRequest,
  /// Adapter does not support the requested operation.
  Unsupported,
  /// Repository, integration, or provider configuration cannot succeed on retry.
  Permanent,
  /// Provider failure permits bounded idempotent retry.
  Transient,
  /// Operation was cooperatively cancelled.
  Cancelled,
  /// Peer violated the negotiated protocol.
  ProtocolFault,
}

/// Bounded secret-free adapter failure.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Failure {
  /// Stable semantic class.
  pub class: FailureClass,
  /// Stable machine-readable code.
  pub code: String,
  /// Bounded diagnostic safe for logs and durable retry records.
  pub diagnostic: String,
  /// Optional retry delay, allowed only for transient failures.
  pub retry_after_ms: Option<u64>,
}

impl Failure {
  /// Validates diagnostic bounds and retry semantics.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    identifier("failure code", &self.code)?;
    bounded_text("failure diagnostic", &self.diagnostic, MAX_DIAGNOSTIC_BYTES)?;
    if self.retry_after_ms.is_some() && self.class != FailureClass::Transient {
      return Err(ProtocolError::Invalid("only transient failures may request retry"));
    }
    Ok(())
  }
}

/// Webhook protocol validation failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProtocolError {
  /// Peers have no compatible version.
  #[error("webhook protocol versions are incompatible")]
  IncompatibleVersion,
  /// A semantic invariant is invalid.
  #[error("invalid webhook protocol value: {0}")]
  Invalid(&'static str),
  /// A bounded value exceeds its limit.
  #[error("webhook protocol limit exceeded: {0}")]
  LimitExceeded(&'static str),
  /// Encoded JSON is malformed or contains unknown fields.
  #[error("malformed webhook protocol message")]
  MalformedMessage,
  /// A response does not match the request version or identifier.
  #[error("webhook protocol response does not correlate to its request")]
  CorrelationMismatch,
}

fn bounded_message(message: &[u8]) -> Result<(), ProtocolError> {
  adapter_core::require_message_size(message, MAX_WEBHOOK_MESSAGE_BYTES)
    .map_err(|_| ProtocolError::LimitExceeded("encoded message"))
}

fn identifier(field: &'static str, value: &str) -> Result<(), ProtocolError> {
  bounded_text(field, value, MAX_IDENTIFIER_BYTES)
}

fn bounded_text(field: &'static str, value: &str, max: usize) -> Result<(), ProtocolError> {
  adapter_core::require_bounded_text(value, max).map_err(|failure| match failure {
    ValidationFailure::LimitExceeded => ProtocolError::LimitExceeded(field),
    _ => ProtocolError::Invalid(field),
  })
}

fn bounded_map(values: &BTreeMap<String, String>, max_entries: usize) -> Result<(), ProtocolError> {
  if values.len() > max_entries {
    return Err(ProtocolError::LimitExceeded("map entries"));
  }
  for (key, value) in values {
    identifier("map key", key)?;
    bounded_text("map value", value, MAX_VALUE_BYTES)?;
  }
  Ok(())
}

fn sha256(value: &str) -> Result<(), ProtocolError> {
  adapter_core::require_sha256(value).map_err(|_| ProtocolError::Invalid("executable digest must be lowercase SHA-256"))
}

#[cfg(test)]
mod tests {
  use super::*;

  const FIXTURE: &str = include_str!("../fixtures/verify-delivery-v1.json");
  const MANAGED_FIXTURES: [&str; 4] = [
    include_str!("../fixtures/create-registration-v1.json"),
    include_str!("../fixtures/observe-registration-v1.json"),
    include_str!("../fixtures/rotate-registration-v1.json"),
    include_str!("../fixtures/delete-registration-v1.json"),
  ];

  #[test]
  fn golden_fixture_round_trips_and_unknown_fields_are_rejected() {
    let request = decode_request(FIXTURE.as_bytes()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(serde_json::to_value(request).unwrap(), expected);
    let mut unknown = expected;
    unknown["command"]["payload"]["provider_payload"] = serde_json::json!({});
    assert!(decode_request(&serde_json::to_vec(&unknown).unwrap()).is_err());
  }

  #[test]
  fn managed_registration_conformance_fixtures_are_strict_and_secret_safe() {
    for fixture in MANAGED_FIXTURES {
      let request = decode_request(fixture.as_bytes()).unwrap();
      let expected: serde_json::Value = serde_json::from_str(fixture).unwrap();
      assert_eq!(serde_json::to_value(&request).unwrap(), expected);
      assert!(!format!("{request:?}").contains("provider-administration-handle"));

      let mut unknown = expected;
      unknown["command"]["payload"]["provider_private_configuration"] = serde_json::json!({});
      assert!(decode_request(&serde_json::to_vec(&unknown).unwrap()).is_err());
    }
  }

  #[test]
  fn canonical_decoders_enforce_total_size_semantics_correlation_and_redaction() {
    assert_eq!(
      decode_request(&vec![b' '; MAX_WEBHOOK_MESSAGE_BYTES + 1]),
      Err(ProtocolError::LimitExceeded("encoded message"))
    );

    let mut invalid: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
    invalid["request_id"] = serde_json::json!("");
    assert!(decode_request(&serde_json::to_vec(&invalid).unwrap()).is_err());

    let request = decode_request(FIXTURE.as_bytes()).unwrap();
    let response = Response {
      protocol_version: WEBHOOK_PROTOCOL_VERSION,
      request_id: "different-request".to_owned(),
      outcome: Outcome::Acknowledged {
        operation_id: "operation-01".to_owned(),
      },
    };
    assert_eq!(response.validate_for(&request), Err(ProtocolError::CorrelationMismatch));

    let debug = format!("{request:?}");
    assert!(!debug.contains("secret-handle"));
    assert!(!debug.contains("cGF5bG9hZA=="));
    assert!(debug.contains("<redacted>"));
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
    let oversized = VerifyDelivery {
      operation_id: "op".to_owned(),
      integration_id: "integration".to_owned(),
      verification_material_handle: "secret".to_owned(),
      headers: BTreeMap::new(),
      body_base64: STANDARD.encode(vec![0_u8; MAX_DELIVERY_BYTES + 1]),
    };
    assert!(oversized.validate().is_err());
    let uppercase_header = VerifyDelivery {
      operation_id: "op".to_owned(),
      integration_id: "integration".to_owned(),
      verification_material_handle: "secret".to_owned(),
      headers: [("X-Signature".to_owned(), "signature".to_owned())]
        .into_iter()
        .collect(),
      body_base64: STANDARD.encode(b"payload"),
    };
    assert_eq!(
      uppercase_header.validate(),
      Err(ProtocolError::Invalid("header names must be lowercase ASCII tokens"))
    );
    assert!(
      Failure {
        class: FailureClass::Permanent,
        code: "bad_configuration".to_owned(),
        diagnostic: "configuration is invalid".to_owned(),
        retry_after_ms: Some(1),
      }
      .validate()
      .is_err()
    );
  }

  #[test]
  fn validation_distinguishes_invalid_values_from_size_limits() {
    assert_eq!(identifier("request_id", ""), Err(ProtocolError::Invalid("request_id")));
    assert_eq!(
      identifier("request_id", "line\nbreak"),
      Err(ProtocolError::Invalid("request_id"))
    );
    assert_eq!(
      identifier("request_id", &"x".repeat(MAX_IDENTIFIER_BYTES + 1)),
      Err(ProtocolError::LimitExceeded("request_id"))
    );
  }
}
