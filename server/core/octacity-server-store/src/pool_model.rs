use std::{collections::BTreeSet, num::NonZeroU16};

use octacity_server_domain::{PoolId, PoolName, PoolVersion, Timestamp};
use octacity_server_scheduler::PoolDrainState;
use serde::{Deserialize, Serialize};

use crate::{AgentPlatform, IdempotencyKey, MutationDisposition, StoreError, StoreInputError, StoreOperation};

/// Maximum number of Agent Pools returned by one management query.
pub const MAX_AGENT_POOL_PAGE_SIZE: u16 = 200;
/// Maximum number of exact platforms in one Pool admission allowlist.
pub const MAX_POOL_ADMISSION_PLATFORMS: usize = 32;
/// Maximum configured static Agents in one Pool.
pub const MAX_POOL_STATIC_CAPACITY: u32 = 10_000;

/// Deterministic ordering policy applied within one accepting Pool.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PoolFairnessPolicy {
  /// Preserve global priority and FIFO order.
  PriorityFifo,
  /// Prefer Build Configurations with fewer currently active Jobs at equal priority.
  ConfigurationFair,
}

/// Platform admission policy applied when an Agent enrolls into a Pool.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum PoolAdmissionPolicy {
  /// Any syntactically valid Agent platform may enroll.
  Any,
  /// Only the listed exact host platforms may enroll.
  Allowlist {
    /// Non-empty bounded set of accepted host platforms.
    platforms: BTreeSet<AgentPlatform>,
  },
}

impl PoolAdmissionPolicy {
  fn validate(&self) -> Result<(), StoreInputError> {
    match self {
      Self::Any => Ok(()),
      Self::Allowlist { platforms } if platforms.is_empty() => Err(StoreInputError::InvalidPoolAdmissionPolicy),
      Self::Allowlist { platforms } if platforms.len() > MAX_POOL_ADMISSION_PLATFORMS => {
        Err(StoreInputError::InvalidPoolAdmissionPolicy)
      }
      Self::Allowlist { .. } => Ok(()),
    }
  }
}

/// Immutable configuration published as one Agent Pool version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentPoolDefinition {
  /// Whether the Pool is administratively enabled.
  pub enabled: bool,
  /// Whether new placement is accepted or draining.
  pub drain_state: PoolDrainState,
  /// Admission rules for future Agent enrollments.
  pub admission_policy: PoolAdmissionPolicy,
  /// Maximum number of concurrent Jobs across this Pool.
  pub concurrency_limit: u32,
  /// Deterministic fairness policy used after durable priority.
  pub fairness_policy: PoolFairnessPolicy,
  /// Maximum number of statically enrolled Agents in this Pool.
  pub static_capacity_limit: u32,
}

impl AgentPoolDefinition {
  /// Revalidates bounded policy and capacity invariants.
  pub fn validate(&self) -> Result<(), StoreInputError> {
    self.admission_policy.validate()?;
    if self.concurrency_limit == 0
      || self.static_capacity_limit == 0
      || self.static_capacity_limit > MAX_POOL_STATIC_CAPACITY
      || self.concurrency_limit > self.static_capacity_limit
    {
      return Err(StoreInputError::InvalidPoolCapacity);
    }
    Ok(())
  }
}

/// Verifies one version-to-version Pool drain transition.
pub fn validate_pool_drain_transition(
  current: PoolDrainState,
  requested: PoolDrainState,
  active_leases: bool,
) -> Result<(), StoreError> {
  current
    .transition_to(requested, active_leases)
    .map(|_| ())
    .map_err(|_| StoreError::Conflict {
      entity: octacity_server_domain::EntityKind::Pool,
    })
}

/// One immutable published Agent Pool version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublishedAgentPool {
  /// Stable Pool identity.
  pub id: PoolId,
  /// Stable operator-facing name.
  pub name: PoolName,
  /// Positive immutable version.
  pub version: PoolVersion,
  /// Published Pool configuration.
  pub definition: AgentPoolDefinition,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

/// Complete atomic request to create an Agent Pool and its first version.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CreateAgentPool {
  /// Stable identity selected once by the application.
  pub id: PoolId,
  /// Stable operator-facing name.
  pub name: PoolName,
  /// Initial Pool definition.
  pub definition: AgentPoolDefinition,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

/// Complete atomic request to append the next Agent Pool version.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PublishAgentPoolVersion {
  /// Stable Pool identity.
  pub id: PoolId,
  /// Version that must still be current.
  pub expected_current_version: PoolVersion,
  /// Replacement Pool definition.
  pub definition: AgentPoolDefinition,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

