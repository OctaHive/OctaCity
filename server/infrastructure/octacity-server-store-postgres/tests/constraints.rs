mod support;

use octacity_server_domain::ProjectId;
use octacity_server_store::LogIndexWorkStore as _;
use octacity_server_store_postgres::PostgresStore;
use sqlx::{Error, PgPool, postgres::PgQueryResult};
use support::TestDatabase;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn authoritative_constraints_reject_invalid_state() {
  let database = TestDatabase::migrated().await;
  let result = verify_constraints(&database.pool).await;
  database.cleanup().await;
  result.unwrap();
}

async fn verify_constraints(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
  seed_authoritative_graph(pool).await?;

  expect_constraint(
    sqlx::query("UPDATE projects SET parent_id = $1 WHERE id = $2")
      .bind(id(2))
      .bind(id(1))
      .execute(pool)
      .await,
    "projects_acyclic",
  )?;

  expect_constraint(
    sqlx::query("UPDATE pipeline_versions SET dag_snapshot = '{\"changed\":true}' WHERE pipeline_id = $1")
      .bind(id(20))
      .execute(pool)
      .await,
    "pipeline_versions_immutable",
  )?;
  expect_constraint(
    sqlx::query("DELETE FROM pipeline_versions WHERE pipeline_id = $1")
      .bind(id(20))
      .execute(pool)
      .await,
    "pipeline_versions_immutable",
  )?;

  expect_constraint(insert_attempt(pool, 162, 3).await, "attempts_monotonic_number")?;
  insert_attempt(pool, 162, 2).await?;
  expect_constraint(
    sqlx::query("UPDATE attempts SET attempt_number = 3 WHERE id = $1")
      .bind(id(162))
      .execute(pool)
      .await,
    "attempts_immutable_identity",
  )?;
  expect_constraint(
    sqlx::query("DELETE FROM attempts WHERE id = $1")
      .bind(id(162))
      .execute(pool)
      .await,
    "attempts_immutable_identity",
  )?;

  insert_job(pool, 179, 162, "retry").await?;
  sqlx::query(
    "INSERT INTO job_dependencies (attempt_id, job_id, dependency_job_id, dependency_policy) \
     VALUES ($1, $2, $3, to_jsonb('all_succeeded'::text))",
  )
  .bind(id(161))
  .bind(id(178))
  .bind(id(177))
  .execute(pool)
  .await?;
  expect_constraint(
    sqlx::query(
      "INSERT INTO job_dependencies (attempt_id, job_id, dependency_job_id, dependency_policy) \
       VALUES ($1, $2, $3, to_jsonb('all_succeeded'::text))",
    )
    .bind(id(161))
    .bind(id(178))
    .bind(id(177))
    .execute(pool)
    .await,
    "job_dependencies_pkey",
  )?;
  expect_constraint(
    sqlx::query(
      "INSERT INTO job_dependencies (attempt_id, job_id, dependency_job_id, dependency_policy) \
       VALUES ($1, $2, $2, to_jsonb('all_succeeded'::text))",
    )
    .bind(id(161))
    .bind(id(177))
    .execute(pool)
    .await,
    "job_dependencies_not_self",
  )?;
  expect_constraint(
    sqlx::query(
      "INSERT INTO job_dependencies (attempt_id, job_id, dependency_job_id, dependency_policy) \
       VALUES ($1, $2, $3, to_jsonb('all_succeeded'::text))",
    )
    .bind(id(161))
    .bind(id(178))
    .bind(id(179))
    .execute(pool)
    .await,
    "job_dependencies_dependency_attempt_fk",
  )?;

  insert_ready_entry(pool, 177).await?;
  expect_constraint(insert_lease(pool, 193, 177).await, "jobs_one_current_queue_or_lease")?;
  sqlx::query("DELETE FROM ready_queue_entries WHERE job_id = $1")
    .bind(id(177))
    .execute(pool)
    .await?;
  insert_lease(pool, 193, 177).await?;
  expect_constraint(insert_lease(pool, 194, 177).await, "leases_one_current_per_job_idx")?;
  expect_constraint(insert_ready_entry(pool, 177).await, "jobs_one_current_queue_or_lease")?;

  expect_constraint(
    sqlx::query(
      "INSERT INTO agent_registrations \
       (id, agent_id, epoch, credential_hash, inventory, registered_at, expires_at) \
       VALUES ($1, $2, 1, decode(repeat('02', 32), 'hex'), '{}', now(), now() + interval '1 hour')",
    )
    .bind(id(129))
    .bind(id(112))
    .execute(pool)
    .await,
    "agent_registrations_agent_epoch_key",
  )?;

  sqlx::query("UPDATE leases SET state = 'completed', completed_at = now() WHERE id = $1")
    .bind(id(193))
    .execute(pool)
    .await?;
  insert_event(pool, 1).await?;
  expect_constraint(insert_event(pool, 3).await, "job_events_contiguous_sequence")?;
  insert_event(pool, 2).await?;
  expect_constraint(insert_event(pool, 2).await, "job_events_contiguous_sequence")?;
  expect_constraint(
    sqlx::query("DELETE FROM job_events WHERE job_id = $1 AND sequence = 2")
      .bind(id(177))
      .execute(pool)
      .await,
    "job_events_append_only",
  )?;

  insert_log_chunk(pool).await?;
  insert_log_work(pool, 225, 1, "index", Some(224)).await?;
  expect_constraint(
    insert_log_work(pool, 226, 1, "index", Some(224)).await,
    "log_indexing_work_contiguous_position",
  )?;
  expect_constraint(
    insert_log_work(pool, 227, 0, "index", Some(224)).await,
    "log_indexing_work_positive_position",
  )?;
  expect_constraint(
    insert_log_work(pool, 228, 2, "delete_build", Some(224)).await,
    "log_indexing_work_operation_shape",
  )?;
  expect_constraint(
    insert_log_work(pool, 229, 3, "delete_build", None).await,
    "log_indexing_work_contiguous_position",
  )?;
  let allocated = insert_allocated_log_work(pool, 230).await?;
  assert_eq!(allocated, 2);
  let committed = PostgresStore::new(pool.clone())
    .committed_log_index_position(ProjectId::from_uuid(id(1))?)
    .await?;
  assert_eq!(
    committed.and_then(|position| i64::try_from(position.get()).ok()),
    Some(2)
  );
  expect_sql_state(
    sqlx::query("UPDATE log_indexing_work SET position = 3 WHERE id = $1")
      .bind(id(230))
      .execute(pool)
      .await,
    "55000",
  )?;

  insert_published_artifact(pool, 209).await?;
  expect_constraint(
    sqlx::query("UPDATE artifacts SET sha256 = decode(repeat('bb', 32), 'hex') WHERE id = $1")
      .bind(id(209))
      .execute(pool)
      .await,
    "artifacts_published_identity_immutable",
  )?;
  expect_constraint(
    insert_published_artifact(pool, 210).await,
    "artifacts_published_job_name_idx",
  )?;
  expect_constraint(
    sqlx::query("DELETE FROM artifacts WHERE id = $1")
      .bind(id(209))
      .execute(pool)
      .await,
    "artifacts_published_identity_immutable",
  )?;

  insert_audit_fact(pool).await?;
  expect_sql_state(
    sqlx::query("UPDATE audit_facts SET outcome = 'changed' WHERE id = $1")
      .bind(id(240))
      .execute(pool)
      .await,
    "55000",
  )?;
  expect_sql_state(
    sqlx::query("DELETE FROM audit_facts WHERE id = $1")
      .bind(id(240))
      .execute(pool)
      .await,
    "55000",
  )?;

  Ok(())
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn concurrent_log_index_position_allocation_is_contiguous() {
  let database = TestDatabase::migrated().await;
  let result = async {
    seed_authoritative_graph(&database.pool).await?;
    let mut tasks = Vec::new();
    for offset in 0..8_u128 {
      let pool = database.pool.clone();
      tasks.push(tokio::spawn(async move {
        insert_allocated_log_work(&pool, 300 + offset).await
      }));
    }
    let mut positions = Vec::new();
    for task in tasks {
      positions.push(task.await??);
    }
    positions.sort_unstable();
    assert_eq!(positions, (1..=8).collect::<Vec<_>>());
    Ok::<(), Box<dyn std::error::Error>>(())
  }
  .await;
  database.cleanup().await;
  result.unwrap();
}

async fn insert_audit_fact(pool: &PgPool) -> Result<PgQueryResult, Error> {
  sqlx::query(
    "INSERT INTO audit_facts \
       (id, actor_kind, operation, target_kind, target_identity, request_identity, idempotency_key, outcome, \
        safe_metadata, occurred_at) \
     VALUES ($1, 'worker', 'constraint-test', 'build', 'build-1', 'request-1', 'audit-1', 'accepted', '{}', now())",
  )
  .bind(id(240))
  .execute(pool)
  .await
}

async fn seed_authoritative_graph(pool: &PgPool) -> Result<(), Error> {
  sqlx::query(
    "INSERT INTO projects (id, parent_id, name, version, created_at, updated_at) VALUES \
       ($1, NULL, 'root', 1, now(), now()), \
       ($2, $1, 'child', 1, now(), now())",
  )
  .bind(id(1))
  .bind(id(2))
  .execute(pool)
  .await?;
  sqlx::query(
    "INSERT INTO repositories \
       (id, version, project_id, name, vcs_integration_id, repository_locator, selection_policy, created_at) \
     VALUES ($1, 1, $2, 'repo', $3, 'repo', '{}', now())",
  )
  .bind(id(16))
  .bind(id(1))
  .bind(id(17))
  .execute(pool)
  .await?;
  sqlx::query("INSERT INTO pipelines (id, project_id, name, created_at) VALUES ($1, $2, 'pipeline', now())")
    .bind(id(20))
    .bind(id(1))
    .execute(pool)
    .await?;
  sqlx::query(
    "INSERT INTO pipeline_versions (pipeline_id, version, dag_snapshot, published_at) VALUES ($1, 1, '{}', now())",
  )
  .bind(id(20))
  .execute(pool)
  .await?;
  sqlx::query(
    "INSERT INTO build_configurations \
       (id, version, project_id, name, enabled, repository_id, repository_version, pipeline_id, \
        pipeline_version, configuration_snapshot, created_at) \
     VALUES ($1, 1, $2, 'config', true, $3, 1, $4, 1, '{}', now())",
  )
  .bind(id(48))
  .bind(id(1))
  .bind(id(16))
  .bind(id(20))
  .execute(pool)
  .await?;
  sqlx::query(
    "INSERT INTO triggers \
       (id, version, build_configuration_id, build_configuration_version, kind, enabled, definition, created_at) \
     VALUES ($1, 1, $2, 1, 'manual', true, '{}', now())",
  )
  .bind(id(64))
  .bind(id(48))
  .execute(pool)
  .await?;
  sqlx::query(
    "INSERT INTO trigger_occurrences \
       (id, trigger_id, trigger_version, deduplication_identity, cause, source_time, state, request_digest, \
        created_at, updated_at) \
     VALUES ($1, $2, 1, 'occurrence', '{}', now(), 'accepted', decode(repeat('aa', 32), 'hex'), now(), now())",
  )
  .bind(id(80))
  .bind(id(64))
  .execute(pool)
  .await?;
  sqlx::query(
    "INSERT INTO pools \
       (id, version, name, enabled, drain_state, admission_policy, concurrency_limit, created_at) \
     VALUES ($1, 1, 'pool', true, 'accepting', '{}', 1, now())",
  )
  .bind(id(96))
  .execute(pool)
  .await?;
  sqlx::query(
    "INSERT INTO agents (id, name, pool_id, pool_version, state, inventory, version, created_at, updated_at) \
     VALUES ($1, 'agent', $2, 1, 'online', '{}', 1, now(), now())",
  )
  .bind(id(112))
  .bind(id(96))
  .execute(pool)
  .await?;
  sqlx::query(
    "INSERT INTO agent_registrations \
       (id, agent_id, epoch, credential_hash, inventory, registered_at, expires_at) \
     VALUES ($1, $2, 1, decode(repeat('01', 32), 'hex'), '{}', now(), now() + interval '1 hour')",
  )
  .bind(id(128))
  .bind(id(112))
  .execute(pool)
  .await?;
  sqlx::query(
    "INSERT INTO builds \
       (id, project_id, build_configuration_id, build_configuration_version, pipeline_id, pipeline_version, \
        repository_id, repository_version, trigger_occurrence_id, immutable_revision, input_snapshot, \
        effective_policy_snapshot, priority, state, version, created_at, updated_at) \
     VALUES ($1, $2, $3, 1, $4, 1, $5, 1, $6, 'revision', '{}', '{}', 0, 'running', 1, now(), now())",
  )
  .bind(id(144))
  .bind(id(1))
  .bind(id(48))
  .bind(id(20))
  .bind(id(16))
  .bind(id(80))
  .execute(pool)
  .await?;
  insert_attempt(pool, 161, 1).await?;
  insert_job(pool, 177, 161, "root").await?;
  insert_job(pool, 178, 161, "child").await?;
  Ok(())
}

async fn insert_attempt(pool: &PgPool, raw_id: u128, number: i64) -> Result<PgQueryResult, Error> {
  sqlx::query(
    "INSERT INTO attempts (id, build_id, attempt_number, state, version, created_at, updated_at) \
     VALUES ($1, $2, $3, 'running', 1, now(), now())",
  )
  .bind(id(raw_id))
  .bind(id(144))
  .bind(number)
  .execute(pool)
  .await
}

async fn insert_job(pool: &PgPool, raw_id: u128, attempt: u128, node: &str) -> Result<PgQueryResult, Error> {
  sqlx::query(
    "INSERT INTO jobs \
       (id, attempt_id, pipeline_node_id, state, job_snapshot, allowed_pool_ids, requirements, version, created_at, updated_at) \
     VALUES ($1, $2, $3, 'ready', '{}', ARRAY[$4]::uuid[], '{}', 1, now(), now())",
  )
  .bind(id(raw_id))
  .bind(id(attempt))
  .bind(node)
  .bind(id(96))
  .execute(pool)
  .await
}

async fn insert_ready_entry(pool: &PgPool, job: u128) -> Result<PgQueryResult, Error> {
  sqlx::query(
    "INSERT INTO ready_queue_entries \
       (job_id, priority, enqueue_order, enqueued_at, project_id, build_configuration_id, \
        build_configuration_version, allowed_pool_ids, requirements) \
     VALUES ($1, 0, 1, now(), $2, $3, 1, ARRAY[$4]::uuid[], '{}')",
  )
  .bind(id(job))
  .bind(id(1))
  .bind(id(48))
  .bind(id(96))
  .execute(pool)
  .await
}

async fn insert_lease(pool: &PgPool, lease: u128, job: u128) -> Result<PgQueryResult, Error> {
  sqlx::query(
    "INSERT INTO leases \
       (id, job_id, pool_id, pool_version, registration_id, fence_hash, state, version, leased_at, expires_at) \
     VALUES ($1, $2, $3, 1, $4, decode(repeat('01', 32), 'hex'), 'active', 1, now(), now() + interval '5 minutes')",
  )
  .bind(id(lease))
  .bind(id(job))
  .bind(id(96))
  .bind(id(128))
  .execute(pool)
  .await
}

async fn insert_event(pool: &PgPool, sequence: i64) -> Result<PgQueryResult, Error> {
  sqlx::query(
    "INSERT INTO job_events \
       (job_id, sequence, lease_id, event_kind, event_time, payload, event_digest, created_at) \
     VALUES ($1, $2, $3, 'progress', now(), '{}', decode(repeat('aa', 32), 'hex'), now())",
  )
  .bind(id(177))
  .bind(sequence)
  .bind(id(193))
  .execute(pool)
  .await
}

async fn insert_published_artifact(pool: &PgPool, artifact: u128) -> Result<PgQueryResult, Error> {
  sqlx::query(
    "INSERT INTO artifacts \
       (id, build_id, attempt_id, job_id, logical_name, media_type, byte_length, sha256, object_identity, \
        object_generation, state, published_at) \
     VALUES ($1, $2, $3, $4, 'result.tar', 'application/x-tar', 10, \
             decode(repeat('aa', 32), 'hex'), $5, 'generation', 'published', now())",
  )
  .bind(id(artifact))
  .bind(id(144))
  .bind(id(161))
  .bind(id(177))
  .bind(format!("artifact-{artifact}"))
  .execute(pool)
  .await
}

async fn insert_log_chunk(pool: &PgPool) -> Result<PgQueryResult, Error> {
  sqlx::query(
    "INSERT INTO log_chunk_manifests \
       (id, build_id, attempt_id, job_id, stream, first_sequence, last_sequence, object_identity, byte_length, \
        sha256, visible, created_at) \
     VALUES ($1, $2, $3, $4, 'stdout', 1, 2, 'log/chunk', 10, decode(repeat('cc', 32), 'hex'), true, now())",
  )
  .bind(id(224))
  .bind(id(144))
  .bind(id(161))
  .bind(id(177))
  .execute(pool)
  .await
}

async fn insert_log_work(
  pool: &PgPool,
  work: u128,
  position: i64,
  operation: &str,
  chunk: Option<u128>,
) -> Result<PgQueryResult, Error> {
  sqlx::query(
    "INSERT INTO log_indexing_work \
       (id, project_id, position, build_id, chunk_id, operation, state, attempt_count, available_at, created_at) \
     VALUES ($1, $2, $3, $4, $5, $6, 'pending', 0, now(), now())",
  )
  .bind(id(work))
  .bind(id(1))
  .bind(position)
  .bind(id(144))
  .bind(chunk.map(id))
  .bind(operation)
  .execute(pool)
  .await
}

async fn insert_allocated_log_work(pool: &PgPool, work: u128) -> Result<i64, Error> {
  sqlx::query_scalar(
    "INSERT INTO log_indexing_work \
       (id, project_id, position, build_id, chunk_id, operation, state, attempt_count, available_at, created_at) \
     VALUES ($1, $2, NULL, $3, NULL, 'delete_build', 'pending', 0, now(), now()) \
     RETURNING position",
  )
  .bind(id(work))
  .bind(id(1))
  .bind(id(144))
  .fetch_one(pool)
  .await
}

fn expect_constraint(result: Result<PgQueryResult, Error>, expected: &str) -> Result<(), Box<dyn std::error::Error>> {
  match result {
    Err(Error::Database(error)) if error.constraint() == Some(expected) => Ok(()),
    Err(error) => Err(std::io::Error::other(format!("expected constraint {expected}, got {error}")).into()),
    Ok(_) => Err(std::io::Error::other(format!("constraint {expected} accepted invalid state")).into()),
  }
}

fn expect_sql_state(result: Result<PgQueryResult, Error>, expected: &str) -> Result<(), Box<dyn std::error::Error>> {
  match result {
    Err(Error::Database(error)) if error.code().as_deref() == Some(expected) => Ok(()),
    Err(error) => Err(std::io::Error::other(format!("expected SQLSTATE {expected}, got {error}")).into()),
    Ok(_) => Err(std::io::Error::other(format!("SQLSTATE {expected} did not reject invalid state")).into()),
  }
}

fn id(value: u128) -> uuid::Uuid {
  uuid::Uuid::from_u128(value)
}
