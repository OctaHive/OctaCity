#[path = "support/authoritative_fixture.rs"]
mod authoritative_fixture;
mod support;

use std::{fmt::Debug, str::FromStr};

use authoritative_fixture::seed_authoritative_prerequisites;
use octacity_protocol::CachePolicy;
use octacity_server_cache::{ActionResultV1, BlobDescriptor, BlobEncoding, CacheCredentialKey, CacheNamespace, Digest};
use octacity_server_domain::Timestamp;
use octacity_server_store::{
  BeginCacheSession, CacheBlobPreparationOutcome, CacheDataAccess, CacheDataStore as _, CachePublicationOutcome,
  CacheSessionStore as _, IdempotencyKey, JobClaim, JobClaimOutcome, JobExecutionStore as _, LeaseAccess, LeaseFence,
  LeaseWindow, PublishCacheAction, PublishCacheBlob, TriggerAcceptanceStore as _,
  testing::{
    CacheSessionStoreContractFixture, authoritative_store_contract_fixture, compatible_snapshot,
    job_spec_template_with_cache, verify_cache_session_store_contract,
  },
};
use octacity_server_store_postgres::{PostgresAuthoritativeStore, PostgresStore};
use serde_json::json;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn postgres_enforces_cache_namespace_fencing_and_revocation() {
  let database = support::TestDatabase::migrated().await;
  let result = verify_cache_sessions(&database).await;
  database.cleanup().await;
  result.unwrap();
}

async fn verify_cache_sessions(database: &support::TestDatabase) -> Result<(), Box<dyn std::error::Error>> {
  let namespace = CacheNamespace::new("project-cache").unwrap();
  let mut fixture = authoritative_store_contract_fixture();
  fixture.request.build.effective_policy_snapshot = json!({
    "project": {"policy": {
      "cache": {
        "namespaces": [namespace.to_string()],
        "read": true,
        "write": true,
        "max_bytes": 1_048_576
      },
      "retention": {"cache_seconds": 3_600}
    }}
  });
  fixture.request.jobs[0].job_spec_template = job_spec_template_with_cache(
    fixture.request.build.id,
    fixture.request.jobs[0].pipeline_node_id.as_str(),
    Some(CachePolicy {
      namespace: namespace.to_string(),
      read: true,
      write: true,
    }),
  );
  seed_authoritative_prerequisites(&database.pool, &fixture).await?;
  let execution = PostgresAuthoritativeStore::new(database.pool.clone(), support::test_signer());
  execution.accept_trigger(fixture.request.clone()).await?;
  let grant = match execution
    .claim_ready_job(JobClaim::new(
      id(970),
      LeaseFence::from_bytes([0x97; 32]),
      fixture.agent_id,
      fixture.registration_epoch,
      fixture.allowed_pool,
      compatible_snapshot(),
      LeaseWindow::new(time(1_000), time(253_402_300_799_000))?,
    )?)
    .await?
  {
    JobClaimOutcome::Claimed(grant) => *grant,
    JobClaimOutcome::Empty => return Err("cache-capable Job was not claimable".into()),
  };
  let access = LeaseAccess {
    lease_id: grant.lease_id,
    fence: grant.fence,
    agent_id: grant.agent_id,
    registration_epoch: grant.registration_epoch,
  };
  verify_l2_metadata(
    &PostgresStore::new(database.pool.clone()),
    access,
    grant.job_id,
    grant.attempt,
    &namespace,
  )
  .await?;
  verify_cache_session_store_contract(
    &PostgresStore::new(database.pool.clone()),
    CacheSessionStoreContractFixture {
      session_id: id(971),
      replay_session_id: id(972),
      lease: access,
      job_id: grant.job_id,
      attempt: grant.attempt,
      namespace,
      other_namespace: CacheNamespace::new("other-cache").unwrap(),
      created_at: time(1_100),
      expires_at: time(100_000),
      revoked_at: time(1_200),
      credential_key: CacheCredentialKey::new([0x98; 32]),
    },
  )
  .await;
  Ok(())
}

