#[path = "support/authoritative_fixture.rs"]
mod authoritative_fixture;
mod support;

use authoritative_fixture::seed_authoritative_prerequisites;
use octacity_server_domain::{
  ArtifactId, ArtifactUploadId, EntityKind, LogIndexingWorkId, RetentionHoldVersion, Timestamp,
};
use octacity_server_store::{
  BuildLogStream, BuildResultComponent, BuildResultRetentionHoldStore as _, BuildRetentionDeadlines,
  BuildRetentionStore as _, ClaimRetentionWork, CompleteRetentionObject, CompleteRetentionSearch, FinishRetentionPass,
  GetBuildResultRetention, IdempotencyKey, JobClaim, JobClaimOutcome, JobEventReadStore as _, JobExecutionStore as _,
  LeaseFence, LeaseGrant, LeaseWindow, LogChunkManifest, LogSearchDocument, LogSearchIndex as _,
  LogSearchMutationDisposition, MutationDisposition, PlaceBuildResultHold, PrepareRetentionWork, ReadJobEvents,
  ReleaseBuildResultHold, ReleaseBuildResultHoldError, RetentionHoldReason, RetentionHoldState, RetentionObject,
  RetentionObjectIdentity, RetentionPassOutcome, RetentionRequestIdentity, StoreError, TriggerAcceptanceStore as _,
  WorkerOwner, WriteLogSearchDocument,
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

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn permanent_hold_replays_and_overdue_release_preserves_deadlines_and_quota() {
  let database = TestDatabase::migrated().await;
  let mut fixture = authoritative_store_contract_fixture();
  fixture.request.build.retention = deadlines(1_500);
  seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  let execution = PostgresAuthoritativeStore::new(database.pool.clone(), support::test_signer());
  execution.accept_trigger(fixture.request.clone()).await.unwrap();
  let grant = claim_root(&execution, &fixture).await;
  insert_output_event(&database.pool, &grant).await;
  insert_output(&database.pool, &fixture, &grant, false, 210).await;
  insert_output(&database.pool, &fixture, &grant, true, 220).await;

  let store = PostgresStore::new(database.pool.clone());
  let placed = place_request(
    fixture.request.build.id,
    "incident investigation",
    None,
    "hold-permanent",
    time(1_400),
  );
  let applied = store.place_build_result_hold(placed.clone()).await.unwrap();
  assert_eq!(applied.disposition, MutationDisposition::Applied);
  assert_eq!(
    applied.retention.hold.as_ref().unwrap().version,
    RetentionHoldVersion::INITIAL
  );
  assert_eq!(output_usage(&database.pool, fixture.request.build.id).await, (3, 5));
  assert!(
    store
      .claim_retention_work(
        ClaimRetentionWork::new(WorkerOwner::new("retention:held").unwrap(), time(1_500), time(2_500), 4,).unwrap()
      )
      .await
      .unwrap()
      .is_empty(),
    "a permanent hold protects every due component"
  );

  let mut replay = placed;
  replay.placed_at = time(1_900);
  replay.request_identity = RetentionRequestIdentity::new("request:replay").unwrap();
  let replayed = store.place_build_result_hold(replay).await.unwrap();
  assert_eq!(replayed.disposition, MutationDisposition::Replayed);
  assert_eq!(replayed.retention, applied.retention);

  assert!(matches!(
    store
      .release_build_result_hold(ReleaseBuildResultHold {
        build_id: fixture.request.build.id,
        expected_version: RetentionHoldVersion::INITIAL,
        actor_identity: None,
        request_identity: RetentionRequestIdentity::new("request:clock-regression").unwrap(),
        idempotency_key: key("release-before-creation"),
        released_at: time(1_399),
      })
      .await,
    Err(ReleaseBuildResultHoldError::Store(StoreError::Conflict {
      entity: EntityKind::RetentionHold
    }))
  ));

  let release = ReleaseBuildResultHold {
    build_id: fixture.request.build.id,
    expected_version: RetentionHoldVersion::INITIAL,
    actor_identity: None,
    request_identity: RetentionRequestIdentity::new("request:release").unwrap(),
    idempotency_key: key("release-permanent"),
    released_at: time(2_000),
  };
  let released = store.release_build_result_hold(release.clone()).await.unwrap();
  assert_eq!(released.disposition, MutationDisposition::Applied);
  assert_eq!(released.retention.deadlines, deadlines(1_500));
  assert_eq!(
    released.retention.hold.as_ref().unwrap().state,
    RetentionHoldState::Released
  );
  assert_eq!(released.retention.hold.as_ref().unwrap().version.get(), 2);
  assert_eq!(
    released
      .retention
      .hold
      .as_ref()
      .unwrap()
      .release_audit
      .as_ref()
      .unwrap()
      .request_identity,
    "request:release"
  );
  let release_replay = store.release_build_result_hold(release).await.unwrap();
  assert_eq!(release_replay.disposition, MutationDisposition::Replayed);
  assert_eq!(release_replay.retention, released.retention);
  assert!(matches!(
    store
      .release_build_result_hold(ReleaseBuildResultHold {
        build_id: fixture.request.build.id,
        expected_version: RetentionHoldVersion::INITIAL,
        actor_identity: None,
        request_identity: RetentionRequestIdentity::new("request:stale").unwrap(),
        idempotency_key: key("release-stale"),
        released_at: time(2_001),
      })
      .await,
    Err(ReleaseBuildResultHoldError::PreconditionFailed)
  ));
  assert!(matches!(
    store
      .place_build_result_hold(place_request(
        fixture.request.build.id,
        "clock moved backwards",
        None,
        "place-before-release",
        time(1_999),
      ))
      .await,
    Err(StoreError::Conflict {
      entity: EntityKind::RetentionHold
    })
  ));
  let claims = store
    .claim_retention_work(
      ClaimRetentionWork::new(
        WorkerOwner::new("retention:released").unwrap(),
        time(2_000),
        time(3_000),
        4,
      )
      .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(claims.len(), 4, "release does not grant a new retention period");
  assert_eq!(output_usage(&database.pool, fixture.request.build.id).await, (3, 5));
  let audit_count: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM audit_facts WHERE target_kind = 'retention_hold' AND target_identity = $1",
  )
  .bind(fixture.request.build.id.to_string())
  .fetch_one(&database.pool)
  .await
  .unwrap();
  assert_eq!(audit_count, 2, "exact replays do not duplicate audit facts");
  let audit_identities: Vec<(String, String)> = sqlx::query_as(
    "SELECT actor_kind, request_identity FROM audit_facts \
     WHERE target_kind = 'retention_hold' AND target_identity = $1 ORDER BY occurred_at, operation",
  )
  .bind(fixture.request.build.id.to_string())
  .fetch_all(&database.pool)
  .await
  .unwrap();
  assert_eq!(
    audit_identities,
    vec![
      (
        "unauthenticated_management".to_owned(),
        "request:hold-permanent".to_owned(),
      ),
      ("unauthenticated_management".to_owned(), "request:release".to_owned(),),
    ]
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn time_bounded_hold_expires_at_the_boundary_and_cannot_revive_hidden_data() {
  let database = TestDatabase::migrated().await;
  let mut fixture = authoritative_store_contract_fixture();
  fixture.request.build.retention = deadlines(1_500);
  seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  let execution = PostgresAuthoritativeStore::new(database.pool.clone(), support::test_signer());
  execution.accept_trigger(fixture.request.clone()).await.unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let placement = place_request(
    fixture.request.build.id,
    "temporary investigation",
    Some(time(1_600)),
    "hold-temporary",
    time(1_400),
  );
  let applied = store.place_build_result_hold(placement.clone()).await.unwrap();
  let mut replay = placement;
  replay.placed_at = time(1_700);
  replay.request_identity = RetentionRequestIdentity::new("request:timed-replay").unwrap();
  let replayed = store.place_build_result_hold(replay).await.unwrap();
  assert_eq!(replayed.disposition, MutationDisposition::Replayed);
  assert_eq!(replayed.retention, applied.retention);
  assert!(
    store
      .claim_retention_work(
        ClaimRetentionWork::new(
          WorkerOwner::new("retention:before-expiry").unwrap(),
          time(1_599),
          time(2_000),
          4,
        )
        .unwrap()
      )
      .await
      .unwrap()
      .is_empty()
  );
  let claims = store
    .claim_retention_work(
      ClaimRetentionWork::new(
        WorkerOwner::new("retention:at-expiry").unwrap(),
        time(1_600),
        time(2_100),
        4,
      )
      .unwrap(),
    )
    .await
    .unwrap();
  let metadata = claims
    .iter()
    .find(|claim| claim.component == BuildResultComponent::Metadata)
    .unwrap();
  store
    .prepare_retention_work(
      PrepareRetentionWork::new(metadata.work_id, metadata.owner.clone(), time(1_601), 1).unwrap(),
    )
    .await
    .unwrap();
  let logs = claims
    .iter()
    .find(|claim| claim.component == BuildResultComponent::Logs)
    .unwrap();
  store
    .prepare_retention_work(PrepareRetentionWork::new(logs.work_id, logs.owner.clone(), time(1_599), 1).unwrap())
    .await
    .unwrap();
  let state = store
    .build_result_retention(GetBuildResultRetention {
      build_id: fixture.request.build.id,
      observed_at: time(1_599),
    })
    .await
    .unwrap();
  assert!(!state.visibility.metadata);
  assert!(!state.visibility.logs);
  assert_eq!(state.hold.as_ref().unwrap().state, RetentionHoldState::Expired);
  assert_eq!(state.hold.as_ref().unwrap().version.get(), 2);
  assert!(matches!(
    store
      .place_build_result_hold(place_request(
        fixture.request.build.id,
        "too late",
        None,
        "hold-after-delete",
        time(1_602),
      ))
      .await,
    Err(StoreError::Conflict {
      entity: EntityKind::RetentionHold
    })
  ));
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn hold_and_first_visibility_transition_serialize_as_one_decision() {
  let database = TestDatabase::migrated().await;
  let mut fixture = authoritative_store_contract_fixture();
  fixture.request.build.retention = deadlines(1_500);
  seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  let execution = PostgresAuthoritativeStore::new(database.pool.clone(), support::test_signer());
  execution.accept_trigger(fixture.request.clone()).await.unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let claims = store
    .claim_retention_work(
      ClaimRetentionWork::new(WorkerOwner::new("retention:race").unwrap(), time(1_500), time(3_000), 4).unwrap(),
    )
    .await
    .unwrap();
  let metadata = claims
    .into_iter()
    .find(|claim| claim.component == BuildResultComponent::Metadata)
    .unwrap();
  let hold_store = store.clone();
  let retention_store = store.clone();
  let build_id = fixture.request.build.id;
  let (hold, preparation) = tokio::join!(
    hold_store.place_build_result_hold(place_request(
      build_id,
      "race protection",
      None,
      "hold-race",
      time(1_501),
    )),
    retention_store
      .prepare_retention_work(PrepareRetentionWork::new(metadata.work_id, metadata.owner, time(1_501), 1).unwrap(),)
  );
  preparation.unwrap();
  let state = store
    .build_result_retention(GetBuildResultRetention {
      build_id,
      observed_at: time(1_502),
    })
    .await
    .unwrap();
  match hold {
    Ok(_) => {
      assert!(state.visibility.complete());
      assert_eq!(state.hold.unwrap().state, RetentionHoldState::Active);
    }
    Err(StoreError::Conflict {
      entity: EntityKind::RetentionHold,
    }) => {
      assert!(!state.visibility.metadata);
      assert!(state.hold.is_none());
    }
    other => panic!("unexpected hold race outcome: {other:?}"),
  }
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

async fn claim_root(
  execution: &PostgresAuthoritativeStore,
  fixture: &octacity_server_store::testing::StoreContractFixture,
) -> LeaseGrant {
  match execution
    .claim_ready_job(
      JobClaim::new(
        id(200),
        LeaseFence::from_bytes([8; 32]),
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
  }
}

fn place_request(
  build_id: octacity_server_domain::BuildId,
  reason: &str,
  expires_at: Option<Timestamp>,
  idempotency_key: &str,
  placed_at: Timestamp,
) -> PlaceBuildResultHold {
  PlaceBuildResultHold {
    build_id,
    reason: RetentionHoldReason::new(reason).unwrap(),
    expires_at,
    actor_identity: None,
    request_identity: RetentionRequestIdentity::new(format!("request:{idempotency_key}")).unwrap(),
    idempotency_key: key(idempotency_key),
    placed_at,
  }
}

fn deadlines(milliseconds: i64) -> BuildRetentionDeadlines {
  BuildRetentionDeadlines {
    metadata: time(milliseconds),
    logs: time(milliseconds),
    artifacts: time(milliseconds),
    reports: time(milliseconds),
  }
}

fn key(value: &str) -> IdempotencyKey {
  IdempotencyKey::new(value).unwrap()
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
