use std::{collections::BTreeSet, fmt, num::NonZeroU16, str::FromStr};

use octacity_server_domain::{AgentId, BuildId, PoolId, Timestamp};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::{StoreError, StoreInputError, StoreOperation};

/// Maximum number of resource targets selected by one attention request.
pub const MAX_OPERATOR_ATTENTION_TARGETS: usize = 64;
/// Maximum number of items returned by one attention page.
pub const MAX_OPERATOR_ATTENTION_PAGE_SIZE: u16 = 100;
/// Maximum UTF-8 bytes retained in one server-classified attention code.
pub const MAX_OPERATOR_ATTENTION_CODE_BYTES: usize = 64;
/// Maximum UTF-8 bytes returned in one safe attention summary.
pub const MAX_OPERATOR_ATTENTION_SUMMARY_BYTES: usize = 512;

/// Opaque identity of one server-runtime source of critical conditions.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CriticalSystemConditionSourceId(Uuid);

impl CriticalSystemConditionSourceId {
  /// Constructs a source identity from a non-nil UUID.
  pub fn from_uuid(value: Uuid) -> Result<Self, OperatorAttentionValueError> {
    if value.is_nil() {
      Err(OperatorAttentionValueError::InvalidIdentity)
    } else {
      Ok(Self(value))
    }
  }

  /// Returns the UUID representation used only inside persistence adapters.
  #[must_use]
  pub const fn as_uuid(self) -> Uuid {
    self.0
  }
}

/// Durable change to one server-classified critical system condition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CriticalSystemConditionChange {
  /// Opens one condition at the time the server first observes it.
  Open {
    /// Runtime source that owns this active condition.
    source: CriticalSystemConditionSourceId,
    /// Stable bounded server-owned classification code.
    code: String,
    /// Bounded non-secret operator summary.
    summary: String,
    /// Time at which the condition was observed.
    occurred_at: Timestamp,
  },
  /// Resolves every active occurrence with the same stable code.
  Resolve {
    /// Runtime source that owns this active condition.
    source: CriticalSystemConditionSourceId,
    /// Stable bounded server-owned classification code.
    code: String,
    /// Time at which the server observed recovery.
    resolved_at: Timestamp,
  },
}

impl CriticalSystemConditionChange {
  /// Constructs a validated critical-condition opening.
  pub fn open(
    source: CriticalSystemConditionSourceId,
    code: impl Into<String>,
    summary: impl Into<String>,
    occurred_at: Timestamp,
  ) -> Result<Self, OperatorAttentionValueError> {
    let code = code.into();
    let summary = summary.into();
    validate_code(&code)?;
    validate_safe_text(&summary, MAX_OPERATOR_ATTENTION_SUMMARY_BYTES)
      .map_err(|()| OperatorAttentionValueError::InvalidSummary)?;
    Ok(Self::Open {
      source,
      code,
      summary,
      occurred_at,
    })
  }

  /// Constructs a validated critical-condition resolution.
  pub fn resolve(
    source: CriticalSystemConditionSourceId,
    code: impl Into<String>,
    resolved_at: Timestamp,
  ) -> Result<Self, OperatorAttentionValueError> {
    let code = code.into();
    validate_code(&code)?;
    Ok(Self::Resolve {
      source,
      code,
      resolved_at,
    })
  }
}

/// Stable server-owned identity of one attention item.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OperatorAttentionId(Uuid);

impl OperatorAttentionId {
  /// Constructs an attention identity from a non-nil UUID.
  pub fn from_uuid(value: Uuid) -> Result<Self, OperatorAttentionValueError> {
    if value.is_nil() {
      Err(OperatorAttentionValueError::InvalidIdentity)
    } else {
      Ok(Self(value))
    }
  }

  /// Returns the UUID representation used by persistence adapters.
  #[must_use]
  pub const fn as_uuid(self) -> Uuid {
    self.0
  }
}

impl fmt::Display for OperatorAttentionId {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "{}", self.0.hyphenated())
  }
}

impl FromStr for OperatorAttentionId {
  type Err = OperatorAttentionValueError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    let parsed = Uuid::parse_str(value).map_err(|_| OperatorAttentionValueError::InvalidIdentity)?;
    if parsed.hyphenated().to_string() != value {
      return Err(OperatorAttentionValueError::InvalidIdentity);
    }
    Self::from_uuid(parsed)
  }
}

impl Serialize for OperatorAttentionId {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.collect_str(self)
  }
}

impl<'de> Deserialize<'de> for OperatorAttentionId {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    String::deserialize(deserializer)?.parse().map_err(D::Error::custom)
  }
}

