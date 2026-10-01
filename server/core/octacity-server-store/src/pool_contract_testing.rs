use std::{collections::BTreeSet, sync::Arc};

use octacity_server_domain::{EntityKind, PoolId, PoolName, PoolVersion};
use octacity_server_scheduler::PoolDrainState;

use crate::test_support::{id, run_ready, time};
use crate::testing::{
  InMemoryAgentPoolStore, ManagementAuditProbe, MutationEvidenceProbe, PoolReferenceKind,
  assert_management_audit_facts, expected_management_audit, management_mutation,
};
use crate::{
  AgentPlatform, AgentPoolDefinition, AgentPoolStore, CreateAgentPool, DeleteAgentPool, IdempotencyKey, ListAgentPools,
  ManagementMutation, MutationDisposition, PoolAdmissionPolicy, PublishAgentPoolVersion, StoreError, StoreOperation,
};

/// Runs the reusable versioned Agent Pool contract against one empty adapter.
pub async fn verify_agent_pool_store_contract<S, P>(store: Arc<S>, evidence: Arc<P>)
where
  S: AgentPoolStore + 'static,
  P: ManagementAuditProbe + MutationEvidenceProbe + 'static,
{
  let mut expected_audit = Vec::new();
  let pool_id = id::<PoolId>(1);
  let created = store
    .create_agent_pool(create(
      pool_id,
      "linux",
      "create-linux",
      definition(PoolDrainState::Accepting),
      10,
    ))
    .await
    .unwrap();
  assert_eq!(created.disposition, MutationDisposition::Applied);
  assert_eq!(created.pool.version, PoolVersion::INITIAL);
  expected_audit.push(expected_management_audit(
    StoreOperation::CreateAgentPool,
    EntityKind::Pool,
    pool_id,
  ));

  let replay = store
    .create_agent_pool(create(
      pool_id,
      "linux",
      "create-linux",
      definition(PoolDrainState::Accepting),
      99,
    ))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.pool, created.pool);
  assert_eq!(
    store
      .create_agent_pool(create(
        id(2),
        "different",
        "create-linux",
        definition(PoolDrainState::Accepting),
        99
      ))
      .await
      .unwrap_err(),
    conflict()
  );

  let draining = store
    .publish_agent_pool_version(management_mutation(PublishAgentPoolVersion {
      id: pool_id,
      expected_current_version: PoolVersion::INITIAL,
      definition: definition(PoolDrainState::GracefulDrain),
      idempotency_key: key("drain-linux"),
      published_at: time(20),
    }))
    .await
    .unwrap();
  assert_eq!(draining.pool.version.get(), 2);
  assert_eq!(draining.pool.definition.drain_state, PoolDrainState::GracefulDrain);
  expected_audit.push(expected_management_audit(
    StoreOperation::PublishAgentPoolVersion,
    EntityKind::Pool,
    pool_id,
  ));
  assert_eq!(
    store
      .publish_agent_pool_version(management_mutation(PublishAgentPoolVersion {
        id: pool_id,
        expected_current_version: PoolVersion::INITIAL,
        definition: definition(PoolDrainState::ForcedDrain),
        idempotency_key: key("stale-pool-version"),
        published_at: time(21),
      }))
      .await
      .unwrap_err(),
    conflict()
  );

  let exact = store.agent_pool_version(pool_id, PoolVersion::INITIAL).await.unwrap();
  assert_eq!(exact, created.pool, "publishing must not rewrite the prior version");
  let page = store
    .list_agent_pools(ListAgentPools::new(None, 1, crate::AgentPoolListVisibility::all()).unwrap())
    .await
    .unwrap();
  assert_eq!(page.pools.as_slice(), std::slice::from_ref(&draining.pool));
  let restricted = store
    .list_agent_pools(
      ListAgentPools::new(None, 1, crate::AgentPoolListVisibility::restricted([pool_id]).unwrap()).unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(restricted, page);
  for visibility in [
    crate::AgentPoolListVisibility::restricted([id(999)]).unwrap(),
    crate::AgentPoolListVisibility::none(),
  ] {
    let hidden = store
      .list_agent_pools(ListAgentPools::new(None, 1, visibility).unwrap())
      .await
      .unwrap();
    assert!(hidden.pools.is_empty());
    assert_eq!(hidden.next_cursor, None);
  }

  verify_visibility_before_pool_pagination(store.as_ref(), &mut expected_audit).await;

  let disposable_id = id::<PoolId>(3);
  let disposable = store
    .create_agent_pool(create(
      disposable_id,
      "disposable",
      "create-disposable-pool",
      definition(PoolDrainState::Accepting),
      30,
    ))
    .await
    .unwrap()
    .pool;
  expected_audit.push(expected_management_audit(
    StoreOperation::CreateAgentPool,
    EntityKind::Pool,
    disposable_id,
  ));
  let deleted = store
    .delete_agent_pool(management_mutation(DeleteAgentPool {
      id: disposable_id,
      expected_current_version: disposable.version,
      idempotency_key: key("delete-disposable-pool"),
      deleted_at: time(31),
    }))
    .await
    .unwrap();
  assert_eq!(deleted.disposition, MutationDisposition::Applied);
  expected_audit.push(expected_management_audit(
    StoreOperation::DeleteAgentPool,
    EntityKind::Pool,
    disposable_id,
  ));
  assert_eq!(
    store
      .delete_agent_pool(management_mutation(DeleteAgentPool {
        id: disposable_id,
        expected_current_version: disposable.version,
        idempotency_key: key("delete-disposable-pool"),
        deleted_at: time(99),
      }))
      .await
      .unwrap()
      .disposition,
    MutationDisposition::Replayed
  );
  assert_eq!(
    store
      .agent_pool_version(disposable_id, disposable.version)
      .await
      .unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::Pool
    }
  );

  let counts = evidence.mutation_evidence_counts().await;
  assert_eq!(counts.idempotency, counts.audit);
  assert_eq!(counts.audit, counts.outbox);
  assert_management_audit_facts(evidence.as_ref(), expected_audit).await;
}

