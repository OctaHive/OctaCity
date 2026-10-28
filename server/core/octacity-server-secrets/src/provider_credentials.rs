//! Integration credentials are transient private input, separate from Job grants.
use std::fmt;
use zeroize::Zeroizing;

/// Maximum transient input accepted by the common provider-credential interface.
/// Concrete providers may impose a smaller authentication-specific bound.
pub const MAX_PROVIDER_CREDENTIAL_BYTES: usize = 16 * 1024;

/// Zeroing, write-only integration credential input. Deliberately not serializable,
/// printable or cloneable; it cannot enter ordinary database/Work representations.
///
/// ```compile_fail
/// fn durable<T: serde::Serialize>() {}
/// durable::<octacity_server_secrets::ProviderCredentialValue>();
/// ```
/// ```compile_fail
/// fn copyable<T: Clone>() {}
/// copyable::<octacity_server_secrets::ProviderCredentialValue>();
/// ```
/// ```compile_fail
/// fn printable<T: std::fmt::Display>() {}
/// printable::<octacity_server_secrets::ProviderCredentialValue>();
/// ```
pub struct ProviderCredentialValue(Zeroizing<Vec<u8>>);
impl ProviderCredentialValue {
  /// Accepts a bounded non-empty value; rejected input is also zeroed on drop.
  pub fn new(bytes: Vec<u8>) -> Result<Self, ProviderCredentialError> {
    let bytes = Zeroizing::new(bytes);
    if bytes.is_empty() || bytes.len() > MAX_PROVIDER_CREDENTIAL_BYTES {
      return Err(ProviderCredentialError::InvalidValue);
    }
    Ok(Self(bytes))
  }
  /// Borrows transient input only for a private installation writer or connection verifier.
  #[must_use]
  pub fn expose_for_installation(&self) -> &[u8] {
    &self.0
  }
}
impl fmt::Debug for ProviderCredentialValue {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str("ProviderCredentialValue([REDACTED])")
  }
}
// Deserialization is deliberately one-way, for the protected submission only.
impl<'de> serde::Deserialize<'de> for ProviderCredentialValue {
  fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
    let value = Zeroizing::new(<String as serde::Deserialize>::deserialize(deserializer)?);
    Self::new(value.as_bytes().to_vec()).map_err(|_| serde::de::Error::custom("invalid credential value"))
  }
}

/// Safe credential-management failure without secret values, paths or OS diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProviderCredentialError {
  /// Input is empty, oversized or invalid for the installed authentication method.
  #[error("invalid provider credential value")]
  InvalidValue,
  /// The selected purpose/resource handle is not installed or its material is missing.
  #[error("provider credential is not configured")]
  NotConfigured,
  /// A new credential cannot overwrite existing installed material.
  #[error("provider credential publication conflicts")]
  Conflict,
  /// Installed configuration, file ownership or private permissions are invalid.
  #[error("provider credential operation denied")]
  Denied,
  /// Protected storage could not complete the operation.
  #[error("provider credential storage unavailable")]
  Unavailable,
}

/// Only safe logical identity and configured state cross the management interface.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct ProviderCredentialReceipt {
  /// Original logical purpose handle; never a filesystem path or secret digest.
  pub reference: crate::LogicalSecretReference,
  /// Successful publication/adoption establishes configured material, not remote readiness.
  pub configured: bool,
}

/// Trusted write-only integration administration, independent of provider wire
/// formats and Job secret grants. Implementations resolve only installed handles.
/// Application authorization and durable administrative replay belong above this port.
///
/// ```compile_fail
/// use octacity_server_secrets::{ProviderCredentialManager, SecretGrantProvider};
/// fn job_grants_are_not_administration(job: &dyn SecretGrantProvider) {
///   let _: &dyn ProviderCredentialManager = job;
/// }
/// ```
#[async_trait::async_trait]
pub trait ProviderCredentialManager: Send + Sync {
  /// Creates protected material without replacing an existing purpose/resource handle.
  async fn provision(
    &self,
    reference: &crate::LogicalSecretReference,
    value: ProviderCredentialValue,
  ) -> Result<ProviderCredentialReceipt, ProviderCredentialError>;
  /// Atomically replaces private material behind the same installed purpose/resource handle.
  /// Administrative ordering, replay and revocation are enforced above this storage primitive.
  async fn rotate(
    &self,
    reference: &crate::LogicalSecretReference,
    value: ProviderCredentialValue,
  ) -> Result<ProviderCredentialReceipt, ProviderCredentialError>;
  /// Adopts an existing installed purpose/resource handle without returning or rewriting its value.
  async fn adopt(
    &self,
    reference: &crate::LogicalSecretReference,
  ) -> Result<ProviderCredentialReceipt, ProviderCredentialError>;
}