/// Typed resource selected by an operator attention request.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum OperatorAttentionTarget {
  /// Immutable Build request and its current state.
  Build(BuildId),
  /// Registered Agent.
  Agent(AgentId),
  /// Static Agent Pool.
  AgentPool(PoolId),
}

/// Server-owned attention classification.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorAttentionCategory {
  /// Attention-worthy Build transition.
  Build,
  /// Attention-worthy Agent transition.
  Agent,
  /// Attention-worthy Agent Pool transition.
  AgentPool,
  /// Critical control-plane condition.
  CriticalSystem,
}

/// Server-owned severity retained by the UI without client inference.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorAttentionSeverity {
  /// Degraded resource requiring operator review.
  Warning,
  /// Failed resource or system condition requiring prompt attention.
  Critical,
}

/// Backend-neutral source event considered by attention classification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperatorAttentionEventKind {
  /// A Build entered the failed state.
  BuildFailed(BuildId),
  /// A Build completed normally and must not become an attention item.
  BuildSucceeded(BuildId),
  /// An Agent became unavailable.
  AgentUnavailable(AgentId),
  /// An Agent Pool became unavailable.
  AgentPoolUnavailable(PoolId),
  /// A server-classified critical system condition changed.
  CriticalSystemCondition {
    /// Bounded machine-readable condition code assigned by the server.
    code: String,
  },
  /// Ordinary audit activity, which is never an attention item.
  AuditActivity,
  /// Raw Job activity, which is never an attention item.
  JobActivity,
}

impl OperatorAttentionEventKind {
  fn target(&self) -> Option<OperatorAttentionTarget> {
    match self {
      Self::BuildFailed(identity) | Self::BuildSucceeded(identity) => Some(OperatorAttentionTarget::Build(*identity)),
      Self::AgentUnavailable(identity) => Some(OperatorAttentionTarget::Agent(*identity)),
      Self::AgentPoolUnavailable(identity) => Some(OperatorAttentionTarget::AgentPool(*identity)),
      Self::CriticalSystemCondition { .. } | Self::AuditActivity | Self::JobActivity => None,
    }
  }

  fn is_critical_system_condition(&self) -> bool {
    matches!(self, Self::CriticalSystemCondition { .. })
  }
}

/// Validated immutable event from which the server may derive an attention item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperatorAttentionEvent {
  id: OperatorAttentionId,
  kind: OperatorAttentionEventKind,
  summary: String,
  occurred_at: Timestamp,
  resolved_at: Option<Timestamp>,
}

impl OperatorAttentionEvent {
  /// Constructs a bounded safe source event.
  pub fn new(
    id: OperatorAttentionId,
    kind: OperatorAttentionEventKind,
    summary: impl Into<String>,
    occurred_at: Timestamp,
    resolved_at: Option<Timestamp>,
  ) -> Result<Self, OperatorAttentionValueError> {
    let summary = summary.into();
    validate_safe_text(&summary, MAX_OPERATOR_ATTENTION_SUMMARY_BYTES)
      .map_err(|_| OperatorAttentionValueError::InvalidSummary)?;
    if let OperatorAttentionEventKind::CriticalSystemCondition { code } = &kind {
      validate_code(code)?;
    }
    if resolved_at.is_some_and(|resolved| resolved < occurred_at) {
      return Err(OperatorAttentionValueError::InvalidResolutionTime);
    }
    Ok(Self {
      id,
      kind,
      summary,
      occurred_at,
      resolved_at,
    })
  }

  /// Returns the stable event identity.
  #[must_use]
  pub const fn id(&self) -> OperatorAttentionId {
    self.id
  }

  /// Returns the typed target used for visibility and scope checks.
  #[must_use]
  pub fn target(&self) -> Option<OperatorAttentionTarget> {
    self.kind.target()
  }

  /// Returns whether this source is a critical system condition.
  #[must_use]
  pub fn is_critical_system_condition(&self) -> bool {
    self.kind.is_critical_system_condition()
  }

  /// Returns the source occurrence time.
  #[must_use]
  pub const fn occurred_at(&self) -> Timestamp {
    self.occurred_at
  }

