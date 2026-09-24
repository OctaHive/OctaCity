#[path = "support/authoritative_fixture.rs"]
mod authoritative_fixture;
mod support;

use std::sync::Arc;

use authoritative_fixture::seed_authoritative_prerequisites;
use octacity_server_domain::{AttemptId, JobId, LeaseId, Timestamp};
use octacity_server_store::{
  AppendJobEvents, BuildLogStream, DurableJobEvent, EventSequence, JobClaim, JobClaimOutcome, JobEventKind,
  JobExecutionStore as _, LeaseAccess, LeaseFence, LeaseWindow, LogChunkManifest, LogChunkManifestStore as _,
  LogIndexWorkStore as _, StoreError, TriggerAcceptanceStore as _,
  testing::{authoritative_store_contract_fixture, compatible_snapshot},
};
use octacity_server_store_postgres::{PostgresAuthoritativeStore, PostgresStore};
use serde_json::json;
use sha2::{Digest as _, Sha256};
use support::TestDatabase;
use tokio::sync::Barrier;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn log_append_atomically_commits_manifest_cursor_work_and_replay() {
  let database = TestDatabase::migrated().await;
  let result = verify_atomic_log_append(&database.pool).await;
  database.cleanup().await;
  result.unwrap();
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn log_append_rollback_cannot_publish_manifest_cursor_or_index_work() {
  let database = TestDatabase::migrated().await;
  let result = verify_log_rollback(&database.pool).await;
  database.cleanup().await;
  result.unwrap();
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn concurrent_log_appends_allocate_one_contiguous_project_sequence() {
  let database = TestDatabase::migrated().await;
  let result = verify_concurrent_allocation(&database.pool).await;
  database.cleanup().await;
  result.unwrap();
}

async fn verify_atomic_log_append(pool: &sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
  let (store, project_id, access, job_id) = leased_job(pool).await?;
  assert_eq!(
    store
      .prepare_job_event_append(access, time(1_500))
      .await?
      .durable_through,
    0
  );
  let request = archived_event(access, job_id, b"safe stdout")?;
  let chunk_id = request.log_chunks[0].chunk_id();
  let first = store.append_job_events(request.clone()).await?;
  assert_eq!(first.acknowledged_through.get(), 1);
  assert_eq!(first.inserted, 1);
  assert!(store.log_chunk_is_committed(chunk_id).await?);
  assert_eq!(
    PostgresStore::new(pool.clone())
      .committed_log_index_position(project_id)
      .await?
      .unwrap()
      .get(),
    1
  );

  let restarted = PostgresAuthoritativeStore::new(pool.clone(), support::test_signer());
  assert_eq!(
    restarted.append_job_events(request).await?,
    first,
    "lost acknowledgement replays exactly"
  );
  let counts: (i64, i64, i64, i64) = sqlx::query_as(
    "SELECT (SELECT COUNT(*) FROM job_events), (SELECT COUNT(*) FROM log_chunk_manifests), \
            (SELECT COUNT(*) FROM log_indexing_work), (SELECT committed_through FROM log_index_project_positions \
             WHERE project_id = $1)",
  )
  .bind(project_id.as_uuid())
  .fetch_one(pool)
  .await?;
  assert_eq!(counts, (1, 1, 1, 1));
  Ok(())
}

async fn verify_log_rollback(pool: &sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
  let (store, project_id, access, job_id) = leased_job(pool).await?;
  sqlx::query(
    "ALTER TABLE audit_facts ADD CONSTRAINT reject_log_append_audit CHECK (operation <> 'append-job-events')",
  )
  .execute(pool)
  .await?;
  let request = archived_event(access, job_id, b"orphan bytes")?;
  assert_eq!(
    store.append_job_events(request.clone()).await.unwrap_err(),
    StoreError::Unavailable
  );
  let counts: (i64, i64, i64, i64) = sqlx::query_as(
    "SELECT (SELECT COUNT(*) FROM job_events), (SELECT COUNT(*) FROM log_chunk_manifests), \
            (SELECT COUNT(*) FROM log_indexing_work), \
            (SELECT COUNT(*) FROM log_index_project_positions WHERE project_id = $1)",
  )
  .bind(project_id.as_uuid())
  .fetch_one(pool)
  .await?;
  assert_eq!(
    counts,
    (0, 0, 0, 0),
    "the complete acknowledgement transaction rolled back"
  );
  sqlx::query("ALTER TABLE audit_facts DROP CONSTRAINT reject_log_append_audit")
    .execute(pool)
    .await?;
  assert_eq!(store.append_job_events(request).await?.acknowledged_through.get(), 1);
  Ok(())
}

async fn verify_concurrent_allocation(pool: &sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
  let (first_store, project_id, first_access, first_job) = leased_job(pool).await?;
  let (second_access, second_job) = clone_leased_job(pool, first_access).await?;
  let second_store = PostgresAuthoritativeStore::new(independent_pool(pool).await, support::test_signer());
  let first_request = archived_event(first_access, first_job, b"first")?;
  let second_request = archived_event(second_access, second_job, b"second")?;
  let barrier = Arc::new(Barrier::new(2));
  let left_barrier = barrier.clone();
  let left = tokio::spawn(async move {
    left_barrier.wait().await;
    first_store.append_job_events(first_request).await
  });
  let right = tokio::spawn(async move {
    barrier.wait().await;
    second_store.append_job_events(second_request).await
  });
  left.await??;
  right.await??;

  let positions: Vec<i64> =
    sqlx::query_scalar("SELECT position FROM log_indexing_work WHERE project_id = $1 ORDER BY position")
      .bind(project_id.as_uuid())
      .fetch_all(pool)
      .await?;
  assert_eq!(positions, [1, 2]);
  let watermark: i64 =
    sqlx::query_scalar("SELECT committed_through FROM log_index_project_positions WHERE project_id = $1")
      .bind(project_id.as_uuid())
      .fetch_one(pool)
      .await?;
  assert_eq!(watermark, 2);
  Ok(())
}

async fn leased_job(
  pool: &sqlx::PgPool,
) -> Result<
  (
    PostgresAuthoritativeStore,
    octacity_server_domain::ProjectId,
    LeaseAccess,
    JobId,
  ),
  Box<dyn std::error::Error>,
> {
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(pool, &fixture).await?;
  let project_id = fixture.request.build.project_id;
  let store = PostgresAuthoritativeStore::new(pool.clone(), support::test_signer());
  store.accept_trigger(fixture.request).await?;
  let claim = JobClaim::new(
    LeaseId::from_uuid(Uuid::from_u128(900)).unwrap(),
    LeaseFence::from_bytes([9; 32]),
    fixture.agent_id,
    fixture.registration_epoch,
    fixture.allowed_pool,
    compatible_snapshot(),
    LeaseWindow::new(time(1_000), time(253_402_300_799_000))?,
  )?;
  let grant = match store.claim_ready_job(claim).await? {
    JobClaimOutcome::Claimed(grant) => *grant,
    JobClaimOutcome::Empty => return Err("fixture job was not claimable".into()),
  };
  let access = LeaseAccess {
    lease_id: grant.lease_id,
    fence: grant.fence,
    agent_id: grant.agent_id,
    registration_epoch: grant.registration_epoch,
  };
  Ok((store, project_id, access, grant.job_id))
}

async fn clone_leased_job(
  pool: &sqlx::PgPool,
  first: LeaseAccess,
) -> Result<(LeaseAccess, JobId), Box<dyn std::error::Error>> {
  let attempt_id = AttemptId::from_uuid(Uuid::from_u128(901)).unwrap();
  let job_id = JobId::from_uuid(Uuid::from_u128(902)).unwrap();
  let lease_id = LeaseId::from_uuid(Uuid::from_u128(903)).unwrap();
  let fence = LeaseFence::from_bytes([10; 32]);
  let first_job: Uuid = sqlx::query_scalar("SELECT job_id FROM leases WHERE id = $1")
    .bind(first.lease_id.as_uuid())
    .fetch_one(pool)
    .await?;
  let first_attempt: Uuid = sqlx::query_scalar("SELECT attempt_id FROM jobs WHERE id = $1")
    .bind(first_job)
    .fetch_one(pool)
    .await?;
  sqlx::query(
    "INSERT INTO attempts (id, build_id, attempt_number, state, version, created_at, updated_at, retry_of_attempt_id) \
     SELECT $1, build_id, 2, 'running', 1, created_at, updated_at, NULL FROM attempts WHERE id = $2",
  )
  .bind(attempt_id.as_uuid())
  .bind(first_attempt)
  .execute(pool)
  .await?;
  sqlx::query(
    "INSERT INTO jobs \
       (id, attempt_id, pipeline_node_id, state, allowed_pool_ids, requirements, version, created_at, updated_at, \
        job_spec_template, dependency_policy, signed_job_spec, infrastructure_requeues) \
     SELECT $1, $2, pipeline_node_id, 'leased', allowed_pool_ids, requirements, 1, created_at, updated_at, \
            job_spec_template, dependency_policy, signed_job_spec, 0 FROM jobs WHERE id = $3",
  )
  .bind(job_id.as_uuid())
  .bind(attempt_id.as_uuid())
  .bind(first_job)
  .execute(pool)
  .await?;
  sqlx::query(
    "INSERT INTO leases \
       (id, job_id, pool_id, pool_version, registration_id, fence_hash, state, version, leased_at, expires_at, \
        completed_at, registration_epoch) \
     SELECT $1, $2, pool_id, pool_version, registration_id, $3, 'active', 1, leased_at, expires_at, NULL, \
            registration_epoch FROM leases WHERE id = $4",
  )
  .bind(lease_id.as_uuid())
  .bind(job_id.as_uuid())
  .bind(Sha256::digest(fence.expose()).to_vec())
  .bind(first.lease_id.as_uuid())
  .execute(pool)
  .await?;
  Ok((
    LeaseAccess {
      lease_id,
      fence,
      agent_id: first.agent_id,
      registration_epoch: first.registration_epoch,
    },
    job_id,
  ))
}

fn archived_event(access: LeaseAccess, job_id: JobId, bytes: &[u8]) -> Result<AppendJobEvents, StoreError> {
  let event = DurableJobEvent::new(
    EventSequence::new(1).unwrap(),
    JobEventKind::new("stdout").unwrap(),
    time(1_200),
    json!({"source": "runner", "redacted": true}),
  )
  .unwrap();
  let chunk = LogChunkManifest::prepare(job_id, BuildLogStream::Stdout, 1, 1, bytes).unwrap();
  AppendJobEvents::new(access, vec![event], time(1_500))?.with_log_chunks(vec![chunk])
}

fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}

async fn independent_pool(source: &sqlx::PgPool) -> sqlx::PgPool {
  sqlx::PgPool::connect_with((*source.connect_options()).clone())
    .await
    .expect("connect independent test pool")
}
