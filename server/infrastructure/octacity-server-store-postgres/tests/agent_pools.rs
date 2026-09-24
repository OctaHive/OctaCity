mod support;

use std::str::FromStr as _;

use octacity_server_domain::{EntityKind, PoolId, PoolName, PoolVersion, Timestamp};
use octacity_server_scheduler::PoolDrainState;
use octacity_server_store::{
  AgentPoolDefinition, AgentPoolStore as _, CreateAgentPool, DeleteAgentPool, IdempotencyKey, PoolAdmissionPolicy,
  PoolFairnessPolicy, StoreError,
};
use octacity_server_store_postgres::PostgresStore;
use serde_json::json;
use support::TestDatabase;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn postgres_prevents_deletion_for_every_protected_pool_reference() {
  let database = TestDatabase::migrated().await;
  let store = PostgresStore::new(database.pool.clone());
  let pool_ids = [id(1), id(2), id(3), id(4)];
  for (offset, pool_id) in pool_ids.into_iter().enumerate() {
    store
      .create_agent_pool(CreateAgentPool {
        id: pool_id,
        name: PoolName::new(format!("guarded-{offset}")).unwrap(),
        definition: AgentPoolDefinition {
          enabled: true,
          drain_state: PoolDrainState::Accepting,
          admission_policy: PoolAdmissionPolicy::Any,
          concurrency_limit: 1,
          fairness_policy: PoolFairnessPolicy::PriorityFifo,
          static_capacity_limit: 2,
        },
        idempotency_key: key(&format!("create-guarded-{offset}")),
        published_at: time(10),
      })
      .await
      .unwrap();
  }
  seed_protected_references(&database.pool, pool_ids).await;

  for (offset, pool_id) in pool_ids.into_iter().enumerate() {
    assert_eq!(
      store
        .delete_agent_pool(DeleteAgentPool {
          id: pool_id,
          expected_current_version: PoolVersion::INITIAL,
          idempotency_key: key(&format!("delete-guarded-{offset}")),
          deleted_at: time(20),
        })
        .await
        .unwrap_err(),
      StoreError::Conflict {
        entity: EntityKind::Pool
      }
    );
  }
  database.cleanup().await;
}