  /// Classifies this source into a safe item, excluding ordinary activity.
  #[must_use]
  pub fn classify(&self) -> Option<OperatorAttentionItem> {
    let (category, severity, code, target) = match &self.kind {
      OperatorAttentionEventKind::BuildFailed(identity) => (
        OperatorAttentionCategory::Build,
        OperatorAttentionSeverity::Critical,
        "build_failed",
        Some(OperatorAttentionTarget::Build(*identity)),
      ),
      OperatorAttentionEventKind::AgentUnavailable(identity) => (
        OperatorAttentionCategory::Agent,
        OperatorAttentionSeverity::Warning,
        "agent_unavailable",
        Some(OperatorAttentionTarget::Agent(*identity)),
      ),
      OperatorAttentionEventKind::AgentPoolUnavailable(identity) => (
        OperatorAttentionCategory::AgentPool,
        OperatorAttentionSeverity::Critical,
        "agent_pool_unavailable",
        Some(OperatorAttentionTarget::AgentPool(*identity)),
      ),
      OperatorAttentionEventKind::CriticalSystemCondition { code } => (
        OperatorAttentionCategory::CriticalSystem,
        OperatorAttentionSeverity::Critical,
        code.as_str(),
        None,
      ),
      OperatorAttentionEventKind::BuildSucceeded(_)
      | OperatorAttentionEventKind::AuditActivity
      | OperatorAttentionEventKind::JobActivity => return None,
    };
    Some(OperatorAttentionItem {
      id: self.id,
      category,
      severity,
      code: code.to_owned(),
      summary: self.summary.clone(),
      occurred_at: self.occurred_at,
      resolved_at: self.resolved_at,
      target,
    })
  }
}

/// Safe bounded attention item independent of transport representation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperatorAttentionItem {
  /// Stable attention identity.
  pub id: OperatorAttentionId,
  /// Typed server classification.
  pub category: OperatorAttentionCategory,
  /// Server-owned severity.
  pub severity: OperatorAttentionSeverity,
  /// Bounded machine-readable code.
  pub code: String,
  /// Bounded non-secret operator summary.
  pub summary: String,
  /// Time at which the attention-worthy transition occurred.
  pub occurred_at: Timestamp,
  /// Time at which the condition resolved, when applicable.
  pub resolved_at: Option<Timestamp>,
  /// Visible resource target sufficient to open its existing detail resource.
  pub target: Option<OperatorAttentionTarget>,
}

/// Newest-first stable pagination position.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperatorAttentionPagePosition {
  /// Occurrence time of the last item returned by the preceding page.
  pub occurred_at: Timestamp,
  /// Stable tie breaker of the last item returned by the preceding page.
  pub id: OperatorAttentionId,
}

/// Bounded target and critical-condition selection with optional time bounds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperatorAttentionScope {
  targets: BTreeSet<OperatorAttentionTarget>,
  include_critical_conditions: bool,
  occurred_from: Option<Timestamp>,
  occurred_through: Option<Timestamp>,
}

impl OperatorAttentionScope {
  /// Constructs a non-empty bounded attention selection.
  pub fn new(
    targets: BTreeSet<OperatorAttentionTarget>,
    include_critical_conditions: bool,
    occurred_from: Option<Timestamp>,
    occurred_through: Option<Timestamp>,
  ) -> Result<Self, OperatorAttentionValueError> {
    if targets.len() > MAX_OPERATOR_ATTENTION_TARGETS || (targets.is_empty() && !include_critical_conditions) {
      return Err(OperatorAttentionValueError::InvalidScope);
    }
    if occurred_from
      .zip(occurred_through)
      .is_some_and(|(from, through)| from > through)
    {
      return Err(OperatorAttentionValueError::InvalidTimeRange);
    }
    Ok(Self {
      targets,
      include_critical_conditions,
      occurred_from,
      occurred_through,
    })
  }

  /// Returns selected resource targets.
  #[must_use]
  pub const fn targets(&self) -> &BTreeSet<OperatorAttentionTarget> {
    &self.targets
  }

  /// Returns whether critical system conditions were explicitly selected.
  #[must_use]
  pub const fn includes_critical_conditions(&self) -> bool {
    self.include_critical_conditions
  }

  /// Returns the inclusive lower occurrence bound.
  #[must_use]
  pub const fn occurred_from(&self) -> Option<Timestamp> {
    self.occurred_from
  }

  /// Returns the inclusive upper occurrence bound.
  #[must_use]
  pub const fn occurred_through(&self) -> Option<Timestamp> {
    self.occurred_through
  }