async fn verify_l2_metadata(
  store: &PostgresStore,
  lease: LeaseAccess,
  job_id: octacity_server_domain::JobId,
  attempt: octacity_server_domain::AttemptNumber,
  namespace: &CacheNamespace,
) -> Result<(), Box<dyn std::error::Error>> {
  let session_id = id(975);
  let credential = CacheCredentialKey::new([0x99; 32]).derive(session_id);
  store
    .begin_cache_session(BeginCacheSession {
      session_id,
      idempotency_key: IdempotencyKey::new("cache:l2").unwrap(),
      lease,
      job_id,
      attempt,
      requested: CachePolicy {
        namespace: namespace.to_string(),
        read: true,
        write: true,
      },
      credential_digest: credential.digest(),
      created_at: time(1_100),
      expires_at: time(10_000_000),
    })
    .await?;
  let access = |at| CacheDataAccess {
    credential_digest: credential.digest(),
    namespace: namespace.clone(),
    observed_at: time(at),
  };
  assert_eq!(
    store.resolve_cache_namespace(credential.digest(), time(1_200)).await?,
    *namespace
  );
  let bytes = b"cache bundle";
  let blob = BlobDescriptor {
    digest: Digest::blake3(bytes),
    encoding: BlobEncoding::Identity,
    encoded_size_bytes: bytes.len() as u64,
    expanded_size_bytes: bytes.len() as u64,
    entry_count: 1,
  };
  assert!(matches!(
    store.prepare_cache_blob(access(1_150), blob.clone()).await?,
    CacheBlobPreparationOutcome::Upload(_)
  ));
  assert_eq!(
    store
      .find_missing_cache_blobs(access(1_175), vec![blob.clone()])
      .await?,
    vec![blob.clone()]
  );
  assert_eq!(
    store
      .publish_cache_blob(PublishCacheBlob {
        access: access(1_200),
        blob: blob.clone(),
      })
      .await?,
    CachePublicationOutcome::Published
  );
  assert!(
    store
      .find_missing_cache_blobs(access(1_300), vec![blob.clone()])
      .await?
      .is_empty()
  );
  let action = Digest::blake3(b"opaque action");
  let result = ActionResultV1 {
    result_version: 1,
    action,
    output_bundle: Some(blob.clone()),
    stdout: None,
    task_outputs: Default::default(),
    artifacts: Vec::new(),
    reports: Vec::new(),
  };
  assert_eq!(
    store
      .publish_cache_action(PublishCacheAction {
        access: access(1_400),
        action,
        wire_size_bytes: serde_json::to_vec(&result)?.len() as u64,
        result: result.clone(),
      })
      .await?,
    CachePublicationOutcome::Published
  );
  assert_eq!(store.cache_action(access(1_500), action).await?, Some(result));

  let oversized = BlobDescriptor {
    digest: Digest::new(octacity_server_cache::DigestAlgorithm::Blake3, [0x44; 32], 2_000_000),
    encoding: BlobEncoding::Identity,
    encoded_size_bytes: 2_000_000,
    expanded_size_bytes: 2_000_000,
    entry_count: 1,
  };
  assert_eq!(
    store.prepare_cache_blob(access(1_600), oversized).await?,
    CacheBlobPreparationOutcome::QuotaExceeded
  );
  let isolated = CacheDataAccess {
    credential_digest: credential.digest(),
    namespace: CacheNamespace::new("other-cache").unwrap(),
    observed_at: time(1_700),
  };
  assert!(store.cache_action(isolated, action).await.is_err());

  let retained = store.prune_cache(access(3_602_000)).await?;
  assert_eq!(retained.deleted_blobs.len(), 1);
  assert_eq!(
    store
      .find_missing_cache_blobs(access(3_602_100), vec![blob.clone()])
      .await?,
    vec![blob]
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
