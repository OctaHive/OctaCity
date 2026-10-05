use std::{
  collections::{BTreeMap, BTreeSet},
  sync::{Mutex, MutexGuard},
};

use async_trait::async_trait;
use octacity_server_domain::{AgentId, EntityKind, PoolId, PoolVersion};

use crate::testing::{ManagementAuditProbe, RecordedManagementAuditFact, recorded_management_audit};
use crate::{
  AgentCurrentExecution, AgentCurrentLeaseState, AgentDetail, AgentDrainMode, AgentPage, AgentPlatform,
  AgentPoolDefinition, AgentStore, DrainAgent, DrainAgentOutcome, EnrolledAgent, ListAgents, ManagementIdempotencyKey,
  ManagementMutation, MutationDisposition, PoolAdmissionPolicy, ReassignAgentPool, ReassignAgentPoolOutcome,
  StoreError, StoreOperation,
};

/// Deterministic process-local Agent management adapter.
#[derive(Default)]
pub struct InMemoryAgentStore {
  state: Mutex<State>,
}

#[derive(Clone, Default)]
struct State {
  agents: BTreeMap<AgentId, (EnrolledAgent, AgentPlatform)>,
  pools: BTreeMap<(PoolId, PoolVersion), AgentPoolDefinition>,
  current_pools: BTreeMap<PoolId, PoolVersion>,
  current_executions: BTreeMap<AgentId, AgentCurrentExecution>,
  mutations: BTreeMap<ManagementIdempotencyKey, (Fingerprint, ReassignAgentPoolOutcome)>,
  drain_mutations: BTreeMap<ManagementIdempotencyKey, (DrainFingerprint, DrainAgentOutcome)>,
  audit: BTreeSet<RecordedManagementAuditFact>,
}

#[async_trait]
impl ManagementAuditProbe for InMemoryAgentStore {
  async fn management_audit_facts(&self) -> Vec<RecordedManagementAuditFact> {
    self
      .lock()
      .expect("in-memory Agent store must remain available")
      .audit
      .iter()
      .cloned()
      .collect()
  }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Fingerprint {
  agent_id: AgentId,
  expected_version: octacity_server_domain::AgentVersion,
  target_pool_id: PoolId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DrainFingerprint {
  agent_id: AgentId,
  expected_version: octacity_server_domain::AgentVersion,
  mode: AgentDrainMode,
}

impl InMemoryAgentStore {
  /// Creates an empty store.
  #[must_use]
  pub fn new() -> Self {
    Self::default()
  }

  /// Seeds one immutable Pool version and makes it current.
  pub fn seed_pool(&self, id: PoolId, version: PoolVersion, definition: AgentPoolDefinition) -> Result<(), StoreError> {
    definition.validate().map_err(|_| StoreError::Unavailable)?;
    let mut state = self.lock()?;
    state.pools.insert((id, version), definition);
    state.current_pools.insert(id, version);
    Ok(())
  }

  /// Seeds one enrolled Agent.
  pub fn seed_agent(&self, agent: EnrolledAgent, platform: AgentPlatform) -> Result<(), StoreError> {
    let mut state = self.lock()?;
    if !state.pools.contains_key(&(agent.pool_id, agent.pool_version)) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Pool,
      });
    }
    state.agents.insert(agent.id, (agent, platform));
    Ok(())
  }

  /// Seeds or clears the safe projection of an Agent's current execution.
  pub fn set_current_execution(
    &self,
    agent_id: AgentId,
    execution: Option<AgentCurrentExecution>,
  ) -> Result<(), StoreError> {
    let mut state = self.lock()?;
    if !state.agents.contains_key(&agent_id) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Agent,
      });
    }
    if let Some(execution) = execution {
      state.current_executions.insert(agent_id, execution);
    } else {
      state.current_executions.remove(&agent_id);
    }
    Ok(())
  }

  fn lock(&self) -> Result<MutexGuard<'_, State>, StoreError> {
    self.state.lock().map_err(|_| StoreError::Unavailable)
  }
}

