use std::{
  collections::{BTreeMap, BTreeSet},
  sync::{Mutex, MutexGuard},
};

use async_trait::async_trait;
use octacity_server_domain::{EntityKind, PoolId, PoolVersion};

use crate::testing::{MutationEvidenceCounts, MutationEvidenceProbe};
use crate::{
  AgentPoolDefinition, AgentPoolMutationOutcome, AgentPoolPage, AgentPoolStore, CreateAgentPool, DeleteAgentPool,
  DeleteAgentPoolOutcome, ListAgentPools, MutationDisposition, PublishAgentPoolVersion, PublishedAgentPool, StoreError,
  StoreOperation, validate_pool_drain_transition,
};

/// Protected resource kinds that make Pool deletion unsafe.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PoolReferenceKind {
  /// One Build Configuration version allows the Pool.
  BuildConfiguration,
  /// One enrolled Agent belongs to the Pool.
  Agent,
  /// One active Lease selected the Pool.
  ActiveLease,
  /// One durable Ready Job allows the Pool.
  ReadyJob,
}

/// Deterministic process-local Agent Pool adapter for application tests.
#[derive(Default)]
pub struct InMemoryAgentPoolStore {
  state: Mutex<PoolMemoryState>,
}

impl InMemoryAgentPoolStore {
  /// Creates an empty Agent Pool store.
  #[must_use]
  pub fn new() -> Self {
    Self::default()
  }

  /// Adds one protected reference used by deletion contract tests.
  pub fn seed_reference(&self, pool_id: PoolId, kind: PoolReferenceKind) -> Result<(), StoreError> {
    let mut state = self.lock()?;
    if !state.current.contains_key(&pool_id) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Pool,
      });
    }
    state.references.insert((pool_id, kind));
    Ok(())
  }

  /// Removes one protected reference used by deletion contract tests.
  pub fn clear_reference(&self, pool_id: PoolId, kind: PoolReferenceKind) -> Result<(), StoreError> {
    self.lock()?.references.remove(&(pool_id, kind));
    Ok(())
  }

  fn lock(&self) -> Result<MutexGuard<'_, PoolMemoryState>, StoreError> {
    self.state.lock().map_err(|_| StoreError::Unavailable)
  }
}