/// Runs the Pool contract and guarded-reference cases against the in-memory adapter.
pub fn verify_in_memory_agent_pool_store_contract() {
  let store = Arc::new(InMemoryAgentPoolStore::new());
  run_ready(
    async {
      verify_agent_pool_store_contract(Arc::clone(&store), Arc::clone(&store)).await;
      for (offset, kind) in [
        PoolReferenceKind::BuildConfiguration,
        PoolReferenceKind::Agent,
        PoolReferenceKind::ActiveLease,
        PoolReferenceKind::ReadyJob,
      ]
      .into_iter()
      .enumerate()
      {
        let raw_id = 100 + u64::try_from(offset).unwrap();
        let pool_id = id::<PoolId>(raw_id);
        let pool = store
          .create_agent_pool(create(
            pool_id,
            &format!("guarded-{offset}"),
            &format!("create-guarded-{offset}"),
            definition(PoolDrainState::Accepting),
            100 + i64::try_from(offset).unwrap(),
          ))
          .await
          .unwrap()
          .pool;
        store.seed_reference(pool_id, kind).unwrap();
        assert_eq!(
          store
            .delete_agent_pool(management_mutation(DeleteAgentPool {
              id: pool_id,
              expected_current_version: pool.version,
              idempotency_key: key(&format!("delete-guarded-{offset}")),
              deleted_at: time(200 + i64::try_from(offset).unwrap()),
            }))
            .await
            .unwrap_err(),
          conflict(),
          "{kind:?} must guard Pool deletion"
        );
      }
    },
    "in-memory Agent Pool operations must complete without I/O",
  );
}

async fn verify_visibility_before_pool_pagination<S>(
  store: &S,
  expected_audit: &mut Vec<crate::testing::RecordedManagementAuditFact>,
) where
  S: AgentPoolStore,
{
  let mut pools = Vec::new();
  for (value, name) in [
    (1_001, "visibility-first"),
    (1_002, "visibility-second"),
    (1_003, "visibility-third"),
  ] {
    let pool = store
      .create_agent_pool(create(
        id(value),
        name,
        name,
        definition(PoolDrainState::Accepting),
        i64::try_from(value).unwrap(),
      ))
      .await
      .unwrap()
      .pool;
    expected_audit.push(expected_management_audit(
      StoreOperation::CreateAgentPool,
      EntityKind::Pool,
      pool.id,
    ));
    pools.push(pool);
  }

  let all = store
    .list_agent_pools(ListAgentPools::new(Some(id(1)), 2, crate::AgentPoolListVisibility::all()).unwrap())
    .await
    .unwrap();
  assert_eq!(all.pools, pools[..2]);
  assert_eq!(all.next_cursor, Some(pools[1].id));

  let restricted = crate::AgentPoolListVisibility::restricted([pools[1].id, pools[2].id]).unwrap();
  let first_visible = store
    .list_agent_pools(ListAgentPools::new(None, 1, restricted.clone()).unwrap())
    .await
    .unwrap();
  assert_eq!(first_visible.pools, vec![pools[1].clone()]);
  assert_eq!(first_visible.next_cursor, Some(pools[1].id));
  let second_visible = store
    .list_agent_pools(ListAgentPools::new(first_visible.next_cursor, 1, restricted).unwrap())
    .await
    .unwrap();
  assert_eq!(second_visible.pools, vec![pools[2].clone()]);
  assert_eq!(second_visible.next_cursor, None);

  let none = store
    .list_agent_pools(ListAgentPools::new(None, 1, crate::AgentPoolListVisibility::none()).unwrap())
    .await
    .unwrap();
  assert!(none.pools.is_empty());
  assert_eq!(none.next_cursor, None);
}

fn create(
  id: PoolId,
  name: &str,
  idempotency_key: &str,
  definition: AgentPoolDefinition,
  at: i64,
) -> ManagementMutation<CreateAgentPool> {
  management_mutation(CreateAgentPool {
    id,
    name: PoolName::new(name).unwrap(),
    definition,
    idempotency_key: key(idempotency_key),
    published_at: time(at),
  })
}

fn definition(drain_state: PoolDrainState) -> AgentPoolDefinition {
  AgentPoolDefinition {
    enabled: true,
    drain_state,
    admission_policy: PoolAdmissionPolicy::Allowlist {
      platforms: BTreeSet::from([AgentPlatform::new("linux", "amd64").unwrap()]),
    },
    concurrency_limit: 2,
    fairness_policy: crate::PoolFairnessPolicy::PriorityFifo,
    static_capacity_limit: 4,
  }
}

fn key(value: &str) -> IdempotencyKey {
  IdempotencyKey::new(value).unwrap()
}

fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Pool,
  }
}
