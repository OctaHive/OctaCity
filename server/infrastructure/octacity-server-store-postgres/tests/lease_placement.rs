#[path = "support/authoritative_fixture.rs"]
mod authoritative_fixture;
mod support;

use std::{fmt::Debug, str::FromStr, time::SystemTime};

use authoritative_fixture::seed_authoritative_prerequisites;
use octacity_server_domain::{LeaseId, Timestamp};
use octacity_server_scheduler::PoolDrainState;
use octacity_server_store::{
  AgentDrainMode, AgentPoolDefinition, AgentPoolStore as _, AgentStore as _, ClaimExpiredLeases, DrainAgent,
  IdempotencyKey, JobClaim, JobClaimOutcome, JobExecutionStore as _, LeaseAccess, LeaseFence, LeaseHeartbeatOutcome,
  LeaseHeartbeatStore as _, LeaseRecoveryAction, LeaseRecoveryStore as _, LeaseWindow, PoolAdmissionPolicy,
  PublishAgentPoolVersion, RecoverExpiredLease, RenewLease, StoreError, TriggerAcceptanceStore as _, WorkerOwner,
  testing::{StoreContractFixture, authoritative_store_contract_fixture},
};
use octacity_server_store_postgres::{PostgresAuthoritativeStore, PostgresStore};
use sha2::{Digest as _, Sha256};
use sqlx::{FromRow, PgPool};
use support::TestDatabase;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn a_waiting_resource_sample_cannot_cross_registration_revocation() {
  use octacity_server_store::{AgentResourceObservation, AgentResourceStore as _};
  let database = TestDatabase::migrated().await;
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  let mut registration = database.pool.begin().await.unwrap();
  sqlx::query("SELECT id FROM agents WHERE id = $1 FOR UPDATE")
    .bind(fixture.agent_id.as_uuid())
    .execute(&mut *registration)
    .await
    .unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let writer = store.clone();
  let observation = AgentResourceObservation {
    agent_id: fixture.agent_id,
    registration_epoch: fixture.registration_epoch,
    snapshot: octacity_server_store::testing::compatible_snapshot(),
    observed_at: system_time().unwrap(),
  };
  let write = tokio::spawn(async move { writer.record_agent_resources(observation).await });
  tokio::time::timeout(std::time::Duration::from_secs(5), async {
    loop {
      let waiting: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE datname = current_database() AND cardinality(pg_blocking_pids(pid)) > 0)")
        .fetch_one(&database.pool).await.unwrap();
      if waiting { break; }
      tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
  }).await.expect("observation must reach the held registration fence");
  sqlx::query("UPDATE agent_registrations SET revoked_at = now() WHERE agent_id = $1")
    .bind(fixture.agent_id.as_uuid())
    .execute(&mut *registration)
    .await
    .unwrap();
  sqlx::query("UPDATE agents SET version = version + 1 WHERE id = $1")
    .bind(fixture.agent_id.as_uuid())
    .execute(&mut *registration)
    .await
    .unwrap();
  registration.commit().await.unwrap();
  write.await.unwrap().unwrap();
  assert!(store.agent_detail(fixture.agent_id).await.unwrap().resources.is_none());
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn current_agent_resources_survive_restart_and_reject_stale_or_fenced_samples() {
  use octacity_server_store::{AgentResourceObservation, AgentResourceStore as _};
  let database = TestDatabase::migrated().await;
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let mut snapshot = octacity_server_store::testing::compatible_snapshot();
  snapshot.work_disk_free_bytes = 1234;
  snapshot.state_disk_free_bytes = 567;
  snapshot.backends = vec![octacity_protocol::BackendHealth {
    backend: "qualified-backend".into(),
    execution: None,
    status: octacity_protocol::BackendHealthStatus::Unavailable,
    message: None,
  }];
  let observation = AgentResourceObservation {
    agent_id: fixture.agent_id,
    registration_epoch: fixture.registration_epoch,
    snapshot,
    observed_at: system_time().unwrap(),
  };
  store.record_agent_resources(observation.clone()).await.unwrap();
  let resources = PostgresStore::new(database.pool.clone())
    .agent_detail(fixture.agent_id)
    .await
    .unwrap()
    .resources
    .unwrap();
  assert_eq!(resources.work_disk_free_bytes, 1234);
  assert_eq!(
    serde_json::to_value(&resources).unwrap()["registration_expires_at"],
    serde_json::json!(253402300799000_i64)
  );
  assert_eq!(resources.state_disk_free_bytes, 567);
  assert_eq!(
    serde_json::to_value(&resources).unwrap()["backends"],
    serde_json::json!([{"backend":"qualified-backend","status":"unavailable","message":null}])
  );
  let mut stale = observation.clone();
  stale.observed_at = Timestamp::from_unix_millis(observation.observed_at.unix_millis() - 1).unwrap();
  stale.snapshot.work_disk_free_bytes = 1;
  store.record_agent_resources(stale).await.unwrap();
  let mut fenced = observation.clone();
  fenced.registration_epoch = octacity_server_store::RegistrationEpoch::new(99).unwrap();
  fenced.snapshot.work_disk_free_bytes = 2;
  store.record_agent_resources(fenced).await.unwrap();
  assert_eq!(
    store
      .agent_detail(fixture.agent_id)
      .await
      .unwrap()
      .resources
      .unwrap()
      .work_disk_free_bytes,
    1234
  );
  use octacity_server_store::{
    AgentCredentialStore as _, AgentPlatform, AgentRegistrationInventory, AgentRegistrationProof, CredentialSecret,
    FreshRegistrationCredential, RegisterAgent, RegistrationValidity,
  };
  let previous = CredentialSecret::from_bytes([0x42; 32]);
  sqlx::query("UPDATE agent_registrations SET credential_hash = $1 WHERE agent_id = $2")
    .bind(previous.digest().as_bytes().to_vec())
    .bind(fixture.agent_id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  sqlx::query("UPDATE agents SET platform_operating_system = 'linux', platform_architecture = 'amd64' WHERE id = $1")
    .bind(fixture.agent_id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  let registered_at = add_millis(observation.observed_at, 1).unwrap();
  let registered = store
    .register_agent(
      RegisterAgent::new(
        FreshRegistrationCredential {
          id: id(9501),
          secret: CredentialSecret::from_bytes([0x43; 32]),
        },
        fixture.agent_id,
        octacity_server_domain::AgentName::new("contract-agent").unwrap(),
        AgentRegistrationProof::Registration {
          credential_id: id(601),
          epoch: fixture.registration_epoch,
          credential: previous,
        },
        AgentRegistrationInventory {
          platform: AgentPlatform::new("linux", "amd64").unwrap(),
          execution_contract_version: octacity_protocol::EXECUTION_CONTRACT_V1,
          inventory: octacity_server_store::testing::compatible_inventory(fixture.agent_id),
        },
        RegistrationValidity {
          registered_at,
          expires_at: add_millis(registered_at, 60000).unwrap(),
        },
      )
      .unwrap(),
    )
    .await
    .unwrap();
  assert!(
    store.agent_detail(fixture.agent_id).await.unwrap().resources.is_none(),
    "a new process cannot inherit prior backend health"
  );
  let mut old_process = observation.clone();
  old_process.observed_at = add_millis(registered_at, 1).unwrap();
  store.record_agent_resources(old_process).await.unwrap();
  assert!(store.agent_detail(fixture.agent_id).await.unwrap().resources.is_none());
  let mut current_process = observation;
  current_process.registration_epoch = registered.registration_epoch;
  current_process.observed_at = add_millis(registered_at, 2).unwrap();
  store.record_agent_resources(current_process).await.unwrap();
  assert!(store.agent_detail(fixture.agent_id).await.unwrap().resources.is_some());
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn agent_availability_preserves_registration_and_current_work_across_restart() {
  use octacity_server_domain::AgentVersion;
  use octacity_server_store::{SetAgentEnabled, testing::management_mutation};
  let database = TestDatabase::migrated().await;
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let execution = PostgresAuthoritativeStore::new(database.pool.clone(), support::test_signer());
  execution.accept_trigger(fixture.request.clone()).await.unwrap();
  let request = SetAgentEnabled {
    agent_id: fixture.agent_id,
    expected_version: AgentVersion::INITIAL,
    enabled: false,
    idempotency_key: IdempotencyKey::new("disable-agent").unwrap(),
    requested_at: system_time().unwrap(),
  };
  let disabled = store
    .set_agent_enabled(management_mutation(request.clone()))
    .await
    .unwrap();
  assert!(!disabled.agent.enabled);
  assert_eq!(disabled.agent.status, octacity_server_store::AgentStatus::Online);
  assert!(matches!(
    execution.claim_ready_job(placement_claim(&fixture)).await.unwrap(),
    JobClaimOutcome::Empty
  ));
  assert!(
    !PostgresStore::new(database.pool.clone())
      .agent_detail(fixture.agent_id)
      .await
      .unwrap()
      .agent
      .enabled
  );
  assert_eq!(
    store
      .set_agent_enabled(management_mutation(request.clone()))
      .await
      .unwrap()
      .disposition,
    octacity_server_store::MutationDisposition::Replayed
  );
  let enabled = store
    .set_agent_enabled(management_mutation(SetAgentEnabled {
      expected_version: disabled.agent.version,
      enabled: true,
      idempotency_key: IdempotencyKey::new("enable-agent").unwrap(),
      ..request.clone()
    }))
    .await
    .unwrap();
  assert!(enabled.agent.enabled);
  let now = system_time().unwrap();
  let current_claim = JobClaim::new(
    id(957),
    LeaseFence::from_bytes([0x57; 32]),
    fixture.agent_id,
    fixture.registration_epoch,
    fixture.allowed_pool,
    octacity_server_store::testing::compatible_snapshot(),
    LeaseWindow::new(now, add_millis(now, 60_000).unwrap()).unwrap(),
  )
  .unwrap();
  let grant = match execution.claim_ready_job(current_claim).await.unwrap() {
    JobClaimOutcome::Claimed(grant) => *grant,
    JobClaimOutcome::Empty => panic!("enabled Agent must receive the original ready Job"),
  };
  let disabled_again = store
    .set_agent_enabled(management_mutation(SetAgentEnabled {
      expected_version: enabled.agent.version,
      idempotency_key: IdempotencyKey::new("disable-active-agent").unwrap(),
      ..request.clone()
    }))
    .await
    .unwrap();
  assert!(!disabled_again.agent.enabled);
  assert_eq!(disabled_again.current_execution.as_ref().unwrap().job_id, grant.job_id);
  let access = LeaseAccess {
    lease_id: grant.lease_id,
    fence: grant.fence,
    agent_id: grant.agent_id,
    registration_epoch: grant.registration_epoch,
  };
  let now = system_time().unwrap();
  let mut renewal = heartbeat(
    "disabled-agent-heartbeat",
    access,
    &grant,
    now,
    add_millis(now, 60_000).unwrap(),
  );
  let mut snapshot = octacity_server_store::testing::compatible_snapshot();
  snapshot.work_disk_free_bytes = 777;
  renewal.resources = Some(snapshot);
  assert!(matches!(
    execution.renew_lease(renewal).await.unwrap(),
    LeaseHeartbeatOutcome::Continue { .. }
  ));
  assert_eq!(
    store
      .agent_detail(fixture.agent_id)
      .await
      .unwrap()
      .resources
      .unwrap()
      .work_disk_free_bytes,
    777
  );
  assert!(
    store
      .set_agent_enabled(management_mutation(SetAgentEnabled {
        enabled: true,
        ..request
      }))
      .await
      .is_err()
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn agent_counts_cover_visible_pool_members_before_pagination() {
  let database = TestDatabase::migrated().await;
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let page = store
    .list_agents(
      octacity_server_store::ListAgents::new(None, 1, octacity_server_store::AgentListVisibility::all())
        .unwrap()
        .for_pool(Some(fixture.allowed_pool)),
    )
    .await
    .unwrap();
  assert_eq!(page.agents.len(), 1);
  assert_eq!((page.active_count, page.total_count), (1, 3));
  let restricted = store
    .list_agents(
      octacity_server_store::ListAgents::new(
        None,
        1,
        octacity_server_store::AgentListVisibility::restricted([fixture.agent_id, fixture.expired_agent_id]).unwrap(),
      )
      .unwrap()
      .for_pool(Some(fixture.allowed_pool)),
    )
    .await
    .unwrap();
  assert_eq!((restricted.active_count, restricted.total_count), (1, 2));
  let drained = store
    .list_agents(
      octacity_server_store::ListAgents::new(None, 1, octacity_server_store::AgentListVisibility::all())
        .unwrap()
        .for_pool(Some(fixture.draining_pool)),
    )
    .await
    .unwrap();
  assert_eq!((drained.active_count, drained.total_count), (0, 1));
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn scheduling_does_not_depend_on_json_snapshot_shape() {
  let database = TestDatabase::migrated().await;
  let result = verify_scheduling_does_not_depend_on_json_snapshot_shape(&database.pool).await;
  database.cleanup().await;
  result.unwrap();
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn committed_ready_work_notifies_an_independent_connection() {
  let database = TestDatabase::migrated().await;
  let result = verify_cross_connection_ready_notification(&database.pool).await;
  database.cleanup().await;
  result.unwrap();
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn placement_binding_is_persisted_only_by_a_committed_claim() {
  let database = TestDatabase::migrated().await;
  let result = verify_atomic_placement_binding(&database.pool).await;
  database.cleanup().await;
  result.unwrap();
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn heartbeat_renews_and_returns_every_persisted_control_directive() {
  let database = TestDatabase::migrated().await;
  let result = verify_heartbeat_directives(&database.pool).await;
  database.cleanup().await;
  result.unwrap();
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn replicas_never_own_the_same_expired_lease_concurrently() {
  let database = TestDatabase::migrated().await;
  let result = verify_exclusive_expiry_claims(&database.pool).await;
  database.cleanup().await;
  result.unwrap();
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn agent_and_pool_drain_change_the_authoritative_heartbeat_directive() {
  let database = TestDatabase::migrated().await;
  let result = verify_drain_directives(&database.pool).await;
  database.cleanup().await;
  result.unwrap();
}

async fn verify_drain_directives(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(pool, &fixture).await?;
  let authoritative = PostgresAuthoritativeStore::new(pool.clone(), support::test_signer());
  let management = PostgresStore::new(pool.clone());
  authoritative.accept_trigger(fixture.request.clone()).await?;
  let observed_at = system_time()?;
  let claim = JobClaim::new(
    id(955),
    LeaseFence::from_bytes([0xa5; 32]),
    fixture.agent_id,
    fixture.registration_epoch,
    fixture.allowed_pool,
    octacity_server_store::testing::compatible_snapshot(),
    LeaseWindow::new(observed_at, add_millis(observed_at, 60_000)?)?,
  )?;
  let grant = match authoritative.claim_ready_job(claim).await? {
    JobClaimOutcome::Claimed(grant) => *grant,
    JobClaimOutcome::Empty => return Err("ready Job was not claimable".into()),
  };
  let access = LeaseAccess {
    lease_id: grant.lease_id,
    fence: grant.fence,
    agent_id: grant.agent_id,
    registration_epoch: grant.registration_epoch,
  };
  management
    .drain_agent(octacity_server_store::testing::management_mutation(DrainAgent {
      agent_id: fixture.agent_id,
      expected_version: octacity_server_domain::AgentVersion::INITIAL,
      mode: AgentDrainMode::Graceful,
      idempotency_key: IdempotencyKey::new("drain-agent-integration")?,
      requested_at: add_millis(observed_at, 1_000)?,
    }))
    .await?;
  let agent_drain_outcome = authoritative
    .renew_lease(heartbeat(
      "agent-drain-heartbeat",
      access,
      &grant,
      add_millis(observed_at, 2_000)?,
      add_millis(observed_at, 62_000)?,
    ))
    .await?;
  assert!(matches!(agent_drain_outcome, LeaseHeartbeatOutcome::Drain { .. }));

  management
    .publish_agent_pool_version(octacity_server_store::testing::management_mutation(
      PublishAgentPoolVersion {
        id: fixture.allowed_pool,
        expected_current_version: octacity_server_domain::PoolVersion::INITIAL,
        definition: AgentPoolDefinition {
          enabled: true,
          drain_state: PoolDrainState::ForcedDrain,
          admission_policy: PoolAdmissionPolicy::Any,
          concurrency_limit: 4,
          fairness_policy: octacity_server_store::PoolFairnessPolicy::PriorityFifo,
          static_capacity_limit: 4,
        },
        idempotency_key: IdempotencyKey::new("force-drain-pool-integration")?,
        published_at: add_millis(observed_at, 3_000)?,
      },
    ))
    .await?;
  assert_eq!(
    authoritative
      .renew_lease(heartbeat(
        "pool-drain-heartbeat",
        access,
        &grant,
        add_millis(observed_at, 4_000)?,
        add_millis(observed_at, 64_000)?,
      ))
      .await?,
    LeaseHeartbeatOutcome::Cancel
  );
  Ok(())
}

async fn verify_scheduling_does_not_depend_on_json_snapshot_shape(
  pool: &PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(pool, &fixture).await?;
  let store = PostgresAuthoritativeStore::new(pool.clone(), support::test_signer());
  store.accept_trigger(fixture.request.clone()).await?;
  sqlx::query("UPDATE builds SET effective_policy_snapshot = '{}'::jsonb WHERE id = $1")
    .bind(fixture.request.build.id.as_uuid())
    .execute(pool)
    .await?;
  assert!(matches!(
    store.claim_ready_job(placement_claim(&fixture)).await?,
    JobClaimOutcome::Claimed(_)
  ));
  Ok(())
}

async fn verify_cross_connection_ready_notification(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(pool, &fixture).await?;
  let mut listener = sqlx::postgres::PgListener::connect_with(pool).await?;
  listener
    .listen(octacity_server_store_postgres::READY_JOB_NOTIFICATION_CHANNEL)
    .await?;
  let store = PostgresAuthoritativeStore::new(pool.clone(), support::test_signer());
  store.accept_trigger(fixture.request).await?;
  tokio::time::timeout(std::time::Duration::from_secs(2), listener.recv()).await??;
  Ok(())
}

async fn verify_exclusive_expiry_claims(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(pool, &fixture).await?;
  sqlx::query(
    "UPDATE build_configuration_versions \
     SET retry_max_attempts = 2, retries_infrastructure = true \
     WHERE build_configuration_id = $1 AND version = $2",
  )
  .bind(fixture.request.build.configuration_id.as_uuid())
  .bind(i64::try_from(fixture.request.build.configuration_version.get())?)
  .execute(pool)
  .await?;
  let left = PostgresAuthoritativeStore::new(independent_pool(pool).await, support::test_signer());
  let right = PostgresAuthoritativeStore::new(independent_pool(pool).await, support::test_signer());
  left.accept_trigger(fixture.request.clone()).await?;
  let grant = match left.claim_ready_job(placement_claim(&fixture)).await? {
    JobClaimOutcome::Claimed(grant) => *grant,
    JobClaimOutcome::Empty => return Err("ready Job was not claimable".into()),
  };
  let left_claim = ClaimExpiredLeases::new(WorkerOwner::new("replica-left")?, time(20_000), time(30_000), 1)?;
  let right_claim = ClaimExpiredLeases::new(WorkerOwner::new("replica-right")?, time(20_000), time(30_000), 1)?;
  let (left_claims, right_claims) = tokio::join!(
    left.claim_expired_leases(left_claim),
    right.claim_expired_leases(right_claim)
  );
  let mut claims = left_claims?.into_iter().chain(right_claims?).collect::<Vec<_>>();
  assert_eq!(claims.len(), 1);
  let abandoned = claims.pop().unwrap();
  assert_eq!(abandoned.lease_id, grant.lease_id);
  let restarted = PostgresAuthoritativeStore::new(independent_pool(pool).await, support::test_signer());
  assert!(
    restarted
      .claim_expired_leases(ClaimExpiredLeases::new(
        WorkerOwner::new("replica-before-claim-expiry")?,
        time(29_999),
        time(40_000),
        1,
      )?)
      .await?
      .is_empty(),
    "a live Lease-recovery claim must survive owner process loss"
  );
  let mut replacement_claims = restarted
    .claim_expired_leases(ClaimExpiredLeases::new(
      WorkerOwner::new("replica-replacement")?,
      time(30_000),
      time(40_000),
      1,
    )?)
    .await?;
  assert_eq!(replacement_claims.len(), 1);
  let replacement_claim = replacement_claims.pop().unwrap();
  assert_eq!(replacement_claim.lease_id, grant.lease_id);
  assert!(matches!(
    restarted
      .recover_expired_lease(RecoverExpiredLease {
        claim: abandoned,
        recovered_at: time(30_001),
      })
      .await,
    Err(StoreError::Conflict {
      entity: octacity_server_domain::EntityKind::Lease
    })
  ));
  let recovered = restarted
    .recover_expired_lease(RecoverExpiredLease {
      claim: replacement_claim,
      recovered_at: time(30_001),
    })
    .await?;
  assert_eq!(recovered.action, LeaseRecoveryAction::Requeued);
  let access = LeaseAccess {
    lease_id: grant.lease_id,
    fence: grant.fence,
    agent_id: grant.agent_id,
    registration_epoch: grant.registration_epoch,
  };
  assert_eq!(
    restarted
      .renew_lease(heartbeat(
        "expired-owner-is-fenced",
        access,
        &grant,
        time(30_002),
        time(40_002),
      ))
      .await?,
    LeaseHeartbeatOutcome::Fenced
  );

  let second_claim = JobClaim::new(
    id(957),
    LeaseFence::from_bytes([0xc7; 32]),
    fixture.agent_id,
    fixture.registration_epoch,
    fixture.allowed_pool,
    octacity_server_store::testing::compatible_snapshot(),
    LeaseWindow::new(time(31_000), time(32_000))?,
  )?;
  let second_grant = match restarted.claim_ready_job(second_claim).await? {
    JobClaimOutcome::Claimed(grant) => *grant,
    JobClaimOutcome::Empty => return Err("requeued Job was not claimable".into()),
  };
  let claims = restarted
    .claim_expired_leases(ClaimExpiredLeases::new(
      WorkerOwner::new("replica-left")?,
      time(33_000),
      time(40_000),
      1,
    )?)
    .await?;
  assert_eq!(claims.len(), 1);
  assert_eq!(claims[0].lease_id, second_grant.lease_id);
  let exhausted = restarted
    .recover_expired_lease(RecoverExpiredLease {
      claim: claims[0].clone(),
      recovered_at: time(33_001),
    })
    .await?;
  assert_eq!(exhausted.action, LeaseRecoveryAction::Failed);
  assert_eq!(exhausted.infrastructure_requeues, 1);
  Ok(())
}

async fn independent_pool(source: &PgPool) -> PgPool {
  PgPool::connect_with((*source.connect_options()).clone())
    .await
    .expect("connect an independent server pool to the test database")
}

async fn verify_heartbeat_directives(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(pool, &fixture).await?;
  let store = PostgresAuthoritativeStore::new(pool.clone(), support::test_signer());
  store.accept_trigger(fixture.request.clone()).await?;
  let observed_at = system_time()?;
  let claim = JobClaim::new(
    id(956),
    LeaseFence::from_bytes([0xb6; 32]),
    fixture.agent_id,
    fixture.registration_epoch,
    fixture.allowed_pool,
    octacity_server_store::testing::compatible_snapshot(),
    LeaseWindow::new(observed_at, add_millis(observed_at, 60_000)?)?,
  )?;
  let grant = match store.claim_ready_job(claim).await? {
    JobClaimOutcome::Claimed(grant) => *grant,
    JobClaimOutcome::Empty => return Err("ready Job was not claimable".into()),
  };
  let access = LeaseAccess {
    lease_id: grant.lease_id,
    fence: grant.fence,
    agent_id: grant.agent_id,
    registration_epoch: grant.registration_epoch,
  };
  let renewed_at = add_millis(observed_at, 1_000)?;
  let renewed_until = add_millis(observed_at, 121_000)?;
  let active = heartbeat("heartbeat-active", access, &grant, renewed_at, renewed_until);
  assert_eq!(
    store.renew_lease(active.clone()).await?,
    LeaseHeartbeatOutcome::Continue {
      expires_at: renewed_until
    }
  );
  let mut replay = active;
  replay.observed_at = add_millis(observed_at, 2_000)?;
  replay.expires_at = add_millis(observed_at, 122_000)?;
  assert_eq!(
    store.renew_lease(replay).await?,
    LeaseHeartbeatOutcome::Continue {
      expires_at: renewed_until
    },
    "an identical heartbeat identity must replay its committed deadline"
  );

  set_lease_state(pool, grant.lease_id, "cancellation_requested").await?;
  assert_eq!(
    store
      .renew_lease(heartbeat(
        "heartbeat-cancel",
        access,
        &grant,
        add_millis(observed_at, 3_000)?,
        add_millis(observed_at, 123_000)?,
      ))
      .await?,
    LeaseHeartbeatOutcome::Cancel
  );
  set_lease_state(pool, grant.lease_id, "drain_requested").await?;
  let drain_until = add_millis(observed_at, 124_000)?;
  assert_eq!(
    store
      .renew_lease(heartbeat(
        "heartbeat-drain",
        access,
        &grant,
        add_millis(observed_at, 4_000)?,
        drain_until,
      ))
      .await?,
    LeaseHeartbeatOutcome::Drain {
      expires_at: drain_until
    }
  );
  let mut fenced = heartbeat(
    "heartbeat-fenced",
    access,
    &grant,
    add_millis(observed_at, 5_000)?,
    add_millis(observed_at, 125_000)?,
  );
  fenced.lease.fence = LeaseFence::from_bytes([0xff; 32]);
  assert_eq!(store.renew_lease(fenced).await?, LeaseHeartbeatOutcome::Fenced);
  Ok(())
}

async fn verify_atomic_placement_binding(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(pool, &fixture).await?;
  let store = PostgresAuthoritativeStore::new(pool.clone(), support::test_signer());
  store.accept_trigger(fixture.request.clone()).await?;

  sqlx::query(
    "ALTER TABLE audit_facts ADD CONSTRAINT reject_claim_ready_job_audit \
     CHECK (operation <> 'claim-ready-job')",
  )
  .execute(pool)
  .await?;

  let claim = placement_claim(&fixture);
  assert_eq!(
    store.claim_ready_job(claim.clone()).await.unwrap_err(),
    StoreError::Unavailable
  );
  assert_uncommitted_claim_is_invisible(pool, &fixture, claim.lease_id).await?;

  sqlx::query("ALTER TABLE audit_facts DROP CONSTRAINT reject_claim_ready_job_audit")
    .execute(pool)
    .await?;
  let grant = match store.claim_ready_job(claim.clone()).await? {
    JobClaimOutcome::Claimed(grant) => *grant,
    JobClaimOutcome::Empty => return Err("rolled-back ready Job was not claimable".into()),
  };

  let persisted: PersistedLeaseBinding = sqlx::query_as(
    "SELECT pool_id, registration_epoch, registration_id, fence_hash, \
            FLOOR(EXTRACT(EPOCH FROM leased_at) * 1000)::BIGINT AS leased_at_millis, \
            FLOOR(EXTRACT(EPOCH FROM expires_at) * 1000)::BIGINT AS expires_at_millis \
     FROM leases WHERE id = $1",
  )
  .bind(grant.lease_id.as_uuid())
  .fetch_one(pool)
  .await?;
  assert_eq!(persisted.pool_id, fixture.allowed_pool.as_uuid());
  assert_eq!(
    persisted.registration_epoch,
    i64::try_from(fixture.registration_epoch.get())?
  );
  assert_eq!(persisted.registration_id, uuid::Uuid::from_u128(601));
  assert_eq!(persisted.leased_at_millis, claim.claimed_at.unix_millis());
  assert_eq!(persisted.expires_at_millis, claim.expires_at.unix_millis());
  assert_eq!(persisted.fence_hash, Sha256::digest(claim.fence.expose()).to_vec());
  assert_ne!(persisted.fence_hash, claim.fence.expose().to_vec());
  assert_eq!(format!("{:?}", grant.fence), "LeaseFence([REDACTED])");
  Ok(())
}

async fn assert_uncommitted_claim_is_invisible(
  pool: &PgPool,
  fixture: &StoreContractFixture,
  lease_id: LeaseId,
) -> Result<(), sqlx::Error> {
  let (leases, queued, ready, idempotency, audit, outbox): (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
    "SELECT \
       (SELECT COUNT(*) FROM leases WHERE id = $1), \
       (SELECT COUNT(*) FROM ready_queue_entries WHERE job_id = $2), \
       (SELECT COUNT(*) FROM jobs WHERE id = $2 AND state = 'ready'), \
       (SELECT COUNT(*) FROM idempotency_records WHERE scope = 'claim-ready-job' AND idempotency_key = $3), \
       (SELECT COUNT(*) FROM audit_facts WHERE operation = 'claim-ready-job'), \
       (SELECT COUNT(*) FROM outbox_entries WHERE topic = 'job.claimed')",
  )
  .bind(lease_id.as_uuid())
  .bind(fixture.request.jobs[0].id.as_uuid())
  .bind(lease_id.to_string())
  .fetch_one(pool)
  .await?;
  assert_eq!(leases, 0, "a failed commit exposed a Lease row");
  assert_eq!(queued, 1, "a failed commit removed the ready queue entry");
  assert_eq!(ready, 1, "a failed commit changed the Job state");
  assert_eq!((idempotency, audit, outbox), (0, 0, 0));
  Ok(())
}

#[derive(FromRow)]
struct PersistedLeaseBinding {
  pool_id: uuid::Uuid,
  registration_epoch: i64,
  registration_id: uuid::Uuid,
  fence_hash: Vec<u8>,
  leased_at_millis: i64,
  expires_at_millis: i64,
}

fn placement_claim(fixture: &StoreContractFixture) -> JobClaim {
  JobClaim::new(
    id(955),
    LeaseFence::from_bytes([0xa5; 32]),
    fixture.agent_id,
    fixture.registration_epoch,
    fixture.allowed_pool,
    octacity_server_store::testing::compatible_snapshot(),
    LeaseWindow::new(time(1_000), time(10_000)).unwrap(),
  )
  .unwrap()
}

fn heartbeat(
  key: &str,
  lease: LeaseAccess,
  grant: &octacity_server_store::LeaseGrant,
  observed_at: Timestamp,
  expires_at: Timestamp,
) -> RenewLease {
  RenewLease {
    resources: None,
    idempotency_key: IdempotencyKey::new(key).unwrap(),
    lease,
    job_id: grant.job_id,
    attempt: grant.attempt,
    observed_at,
    expires_at,
  }
}

async fn set_lease_state(pool: &PgPool, lease_id: LeaseId, state: &str) -> Result<(), sqlx::Error> {
  sqlx::query("UPDATE leases SET state = $1, version = version + 1 WHERE id = $2")
    .bind(state)
    .bind(lease_id.as_uuid())
    .execute(pool)
    .await?;
  Ok(())
}

fn system_time() -> Result<Timestamp, Box<dyn std::error::Error>> {
  let millis = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_millis();
  Ok(Timestamp::from_unix_millis(i64::try_from(millis)?)?)
}

fn add_millis(value: Timestamp, milliseconds: i64) -> Result<Timestamp, Box<dyn std::error::Error>> {
  Ok(Timestamp::from_unix_millis(
    value
      .unix_millis()
      .checked_add(milliseconds)
      .ok_or("timestamp overflow")?,
  )?)
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
