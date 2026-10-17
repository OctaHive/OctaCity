//! Read-only installed connection checks, without task selection or execution authority.
use crate::*;

/// Small independent response ceiling for connection diagnostics.
pub const MAX_CHECK_RESPONSE_BYTES: usize = 8192;
/// Closed readiness of one independently checked aspect; never an authority grant.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
  /// Confirmed by this bounded read.
  Ready,
  /// Required installed configuration or credential is absent/unusable.
  Missing,
  /// Configuration or request is outside its permitted ceiling.
  Invalid,
  /// Authentication or resource access was refused.
  Denied,
  /// Native identity differs from the original installed target.
  Conflict,
  /// The installed capability does not support this check.
  Unsupported,
  /// This aspect has not been proven read-only.
  Unverified,
  /// Provider requires a later retry.
  Throttled,
  /// Provider or verified process is unavailable.
  Unavailable,
}
/// One exact diagnostic request; no selector, task, cursor, mapping or mutation exists here.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckIntent {
  /// Original bounded check identity, also used for cancellation.
  pub check_id: String,
  /// Exact installed provider profile.
  pub configuration_digest: String,
  /// Expected approved endpoint; never used as an arbitrary fetch destination.
  pub endpoint: String,
  /// Exact native container and the selected role's private handle.
  pub resource: ResourceAccess,
  /// Installed trusted network ceiling.
  pub network_hosts: Vec<String>,
}
impl CheckIntent {
  /// Rejects wrong role classes before exposing any private handle.
  pub fn validate(&self, class: CredentialClass) -> Result<(), ProtocolError> {
    digest(&self.check_id)?;
    digest(&self.configuration_digest)?;
    common::require_bounded_text(&self.endpoint, 2048).map_err(|_| ProtocolError::Invalid)?;
    if !self.endpoint.starts_with("https://") {
      return Err(ProtocolError::Invalid);
    }
    self.resource.validate(class)?;
    common::require_network_hosts(&self.network_hosts).map_err(|_| ProtocolError::Invalid)
  }
}
/// Correlated safe facts. A successful check cannot activate a connection or authorize a write.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckReport {
  /// Original check identity.
  pub check_id: String,
  /// Original installed profile.
  pub configuration_digest: String,
  /// Original native container; Ready access confirms this exact identity.
  pub resource_id: String,
  /// Installed configuration readiness.
  pub configuration: CheckState,
  /// Authentication readiness, independent of resource permissions.
  pub authentication: CheckState,
  /// Exact native resource access/identity readiness.
  pub resource_access: CheckState,
  /// Requested operation readiness; write access may remain Unverified.
  pub capability: CheckState,
  /// Bounded provider retry advice, without an automatic retry.
  pub retry_after_millis: Option<u32>,
}
impl CheckReport {
  /// Starts a scoped diagnostic with no prerequisite or operation claimed ready.
  pub fn unverified(intent: &CheckIntent) -> Self {
    Self {
      check_id: intent.check_id.clone(),
      configuration_digest: intent.configuration_digest.clone(),
      resource_id: intent.resource.resource_id.clone(),
      configuration: CheckState::Unverified,
      authentication: CheckState::Unverified,
      resource_access: CheckState::Unverified,
      capability: CheckState::Unverified,
      retry_after_millis: None,
    }
  }

  /// Rejects substituted scope and claims of readiness after an unverified prerequisite.
  pub fn validate_for(&self, intent: &CheckIntent) -> Result<(), ProtocolError> {
    if self.check_id != intent.check_id
      || self.configuration_digest != intent.configuration_digest
      || self.resource_id != intent.resource.resource_id
      || self.retry_after_millis.is_some_and(|v| v > 3_600_000)
      || (self.authentication == CheckState::Ready && self.configuration != CheckState::Ready)
      || (self.resource_access == CheckState::Ready && self.authentication != CheckState::Ready)
      || (self.capability == CheckState::Ready && self.resource_access != CheckState::Ready)
    {
      return Err(ProtocolError::Invalid);
    }
    Ok(())
  }
}