#[derive(Clone, Default)]
struct PoolMemoryState {
  versions: BTreeMap<(PoolId, PoolVersion), PublishedAgentPool>,
  current: BTreeMap<PoolId, PoolVersion>,
  names: BTreeMap<octacity_server_domain::PoolName, PoolId>,
  mutations: BTreeMap<(&'static str, String), StoredMutation>,
  references: BTreeSet<(PoolId, PoolReferenceKind)>,
  audit: BTreeSet<String>,
  outbox: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Fingerprint {
  Create {
    name: octacity_server_domain::PoolName,
    definition: AgentPoolDefinition,
  },
  Publish {
    id: PoolId,
    expected: PoolVersion,
    definition: AgentPoolDefinition,
  },
  Delete {
    id: PoolId,
    expected: PoolVersion,
  },
}

#[derive(Clone)]
enum StoredOutcome {
  Pool(PublishedAgentPool),
  Deleted(PoolId),
}

#[derive(Clone)]
struct StoredMutation {
  fingerprint: Fingerprint,
  outcome: StoredOutcome,
}

#[async_trait]
impl MutationEvidenceProbe for InMemoryAgentPoolStore {
  async fn mutation_evidence_counts(&self) -> MutationEvidenceCounts {
    let state = self.lock().expect("in-memory Agent Pool store must remain available");
    MutationEvidenceCounts {
      idempotency: state.mutations.len(),
      audit: state.audit.len(),
      outbox: state.outbox.len(),
    }
  }
}

#[async_trait]
impl AgentPoolStore for InMemoryAgentPoolStore {
  async fn create_agent_pool(&self, request: CreateAgentPool) -> Result<AgentPoolMutationOutcome, StoreError> {
    request
      .definition
      .validate()
      .map_err(|source| StoreError::invalid(StoreOperation::CreateAgentPool, source))?;
    let scope = "create-agent-pool";
    let fingerprint = Fingerprint::Create {
      name: request.name.clone(),
      definition: request.definition.clone(),
    };
    let mut state = self.lock()?;
    if let Some(outcome) = replay_pool(&state, scope, request.idempotency_key.as_str(), &fingerprint)? {
      return Ok(outcome);
    }
    if state.current.contains_key(&request.id) {
      return Err(StoreError::Duplicate {
        entity: EntityKind::Pool,
      });
    }
    if state.names.contains_key(&request.name) {
      return Err(StoreError::Conflict {
        entity: EntityKind::Pool,
      });
    }
    let pool = PublishedAgentPool {
      id: request.id,
      name: request.name,
      version: PoolVersion::INITIAL,
      definition: request.definition,
      published_at: request.published_at,
    };
    state.current.insert(pool.id, pool.version);
    state.names.insert(pool.name.clone(), pool.id);
    state.versions.insert((pool.id, pool.version), pool.clone());
    record(
      &mut state,
      scope,
      request.idempotency_key.as_str(),
      fingerprint,
      StoredOutcome::Pool(pool.clone()),
    );
    Ok(AgentPoolMutationOutcome {
      disposition: MutationDisposition::Applied,
      pool,
    })
  }

  async fn publish_agent_pool_version(
    &self,
    request: PublishAgentPoolVersion,
  ) -> Result<AgentPoolMutationOutcome, StoreError> {
    request
      .definition
      .validate()
      .map_err(|source| StoreError::invalid(StoreOperation::PublishAgentPoolVersion, source))?;
    let scope = "publish-agent-pool-version";
    let fingerprint = Fingerprint::Publish {
      id: request.id,
      expected: request.expected_current_version,
      definition: request.definition.clone(),
    };
    let mut state = self.lock()?;
    if let Some(outcome) = replay_pool(&state, scope, request.idempotency_key.as_str(), &fingerprint)? {
      return Ok(outcome);
    }
    let current_version = state.current.get(&request.id).copied().ok_or(StoreError::NotFound {
      entity: EntityKind::Pool,
    })?;
    if current_version != request.expected_current_version {
      return Err(StoreError::Conflict {
        entity: EntityKind::Pool,
      });
    }
    let current = state
      .versions
      .get(&(request.id, current_version))
      .cloned()
      .ok_or(StoreError::Unavailable)?;
    if request.published_at < current.published_at {
      return Err(StoreError::Conflict {
        entity: EntityKind::Pool,
      });
    }
    let active_leases = state.references.contains(&(request.id, PoolReferenceKind::ActiveLease));
    validate_pool_drain_transition(
      current.definition.drain_state,
      request.definition.drain_state,
      active_leases,
    )?;
    let version = current_version.next().map_err(|_| StoreError::Unavailable)?;
    let pool = PublishedAgentPool {
      version,
      definition: request.definition,
      published_at: request.published_at,
      ..current
    };
    state.current.insert(pool.id, pool.version);
    state.versions.insert((pool.id, pool.version), pool.clone());
    record(
      &mut state,
      scope,
      request.idempotency_key.as_str(),
      fingerprint,
      StoredOutcome::Pool(pool.clone()),
    );
    Ok(AgentPoolMutationOutcome {
      disposition: MutationDisposition::Applied,
      pool,
    })
  }

  async fn agent_pool_version(&self, pool_id: PoolId, version: PoolVersion) -> Result<PublishedAgentPool, StoreError> {
    self
      .lock()?
      .versions
      .get(&(pool_id, version))
      .cloned()
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Pool,
      })
  }

