use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use super::{ContractValueError, validate_url_safe_cursor};

/// Server-owned operator-attention category.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorAttentionCategory {
  /// Failed Build state.
  Build,
  /// Unavailable Agent state.
  Agent,
  /// Unavailable Agent Pool state.
  AgentPool,
  /// Critical control-plane condition.
  CriticalSystem,
}

/// Server-owned operator-attention severity.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorAttentionSeverity {
  /// Degraded resource requiring review.
  Warning,
  /// Failure requiring prompt attention.
  Critical,
}

/// Kind of resource targeted by an attention item.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorAttentionTargetKind {
  /// Immutable Build.
  Build,
  /// Registered Agent.
  Agent,
  /// Static Agent Pool.
  AgentPool,
}

/// Visible resource target sufficient for navigation to its detail resource.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorAttentionTargetResource {
  /// Target resource kind.
  pub kind: OperatorAttentionTargetKind,
  /// Stable target identity.
  pub id: String,
}

/// Safe server-classified operator-attention item.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorAttentionItemResource {
  /// Stable attention identity.
  pub id: String,
  /// Typed server-owned category.
  pub category: OperatorAttentionCategory,
  /// Server-owned severity.
  pub severity: OperatorAttentionSeverity,
  /// Bounded machine-readable classification code.
  pub code: String,
  /// Bounded non-secret operator summary.
  pub summary: String,
  /// Occurrence time in Unix milliseconds.
  pub occurred_at_unix_ms: i64,
  /// Resolution time in Unix milliseconds, when resolved.
  pub resolved_at_unix_ms: Option<i64>,
  /// Visible target resource, absent for critical system conditions.
  pub target: Option<OperatorAttentionTargetResource>,
}

/// Bounded opaque cursor owned by the operator-attention contract.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OperatorAttentionCursor(String);

impl OperatorAttentionCursor {
  /// Validates a non-empty URL-safe cursor within the attention-specific bound.
  pub fn new(value: impl Into<String>) -> Result<Self, ContractValueError> {
    validate_url_safe_cursor(
      value.into(),
      octacity_server_application::MAX_OPERATOR_ATTENTION_CURSOR_BYTES,
    )
    .map(Self)
  }

  /// Borrows the opaque encoded cursor.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

impl Serialize for OperatorAttentionCursor {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str(&self.0)
  }
}

impl<'de> Deserialize<'de> for OperatorAttentionCursor {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    String::deserialize(deserializer).and_then(|value| Self::new(value).map_err(D::Error::custom))
  }
}

/// One deterministic newest-first page of safe attention items.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorAttentionPage {
  /// Safe visible items in newest-first order.
  pub items: Vec<OperatorAttentionItemResource>,
  /// Exclusive cursor for the following page, or `None` at the end.
  pub next_cursor: Option<OperatorAttentionCursor>,
}
