#[path = "support/authoritative_fixture.rs"]
mod authoritative_fixture;
mod support;

use authoritative_fixture::seed_authoritative_prerequisites;
use std::time::Duration;

use octacity_server_domain::Timestamp;
use octacity_server_factory::{FactoryClaim, FactoryClaimFence, FactoryDigest, FactoryKey, FactoryRunId};
use octacity_server_store::{
  AdvanceFactoryRetentionWork, ClaimFactoryRetentionWork, FactoryOutboxRecord, FactoryRetentionStore as _,
  FailFactoryRetentionWork, RestoreInventoryStore as _, TriggerAcceptanceStore as _, WorkerOwner,
  testing::authoritative_store_contract_fixture,
};
use octacity_server_store_postgres::{PostgresAuthoritativeStore, PostgresStore};
use sqlx::{Executor as _, types::Json};
use support::TestDatabase;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn factory_retention_preserves_live_runs_and_retries_bounded_cleanup() {
  let database = TestDatabase::migrated().await;
  seed_factory_runs(&database.pool).await;
  let store = PostgresStore::new(database.pool.clone());
  let owner = WorkerOwner::new("factory-retention:test").unwrap();

  let claims = store
    .claim_factory_retention_work(ClaimFactoryRetentionWork::new(owner.clone(), time(20), time(30), 8).unwrap())
    .await
    .unwrap();
  assert_eq!(
    claims.iter().map(|claim| claim.run_id).collect::<Vec<_>>(),
    vec![run_id(3)],
    "active and escalated Runs retain their exact state even if malformed work is present"
  );

  let first = store
    .advance_factory_retention_work(AdvanceFactoryRetentionWork::new(run_id(3), owner.clone(), time(21), 2).unwrap())
    .await
    .unwrap();
  assert_eq!(first.released_references, 2);
  assert!(!first.completed);
  assert_eq!(run_visibility(&database.pool, run_id(3)).await, (false, 1));

  let reclaimed = store
    .claim_factory_retention_work(ClaimFactoryRetentionWork::new(owner.clone(), time(22), time(32), 8).unwrap())
    .await
    .unwrap();
  assert_eq!(reclaimed.len(), 1);
  assert_eq!(reclaimed[0].run_id, run_id(3));
  let second = store
    .advance_factory_retention_work(AdvanceFactoryRetentionWork::new(run_id(3), owner, time(23), 2).unwrap())
    .await
    .unwrap();
  assert_eq!(second.released_references, 1);
  assert_eq!(second.deleted_records, 2);
  assert!(!second.completed);
  assert_eq!(run_visibility(&database.pool, run_id(3)).await, (false, 0));

  let owner = WorkerOwner::new("factory-retention:metadata").unwrap();
  let mut completed = false;
  for observed_at in 24..32 {
    let claims = store
      .claim_factory_retention_work(
        ClaimFactoryRetentionWork::new(owner.clone(), time(observed_at), time(observed_at + 10), 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(claims.len(), 1);
    let pass = store
      .advance_factory_retention_work(
        AdvanceFactoryRetentionWork::new(run_id(3), owner.clone(), time(observed_at), 2).unwrap(),
      )
      .await
      .unwrap();
    if pass.completed {
      completed = true;
      break;
    }
  }
  assert!(completed, "bounded metadata cleanup must eventually complete");

  let recovery = store.recover_restored_factory_state(time(40), 1).await.unwrap();
  assert_eq!(recovery.expired_claims, 1);
  assert_eq!(recovery.expired_retention_claims, 1);
  assert_eq!(recovery.requeued_outbox, 1);
  assert!(!recovery.has_more);
  let (claim_id, latest_outbox): (Option<Vec<u8>>, String) = sqlx::query_as(
    "SELECT current.claim_id, latest.state FROM factory_run_current AS current \
     JOIN LATERAL (SELECT state FROM factory_outbox_records WHERE run_id = current.run_id \
       ORDER BY attempt DESC, CASE state WHEN 'pending' THEN 0 WHEN 'claimed' THEN 1 ELSE 2 END DESC, \
         recorded_at DESC, id DESC LIMIT 1) AS latest ON true WHERE current.run_id = $1",
  )
  .bind(run_id(1).as_uuid())
  .fetch_one(&database.pool)
  .await
  .unwrap();
  assert!(claim_id.is_none());
  assert_eq!(latest_outbox, "pending");

  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn active_build_hold_preserves_the_terminal_factory_run() {
  let database = TestDatabase::migrated().await;
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  PostgresAuthoritativeStore::new(database.pool.clone(), support::test_signer())
    .accept_trigger(fixture.request.clone())
    .await
    .unwrap();
  seed_factory_runs(&database.pool).await;
  link_factory_build_and_hold(&database.pool, &fixture).await;
  let store = PostgresStore::new(database.pool.clone());
  let owner = WorkerOwner::new("factory-retention:hold").unwrap();

  assert!(
    store
      .claim_factory_retention_work(ClaimFactoryRetentionWork::new(owner.clone(), time(20), time(30), 8).unwrap())
      .await
      .unwrap()
      .is_empty(),
    "an active ordinary Build Result hold must retain its terminal Factory Run"
  );
  sqlx::query(
    "UPDATE build_result_retention_holds SET released_at = to_timestamp(0.021), \
       release_actor_kind = 'system', release_request_identity = 'release' WHERE build_id = $1",
  )
  .bind(fixture.request.build.id.as_uuid())
  .execute(&database.pool)
  .await
  .unwrap();
  let claims = store
    .claim_factory_retention_work(ClaimFactoryRetentionWork::new(owner, time(22), time(32), 8).unwrap())
    .await
    .unwrap();
  assert_eq!(claims.len(), 1);
  assert_eq!(claims[0].run_id, run_id(3));

  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn a_concurrent_build_hold_wins_before_factory_retention_hides_the_run() {
  let database = TestDatabase::migrated().await;
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  PostgresAuthoritativeStore::new(database.pool.clone(), support::test_signer())
    .accept_trigger(fixture.request.clone())
    .await
    .unwrap();
  seed_factory_runs(&database.pool).await;
  link_factory_build(&database.pool, &fixture).await;

  let owner = WorkerOwner::new("factory-retention:hold-race").unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let claims = store
    .claim_factory_retention_work(ClaimFactoryRetentionWork::new(owner.clone(), time(20), time(30), 8).unwrap())
    .await
    .unwrap();
  assert_eq!(claims.len(), 1);

  let mut hold_transaction = database.pool.begin().await.unwrap();
  sqlx::query("SELECT id FROM builds WHERE id = $1 FOR UPDATE")
    .bind(fixture.request.build.id.as_uuid())
    .execute(&mut *hold_transaction)
    .await
    .unwrap();

  let pool = database.pool.clone();
  let advancing = tokio::spawn(async move {
    PostgresStore::new(pool)
      .advance_factory_retention_work(AdvanceFactoryRetentionWork::new(run_id(3), owner, time(21), 2).unwrap())
      .await
  });
  let mut advancing = Box::pin(advancing);
  assert!(
    tokio::time::timeout(Duration::from_millis(100), &mut advancing)
      .await
      .is_err(),
    "retention must wait for an in-flight Build hold decision"
  );

  insert_build_hold(&mut hold_transaction, &fixture).await;
  hold_transaction.commit().await.unwrap();
  assert!(matches!(
    advancing.await.unwrap().unwrap_err(),
    octacity_server_store::StoreError::Conflict { .. }
  ));
  assert_eq!(run_visibility(&database.pool, run_id(3)).await, (true, 3));

  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn factory_retention_failure_is_exactly_replay_safe() {
  let database = TestDatabase::migrated().await;
  seed_factory_runs(&database.pool).await;
  let store = PostgresStore::new(database.pool.clone());
  let owner = WorkerOwner::new("factory-retention:failure").unwrap();
  let claims = store
    .claim_factory_retention_work(ClaimFactoryRetentionWork::new(owner.clone(), time(20), time(30), 1).unwrap())
    .await
    .unwrap();
  assert_eq!(claims.len(), 1);
  let failure = FailFactoryRetentionWork {
    run_id: claims[0].run_id,
    owner,
    failed_at: time(21),
    error_code: "object-store-unavailable".to_owned(),
    retry_at: Some(time(40)),
  };
  store.fail_factory_retention_work(failure.clone()).await.unwrap();
  store.fail_factory_retention_work(failure.clone()).await.unwrap();
  let mut conflicting = failure;
  conflicting.error_code = "different-classification".to_owned();
  assert!(matches!(
    store.fail_factory_retention_work(conflicting).await.unwrap_err(),
    octacity_server_store::StoreError::Conflict { .. }
  ));

  database.cleanup().await;
}

async fn link_factory_build_and_hold(
  pool: &sqlx::PgPool,
  fixture: &octacity_server_store::testing::StoreContractFixture,
) {
  link_factory_build(pool, fixture).await;
  let mut transaction = pool.begin().await.unwrap();
  insert_build_hold(&mut transaction, fixture).await;
  transaction.commit().await.unwrap();
}

async fn link_factory_build(pool: &sqlx::PgPool, fixture: &octacity_server_store::testing::StoreContractFixture) {
  let stage_id = uuid(3_000);
  sqlx::query(
    "INSERT INTO factory_stage_attempts \
       (id, run_id, attempt_number, stage_kind, target_digest, input_digest, stage_attempt, created_at) \
     VALUES ($1, $2, 1, 'implementation', decode(repeat('31', 32), 'hex'), \
       decode(repeat('32', 32), 'hex'), '{}', to_timestamp(0))",
  )
  .bind(stage_id)
  .bind(run_id(3).as_uuid())
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO factory_build_links \
       (build_id, run_id, stage_attempt_id, attempt_id, factory_configuration_id, \
        factory_configuration_version, build_configuration_id, build_configuration_version, \
        task_envelope_digest, effective_policy_digest, input_digest, exact_revision, link, linked_at) \
     VALUES ($1, $2, $3, $4, '00000000-0000-0000-0000-000000000104', 1, $5, $6, \
       decode(repeat('33', 32), 'hex'), decode(repeat('34', 32), 'hex'), \
       decode(repeat('35', 32), 'hex'), $7, '{}', to_timestamp(0))",
  )
  .bind(fixture.request.build.id.as_uuid())
  .bind(run_id(3).as_uuid())
  .bind(stage_id)
  .bind(fixture.request.attempt_id.as_uuid())
  .bind(fixture.request.build.configuration_id.as_uuid())
  .bind(i64::try_from(fixture.request.build.configuration_version.get()).unwrap())
  .bind(fixture.request.build.immutable_revision.as_str())
  .execute(pool)
  .await
  .unwrap();
}

async fn insert_build_hold(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  fixture: &octacity_server_store::testing::StoreContractFixture,
) {
  sqlx::query(
    "INSERT INTO build_result_retention_holds \
       (build_id, version, reason, created_at, actor_kind, request_identity) \
     VALUES ($1, 1, 'Factory investigation', to_timestamp(0.001), 'system', 'hold')",
  )
  .bind(fixture.request.build.id.as_uuid())
  .execute(&mut **transaction)
  .await
  .unwrap();
}

async fn seed_factory_runs(pool: &sqlx::PgPool) {
  let mut transaction = pool.begin().await.unwrap();
  transaction
    .execute(
      "INSERT INTO projects (id, parent_id, name, version, created_at, updated_at) \
       VALUES ('00000000-0000-0000-0000-000000000101', NULL, 'factory-retention', 1, now(), now())",
    )
    .await
    .unwrap();
  transaction
    .execute(
      "INSERT INTO repositories (id, project_id, name, created_at) \
       VALUES ('00000000-0000-0000-0000-000000000102', \
               '00000000-0000-0000-0000-000000000101', 'repository', now()); \
       INSERT INTO repository_versions \
         (repository_id, version, vcs_integration_id, repository_locator, selection_policy, published_at) \
       VALUES ('00000000-0000-0000-0000-000000000102', 1, \
               '00000000-0000-0000-0000-000000000103', 'https://example.invalid/repository', '{}', now())",
    )
    .await
    .unwrap();
  transaction
    .execute(
      "INSERT INTO factory_configurations (id, project_id, current_version, created_at, updated_at) \
       VALUES ('00000000-0000-0000-0000-000000000104', \
               '00000000-0000-0000-0000-000000000101', 1, now(), now()); \
       INSERT INTO factory_configuration_versions \
         (factory_configuration_id, version, definition_digest, definition, enabled, published_at) \
       VALUES ('00000000-0000-0000-0000-000000000104', 1, decode(repeat('01', 32), 'hex'), '{}', true, now())",
    )
    .await
    .unwrap();

  for (ordinal, state) in [(1_u8, "implementing"), (2, "escalated"), (3, "completed")] {
    let work_id = uuid(200 + u128::from(ordinal));
    let run_id = uuid(u128::from(ordinal));
    let external_identity = format!("work-{ordinal}");
    sqlx::query(
      "INSERT INTO factory_work_envelopes \
         (id, project_id, factory_configuration_id, factory_configuration_version, source_kind, \
          security_scope_digest, external_identity, repository_id, repository_version, exact_revision, \
          intent_digest, envelope_digest, envelope, admitted_at) \
       VALUES ($1, '00000000-0000-0000-0000-000000000101', \
         '00000000-0000-0000-0000-000000000104', 1, 'test', decode(repeat('02', 32), 'hex'), $2, \
         '00000000-0000-0000-0000-000000000102', 1, 'revision', decode(repeat($3, 32), 'hex'), \
         decode(repeat($4, 32), 'hex'), $5, to_timestamp(0))",
    )
    .bind(work_id)
    .bind(external_identity)
    .bind(format!("{ordinal:02x}"))
    .bind(format!("{:02x}", ordinal + 8))
    .bind(Json(serde_json::json!({"fixture": ordinal})))
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
      "INSERT INTO factory_runs \
         (id, project_id, work_envelope_id, factory_configuration_id, factory_configuration_version, \
          state, version, subject_digest, admitted_at, updated_at) \
       VALUES ($1, '00000000-0000-0000-0000-000000000101', $2, \
         '00000000-0000-0000-0000-000000000104', 1, $3, 1, decode(repeat($4, 32), 'hex'), \
         to_timestamp(0), to_timestamp(0))",
    )
    .bind(run_id)
    .bind(work_id)
    .bind(state)
    .bind(format!("{:02x}", ordinal + 16))
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query("INSERT INTO factory_retention_work (run_id, available_at) VALUES ($1, to_timestamp(0))")
      .bind(run_id)
      .execute(&mut *transaction)
      .await
      .unwrap();
    for reference in 0_u128..3 {
      sqlx::query(
        "INSERT INTO factory_artifact_references (run_id, artifact_id, role, created_at) \
         VALUES ($1, $2, 'specification', to_timestamp(0))",
      )
      .bind(run_id)
      .bind(uuid(1_000 + u128::from(ordinal) * 10 + reference))
      .execute(&mut *transaction)
      .await
      .unwrap();
    }
  }
  seed_ambiguous_ownership(&mut transaction).await;
  transaction.commit().await.unwrap();
}

async fn seed_ambiguous_ownership(transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>) {
  let claim_id = vec![31_u8; 32];
  sqlx::query(
    "INSERT INTO factory_run_claims (id, run_id, owner, fence, claimed_at, expires_at) \
     VALUES ($1, $2, 'restored-owner', decode(repeat('20', 32), 'hex'), to_timestamp(1), to_timestamp(100))",
  )
  .bind(&claim_id)
  .bind(run_id(1).as_uuid())
  .execute(&mut **transaction)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO factory_run_budgets (id, run_id, run_version, usage, recorded_at) \
     VALUES (decode(repeat('21', 32), 'hex'), $1, 1, '{}', to_timestamp(0))",
  )
  .bind(run_id(1).as_uuid())
  .execute(&mut **transaction)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO factory_lifecycle_checkpoints \
       (id, run_id, run_version, lifecycle, cancellation_requested, recorded_at) \
     VALUES (decode(repeat('22', 32), 'hex'), $1, 1, '{}', false, to_timestamp(0))",
  )
  .bind(run_id(1).as_uuid())
  .execute(&mut **transaction)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO factory_run_current \
       (run_id, run_version, claim_id, budget_id, lifecycle_checkpoint_id) \
     VALUES ($1, 1, $2, decode(repeat('21', 32), 'hex'), decode(repeat('22', 32), 'hex'))",
  )
  .bind(run_id(1).as_uuid())
  .bind(&claim_id)
  .execute(&mut **transaction)
  .await
  .unwrap();

  let pending = FactoryOutboxRecord::pending(
    FactoryDigest::from_bytes([41; 32]),
    run_id(1),
    FactoryKey::new("fixture").unwrap(),
    FactoryDigest::from_bytes([42; 32]),
    time(1),
    time(1),
  );
  let claimed = FactoryOutboxRecord::claimed(
    &pending,
    FactoryKey::new("restored-owner").unwrap(),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([43; 32])),
      time(2),
      time(100_000),
    )
    .unwrap(),
    0,
    time(2),
  );
  insert_outbox(transaction, &pending).await;
  insert_outbox(transaction, &claimed).await;
  sqlx::query(
    "UPDATE factory_retention_work SET claim_owner = 'restored-retention-owner', \
       claim_expires_at = to_timestamp(100) WHERE run_id = $1",
  )
  .bind(run_id(2).as_uuid())
  .execute(&mut **transaction)
  .await
  .unwrap();
}

