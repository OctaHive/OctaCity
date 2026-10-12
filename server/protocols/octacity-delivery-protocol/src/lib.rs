//! Provider-neutral trusted Delivery process contract, separate from read-only VCS.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod manifest;
mod request;
mod response;
pub use manifest::*;
use octacity_adapter_protocol as common;
pub use request::*;
pub use response::*;

/// Independently negotiated Delivery protocol version.
pub const DELIVERY_PROTOCOL_VERSION: u16 = 1;
/// Complete JSON frame ceiling; Artifact bytes travel through protected handles.
pub const MAX_DELIVERY_MESSAGE_BYTES: usize = 64 * 1024;
/// Maximum size of one logical identifier, revision or protected handle.
pub const MAX_DELIVERY_FIELD_BYTES: usize = 1024;
/// Maximum encoded candidate bundle size authorized by this protocol.
pub const MAX_DELIVERY_BUNDLE_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum encoded canonical capture manifest size.
pub const MAX_DELIVERY_MANIFEST_BYTES: u64 = 1024 * 1024;

/// Secret-free failure at the Delivery process boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProtocolError {
  /// A field, operation or authority class violates the protocol contract.
  #[error("invalid Delivery contract")]
  Invalid,
  /// The peer selected an unsupported protocol version.
  #[error("incompatible Delivery protocol version")]
  IncompatibleVersion,
  /// One encoded input exceeds its fixed contract ceiling.
  #[error("Delivery contract exceeds its bound")]
  LimitExceeded,
}
fn field(value: &str) -> Result<(), ProtocolError> {
  common::require_bounded_text(value, MAX_DELIVERY_FIELD_BYTES).map_err(|_| ProtocolError::Invalid)
}
fn digest(value: &str) -> Result<(), ProtocolError> {
  common::require_sha256(value).map_err(|_| ProtocolError::Invalid)
}
/// Decodes one bounded strict request before any credential resolution or effect.
pub fn decode_request(bytes: &[u8]) -> Result<Request, ProtocolError> {
  common::require_message_size(bytes, MAX_DELIVERY_MESSAGE_BYTES).map_err(|_| ProtocolError::LimitExceeded)?;
  let request: Request = common::decode_json(bytes).map_err(|_| ProtocolError::Invalid)?;
  request.validate()?;
  Ok(request)
}
/// Decodes a bounded response and verifies exact request/candidate correlation.
pub fn decode_response(bytes: &[u8], request: &Request) -> Result<Response, ProtocolError> {
  common::require_message_size(bytes, MAX_DELIVERY_MESSAGE_BYTES).map_err(|_| ProtocolError::LimitExceeded)?;
  let response: Response = common::decode_json(bytes).map_err(|_| ProtocolError::Invalid)?;
  response.validate_for(request)?;
  Ok(response)
}
