#[path = "support/authoritative_fixture.rs"]
mod authoritative_fixture;
mod support;

use authoritative_fixture::seed_authoritative_prerequisites;
use octacity_server_domain::{ArtifactId, ArtifactUploadId, LogIndexingWorkId, Timestamp};
use octacity_server_store::{
  BuildLogStream, BuildResultComponent, BuildRetentionStore as _, ClaimRetentionWork, CompleteRetentionObject,
  CompleteRetentionSearch, FinishRetentionPass, JobClaim, JobClaimOutcome, JobEventReadStore as _,
  JobExecutionStore as _, LeaseFence, LeaseGrant, LeaseWindow, LogChunkManifest, LogSearchDocument,
  LogSearchIndex as _, LogSearchMutationDisposition, PrepareRetentionWork, ReadJobEvents, RetentionObject,
  RetentionObjectIdentity, RetentionPassOutcome, TriggerAcceptanceStore as _, WorkerOwner, WriteLogSearchDocument,
  testing::{authoritative_store_contract_fixture, compatible_snapshot},
};
use octacity_server_store_postgres::{PostgresAuthoritativeStore, PostgresLogSearchIndex, PostgresStore};
use support::TestDatabase;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn retention_deadlines_and_interrupted_cleanup_are_durable() {
  let database = TestDatabase::migrated().await;
  let mut fixture = authoritative_store_contract_fixture();
  fixture.request.build.retention = octacity_server_store::BuildRetentionDeadlines {
    metadata: time(1_500),
    logs: time(1_500),
    artifacts: time(1_500),
    reports: time(1_500),
  };
  seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  let execution = PostgresAuthoritativeStore::new(database.pool.clone(), support::test_signer());
  execution.accept_trigger(fixture.request.clone()).await.unwrap();
  let grant = match execution
    .claim_ready_job(
      JobClaim::new(
        id(100),
        LeaseFence::from_bytes([9; 32]),
        fixture.agent_id,
        fixture.registration_epoch,
        fixture.allowed_pool,
        compatible_snapshot(),
        LeaseWindow::new(time(600), time(10_000)).unwrap(),
      )
      .unwrap(),
    )
    .await
    .unwrap()
  {
    JobClaimOutcome::Claimed(grant) => *grant,
    JobClaimOutcome::Empty => panic!("fixture root Job must be claimable"),
  };
  let manifest = LogChunkManifest::prepare(grant.job_id, BuildLogStream::Stdout, 1, 1, b"retained log").unwrap();
  insert_log_manifest(&database.pool, &fixture, &manifest).await;
  insert_output_event(&database.pool, &grant).await;
  insert_output(&database.pool, &fixture, &grant, false, 110).await;
  insert_output(&database.pool, &fixture, &grant, true, 120).await;

  let store = PostgresStore::new(database.pool.clone());
  let first_owner = WorkerOwner::new("retention:first").unwrap();
  assert!(
    store
      .claim_retention_work(ClaimRetentionWork::new(first_owner.clone(), time(1_499), time(2_000), 4).unwrap())
      .await
      .unwrap()
      .is_empty()
  );
  let claims = store
    .claim_retention_work(ClaimRetentionWork::new(first_owner, time(1_500), time(2_000), 4).unwrap())
    .await
    .unwrap();
  assert_eq!(claims.len(), 4, "the exact deadline is eligible");
  assert_eq!(output_usage(&database.pool, fixture.request.build.id).await, (3, 5));

  let log_claim = claims
    .iter()
    .find(|claim| claim.component == BuildResultComponent::Logs)
    .unwrap();
  let preparation = store
    .prepare_retention_work(
      PrepareRetentionWork::new(log_claim.work_id, log_claim.owner.clone(), time(1_600), 1).unwrap(),
    )
    .await
    .unwrap();
  assert!(
    preparation.objects.is_empty(),
    "log bytes wait for the search tombstone"
  );
  let deletion = preparation.search_deletion.unwrap();
  let hidden_events = store
    .read_job_events(ReadJobEvents::new(grant.job_id, 0, 10).unwrap())
    .await
    .unwrap();
  assert!(hidden_events.events.is_empty());
  assert_eq!(
    hidden_events.cursor, 1,
    "hidden output still advances the durable cursor"
  );
  let index = PostgresLogSearchIndex::new(database.pool.clone());
  assert_eq!(
    index.delete(deletion).await.unwrap(),
    LogSearchMutationDisposition::Applied
  );
  store
    .complete_retention_search(CompleteRetentionSearch {
      work_id: log_claim.work_id,
      owner: log_claim.owner.clone(),
      completed_at: time(1_700),
    })
    .await
    .unwrap();

  let second_owner = WorkerOwner::new("retention:second").unwrap();
  assert!(
    store
      .claim_retention_work(ClaimRetentionWork::new(second_owner.clone(), time(1_999), time(3_000), 4).unwrap())
      .await
      .unwrap()
      .is_empty(),
    "a live claim cannot be stolen"
  );
  let reclaimed = store
    .claim_retention_work(ClaimRetentionWork::new(second_owner, time(2_000), time(3_000), 4).unwrap())
    .await
    .unwrap();
  let log_claim = reclaimed
    .iter()
    .find(|claim| claim.component == BuildResultComponent::Logs)
    .unwrap();
  let preparation = store
    .prepare_retention_work(
      PrepareRetentionWork::new(log_claim.work_id, log_claim.owner.clone(), time(2_100), 1).unwrap(),
    )
    .await
    .unwrap();
  let RetentionObject::Log(queued) = &preparation.objects[0] else {
    panic!("log retention returned a non-log object");
  };
  assert_eq!(queued.chunk_id(), manifest.chunk_id());
  let completion = CompleteRetentionObject {
    work_id: log_claim.work_id,
    owner: log_claim.owner.clone(),
    object: RetentionObjectIdentity::Log(manifest.chunk_id()),
    completed_at: time(2_200),
  };
  store.complete_retention_object(completion.clone()).await.unwrap();
  store.complete_retention_object(completion).await.unwrap();
  assert_eq!(
    store
      .finish_retention_pass(FinishRetentionPass {
        work_id: log_claim.work_id,
        owner: log_claim.owner.clone(),
        finished_at: time(2_300),
      })
      .await
      .unwrap(),
    RetentionPassOutcome::Completed
  );

  for component in [BuildResultComponent::Artifacts, BuildResultComponent::Reports] {
    let claim = reclaimed.iter().find(|claim| claim.component == component).unwrap();
    let preparation = store
      .prepare_retention_work(PrepareRetentionWork::new(claim.work_id, claim.owner.clone(), time(2_400), 1).unwrap())
      .await
      .unwrap();
    assert_eq!(preparation.objects.len(), 1);
  }
  assert_eq!(
    output_usage(&database.pool, fixture.request.build.id).await,
    (3, 5),
    "hidden output bytes remain quota-charged until physical deletion completes"
  );
  let visibility: (bool, bool, bool, bool) = sqlx::query_as(
    "SELECT metadata_visible, logs_visible, artifacts_visible, reports_visible FROM builds WHERE id = $1",
  )
  .bind(fixture.request.build.id.as_uuid())
  .fetch_one(&database.pool)
  .await
  .unwrap();
  assert_eq!(visibility, (true, false, false, false));

  index.start_rebuild(fixture.request.build.project_id).await.unwrap();
  let disposition = index
    .rebuild(WriteLogSearchDocument {
      work_id: LogIndexingWorkId::from_uuid(uuid::Uuid::from_u128(999)).unwrap(),
      position: octacity_server_store::LogIndexPosition::new(2).unwrap(),
      document: LogSearchDocument {
        chunk_id: manifest.chunk_id(),
        project_id: fixture.request.build.project_id,
        build_id: fixture.request.build.id,
        attempt_id: fixture.request.attempt_id,
        job_id: grant.job_id,
        stream: BuildLogStream::Stdout,
        first_sequence: 1,
        last_sequence: 1,
        occurred_at: time(1_000),
        redacted_text: "retained log".to_owned(),
      },
    })
    .await
    .unwrap();
  assert_eq!(disposition, LogSearchMutationDisposition::Superseded);
  database.cleanup().await;
}

