#[path = "support/authoritative_fixture.rs"]
mod authoritative_fixture;
mod support;

use std::{fmt::Debug, str::FromStr};

use authoritative_fixture::seed_authoritative_prerequisites;
use octacity_protocol::CachePolicy;
use octacity_server_cache::{CacheCredentialKey, CacheNamespace};
use octacity_server_domain::Timestamp;
use octacity_server_store::{
  JobClaim, JobClaimOutcome, JobExecutionStore as _, LeaseAccess, LeaseFence, LeaseWindow, TriggerAcceptanceStore as _,
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
