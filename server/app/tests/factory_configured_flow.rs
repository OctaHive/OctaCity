//! Same public configured-owner contract under PostgreSQL persistence and restart.
use async_trait::async_trait;
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
  fixture.draft.evaluation.budget = fixture.draft.hard_budget;
  for stage in &mut fixture.draft.stages {
    stage.budget = fixture.draft.hard_budget;
  }
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
