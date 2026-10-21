//! Independent provider-neutral Work Source, Reporter and review read contracts.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use octacity_adapter_protocol as common;
use serde::{Deserialize, Serialize};

mod access;
pub mod check;
pub mod preview;
pub mod reporter;
pub mod review;
pub mod source;
pub use access::*;
mod snapshot;
pub use snapshot::*;
mod manifest;
pub use manifest::*;

/// Complete request frame ceiling; no credential bytes or provider DTOs cross this boundary.
pub const MAX_REQUEST_BYTES: usize = 64 * 1024;
/// Complete normalized response ceiling, including private candidate data.
pub const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
/// Maximum length of one logical identifier or protected handle.
pub const MAX_FIELD_BYTES: usize = 1024;

/// Secret-free failure while checking an independently scoped Work integration frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProtocolError {
  /// A closed field, operation or authority class is invalid.
  #[error("invalid work integration contract")]
  Invalid,
  /// The selected role's version is unsupported.
  #[error("incompatible work integration protocol version")]
  IncompatibleVersion,
  /// An encoded frame or normalized value exceeds its ceiling.
  #[error("work integration contract exceeds its bound")]
  LimitExceeded,
}

/// Provider-neutral safe failure categories; no free-form diagnostic field exists.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
  /// Invalid scope or input, rejected before any effect.
  InvalidRequest,
  /// This installed role or operation is unavailable.
  Unsupported,
  /// Original scope or desired effects conflict with observed state.
  Conflict,
  /// Operator intervention or credential/configuration repair is required.
  Permanent,
  /// Provider outage, rate limit or uncertain effect; retry under owner policy.
  Transient,
  /// Original operation stopped.
  Cancelled,
  /// Provider normalization or framing violated the contract.
  ProtocolFault,
}
/// Safe classified failure, distinct from an empty successful discovery page.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Failure {
  /// Closed failure category.
  pub class: FailureClass,
  /// Bounded retry hint; cannot enlarge the original deadline/retry policy.
  pub retry_after_millis: Option<u32>,
}
impl Failure {
  /// Validates the hint without exposing provider diagnostics.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    if self.retry_after_millis.is_some_and(|delay| delay > 3_600_000) {
      return Err(ProtocolError::Invalid);
    }
    Ok(())
  }
}

/// Disjoint protocol families; sharing a provider executable grants no other role.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "family", content = "message", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
  /// Independently versioned read-only discovery.
  Source(source::Request),
  /// Independently versioned projection of accepted configured effects.
  Reporter(reporter::Request),
  /// Independently authorized repository review reads.
  Review(review::Request),
}
/// One bounded terminal observation, never a Factory command or execution outcome.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "family", content = "message", rename_all = "snake_case", deny_unknown_fields)]
pub enum Response {
  /// Discovery's correlated terminal response.
  Source(source::Response),
  /// Reporter's correlated terminal response.
  Reporter(reporter::Response),
  /// Correlated repository review facts.
  Review(review::Response),
}
impl Request {
  /// Independent diagnostic ceiling; private discovery keeps its existing bound.
  pub fn max_response_bytes(&self) -> usize {
    match self {
      Self::Source(source::Request {
        command: source::Command::Check(_),
        ..
      })
      | Self::Reporter(reporter::Request {
        command: reporter::Command::Check(_),
        ..
      }) => check::MAX_CHECK_RESPONSE_BYTES,
      _ => MAX_RESPONSE_BYTES,
    }
  }
  /// Validates the selected role before any protected handle is exposed.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    match self {
      Self::Source(request) => request.validate(),
      Self::Reporter(request) => request.validate(),
      Self::Review(request) => request.validate(),
    }
  }
}
impl Response {
  /// Checks role, version, request and stable operation identity.
  pub fn validate_for(&self, request: &Request) -> Result<(), ProtocolError> {
    request.validate()?;
    match (self, request) {
      (Self::Source(response), Request::Source(request)) => response.validate_for(request),
      (Self::Reporter(response), Request::Reporter(request)) => response.validate_for(request),
      (Self::Review(response), Request::Review(request)) => response.validate_for(request),
      _ => Err(ProtocolError::Invalid),
    }
  }
}
/// Decodes one closed request with a hard allocation/input ceiling.
pub fn decode_request(bytes: &[u8]) -> Result<Request, ProtocolError> {
  common::require_message_size(bytes, MAX_REQUEST_BYTES).map_err(|_| ProtocolError::LimitExceeded)?;
  let request: Request = common::decode_json(bytes).map_err(|_| ProtocolError::Invalid)?;
  request.validate()?;
  Ok(request)
}
/// Decodes one closed response and verifies its original request scope.
pub fn decode_response(bytes: &[u8], request: &Request) -> Result<Response, ProtocolError> {
  common::require_message_size(bytes, request.max_response_bytes()).map_err(|_| ProtocolError::LimitExceeded)?;
  if let Request::Source(source::Request {
    command: source::Command::Scan(intent) | source::Command::Preview(intent),
    ..
  }) = request
  {
    common::require_message_size(bytes, intent.max_page_bytes as usize).map_err(|_| ProtocolError::LimitExceeded)?;
  }
  if let Request::Review(review::Request {
    command: review::Command::Observe(intent) | review::Command::Checks(intent) | review::Command::Authorize(intent),
    ..
  }) = request
  {
    common::require_message_size(bytes, intent.max_page_bytes as usize).map_err(|_| ProtocolError::LimitExceeded)?;
  }
  let response: Response = common::decode_json(bytes).map_err(|_| ProtocolError::Invalid)?;
  response.validate_for(request)?;
  Ok(response)
}
fn field(value: &str) -> Result<(), ProtocolError> {
  common::require_bounded_text(value, MAX_FIELD_BYTES).map_err(|_| ProtocolError::Invalid)
}
fn digest(value: &str) -> Result<(), ProtocolError> {
  common::require_sha256(value).map_err(|_| ProtocolError::Invalid)
}
