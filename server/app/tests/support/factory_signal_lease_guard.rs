//! Native transaction authority after the application's earlier lease read.
use super::*;
use octacity_server_store_postgres::PostgresAuthoritativeStore;
use sha2::Digest as _;

async fn retained_assignment(
  database: &postgres_support::TestDatabase,
  build: &AcceptFactoryBuild,
) -> (PostgresAuthoritativeStore, LeaseAccess, JobId) {
  let authority = PostgresAuthoritativeStore::new(database.pool.clone(), postgres_support::test_signer());
  let job = build.jobs.iter().find(|j| j.dependencies.is_empty()).unwrap();
  authority.accept_factory_build(build.clone()).await.unwrap();
  let agent = AgentId::generate();
  let registration = uuid::Uuid::new_v4();
  let access = LeaseAccess {
    lease_id: LeaseId::generate(),
    fence: LeaseFence::from_bytes([81; 32]),
    agent_id: agent,
    registration_epoch: RegistrationEpoch::new(1).unwrap(),
  };
  // Ordinary placement has separate contracts. Reuse its retained assignment
  // shape so this test can hold a deterministic logical Node clock while the
  // database independently checks registration and Lease expiry.
  sqlx::query("INSERT INTO agents (id,name,pool_id,pool_version,state,inventory,version,created_at,updated_at) VALUES ($1,'signal-lease-fixture',$2,1,'online',$3,1,to_timestamp(0),to_timestamp(0))")
    .bind(agent.as_uuid()).bind(job.allowed_pools[0].as_uuid()).bind(sqlx::types::Json(octacity_server_store::testing::compatible_inventory(agent))).execute(&database.pool).await.unwrap();
  sqlx::query("INSERT INTO agent_registrations (id,agent_id,epoch,credential_hash,inventory,registered_at,expires_at) VALUES ($1,$2,1,$3,$4,to_timestamp(0),to_timestamp(253402300799))")
    .bind(registration).bind(agent.as_uuid()).bind(vec![81_u8;32]).bind(sqlx::types::Json(octacity_server_store::testing::compatible_inventory(agent))).execute(&database.pool).await.unwrap();
  sqlx::query("INSERT INTO leases (id,job_id,pool_id,pool_version,registration_id,registration_epoch,fence_hash,state,version,leased_at,expires_at) VALUES ($1,$2,$3,1,$4,1,$5,'active',1,to_timestamp(0),to_timestamp(253402300799))")
    .bind(access.lease_id.as_uuid()).bind(job.id.as_uuid()).bind(job.allowed_pools[0].as_uuid()).bind(registration).bind(sha2::Sha256::digest(access.fence.expose()).to_vec()).execute(&database.pool).await.unwrap();
  sqlx::query("UPDATE jobs SET state='leased' WHERE id=$1")
    .bind(job.id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  sqlx::query("DELETE FROM ready_queue_entries WHERE job_id=$1")
    .bind(job.id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  (authority, access, job.id)
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn signal_reservation_rejects_a_lease_that_ended_after_the_earlier_application_check() {
  let (database, store, build) = prepared_factory_build(true).await;
  let (authority, access, job_id) = retained_assignment(&database, &build).await;
  authority
    .authorize_tool_action_lease(access, job_id, AttemptNumber::FIRST, at(12))
    .await
    .unwrap();
  authority
    .complete_job(JobCompletion {
      completion_id: IdempotencyKey::new("lease-ended-before-signal").unwrap(),
      lease: access,
      final_sequence: None,
      kind: JobCompletionKind::Cancelled,
      execution: None,
      completed_at: at(13),
    })
    .await
    .unwrap();
  let request = decision_signal_fixture::tool_risk_request(build.intent.input(), build.intent.node());
  let reservation = ReserveFactoryDecisionSignalRequest {
    request: request.clone(),
    ownership: build.ownership,
    observed_at: at(14),
    reservation_limit: None,
    lease_guard: Some(FactoryDecisionSignalLeaseGuard {
      lease: access,
      job_id,
      attempt: AttemptNumber::FIRST,
    }),
  };
  assert!(
    store
      .reserve_factory_decision_signal_request(reservation)
      .await
      .is_err(),
    "a earlier read cannot authorize provider I/O after the actual lease completes"
  );
  assert!(!store.factory_decision_signal_request_reserved(&request).await.unwrap());
  assert_eq!(
    store
      .factory_decision_signal_node_usage(build.intent.input().factory_run_id(), build.intent.node().id())
      .await
      .unwrap(),
    FactoryDecisionSignalNodeUsage::default()
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn direct_tool_risk_reservation_cannot_omit_ordinary_lease_authority() {
  let (database, store, build) = prepared_factory_build(true).await;
  let request = decision_signal_fixture::tool_risk_request(build.intent.input(), build.intent.node());
  assert!(
    store
      .reserve_factory_decision_signal_request(ReserveFactoryDecisionSignalRequest {
        request: request.clone(),
        ownership: build.ownership,
        observed_at: at(12),
        reservation_limit: None,
        lease_guard: None
      })
      .await
      .is_err()
  );
  assert!(!store.factory_decision_signal_request_reserved(&request).await.unwrap());
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn direct_tool_risk_reservation_requires_the_signed_tool_risk_mode() {
  for mode in [None, Some(octacity_protocol::FactoryToolControlModeV3::Deterministic)] {
    let (database, store, build) = prepared_factory_build_with_tool_control(true, 10_000, None, mode).await;
    build.validate().unwrap();
    let (_, access, job_id) = retained_assignment(&database, &build).await;
    let request = decision_signal_fixture::tool_risk_request(build.intent.input(), build.intent.node());
    assert!(
      store
        .reserve_factory_decision_signal_request(ReserveFactoryDecisionSignalRequest {
          request: request.clone(),
          ownership: build.ownership,
          observed_at: at(12),
          reservation_limit: None,
          lease_guard: Some(FactoryDecisionSignalLeaseGuard {
            lease: access,
            job_id,
            attempt: AttemptNumber::FIRST
          })
        })
        .await
        .is_err(),
      "a current readonly/deterministic lease cannot authorize provider I/O through the direct port"
    );
    assert!(!store.factory_decision_signal_request_reserved(&request).await.unwrap());
    database.cleanup().await;
  }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn signed_tool_risk_lease_reserves_the_exact_common_node_under_native_authority() {
  let (database, store, build) = prepared_factory_build_with_tool_control(
    true,
    10_000,
    None,
    Some(octacity_protocol::FactoryToolControlModeV3::ToolRisk),
  )
  .await;
  build.validate().unwrap();
  let (_, access, job_id) = retained_assignment(&database, &build).await;
  let job = store.job(job_id).await.unwrap();
  assert_eq!(job.state, octacity_server_job::JobState::Leased);
  let attempt = store.attempt(job.attempt_id).await.unwrap();
  // Native acceptance starts the Build and Attempt immediately; the assigned
  // Job still awaits the asynchronous Agent Running event.
  assert_eq!(attempt.state, octacity_server_orchestrator::AttemptState::Running);
  assert_eq!(
    store.build(attempt.build_id).await.unwrap().state,
    octacity_server_orchestrator::BuildState::Running
  );
  let request = decision_signal_fixture::tool_risk_request(build.intent.input(), build.intent.node());
  assert_eq!(
    store
      .reserve_factory_decision_signal_request(ReserveFactoryDecisionSignalRequest {
        request: request.clone(),
        ownership: build.ownership,
        observed_at: at(12),
        reservation_limit: Some(request.request().budget()),
        lease_guard: Some(FactoryDecisionSignalLeaseGuard {
          lease: access,
          job_id,
          attempt: AttemptNumber::FIRST
        })
      })
      .await
      .unwrap(),
    FactoryDecisionSignalRequestDisposition::Reserved
  );
  assert!(store.factory_decision_signal_request_reserved(&request).await.unwrap());
  assert_eq!(
    store
      .factory_decision_signal_node_usage(build.intent.input().factory_run_id(), build.intent.node().id())
      .await
      .unwrap()
      .pending
      .tokens,
    100
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn signal_reservation_waiting_on_run_lock_uses_fresh_lease_expiry_at_commit() {
  let (database, store, build) = prepared_factory_build_with_tool_control(
    true,
    10_000,
    None,
    Some(octacity_protocol::FactoryToolControlModeV3::ToolRisk),
  )
  .await;
  let (authority, access, job_id) = retained_assignment(&database, &build).await;
  authority
    .authorize_tool_action_lease(access, job_id, AttemptNumber::FIRST, at(12))
    .await
    .unwrap();
  let request = decision_signal_fixture::tool_risk_request(build.intent.input(), build.intent.node());
  let reservation = ReserveFactoryDecisionSignalRequest {
    request: request.clone(),
    ownership: build.ownership,
    observed_at: at(12),
    reservation_limit: None,
    lease_guard: Some(FactoryDecisionSignalLeaseGuard {
      lease: access,
      job_id,
      attempt: AttemptNumber::FIRST,
    }),
  };
  let mut locked = database.pool.begin().await.unwrap();
  sqlx::query("SELECT id FROM factory_runs WHERE id=$1 FOR UPDATE")
    .bind(build.intent.input().factory_run_id().as_uuid())
    .fetch_one(&mut *locked)
    .await
    .unwrap();
  let pending = tokio::spawn({
    let store = store.clone();
    async move { store.reserve_factory_decision_signal_request(reservation).await }
  });
  // Observe the actual database lock wait, rather than assuming scheduler speed
  // or using a sleep to place the expiry inside the transaction window.
  tokio::time::timeout(std::time::Duration::from_secs(30),async {
    loop {
      let waiting:bool=sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE datname=$1 AND state='active' AND wait_event_type='Lock' AND query LIKE '%SELECT id FROM factory_runs%')").bind(&database.name).fetch_one(&database.pool).await.unwrap();
      if waiting {break;}
      tokio::task::yield_now().await;
    }
  }).await.expect("reservation must wait on the held Run row");
  sqlx::query("UPDATE leases SET expires_at=clock_timestamp() WHERE id=$1")
    .bind(access.lease_id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  locked.commit().await.unwrap();
  let result = pending.await.unwrap();
  let reserved = store.factory_decision_signal_request_reserved(&request).await.unwrap();
  database.cleanup().await;
  assert!(
    result.is_err(),
    "transaction-start now() cannot authorize a lease that expired during its Run-lock wait"
  );
  assert!(!reserved);
}
