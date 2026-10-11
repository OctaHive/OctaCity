//! Immutable initiation facts for ordinary Builds, independent of execution state.

use octacity_server_domain::Timestamp;
use octacity_server_factory::{FactoryDigest, FactoryError, FactoryRunId, FlowBuildIntent, NodeAttemptId};
use octacity_server_trigger::{NormalizedTriggerOccurrence, TriggerOccurrenceState};
use serde::{Deserialize, Serialize};

/// A real Trigger or an exact common Factory node operation that initiated a Build.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildOrigin {
  /// Normalized Trigger occurrence and its retained acceptance facts.
  Trigger(Box<TriggerBuildOrigin>),
  /// Exact retained common node operation, without a synthetic Trigger.
  Factory(FactoryBuildOrigin),
}

/// Compact authoritative initiation facts for Project Build discovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildSummaryOrigin {
  /// Provider-neutral initiating Trigger cause.
  Trigger(octacity_server_trigger::TriggerCause),
  /// Exact immutable common node operation.
  Factory(FactoryBuildOrigin),
}

/// Authoritative initiation facts for an ordinary Trigger Build.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TriggerBuildOrigin {
  /// Normalized occurrence that created the Build.
  pub occurrence: NormalizedTriggerOccurrence,
  /// Current durable occurrence state.
  pub state: TriggerOccurrenceState,
  /// Authoritative occurrence creation time.
  pub created_at: Timestamp,
  /// Last accepted occurrence transition time.
  pub updated_at: Timestamp,
}

/// Immutable operation ownership retained independently of ordinary Build retries.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryBuildOrigin {
  /// Canonical operation identity of the exact persisted Build intent.
  pub operation_id: FactoryDigest,
  /// Root owning the immutable Node Attempt.
  pub run_id: FactoryRunId,
  /// Exact common Node Attempt, irrespective of its logical name or primitive.
  pub node_attempt_id: NodeAttemptId,
  /// Full frozen input identity, including its Flow and cycle bindings.
  pub input_digest: FactoryDigest,
}

impl FactoryBuildOrigin {
  /// Derives ownership from the validated common intent; node names select no behavior.
  pub fn from_intent(intent: &FlowBuildIntent) -> Result<Self, FactoryError> {
    intent.input().verify_attempt(intent.node())?;
    Ok(Self {
      operation_id: intent.operation_id()?,
      run_id: intent.input().factory_run_id(),
      node_attempt_id: intent.node().id(),
      input_digest: intent.input().digest()?,
    })
  }
}
