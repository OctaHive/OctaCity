#[path = "support/authoritative_fixture.rs"]
mod authoritative_fixture;
mod support;

use std::{fmt::Debug, str::FromStr};

use authoritative_fixture::seed_authoritative_prerequisites;
use octacity_server_domain::{ArtifactName, Timestamp};
use octacity_server_store::{
  ArtifactContentDigest, ArtifactIdentity, ArtifactMediaType, ArtifactRetentionPolicy, ArtifactType, JobClaim,
  JobClaimOutcome, JobExecutionStore as _, LeaseAccess, LeaseFence, LeaseWindow, TriggerAcceptanceStore as _,
  testing::{
    ArtifactRecordStoreContractFixture, ArtifactUploadStoreContractFixture, authoritative_store_contract_fixture,
    compatible_snapshot, verify_artifact_record_store_contract, verify_artifact_upload_store_contract,
  },
};
use octacity_server_store_postgres::{PostgresAuthoritativeStore, PostgresStore};

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn postgres_enforces_artifact_identity_fencing_lifecycle_and_uniqueness() {
  let database = support::TestDatabase::migrated().await;
  let result = verify_artifact_records(&database).await;
  database.cleanup().await;
  result.unwrap();
}

async fn verify_artifact_records(database: &support::TestDatabase) -> Result<(), Box<dyn std::error::Error>> {
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(&database.pool, &fixture).await?;
  let execution = PostgresAuthoritativeStore::new(database.pool.clone(), support::test_signer());
  execution.accept_trigger(fixture.request.clone()).await?;
  let grant = match execution
    .claim_ready_job(JobClaim::new(
      id(950),
      LeaseFence::from_bytes([0x95; 32]),
      fixture.agent_id,
      fixture.registration_epoch,
      fixture.allowed_pool,
      compatible_snapshot(),
      LeaseWindow::new(time(1_000), time(253_402_300_799_000))?,
    )?)
    .await?
  {
    JobClaimOutcome::Claimed(grant) => *grant,
    JobClaimOutcome::Empty => return Err("seeded root Job was not claimable".into()),
  };
  let access = LeaseAccess {
    lease_id: grant.lease_id,
    fence: grant.fence,
    agent_id: grant.agent_id,
    registration_epoch: grant.registration_epoch,
  };
  let identity = ArtifactIdentity {
    artifact_id: id(951),
    build_id: fixture.request.build.id,
    attempt_id: fixture.request.attempt_id,
    job_id: grant.job_id,
    lease_id: grant.lease_id,
    logical_name: ArtifactName::new("dist/result.tar").unwrap(),
    artifact_type: ArtifactType::Artifact,
    media_type: ArtifactMediaType::new("application/x-tar").unwrap(),
    size_bytes: 42,
    digest: ArtifactContentDigest::from_bytes([0xaa; 32]),
    retention: ArtifactRetentionPolicy::DeleteAfter(time(5_000)),
  };
  verify_artifact_record_store_contract(
    &PostgresStore::new(database.pool.clone()),
    ArtifactRecordStoreContractFixture {
      identity,
      duplicate_artifact_id: id(952),
      lease: access,
      reserved_at: time(1_100),
      verification_at: time(1_300),
      publication_at: time(1_400),
      early_expiry_at: time(4_999),
      expiry_at: time(5_000),
      deletion_at: time(5_100),
    },
  )
  .await;
  verify_artifact_upload_store_contract(
    &PostgresStore::new(database.pool.clone()),
    ArtifactUploadStoreContractFixture {
      artifact_id: id(953),
      replay_artifact_id: id(954),
      upload_id: id(955),
      replay_upload_id: id(956),
      lease: access,
      job_id: grant.job_id,
      attempt: grant.attempt,
      build_id: fixture.request.build.id,
      reserved_at: time(1_600),
      capability_expires_at: time(2_600),
      verification_at: time(1_700),
      rejection_at: time(1_800),
      publication_at: time(1_900),
    },
  )
  .await;
  Ok(())
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
