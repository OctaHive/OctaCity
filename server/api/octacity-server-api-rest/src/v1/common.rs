use std::{fmt, str::FromStr};

use axum::http::HeaderValue;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use thiserror::Error;

/// HTTP header carrying a stable replay identity for every mutation.
pub const IDEMPOTENCY_KEY_HEADER: &str = "idempotency-key";
/// HTTP conditional header carrying the current resource version.
pub const OPTIMISTIC_PRECONDITION_HEADER: &str = "if-match";
/// Maximum UTF-8 bytes accepted in one idempotency header value.
pub const MAX_IDEMPOTENCY_KEY_BYTES: usize = 128;
/// Maximum bytes accepted in one opaque pagination cursor.
pub const MAX_CURSOR_BYTES: usize = 512;

/// Stable machine-readable failure code for management clients.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
  /// The request shape or one value is invalid.
  InvalidRequest,
  /// The request media type is not supported.
  UnsupportedMediaType,
  /// The requested management API version is not supported.
  UnsupportedApiVersion,
  /// The encoded request exceeds its endpoint limit.
  PayloadTooLarge,
  /// The mutation omitted or supplied an invalid idempotency key.
  InvalidIdempotencyKey,
  /// The same idempotency key was reused for different intent.
  IdempotencyConflict,
  /// A mutation requiring optimistic concurrency omitted `If-Match`.
  PreconditionRequired,
  /// The supplied optimistic version is no longer current.
  PreconditionFailed,
  /// The addressed resource does not exist or is not visible.
  NotFound,
  /// Current durable state conflicts with the requested operation.
  Conflict,
  /// A required capability is not available in this deployment.
  CapabilityUnavailable,
  /// A required server dependency is temporarily unavailable.
  Unavailable,
  /// A bounded request rate was exceeded.
  RateLimited,
  /// The server could not classify an internal failure more specifically.
  Internal,
}

/// Stable JSON error body returned by every management endpoint.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorResponse {
  /// Stable code intended for programmatic branching.
  pub code: ErrorCode,
  /// Safe bounded explanation intended for an operator.
  pub message: String,
  /// Correlation identity also returned in the `x-request-id` header.
  pub request_id: String,
}

/// Whether a mutating response was newly applied or exactly replayed.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationDisposition {
  /// This request committed new durable state.
  Applied,
  /// A prior identical request produced the returned state.
  Replayed,
}

/// Versioned mutation result containing a REST-owned resource DTO.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MutationResponse<T> {
  /// Whether this request applied or replayed the mutation.
  pub disposition: MutationDisposition,
  /// Resource state committed by the original request.
  pub resource: T,
}

/// Deterministic collection page with an opaque exclusive cursor.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CursorPage<T> {
  /// Stable ordered items in this bounded page.
  pub items: Vec<T>,
  /// Cursor for the following page, or `None` at the end.
  pub next_cursor: Option<Cursor>,
}

/// Bounded opaque pagination cursor owned by the REST adapter.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Cursor(String);

impl Cursor {
  /// Validates a non-empty printable cursor within the v1 size limit.
  pub fn new(value: impl Into<String>) -> Result<Self, ContractValueError> {
    let value = value.into();
    if visible(&value, MAX_CURSOR_BYTES) && !value.chars().any(char::is_whitespace) {
      Ok(Self(value))
    } else {
      Err(ContractValueError::InvalidCursor)
    }
  }

  /// Borrows the opaque encoded cursor.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

impl Serialize for Cursor {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str(&self.0)
  }
}

impl<'de> Deserialize<'de> for Cursor {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    String::deserialize(deserializer).and_then(|value| Self::new(value).map_err(D::Error::custom))
  }
}

/// Validated value of the required `Idempotency-Key` request header.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
  /// Validates visible trimmed caller intent within the v1 size limit.
  pub fn new(value: impl Into<String>) -> Result<Self, ContractValueError> {
    let value = value.into();
    if visible(&value, MAX_IDEMPOTENCY_KEY_BYTES) {
      Ok(Self(value))
    } else {
      Err(ContractValueError::InvalidIdempotencyKey)
    }
  }

  /// Parses an HTTP header without accepting lossy text conversion.
  pub fn from_header(value: &HeaderValue) -> Result<Self, ContractValueError> {
    value
      .to_str()
      .map_err(|_| ContractValueError::InvalidIdempotencyKey)
      .and_then(Self::new)
  }

  /// Borrows the validated caller key.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }

  /// Encodes the validated key for an HTTP request.
  pub fn to_header_value(&self) -> HeaderValue {
    HeaderValue::from_str(&self.0).expect("validated visible ASCII is a legal header value")
  }
}

impl FromStr for IdempotencyKey {
  type Err = ContractValueError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    Self::new(value)
  }
}

/// Strong `If-Match` entity tag carrying one positive resource version.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VersionPrecondition(u64);

impl VersionPrecondition {
  /// Creates a strong precondition for one positive resource version.
  pub fn new(version: u64) -> Result<Self, ContractValueError> {
    if version == 0 {
      Err(ContractValueError::InvalidVersionPrecondition)
    } else {
      Ok(Self(version))
    }
  }

  /// Parses exactly one canonical strong `If-Match` entity tag such as `"7"`.
  pub fn from_header(value: &HeaderValue) -> Result<Self, ContractValueError> {
    let value = value
      .to_str()
      .map_err(|_| ContractValueError::InvalidVersionPrecondition)?;
    value.parse()
  }

  /// Returns the expected positive resource version.
  #[must_use]
  pub const fn version(self) -> u64 {
    self.0
  }

  /// Encodes this precondition as one canonical strong entity tag.
  pub fn to_header_value(self) -> HeaderValue {
    HeaderValue::from_str(&self.to_string()).expect("a quoted integer is a legal header value")
  }
}

impl FromStr for VersionPrecondition {
  type Err = ContractValueError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    let raw = value
      .strip_prefix('"')
      .and_then(|value| value.strip_suffix('"'))
      .ok_or(ContractValueError::InvalidVersionPrecondition)?;
    let version = raw
      .parse::<u64>()
      .map_err(|_| ContractValueError::InvalidVersionPrecondition)?;
    if raw != version.to_string() {
      return Err(ContractValueError::InvalidVersionPrecondition);
    }
    Self::new(version)
  }
}

impl fmt::Display for VersionPrecondition {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "\"{}\"", self.0)
  }
}

/// Classified invalid v1 wire value.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ContractValueError {
  /// An opaque collection cursor is empty, unsafe, or oversized.
  #[error("invalid pagination cursor")]
  InvalidCursor,
  /// An idempotency header is empty, unsafe, or oversized.
  #[error("invalid idempotency key")]
  InvalidIdempotencyKey,
  /// An optimistic precondition is not one canonical positive strong ETag.
  #[error("invalid optimistic version precondition")]
  InvalidVersionPrecondition,
}

fn visible(value: &str, maximum: usize) -> bool {
  !value.is_empty()
    && value.len() <= maximum
    && value.trim() == value
    && value.is_ascii()
    && !value.chars().any(char::is_control)
}