async fn insert_output_event(pool: &sqlx::PgPool, grant: &LeaseGrant) {
  sqlx::query(
    "INSERT INTO job_events \
       (job_id, sequence, lease_id, event_kind, event_time, payload, event_digest, created_at) \
     VALUES ($1, 1, $2, 'stdout', to_timestamp(1), '{\"redacted\":true}'::jsonb, $3, to_timestamp(1))",
  )
  .bind(grant.job_id.as_uuid())
  .bind(grant.lease_id.as_uuid())
  .bind(vec![7_u8; 32])
  .execute(pool)
  .await
  .unwrap();
}

async fn output_usage(pool: &sqlx::PgPool, build_id: octacity_server_domain::BuildId) -> (i64, i64) {
  sqlx::query_as(
    "SELECT COALESCE(SUM(byte_length) FILTER (WHERE artifact_type = 'artifact' AND state <> 'deleted'), 0)::BIGINT, \
       COALESCE(SUM(byte_length) FILTER (WHERE artifact_type = 'report' AND state <> 'deleted'), 0)::BIGINT \
     FROM artifacts WHERE build_id = $1",
  )
  .bind(build_id.as_uuid())
  .fetch_one(pool)
  .await
  .unwrap()
}

async fn insert_log_manifest(
  pool: &sqlx::PgPool,
  fixture: &octacity_server_store::testing::StoreContractFixture,
  manifest: &LogChunkManifest,
) {
  sqlx::query(
    "INSERT INTO log_chunk_manifests \
       (id, build_id, attempt_id, job_id, stream, first_sequence, last_sequence, object_identity, byte_length, \
        sha256, visible, created_at) \
     VALUES ($1, $2, $3, $4, 'stdout', 1, 1, $5, $6, $7, true, to_timestamp(1))",
  )
  .bind(manifest.chunk_id().as_uuid())
  .bind(fixture.request.build.id.as_uuid())
  .bind(fixture.request.attempt_id.as_uuid())
  .bind(fixture.request.jobs[0].id.as_uuid())
  .bind(manifest.object_identity())
  .bind(i64::try_from(manifest.byte_length()).unwrap())
  .bind(manifest.digest().as_bytes().to_vec())
  .execute(pool)
  .await
  .unwrap();
}