async fn seed_protected_references(pool: &sqlx::PgPool, pools: [PoolId; 4]) {
  let project = uuid(10);
  let repository = uuid(11);
  let pipeline = uuid(12);
  let configuration = uuid(13);
  sqlx::query(
    "INSERT INTO projects (id, parent_id, name, version, created_at, updated_at) \
     VALUES ($1, NULL, 'pool-guards', 1, now(), now())",
  )
  .bind(project)
  .execute(pool)
  .await
  .unwrap();
  sqlx::query("INSERT INTO repositories (id, project_id, name, created_at) VALUES ($1, $2, 'source', now())")
    .bind(repository)
    .bind(project)
    .execute(pool)
    .await
    .unwrap();
  sqlx::query(
    "INSERT INTO repository_versions \
       (repository_id, version, vcs_integration_id, repository_locator, selection_policy, published_at) \
     VALUES ($1, 1, $2, 'https://example.test/source.git', '{}', now())",
  )
  .bind(repository)
  .bind(uuid(14))
  .execute(pool)
  .await
  .unwrap();
  sqlx::query("INSERT INTO pipelines (id, project_id, name, created_at) VALUES ($1, $2, 'pipeline', now())")
    .bind(pipeline)
    .bind(project)
    .execute(pool)
    .await
    .unwrap();
  sqlx::query(
    "INSERT INTO pipeline_versions (pipeline_id, version, dag_snapshot, published_at) VALUES ($1, 1, '{}', now())",
  )
  .bind(pipeline)
  .execute(pool)
  .await
  .unwrap();
  sqlx::query("INSERT INTO build_configurations (id, project_id, name, created_at) VALUES ($1, $2, 'build', now())")
    .bind(configuration)
    .bind(project)
    .execute(pool)
    .await
    .unwrap();
  sqlx::query(
    "INSERT INTO build_configuration_versions \
       (build_configuration_id, version, enabled, repository_id, repository_version, pipeline_id, pipeline_version, \
        configuration_snapshot, job_concurrency_limit, allowed_pool_ids, retry_max_attempts, retries_infrastructure, \
        published_at) \
     VALUES ($1, 1, true, $2, 1, $3, 1, $4, 1, $5, 1, false, now())",
  )
  .bind(configuration)
  .bind(repository)
  .bind(pipeline)
  .bind(json!({"allowed_pools": [pools[0].to_string()]}))
  .bind(vec![pools[0].as_uuid()])
  .execute(pool)
  .await
  .unwrap();

  let agent = uuid(20);
  let registration = uuid(21);
  sqlx::query(
    "INSERT INTO agents (id, name, pool_id, pool_version, state, inventory, version, created_at, updated_at) \
     VALUES ($1, 'agent', $2, 1, 'online', '{}', 1, now(), now())",
  )
  .bind(agent)
  .bind(pools[1].as_uuid())
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO agent_registrations (id, agent_id, epoch, credential_hash, inventory, registered_at, expires_at) \
     VALUES ($1, $2, 1, decode(repeat('01', 32), 'hex'), '{}', now(), now() + interval '1 hour')",
  )
  .bind(registration)
  .bind(agent)
  .execute(pool)
  .await
  .unwrap();

  let trigger = uuid(30);
  let occurrence = uuid(31);
  let build = uuid(32);
  let attempt = uuid(33);
  let leased_job = uuid(34);
  let ready_job = uuid(35);
  sqlx::query(
    "INSERT INTO triggers \
       (id, version, build_configuration_id, build_configuration_version, kind, enabled, definition, created_at) \
     VALUES ($1, 1, $2, 1, 'manual', true, '{}', now())",
  )
  .bind(trigger)
  .bind(configuration)
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO trigger_occurrences \
       (id, trigger_id, trigger_version, build_configuration_id, build_configuration_version, kind, \
        deduplication_identity, cause, causality, provider_metadata, source_time, state, request_digest, created_at, updated_at) \
     VALUES ($1, $2, 1, $3, 1, 'manual', 'pool-guard', '{\"kind\":\"manual\"}', \
             jsonb_build_object('root_occurrence_id', $1::text, 'parent_occurrence_id', NULL, 'depth', 0), '{}', \
             now(), 'accepted', decode(repeat('02', 32), 'hex'), now(), now())",
  )
  .bind(occurrence)
  .bind(trigger)
  .bind(configuration)
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO builds \
       (id, project_id, build_configuration_id, build_configuration_version, pipeline_id, pipeline_version, \
        repository_id, repository_version, trigger_occurrence_id, immutable_revision, input_snapshot, \
        effective_policy_snapshot, project_job_concurrency_limit, priority, state, version, created_at, updated_at, \
        metadata_retention_until, log_retention_until, artifact_retention_until, report_retention_until) \
     VALUES ($1, $2, $3, 1, $4, 1, $5, 1, $6, 'revision', '{}', '{}', 1, 0, 'running', 1, now(), now(), \
             now() + interval '100 years', now() + interval '100 years', now() + interval '100 years', \
             now() + interval '100 years')",
  )
  .bind(build)
  .bind(project)
  .bind(configuration)
  .bind(pipeline)
  .bind(repository)
  .bind(occurrence)
  .execute(pool)
  .await
  .unwrap();
  sqlx::query("INSERT INTO attempts (id, build_id, attempt_number, state, version, created_at, updated_at) VALUES ($1, $2, 1, 'running', 1, now(), now())")
    .bind(attempt)
    .bind(build)
    .execute(pool)
    .await
    .unwrap();
  for (job, state, allowed_pool) in [(leased_job, "leased", pools[2]), (ready_job, "ready", pools[3])] {
    sqlx::query(
      "INSERT INTO jobs \
         (id, attempt_id, pipeline_node_id, state, allowed_pool_ids, requirements, job_spec_template, dependency_policy, \
          signed_job_spec, version, created_at, updated_at) \
       VALUES ($1, $2, $3, $4, ARRAY[$5]::uuid[], '{}', '{}', '\"all_succeeded\"', '{}', 1, now(), now())",
    )
    .bind(job)
    .bind(attempt)
    .bind(format!("node-{job}"))
    .bind(state)
    .bind(allowed_pool.as_uuid())
    .execute(pool)
    .await
    .unwrap();
  }
  sqlx::query(
    "INSERT INTO leases \
       (id, job_id, pool_id, pool_version, registration_id, registration_epoch, fence_hash, state, version, \
        leased_at, expires_at) \
     VALUES ($1, $2, $3, 1, $4, 1, decode(repeat('03', 32), 'hex'), 'active', 1, now(), \
             now() + interval '5 minutes')",
  )
  .bind(uuid(36))
  .bind(leased_job)
  .bind(pools[2].as_uuid())
  .bind(registration)
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO ready_queue_entries \
       (job_id, priority, enqueue_order, enqueued_at, project_id, build_configuration_id, build_configuration_version, \
        allowed_pool_ids, requirements) \
     VALUES ($1, 0, 1, now(), $2, $3, 1, ARRAY[$4]::uuid[], '{}')",
  )
  .bind(ready_job)
  .bind(project)
  .bind(configuration)
  .bind(pools[3].as_uuid())
  .execute(pool)
  .await
  .unwrap();
}

fn id(value: u128) -> PoolId {
  PoolId::from_str(&format!("{value:08x}-0000-4000-8000-000000000000")).unwrap()
}

fn uuid(value: u128) -> uuid::Uuid {
  uuid::Uuid::from_u128((value << 96) | (0x4000_u128 << 64) | (0x8000_u128 << 48))
}

fn key(value: &str) -> IdempotencyKey {
  IdempotencyKey::new(value).unwrap()
}

fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}
