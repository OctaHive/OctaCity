use std::{
  collections::{BTreeMap, BTreeSet},
  sync::{Mutex, MutexGuard},
};

use async_trait::async_trait;
use octacity_server_domain::{AgentId, EntityKind, PoolId, PoolVersion};

use crate::{
  AgentDrainMode, AgentPage, AgentPlatform, AgentPoolDefinition, AgentStore, DrainAgent, DrainAgentOutcome,
  EnrolledAgent, ListAgents, MutationDisposition, PoolAdmissionPolicy, ReassignAgentPool, ReassignAgentPoolOutcome,
  StoreError,
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
  active_leases: BTreeSet<AgentId>,
  mutations: BTreeMap<String, (Fingerprint, ReassignAgentPoolOutcome)>,
  drain_mutations: BTreeMap<String, (DrainFingerprint, DrainAgentOutcome)>,
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

  /// Marks whether an Agent owns an active Lease.
  pub fn set_active_lease(&self, agent_id: AgentId, active: bool) -> Result<(), StoreError> {
    let mut state = self.lock()?;
    if !state.agents.contains_key(&agent_id) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Agent,
      });
    }
    if active {
      state.active_leases.insert(agent_id);
    } else {
      state.active_leases.remove(&agent_id);
    }
    Ok(())
  }

  fn lock(&self) -> Result<MutexGuard<'_, State>, StoreError> {
    self.state.lock().map_err(|_| StoreError::Unavailable)
  }
}

#[async_trait]
impl AgentStore for InMemoryAgentStore {
  async fn agent(&self, agent_id: AgentId) -> Result<EnrolledAgent, StoreError> {
    self
      .lock()?
      .agents
      .get(&agent_id)
      .map(|record| record.0.clone())
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Agent,
      })
  }

  async fn list_agents(&self, request: ListAgents) -> Result<AgentPage, StoreError> {
    let state = self.lock()?;
    let limit = usize::from(request.limit().get());
    let mut agents = state
      .agents
      .iter()
      .filter(|(id, _)| request.after().is_none_or(|after| **id > after))
      .map(|(_, record)| record.0.clone())
      .take(limit + 1)
      .collect::<Vec<_>>();
    let has_more = agents.len() > limit;
    agents.truncate(limit);
    let next_cursor = has_more.then(|| agents.last().expect("non-zero full page has a last Agent").id);
    Ok(AgentPage { agents, next_cursor })
  }

  async fn reassign_agent_pool(&self, request: ReassignAgentPool) -> Result<ReassignAgentPoolOutcome, StoreError> {
    let fingerprint = Fingerprint {
      agent_id: request.agent_id,
      expected_version: request.expected_version,
      target_pool_id: request.target_pool_id,
    };
    let mut state = self.lock()?;
    if let Some((stored_fingerprint, stored_outcome)) = state.mutations.get(request.idempotency_key.as_str()) {
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
    if state.active_leases.contains(&request.agent_id) {
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
      PoolAdmissionPolicy::Allowlist { platforms } => platforms.contains(&platform),
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
    state
      .mutations
      .insert(request.idempotency_key.to_string(), (fingerprint, outcome.clone()));
    Ok(outcome)
  }

  async fn drain_agent(&self, request: DrainAgent) -> Result<DrainAgentOutcome, StoreError> {
    let fingerprint = DrainFingerprint {
      agent_id: request.agent_id,
      expected_version: request.expected_version,
      mode: request.mode,
    };
    let mut state = self.lock()?;
    if let Some((stored_fingerprint, stored_outcome)) = state.drain_mutations.get(request.idempotency_key.as_str()) {
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
    let outcome = DrainAgentOutcome {
      disposition: MutationDisposition::Applied,
      agent,
    };
    state
      .drain_mutations
      .insert(request.idempotency_key.to_string(), (fingerprint, outcome.clone()));
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
  use octacity_server_domain::{AgentName, AgentVersion, Timestamp};
  use octacity_server_scheduler::PoolDrainState;

  use super::*;
  use crate::test_support::{id, run_ready, time};

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

    run_ready(
      async move {
        let listed = store.list_agents(ListAgents::new(None, 1).unwrap()).await.unwrap();
        assert_eq!(listed.agents.len(), 1);
        assert_eq!(store.agent(agent_id).await.unwrap().pool_id, source);

        store.set_active_lease(agent_id, true).unwrap();
        assert_eq!(
          store
            .reassign_agent_pool(reassign(agent_id, target, "move-agent"))
            .await
            .unwrap_err(),
          StoreError::Conflict {
            entity: EntityKind::Lease
          },
        );
        assert_eq!(store.agent(agent_id).await.unwrap().pool_id, source);

        store.set_active_lease(agent_id, false).unwrap();
        let moved = store
          .reassign_agent_pool(reassign(agent_id, target, "move-agent"))
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
          .reassign_agent_pool(reassign(agent_id, target, "move-agent"))
          .await
          .unwrap();
        assert_eq!(replay.disposition, MutationDisposition::Replayed);
        assert_eq!(replay.agent, moved.agent);

        store.set_active_lease(agent_id, true).unwrap();
        let drained = store
          .drain_agent(DrainAgent {
            agent_id,
            expected_version: moved.agent.version,
            mode: AgentDrainMode::Graceful,
            idempotency_key: crate::IdempotencyKey::new("drain-agent").unwrap(),
            requested_at: time(30),
          })
          .await
          .unwrap();
        assert_eq!(drained.agent.status, crate::AgentStatus::Draining);
        assert_eq!(drained.agent.last_seen_at, time(10));
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
