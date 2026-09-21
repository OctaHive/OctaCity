use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::AgentPlatform;

/// Request body for issuing one replay-safe, single-use Agent enrollment token.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueAgentEnrollmentRequest {
  /// Pool selected for initial registration.
  pub pool_id: String,
  /// Exact immutable Pool policy version.
  pub pool_version: u64,
  /// Optional exact platform restriction; omission permits any platform allowed by the Pool.
  pub expected_platform: Option<AgentPlatform>,
}

/// One-time response containing the enrollment bearer consumed by an Agent.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueAgentEnrollmentResponse {
  /// Whether issuance applied or exactly replayed.
  pub disposition: super::MutationDisposition,
  /// Full enrollment bearer; clients must store it as a credential and never log it.
  pub credential: String,
  /// Bound Pool identity.
  pub pool_id: String,
  /// Bound Pool version.
  pub pool_version: u64,
  /// Exclusive credential expiry as Unix milliseconds.
  pub expires_at_unix_ms: i64,
}

impl fmt::Debug for IssueAgentEnrollmentResponse {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("IssueAgentEnrollmentResponse")
      .field("disposition", &self.disposition)
      .field("credential", &"[REDACTED]")
      .field("pool_id", &self.pool_id)
      .field("pool_version", &self.pool_version)
      .field("expires_at_unix_ms", &self.expires_at_unix_ms)
      .finish()
  }
}

/// Request body for moving one idle Agent to another Pool.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReassignAgentPoolRequest {
  /// Destination Pool identity.
  pub pool_id: String,
}

/// Request body for preventing an Agent from receiving more Jobs.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DrainAgentRequest {
  /// Handling of a current Lease.
  pub mode: AgentDrainMode,
}

/// Stable current-Lease behavior selected by an Agent drain request.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentDrainMode {
  /// Let current work finish and return `drain` from heartbeats.
  Graceful,
  /// Request cancellation of current work.
  Forced,
}

/// Stable normalized Agent lifecycle.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
  /// Agent is available.
  Online,
  /// Agent is unavailable.
  Offline,
  /// Agent is finishing current work.
  Draining,
}

/// Static host capacity reported by an Agent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCapacity {
  /// Logical processors.
  pub logical_cpu_count: u32,
  /// Physical memory in bytes.
  pub total_memory_bytes: u64,
  /// Work filesystem capacity in bytes.
  pub work_disk_total_bytes: u64,
  /// State filesystem capacity in bytes.
  pub state_disk_total_bytes: u64,
  /// Availability of validated hypervisor isolation.
  pub virtualization_available: bool,
}

/// REST representation of one enrolled Agent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentResource {
  /// Stable Agent identity.
  pub id: String,
  /// Operator-facing name.
  pub name: String,
  /// Optimistic mutable-state version.
  pub version: u64,
  /// Exactly one current Pool identity.
  pub pool_id: String,
  /// Exact current Pool policy version.
  pub pool_version: u64,
  /// Validated inventory without duplicated identity or capacity.
  pub inventory: Value,
  /// Static capacity projected separately from inventory.
  pub capacity: AgentCapacity,
  /// Normalized lifecycle.
  pub status: AgentStatus,
  /// Last authoritative contact as Unix milliseconds.
  pub last_seen_at_unix_ms: i64,
}