  /// Derives a deterministic cursor binding without embedding the target set.
  #[must_use]
  pub fn digest(&self) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"octacity-operator-attention-scope-v1\0");
    for target in &self.targets {
      let (kind, identity) = match target {
        OperatorAttentionTarget::Build(identity) => (b"build".as_slice(), identity.as_uuid()),
        OperatorAttentionTarget::Agent(identity) => (b"agent".as_slice(), identity.as_uuid()),
        OperatorAttentionTarget::AgentPool(identity) => (b"agent_pool".as_slice(), identity.as_uuid()),
      };
      digest.update(kind);
      digest.update([0]);
      digest.update(identity.as_bytes());
    }
    digest.update([u8::from(self.include_critical_conditions)]);
    digest.update(
      self
        .occurred_from
        .map(Timestamp::unix_millis)
        .unwrap_or(i64::MIN)
        .to_be_bytes(),
    );
    digest.update(
      self
        .occurred_through
        .map(Timestamp::unix_millis)
        .unwrap_or(i64::MAX)
        .to_be_bytes(),
    );
    digest.finalize().into()
  }

  fn selects(&self, event: &OperatorAttentionEvent) -> bool {
    event.target().is_some_and(|target| self.targets.contains(&target))
      || (self.include_critical_conditions && event.is_critical_system_condition())
  }

  fn contains_time(&self, occurred_at: Timestamp) -> bool {
    self.occurred_from.is_none_or(|from| occurred_at >= from)
      && self.occurred_through.is_none_or(|through| occurred_at <= through)
  }
}

/// Authorization-derived visibility applied before classification and ordering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperatorAttentionVisibility {
  /// Every supported target and critical condition is visible.
  All,
  /// No attention source is visible.
  None,
  /// Only the listed targets and optional critical system conditions are visible.
  Restricted {
    /// Visible typed resource targets.
    targets: BTreeSet<OperatorAttentionTarget>,
    /// Whether control-plane critical conditions are visible.
    critical_conditions: bool,
  },
}

impl OperatorAttentionVisibility {
  /// Constructs unrestricted visibility for the trusted-network policy.
  #[must_use]
  pub const fn all() -> Self {
    Self::All
  }

  /// Constructs explicit empty visibility.
  #[must_use]
  pub const fn none() -> Self {
    Self::None
  }

  /// Constructs one bounded non-empty restricted scope.
  pub fn restricted(
    targets: impl IntoIterator<Item = OperatorAttentionTarget>,
    critical_conditions: bool,
  ) -> Result<Self, OperatorAttentionValueError> {
    let targets = targets.into_iter().collect::<BTreeSet<_>>();
    if targets.len() > crate::MAX_READ_VISIBILITY_IDENTITIES || (targets.is_empty() && !critical_conditions) {
      return Err(OperatorAttentionValueError::InvalidVisibility);
    }
    Ok(Self::Restricted {
      targets,
      critical_conditions,
    })
  }

  /// Returns whether a resource target may contribute attention items.
  #[must_use]
  pub fn allows_target(&self, target: &OperatorAttentionTarget) -> bool {
    match self {
      Self::All => true,
      Self::None => false,
      Self::Restricted { targets, .. } => targets.contains(target),
    }
  }

  /// Returns whether critical system conditions may contribute attention items.
  #[must_use]
  pub const fn allows_critical_conditions(&self) -> bool {
    match self {
      Self::All => true,
      Self::None => false,
      Self::Restricted {
        critical_conditions, ..
      } => *critical_conditions,
    }
  }
}

/// Validated store request for a newest-first attention page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListOperatorAttention {
  scope: OperatorAttentionScope,
  after: Option<OperatorAttentionPagePosition>,
  limit: NonZeroU16,
  visibility: OperatorAttentionVisibility,
}

impl ListOperatorAttention {
  /// Combines validated scope, page bounds, and authorization visibility.
  pub fn new(
    scope: OperatorAttentionScope,
    after: Option<OperatorAttentionPagePosition>,
    limit: u16,
    visibility: OperatorAttentionVisibility,
  ) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit)
      .filter(|limit| limit.get() <= MAX_OPERATOR_ATTENTION_PAGE_SIZE)
      .ok_or(StoreError::invalid(
        StoreOperation::ListOperatorAttention,
        StoreInputError::InvalidOperatorAttentionPageSize,
      ))?;
    Ok(Self {
      scope,
      after,
      limit,
      visibility,
    })
  }

  /// Returns the validated attention selection.
  #[must_use]
  pub const fn scope(&self) -> &OperatorAttentionScope {
    &self.scope
  }

  /// Returns the exclusive newest-first continuation position.
  #[must_use]
  pub const fn after(&self) -> Option<OperatorAttentionPagePosition> {
    self.after
  }

  /// Returns the positive bounded page size.
  #[must_use]
  pub const fn limit(&self) -> NonZeroU16 {
    self.limit
  }

  /// Returns authorization-derived visibility.
  #[must_use]
  pub const fn visibility(&self) -> &OperatorAttentionVisibility {
    &self.visibility
  }

  /// Applies visibility before scope selection or classification.
  #[must_use]
  pub fn source_is_visible(&self, event: &OperatorAttentionEvent) -> bool {
    event.target().map_or_else(
      || event.is_critical_system_condition() && self.visibility.allows_critical_conditions(),
      |target| self.visibility.allows_target(&target),
    )
  }

  /// Applies requested target, critical-condition, and time constraints.
  #[must_use]
  pub fn source_is_selected(&self, event: &OperatorAttentionEvent) -> bool {
    self.scope.selects(event) && self.scope.contains_time(event.occurred_at())
  }
}