#[async_trait]
impl AgentStore for InMemoryAgentStore {
  async fn agent_detail(&self, agent_id: AgentId) -> Result<AgentDetail, StoreError> {
    let state = self.lock()?;
    let agent = state
      .agents
      .get(&agent_id)
      .map(|record| record.0.clone())
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Agent,
      })?;
    Ok(AgentDetail {
      agent,
      current_execution: state.current_executions.get(&agent_id).cloned(),
    })
  }

  async fn list_agents(&self, request: ListAgents) -> Result<AgentPage, StoreError> {
    if request.visibility().kind() == crate::ReadVisibilityKind::None {
      return Ok(AgentPage {
        agents: Vec::new(),
        next_cursor: None,
      });
    }
    let state = self.lock()?;
    let limit = usize::from(request.limit().get());
    let mut agents = state
      .agents
      .iter()
      .filter(|(id, _)| request.visibility().allows(id))
      .filter(|(_, record)| request.pool_id().is_none_or(|pool_id| record.0.pool_id == pool_id))
      .filter(|(id, _)| request.after().is_none_or(|after| **id > after))
      .map(|(_, record)| record.0.clone())
      .take(limit + 1)
      .collect::<Vec<_>>();
    let has_more = agents.len() > limit;
    agents.truncate(limit);
    let next_cursor = has_more.then(|| agents.last().expect("non-zero full page has a last Agent").id);
    Ok(AgentPage { agents, next_cursor })
  }

  async fn reassign_agent_pool(
    &self,
    request: ManagementMutation<ReassignAgentPool>,
  ) -> Result<ReassignAgentPoolOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let idempotency = audit.scoped_idempotency_key(&request.idempotency_key);
    let fingerprint = Fingerprint {
      agent_id: request.agent_id,
      expected_version: request.expected_version,
      target_pool_id: request.target_pool_id,
    };
    let mut state = self.lock()?;
    if let Some((stored_fingerprint, stored_outcome)) = state.mutations.get(&idempotency) {
      if stored_fingerprint != &fingerprint {
        return Err(StoreError::Conflict {
          entity: EntityKind::Agent,
        });
      }
      let mut outcome = stored_outcome.clone();
      outcome.disposition = MutationDisposition::Replayed;
      return Ok(outcome);
    }
    let target_version = state
      .current_pools
      .get(&request.target_pool_id)
      .copied()
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Pool,
      })?;
    let definition = state
      .pools
      .get(&(request.target_pool_id, target_version))
      .cloned()
      .ok_or(StoreError::Unavailable)?;
    let (current, platform) = state
      .agents
      .get(&request.agent_id)
      .cloned()
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Agent,
      })?;
    if current.version != request.expected_version || current.pool_id == request.target_pool_id {
      return Err(StoreError::Conflict {
        entity: EntityKind::Agent,
      });
    }
    if state.current_executions.contains_key(&request.agent_id) {
      return Err(StoreError::Conflict {
        entity: EntityKind::Lease,
      });
    }
    let target_count = state
      .agents
      .values()
      .filter(|record| record.0.pool_id == request.target_pool_id)
      .count();
    let compatible = match &definition.admission_policy {
      PoolAdmissionPolicy::Any => true,
      PoolAdmissionPolicy::Allowlist { platforms } | PoolAdmissionPolicy::ExecutionAllowlist { platforms, .. } => {
        platforms.contains(&platform)
      }
    };
    if !definition.enabled
      || definition.drain_state != octacity_server_scheduler::PoolDrainState::Accepting
      || !compatible
      || target_count >= definition.static_capacity_limit as usize
    {
      return Err(StoreError::Conflict {
        entity: EntityKind::Pool,
      });
    }
    let mut agent = current;
    agent.pool_id = request.target_pool_id;
    agent.pool_version = target_version;
    agent.version = agent.version.next().map_err(|_| StoreError::Unavailable)?;
    agent.status = crate::AgentStatus::Offline;
    agent.updated_at = request.reassigned_at;
    state.agents.insert(agent.id, (agent.clone(), platform));
    let outcome = ReassignAgentPoolOutcome {
      disposition: MutationDisposition::Applied,
      agent,
    };
    state.mutations.insert(idempotency, (fingerprint, outcome.clone()));
    state.audit.insert(recorded_management_audit(
      &audit,
      StoreOperation::ReassignAgentPool,
      EntityKind::Agent,
      request.agent_id,
    ));
    Ok(outcome)
  }

  async fn drain_agent(&self, request: ManagementMutation<DrainAgent>) -> Result<DrainAgentOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let idempotency = audit.scoped_idempotency_key(&request.idempotency_key);
    let fingerprint = DrainFingerprint {
      agent_id: request.agent_id,
      expected_version: request.expected_version,
      mode: request.mode,
    };
    let mut state = self.lock()?;
    if let Some((stored_fingerprint, stored_outcome)) = state.drain_mutations.get(&idempotency) {
      if stored_fingerprint != &fingerprint {
        return Err(StoreError::Conflict {
          entity: EntityKind::Agent,
        });
      }
      let mut outcome = stored_outcome.clone();
      outcome.disposition = MutationDisposition::Replayed;
      return Ok(outcome);
    }
    let (mut agent, platform) = state
      .agents
      .get(&request.agent_id)
      .cloned()
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Agent,
      })?;
    if agent.version != request.expected_version || agent.status == crate::AgentStatus::Draining {
      return Err(StoreError::Conflict {
        entity: EntityKind::Agent,
      });
    }
    agent.version = agent.version.next().map_err(|_| StoreError::Unavailable)?;
    agent.status = crate::AgentStatus::Draining;
    agent.updated_at = request.requested_at;
    state.agents.insert(agent.id, (agent.clone(), platform));
    let current_execution = state.current_executions.get_mut(&request.agent_id).map(|execution| {
      execution.lease_state = match request.mode {
        AgentDrainMode::Graceful => AgentCurrentLeaseState::DrainRequested,
        AgentDrainMode::Forced => AgentCurrentLeaseState::CancellationRequested,
      };
      execution.clone()
    });
    let outcome = DrainAgentOutcome {
      disposition: MutationDisposition::Applied,
      agent,
      current_execution,
    };
    state
      .drain_mutations
      .insert(idempotency, (fingerprint, outcome.clone()));
    state.audit.insert(recorded_management_audit(
      &audit,
      StoreOperation::DrainAgent,
      EntityKind::Agent,
      request.agent_id,
    ));
    Ok(outcome)
  }
}