/// Complete atomic request to delete an unreferenced Agent Pool.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DeleteAgentPool {
  /// Stable Pool identity.
  pub id: PoolId,
  /// Version that must still be current.
  pub expected_current_version: PoolVersion,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative deletion time.
  pub deleted_at: Timestamp,
}

/// Result of creating or publishing an Agent Pool version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentPoolMutationOutcome {
  /// Whether the command applied or replayed a prior result.
  pub disposition: MutationDisposition,
  /// Immutable state committed by the original command.
  pub pool: PublishedAgentPool,
}

/// Result of deleting one Agent Pool.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeleteAgentPoolOutcome {
  /// Whether the command applied or replayed a prior result.
  pub disposition: MutationDisposition,
  /// Stable identity of the deleted Pool.
  pub pool_id: PoolId,
}

/// Bounded deterministic query over current Agent Pool versions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListAgentPools {
  after: Option<PoolId>,
  limit: NonZeroU16,
}

impl ListAgentPools {
  /// Validates a stable-identity cursor and page size.
  pub fn new(after: Option<PoolId>, limit: u16) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit)
      .filter(|value| value.get() <= MAX_AGENT_POOL_PAGE_SIZE)
      .ok_or_else(|| {
        StoreError::invalid(
          StoreOperation::ListAgentPools,
          StoreInputError::InvalidAgentPoolPageSize,
        )
      })?;
    Ok(Self { after, limit })
  }

  /// Exclusive stable-identity cursor.
  #[must_use]
  pub const fn after(self) -> Option<PoolId> {
    self.after
  }

  /// Positive bounded page size.
  #[must_use]
  pub const fn limit(self) -> NonZeroU16 {
    self.limit
  }
}

/// One deterministic page of current Agent Pool versions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentPoolPage {
  /// Pools ordered by stable identity.
  pub pools: Vec<PublishedAgentPool>,
  /// Cursor for the next page, when more Pools exist.
  pub next_cursor: Option<PoolId>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn pool_policy_rejects_empty_admission_and_invalid_capacity_bounds() {
    let mut definition = valid_definition();
    definition.admission_policy = PoolAdmissionPolicy::Allowlist {
      platforms: BTreeSet::new(),
    };
    assert_eq!(definition.validate(), Err(StoreInputError::InvalidPoolAdmissionPolicy));

    for (concurrency_limit, static_capacity_limit) in [(0, 1), (1, 0), (2, 1), (1, MAX_POOL_STATIC_CAPACITY + 1)] {
      let mut definition = valid_definition();
      definition.concurrency_limit = concurrency_limit;
      definition.static_capacity_limit = static_capacity_limit;
      assert_eq!(definition.validate(), Err(StoreInputError::InvalidPoolCapacity));
    }
  }

  #[test]
  fn pool_policy_accepts_its_documented_capacity_boundary() {
    let definition = AgentPoolDefinition {
      concurrency_limit: MAX_POOL_STATIC_CAPACITY,
      static_capacity_limit: MAX_POOL_STATIC_CAPACITY,
      ..valid_definition()
    };
    assert_eq!(definition.validate(), Ok(()));
  }

  #[test]
  fn drain_transitions_require_the_state_machine_and_cleared_leases() {
    assert_eq!(
      validate_pool_drain_transition(PoolDrainState::GracefulDrain, PoolDrainState::Drained, true),
      Err(conflict())
    );
    assert_eq!(
      validate_pool_drain_transition(PoolDrainState::Accepting, PoolDrainState::Drained, false),
      Err(conflict())
    );
    assert_eq!(
      validate_pool_drain_transition(PoolDrainState::GracefulDrain, PoolDrainState::Drained, false),
      Ok(())
    );
    assert_eq!(
      validate_pool_drain_transition(PoolDrainState::Drained, PoolDrainState::Accepting, false),
      Ok(())
    );
  }

  fn valid_definition() -> AgentPoolDefinition {
    AgentPoolDefinition {
      enabled: true,
      drain_state: PoolDrainState::Accepting,
      admission_policy: PoolAdmissionPolicy::Any,
      concurrency_limit: 1,
      fairness_policy: PoolFairnessPolicy::PriorityFifo,
      static_capacity_limit: 1,
    }
  }

  fn conflict() -> StoreError {
    StoreError::Conflict {
      entity: octacity_server_domain::EntityKind::Pool,
    }
  }
}
