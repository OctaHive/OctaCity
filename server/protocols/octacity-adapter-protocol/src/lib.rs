//! Small shared invariants for versioned adapter process protocols.
//!
//! This crate deliberately does not own any provider operation or envelope.
//! Each protocol remains independently versioned while delegating only the
//! byte, text, digest, negotiation, decoding, and correlation mechanics that
//! must behave identically at every process boundary.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use serde::de::DeserializeOwned;

/// Failure category returned by shared protocol validation primitives.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationFailure {
  /// One range contains zero or has its bounds reversed.
  InvalidRange,
  /// Two valid version ranges do not overlap.
  IncompatibleVersion,
  /// A byte or text value exceeds its declared bound.
  LimitExceeded,
  /// Required text is empty or contains a control character.
  InvalidText,
  /// A digest is not 64 lowercase hexadecimal characters.
  InvalidSha256,
  /// JSON is malformed or violates the target's strict schema.
  MalformedMessage,
  /// A response version or request identity does not match its request.
  CorrelationMismatch,
}

/// Selects the newest version common to two inclusive ranges.
pub fn negotiate_version(
  local_min: u16,
  local_max: u16,
  peer_min: u16,
  peer_max: u16,
) -> Result<u16, ValidationFailure> {
  if local_min == 0 || peer_min == 0 || local_min > local_max || peer_min > peer_max {
    return Err(ValidationFailure::InvalidRange);
  }
  let selected = local_max.min(peer_max);
  if selected < local_min.max(peer_min) {
    return Err(ValidationFailure::IncompatibleVersion);
  }
  Ok(selected)
}

/// Rejects a complete encoded message above its protocol-specific byte limit.
pub fn require_message_size(message: &[u8], max_bytes: usize) -> Result<(), ValidationFailure> {
  if message.len() > max_bytes {
    Err(ValidationFailure::LimitExceeded)
  } else {
    Ok(())
  }
}

/// Requires non-empty control-free text within one UTF-8 byte limit.
pub fn require_bounded_text(value: &str, max_bytes: usize) -> Result<(), ValidationFailure> {
  if value.is_empty() || value.chars().any(char::is_control) {
    return Err(ValidationFailure::InvalidText);
  }
  if value.len() > max_bytes {
    return Err(ValidationFailure::LimitExceeded);
  }
  Ok(())
}

/// Requires a canonical lowercase hexadecimal SHA-256 digest.
pub fn require_sha256(value: &str) -> Result<(), ValidationFailure> {
  if value.len() == 64
    && value
      .bytes()
      .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
  {
    Ok(())
  } else {
    Err(ValidationFailure::InvalidSha256)
  }
}

/// Decodes one strict protocol-owned JSON representation.
pub fn decode_json<T: DeserializeOwned>(message: &[u8]) -> Result<T, ValidationFailure> {
  serde_json::from_slice(message).map_err(|_| ValidationFailure::MalformedMessage)
}

/// Requires a response to echo the exact request version and identity.
pub fn require_correlation(
  response_version: u16,
  response_id: &str,
  request_version: u16,
  request_id: &str,
) -> Result<(), ValidationFailure> {
  if response_version == request_version && response_id == request_id {
    Ok(())
  } else {
    Err(ValidationFailure::CorrelationMismatch)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn shared_invariants_cover_edges_without_owning_wire_schemas() {
    assert_eq!(negotiate_version(1, 3, 2, 4), Ok(3));
    assert_eq!(
      negotiate_version(1, 1, 2, 2),
      Err(ValidationFailure::IncompatibleVersion)
    );
    assert_eq!(require_bounded_text("ok", 2), Ok(()));
    assert_eq!(require_sha256(&"a".repeat(64)), Ok(()));
    assert_eq!(require_correlation(1, "id", 1, "id"), Ok(()));
  }
}