#[cfg(test)]
mod tests {
  use std::{collections::BTreeMap, sync::Arc};

  use octacity_protocol::{
    AgentInventory, COORDINATOR_PROTOCOL_VERSION, HostCapacity, OctaInventory, PlatformArchitecture, PlatformOs,
    PlatformSpec,
  };
  use octacity_server_domain::{AgentName, AgentVersion, AttemptId, BuildId, JobId, LeaseId, Timestamp};
  use octacity_server_scheduler::PoolDrainState;

  use super::*;
  use crate::test_support::{id, run_ready, time};
  use crate::testing::{assert_management_audit_facts, expected_management_audit};

  #[test]
  fn reassignment_is_atomic_paginated_and_rejected_during_an_active_lease() {
    let store = Arc::new(InMemoryAgentStore::new());
    let source = id::<PoolId>(1);
    let target = id::<PoolId>(2);
    for pool_id in [source, target] {
      store
        .seed_pool(pool_id, PoolVersion::INITIAL, pool_definition())
        .unwrap();
    }
    let agent = enrolled_agent(source);
    let agent_id = agent.id;
    store
      .seed_agent(agent, AgentPlatform::new("linux", "amd64").unwrap())
      .unwrap();
    let mut second_agent = enrolled_agent(source);
    second_agent.id = id(11);
    second_agent.name = AgentName::new("builder-2").unwrap();
    store
      .seed_agent(second_agent.clone(), AgentPlatform::new("linux", "amd64").unwrap())
      .unwrap();

    run_ready(
      async move {
        let listed = store
          .list_agents(ListAgents::new(None, 1, crate::AgentListVisibility::all()).unwrap())
          .await
          .unwrap();
        assert_eq!(listed.agents.len(), 1);
        assert_eq!(listed.next_cursor, Some(agent_id));
        let restricted = store
          .list_agents(
            ListAgents::new(
              None,
              1,
              crate::AgentListVisibility::restricted([second_agent.id]).unwrap(),
            )
            .unwrap(),
          )
          .await
          .unwrap();
        assert_eq!(restricted.agents, vec![second_agent]);
        assert_eq!(restricted.next_cursor, None);
        let pool_filtered = store
          .list_agents(
            ListAgents::new(None, 10, crate::AgentListVisibility::all())
              .unwrap()
              .for_pool(Some(target)),
          )
          .await
          .unwrap();
        assert!(pool_filtered.agents.is_empty());
        assert_eq!(pool_filtered.next_cursor, None);
        let none = store
          .list_agents(ListAgents::new(None, 1, crate::AgentListVisibility::none()).unwrap())
          .await
          .unwrap();
        assert!(none.agents.is_empty());
        assert_eq!(none.next_cursor, None);
        assert_eq!(store.agent_detail(agent_id).await.unwrap().agent.pool_id, source);

        let active_execution = current_execution();
        store
          .set_current_execution(agent_id, Some(active_execution.clone()))
          .unwrap();
        assert_eq!(
          store.agent_detail(agent_id).await.unwrap().current_execution,
          Some(active_execution.clone())
        );
        assert_eq!(
          store
            .reassign_agent_pool(crate::testing::management_mutation(reassign(
              agent_id,
              target,
              "move-agent"
            )))
            .await
            .unwrap_err(),
          StoreError::Conflict {
            entity: EntityKind::Lease
          },
        );
        assert_eq!(store.agent_detail(agent_id).await.unwrap().agent.pool_id, source);

        store.set_current_execution(agent_id, None).unwrap();
        let moved = store
          .reassign_agent_pool(crate::testing::management_mutation(reassign(
            agent_id,
            target,
            "move-agent",
          )))
          .await
          .unwrap();
        assert_eq!(moved.disposition, MutationDisposition::Applied);
        assert_eq!(moved.agent.pool_id, target);
        assert_eq!(moved.agent.status, crate::AgentStatus::Offline);
        assert_eq!(
          moved.agent.last_seen_at,
          time(10),
          "management mutation must not forge contact time"
        );
        let replay = store
          .reassign_agent_pool(crate::testing::management_mutation(reassign(
            agent_id,
            target,
            "move-agent",
          )))
          .await
          .unwrap();
        assert_eq!(replay.disposition, MutationDisposition::Replayed);
        assert_eq!(replay.agent, moved.agent);

        store
          .set_current_execution(agent_id, Some(active_execution.clone()))
          .unwrap();
        let drained = store
          .drain_agent(crate::testing::management_mutation(DrainAgent {
            agent_id,
            expected_version: moved.agent.version,
            mode: AgentDrainMode::Graceful,
            idempotency_key: crate::IdempotencyKey::new("drain-agent").unwrap(),
            requested_at: time(30),
          }))
          .await
          .unwrap();
        assert_eq!(drained.agent.status, crate::AgentStatus::Draining);
        assert_eq!(drained.agent.last_seen_at, time(10));
        assert_eq!(
          drained
            .current_execution
            .as_ref()
            .map(|execution| execution.lease_state),
          Some(AgentCurrentLeaseState::DrainRequested)
        );
        store.set_current_execution(agent_id, None).unwrap();
        let replayed = store
          .drain_agent(crate::testing::management_mutation(DrainAgent {
            agent_id,
            expected_version: moved.agent.version,
            mode: AgentDrainMode::Graceful,
            idempotency_key: crate::IdempotencyKey::new("drain-agent").unwrap(),
            requested_at: time(30),
          }))
          .await
          .unwrap();
        assert_eq!(replayed.disposition, MutationDisposition::Replayed);
        assert_eq!(replayed.current_execution, drained.current_execution);
        assert_management_audit_facts(
          store.as_ref(),
          [
            expected_management_audit(StoreOperation::ReassignAgentPool, EntityKind::Agent, agent_id),
            expected_management_audit(StoreOperation::DrainAgent, EntityKind::Agent, agent_id),
          ],
        )
        .await;
      },
      "in-memory Agent management futures must be immediately ready",
    );
  }

