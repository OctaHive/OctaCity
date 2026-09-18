#[path = "support/authoritative_fixture.rs"]
mod authoritative_fixture;
mod support;

use std::{fmt::Debug, str::FromStr, sync::Arc};

use async_trait::async_trait;
use authoritative_fixture::seed_authoritative_prerequisites;
use octacity_server_domain::{EntityKind, PipelineId, PoolId, ProjectId, Timestamp, TriggerId};
use octacity_server_store::{
  AgentCredentialStore as _, AgentRegistrationProof, AppendJobEvents, AuthoritativeStore as _, CredentialSecret,
  DurableJobEvent, EventSequence, JobClaim, JobClaimOutcome, JobCompletion, JobCompletionKind, JobEventKind,
  LeaseAccess, LeaseFence, MutationDisposition, RegisterAgent, StoreError,
  testing::{
    MutationEvidenceCounts, MutationEvidenceProbe, agent_credential_store_contract_fixture,
    authoritative_store_contract_fixture, verify_agent_credential_store_contract, verify_authoritative_store_contract,
    verify_configuration_store_contract, verify_pipeline_store_contract, verify_project_store_contract,
  },
};
use octacity_server_store_postgres::PostgresStore;
use serde_json::json;
use sqlx::PgPool;
use support::TestDatabase;

struct PostgresEvidenceProbe(PgPool);

