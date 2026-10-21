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
  /// Optional independently versioned repository review reader.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub review: Option<ProtocolRange>,
}
/// Read-only Source capability.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceCapabilities {
  /// Explicit opt-in for summaries/reasons, without production scan or task events.
  #[serde(default)]
  pub preview: bool,
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
  /// Explicit support for immutable input-owned effects under the installed mapping ceiling.
  #[serde(default)]
  pub custom_effects: bool,
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
/// Read-only code repository observations using existing repository credentials.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewCapabilities {
  /// Explicit support for bounded head-specific check pages.
  pub checks: bool,
  /// Same-read cooperative cancellation.
  pub cancel: bool,
  /// Required independently scoped repository read class.
  pub credential_class: CredentialClass,
  /// Exact installed transport/repository profile.
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
  /// Independently installed repository reader, never inherited from other roles.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub review: Option<ReviewCapabilities>,
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
      || self.protocol.review.is_some() != self.capabilities.review.is_some()
      || (self.capabilities.source.is_none()
        && self.capabilities.reporter.is_none()
        && self.capabilities.review.is_none())
    {
      return Err(ProtocolError::Invalid);
    }
    if let Some(range) = self.protocol.source {
      range.negotiate(range)?;
    }
    if let Some(range) = self.protocol.reporter {
      range.negotiate(range)?;
    }
    if let Some(range) = self.protocol.review {
      range.negotiate(range)?;
    }
    if let Some(review) = &self.capabilities.review {
      digest(&review.configuration_digest)?;
      if review.credential_class != CredentialClass::RepositoryRead {
        return Err(ProtocolError::Invalid);
      }
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
      if reporter.custom_effects
        && !self.protocol.reporter.is_some_and(|range| {
          range.min <= reporter::ORDERED_PROTOCOL_VERSION && range.max >= reporter::ORDERED_PROTOCOL_VERSION
        })
      {
        return Err(ProtocolError::Invalid);
      }
    }
    Ok(())
  }
}
