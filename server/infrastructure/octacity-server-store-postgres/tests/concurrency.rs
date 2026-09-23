#[path = "support/authoritative_fixture.rs"]
mod authoritative_fixture;
mod support;

use std::{
  collections::{BTreeMap, BTreeSet},
  sync::Arc,
};

use authoritative_fixture::seed_authoritative_prerequisites;
use octacity_protocol::{PlatformArchitecture, PlatformOs};
use octacity_server_domain::{
  AgentId, EntityKind, JobId, LeaseId, PipelineNodeId, RuntimeClass, Timestamp, TriggerId, TriggerVersion,
};
use octacity_server_job::JobRequirements;
use octacity_server_pipeline::DependencyPolicy;
use octacity_server_store::{
  AppendJobEvents, AppendJobEventsOutcome, ClaimDueSchedules, CompletionDisposition, CreateSchedule,
  CreateTriggerDefinition, DurableJobEvent, EventSequence, IdempotencyKey, JobClaim, JobClaimOutcome, JobCompletion,
  JobCompletionKind, JobEventKind, JobExecutionStore as _, LeaseAccess, LeaseFence, LeaseWindow, MaterializedJob,
  MissedRunPolicy, MutationDisposition, ScheduleDefinition, ScheduleStore as _, TriggerAcceptanceStore as _,
  TriggerKind, WorkerOwner, testing::authoritative_store_contract_fixture,
};
use octacity_server_store_postgres::{PostgresAuthoritativeStore, PostgresStore};
use serde_json::json;
use support::TestDatabase;
use tokio::sync::Barrier;