#[async_trait]
impl MutationEvidenceProbe for PostgresEvidenceProbe {
  async fn mutation_evidence_counts(&self) -> MutationEvidenceCounts {
    let (idempotency, audit, outbox): (i64, i64, i64) = sqlx::query_as(
      "SELECT (SELECT COUNT(*) FROM idempotency_records), \
              (SELECT COUNT(*) FROM audit_facts), \
              (SELECT COUNT(*) FROM outbox_entries)",
    )
    .fetch_one(&self.0)
    .await
    .expect("contract evidence must remain queryable");
    MutationEvidenceCounts {
      idempotency: usize::try_from(idempotency).unwrap(),
      audit: usize::try_from(audit).unwrap(),
      outbox: usize::try_from(outbox).unwrap(),
    }
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn postgres_satisfies_the_authoritative_store_contract() {
  let database = TestDatabase::migrated().await;
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  let store = Arc::new(PostgresStore::new(database.pool.clone()));

  let evidence = Arc::new(PostgresEvidenceProbe(database.pool.clone()));
  let result = tokio::spawn(verify_authoritative_store_contract(store, evidence)).await;
  database.cleanup().await;
  result.expect("PostgreSQL authoritative-store contract failed");
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn postgres_satisfies_the_agent_credential_contract() {
  let database = TestDatabase::migrated().await;
  let fixture = agent_credential_store_contract_fixture();
  seed_credential_pool(&database.pool, &fixture.enrollment).await;
  let store = Arc::new(PostgresStore::new(database.pool.clone()));

  let evidence = Arc::new(PostgresEvidenceProbe(database.pool.clone()));
  let result = tokio::spawn(verify_agent_credential_store_contract(store, evidence)).await;
  database.cleanup().await;
  result.expect("PostgreSQL Agent credential contract failed");
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn postgres_satisfies_the_project_store_contract() {
  let database = TestDatabase::migrated().await;
  let store = Arc::new(PostgresStore::new(database.pool.clone()));
  let evidence = Arc::new(PostgresEvidenceProbe(database.pool.clone()));
  let result = tokio::spawn(verify_project_store_contract(store, evidence)).await;
  database.cleanup().await;
  result.expect("PostgreSQL Project-store contract failed");
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn postgres_satisfies_the_pipeline_store_contract() {
  let database = TestDatabase::migrated().await;
  let project_id = id::<ProjectId>(1);
  sqlx::query(
    "INSERT INTO projects (id, parent_id, name, version, created_at, updated_at) \
     VALUES ($1, NULL, 'pipeline-contract', 1, now(), now())",
  )
  .bind(project_id.as_uuid())
  .execute(&database.pool)
  .await
  .unwrap();
  let store = Arc::new(PostgresStore::new(database.pool.clone()));
  let evidence = Arc::new(PostgresEvidenceProbe(database.pool.clone()));
  let result = tokio::spawn(verify_pipeline_store_contract(store, evidence, project_id)).await;
  database.cleanup().await;
  result.expect("PostgreSQL Pipeline-store contract failed");
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn postgres_satisfies_the_configuration_store_contract() {
  let database = TestDatabase::migrated().await;
  let project_id = id::<ProjectId>(1);
  let pipeline_id = id::<PipelineId>(2);
  let pool_id = id::<PoolId>(3);
  sqlx::query(
    "INSERT INTO projects (id, parent_id, name, version, created_at, updated_at) \
     VALUES ($1, NULL, 'configuration-contract', 1, now(), now())",
  )
  .bind(project_id.as_uuid())
  .execute(&database.pool)
  .await
  .unwrap();
  sqlx::query("INSERT INTO pipelines (id, project_id, name, created_at) VALUES ($1, $2, 'main', now())")
    .bind(pipeline_id.as_uuid())
    .bind(project_id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  sqlx::query(
    "INSERT INTO pipeline_versions (pipeline_id, version, dag_snapshot, published_at) \
     VALUES ($1, 1, '{}', now()), ($1, 2, '{}', now())",
  )
  .bind(pipeline_id.as_uuid())
  .execute(&database.pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO pools (id, version, name, enabled, drain_state, admission_policy, concurrency_limit, created_at) \
     VALUES ($1, 1, 'configuration-pool', true, 'accepting', '{}', 1, now())",
  )
  .bind(pool_id.as_uuid())
  .execute(&database.pool)
  .await
  .unwrap();
  let store = Arc::new(PostgresStore::new(database.pool.clone()));
  let evidence = Arc::new(PostgresEvidenceProbe(database.pool.clone()));
  let result = tokio::spawn(verify_configuration_store_contract(
    store,
    evidence,
    project_id,
    pipeline_id,
    pool_id,
  ))
  .await;
  database.cleanup().await;
  result.expect("PostgreSQL Configuration-store contract failed");
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn agent_credentials_are_hashed_at_rest_and_replay_survives_adapter_restart() {
  let database = TestDatabase::migrated().await;
  let fixture = agent_credential_store_contract_fixture();
  seed_credential_pool(&database.pool, &fixture.enrollment).await;
  let first_store = PostgresStore::new(database.pool.clone());
  first_store
    .issue_agent_enrollment(fixture.enrollment.clone())
    .await
    .unwrap();

  let enrollment_hash: Vec<u8> =
    sqlx::query_scalar("SELECT credential_hash FROM agent_enrollment_credentials WHERE id = $1")
      .bind(fixture.enrollment.credential_id.as_uuid())
      .fetch_one(&database.pool)
      .await
      .unwrap();
  assert_eq!(enrollment_hash, fixture.enrollment.credential.digest().as_bytes());
  assert_ne!(enrollment_hash, [0x11; 32]);

  let restarted_store = PostgresStore::new(database.pool.clone());
  assert_eq!(
    restarted_store
      .issue_agent_enrollment(fixture.enrollment.clone())
      .await
      .unwrap()
      .disposition,
    MutationDisposition::Replayed
  );
  let registration = RegisterAgent::new(
    id(700),
    CredentialSecret::from_bytes([0x61; 32]),
    fixture.agent_id,
    octacity_server_domain::AgentName::new("at-rest-agent").unwrap(),
    AgentRegistrationProof::Enrollment {
      credential_id: fixture.enrollment.credential_id,
      credential: fixture.enrollment.credential,
    },
    octacity_server_store::AgentPlatform::new("linux", "amd64").unwrap(),
    json!({"host_platform": "linux-amd64"}),
    time(200),
    time(900),
  )
  .unwrap();
  let expected_hash = registration.credential.digest().as_bytes();
  let registered = restarted_store.register_agent(registration).await.unwrap();
  let registration_hash: Vec<u8> = sqlx::query_scalar("SELECT credential_hash FROM agent_registrations WHERE id = $1")
    .bind(registered.credential_id.as_uuid())
    .fetch_one(&database.pool)
    .await
    .unwrap();
  assert_eq!(registration_hash, expected_hash);
  assert_ne!(registration_hash, [0x61; 32]);

  let sensitive_audit_fields: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM audit_facts \
     WHERE safe_metadata ?| ARRAY['credential', 'credential_hash', 'secret']",
  )
  .fetch_one(&database.pool)
  .await
  .unwrap();
  assert_eq!(sensitive_audit_fields, 0);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn concurrent_enrollment_consumption_creates_exactly_one_agent() {
  let database = TestDatabase::migrated().await;
  let fixture = agent_credential_store_contract_fixture();
  seed_credential_pool(&database.pool, &fixture.enrollment).await;
  let store = PostgresStore::new(database.pool.clone());
  store.issue_agent_enrollment(fixture.enrollment.clone()).await.unwrap();

  let request = |registration: u64, agent: u64, secret: u8| {
    RegisterAgent::new(
      id(registration),
      CredentialSecret::from_bytes([secret; 32]),
      id(agent),
      octacity_server_domain::AgentName::new(format!("concurrent-agent-{agent}")).unwrap(),
      AgentRegistrationProof::Enrollment {
        credential_id: fixture.enrollment.credential_id,
        credential: fixture.enrollment.credential.clone(),
      },
      octacity_server_store::AgentPlatform::new("linux", "amd64").unwrap(),
      json!({"host_platform": "linux-amd64"}),
      time(200),
      time(900),
    )
    .unwrap()
  };
  let left = store.clone();
  let right = store.clone();
  let (left_result, right_result) = tokio::join!(
    left.register_agent(request(710, 711, 0x71)),
    right.register_agent(request(720, 721, 0x72)),
  );
  let outcomes = [left_result, right_result];
  assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
  assert_eq!(
    outcomes
      .iter()
      .filter(|outcome| matches!(outcome, Err(StoreError::CredentialRejected)))
      .count(),
    1
  );
  let agent_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agents")
    .fetch_one(&database.pool)
    .await
    .unwrap();
  let registration_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_registrations")
    .fetch_one(&database.pool)
    .await
    .unwrap();
  assert_eq!((agent_count, registration_count), (1, 1));
  database.cleanup().await;
}

async fn seed_credential_pool(pool: &PgPool, enrollment: &octacity_server_store::IssueAgentEnrollment) {
  sqlx::query(
    "INSERT INTO pools \
       (id, version, name, enabled, drain_state, admission_policy, concurrency_limit, created_at) \
     VALUES ($1, $2, 'credential-contract', true, 'accepting', '{}', 1, now())",
  )
  .bind(enrollment.pool_id.as_uuid())
  .bind(i64::try_from(enrollment.pool_version.get()).unwrap())
  .execute(pool)
  .await
  .unwrap();
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn postgres_classifies_missing_fenced_and_expired_mutations() {
  let database = TestDatabase::migrated().await;
  let result = verify_classified_outcomes(&database.pool).await;
  database.cleanup().await;
  result.unwrap();
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn accepted_mutations_atomically_persist_replay_audit_and_outbox() {
  let database = TestDatabase::migrated().await;
  let result = verify_mutation_envelopes(&database.pool).await;
  database.cleanup().await;
  result.unwrap();
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn auxiliary_record_failure_rolls_back_the_domain_mutation() {
  let database = TestDatabase::migrated().await;
  let result = verify_auxiliary_failure_rollback(&database.pool).await;
  database.cleanup().await;
  result.unwrap();
}

async fn verify_classified_outcomes(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(pool, &fixture).await?;
  let store = PostgresStore::new(pool.clone());

  let mut missing = fixture.request.clone();
  missing.trigger.trigger.id = id::<TriggerId>(999);
  assert_eq!(
    store.accept_trigger(missing).await.unwrap_err(),
    StoreError::NotFound {
      entity: EntityKind::Trigger
    }
  );

  store.accept_trigger(fixture.request.clone()).await?;
  let claim = JobClaim::new(
    id(500),
    LeaseFence::from_bytes([5; 32]),
    fixture.agent_id,
    fixture.registration_epoch,
    fixture.allowed_pool,
    time(1_000),
    time(2_000),
  )?;
  let grant = match store.claim_ready_job(claim).await? {
    JobClaimOutcome::Claimed(grant) => grant,
    JobClaimOutcome::Empty => return Err("seeded root Job was not claimable".into()),
  };
  let access = LeaseAccess {
    lease_id: grant.lease_id,
    fence: grant.fence,
    agent_id: grant.agent_id,
    registration_epoch: grant.registration_epoch,
  };
  let fenced = LeaseAccess {
    fence: LeaseFence::from_bytes([0xff; 32]),
    ..access
  };
  assert_eq!(
    store.append_job_events(one_event(fenced)).await.unwrap_err(),
    StoreError::Fenced { lease: grant.lease_id }
  );
  assert_eq!(
    store.append_job_events(one_event(access)).await.unwrap_err(),
    StoreError::Expired { lease: grant.lease_id }
  );
  Ok(())
}

async fn verify_mutation_envelopes(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(pool, &fixture).await?;
  let request = fixture.request.clone();
  let store = PostgresStore::new(pool.clone());
  let accepted = store.accept_trigger(request.clone()).await?;
  let claim = JobClaim::new(
    id(800),
    LeaseFence::from_bytes([8; 32]),
    fixture.agent_id,
    fixture.registration_epoch,
    fixture.allowed_pool,
    time(1_000),
    time(253_402_300_799_000),
  )?;
  let grant = match store.claim_ready_job(claim).await? {
    JobClaimOutcome::Claimed(grant) => grant,
    JobClaimOutcome::Empty => return Err("seeded root Job was not claimable".into()),
  };
  let access = LeaseAccess {
    lease_id: grant.lease_id,
    fence: grant.fence,
    agent_id: grant.agent_id,
    registration_epoch: grant.registration_epoch,
  };
  let event = one_event(access);
  let appended = store.append_job_events(event.clone()).await?;
  let completion = JobCompletion {
    lease: access,
    final_sequence: Some(EventSequence::new(1)?),
    kind: JobCompletionKind::Succeeded,
    completed_at: time(2_000),
  };
  let completed = store.complete_job(completion).await?;

  assert_eq!(mutation_record_count(pool, "idempotency_records").await?, 4);
  assert_eq!(mutation_record_count(pool, "audit_facts").await?, 4);
  assert_eq!(mutation_record_count(pool, "outbox_entries").await?, 4);
  let topics: Vec<String> = sqlx::query_scalar("SELECT topic FROM outbox_entries ORDER BY topic")
    .fetch_all(pool)
    .await?;
  assert_eq!(
    topics,
    ["build.accepted", "job.claimed", "job.completed", "job.events-appended"]
  );

  drop(store);
  let recovered = PostgresStore::new(pool.clone());
  assert_eq!(
    recovered.accept_trigger(request).await?.disposition,
    MutationDisposition::Replayed
  );
  assert_eq!(recovered.claim_ready_job(claim).await?, JobClaimOutcome::Claimed(grant));
  assert_eq!(recovered.append_job_events(event).await?, appended);
  assert_eq!(
    recovered.complete_job(completion).await?.disposition,
    MutationDisposition::Replayed
  );
  assert_eq!(completed.job_id, grant.job_id);
  assert_eq!(accepted.ready_jobs, [grant.job_id]);
  assert_eq!(mutation_record_count(pool, "idempotency_records").await?, 4);
  assert_eq!(mutation_record_count(pool, "audit_facts").await?, 4);
  assert_eq!(mutation_record_count(pool, "outbox_entries").await?, 4);

  let mismatched_claim = JobClaim {
    expires_at: time(253_402_300_798_999),
    ..claim
  };
  assert_eq!(
    recovered.claim_ready_job(mismatched_claim).await.unwrap_err(),
    StoreError::Conflict {
      entity: EntityKind::Lease
    }
  );

  Ok(())
}

async fn verify_auxiliary_failure_rollback(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(pool, &fixture).await?;
  sqlx::query(
    "ALTER TABLE audit_facts ADD CONSTRAINT reject_accept_trigger_audit \
     CHECK (operation <> 'accept-trigger')",
  )
  .execute(pool)
  .await?;

  let store = PostgresStore::new(pool.clone());
  assert_eq!(
    store.accept_trigger(fixture.request.clone()).await.unwrap_err(),
    StoreError::Unavailable
  );
  for table in [
    "trigger_occurrences",
    "builds",
    "attempts",
    "jobs",
    "ready_queue_entries",
    "idempotency_records",
    "audit_facts",
    "outbox_entries",
  ] {
    assert_eq!(
      mutation_record_count(pool, table).await?,
      0,
      "{table} leaked partial state"
    );
  }

  sqlx::query("ALTER TABLE audit_facts DROP CONSTRAINT reject_accept_trigger_audit")
    .execute(pool)
    .await?;
  assert_eq!(
    store.accept_trigger(fixture.request).await?.disposition,
    MutationDisposition::Applied
  );
  Ok(())
}

async fn mutation_record_count(pool: &PgPool, table: &str) -> Result<i64, sqlx::Error> {
  let query = sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}"));
  sqlx::query_scalar(query).fetch_one(pool).await
}

fn one_event(lease: LeaseAccess) -> AppendJobEvents {
  AppendJobEvents::new(
    lease,
    vec![
      DurableJobEvent::new(
        EventSequence::new(1).unwrap(),
        JobEventKind::new("progress").unwrap(),
        time(1_500),
        json!({"state": "running"}),
      )
      .unwrap(),
    ],
    time(1_600),
  )
  .unwrap()
}

fn time(milliseconds: i64) -> Timestamp {
  Timestamp::from_unix_millis(milliseconds).unwrap()
}

fn id<T>(value: u64) -> T
where
  T: FromStr,
  T::Err: Debug,
{
  format!("00000000-0000-0000-0000-{value:012x}").parse().unwrap()
}