/// One deterministic newest-first page of safe visible attention items.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperatorAttentionPage {
  /// Safe classified items in deterministic order.
  pub items: Vec<OperatorAttentionItem>,
  /// Exclusive position for the following page.
  pub next_cursor: Option<OperatorAttentionPagePosition>,
}

/// Invalid operator-attention model value.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OperatorAttentionValueError {
  /// A stable attention identity is nil, malformed, or non-canonical.
  #[error("operator attention identity is invalid")]
  InvalidIdentity,
  /// The target/critical selection is empty or exceeds its fixed bound.
  #[error("operator attention scope is invalid")]
  InvalidScope,
  /// The lower occurrence bound follows the upper bound.
  #[error("operator attention time range is invalid")]
  InvalidTimeRange,
  /// A critical-condition code is malformed or exceeds its fixed bound.
  #[error("operator attention code is invalid")]
  InvalidCode,
  /// A safe summary is empty, non-canonical, or exceeds its fixed bound.
  #[error("operator attention summary is invalid")]
  InvalidSummary,
  /// A resolution time precedes the source occurrence.
  #[error("operator attention resolution time is invalid")]
  InvalidResolutionTime,
  /// An authorization-derived restricted scope is empty or oversized.
  #[error("operator attention visibility is invalid")]
  InvalidVisibility,
}

fn validate_code(value: &str) -> Result<(), OperatorAttentionValueError> {
  if value.is_empty()
    || value.len() > MAX_OPERATOR_ATTENTION_CODE_BYTES
    || value
      .bytes()
      .any(|byte| !byte.is_ascii_lowercase() && !byte.is_ascii_digit() && byte != b'_')
  {
    return Err(OperatorAttentionValueError::InvalidCode);
  }
  Ok(())
}

fn validate_safe_text(value: &str, max_bytes: usize) -> Result<(), ()> {
  if value.is_empty() || value.len() > max_bytes || value.trim() != value || value.chars().any(char::is_control) {
    return Err(());
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn source_events_enforce_safe_projection_bounds_and_resolution_order() {
    let id = OperatorAttentionId::from_uuid(Uuid::from_u128(1)).unwrap();
    let occurred_at = Timestamp::from_unix_millis(10).unwrap();
    let resolved_at = Timestamp::from_unix_millis(9).unwrap();

    assert_eq!(
      OperatorAttentionEvent::new(
        id,
        OperatorAttentionEventKind::CriticalSystemCondition {
          code: "Not Canonical".to_owned()
        },
        "safe summary",
        occurred_at,
        None,
      ),
      Err(OperatorAttentionValueError::InvalidCode)
    );
    assert_eq!(
      OperatorAttentionEvent::new(
        id,
        OperatorAttentionEventKind::AuditActivity,
        "unsafe\nsummary",
        occurred_at,
        None,
      ),
      Err(OperatorAttentionValueError::InvalidSummary)
    );
    assert_eq!(
      OperatorAttentionEvent::new(
        id,
        OperatorAttentionEventKind::JobActivity,
        "safe summary",
        occurred_at,
        Some(resolved_at),
      ),
      Err(OperatorAttentionValueError::InvalidResolutionTime)
    );
  }

  #[test]
  fn store_request_revalidates_the_shared_page_bound() {
    let scope = OperatorAttentionScope::new(BTreeSet::new(), true, None, None).unwrap();
    for limit in [0, MAX_OPERATOR_ATTENTION_PAGE_SIZE + 1] {
      assert_eq!(
        ListOperatorAttention::new(scope.clone(), None, limit, OperatorAttentionVisibility::all()),
        Err(StoreError::InvalidInput {
          operation: StoreOperation::ListOperatorAttention,
          source: StoreInputError::InvalidOperatorAttentionPageSize,
        })
      );
    }
  }
}
