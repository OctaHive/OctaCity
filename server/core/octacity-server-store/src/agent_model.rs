use std::num::NonZeroU16;

use octacity_protocol::AgentInventory;
use octacity_server_domain::{
  AgentId, AgentName, AgentVersion, AttemptId, BuildId, JobId, LeaseId, PoolId, PoolVersion, Timestamp,
};
use serde::{Deserialize, Serialize};

use crate::{AgentListVisibility, IdempotencyKey, MutationDisposition, StoreError, StoreInputError, StoreOperation};

/// Maximum number of Agents returned by one management query.
pub const MAX_AGENT_PAGE_SIZE: u16 = 200;

/// Normalized lifecycle reported by the management API.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
  /// The Agent most recently registered successfully.
  Online,
  /// The Agent is not currently available for placement.
  Offline,
  /// The Agent is finishing current work but cannot receive new work.
  Draining,
}

/// Operator-selected behavior for an Agent's current Lease while draining.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentDrainMode {
  /// Let current work finish while preventing another assignment.
  Graceful,
  /// Request cancellation of current work and prevent another assignment.
  Forced,
}

/// One enrolled Agent reconstructed from authoritative state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EnrolledAgent {
  /// Stable Agent identity.
  pub id: AgentId,
  /// Stable operator-facing name.
  pub name: AgentName,
  /// Optimistic mutable-state version.
  pub version: AgentVersion,
  /// The Agent's one and only current Pool.
  pub pool_id: PoolId,
  /// Exact Pool policy version governing the assignment.
  pub pool_version: PoolVersion,
  /// Validated scheduler-visible inventory.
  pub inventory: AgentInventory,
  /// Normalized lifecycle state.
  pub status: AgentStatus,
  /// Last authoritative Agent contact time.
  pub last_seen_at: Timestamp,
  /// Initial enrollment time.
  pub created_at: Timestamp,
  /// Last management mutation or registration time.
  pub updated_at: Timestamp,
}

/// Safe facts about the non-terminal execution currently owned by an Agent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentCurrentExecution {
  /// Active lease identity without its fencing material.
  pub lease_id: LeaseId,
  /// Build containing the current Job.
  pub build_id: BuildId,
  /// Attempt containing the current Job.
  pub attempt_id: AttemptId,
  /// Current Job identity.
  pub job_id: JobId,
  /// Authoritative non-terminal lease state.
  pub lease_state: AgentCurrentLeaseState,
}

/// Non-terminal lease states visible through Agent capacity reads.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentCurrentLeaseState {
  /// The Agent may continue executing and renewing the lease.
  Active,
  /// The coordinator has requested cancellation.
  CancellationRequested,
  /// The coordinator has requested draining after execution stops.
  DrainRequested,
}

/// One enrolled Agent together with its optional current execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentDetail {
  /// Authoritative enrolled Agent state.
  pub agent: EnrolledAgent,
  /// Current non-terminal execution, or `None` when the Agent is idle.
  pub current_execution: Option<AgentCurrentExecution>,
}

/// Bounded deterministic query over enrolled Agents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListAgents {
  after: Option<AgentId>,
  limit: NonZeroU16,
  visibility: AgentListVisibility,
  pool_id: Option<PoolId>,
}

impl ListAgents {
  /// Validates a stable-identity cursor and page size.
  pub fn new(after: Option<AgentId>, limit: u16, visibility: AgentListVisibility) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit)
      .filter(|value| value.get() <= MAX_AGENT_PAGE_SIZE)
      .ok_or_else(|| StoreError::invalid(StoreOperation::ListAgents, StoreInputError::InvalidAgentPageSize))?;
    Ok(Self {
      after,
      limit,
      visibility,
      pool_id: None,
    })
  }

  /// Restricts the page to Agents assigned to one exact Pool.
  #[must_use]
  pub const fn for_pool(mut self, pool_id: Option<PoolId>) -> Self {
    self.pool_id = pool_id;
    self
  }

  /// Exclusive stable-identity cursor.
  #[must_use]
  pub const fn after(&self) -> Option<AgentId> {
    self.after
  }

  /// Positive bounded page size.
  #[must_use]
  pub const fn limit(&self) -> NonZeroU16 {
    self.limit
  }

  /// Authorization-derived Agent identities applied before cursor and limit.
  #[must_use]
  pub const fn visibility(&self) -> &AgentListVisibility {
    &self.visibility
  }

  /// Optional exact current Pool filter applied before cursor pagination.
  #[must_use]
  pub const fn pool_id(&self) -> Option<PoolId> {
    self.pool_id
  }
}

/// One deterministic page of enrolled Agents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentPage {
  /// Agents ordered by stable identity.
  pub agents: Vec<EnrolledAgent>,
  /// Cursor for the next page, when more Agents exist.
  pub next_cursor: Option<AgentId>,
}

/// Atomic request to move an idle Agent to the latest version of another Pool.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReassignAgentPool {
  /// Stable Agent identity.
  pub agent_id: AgentId,
  /// Agent version that must still be current.
  pub expected_version: AgentVersion,
  /// Destination Pool identity; its current version is resolved atomically.
  pub target_pool_id: PoolId,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative mutation time.
  pub reassigned_at: Timestamp,
}

/// Result of an Agent Pool reassignment.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReassignAgentPoolOutcome {
  /// Whether this command applied or replayed a prior result.
  pub disposition: MutationDisposition,
  /// Complete normalized Agent state committed by the original command.
  pub agent: EnrolledAgent,
}

/// Atomic request to place one Agent into drain state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DrainAgent {
  /// Stable Agent identity.
  pub agent_id: AgentId,
  /// Agent version that must still be current.
  pub expected_version: AgentVersion,
  /// Graceful or forced handling of a current Lease.
  pub mode: AgentDrainMode,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative mutation time.
  pub requested_at: Timestamp,
}

/// Result of one Agent drain command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DrainAgentOutcome {
  /// Whether this command applied or replayed a prior result.
  pub disposition: MutationDisposition,
  /// Complete normalized Agent state committed by the original command.
  pub agent: EnrolledAgent,
  /// Safe execution state committed with the original command.
  #[serde(default)]
  pub current_execution: Option<AgentCurrentExecution>,
}