async fn insert_output(
  pool: &sqlx::PgPool,
  fixture: &octacity_server_store::testing::StoreContractFixture,
  grant: &LeaseGrant,
  report: bool,
  value: u128,
) {
  let artifact_id = ArtifactId::from_uuid(uuid::Uuid::from_u128(value)).unwrap();
  let upload_id = ArtifactUploadId::from_uuid(uuid::Uuid::from_u128(value + 1)).unwrap();
  let (artifact_type, report_format, logical_name, size) = if report {
    ("report", Some("junit"), "report.xml", 5_i64)
  } else {
    ("artifact", None, "artifact.bin", 3_i64)
  };
  sqlx::query(
    "INSERT INTO artifacts \
       (id, build_id, attempt_id, job_id, lease_id, logical_name, artifact_type, media_type, report_format, \
        byte_length, sha256, object_identity, object_generation, state, retention_until, version, created_at, \
        published_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, 'application/octet-stream', $8, $9, $10, 'object', 'generation', \
             'published', to_timestamp(1.5), 1, to_timestamp(1), to_timestamp(1))",
  )
  .bind(artifact_id.as_uuid())
  .bind(fixture.request.build.id.as_uuid())
  .bind(fixture.request.attempt_id.as_uuid())
  .bind(grant.job_id.as_uuid())
  .bind(grant.lease_id.as_uuid())
  .bind(logical_name)
  .bind(artifact_type)
  .bind(report_format)
  .bind(size)
  .bind(vec![value as u8; 32])
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO artifact_uploads \
       (id, artifact_id, job_id, lease_id, idempotency_key, logical_name, media_type, report_format, expected_size, \
        expected_sha256, object_identity, object_generation, state, created_at, expires_at, completed_at, \
        artifact_type, transport_media_type, producer_run_id, producer_task_id) \
     VALUES ($1, $2, $3, $4, $5, $6, 'application/octet-stream', $7, $8, $9, 'object', 'generation', 'published', \
             to_timestamp(1), to_timestamp(10), to_timestamp(1), $10, 'application/octet-stream', '1', '1')",
  )
  .bind(upload_id.as_uuid())
  .bind(artifact_id.as_uuid())
  .bind(grant.job_id.as_uuid())
  .bind(grant.lease_id.as_uuid())
  .bind(format!("retention-{value}"))
  .bind(logical_name)
  .bind(report_format)
  .bind(size)
  .bind(vec![value as u8; 32])
  .bind(artifact_type)
  .execute(pool)
  .await
  .unwrap();
}

fn id(value: u128) -> octacity_server_domain::LeaseId {
  octacity_server_domain::LeaseId::from_uuid(uuid::Uuid::from_u128(value)).unwrap()
}

fn time(milliseconds: i64) -> Timestamp {
  Timestamp::from_unix_millis(milliseconds).unwrap()
}