async fn insert_outbox(transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>, record: &FactoryOutboxRecord) {
  sqlx::query(
    "INSERT INTO factory_outbox_records \
       (id, operation_id, run_id, kind, input_digest, state, attempt, available_at, recorded_at, \
        owner, fence, claimed_at, claim_expires_at) \
     VALUES ($1,$2,$3,$4,$5,$6,$7,to_timestamp($8::double precision / 1000.0), \
       to_timestamp($9::double precision / 1000.0),$10,$11, \
       CASE WHEN $12::BIGINT IS NULL THEN NULL ELSE to_timestamp($12::double precision / 1000.0) END, \
       CASE WHEN $13::BIGINT IS NULL THEN NULL ELSE to_timestamp($13::double precision / 1000.0) END)",
  )
  .bind(record.id.as_bytes().as_slice())
  .bind(record.operation_id.as_bytes().as_slice())
  .bind(record.run_id.as_uuid())
  .bind(record.kind.as_str())
  .bind(record.input_digest.as_bytes().as_slice())
  .bind(match record.state {
    octacity_server_store::FactoryOutboxState::Pending => "pending",
    octacity_server_store::FactoryOutboxState::Claimed => "claimed",
    _ => unreachable!(),
  })
  .bind(i32::from(record.attempt))
  .bind(record.available_at.unix_millis())
  .bind(record.recorded_at.unix_millis())
  .bind(record.owner.as_ref().map(FactoryKey::as_str))
  .bind(record.claim.map(|value| value.fence().digest().as_bytes().to_vec()))
  .bind(record.claim.map(|value| value.claimed_at().unix_millis()))
  .bind(record.claim.map(|value| value.expires_at().unix_millis()))
  .execute(&mut **transaction)
  .await
  .unwrap();
}

async fn run_visibility(pool: &sqlx::PgPool, run_id: FactoryRunId) -> (bool, i64) {
  sqlx::query_as(
    "SELECT run.visible, (SELECT COUNT(*) FROM factory_artifact_references AS reference \
       WHERE reference.run_id = run.id AND reference.released_at IS NULL) \
     FROM factory_runs AS run WHERE run.id = $1",
  )
  .bind(run_id.as_uuid())
  .fetch_one(pool)
  .await
  .unwrap()
}

fn run_id(value: u128) -> FactoryRunId {
  FactoryRunId::from_uuid(uuid(value)).unwrap()
}

fn uuid(value: u128) -> uuid::Uuid {
  uuid::Uuid::from_u128(value)
}

fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}
