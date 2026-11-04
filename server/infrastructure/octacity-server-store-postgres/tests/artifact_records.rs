use support::authoritative_fixture;
mod support;

use std::{fmt::Debug, str::FromStr};

use authoritative_fixture::seed_authoritative_prerequisites;
use octacity_server_domain::{ArtifactName, Timestamp};
use octacity_server_store::{
  ArtifactContentDigest, ArtifactIdentity, ArtifactListVisibility, ArtifactMediaType, ArtifactRecordStore as _,
  ArtifactRetentionPolicy, ArtifactType, ArtifactVerificationResult, BeginArtifactUpload, IdempotencyKey, JobClaim,
  JobClaimOutcome, JobExecutionStore as _, LeaseAccess, LeaseFence, LeaseWindow, ListPublishedArtifacts,
  RestoreInventoryStore as _, TriggerAcceptanceStore as _, VerifyArtifactUpload,
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
  sqlx::query(
    "UPDATE jobs SET job_spec_template = jsonb_set(job_spec_template, '{policy,outputs}', \
     '{\"artifact_count\":16,\"artifact_bytes\":1048576,\"report_count\":16,\"report_bytes\":1048576,\"single_output_bytes\":1048576}'::jsonb) \
     WHERE attempt_id = $1",
  )
  .bind(fixture.request.attempt_id.as_uuid())
  .execute(&database.pool)
  .await?;
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
  let store = PostgresStore::new(database.pool.clone());
  let all = store
    .list_published_artifacts(ListPublishedArtifacts {
      attempt_id: None,
      build_id: fixture.request.build.id,
      limit: 1,
      visibility: ArtifactListVisibility::all(),
    })
    .await?;
  assert_eq!(all.len(), 1);
  assert_eq!(all[0].artifact.identity().artifact_id, id(953));
  let restricted = store
    .list_published_artifacts(ListPublishedArtifacts {
      attempt_id: None,
      build_id: fixture.request.build.id,
      limit: 1,
      visibility: ArtifactListVisibility::restricted([id(953)])?,
    })
    .await?;
  assert_eq!(restricted, all);
  for visibility in [
    ArtifactListVisibility::restricted([id(999)])?,
    ArtifactListVisibility::none(),
  ] {
    assert!(
      store
        .list_published_artifacts(ListPublishedArtifacts {
          attempt_id: None,
          build_id: fixture.request.build.id,
          limit: 1,
          visibility,
        })
        .await?
        .is_empty()
    );
  }
  let restored = PostgresStore::new(database.pool.clone())
    .restore_artifact_page(None, 1)
    .await?;
  assert_eq!(restored.items.len(), 1);
  assert_eq!(restored.items[0].artifact.identity().artifact_id, id(953));
  assert_eq!(restored.next_after, None);
  let media_type = ArtifactMediaType::new("application/octet-stream")?;
  let upload = store
    .begin_artifact_upload(BeginArtifactUpload {
      artifact_id: id(960),
      upload_id: id(961),
      idempotency_key: IdempotencyKey::new("protected-inventory:payload")?,
      lease: access,
      job_id: grant.job_id,
      attempt: grant.attempt,
      logical_name: ArtifactName::new("inventory.json")?,
      producer_run_id: 17,
      producer_task_id: 23,
      artifact_type: ArtifactType::Artifact,
      media_type: media_type.clone(),
      transport_media_type: media_type,
      size_bytes: 42,
      digest: ArtifactContentDigest::from_bytes([0x43; 32]),
      reserved_at: time(2_000),
      capability_expires_at: time(3_000),
    })
    .await?;
  let verification = VerifyArtifactUpload {
    upload_id: upload.upload.upload_id,
    lease: access,
    job_id: grant.job_id,
    attempt: grant.attempt,
    observed_at: time(2_100),
  };
  assert!(
    !store
      .has_published_artifact_from_producer(grant.job_id, 17, 23, "protected-inventory:")
      .await?
  );
  store.begin_artifact_verification(verification).await?;
  store
    .finish_artifact_verification(verification, ArtifactVerificationResult::Verified)
    .await?;
  assert!(
    store
      .has_published_artifact_from_producer(grant.job_id, 17, 23, "protected-inventory:")
      .await?
  );
  assert!(
    !store
      .has_published_artifact_from_producer(grant.job_id, 17, 24, "protected-inventory:")
      .await?
  );
  assert!(
    !store
      .has_published_artifact_from_producer(grant.job_id, 17, 23, "output:")
      .await?
  );
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
