//! Independent role advertisement inside one verified provider installation.
use crate::*;

/// Inclusive independently negotiated role version range.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolRange {
  /// Oldest supported version.
  pub min: u16,
  /// Newest supported version.
  pub max: u16,
}
impl ProtocolRange {
  /// Selects the newest common version without exposing protected handles.
  pub fn negotiate(self, peer: Self) -> Result<u16, ProtocolError> {
    common::negotiate_version(self.min, self.max, peer.min, peer.max).map_err(|_| ProtocolError::IncompatibleVersion)
  }
}
/// Advertised role versions; absence never inherits another capability.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolRanges {
  /// Optional Source contract range.
  pub source: Option<ProtocolRange>,
  /// Optional independent Reporter contract range.
  pub reporter: Option<ProtocolRange>,
}
/// Read-only Source capability.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceCapabilities {
  /// Explicit opt-in for bounded read-only connection diagnostics.
  #[serde(default)]
  pub check: bool,
  /// Explicit current installed read-grant check, independent of ticket discovery.
  #[serde(default)]
  pub authorize: bool,
  /// Whether same-scan cooperative cancellation is supported.
  pub cancel: bool,
  /// Required independent ticket read class.
  pub credential_class: CredentialClass,
  /// Exact operator profile for discovery.
  pub configuration_digest: String,
}
/// Required stable write recovery contract.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IdempotencyMode {
  /// Inspect the original item/effects before any retry of an uncertain projection.
  ObserveBeforeRetry,
}
/// Separately authorized configured-effects capability.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReporterCapabilities {
  /// Explicit opt-in for bounded read-only connection diagnostics.
  #[serde(default)]
  pub check: bool,
  /// Read exact current effect state without mutation.
  pub observe: bool,
  /// Converge exact installed effects.
  pub project: bool,
  /// Whether same-projection cooperative cancellation is supported.
  pub cancel: bool,
  /// Stable recovery semantics required before writes can be selected.
  pub idempotency: IdempotencyMode,
  /// Required independent ticket write class.
  pub credential_class: CredentialClass,
  /// Exact operator profile for reporting.
  pub configuration_digest: String,
}
/// Roles of one provider executable; no role is enabled implicitly.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
  /// Discovery, when installed explicitly.
  pub source: Option<SourceCapabilities>,
  /// Independent reporting, when installed explicitly.
  pub reporter: Option<ReporterCapabilities>,
}
/// Verified provider identity and independent Work integration contracts.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterManifest {
  /// Open provider identity, independent of Flow phases and business labels.
  pub adapter_id: String,
  /// Operator-pinned executable content digest.
  pub executable_sha256: String,
  /// Independent optional role versions.
  pub protocol: ProtocolRanges,
  /// Explicit independent authority/operation declarations.
  pub capabilities: Capabilities,
}
impl AdapterManifest {
  /// Checks declarations without requiring an unsupported role to disable a compatible peer role.
  pub fn validate(&self) -> Result<(), ProtocolError> {
    field(&self.adapter_id)?;
    digest(&self.executable_sha256)?;
    if self.protocol.source.is_some() != self.capabilities.source.is_some()
      || self.protocol.reporter.is_some() != self.capabilities.reporter.is_some()
      || (self.capabilities.source.is_none() && self.capabilities.reporter.is_none())
    {
      return Err(ProtocolError::Invalid);
    }
    if let Some(range) = self.protocol.source {
      range.negotiate(range)?;
    }
    if let Some(range) = self.protocol.reporter {
      range.negotiate(range)?;
    }
    if let Some(source) = &self.capabilities.source {
      digest(&source.configuration_digest)?;
      if source.credential_class != CredentialClass::WorkSourceRead {
        return Err(ProtocolError::Invalid);
      }
    }
    if let Some(reporter) = &self.capabilities.reporter {
      digest(&reporter.configuration_digest)?;
      if reporter.credential_class != CredentialClass::WorkReporterWrite || (reporter.project && !reporter.observe) {
        return Err(ProtocolError::Invalid);
      }
    }
    Ok(())
  }
}
