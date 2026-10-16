//! Capability-specific protected handles and immutable installed references.
use crate::*;

/// Closed Work integration credential classes; Delivery and repository credentials are absent.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialClass {
  /// Read-only ticket discovery, not code repository access.
  WorkSourceRead,
  /// Separately authorized configured ticket effects.
  WorkReporterWrite,
}
/// Opaque installed grant selection, never a token, path or provider-supplied authority.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CredentialHandle {
  /// Required grant class, checked before process execution and again by the provider.
  pub class: CredentialClass,
  /// Private broker/installation handle.
  pub handle: String,
}
impl std::fmt::Debug for CredentialHandle {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("CredentialHandle")
      .field("class", &self.class)
      .finish_non_exhaustive()
  }
}
/// Exact ticket container, independent of the code Repository selected for Factory.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceAccess {
  /// Stable native repository, project or tracker container identity.
  pub resource_id: String,
  /// Operator-installed alias; never a caller-supplied URL.
  pub resource_locator: String,
  /// Explicit role-specific protected handle.
  pub credential: CredentialHandle,
}
impl ResourceAccess {
  /// Checks normalized scope without resolving any protected handle.
  pub fn validate(&self, class: CredentialClass) -> Result<(), ProtocolError> {
    field(&self.resource_id)?;
    field(&self.resource_locator)?;
    field(&self.credential.handle)?;
    if self.credential.class != class {
      return Err(ProtocolError::Invalid);
    }
    Ok(())
  }
}
impl std::fmt::Debug for ResourceAccess {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("ResourceAccess")
      .field("resource_id", &self.resource_id)
      .finish_non_exhaustive()
  }
}
/// Exact installed selector or effect mapping; spelling carries no lifecycle meaning.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationReference {
  /// Installed configuration identity.
  pub identity: String,
  /// Immutable normalized configuration content.
  pub sha256: String,
}
impl ConfigurationReference {
  /// Rejects missing identity or mutable content selection.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    field(&self.identity)?;
    digest(&self.sha256)
  }
}
