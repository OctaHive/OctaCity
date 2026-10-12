use crate::*;
use serde::{Deserialize, Serialize};

/// Independently negotiated supported Delivery version range.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolRange {
  /// Oldest supported version, inclusive.
  pub min: u16,
  /// Newest supported version, inclusive.
  pub max: u16,
}
impl ProtocolRange {
  /// Selects the newest supported common version before exposing any handle.
  pub fn negotiate(self, peer: Self) -> Result<u16, ProtocolError> {
    common::negotiate_version(self.min, self.max, peer.min, peer.max).map_err(|_| ProtocolError::IncompatibleVersion)
  }
}
/// Required stable replay contract; weaker write adapters cannot be selected.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IdempotencyMode {
  /// Inspect the same stable identity before any retry of a potentially accepted write.
  ObserveBeforeRetry,
}
/// Capability declaration of one installed plugin at the Delivery seam.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
  /// Observe exact branch/review state without mutation.
  pub observe: bool,
  /// Publish an exact accepted candidate and create/update its review.
  pub publish: bool,
  /// Cooperatively cancel the original operation.
  pub cancel: bool,
  /// Verified stable replay semantics.
  pub idempotency: IdempotencyMode,
  /// Credential class required by this capability; separate from other roles of the same plugin.
  pub credential_class: CredentialClass,
  /// Immutable operator provider profile; validated before passing protected handles.
  pub configuration_digest: String,
}
/// Exact installed plugin identity, executable and Delivery contract.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterManifest {
  /// Stable provider-plugin identity, independent of Flow and node names.
  pub adapter_id: String,
  /// Operator-pinned executable digest.
  pub executable_sha256: String,
  /// Independently negotiated Delivery range.
  pub protocol: ProtocolRange,
  /// Declared supported operations and their authority/replay contract.
  pub capabilities: Capabilities,
}
impl AdapterManifest {
  /// Rejects unsafe publication capability before executable or credential use.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    field(&self.adapter_id)?;
    digest(&self.executable_sha256)?;
    digest(&self.capabilities.configuration_digest)?;
    self.protocol.negotiate(self.protocol)?;
    if self.capabilities.publish && !self.capabilities.observe {
      return Err(ProtocolError::Invalid);
    }
    Ok(())
  }
}