  async fn list_agent_pools(&self, request: ListAgentPools) -> Result<AgentPoolPage, StoreError> {
    let state = self.lock()?;
    let limit = usize::from(request.limit().get());
    let mut pools = state
      .current
      .iter()
      .filter(|(id, _)| request.after().is_none_or(|after| **id > after))
      .map(|(id, version)| {
        state
          .versions
          .get(&(*id, *version))
          .cloned()
          .ok_or(StoreError::Unavailable)
      })
      .take(limit + 1)
      .collect::<Result<Vec<_>, _>>()?;
    let has_more = pools.len() > limit;
    pools.truncate(limit);
    let next_cursor = has_more.then(|| pools.last().expect("a non-zero full page has a last item").id);
    Ok(AgentPoolPage { pools, next_cursor })
  }

  async fn delete_agent_pool(&self, request: DeleteAgentPool) -> Result<DeleteAgentPoolOutcome, StoreError> {
    let scope = "delete-agent-pool";
    let fingerprint = Fingerprint::Delete {
      id: request.id,
      expected: request.expected_current_version,
    };
    let mut state = self.lock()?;
    if let Some(outcome) = replay_delete(&state, scope, request.idempotency_key.as_str(), &fingerprint)? {
      return Ok(outcome);
    }
    let current_version = state.current.get(&request.id).copied().ok_or(StoreError::NotFound {
      entity: EntityKind::Pool,
    })?;
    if current_version != request.expected_current_version {
      return Err(StoreError::Conflict {
        entity: EntityKind::Pool,
      });
    }
    let current = state
      .versions
      .get(&(request.id, current_version))
      .ok_or(StoreError::Unavailable)?;
    if request.deleted_at < current.published_at || state.references.iter().any(|(pool_id, _)| *pool_id == request.id) {
      return Err(StoreError::Conflict {
        entity: EntityKind::Pool,
      });
    }
    state.current.remove(&request.id);
    state.names.retain(|_, id| *id != request.id);
    state.versions.retain(|(id, _), _| *id != request.id);
    record(
      &mut state,
      scope,
      request.idempotency_key.as_str(),
      fingerprint,
      StoredOutcome::Deleted(request.id),
    );
    Ok(DeleteAgentPoolOutcome {
      disposition: MutationDisposition::Applied,
      pool_id: request.id,
    })
  }
}

fn replay_pool(
  state: &PoolMemoryState,
  scope: &'static str,
  key: &str,
  fingerprint: &Fingerprint,
) -> Result<Option<AgentPoolMutationOutcome>, StoreError> {
  let Some(stored) = state.mutations.get(&(scope, key.to_owned())) else {
    return Ok(None);
  };
  if &stored.fingerprint != fingerprint {
    return Err(StoreError::Conflict {
      entity: EntityKind::Pool,
    });
  }
  let StoredOutcome::Pool(pool) = &stored.outcome else {
    return Err(StoreError::Unavailable);
  };
  Ok(Some(AgentPoolMutationOutcome {
    disposition: MutationDisposition::Replayed,
    pool: pool.clone(),
  }))
}

fn replay_delete(
  state: &PoolMemoryState,
  scope: &'static str,
  key: &str,
  fingerprint: &Fingerprint,
) -> Result<Option<DeleteAgentPoolOutcome>, StoreError> {
  let Some(stored) = state.mutations.get(&(scope, key.to_owned())) else {
    return Ok(None);
  };
  if &stored.fingerprint != fingerprint {
    return Err(StoreError::Conflict {
      entity: EntityKind::Pool,
    });
  }
  let StoredOutcome::Deleted(pool_id) = stored.outcome else {
    return Err(StoreError::Unavailable);
  };
  Ok(Some(DeleteAgentPoolOutcome {
    disposition: MutationDisposition::Replayed,
    pool_id,
  }))
}

fn record(
  state: &mut PoolMemoryState,
  scope: &'static str,
  key: &str,
  fingerprint: Fingerprint,
  outcome: StoredOutcome,
) {
  state
    .mutations
    .insert((scope, key.to_owned()), StoredMutation { fingerprint, outcome });
  state.audit.insert(format!("{scope}:{key}"));
  state.outbox.insert(format!("{scope}:{key}"));
}