async fn independent_pool(source: &sqlx::PgPool) -> sqlx::PgPool {
  sqlx::PgPool::connect_with((*source.connect_options()).clone())
    .await
    .expect("connect an independent server pool to the test database")
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn concurrent_servers_accept_one_trigger_occurrence_once() {
  let database = TestDatabase::migrated().await;
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  let left = PostgresAuthoritativeStore::new(independent_pool(&database.pool).await, support::test_signer());
  let right = PostgresAuthoritativeStore::new(independent_pool(&database.pool).await, support::test_signer());
  let request = fixture.request;
  let occurrence_id = request.trigger.id;
  let target = request.trigger.target;
  let verify_pool = database.pool.clone();

  let result = tokio::spawn(async move {
    let barrier = Arc::new(Barrier::new(2));
    let left_task = tokio::spawn(accept_after_barrier(left, request.clone(), barrier.clone()));
    let right_task = tokio::spawn(accept_after_barrier(right, request, barrier));
    let left_outcome = left_task.await.unwrap().unwrap();
    let right_outcome = right_task.await.unwrap().unwrap();
    let dispositions = [left_outcome.disposition, right_outcome.disposition];
    assert_eq!(
      dispositions
        .iter()
        .filter(|disposition| **disposition == MutationDisposition::Applied)
        .count(),
      1
    );
    assert_eq!(
      dispositions
        .iter()
        .filter(|disposition| **disposition == MutationDisposition::Replayed)
        .count(),
      1
    );
    assert_eq!(left_outcome.build_id, right_outcome.build_id);
    assert_eq!(left_outcome.attempt_id, right_outcome.attempt_id);
    let build_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM builds WHERE trigger_occurrence_id = $1")
      .bind(occurrence_id.as_uuid())
      .fetch_one(&verify_pool)
      .await
      .unwrap();
    assert_eq!(build_count, 1, "one deduplicated occurrence may create only one Build");
    let (kind, configuration_id, configuration_version, cause, causality, metadata): (
      String,
      uuid::Uuid,
      i64,
      sqlx::types::Json<serde_json::Value>,
      sqlx::types::Json<serde_json::Value>,
      sqlx::types::Json<serde_json::Value>,
    ) = sqlx::query_as(
      "SELECT kind, build_configuration_id, build_configuration_version, cause, causality, provider_metadata \
       FROM trigger_occurrences WHERE id = $1",
    )
    .bind(occurrence_id.as_uuid())
    .fetch_one(&verify_pool)
    .await
    .unwrap();
    assert_eq!(kind, "manual");
    assert_eq!(configuration_id, target.configuration_id.as_uuid());
    assert_eq!(
      configuration_version,
      i64::try_from(target.configuration_version.get()).unwrap()
    );
    assert_eq!(cause.0, json!({"kind": "manual"}));
    assert_eq!(
      causality.0,
      json!({
        "depth": 0,
        "parent_occurrence_id": null,
        "root_occurrence_id": occurrence_id,
      })
    );
    assert_eq!(metadata.0, json!({}));
  })
  .await;

  database.cleanup().await;
  result.unwrap();
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn concurrent_agents_lease_one_ready_job_once() {
  let database = TestDatabase::migrated().await;
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  let second_agent = AgentId::from_uuid(uuid::Uuid::from_u128(700)).unwrap();
  seed_matching_agent(&database.pool, second_agent, fixture.allowed_pool).await;
  let left = PostgresAuthoritativeStore::new(independent_pool(&database.pool).await, support::test_signer());
  let right = PostgresAuthoritativeStore::new(independent_pool(&database.pool).await, support::test_signer());
  left.accept_trigger(fixture.request).await.unwrap();
  let left_claim = claim(
    701,
    [1; 32],
    fixture.agent_id,
    fixture.registration_epoch,
    fixture.allowed_pool,
  );
  let right_claim = claim(
    702,
    [2; 32],
    second_agent,
    fixture.registration_epoch,
    fixture.allowed_pool,
  );

  let result = tokio::spawn(async move {
    let barrier = Arc::new(Barrier::new(2));
    let left_task = tokio::spawn(claim_after_barrier(left, left_claim, barrier.clone()));
    let right_task = tokio::spawn(claim_after_barrier(right, right_claim, barrier));
    let outcomes = [left_task.await.unwrap().unwrap(), right_task.await.unwrap().unwrap()];
    assert_eq!(
      outcomes
        .iter()
        .filter(|outcome| matches!(outcome, JobClaimOutcome::Claimed(_)))
        .count(),
      1
    );
    assert_eq!(
      outcomes
        .iter()
        .filter(|outcome| matches!(outcome, JobClaimOutcome::Empty))
        .count(),
      1
    );
  })
  .await;

  database.cleanup().await;
  result.unwrap();
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn concurrent_event_and_completion_replays_have_one_dag_transition() {
  let database = TestDatabase::migrated().await;
  let fixture = authoritative_store_contract_fixture();
  let child_job = fixture.request.jobs[1].id;
  seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  let left = PostgresAuthoritativeStore::new(independent_pool(&database.pool).await, support::test_signer());
  let right = PostgresAuthoritativeStore::new(independent_pool(&database.pool).await, support::test_signer());
  left.accept_trigger(fixture.request).await.unwrap();
  let grant = match left
    .claim_ready_job(claim(
      710,
      [10; 32],
      fixture.agent_id,
      fixture.registration_epoch,
      fixture.allowed_pool,
    ))
    .await
    .unwrap()
  {
    JobClaimOutcome::Claimed(grant) => *grant,
    JobClaimOutcome::Empty => panic!("seeded root Job must be claimable"),
  };
  let access = LeaseAccess {
    lease_id: grant.lease_id,
    fence: grant.fence,
    agent_id: grant.agent_id,
    registration_epoch: grant.registration_epoch,
  };
  let verify_pool = database.pool.clone();

  let result = tokio::spawn(async move {
    let replayed_event = event(access, 1, 1);
    let barrier = Arc::new(Barrier::new(2));
    let left_task = tokio::spawn(append_after_barrier(
      left.clone(),
      replayed_event.clone(),
      barrier.clone(),
    ));
    let right_task = tokio::spawn(append_after_barrier(right.clone(), replayed_event, barrier));
    let appended = [left_task.await.unwrap().unwrap(), right_task.await.unwrap().unwrap()];
    assert!(appended.iter().all(|outcome| outcome.inserted == 1));
    assert!(appended.iter().all(|outcome| outcome.acknowledged_through.get() == 1));
    let durable_events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM job_events WHERE job_id = $1")
      .bind(grant.job_id.as_uuid())
      .fetch_one(&verify_pool)
      .await
      .unwrap();
    assert_eq!(durable_events, 1, "a replayed result must not duplicate state");

    let barrier = Arc::new(Barrier::new(2));
    let left_task = tokio::spawn(append_after_barrier(left.clone(), event(access, 2, 2), barrier.clone()));
    let right_task = tokio::spawn(append_after_barrier(right.clone(), event(access, 2, 3), barrier));
    let conflicting = [left_task.await.unwrap(), right_task.await.unwrap()];
    assert_eq!(conflicting.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert_eq!(
      conflicting.into_iter().find_map(Result::err),
      Some(octacity_server_store::StoreError::Conflict {
        entity: EntityKind::Job
      })
    );

    let completion = JobCompletion {
      completion_id: octacity_server_store::IdempotencyKey::new("concurrent-completion").unwrap(),
      lease: access,
      final_sequence: Some(EventSequence::new(2).unwrap()),
      kind: JobCompletionKind::Succeeded,
      completed_at: time(3_000),
    };
    let barrier = Arc::new(Barrier::new(2));
    let left_task = tokio::spawn(complete_after_barrier(left, completion.clone(), barrier.clone()));
    let right_task = tokio::spawn(complete_after_barrier(right, completion, barrier));
    let completed = [left_task.await.unwrap().unwrap(), right_task.await.unwrap().unwrap()];
    assert_eq!(
      completed
        .iter()
        .filter(|outcome| outcome.disposition == MutationDisposition::Applied)
        .count(),
      1
    );
    assert_eq!(
      completed
        .iter()
        .filter(|outcome| outcome.disposition == MutationDisposition::Replayed)
        .count(),
      1
    );
    assert!(completed.iter().all(|outcome| outcome.ready_jobs == [child_job]));
  })
  .await;

  database.cleanup().await;
  result.unwrap();
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn concurrent_fan_in_completions_cannot_leave_a_satisfied_child_blocked() {
  let database = TestDatabase::migrated().await;
  let mut fixture = authoritative_store_contract_fixture();
  let second_agent = AgentId::from_uuid(uuid::Uuid::from_u128(730)).unwrap();
  let first_root = materialized_job(
    fixture.request.build.id,
    fixture.request.attempt_id,
    731,
    "first-root",
    Vec::new(),
    fixture.allowed_pool,
  );
  let second_root = materialized_job(
    fixture.request.build.id,
    fixture.request.attempt_id,
    732,
    "second-root",
    Vec::new(),
    fixture.allowed_pool,
  );
  let child = materialized_job(
    fixture.request.build.id,
    fixture.request.attempt_id,
    733,
    "fan-in-child",
    vec![first_root.id, second_root.id],
    fixture.allowed_pool,
  );
  let child_id = child.id;
  fixture.request.jobs = vec![first_root, second_root, child];
  seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  seed_matching_agent(&database.pool, second_agent, fixture.allowed_pool).await;
  let left = PostgresAuthoritativeStore::new(independent_pool(&database.pool).await, support::test_signer());
  let right = PostgresAuthoritativeStore::new(independent_pool(&database.pool).await, support::test_signer());
  left.accept_trigger(fixture.request).await.unwrap();

  let first_grant = claimed(
    left
      .claim_ready_job(claim(
        734,
        [34; 32],
        fixture.agent_id,
        fixture.registration_epoch,
        fixture.allowed_pool,
      ))
      .await
      .unwrap(),
  );
  let second_grant = claimed(
    right
      .claim_ready_job(claim(
        735,
        [35; 32],
        second_agent,
        fixture.registration_epoch,
        fixture.allowed_pool,
      ))
      .await
      .unwrap(),
  );
  let first_completion = successful_completion(first_grant, 4_000);
  let second_completion = successful_completion(second_grant, 4_000);
  let verify_pool = database.pool.clone();

  let result = tokio::spawn(async move {
    let barrier = Arc::new(Barrier::new(2));
    let left_task = tokio::spawn(complete_after_barrier(left, first_completion, barrier.clone()));
    let right_task = tokio::spawn(complete_after_barrier(right, second_completion, barrier));
    let outcomes = [left_task.await.unwrap().unwrap(), right_task.await.unwrap().unwrap()];
    assert_eq!(
      outcomes
        .iter()
        .flat_map(|outcome| outcome.ready_jobs.iter())
        .copied()
        .collect::<Vec<_>>(),
      [child_id]
    );
    let ready_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ready_queue_entries WHERE job_id = $1")
      .bind(child_id.as_uuid())
      .fetch_one(&verify_pool)
      .await
      .unwrap();
    assert_eq!(ready_count, 1);
  })
  .await;

  database.cleanup().await;
  result.unwrap();
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn concurrent_schema_primitives_have_one_visible_winner() {
  let database = TestDatabase::migrated().await;
  let fixture = authoritative_store_contract_fixture();
  let build_id = fixture.request.build.id;
  let configuration_id = fixture.request.build.configuration_id;
  let configuration_version = fixture.request.build.configuration_version;
  let schedule_trigger_id = TriggerId::from_uuid(uuid::Uuid::from_u128(741)).unwrap();
  seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  PostgresAuthoritativeStore::new(database.pool.clone(), support::test_signer())
    .accept_trigger(fixture.request)
    .await
    .unwrap();
  let schedule = ScheduleDefinition {
    expression: "* * * * * * *".to_owned(),
    timezone: "UTC".to_owned(),
    missed_run_policy: MissedRunPolicy::RunOnce,
  };
  PostgresStore::new(database.pool.clone())
    .create_schedule(CreateSchedule {
      trigger: CreateTriggerDefinition {
        id: schedule_trigger_id,
        version: TriggerVersion::INITIAL,
        configuration_id,
        configuration_version,
        kind: TriggerKind::Scheduled,
        enabled: true,
        definition: json!({}),
        idempotency_key: IdempotencyKey::new("schedule-concurrency").unwrap(),
        created_at: Timestamp::from_unix_millis(0).unwrap(),
      },
      next_occurrence_at: Timestamp::from_unix_millis(1_000).unwrap(),
      schedule,
    })
    .await
    .unwrap();
  let left_pool = independent_pool(&database.pool).await;
  let right_pool = independent_pool(&database.pool).await;
  let left_store = PostgresStore::new(left_pool.clone());
  let right_store = PostgresStore::new(right_pool.clone());
  let verify_pool = database.pool.clone();

  let result = tokio::spawn(async move {
    let barrier = Arc::new(Barrier::new(2));
    let (left_claim, right_claim) = tokio::join!(
      claim_schedule(left_store.clone(), "server-a", barrier.clone()),
      claim_schedule(right_store.clone(), "server-b", barrier)
    );
    assert_eq!(
      [left_claim.unwrap(), right_claim.unwrap()]
        .iter()
        .filter(|claimed| **claimed)
        .count(),
      1
    );
    let barrier = Arc::new(Barrier::new(2));
    let (left_attempt, right_attempt) = tokio::join!(
      insert_second_attempt(
        &left_pool,
        build_id.as_uuid(),
        uuid::Uuid::from_u128(720),
        barrier.clone()
      ),
      insert_second_attempt(&right_pool, build_id.as_uuid(), uuid::Uuid::from_u128(721), barrier)
    );
    assert_single_database_winner([left_attempt, right_attempt]);
    let attempt_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM attempts WHERE build_id = $1")
      .bind(build_id.as_uuid())
      .fetch_one(&verify_pool)
      .await
      .unwrap();
    assert_eq!(
      attempt_count, 2,
      "the initial Attempt and exactly one retry must be visible"
    );

    let audit_id = uuid::Uuid::from_u128(730);
    let barrier = Arc::new(Barrier::new(2));
    let (left_audit, right_audit) = tokio::join!(
      insert_audit_fact(&left_pool, audit_id, barrier.clone()),
      insert_audit_fact(&right_pool, audit_id, barrier)
    );
    assert_single_database_winner([left_audit, right_audit]);
    let audit_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_facts WHERE id = $1")
      .bind(audit_id)
      .fetch_one(&verify_pool)
      .await
      .unwrap();
    assert_eq!(audit_count, 1);

    let barrier = Arc::new(Barrier::new(2));
    let (left_idempotency, right_idempotency) = tokio::join!(
      reserve_idempotency(&left_pool, [1; 32], barrier.clone()),
      reserve_idempotency(&right_pool, [1; 32], barrier)
    );
    let dispositions = [left_idempotency.unwrap(), right_idempotency.unwrap()];
    assert_eq!(
      dispositions
        .iter()
        .filter(|disposition| **disposition == PrimitiveDisposition::Applied)
        .count(),
      1
    );
    assert_eq!(
      dispositions
        .iter()
        .filter(|disposition| **disposition == PrimitiveDisposition::Replayed)
        .count(),
      1
    );
    assert_eq!(
      reserve_idempotency(&left_pool, [2; 32], Arc::new(Barrier::new(1)))
        .await
        .unwrap(),
      PrimitiveDisposition::Conflict
    );
    let idempotency_count: i64 = sqlx::query_scalar(
      "SELECT COUNT(*) FROM idempotency_records WHERE scope = 'contract' AND idempotency_key = 'same-key'",
    )
    .fetch_one(&verify_pool)
    .await
    .unwrap();
    assert_eq!(idempotency_count, 1);
  })
  .await;

  database.cleanup().await;
  result.unwrap();
}

async fn accept_after_barrier(
  store: PostgresAuthoritativeStore,
  request: octacity_server_store::AcceptTrigger,
  barrier: Arc<Barrier>,
) -> Result<octacity_server_store::AcceptTriggerOutcome, octacity_server_store::StoreError> {
  barrier.wait().await;
  store.accept_trigger(request).await
}

async fn claim_after_barrier(
  store: PostgresAuthoritativeStore,
  claim: JobClaim,
  barrier: Arc<Barrier>,
) -> Result<JobClaimOutcome, octacity_server_store::StoreError> {
  barrier.wait().await;
  store.claim_ready_job(claim).await
}

async fn append_after_barrier(
  store: PostgresAuthoritativeStore,
  request: AppendJobEvents,
  barrier: Arc<Barrier>,
) -> Result<AppendJobEventsOutcome, octacity_server_store::StoreError> {
  barrier.wait().await;
  store.append_job_events(request).await
}

async fn complete_after_barrier(
  store: PostgresAuthoritativeStore,
  request: JobCompletion,
  barrier: Arc<Barrier>,
) -> Result<CompletionDisposition, octacity_server_store::StoreError> {
  barrier.wait().await;
  store.complete_job(request).await
}

async fn claim_schedule(
  store: PostgresStore,
  owner: &str,
  barrier: Arc<Barrier>,
) -> Result<bool, octacity_server_store::StoreError> {
  barrier.wait().await;
  store
    .claim_due_schedules(ClaimDueSchedules::new(
      WorkerOwner::new(owner)?,
      Timestamp::from_unix_millis(1_000).unwrap(),
      Timestamp::from_unix_millis(2_000).unwrap(),
      1,
    )?)
    .await
    .map(|claims| claims.len() == 1)
}

async fn insert_second_attempt(
  pool: &sqlx::PgPool,
  build_id: uuid::Uuid,
  attempt_id: uuid::Uuid,
  barrier: Arc<Barrier>,
) -> Result<(), sqlx::Error> {
  barrier.wait().await;
  sqlx::query(
    "INSERT INTO attempts (id, build_id, attempt_number, state, version, created_at, updated_at) \
     VALUES ($1, $2, 2, 'running', 1, now(), now())",
  )
  .bind(attempt_id)
  .bind(build_id)
  .execute(pool)
  .await?;
  Ok(())
}

async fn insert_audit_fact(
  pool: &sqlx::PgPool,
  audit_id: uuid::Uuid,
  barrier: Arc<Barrier>,
) -> Result<(), sqlx::Error> {
  barrier.wait().await;
  sqlx::query(
    "INSERT INTO audit_facts \
       (id, actor_kind, operation, target_kind, target_identity, request_identity, idempotency_key, outcome, \
        safe_metadata, occurred_at) \
     VALUES ($1, 'worker', 'contract-race', 'build', 'build-1', 'request-1', 'same-key', 'accepted', '{}', now())",
  )
  .bind(audit_id)
  .execute(pool)
  .await?;
  Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PrimitiveDisposition {
  Applied,
  Replayed,
  Conflict,
}

async fn reserve_idempotency(
  pool: &sqlx::PgPool,
  digest: [u8; 32],
  barrier: Arc<Barrier>,
) -> Result<PrimitiveDisposition, sqlx::Error> {
  barrier.wait().await;
  let mut transaction = pool.begin().await?;
  let inserted = sqlx::query(
    "INSERT INTO idempotency_records (scope, idempotency_key, request_digest, outcome, created_at) \
     VALUES ('contract', 'same-key', $1, '{\"result\":\"accepted\"}', now()) \
     ON CONFLICT DO NOTHING",
  )
  .bind(digest.as_slice())
  .execute(&mut *transaction)
  .await?;
  let disposition = if inserted.rows_affected() == 1 {
    PrimitiveDisposition::Applied
  } else {
    let existing: Vec<u8> = sqlx::query_scalar(
      "SELECT request_digest FROM idempotency_records \
       WHERE scope = 'contract' AND idempotency_key = 'same-key' FOR UPDATE",
    )
    .fetch_one(&mut *transaction)
    .await?;
    if existing == digest {
      PrimitiveDisposition::Replayed
    } else {
      PrimitiveDisposition::Conflict
    }
  };
  transaction.commit().await?;
  Ok(disposition)
}

fn assert_single_database_winner(outcomes: [Result<(), sqlx::Error>; 2]) {
  assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
  let error = outcomes.into_iter().find_map(Result::err).unwrap();
  let sql_state = error.as_database_error().and_then(|database| database.code());
  assert!(matches!(sql_state.as_deref(), Some("23505" | "23514")));
}

async fn seed_matching_agent(pool: &sqlx::PgPool, agent: AgentId, allowed_pool: octacity_server_domain::PoolId) {
  sqlx::query(
    "INSERT INTO agents \
       (id, name, pool_id, pool_version, state, inventory, version, created_at, updated_at) \
     VALUES ($1, 'second-contract-agent', $2, 1, 'online', '{}', 1, now(), now())",
  )
  .bind(agent.as_uuid())
  .bind(allowed_pool.as_uuid())
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO agent_registrations \
       (id, agent_id, epoch, credential_hash, inventory, registered_at, expires_at) \
     VALUES ($1, $2, 1, decode(repeat('03', 32), 'hex'), $3, to_timestamp(0), '9999-12-31 23:59:59+00')",
  )
  .bind(uuid::Uuid::from_u128(703))
  .bind(agent.as_uuid())
  .bind(sqlx::types::Json(octacity_server_store::testing::compatible_inventory(
    agent,
  )))
  .execute(pool)
  .await
  .unwrap();
}

fn claim(
  lease: u128,
  fence: [u8; 32],
  agent_id: AgentId,
  registration_epoch: octacity_server_store::RegistrationEpoch,
  pool_id: octacity_server_domain::PoolId,
) -> JobClaim {
  JobClaim::new(
    LeaseId::from_uuid(uuid::Uuid::from_u128(lease)).unwrap(),
    LeaseFence::from_bytes(fence),
    agent_id,
    registration_epoch,
    pool_id,
    octacity_server_store::testing::compatible_snapshot(),
    LeaseWindow::new(
      Timestamp::from_unix_millis(1_000).unwrap(),
      Timestamp::from_unix_millis(253_402_300_799_000).unwrap(),
    )
    .unwrap(),
  )
  .unwrap()
}

fn event(lease: LeaseAccess, sequence: u64, digest_marker: u8) -> AppendJobEvents {
  AppendJobEvents::new(
    lease,
    vec![
      DurableJobEvent::new(
        EventSequence::new(sequence).unwrap(),
        JobEventKind::new("progress").unwrap(),
        time(2_000),
        json!({"digest_marker": digest_marker}),
      )
      .unwrap(),
    ],
    time(2_500),
  )
  .unwrap()
}

fn materialized_job(
  build_id: octacity_server_domain::BuildId,
  _attempt_id: octacity_server_domain::AttemptId,
  id: u128,
  node: &str,
  dependencies: Vec<JobId>,
  pool_id: octacity_server_domain::PoolId,
) -> MaterializedJob {
  let job_id = JobId::from_uuid(uuid::Uuid::from_u128(id)).unwrap();
  MaterializedJob::new(
    job_id,
    PipelineNodeId::new(node).unwrap(),
    dependencies,
    DependencyPolicy::AllSucceeded,
    vec![pool_id],
    octacity_server_store::MaterializedJobPayload::new(
      JobRequirements {
        capabilities: BTreeSet::new(),
        labels: BTreeMap::new(),
        minimum_cpu_millis: 0,
        minimum_memory_bytes: 0,
        minimum_disk_bytes: 0,
        runtime_class: RuntimeClass::Native,
        operating_system: PlatformOs::Linux,
        architecture: PlatformArchitecture::Amd64,
      },
      octacity_server_store::testing::job_spec_template(build_id, node),
    )
    .unwrap(),
  )
  .unwrap()
}

fn claimed(outcome: JobClaimOutcome) -> octacity_server_store::LeaseGrant {
  match outcome {
    JobClaimOutcome::Claimed(grant) => *grant,
    JobClaimOutcome::Empty => panic!("a seeded root Job must be claimable"),
  }
}

fn successful_completion(grant: octacity_server_store::LeaseGrant, completed_at: i64) -> JobCompletion {
  JobCompletion {
    completion_id: octacity_server_store::IdempotencyKey::new(format!("completion-{}", grant.lease_id)).unwrap(),
    lease: LeaseAccess {
      lease_id: grant.lease_id,
      fence: grant.fence,
      agent_id: grant.agent_id,
      registration_epoch: grant.registration_epoch,
    },
    final_sequence: None,
    kind: JobCompletionKind::Succeeded,
    completed_at: time(completed_at),
  }
}

fn time(milliseconds: i64) -> Timestamp {
  Timestamp::from_unix_millis(milliseconds).unwrap()
}
