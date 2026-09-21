use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::MutationDisposition;

/// Summary of one Build Attempt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptSummaryResource {
  /// Stable Attempt identity.
  pub id: String,
  /// Owning Build identity.
  pub build_id: String,
  /// Positive number within the Build.
  pub number: u64,
  /// Prior failed Attempt, when this is a retry.
  pub retry_of_attempt_id: Option<String>,
  /// Current aggregate state.
  pub state: String,
  /// Optimistic state version.
  pub version: u64,
  /// Authoritative creation time as Unix milliseconds.
  pub created_at_unix_ms: i64,
  /// Latest accepted transition time as Unix milliseconds.
  pub updated_at_unix_ms: i64,
}

/// REST representation of one Build and its latest Attempt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildResource {
  /// Stable Build identity.
  pub id: String,
  /// Owning Project identity.
  pub project_id: String,
  /// Exact Build Configuration identity and version.
  pub configuration_id: String,
  /// Exact Build Configuration version.
  pub configuration_version: u64,
  /// Exact Pipeline identity and version.
  pub pipeline_id: String,
  /// Exact Pipeline version.
  pub pipeline_version: u64,
  /// Exact Repository identity and version.
  pub repository_id: String,
  /// Exact Repository version.
  pub repository_version: u64,
  /// Immutable provider-native source revision.
  pub immutable_revision: String,
  /// Safe primitive Build parameters.
  pub parameters: Value,
  /// Source expression retained for diagnosis.
  pub source: Value,
  /// Immutable effective Project-policy snapshot.
  pub effective_policy: Value,
  /// Durable ready-queue priority.
  pub priority: i64,
  /// Current aggregate state.
  pub state: String,
  /// Optimistic state version.
  pub version: u64,
  /// Safe normalized initiating Trigger facts.
  pub trigger: Value,
  /// Authoritative creation time as Unix milliseconds.
  pub created_at_unix_ms: i64,
  /// Latest accepted transition time as Unix milliseconds.
  pub updated_at_unix_ms: i64,
  /// Latest Attempt in this Build.
  pub current_attempt: AttemptSummaryResource,
}

/// One causal dependency edge in an Attempt DAG.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DagEdgeResource {
  /// Direct predecessor Job.
  pub predecessor_job_id: String,
  /// Dependent Job.
  pub dependent_job_id: String,
  /// Dependency and failure-propagation policy.
  pub dependency_policy: Value,
}

/// Non-secret queue inputs for a ready Job.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JobQueueResource {
  /// Explicit Build priority.
  pub priority: i64,
  /// Queue insertion time as Unix milliseconds.
  pub enqueued_at_unix_ms: i64,
}

/// Non-secret selected execution assignment.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JobAssignmentResource {
  /// Selected Pool identity.
  pub selected_pool_id: String,
  /// Assigned Agent identity.
  pub assigned_agent_id: String,
}

/// Safe terminal Job diagnostics.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JobTerminalResource {
  /// Terminal state.
  pub state: String,
  /// Stable failure classification, when terminal success is absent.
  pub failure_classification: Option<String>,
  /// Authoritative terminal time as Unix milliseconds.
  pub completed_at_unix_ms: i64,
}

/// REST representation of one materialized Job.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JobResource {
  /// Stable Job identity.
  pub id: String,
  /// Owning Attempt identity.
  pub attempt_id: String,
  /// Immutable Pipeline node identity.
  pub pipeline_node_id: String,
  /// Direct predecessor Job identities.
  pub dependency_job_ids: Vec<String>,
  /// Fan-in and failure-propagation policy.
  pub dependency_policy: Value,
  /// Pool allowlist captured for placement.
  pub allowed_pool_ids: Vec<String>,
  /// Backend-neutral placement inputs.
  pub placement: Value,
  /// Current scheduling or execution state.
  pub state: String,
  /// Optimistic state version.
  pub version: u64,
  /// Authoritative creation time as Unix milliseconds.
  pub created_at_unix_ms: i64,
  /// Latest accepted transition time as Unix milliseconds.
  pub updated_at_unix_ms: i64,
  /// Queue diagnostics when ready.
  pub queue: Option<JobQueueResource>,
  /// Selected execution assignment, retained after completion.
  pub assignment: Option<JobAssignmentResource>,
  /// Terminal outcome and failure classification.
  pub terminal: Option<JobTerminalResource>,
  /// Greatest contiguous durable event sequence.
  pub event_cursor: u64,
  /// Logical published output references. Physical storage details never appear here.
  pub outputs: Vec<Value>,
}

/// REST representation of one Attempt and its complete diagnostic DAG.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptResource {
  /// Attempt aggregate summary.
  pub attempt: AttemptSummaryResource,
  /// Materialized Jobs ordered by stable identity.
  pub jobs: Vec<JobResource>,
  /// Causal edges ordered by predecessor and dependent identity.
  pub edges: Vec<DagEdgeResource>,
}

/// Response from idempotent Build cancellation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancelBuildResponse {
  /// Whether the command was applied or replayed.
  pub disposition: MutationDisposition,
  /// Cancelled Build.
  pub build_id: String,
  /// Attempt current when cancellation was accepted.
  pub attempt_id: String,
  /// Jobs made terminal immediately.
  pub cancelled_job_ids: Vec<String>,
  /// Jobs whose owners receive cancellation directives.
  pub cancelling_job_ids: Vec<String>,
}

/// Response from idempotent Build retry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetryBuildResponse {
  /// Whether the command was applied or replayed.
  pub disposition: MutationDisposition,
  /// Retried Build.
  pub build_id: String,
  /// Prior failed Attempt retained as history.
  pub source_attempt_id: String,
  /// Newly materialized Attempt.
  pub attempt_id: String,
  /// Positive number allocated to the new Attempt.
  pub attempt_number: u64,
  /// Root Jobs made ready for placement.
  pub ready_job_ids: Vec<String>,
}
