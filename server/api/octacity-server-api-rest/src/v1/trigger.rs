use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::MutationDisposition;

/// Request body for publishing an immutable Project policy version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishProjectPolicyRequest {
  /// Complete strict policy-directive document.
  pub policy: Value,
}

/// REST result of a Project policy publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPolicyResource {
  /// Owning Project identity.
  pub project_id: String,
  /// Exact immutable policy version.
  pub version: u64,
}

/// Request body for creating a manual Trigger definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateManualTriggerDefinitionRequest {
  /// Target Build Configuration identity.
  pub configuration_id: String,
  /// Exact immutable Build Configuration version.
  pub configuration_version: u64,
  /// Whether occurrences may be accepted.
  pub enabled: bool,
  /// Manual Trigger-specific bounded definition document.
  pub definition: Value,
}

/// REST result of creating a Trigger definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerDefinitionResource {
  /// Stable Trigger identity.
  pub id: String,
  /// Exact immutable Trigger version.
  pub version: u64,
}

/// Source expression supplied by a manual Build request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case", deny_unknown_fields)]
pub enum ManualSource {
  /// Resolve the Repository version's configured default reference.
  DefaultReference,
  /// Resolve one explicitly allowed mutable branch or tag.
  Reference(String),
  /// Verify and use an explicitly supplied immutable revision.
  ExactRevision(String),
}

/// Request body for accepting one manual Trigger occurrence.
///
/// Transport observation and acceptance times are server-owned. Replay
/// identity is carried by `Idempotency-Key`, while the deduplication identity
/// remains part of the normalized Trigger intent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptManualTriggerRequest {
  /// Exact immutable Trigger definition identity.
  pub trigger_id: String,
  /// Exact immutable Trigger definition version.
  pub trigger_version: u64,
  /// Exact Build Configuration target identity.
  pub configuration_id: String,
  /// Exact immutable Build Configuration version.
  pub configuration_version: u64,
  /// Stable source-scoped occurrence identity.
  pub deduplication_identity: String,
  /// Allowed source expression to resolve once.
  pub source: ManualSource,
  /// Primitive parameter values supplied before defaults are applied.
  pub parameters: BTreeMap<String, Value>,
  /// Durable ready-queue priority copied to root Jobs.
  pub priority: i64,
}

/// REST result of one accepted or intentionally suppressed Trigger evaluation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub enum TriggerEvaluationResponse {
  /// Evaluation created one Build and materialized its first Attempt.
  Accepted {
    /// Whether this request applied or replayed the evaluation.
    disposition: MutationDisposition,
    /// Stable normalized Trigger occurrence identity.
    trigger_occurrence_id: String,
    /// Build associated with the occurrence.
    build_id: String,
    /// First Attempt associated with the Build.
    attempt_id: String,
    /// Root Jobs inserted into the global ready queue.
    ready_job_ids: Vec<String>,
  },
  /// Policy intentionally created no Build or queued work.
  Suppressed {
    /// Whether this request applied or replayed the evaluation.
    disposition: MutationDisposition,
    /// Stable normalized Trigger occurrence identity.
    trigger_occurrence_id: String,
  },
}
