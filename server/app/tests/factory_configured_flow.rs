//! Same public configured-owner contract under PostgreSQL persistence and restart.
use async_trait::async_trait;
use octacity_server_application::MutationDisposition;
use octacity_server_application::*;
use octacity_server_domain::*;
use octacity_server_factory::*;
use octacity_server_store::*;
use octacity_server_store_postgres::PostgresStore;
use std::sync::{Arc, Mutex};
#[allow(dead_code)]
#[path = "../../application/src/factory_node_test_support.rs"]
mod factory_node_test_support;
#[allow(dead_code)]
#[path = "../../application/src/factory_flow_tests.rs"]
mod flow_contract;
#[allow(dead_code)]
#[path = "../../application/src/factory_flow_control_tests.rs"]
mod human_contract;
#[allow(dead_code)]
#[path = "../../infrastructure/octacity-server-store-postgres/tests/support/mod.rs"]
mod postgres_support;

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn at(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}
struct Seeder {
  pool: sqlx::PgPool,
  seeded: Mutex<std::collections::BTreeSet<BuildId>>,
}
#[async_trait]
impl flow_contract::FactoryTestBuildSeeder for Seeder {
  async fn seed(&self, request: &CreateFactoryBuild, accepted: &FactoryBuildAcceptance) {
    let insert = self.seeded.lock().unwrap().insert(accepted.build_id);
    if insert {
      postgres_support::factory_build::seed_factory_build(
        &self.pool,
        &ExactSubject::new(
          request.project_id,
          request.repository_id,
          request.immutable_revision.clone(),
        ),
        RepositoryVersion::INITIAL,
        request.build_configuration.id(),
        accepted.build_id,
        accepted.attempt_id,
        accepted.job_ids[0],
      )
      .await;
    }
  }
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn configured_manual_intake_and_unknown_node_recover_the_same_journal_and_pool_in_postgres() {
  let database = postgres_support::TestDatabase::migrated().await;
  let (admission, execution) = flow_contract::configured_fixture();
  let store = admit(&database.pool, &admission).await;
  let mut execution = Arc::try_unwrap(execution).ok().unwrap();
  execution.seeder = Some(Arc::new(Seeder {
    pool: database.pool.clone(),
    seeded: Mutex::new(Default::default()),
  }));
  let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
  let ready =
    flow_contract::verify_configured_owner_journey(store.clone(), restarted.clone(), &admission, Arc::new(execution))
      .await;
  let policy = &admission.flow.pool_settings()[&key("development.ready")].policy;
  assert!(
    restarted
      .phase_ready_entries(policy.digest(), None, 10)
      .await
      .unwrap()
      .is_empty()
  );
  let snapshot = restarted.factory_run_snapshot(admission.run.id()).await.unwrap();
  let claim = snapshot.current_claim.as_ref().unwrap();
  assert_eq!(
    restarted
      .phase_pool_selection_for_claim(admission.run.id(), claim.id)
      .await
      .unwrap()
      .unwrap()
      .entry,
    *ready
  );
  database.cleanup().await;
}

async fn admit(pool: &sqlx::PgPool, admission: &PublishedFactoryAdmission) -> Arc<PostgresStore> {
  let work = &admission.work;
  let project = work.subject().project_id();
  let repository = work.subject().repository_id();
  sqlx::query("INSERT INTO projects (id,name,version,created_at,updated_at) VALUES ($1,'configured-factory',1,to_timestamp(0),to_timestamp(0))").bind(project.as_uuid()).execute(pool).await.unwrap();
  sqlx::query("INSERT INTO repositories (id,project_id,name,created_at) VALUES ($1,$2,'source',to_timestamp(0))")
    .bind(repository.as_uuid())
    .bind(project.as_uuid())
    .execute(pool)
    .await
    .unwrap();
  sqlx::query("INSERT INTO repository_versions (repository_id,version,vcs_integration_id,repository_locator,selection_policy,published_at) VALUES ($1,1,$2,'https://example.test/source.git',$3,to_timestamp(0))").bind(repository.as_uuid()).bind(uuid::Uuid::new_v4()).bind(sqlx::types::Json(RepositorySelectionPolicy { allowed_references:Default::default(),default_reference:None,allow_exact_revision:true })).execute(pool).await.unwrap();
  let store = Arc::new(PostgresStore::new(pool.clone()));
  let mut fixture = postgres_support::factory_configuration::fixture(project);
  fixture.draft.hard_budget = admission.flow.limits().budget();
  fixture.draft.wip_limits = admission.flow.wip_limits();
  fixture.draft.flow = Some(
    FactoryFlowConfiguration::new(
      admission.flow.closure().clone(),
      admission.flow.limits().clone(),
      admission.flow.data_schemas().to_vec(),
      admission.flow.pool_settings().clone(),
    )
    .unwrap(),
  );
  let configuration = FactoryConfiguration::publish(
    work.configuration().id(),
    work.configuration().version(),
    project,
    work.configuration().definition_digest(),
    fixture.draft.clone(),
    &fixture.choices,
  )
  .unwrap();
  store
    .create_factory_configuration(octacity_server_store::testing::management_mutation_with_request(
      CreateFactoryConfiguration {
        idempotency_key: IdempotencyKey::new("configured-template").unwrap(),
        published_at: at(0),
        intent: FactoryConfigurationMutationIntent::Create {
          id: configuration.reference().id(),
          project_id: project,
          definition_digest: configuration.reference().definition_digest(),
          draft: fixture.draft,
        },
        configuration,
      },
      "configured-template",
    ))
    .await
    .unwrap();
  let request = AdmitFactoryWork {
    probe: FactoryAdmissionProbe {
      source_scope: FactoryWorkSourceScope {
        source: key("manual"),
        security_scope: octacity_server_store::ManagementSecurityScope::trusted_network(),
      },
      external_identity: work.external_identity().clone(),
      intent_digest: FactoryDigest::from_bytes([1; 32]),
      idempotency_key: IdempotencyKey::new("configured-manual-intake").unwrap(),
    },
    repository_version: RepositoryVersion::INITIAL,
    work: work.clone(),
    run: admission.run.clone(),
    flow: admission.flow.clone(),
    admitted_at: at(1),
  };
  store
    .admit_factory_work(octacity_server_store::testing::management_mutation_with_request(
      request.clone(),
      "configured-manual-intake",
    ))
    .await
    .unwrap();
  store
    .admit_factory_work(octacity_server_store::testing::management_mutation_with_request(
      request,
      "configured-manual-replay",
    ))
    .await
    .unwrap();
  store
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn configured_nested_flow_restores_caller_and_child_results_in_postgres() {
  let database = postgres_support::TestDatabase::migrated().await;
  let admission = flow_contract::nested_fixture();
  let store = admit(&database.pool, &admission).await;
  flow_contract::verify_nested_owner(store, Arc::new(PostgresStore::new(database.pool.clone())), &admission).await;
  database.cleanup().await;
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn configured_pooled_repeats_reserve_each_execution_after_postgres_restart() {
  let database = postgres_support::TestDatabase::migrated().await;
  let admission = flow_contract::bounded_fixture(false, true);
  let store = admit(&database.pool, &admission).await;
  flow_contract::verify_bounded_owner(
    store,
    Arc::new(PostgresStore::new(database.pool.clone())),
    &admission,
    false,
    true,
  )
  .await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn the_unreleased_factory_schema_accepts_only_technical_execution_states() {
  let database = postgres_support::TestDatabase::empty().await;
  octacity_server_store_postgres::MIGRATOR
    .run_to(51, &database.pool)
    .await
    .unwrap();
  let (admission, _) = flow_contract::configured_fixture();
  let store = admit(&database.pool, &admission).await;
  let original = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  for state in ["admitted", "running", "waiting", "cancelled", "completed"] {
    let mut transaction = database.pool.begin().await.unwrap();
    sqlx::query("UPDATE factory_runs SET state=$2 WHERE id=$1")
      .bind(admission.run.id().as_uuid())
      .bind(state)
      .execute(&mut *transaction)
      .await
      .expect("technical state is supported by the unreleased Factory schema");
    transaction.rollback().await.unwrap();
  }
  for state in [
    "implementing",
    "validating",
    "evaluating",
    "reworking",
    "ready_for_delivery",
    "delivering",
    "escalated",
    "rejected",
    "accessibility_audit",
  ] {
    let error = sqlx::query("UPDATE factory_runs SET state=$2 WHERE id=$1")
      .bind(admission.run.id().as_uuid())
      .bind(state)
      .execute(&database.pool)
      .await
      .expect_err("logical stages and outcomes cannot become Run execution state");
    assert_eq!(
      error.as_database_error().and_then(|error| error.code()).as_deref(),
      Some("23514")
    );
  }
  let restarted = PostgresStore::new(database.pool.clone());
  assert_eq!(
    restarted.factory_run_snapshot(admission.run.id()).await.unwrap(),
    original
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn an_operator_response_resumes_an_exact_human_node_after_postgres_restart() {
  let database = postgres_support::TestDatabase::migrated().await;
  let admission = human_contract::human_fixture();
  let store = admit(&database.pool, &admission).await;
  let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
  human_contract::verify_human_owner(store, restarted, &admission).await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn technical_cancellation_resumes_without_a_business_outcome_after_postgres_restart() {
  let database = postgres_support::TestDatabase::migrated().await;
  let admission = human_contract::human_fixture();
  let store = admit(&database.pool, &admission).await;
  let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
  human_contract::verify_human_cancellation(store, restarted, &admission).await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn human_response_supports_the_admitted_payload_bound_in_postgres() {
  let database = postgres_support::TestDatabase::migrated().await;
  let schema = FlowDataSchema::new(
    key("operator.document"),
    key("v1"),
    FlowValueSchema::String {
      min_bytes: 1,
      max_bytes: MAX_FLOW_DATA_BYTES - 4096,
    },
  )
  .unwrap();
  let admission = human_contract::human_fixture_with_response(schema);
  let store = admit(&database.pool, &admission).await;
  let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
  let value = serde_json::json!("review evidence ".repeat(20_000));
  human_contract::verify_human_response(store, restarted, &admission, value).await;
  database.cleanup().await;
}

#[allow(dead_code)]
#[path = "../../application/src/factory_node_cancellation_tests.rs"]
mod cancellation_contract;
#[allow(dead_code)]
#[path = "../../application/src/factory_node_failure_tests.rs"]
mod failure_contract;
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn configured_node_retry_restores_operator_intent_and_frozen_input_in_postgres() {
  let database = postgres_support::TestDatabase::migrated().await;
  let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture_with_retries(1);
  let store = admit(&database.pool, &fixture.admission).await;
  let ids = (BuildId::generate(), AttemptId::generate(), JobId::generate());
  let build_configuration = fixture
    .admission
    .flow
    .closure()
    .definition(fixture.input.definition())
    .unwrap()
    .node(fixture.input.node())
    .unwrap()
    .build()
    .unwrap()
    .build_configuration
    .clone();
  postgres_support::factory_build::seed_factory_build(
    &database.pool,
    fixture.admission.work.subject(),
    RepositoryVersion::INITIAL,
    build_configuration.id(),
    ids.0,
    ids.1,
    ids.2,
  )
  .await;
  let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
  failure_contract::verify_confirmed_node_retry(store.clone(), restarted.clone(), &fixture, ids).await;
  sqlx::query("DELETE FROM factory_run_controls WHERE run_id = $1 AND control_kind = 'retry-factory-node'")
    .bind(fixture.admission.run.id().as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  assert!(
    restarted
      .factory_run_snapshot(fixture.admission.run.id())
      .await
      .is_err(),
    "restoration must reject a technical retry whose accepted operator intent is missing"
  );
  database.cleanup().await;
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn configured_execution_failure_restores_the_exact_stop_and_usage_in_postgres() {
  let database = postgres_support::TestDatabase::migrated().await;
  let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture();
  let store = admit(&database.pool, &fixture.admission).await;
  let ids = (BuildId::generate(), AttemptId::generate(), JobId::generate());
  let build_configuration = fixture
    .admission
    .flow
    .closure()
    .definition(fixture.input.definition())
    .unwrap()
    .node(fixture.input.node())
    .unwrap()
    .build()
    .unwrap()
    .build_configuration
    .clone();
  postgres_support::factory_build::seed_factory_build(
    &database.pool,
    fixture.admission.work.subject(),
    RepositoryVersion::INITIAL,
    build_configuration.id(),
    ids.0,
    ids.1,
    ids.2,
  )
  .await;
  let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
  failure_contract::verify_failed_node_owner(store, restarted, &fixture, ids).await;
  database.cleanup().await;
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn configured_effectful_cancellation_retains_the_stop_receipt_and_budget_across_restart() {
  let database = postgres_support::TestDatabase::migrated().await;
  let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture();
  let store = admit(&database.pool, &fixture.admission).await;
  let ids = (BuildId::generate(), AttemptId::generate(), JobId::generate());
  let build_configuration = fixture
    .admission
    .flow
    .closure()
    .definition(fixture.input.definition())
    .unwrap()
    .node(fixture.input.node())
    .unwrap()
    .build()
    .unwrap()
    .build_configuration
    .clone();
  postgres_support::factory_build::seed_factory_build(
    &database.pool,
    fixture.admission.work.subject(),
    RepositoryVersion::INITIAL,
    build_configuration.id(),
    ids.0,
    ids.1,
    ids.2,
  )
  .await;
  let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
  sqlx::query("INSERT INTO retention_work (id,resource_kind,resource_identity,phase,available_at,deadline_at,attempt_count,project_id,build_id) VALUES ($1,'metadata',$2,'pending',to_timestamp(0.100),to_timestamp(0.100),0,$3,$4)")
    .bind(uuid::Uuid::new_v4()).bind(ids.0.to_string()).bind(fixture.admission.work.subject().project_id().as_uuid())
    .bind(ids.0.as_uuid()).execute(&database.pool).await.unwrap();
  cancellation_contract::verify_effectful_cancellation(store.clone(), restarted.clone(), &fixture, ids).await;
  let owner = WorkerOwner::new("configured-node-retention").unwrap();
  assert!(
    store
      .claim_factory_retention_work(ClaimFactoryRetentionWork::new(owner.clone(), at(20), at(30), 1).unwrap())
      .await
      .unwrap()
      .is_empty(),
    "a generic Build's retained deadline must protect its owning Flow"
  );
  let mut completed = false;
  for observed in 101..180 {
    let claims = restarted
      .claim_factory_retention_work(
        ClaimFactoryRetentionWork::new(owner.clone(), at(observed), at(observed + 10), 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].run_id, fixture.admission.run.id());
    let pass = restarted
      .advance_factory_retention_work(
        AdvanceFactoryRetentionWork::new(fixture.admission.run.id(), owner.clone(), at(observed), 2).unwrap(),
      )
      .await
      .unwrap();
    if pass.completed {
      completed = true;
      break;
    }
  }
  assert!(
    completed,
    "bounded cleanup must delete stop receipts before their referenced execution and node"
  );
  assert!(
    restarted
      .factory_run_snapshot(fixture.admission.run.id())
      .await
      .is_err()
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn configured_queued_node_retry_restores_frozen_input_and_a_fresh_reservation_in_postgres() {
  let database = postgres_support::TestDatabase::migrated().await;
  let (admission, input) = failure_contract::pooled_retry_fixture();
  let store = admit(&database.pool, &admission).await;
  let ids = (BuildId::generate(), AttemptId::generate(), JobId::generate());
  let build_configuration = admission
    .flow
    .closure()
    .definition(input.definition())
    .unwrap()
    .node(input.node())
    .unwrap()
    .build()
    .unwrap()
    .build_configuration
    .clone();
  postgres_support::factory_build::seed_factory_build(
    &database.pool,
    admission.work.subject(),
    RepositoryVersion::INITIAL,
    build_configuration.id(),
    ids.0,
    ids.1,
    ids.2,
  )
  .await;
  let restarted = Arc::new(PostgresStore::new(database.pool.clone()));
  failure_contract::verify_pooled_node_retry(store, restarted, &admission, input, ids).await;
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn independently_claimed_runs_preserve_configured_node_capacity_in_postgres() {
  let database = postgres_support::TestDatabase::migrated().await;
  let admissions = failure_contract::single_slot_admissions();
  let store = admit_runs(&database.pool, &admissions).await;
  let ids = (BuildId::generate(), AttemptId::generate(), JobId::generate());
  let node = &admissions[0].flow.closure().definitions()[0].nodes()[0];
  postgres_support::factory_build::seed_factory_build(
    &database.pool,
    admissions[0].work.subject(),
    RepositoryVersion::INITIAL,
    node.build().unwrap().build_configuration.id(),
    ids.0,
    ids.1,
    ids.2,
  )
  .await;
  failure_contract::verify_configuration_node_capacity(
    store,
    Arc::new(PostgresStore::new(database.pool.clone())),
    &admissions,
    ids,
  )
  .await;
  database.cleanup().await;
}

async fn admit_runs(pool: &sqlx::PgPool, admissions: &[PublishedFactoryAdmission]) -> Arc<PostgresStore> {
  let store = admit(pool, &admissions[0]).await;
  for second in &admissions[1..] {
    store
      .admit_factory_work(octacity_server_store::testing::management_mutation_with_request(
        AdmitFactoryWork {
          probe: FactoryAdmissionProbe {
            source_scope: FactoryWorkSourceScope {
              source: key("manual"),
              security_scope: octacity_server_store::ManagementSecurityScope::trusted_network(),
            },
            external_identity: second.work.external_identity().clone(),
            intent_digest: FactoryDigest::from_bytes([2; 32]),
            idempotency_key: IdempotencyKey::new("second-concurrent-run").unwrap(),
          },
          repository_version: RepositoryVersion::INITIAL,
          work: second.work.clone(),
          run: second.run.clone(),
          flow: second.flow.clone(),
          admitted_at: at(1),
        },
        "second-concurrent-run",
      ))
      .await
      .unwrap();
  }
  store
}

#[allow(dead_code)]
#[path = "../../application/src/factory_job_test_support.rs"]
mod factory_job_test_support;

struct NeverDispatch;

struct PreparedBuildCompiler {
  prepared: AcceptFactoryBuild,
  calls: std::sync::atomic::AtomicUsize,
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn factory_build_admission_rejects_jobs_without_the_frozen_execution_authority() {
  let (database, _, request) = prepared_factory_build(false).await;
  assert!(
    request.validate().is_err(),
    "ordinary repository-controlled templates do not carry the pinned node input, permissions and deadline"
  );
  database.cleanup().await;
}
#[async_trait]
impl FactoryBuildMaterializer for PreparedBuildCompiler {
  async fn materialize(
    &self,
    intent: &FlowBuildIntent,
    ownership: FactoryClaimOwnership,
    at: Timestamp,
  ) -> Result<AcceptFactoryBuild, OrdinaryBuildApplicationError> {
    assert_eq!(intent, &self.prepared.intent);
    self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let mut prepared = self.prepared.clone();
    prepared.ownership = ownership;
    prepared.accepted_at = at;
    Ok(prepared)
  }
}
struct BuildClock;
impl FactoryExecutionClock for BuildClock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(12))
  }
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn the_ordinary_build_service_dispatches_and_recovers_the_persisted_node_operation() {
  let (database, run_store, request) = prepared_factory_build(true).await;
  let compiler = Arc::new(PreparedBuildCompiler {
    prepared: request.clone(),
    calls: std::sync::atomic::AtomicUsize::new(0),
  });
  let ordinary = || {
    Arc::new(FactoryOrdinaryBuildService::new(
      Arc::new(octacity_server_store_postgres::PostgresAuthoritativeStore::new(
        database.pool.clone(),
        postgres_support::test_signer(),
      )),
      run_store.clone(),
      compiler.clone(),
      Arc::new(BuildClock),
    ))
  };
  let snapshot = run_store
    .factory_run_snapshot(request.intent.input().factory_run_id())
    .await
    .unwrap();
  let schema = snapshot
    .admitted_flow
    .data_schemas()
    .iter()
    .find(|schema| {
      schema.reference() == snapshot.admitted_flow.closure().definitions()[0].nodes()[0].outcomes()[0].schema()
    })
    .unwrap();
  let first = FactoryNodeBuildAdapter::new(ordinary(), Arc::new(factory_node_test_support::Outputs(None)))
    .observe_or_dispatch(
      &request.intent,
      &snapshot.admitted_flow,
      schema,
      request.ownership.clone(),
      at(12),
    )
    .await
    .unwrap();
  let recovered = FactoryNodeBuildAdapter::new(ordinary(), Arc::new(factory_node_test_support::Outputs(None)))
    .observe_or_dispatch(
      &request.intent,
      &snapshot.admitted_flow,
      schema,
      request.ownership.clone(),
      at(13),
    )
    .await
    .unwrap();
  assert_eq!(first, recovered, "restart must observe the same ordinary execution");
  assert!(matches!(first, FactoryNodeBuildStep::Waiting(_)));
  assert_eq!(
    compiler.calls.load(std::sync::atomic::Ordering::SeqCst),
    1,
    "recovery must not materialize another Build"
  );
  database.cleanup().await;
}

fn protected_build_admission(managed: bool) -> PublishedFactoryAdmission {
  protected_build_admission_with_output_budget(managed, 10_000)
}

fn protected_build_admission_with_output_budget(managed: bool, output_bytes: u64) -> PublishedFactoryAdmission {
  protected_build_admission_with_capture_paths(managed, output_bytes, None)
}
fn protected_build_admission_with_capture_paths(
  managed: bool,
  output_bytes: u64,
  capture_paths: Option<&[&str]>,
) -> PublishedFactoryAdmission {
  let original = failure_contract::single_slot_admissions().remove(0);
  if !managed {
    return original;
  }
  let grants = factory_job_test_support::permissions(false);
  let old = &original.flow.closure().definitions()[0];
  let declared = &old.nodes()[0];
  let budget = BudgetLimit::new(1, 600_000, 100, 1_000, output_bytes).unwrap();
  let total = BudgetLimit::new(16, 9_600_000, 1_600, 16_000, 160_000).unwrap();
  let mut binding = declared.build().unwrap().clone();
  binding.tool = grants.executables().next().unwrap().clone();
  binding.plugin = grants.plugins().next().unwrap().clone();
  binding.capture_write_paths = capture_paths.map(|paths| {
    paths
      .iter()
      .map(|path| FactoryRepositoryPath::new(*path).unwrap())
      .collect()
  });
  let node = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: declared.key().clone(),
    kind: declared.kind(),
    input_schema: declared.input_schema().cloned(),
    outcomes: declared.outcomes().to_vec(),
    budget,
    permissions: grants.clone(),
    required: true,
    subflow: None,
  })
  .unwrap()
  .with_build(binding)
  .unwrap();
  let node = match declared.input_binding() {
    Some(binding) => node.with_input_binding(binding.clone()).unwrap(),
    None => node
      .with_input_binding(
        FlowInputBinding::new(
          Default::default(),
          FlowDataMapping::new(FlowValueMapping::Constant {
            value: serde_json::json!({"count":3}),
          })
          .unwrap(),
        )
        .unwrap(),
      )
      .unwrap(),
  };
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: old.input_schema().cloned(),
    entry: node.key().clone(),
    nodes: vec![node],
    transitions: old.transitions().to_vec(),
    terminals: old.terminals().to_vec(),
    context_projections: old.context_projections().to_vec(),
    data_projections: old.data_projections().to_vec(),
    execution: FlowExecutionPolicy::new(total, grants.clone(), 1).unwrap(),
  })
  .unwrap();
  let root = FlowRun::root(&original.run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&root).unwrap();
  let flow = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    FlowAdmissionLimits::product_defaults(1, total, grants).unwrap(),
    root,
    cycle,
    original.flow.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(original.flow.data_schemas().to_vec())
  .unwrap();
  PublishedFactoryAdmission { flow, ..original }
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn factory_build_cancellation_uses_ordinary_job_and_queue_state_and_replays_after_restart() {
  let (database, runs, request) = prepared_factory_build(true).await;
  let compiler = Arc::new(PreparedBuildCompiler {
    prepared: request.clone(),
    calls: Default::default(),
  });
  let service = || {
    FactoryOrdinaryBuildService::new(
      Arc::new(octacity_server_store_postgres::PostgresAuthoritativeStore::new(
        database.pool.clone(),
        postgres_support::test_signer(),
      )),
      runs.clone(),
      compiler.clone(),
      Arc::new(BuildClock),
    )
  };
  let operation = request.intent.operation_id().unwrap();
  let ownership = request.ownership.clone();
  let accepted = octacity_server_store_postgres::PostgresAuthoritativeStore::new(
    database.pool.clone(),
    postgres_support::test_signer(),
  )
  .accept_factory_build(request)
  .await
  .unwrap();
  let cancel = CancelFactoryBuild {
    ownership,
    operation_id: FactoryDigest::sha256(
      "octacity.factory.build-cancellation.v1",
      &[operation.as_bytes().as_slice()],
    ),
    build_operation_id: operation,
    build_id: accepted.build_id,
    requested_at: at(12),
  };
  let mut foreign = cancel.clone();
  foreign.build_operation_id = FactoryDigest::from_bytes([99; 32]);
  assert!(service().cancel_factory_build(foreign).await.is_err());
  assert_eq!(
    runs.build(accepted.build_id).await.unwrap().state,
    octacity_server_orchestrator::BuildState::Running
  );
  service().cancel_factory_build(cancel.clone()).await.unwrap();
  service().cancel_factory_build(cancel).await.unwrap();
  let attempt = runs.attempt(accepted.attempt_id).await.unwrap();
  assert_eq!(attempt.state, octacity_server_orchestrator::AttemptState::Cancelled);
  assert!(
    attempt
      .jobs
      .iter()
      .all(|job| job.state == octacity_server_job::JobState::Cancelled && job.queue.is_none())
  );
  assert_eq!(compiler.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
  let claims = runs
    .claim_internal_trigger_events(
      ClaimInternalTriggerEvents::new(
        WorkerOwner::new("factory-terminal-events").unwrap(),
        at(14),
        at(100),
        10,
      )
      .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(
    claims.len(),
    1,
    "a real terminal Factory Build must be consumable without an invented Trigger"
  );
  assert_eq!(claims[0].source_build_id, accepted.build_id);
  assert!(claims[0].trigger_ancestry.is_empty());
  runs
    .complete_internal_trigger_event(CompleteInternalTriggerEvent {
      event_identity: claims[0].event_identity.clone(),
      owner: claims[0].owner.clone(),
      completed_at: at(15),
    })
    .await
    .unwrap();

  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn factory_build_admission_rejects_an_existing_pool_outside_its_configuration() {
  let (database, _, mut request) = prepared_factory_build(true).await;
  let foreign = PoolId::generate();
  sqlx::query("INSERT INTO pools (id,version,name,enabled,drain_state,admission_policy,concurrency_limit,created_at) VALUES ($1,1,'foreign',true,'accepting','{\"mode\":\"any\"}',4,to_timestamp(0))")
    .bind(foreign.as_uuid()).execute(&database.pool).await.unwrap();
  request.jobs[0].allowed_pools = vec![foreign];
  let operation = request.intent.operation_id().unwrap();
  let store = octacity_server_store_postgres::PostgresAuthoritativeStore::new(
    database.pool.clone(),
    postgres_support::test_signer(),
  );
  assert!(
    store.accept_factory_build(request).await.is_err(),
    "node Job cannot widen the pinned Build Configuration pools"
  );
  assert!(store.factory_build_for_operation(operation).await.unwrap().is_none());
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn deleting_a_managed_upload_does_not_replenish_its_signed_output_budget() {
  use sha2::Digest as _;
  let (database, artifacts, request) = prepared_factory_build(true).await;
  let execution = octacity_server_store_postgres::PostgresAuthoritativeStore::new(
    database.pool.clone(),
    postgres_support::test_signer(),
  );
  let job = request
    .jobs
    .iter()
    .find(|job| job.dependencies.is_empty())
    .unwrap()
    .clone();
  let accepted = execution.accept_factory_build(request).await.unwrap();
  let agent = AgentId::generate();
  let registration = uuid::Uuid::new_v4();
  let access = LeaseAccess {
    lease_id: LeaseId::generate(),
    fence: LeaseFence::from_bytes([81; 32]),
    agent_id: agent,
    registration_epoch: RegistrationEpoch::new(1).unwrap(),
  };
  // Retained assignment fixture: this test exercises upload authority and
  // accounting, while placement has its own ordinary execution contracts.
  sqlx::query("INSERT INTO agents (id,name,pool_id,pool_version,state,inventory,version,created_at,updated_at) VALUES ($1,'output-fixture',$2,1,'online',$3,1,to_timestamp(0),to_timestamp(0))")
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
  let request = BeginArtifactUpload {
    artifact_id: ArtifactId::generate(),
    upload_id: ArtifactUploadId::generate(),
    idempotency_key: IdempotencyKey::new("managed-output-first").unwrap(),
    lease: access,
    job_id: job.id,
    attempt: AttemptNumber::FIRST,
    logical_name: ArtifactName::new("result.bin").unwrap(),
    producer_run_id: 1,
    producer_task_id: 1,
    artifact_type: ArtifactType::Artifact,
    media_type: ArtifactMediaType::new("application/octet-stream").unwrap(),
    transport_media_type: ArtifactMediaType::new("application/octet-stream").unwrap(),
    size_bytes: 1024,
    digest: ArtifactContentDigest::from_bytes([82; 32]),
    reserved_at: at(14),
    capability_expires_at: at(100),
  };
  let first = artifacts
    .begin_artifact_upload(request.clone())
    .await
    .unwrap()
    .upload
    .artifact;
  assert_eq!(first.identity().build_id, accepted.build_id);
  artifacts
    .transition_artifact(TransitionArtifact {
      expected_identity: first.identity().clone(),
      expected_version: first.version(),
      event: ArtifactEvent::Delete,
      authority: ArtifactTransitionAuthority::Lease(access),
      transitioned_at: at(15),
    })
    .await
    .unwrap();
  let second = BeginArtifactUpload {
    artifact_id: ArtifactId::generate(),
    upload_id: ArtifactUploadId::generate(),
    idempotency_key: IdempotencyKey::new("managed-output-second").unwrap(),
    logical_name: ArtifactName::new("another.bin").unwrap(),
    reserved_at: at(16),
    ..request
  };
  assert!(
    matches!(
      artifacts.begin_artifact_upload(second.clone()).await,
      Err(StoreError::Conflict {
        entity: EntityKind::Artifact
      })
    ),
    "deletion frees storage, not the signed execution budget"
  );
  assert!(
    matches!(
      artifacts
        .reserve_artifact(ReserveArtifact {
          identity: ArtifactIdentity {
            artifact_id: second.artifact_id,
            build_id: accepted.build_id,
            attempt_id: accepted.attempt_id,
            job_id: job.id,
            lease_id: access.lease_id,
            logical_name: second.logical_name,
            artifact_type: second.artifact_type,
            media_type: second.media_type,
            size_bytes: second.size_bytes,
            digest: second.digest,
            retention: ArtifactRetentionPolicy::DeleteAfter(at(100)),
          },
          lease: access,
          reserved_at: at(16),
        })
        .await,
      Err(StoreError::Conflict {
        entity: EntityKind::Artifact
      })
    ),
    "a direct reservation must obey the same managed execution ceiling"
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn expired_claim_history_cannot_disable_snapshot_or_cancellation() {
  let (database, runs, request) = prepared_factory_build(true).await;
  let run_id = request.intent.input().factory_run_id();
  let before = runs.factory_run_snapshot(run_id).await.unwrap();
  let owner = key("historical-owner");
  let records: Vec<_> = (1..=32_769_u64)
    .map(|index| {
      FactoryRunClaimRecord::new(
        run_id,
        owner.clone(),
        FactoryClaim::new(
          FactoryClaimFence::new(FactoryDigest::sha256("test.expired-history", &[&index.to_be_bytes()])),
          at(index as i64 * 30_000),
          at((index as i64 + 1) * 30_000),
        )
        .unwrap(),
      )
    })
    .collect();
  // Bulk fixture construction keeps a multi-day ownership history test cheap;
  // assertions use the normal snapshot and operator control ports.
  sqlx::query("INSERT INTO factory_run_claims (id,run_id,owner,fence,claimed_at,expires_at) SELECT id,$1,$2,fence,to_timestamp(claimed::double precision/1000),to_timestamp(expires::double precision/1000) FROM UNNEST($3::bytea[],$4::bytea[],$5::bigint[],$6::bigint[]) AS r(id,fence,claimed,expires)")
    .bind(run_id.as_uuid()).bind(owner.as_str())
    .bind(records.iter().map(|r| r.id.as_bytes().to_vec()).collect::<Vec<_>>())
    .bind(records.iter().map(|r| r.claim.fence().digest().as_bytes().to_vec()).collect::<Vec<_>>())
    .bind(records.iter().map(|r| r.claim.claimed_at().unix_millis()).collect::<Vec<_>>())
    .bind(records.iter().map(|r| r.claim.expires_at().unix_millis()).collect::<Vec<_>>())
    .execute(&database.pool).await.unwrap();
  let claimed_at = at(983_100_000);
  let record = FactoryRunClaimRecord::new(
    run_id,
    key("current-owner"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([73; 32])),
      claimed_at,
      at(983_130_000),
    )
    .unwrap(),
  );
  runs
    .claim_factory_run(ClaimFactoryRun {
      run_id,
      expected_version: before.run.version(),
      record: record.clone(),
      audit: FactoryAuditFact::new(
        run_id,
        octacity_server_store::AuditActorKind::Worker,
        None,
        key("factory.claimed"),
        record.id,
        key("claimed"),
        claimed_at,
      ),
    })
    .await
    .unwrap();
  let snapshot = runs
    .factory_run_snapshot(run_id)
    .await
    .expect("expired ownership history must not consume the working snapshot limit");
  assert_eq!(snapshot.current_claim, Some(record.clone()));
  let mut after = None;
  let mut retained = std::collections::BTreeSet::new();
  loop {
    let page = runs
      .list_factory_run_diagnostics(
        ListFactoryRunDiagnostics::new(run_id, FactoryRunDiagnosticKind::Claim, after, 100).unwrap(),
      )
      .await
      .unwrap();
    assert!(page.items.len() <= 100);
    for item in page.items {
      assert!(
        retained.insert(item.cursor()),
        "history pages must not duplicate a claim"
      );
    }
    after = page.next;
    if after.is_none() {
      break;
    }
  }
  assert_eq!(retained.len(), records.len() + 2);
  assert!(retained.contains(&key(&record.id.to_string())));
  let audits = runs
    .list_factory_run_diagnostics(
      ListFactoryRunDiagnostics::new(run_id, FactoryRunDiagnosticKind::Audit, None, 100).unwrap(),
    )
    .await
    .unwrap();
  assert!(
    audits
      .items
      .iter()
      .any(|row| matches!(row, FactoryRunDiagnosticRecord::Audit(fact) if fact.request_identity_digest == record.id))
  );
  let outcome = runs
    .apply_factory_run_control(octacity_server_store::testing::management_mutation(
      ApplyFactoryRunControl {
        run_id,
        expected_version: snapshot.run.version(),
        idempotency_key: IdempotencyKey::new("cancel-long-wait").unwrap(),
        intent: FactoryRunControlIntent::Cancel,
        requested_at: at(983_100_001),
      },
    ))
    .await
    .unwrap();
  assert!(outcome.cancellation_requested);
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn a_current_claim_cannot_belong_to_another_run() {
  let database = postgres_support::TestDatabase::migrated().await;
  let admissions = failure_contract::single_slot_admissions();
  let store = admit_runs(&database.pool, &admissions).await;
  let (claims, _, _) = failure_contract::claim_configuration_nodes(store.as_ref(), &admissions).await;
  assert_eq!(
    store
      .factory_run_snapshot(admissions[0].run.id())
      .await
      .unwrap()
      .current_claim,
    Some(claims[0].clone())
  );
  // Corrupt only the retained pointer: the foreign proof remains canonical,
  // so digest validation alone cannot establish ownership of this Run.
  sqlx::query("UPDATE factory_run_current SET claim_id=$1 WHERE run_id=$2")
    .bind(claims[1].id.as_bytes().as_slice())
    .bind(admissions[0].run.id().as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  assert!(
    matches!(
      store.factory_run_snapshot(admissions[0].run.id()).await,
      Err(StoreError::Unavailable)
    ),
    "a foreign Run's valid proof must not authorize this Run"
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn ordinary_build_retry_cannot_restart_a_cancelled_factory_operation() {
  let (database, runs, request) = prepared_factory_build(true).await;
  let store = octacity_server_store_postgres::PostgresAuthoritativeStore::new(
    database.pool.clone(),
    postgres_support::test_signer(),
  );
  let accepted = store.accept_factory_build(request.clone()).await.unwrap();
  // Retained ordinary failure is a fixture; the owning Run's cancellation is
  // accepted through its actual control port before any retry is attempted.
  sqlx::query("UPDATE builds SET state='failed' WHERE id=$1")
    .bind(accepted.build_id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  sqlx::query("UPDATE attempts SET state='failed' WHERE id=$1")
    .bind(accepted.attempt_id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  sqlx::query("UPDATE jobs SET state='skipped' WHERE attempt_id=$1")
    .bind(accepted.attempt_id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  sqlx::query("DELETE FROM ready_queue_entries WHERE job_id IN (SELECT id FROM jobs WHERE attempt_id=$1)")
    .bind(accepted.attempt_id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  let run_id = request.intent.input().factory_run_id();
  let snapshot = runs.factory_run_snapshot(run_id).await.unwrap();
  runs
    .apply_factory_run_control(octacity_server_store::testing::management_mutation(
      ApplyFactoryRunControl {
        run_id,
        expected_version: snapshot.run.version(),
        idempotency_key: IdempotencyKey::new("stop-before-build-retry").unwrap(),
        intent: FactoryRunControlIntent::Cancel,
        requested_at: at(13),
      },
    ))
    .await
    .unwrap();
  let mut source = octacity_server_store::testing::authoritative_store_contract_fixture().request;
  source.build = request.build;
  source.jobs = request.jobs;
  let retry = octacity_server_store::testing::retry_request(&source, 99_001, "cannot-resurrect-node", at(14));
  assert!(
    matches!(
      store
        .retry_build(octacity_server_store::testing::management_mutation(retry))
        .await,
      Err(StoreError::Conflict {
        entity: EntityKind::Build
      })
    ),
    "Build Retry does not carry current Factory node authority"
  );
  assert_eq!(
    runs.latest_attempt(accepted.build_id).await.unwrap().id,
    accepted.attempt_id
  );
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn a_multi_job_build_cannot_multiply_the_node_output_budget() {
  let (database, _, request) = prepared_factory_build_with_output_budget(true, 3_000).await;
  assert_eq!(request.jobs.len(), 2);
  let operation = request.intent.operation_id().unwrap();
  let store = octacity_server_store_postgres::PostgresAuthoritativeStore::new(
    database.pool.clone(),
    postgres_support::test_signer(),
  );
  assert!(
    store.accept_factory_build(request).await.is_err(),
    "two Job grants of 2048 bytes cannot fit a 3000 byte Node budget"
  );
  assert!(store.factory_build_for_operation(operation).await.unwrap().is_none());
  database.cleanup().await;
}

async fn prepared_factory_build(
  managed: bool,
) -> (postgres_support::TestDatabase, Arc<PostgresStore>, AcceptFactoryBuild) {
  prepared_factory_build_with_output_budget(managed, 10_000).await
}

async fn prepared_factory_build_with_output_budget(
  managed: bool,
  output_bytes: u64,
) -> (postgres_support::TestDatabase, Arc<PostgresStore>, AcceptFactoryBuild) {
  prepared_factory_build_with_capture_paths(managed, output_bytes, None).await
}
async fn prepared_factory_build_with_capture_paths(
  managed: bool,
  output_bytes: u64,
  capture_paths: Option<&[&str]>,
) -> (postgres_support::TestDatabase, Arc<PostgresStore>, AcceptFactoryBuild) {
  let database = postgres_support::TestDatabase::migrated().await;
  let admissions = [protected_build_admission_with_capture_paths(
    managed,
    output_bytes,
    capture_paths,
  )];
  let admissions = &admissions[..];
  let store = admit_runs(&database.pool, admissions).await;
  let (claims, targets, inputs) = failure_contract::claim_configuration_nodes(store.as_ref(), admissions).await;
  FactoryNodeRunner::new(
    store.clone(),
    Arc::new(NeverDispatch),
    factory_node_test_support::clock(),
  )
  .run_once(
    admissions[0].run.id(),
    claims[0].id,
    targets[0].clone(),
    Some(inputs[0].clone()),
    at(11),
  )
  .await
  .unwrap();
  let snapshot = store.factory_run_snapshot(admissions[0].run.id()).await.unwrap();
  let intent = snapshot.flow.data.build_intents[0].clone();
  let fixture = octacity_server_store::testing::authoritative_store_contract_fixture();
  let mut build = fixture.request.build.clone();
  build.project_id = intent.input().subject().project_id();
  build.repository_id = intent.input().subject().repository_id();
  build.configuration_id = intent.profile().build_configuration.id();
  build.configuration_version = intent.profile().build_configuration.version();
  build.immutable_revision = intent.input().subject().base_revision().clone();
  build.priority = intent.priority();
  sqlx::query("INSERT INTO pools (id,version,name,enabled,drain_state,admission_policy,concurrency_limit,created_at) VALUES ($1,1,'factory-build',true,'accepting','{\"mode\":\"any\"}',4,to_timestamp(0))")
    .bind(fixture.allowed_pool.as_uuid()).execute(&database.pool).await.unwrap();
  sqlx::query("INSERT INTO pipelines (id,project_id,name,created_at) VALUES ($1,$2,'factory-build',to_timestamp(0))")
    .bind(build.pipeline_id.as_uuid())
    .bind(build.project_id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  sqlx::query(
    "INSERT INTO pipeline_versions (pipeline_id,version,dag_snapshot,published_at) VALUES ($1,$2,'{}',to_timestamp(0))",
  )
  .bind(build.pipeline_id.as_uuid())
  .bind(build.pipeline_version.get() as i64)
  .execute(&database.pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO build_configurations (id,project_id,name,created_at) VALUES ($1,$2,'factory-build',to_timestamp(0))",
  )
  .bind(build.configuration_id.as_uuid())
  .bind(build.project_id.as_uuid())
  .execute(&database.pool)
  .await
  .unwrap();
  sqlx::query("INSERT INTO build_configuration_versions (build_configuration_id,version,enabled,repository_id,repository_version,pipeline_id,pipeline_version,configuration_snapshot,job_concurrency_limit,allowed_pool_ids,retry_max_attempts,retries_infrastructure,published_at) VALUES ($1,$2,true,$3,$4,$5,$6,'{}',4,ARRAY[$7::uuid],1,false,to_timestamp(0))")
    .bind(build.configuration_id.as_uuid()).bind(build.configuration_version.get() as i64)
    .bind(build.repository_id.as_uuid()).bind(build.repository_version.get() as i64)
    .bind(build.pipeline_id.as_uuid()).bind(build.pipeline_version.get() as i64)
    .bind(fixture.allowed_pool.as_uuid()).execute(&database.pool).await.unwrap();
  let accepted_at = at(12);
  let jobs = fixture
    .request
    .jobs
    .iter()
    .cloned()
    .map(|mut job| {
      job.allowed_pools = vec![fixture.allowed_pool];
      if managed {
        let canonical = factory_job_test_support::canonical_v3_spec();
        let input = intent.input();
        let causality = octacity_protocol::FactoryCausalityV3 {
          factory_run_id: input.factory_run_id().to_string(),
          factory_configuration_id: input.configuration().id().to_string(),
          factory_configuration_version: input.configuration().version().get(),
          flow_run_id: input.flow_run_id().to_string(),
          workflow_cycle_id: input.cycle_id().to_string(),
          node_attempt_id: intent.node().id().to_string(),
          input_digest: input.digest().unwrap().to_string(),
          subject_digest: input.subject().digest().to_string(),
          source_write_paths: capture_paths.map(|paths| paths.iter().map(|path| (*path).to_owned()).collect()),
          change_set_capture: capture_paths.map(|paths| octacity_protocol::ChangeSetCaptureV3 {
            author_name: "Factory fixture".to_owned(),
            author_email: "factory@example.test".to_owned(),
            committed_at: 100,
            patch_max_bytes: None,
            policy: octacity_protocol::ChangeSetCapturePolicyV3 {
              allowed_path_prefixes: paths.iter().map(|path| (*path).to_owned()).collect(),
              forbidden_control_paths: vec![".git".to_owned()],
              max_changed_paths: 64,
              max_file_bytes: 1024,
              max_total_bytes: 2048,
              allow_empty: false,
              binary_policy: octacity_protocol::ChangeSetBinaryPolicyV3::Reject,
              forbidden_secret_patterns: octacity_protocol::ChangeSetSecretPatternV3::ALL.to_vec(),
            },
          }),
          change_set_materialization: None,
        };
        let grants = intent.permissions().clone();
        job.job_spec_template = derive_factory_managed_job_spec_template(FactoryManagedJobSpecInput {
          build: octacity_server_job::JobSpecBuildSnapshot::new(
            build.id,
            build.immutable_revision.clone(),
            None,
            RepositoryLocator::new("https://example.test/source.git").unwrap(),
            Default::default(),
          )
          .unwrap(),
          pipeline_node_id: job.pipeline_node_id.clone(),
          execution: canonical.execution.clone(),
          causality,
          protected_inputs: canonical.protected_inputs.clone(),
          policy_layers: FactoryBuildPolicyLayers {
            project: grants.clone(),
            configuration: grants.clone(),
            task: grants.clone(),
            local: LocalPermissionCeiling::fully_enforced(grants),
          },
          required_enforcement: canonical.required_enforcement.clone(),
          policy: factory_job_test_support::policy(&canonical),
        })
        .unwrap()
        .with_execution_deadline(intent.node().deadline())
        .unwrap();
        job.requirements.minimum_cpu_millis = canonical.runtime.cpu_millis;
        job.requirements.minimum_memory_bytes = canonical.runtime.memory_bytes;
        job.requirements.minimum_disk_bytes = canonical.runtime.writable_disk_bytes;
        job.requirements.runtime_class = RuntimeClass::Virtualization;
        job.requirements.operating_system = canonical.runtime.target.target_platform.os;
        job.requirements.architecture = canonical.runtime.target.target_platform.architecture;
        job.requirements.host_platform = Some(canonical.runtime.target.host_platform);
        job.requirements.required_guarantees = canonical.runtime.target.required_guarantees.iter().copied().collect();
      }
      job
    })
    .collect();
  let request = AcceptFactoryBuild {
    intent: intent.clone(),
    ownership: FactoryClaimOwnership::new(claims[0].owner.clone(), claims[0].claim),
    build,
    attempt_id: fixture.request.attempt_id,
    jobs,
    accepted_at,
  };
  (database, store, request)
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn a_persisted_node_intent_creates_one_ordinary_build_under_current_ownership() {
  let (database, store, request) = prepared_factory_build(true).await;
  let intent = request.intent.clone();
  let authoritative = octacity_server_store_postgres::PostgresAuthoritativeStore::new(
    database.pool.clone(),
    postgres_support::test_signer(),
  );
  let mut unsorted = request.clone();
  unsorted.jobs.reverse();
  assert!(authoritative.accept_factory_build(unsorted).await.is_err());
  let mut stale = request.clone();
  stale.ownership = FactoryClaimOwnership::new(key("another.owner"), request.ownership.claim());
  assert!(authoritative.accept_factory_build(stale).await.is_err());
  assert!(
    authoritative
      .factory_build_for_operation(intent.operation_id().unwrap())
      .await
      .unwrap()
      .is_none()
  );
  let accepted = authoritative.accept_factory_build(request.clone()).await.unwrap();
  let restarted = octacity_server_store_postgres::PostgresAuthoritativeStore::new(
    database.pool.clone(),
    postgres_support::test_signer(),
  );
  let mut late_replay = request.clone();
  late_replay.accepted_at = at(1_000_000);
  assert_eq!(restarted.accept_factory_build(late_replay).await.unwrap(), accepted);
  assert_eq!(
    restarted
      .factory_build_for_operation(intent.operation_id().unwrap())
      .await
      .unwrap(),
    Some(accepted.clone())
  );
  let actual = store.build(accepted.build_id).await.unwrap();
  assert_eq!(
    actual.origin,
    BuildOrigin::Factory(FactoryBuildOrigin::from_intent(&intent).unwrap())
  );
  assert_eq!(actual.build, request.build);
  let attempt = store.attempt(accepted.attempt_id).await.unwrap();
  assert_eq!(attempt.jobs.len(), request.jobs.len());
  assert_eq!(
    attempt.jobs.iter().filter(|job| job.queue.is_some()).count(),
    request.jobs.iter().filter(|job| job.dependencies.is_empty()).count()
  );
  let mut conflicting = request;
  conflicting.build.priority += 1;
  assert!(restarted.accept_factory_build(conflicting).await.is_err());
  database.cleanup().await;
}

#[async_trait]
impl FactoryNodeExecutor for NeverDispatch {
  async fn observe(&self, _: FactoryNodeExecutionRequest<'_>) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    panic!("creating a fenced intent must precede dispatch")
  }
  async fn cancel(
    &self,
    _: FactoryNodeExecutionRequest<'_>,
    _: Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    panic!("capacity acquisition does not cancel executions")
  }
}
#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn concurrent_node_appends_share_one_configuration_capacity_lock() {
  let database = postgres_support::TestDatabase::migrated().await;
  let admissions = failure_contract::single_slot_admissions();
  let store = admit_runs(&database.pool, &admissions).await;
  let (claims, targets, inputs) = failure_contract::claim_configuration_nodes(store.as_ref(), &admissions).await;
  let first = FactoryNodeRunner::new(
    store.clone(),
    Arc::new(NeverDispatch),
    factory_node_test_support::clock(),
  );
  let second = FactoryNodeRunner::new(
    Arc::new(PostgresStore::new(database.pool.clone())),
    Arc::new(NeverDispatch),
    factory_node_test_support::clock(),
  );
  let before = [
    store.factory_run_snapshot(admissions[0].run.id()).await.unwrap(),
    store.factory_run_snapshot(admissions[1].run.id()).await.unwrap(),
  ];
  let outcomes = tokio::join!(
    first.run_once(
      admissions[0].run.id(),
      claims[0].id,
      targets[0].clone(),
      Some(inputs[0].clone()),
      at(11)
    ),
    second.run_once(
      admissions[1].run.id(),
      claims[1].id,
      targets[1].clone(),
      Some(inputs[1].clone()),
      at(11)
    ),
  );
  assert_eq!(
    usize::from(outcomes.0.is_ok()) + usize::from(outcomes.1.is_ok()),
    1,
    "exactly one concurrent owner can acquire the configured slot"
  );
  for (index, outcome) in [outcomes.0, outcomes.1].into_iter().enumerate() {
    let snapshot = store.factory_run_snapshot(admissions[index].run.id()).await.unwrap();
    if outcome.is_ok() {
      assert_eq!(snapshot.flow.runtime().active_node_count(), 1);
    } else {
      assert_eq!(
        snapshot, before[index],
        "capacity rejection must not append history or advance the projection"
      );
    }
  }
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn root_progress_rejects_a_retired_global_signal_phase_in_postgres() {
  let database = postgres_support::TestDatabase::migrated().await;
  let admission = human_contract::human_fixture();
  let store = admit(&database.pool, &admission).await;
  let before = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  sqlx::query("UPDATE factory_lifecycle_checkpoints SET lifecycle = lifecycle || '{\"signal\":\"Disabled\"}'::jsonb WHERE id = $1")
    .bind(before.current.lifecycle_checkpoint_id.as_bytes().as_slice())
    .execute(&database.pool).await.unwrap();
  assert!(
    store.factory_run_snapshot(admission.run.id()).await.is_err(),
    "signals are configured node operations; the root must not restore a global signal phase"
  );
  database.cleanup().await;
}

#[allow(dead_code)]
#[path = "support/runtime.rs"]
mod runtime_fixture;

struct RuntimeInstalledNode {
  observed: tokio::sync::Notify,
  observations: std::sync::atomic::AtomicUsize,
}
#[async_trait]
impl FactoryNodeExecutor for RuntimeInstalledNode {
  async fn observe(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
  ) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    request.input.verify_attempt(request.attempt).unwrap();
    self.observations.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    self.observed.notify_one();
    Ok(FactoryNodeExecutionStep::Waiting { execution: None })
  }
  async fn cancel(
    &self,
    _: FactoryNodeExecutionRequest<'_>,
    _: Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    Ok(FactoryNodeCancellationStep::Stopped {
      execution: None,
      usage: BudgetUsage::default(),
    })
  }
}
struct RuntimeCapabilities {
  project: ProjectId,
  choices: FactoryConfigurationChoices,
}
#[async_trait]
impl octacity_server_store_postgres::FactoryCapabilitySource for RuntimeCapabilities {
  async fn availability(&self, project: ProjectId) -> Result<FactoryConfigurationAvailability, StoreError> {
    assert_eq!(project, self.project);
    Ok(FactoryConfigurationAvailability::Available(self.choices.clone()))
  }
}
struct RuntimeCapabilityHealth(std::sync::atomic::AtomicBool);
#[async_trait]
impl octacity_server::FactoryRuntimeReadiness for RuntimeCapabilityHealth {
  async fn available(&self) -> bool {
    self.0.load(std::sync::atomic::Ordering::SeqCst)
  }
}
struct RuntimeInstaller {
  node: Arc<RuntimeInstalledNode>,
  kind: FlowNodeKind,
  execution: NodeExecutionIdentity,
  capabilities: Arc<RuntimeCapabilities>,
  health: Arc<RuntimeCapabilityHealth>,
}
#[async_trait]
impl octacity_server::FactoryRuntimeInstaller for RuntimeInstaller {
  async fn install(
    &self,
    _: octacity_server::FactoryRuntimeResources,
  ) -> Result<octacity_server::FactoryRuntimeInstallation, octacity_server::FactoryRuntimeInstallationError> {
    Ok(octacity_server::FactoryRuntimeInstallation {
      executors: vec![(self.kind, self.execution.clone(), self.node.clone())],
      inputs: Arc::new(flow_contract::Inputs),
      capabilities: self.capabilities.clone(),
      readiness: self.health.clone(),
    })
  }
}
#[tokio::test]
#[ignore = "requires disposable PostgreSQL and object storage"]
async fn production_factory_owner_uses_installed_bindings_and_recovers_the_same_arbitrary_node() {
  let database = postgres_support::TestDatabase::migrated().await;
  let admission = protected_build_admission(true);
  let store = admit(&database.pool, &admission).await;
  let declared = &admission.flow.closure().definitions()[0].nodes()[0];
  let node = Arc::new(RuntimeInstalledNode {
    observed: tokio::sync::Notify::new(),
    observations: Default::default(),
  });
  let health = Arc::new(RuntimeCapabilityHealth(std::sync::atomic::AtomicBool::new(true)));
  let installer = Arc::new(RuntimeInstaller {
    node: node.clone(),
    kind: declared.kind(),
    execution: NodeExecutionIdentity::External(declared.build().unwrap().tool.clone()),
    capabilities: Arc::new(RuntimeCapabilities {
      project: admission.work.subject().project_id(),
      choices: postgres_support::factory_configuration::fixture(admission.work.subject().project_id()).choices,
    }),
    health: health.clone(),
  });
  let mut url = url::Url::parse(&std::env::var("OCTACITY_POSTGRES_URL").unwrap()).unwrap();
  url.set_path(&database.name);
  let directory = tempfile::tempdir().unwrap();
  let config = octacity_server::ServerConfig::parse_toml(&runtime_fixture::runtime_config_source(
    directory.path(),
    url.as_str(),
    &std::env::var("OCTACITY_MINIO_ENDPOINT").unwrap(),
    "[factory]\npoll_interval_milliseconds=10\nclaim_lifetime_milliseconds=100\nbatch_size=4\nconcurrency=2",
  ))
  .unwrap();
  let runtime = octacity_server::ServerRuntime::start_with_factory(config.clone(), installer.clone())
    .await
    .unwrap();
  tokio::time::timeout(std::time::Duration::from_secs(5), node.observed.notified())
    .await
    .unwrap();
  let frozen = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  assert_eq!(frozen.flow.attempts.len(), 1);
  assert_eq!(frozen.flow.attempts[0].node_key(), declared.key());
  assert_eq!(frozen.flow.attempts[0].execution(), &installer.execution);
  let client = reqwest::Client::new();
  let origin = format!("http://{}", runtime.management_addr());
  tokio::time::timeout(std::time::Duration::from_secs(5), async {
    loop {
      if client
        .get(format!("{origin}/health/ready"))
        .send()
        .await
        .unwrap()
        .status()
        .is_success()
      {
        break;
      }
      tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
  })
  .await
  .unwrap();
  health.0.store(false, std::sync::atomic::Ordering::SeqCst);
  tokio::time::timeout(std::time::Duration::from_secs(2), async {
    loop {
      if client
        .get(format!("{origin}/health/ready"))
        .send()
        .await
        .unwrap()
        .status()
        == reqwest::StatusCode::SERVICE_UNAVAILABLE
      {
        break;
      }
      tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
  })
  .await
  .unwrap();
  runtime.shutdown().await.unwrap();
  health.0.store(true, std::sync::atomic::Ordering::SeqCst);
  let before_restart = node.observations.load(std::sync::atomic::Ordering::SeqCst);
  let restarted = octacity_server::ServerRuntime::start_with_factory(config, installer)
    .await
    .unwrap();
  tokio::time::timeout(std::time::Duration::from_secs(5), node.observed.notified())
    .await
    .unwrap();
  tokio::time::timeout(std::time::Duration::from_secs(5), async {
    while node.observations.load(std::sync::atomic::Ordering::SeqCst) <= before_restart {
      tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
  })
  .await
  .unwrap();
  let recovered = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  assert_eq!(recovered.flow.attempts, frozen.flow.attempts);
  assert_eq!(recovered.flow.data.inputs, frozen.flow.data.inputs);
  assert_eq!(recovered.flow.data.build_intents, frozen.flow.data.build_intents);
  restarted.shutdown().await.unwrap();
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn signed_job_capture_cannot_widen_the_frozen_node_write_scope() {
  let (database, _, request) =
    prepared_factory_build_with_capture_paths(true, 10_000, Some(&["contracts", "plans/changes"])).await;
  request.validate().unwrap();
  let store = octacity_server_store_postgres::PostgresAuthoritativeStore::new(
    database.pool.clone(),
    postgres_support::test_signer(),
  );
  let operation = request.intent.operation_id().unwrap();
  for roots in [
    serde_json::Value::Null,
    serde_json::json!(["contracts"]),
    serde_json::json!(["src"]),
  ] {
    let mut substituted = request.clone();
    let mut wire = serde_json::to_value(&substituted.jobs[0].job_spec_template).unwrap();
    *wire.pointer_mut("/execution/factory/source_write_paths").unwrap() = roots;
    substituted.jobs[0].job_spec_template = serde_json::from_value(wire).unwrap();
    assert!(
      store.accept_factory_build(substituted).await.is_err(),
      "capture filtering cannot replace the exact signed writable directory scope"
    );
    assert!(store.factory_build_for_operation(operation).await.unwrap().is_none());
  }
  for whitelist in [
    serde_json::json!(["."]),
    serde_json::json!(["contracts"]),
    serde_json::Value::Null,
  ] {
    let mut substituted = request.clone();
    let mut wire = serde_json::to_value(&substituted.jobs[0].job_spec_template).unwrap();
    let capture = wire
      .pointer_mut("/execution/factory/change_set_capture")
      .expect("managed template retains the exact capture policy");
    if whitelist.is_null() {
      *capture = serde_json::Value::Null;
    } else {
      capture["policy"]["allowed_path_prefixes"] = whitelist;
    }
    substituted.jobs[0].job_spec_template = serde_json::from_value(wire).unwrap();
    assert!(
      store.accept_factory_build(substituted).await.is_err(),
      "a signed job cannot substitute even one DAG member's effective write scope"
    );
    assert!(store.factory_build_for_operation(operation).await.unwrap().is_none());
  }
  let accepted = store.accept_factory_build(request).await.unwrap();
  assert_eq!(
    store.factory_build_for_operation(operation).await.unwrap(),
    Some(accepted)
  );
  database.cleanup().await;
}
