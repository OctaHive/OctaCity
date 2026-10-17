//! Recoverable private publication; retained identities contain no secret-derived data.
use crate::{LogicalSecretReference, ProviderCredentialError, ProviderCredentialValue};
use serde::{Deserialize, Serialize};

/// Stable administrative operation and monotonically increasing material generation.
/// The authoritative store allocates it before any private preparation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, try_from = "PublicationWire")]
pub struct CredentialPublication {
  /// Exact installed purpose/resource reference.
  pub reference: LogicalSecretReference,
  /// Opaque identity derived only from the accepted safe administrative intent.
  pub operation: [u8; 32],
  /// Positive fencing generation, shared by rotations and revocations of this handle.
  pub generation: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicationWire {
  reference: LogicalSecretReference,
  operation: [u8; 32],
  generation: u64,
}
impl TryFrom<PublicationWire> for CredentialPublication {
  type Error = ProviderCredentialError;
  fn try_from(wire: PublicationWire) -> Result<Self, Self::Error> {
    Self::new(wire.reference, wire.operation, wire.generation)
  }
}
impl CredentialPublication {
  /// Rejects invalid retained identities, including on deserialization.
  pub fn new(
    reference: LogicalSecretReference,
    operation: [u8; 32],
    generation: u64,
  ) -> Result<Self, ProviderCredentialError> {
    if operation == [0; 32] || generation == 0 || generation > i64::MAX as u64 {
      return Err(ProviderCredentialError::Denied);
    }
    Ok(Self {
      reference,
      operation,
      generation,
    })
  }
}
/// Purpose-preserving operation; none can select another installation or native resource.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialPublicationKind {
  /// Configure an absent or explicitly revoked handle under a newer generation.
  Provision,
  /// Adopt existing private material without exposing its value.
  Adopt,
  /// Replace configured material behind the original handle.
  Rotate,
  /// Install a durable private tombstone, preventing older writers from restoring authority.
  Revoke,
}
/// Private half of recoverable administration. Implementations atomically fence
/// activation across processes and preserve that fence across restarts/revocation.
/// Neither preparation nor cleanup confers connection or Project-use authority.
#[async_trait::async_trait]
pub trait ProviderCredentialPublisher: Send + Sync {
  /// Retains bounded private input under the original operation. Repeating a prepared
  /// operation uses its original bytes, even when no secret is resubmitted.
  async fn prepare(
    &self,
    publication: &CredentialPublication,
    kind: CredentialPublicationKind,
    value: Option<ProviderCredentialValue>,
  ) -> Result<(), ProviderCredentialError>;
  /// Publishes only the retained preparation. Equal-operation recovery is idempotent;
  /// lower or conflicting generations cannot overwrite newer material or revocation.
  async fn activate(&self, publication: &CredentialPublication) -> Result<(), ProviderCredentialError>;
  /// Erases only this operation's prepared secret and retains a safe cancellation
  /// tombstone so delayed preparation/activation cannot recreate an orphan.
  async fn discard(&self, publication: &CredentialPublication) -> Result<(), ProviderCredentialError>;
}
