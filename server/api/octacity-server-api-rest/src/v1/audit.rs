use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Cursor;

/// Stable actor classifications exposed by the v1 audit contract.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditActorKind {
  /// A request on the unauthenticated trusted-network listener.
  UnauthenticatedManagement,
  /// An authenticated Agent process.
  Agent,
  /// A normalized Trigger decision.
  Trigger,
  /// A server-owned orchestration decision.
  Orchestrator,
  /// A verified external adapter operation.
  Adapter,
  /// A durable background worker.
  Worker,
}

/// Actor information available when a mutation was accepted.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuditActor {
  /// Stable actor classification.
  pub kind: AuditActorKind,
  /// Authenticated or durable worker identity when one actually existed.
  pub identity: Option<String>,
}

/// Stable outcome of an immutable audit fact.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
  /// The authoritative mutation committed with this fact.
  Accepted,
}

/// One immutable structured audit fact.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuditFactResource {
  /// Stable fact identity.
  pub id: String,
  /// Honest available actor information.
  pub actor: AuditActor,
  /// Stable mutation operation name.
  pub operation: String,
  /// Stable target classification.
  pub target_kind: String,
  /// Logical target identity.
  pub target_identity: String,
  /// Request identity when the caller or worker supplied one.
  pub request_identity: Option<String>,
  /// Idempotency identity when the mutation supports replay.
  pub idempotency_key: Option<String>,
  /// Stable committed outcome.
  pub outcome: AuditOutcome,
  /// Bounded explicitly selected non-sensitive metadata.
  pub metadata: Value,
  /// Authoritative commit time in Unix milliseconds.
  pub occurred_at_unix_ms: i64,
}

/// Deterministic bounded page of immutable audit facts.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuditFactPage {
  /// Facts in stable newest-first order.
  pub items: Vec<AuditFactResource>,
  /// Opaque exclusive cursor for the following page.
  pub next_cursor: Option<Cursor>,
}
