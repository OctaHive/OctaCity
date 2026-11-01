//! Ordinary v4 retry/cancellation over durable snapshots, independently of Factory.
use super::*;
use octacity_protocol::{
  AgentInventory, BackendHealth, BackendHealthStatus, ExecutionCapabilityV2, ExecutionContractRange,
  ExecutionProviderId, FactoryExecutionCapabilityV3, HostSnapshot, JobBinding, JobSpecV3, OctaInventory,
  TaskPluginInventory, VerifiedJobSpec, verify_compatible_job_spec,
};
use octacity_server_store::{CancelBuild, testing::StoreContractFixture};

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_free_cancellation_can_be_delivered_by_the_ordinary_terminal_outbox() {
  use octacity_server_store::{
    ClaimInternalTriggerEvents, CompleteInternalTriggerEvent, InternalTriggerEventStore as _, TerminalBuildEvent,
    WorkerOwner,
  };
  let database = TestDatabase::migrated().await;
  let fixture = octacity_server_store::testing::source_free_store_contract_fixture();
  seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  let store = PostgresAuthoritativeStore::new(database.pool.clone(), support::test_signer());
  store.accept_trigger(fixture.request.clone()).await.unwrap();
  store
    .cancel_build(management_mutation(CancelBuild {
      build_id: fixture.request.build.id,
      idempotency_key: IdempotencyKey::new("source-free-outbox-cancel").unwrap(),
      requested_at: time(2000),
    }))
    .await
    .unwrap();
  let outbox = PostgresStore::new(database.pool.clone());
  let claim_request = ClaimInternalTriggerEvents::new(
    WorkerOwner::new("source-free-outbox").unwrap(),
    time(3000),
    time(4000),
    10,
  )
  .unwrap();
  let claims = outbox
    .claim_internal_trigger_events(claim_request.clone())
    .await
    .unwrap();
  assert_eq!(claims.len(), 1);
  let claim = &claims[0];
  assert_eq!(claim.source_build_id, fixture.request.build.id);
  assert_eq!(claim.event_kind, TerminalBuildEvent::Cancelled);
  assert_eq!(
    claim.source,
    octacity_server_domain::BuildSourceMode::SourceFreeV1.into()
  );
  assert!(claim.matches.is_empty());
  outbox
    .complete_internal_trigger_event(CompleteInternalTriggerEvent {
      event_identity: claim.event_identity.clone(),
      owner: claim.owner.clone(),
      completed_at: time(3100),
    })
    .await
    .unwrap();
  assert!(
    outbox
      .claim_internal_trigger_events(claim_request)
      .await
      .unwrap()
      .is_empty()
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn source_free_jobs_fail_retry_cancel_and_replay_across_independent_postgres_pools() {
  let database = TestDatabase::migrated().await;
  let mut fixture = octacity_server_store::testing::source_free_store_contract_fixture();
  let (inventory, snapshot) = source_free_placement(&mut fixture);
  for job in &fixture.request.jobs {
    assert!(
      octacity_server_scheduler::is_compatible_with_contract(
        &inventory,
        &snapshot,
        &job.requirements,
        &job.job_spec_template,
        4
      ),
      "the installed test Agent must satisfy ordinary placement"
    );
  }
  seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  let execution = &inventory.executions[0];
  let policy = octacity_server_store::PoolAdmissionPolicy::ExecutionAllowlist {
    platforms: std::collections::BTreeSet::from([octacity_server_store::AgentPlatform::new("macos", "arm64").unwrap()]),
    execution_targets: std::collections::BTreeSet::from([octacity_server_store::PoolExecutionTarget {
      mode: execution.mode,
      host_platform: execution.host_platform,
      target_platform: execution.target_platform,
      required_guarantees: execution.guarantees.iter().copied().collect(),
    }]),
  };
  assert!(policy.permits_runtime(fixture.request.jobs[0].job_spec_template.placement_policy().runtime));
  sqlx::query("UPDATE pools SET admission_policy=$1 WHERE id=$2")
    .bind(sqlx::types::Json(&policy))
    .bind(fixture.allowed_pool.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  // Installation setup only: subsequent state changes use ordinary store ports.
  sqlx::query("UPDATE agent_registrations SET inventory=$1, execution_contract_version=4 WHERE agent_id=$2")
    .bind(sqlx::types::Json(&inventory))
    .bind(fixture.agent_id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  let signer = support::test_signer();
  let store = PostgresAuthoritativeStore::new(database.pool.clone(), signer.clone());
  store.accept_trigger(fixture.request.clone()).await.unwrap();
  drop(store);
  let options = database.pool.connect_options();
  database.pool.close().await;
  let pool = PgPool::connect_with((*options).clone()).await.unwrap();
  let restarted = PostgresAuthoritativeStore::new(pool.clone(), signer.clone());
  assert_eq!(
    restarted.build(fixture.request.build.id).await.unwrap().build,
    fixture.request.build
  );
  let claim = JobClaim::new(
    id(8900),
    LeaseFence::from_bytes([89; 32]),
    fixture.agent_id,
    fixture.registration_epoch,
    fixture.allowed_pool,
    snapshot,
    LeaseWindow::new(time(1000), time(253_402_300_799_000)).unwrap(),
  )
  .unwrap();
  let grant = match restarted.claim_ready_job(claim).await.unwrap() {
    JobClaimOutcome::Claimed(grant) => *grant,
    JobClaimOutcome::Empty => panic!("a qualified v4 Agent without a VCS plugin must claim the ordinary Job"),
  };
  let keys = std::collections::BTreeMap::from([(signer.key_id().to_owned(), signer.verifying_key())]);
  let verified = verify_compatible_job_spec(
    &grant.signed_job_spec,
    &keys,
    JobBinding {
      job_id: &grant.job_id.to_string(),
      attempt: 1,
      now: 1,
    },
  )
  .unwrap();
  let VerifiedJobSpec::V4(spec) = verified else {
    panic!("source absence requires the signed v4 form")
  };
  assert!(spec.factory.is_none());
  assert!(
    spec
      .permissions
      .mounts
      .iter()
      .all(|mount| mount.root != "/workspace/source")
  );
  let access = LeaseAccess {
    lease_id: grant.lease_id,
    fence: grant.fence,
    agent_id: grant.agent_id,
    registration_epoch: grant.registration_epoch,
  };
  restarted
    .complete_job(JobCompletion {
      completion_id: IdempotencyKey::new("source-free-failure").unwrap(),
      lease: access,
      final_sequence: None,
      kind: JobCompletionKind::Failed(JobFailureClass::Execution),
      execution: None,
      completed_at: time(2000),
    })
    .await
    .unwrap();
  let original = restarted.attempt(fixture.request.attempt_id).await.unwrap();
  assert_eq!(original.state, AttemptState::Failed);
  let retry = retry_request(&fixture.request, 8910, "source-free-retry", time(2100));
  let retried = restarted.retry_build(management_mutation(retry.clone())).await.unwrap();
  assert_eq!(retried.disposition, MutationDisposition::Applied);
  drop(restarted);
  pool.close().await;
  let pool = PgPool::connect_with((*options).clone()).await.unwrap();
  let restarted = PostgresAuthoritativeStore::new(pool.clone(), signer);
  let second = restarted.latest_attempt(fixture.request.build.id).await.unwrap();
  assert_eq!(second.number.get(), 2);
  for job in &second.jobs {
    let template = &job.job.job_spec_template;
    assert!(template.source_revision().is_none());
    assert!(template.managed_intent().unwrap().factory().is_none());
    assert_eq!(template.placement_policy().minimum_execution_contract, 4);
  }
  let cancellation = CancelBuild {
    build_id: fixture.request.build.id,
    idempotency_key: IdempotencyKey::new("source-free-cancel").unwrap(),
    requested_at: time(2200),
  };
  let cancelled = restarted
    .cancel_build(management_mutation(cancellation.clone()))
    .await
    .unwrap();
  assert_eq!(cancelled.build_state, BuildState::Cancelled);
  assert_eq!(cancelled.cancelled_jobs.len(), 2);
  assert_eq!(restarted.attempt(fixture.request.attempt_id).await.unwrap(), original);
  assert_eq!(
    restarted.build(fixture.request.build.id).await.unwrap().build,
    fixture.request.build
  );
  assert_eq!(
    restarted
      .retry_build(management_mutation(retry))
      .await
      .unwrap()
      .disposition,
    MutationDisposition::Replayed
  );
  assert_eq!(
    restarted
      .cancel_build(management_mutation(cancellation))
      .await
      .unwrap()
      .disposition,
    MutationDisposition::Replayed
  );
  assert_eq!(
    restarted.latest_attempt(fixture.request.build.id).await.unwrap().id,
    second.id
  );
  drop(restarted);
  pool.close().await;
  database.cleanup().await;
}

fn source_free_placement(fixture: &mut StoreContractFixture) -> (AgentInventory, HostSnapshot) {
  let canonical: JobSpecV3 = serde_json::from_str(include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../shared/protocol-fixtures/job-spec/job-spec-v3.json"
  )))
  .unwrap();
  let target = &canonical.runtime.target;
  for job in &mut fixture.request.jobs {
    let requirements = &mut job.requirements;
    requirements.capabilities.clear();
    requirements.runtime_class = octacity_server_domain::RuntimeClass::Virtualization;
    requirements.operating_system = target.target_platform.os;
    requirements.architecture = target.target_platform.architecture;
    requirements.host_platform = Some(target.host_platform);
    requirements.required_guarantees = target.required_guarantees.clone();
    requirements.minimum_cpu_millis = canonical.runtime.cpu_millis;
    requirements.minimum_memory_bytes = canonical.runtime.memory_bytes;
    requirements.minimum_disk_bytes = canonical.runtime.writable_disk_bytes;
  }
  let execution = ExecutionCapabilityV2 {
    provider: ExecutionProviderId::new("qualified-vm").unwrap(),
    mode: target.mode,
    host_platform: target.host_platform,
    target_platform: target.target_platform,
    guarantees: target.required_guarantees.clone(),
    immutable_images: true,
  };
  let mut inventory = octacity_server_store::testing::compatible_inventory(fixture.agent_id);
  inventory.execution_contract = ExecutionContractRange { min: 1, max: 4 };
  inventory.host_platform = target.host_platform;
  inventory.host_capacity.total_memory_bytes = 1_073_741_824;
  inventory.host_capacity.work_disk_total_bytes = 2_147_483_648;
  inventory.host_capacity.state_disk_total_bytes = 2_147_483_648;
  inventory.host_capacity.virtualization_available = true;
  inventory.runtimes.clear();
  inventory.executions = vec![execution.clone()];
  inventory.factory_executions = vec![FactoryExecutionCapabilityV3 {
    workspace_anchor: false,
    controlled_tools: false,
    execution: execution.clone(),
    enforcement: canonical.required_enforcement,
  }];
  inventory.source_plugins.clear();
  inventory.cache = None;
  inventory.octa = OctaInventory {
    version: canonical.octa.version,
    runner_sha256: canonical.octa.runner_sha256,
    build_commit: None,
    runner_protocols: vec![canonical.octa.runner_protocol],
    event_schemas: vec![canonical.octa.event_schema],
    plugin_protocols: vec![canonical.octa.plugin_protocol],
    octafile_versions: vec![1],
    features: vec![],
    plugins: canonical
      .octa
      .plugin_digests
      .into_iter()
      .map(|(name, sha256)| TaskPluginInventory {
        name,
        version: "0.5.0".to_owned(),
        protocol: canonical.octa.plugin_protocol,
        platforms: vec!["linux-aarch64".to_owned()],
        sha256,
        capabilities: vec![],
      })
      .collect(),
  };
  inventory.validate().unwrap();
  let snapshot = HostSnapshot {
    available_cpu_millis: 2000,
    available_memory_bytes: 1_073_741_824,
    work_disk_free_bytes: 2_147_483_648,
    state_disk_free_bytes: 2_147_483_648,
    active_job: None,
    backends: vec![BackendHealth {
      backend: execution.provider.to_string(),
      execution: Some(execution),
      status: BackendHealthStatus::Ready,
      message: None,
    }],
  };
  (inventory, snapshot)
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn additive_source_free_upgrade_preserves_mixed_legacy_and_new_build_history() {
  use octacity_server_domain::{BuildConfigurationVersion, PipelineVersion, TriggerVersion};
  use octacity_server_store::TriggerCausality;
  let database = TestDatabase::empty().await;
  let old_schema = sqlx::migrate::Migrator::with_migrations(
    octacity_server_store_postgres::MIGRATOR
      .iter()
      .filter(|migration| migration.version < 110)
      .cloned()
      .collect(),
  );
  octacity_server_store_postgres::run_migrator(&database.pool, &old_schema)
    .await
    .unwrap();
  let legacy = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(&database.pool, &legacy).await.unwrap();
  let store = PostgresAuthoritativeStore::new(database.pool.clone(), support::test_signer());
  store.accept_trigger(legacy.request.clone()).await.unwrap();
  let old_build = store.build(legacy.request.build.id).await.unwrap();
  let old_attempt = store.latest_attempt(legacy.request.build.id).await.unwrap();
  let old_bytes = serde_json::to_vec(&old_attempt.jobs[0].job.job_spec_template).unwrap();
  for _ in 0..2 {
    octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  }
  // Add immutable versions on the existing owners; never modify the retained v1 rows.
  sqlx::query("INSERT INTO pipeline_versions (pipeline_id,version,dag_snapshot,published_at) SELECT pipeline_id,2,dag_snapshot,published_at FROM pipeline_versions WHERE pipeline_id=$1 AND version=1")
    .bind(legacy.request.build.pipeline_id.as_uuid()).execute(&database.pool).await.unwrap();
  sqlx::query("INSERT INTO build_configuration_versions (build_configuration_id,version,enabled,repository_id,repository_version,pipeline_id,pipeline_version,configuration_snapshot,job_concurrency_limit,allowed_pool_ids,retry_max_attempts,retries_infrastructure,published_at) SELECT build_configuration_id,2,enabled,NULL,NULL,pipeline_id,2,configuration_snapshot || '{\"source\":{\"kind\":\"source_free_v1\"}}'::jsonb,job_concurrency_limit,allowed_pool_ids,retry_max_attempts,retries_infrastructure,published_at FROM build_configuration_versions WHERE build_configuration_id=$1 AND version=1")
    .bind(legacy.request.build.configuration_id.as_uuid()).execute(&database.pool).await.unwrap();
  sqlx::query("INSERT INTO triggers (id,version,build_configuration_id,build_configuration_version,kind,enabled,definition,created_at) SELECT id,2,build_configuration_id,2,kind,enabled,definition,created_at FROM triggers WHERE id=$1 AND version=1")
    .bind(legacy.request.trigger.trigger.id.as_uuid()).execute(&database.pool).await.unwrap();
  let mut fresh = octacity_server_store::testing::source_free_store_contract_fixture().request;
  fresh.build.id = id(9001);
  fresh.attempt_id = id(9002);
  fresh.build.configuration_version = BuildConfigurationVersion::new(2).unwrap();
  fresh.build.pipeline_version = PipelineVersion::new(2).unwrap();
  fresh.trigger.id = id(9000);
  fresh.trigger.causality = TriggerCausality::root(fresh.trigger.id);
  fresh.trigger.trigger.version = TriggerVersion::new(2).unwrap();
  fresh.trigger.target.configuration_version = fresh.build.configuration_version;
  let identities: std::collections::BTreeMap<_, _> = fresh
    .jobs
    .iter()
    .enumerate()
    .map(|(index, job)| (job.id, id::<octacity_server_domain::JobId>(9003 + index as u64)))
    .collect();
  for job in &mut fresh.jobs {
    job.id = identities[&job.id];
    job.dependencies = job.dependencies.iter().map(|id| identities[id]).collect();
    let mut template = serde_json::to_value(&job.job_spec_template).unwrap();
    template["build_id"] = serde_json::to_value(fresh.build.id).unwrap();
    job.job_spec_template = serde_json::from_value(template).unwrap();
  }
  fresh.validate().unwrap();
  store.accept_trigger(fresh.clone()).await.unwrap();
  drop(store);
  let options = database.pool.connect_options();
  database.pool.close().await;
  let pool = PgPool::connect_with((*options).clone()).await.unwrap();
  let restarted = PostgresAuthoritativeStore::new(pool.clone(), support::test_signer());
  assert_eq!(restarted.build(legacy.request.build.id).await.unwrap(), old_build);
  assert_eq!(
    restarted.latest_attempt(legacy.request.build.id).await.unwrap(),
    old_attempt
  );
  assert_eq!(
    serde_json::to_vec(
      &restarted.latest_attempt(legacy.request.build.id).await.unwrap().jobs[0]
        .job
        .job_spec_template
    )
    .unwrap(),
    old_bytes
  );
  assert_eq!(restarted.build(fresh.build.id).await.unwrap().build, fresh.build);
  for job in restarted.latest_attempt(fresh.build.id).await.unwrap().jobs {
    assert!(job.job.job_spec_template.source_revision().is_none());
    assert!(job.job.job_spec_template.managed_intent().unwrap().factory().is_none());
  }
  for request in [legacy.request, fresh] {
    assert_eq!(
      restarted.accept_trigger(request).await.unwrap().disposition,
      MutationDisposition::Replayed
    );
  }
  drop(restarted);
  pool.close().await;
  database.cleanup().await;
}