  fn reassign(agent_id: AgentId, target_pool_id: PoolId, key_value: &str) -> ReassignAgentPool {
    ReassignAgentPool {
      agent_id,
      expected_version: AgentVersion::INITIAL,
      target_pool_id,
      idempotency_key: crate::IdempotencyKey::new(key_value).unwrap(),
      reassigned_at: time(20),
    }
  }

  fn current_execution() -> AgentCurrentExecution {
    AgentCurrentExecution {
      lease_id: id::<LeaseId>(20),
      build_id: id::<BuildId>(21),
      attempt_id: id::<AttemptId>(22),
      job_id: id::<JobId>(23),
      lease_state: AgentCurrentLeaseState::Active,
    }
  }

  fn pool_definition() -> AgentPoolDefinition {
    AgentPoolDefinition {
      enabled: true,
      drain_state: PoolDrainState::Accepting,
      admission_policy: PoolAdmissionPolicy::Any,
      concurrency_limit: 2,
      fairness_policy: crate::PoolFairnessPolicy::PriorityFifo,
      static_capacity_limit: 2,
    }
  }

  fn enrolled_agent(pool_id: PoolId) -> EnrolledAgent {
    EnrolledAgent {
      id: id(10),
      name: AgentName::new("builder-1").unwrap(),
      version: AgentVersion::INITIAL,
      pool_id,
      pool_version: PoolVersion::INITIAL,
      inventory: AgentInventory {
        agent_id: "builder-1".to_owned(),
        agent_version: "0.1.0".to_owned(),
        coordinator_protocols: vec![COORDINATOR_PROTOCOL_VERSION],
        execution_contract: octacity_protocol::ExecutionContractRange { min: 1, max: 1 },
        labels: BTreeMap::new(),
        host_platform: PlatformSpec {
          os: PlatformOs::Linux,
          architecture: PlatformArchitecture::Amd64,
        },
        host_capacity: HostCapacity {
          logical_cpu_count: 4,
          total_memory_bytes: 1_024,
          work_disk_total_bytes: 2_048,
          state_disk_total_bytes: 2_048,
          virtualization_available: false,
        },
        runtimes: Vec::new(),
        executions: Vec::new(),
        octa: OctaInventory {
          version: "0.1.0".to_owned(),
          runner_sha256: "1".repeat(64),
          build_commit: None,
          runner_protocols: vec![1],
          event_schemas: vec![1],
          plugin_protocols: vec![1],
          octafile_versions: vec![1],
          features: Vec::new(),
          plugins: Vec::new(),
        },
        source_plugins: Vec::new(),
        cache: None,
      },
      status: crate::AgentStatus::Online,
      last_seen_at: time(10),
      created_at: Timestamp::from_unix_millis(10).unwrap(),
      updated_at: time(10),
    }
  }
}
