use serde::{Deserialize, Serialize};

/// Request body for creating one static Agent Pool.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAgentPoolRequest {
  /// Stable operator-facing Pool name.
  pub name: String,
  /// Initial versioned Pool policy.
  pub definition: AgentPoolDefinition,
}

/// Request body for publishing the next Agent Pool version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishAgentPoolVersionRequest {
  /// Replacement versioned Pool policy.
  pub definition: AgentPoolDefinition,
}

/// Versioned policy of one static Agent Pool.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentPoolDefinition {
  /// Whether the Pool is administratively enabled.
  pub enabled: bool,
  /// Placement/drain lifecycle.
  pub drain_state: AgentPoolDrainState,
  /// Admission policy for future Agent enrollment.
  pub admission_policy: AgentPoolAdmissionPolicy,
  /// Maximum concurrent Jobs in this Pool.
  pub concurrency_limit: u32,
  /// Deterministic placement fairness policy.
  pub fairness_policy: AgentPoolFairnessPolicy,
  /// Maximum statically enrolled Agents in this Pool.
  pub static_capacity_limit: u32,
}

/// Placement ordering policy within one Pool.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentPoolFairnessPolicy {
  /// Global priority followed by durable FIFO order.
  PriorityFifo,
  /// Prefer configurations with fewer active Jobs at equal priority.
  ConfigurationFair,
}

/// Stable Pool drain lifecycle values.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentPoolDrainState {
  /// The Pool accepts compatible Jobs.
  Accepting,
  /// New placement is stopped while current Leases finish.
  GracefulDrain,
  /// New placement is stopped and current Leases must terminate.
  ForcedDrain,
  /// No current Lease remains and placement stays stopped.
  Drained,
}

/// Stable Agent enrollment admission policy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentPoolAdmissionPolicy {
  /// Any syntactically valid Agent host platform may enroll.
  Any,
  /// Only listed exact host platforms may enroll.
  Allowlist {
    /// Bounded unique accepted platform pairs.
    platforms: Vec<AgentPlatform>,
  },
}

/// Provider-neutral Agent host platform.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentPlatform {
  /// Canonical operating-system label.
  pub operating_system: String,
  /// Canonical architecture label.
  pub architecture: String,
}

/// REST representation of one exact Agent Pool version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentPoolResource {
  /// Stable opaque Pool identity.
  pub id: String,
  /// Stable operator-facing name.
  pub name: String,
  /// Exact positive Pool version.
  pub version: u64,
  /// Immutable policy published in this version.
  pub definition: AgentPoolDefinition,
  /// Authoritative publication time as Unix milliseconds.
  pub published_at_unix_ms: i64,
}

/// Result of deleting one Agent Pool.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteAgentPoolResponse {
  /// Whether this request applied or replayed the deletion.
  pub disposition: super::MutationDisposition,
  /// Stable identity of the deleted Pool.
  pub pool_id: String,
}
