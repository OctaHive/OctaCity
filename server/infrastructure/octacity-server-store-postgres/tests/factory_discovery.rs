mod support;

use std::collections::BTreeSet;

use octacity_server_domain::{
  ArtifactId, BuildConfigurationId, BuildConfigurationVersion, ImmutableRevision, IntegrationId, ProjectId,
  RepositoryId, RepositoryLocator, RepositoryName, RepositoryVersion, SourceReference, Timestamp,
};
use octacity_server_factory::{
  BudgetLimit, BuildConfigurationRef, DeliveryPolicyDraft, EvaluationPolicyDraft, ExactSubject, ExternalWorkIdentity,
  FactoryArtifactReference, FactoryChoiceKind, FactoryConfiguration, FactoryConfigurationChoiceEntries,
  FactoryConfigurationChoices, FactoryConfigurationDraft, FactoryConfigurationId, FactoryConfigurationVersion,
  FactoryCredentialProfiles, FactoryDigest, FactoryKey, FactoryMetadata, FactoryReferenceChoice, FactoryRun,
  FactoryRunId, FactoryStageDraft, FactoryStageKind, FactoryWipLimits, ImmutableReference, ReworkPolicyDraft,
  RiskClass, WorkArtifacts, WorkClassification, WorkEnvelope, WorkEnvelopeId, WorkPriority,
};
use octacity_server_store::{
  AdmitFactoryWork, CreateFactoryConfiguration, FactoryAdmissionProbe, FactoryAdmissionStore as _,
  FactoryConfigurationListVisibility, FactoryConfigurationMutationIntent, FactoryConfigurationStore as _,
  FactoryDiscoveryStore as _, FactoryRunFilter, FactoryRunListVisibility, FactoryRunPage, FactoryWorkSourceScope,
  IdempotencyKey, ListFactoryRuns, ListProjectFactoryConfigurations, PublishedRepository, ReplaceFactoryConfiguration,
  RepositoryDefinition, RepositorySelectionPolicy,
  testing::{InMemoryFactoryConfigurationStore, management_mutation_with_request},
};
use octacity_server_store_postgres::PostgresStore;
use serde_json::Value;
use sqlx::{Executor as _, types::Json};
use support::TestDatabase;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn factory_configuration_admission_and_discovery_match_the_in_memory_contract() {
  let database = TestDatabase::migrated().await;
  let memory = InMemoryFactoryConfigurationStore::new();
  let postgres = PostgresStore::new(database.pool.clone());
  let foreign_project_id = project_id(2);
  let project_id = project_id(1);
  seed_projects(&database.pool, project_id, foreign_project_id).await;
  memory.seed_project(project_id).unwrap();
  memory.seed_project(foreign_project_id).unwrap();

  let fixture = fixture(project_id);
  memory.enable_project(project_id, fixture.choices.clone()).unwrap();
  let published_at = time(10);
  let configuration_ids = [101_u128, 104, 103, 102].map(configuration_id);
  for (offset, configuration_id) in configuration_ids.into_iter().enumerate() {
    let configuration = FactoryConfiguration::publish(
      configuration_id,
      FactoryConfigurationVersion::INITIAL,
      project_id,
      digest(u8::try_from(20 + offset).unwrap()),
      fixture.draft.clone(),
      &fixture.choices,
    )
    .unwrap();
    let intent = FactoryConfigurationMutationIntent::Create {
      id: configuration_id,
      project_id,
      definition_digest: configuration.reference().definition_digest(),
      draft: fixture.draft.clone(),
    };
    let request = CreateFactoryConfiguration {
      configuration,
      idempotency_key: idempotency(&format!("factory-config-{configuration_id}")),
      published_at,
      intent,
    };
    let expected = memory
      .create_factory_configuration(management_mutation_with_request(
        request.clone(),
        format!("memory-config-{offset}"),
      ))
      .await
      .unwrap();
    let actual = postgres
      .create_factory_configuration(management_mutation_with_request(
        request,
        format!("postgres-config-{offset}"),
      ))
      .await
      .unwrap();
    assert_eq!(actual.configuration, expected.configuration);
  }

  let initial = memory
    .factory_configuration_version(configuration_id(101), FactoryConfigurationVersion::INITIAL)
    .await
    .unwrap();
  let replacement = initial
    .configuration
    .replace(
      FactoryConfigurationVersion::new(2).unwrap(),
      digest(40),
      fixture.draft.clone(),
      &fixture.choices,
    )
    .unwrap();
  let replacement_request = ReplaceFactoryConfiguration {
    expected_current_version: FactoryConfigurationVersion::INITIAL,
    configuration: replacement,
    idempotency_key: idempotency("replace-factory-config-101"),
    published_at: time(11),
    intent: FactoryConfigurationMutationIntent::Replace {
      id: configuration_id(101),
      expected_current_version: FactoryConfigurationVersion::INITIAL,
      definition_digest: digest(40),
      draft: fixture.draft.clone(),
    },
  };
  let expected = memory
    .replace_factory_configuration(management_mutation_with_request(
      replacement_request.clone(),
      "memory-replacement",
    ))
    .await
    .unwrap();
  let actual = postgres
    .replace_factory_configuration(management_mutation_with_request(
      replacement_request,
      "postgres-replacement",
    ))
    .await
    .unwrap();
  assert_eq!(actual.configuration, expected.configuration);
  assert_eq!(
    postgres
      .current_factory_configuration(configuration_id(101))
      .await
      .unwrap(),
    memory
      .current_factory_configuration(configuration_id(101))
      .await
      .unwrap(),
  );

  let configuration_visibility = || {
    FactoryConfigurationListVisibility::restricted([
      configuration_id(101),
      configuration_id(103),
      configuration_id(102),
    ])
    .unwrap()
  };
  let first_request =
    || ListProjectFactoryConfigurations::new(project_id, None, 2, configuration_visibility()).unwrap();
  let expected = memory
    .list_project_factory_configurations(first_request())
    .await
    .unwrap();
  let actual = postgres
    .list_project_factory_configurations(first_request())
    .await
    .unwrap();
  assert_eq!(actual, expected);
  assert_eq!(actual.total, 3);
  assert_eq!(
    configuration_ids_from(&actual.items),
    [configuration_id(103), configuration_id(102)]
  );

  let second_request =
    || ListProjectFactoryConfigurations::new(project_id, actual.next_cursor, 2, configuration_visibility()).unwrap();
  let expected = memory
    .list_project_factory_configurations(second_request())
    .await
    .unwrap();
  let actual = postgres
    .list_project_factory_configurations(second_request())
    .await
    .unwrap();
  assert_eq!(actual, expected);
  assert_eq!(actual.total, 3, "count is computed before cursor pagination");
  assert_eq!(configuration_ids_from(&actual.items), [configuration_id(101)]);

  let repository = repository(project_id, repository_id(301));
  seed_repository(&database.pool, &repository).await;
  memory.seed_repository_version(repository.clone()).unwrap();
  let admissions = [
    admission(501, 601, "work-1", 30, project_id, configuration_id(101), &repository),
    admission(502, 602, "work-2", 30, project_id, configuration_id(101), &repository),
    admission(503, 603, "work-3", 20, project_id, configuration_id(101), &repository),
    admission(504, 604, "work-4", 10, project_id, configuration_id(101), &repository),
  ];
  let replay_probe = admissions[0].probe.clone();
  for (index, request) in admissions.into_iter().enumerate() {
    let expected = memory
      .admit_factory_work(management_mutation_with_request(
        request.clone(),
        format!("memory-admission-{index}"),
      ))
      .await
      .unwrap();
    let actual = postgres
      .admit_factory_work(management_mutation_with_request(
        request,
        format!("postgres-admission-{index}"),
      ))
      .await
      .unwrap();
    assert_eq!(actual.admission, expected.admission);
  }
  let expected = memory.replay_factory_admission(&replay_probe).await.unwrap().unwrap();
  let actual = postgres.replay_factory_admission(&replay_probe).await.unwrap().unwrap();
  assert_eq!(actual, expected);

  let run_visibility = || FactoryRunListVisibility::restricted([run_id(501), run_id(502), run_id(504)]).unwrap();
  let first_request =
    || ListFactoryRuns::new(Some(project_id), FactoryRunFilter::default(), None, 2, run_visibility()).unwrap();
  let first = assert_run_parity(&memory, &postgres, first_request()).await;
  assert_eq!(first.total, 3);
  assert_eq!(run_ids(&first), [run_id(502), run_id(501)]);

  let second_request = || {
    ListFactoryRuns::new(
      Some(project_id),
      FactoryRunFilter::default(),
      first.next_cursor,
      2,
      run_visibility(),
    )
    .unwrap()
  };
  let second = assert_run_parity(&memory, &postgres, second_request()).await;
  assert_eq!(second.total, 3);
  assert_eq!(run_ids(&second), [run_id(504)]);
  assert_eq!(second.next_cursor, None);

  let filtered = assert_run_parity(
    &memory,
    &postgres,
    ListFactoryRuns::new(
      None,
      FactoryRunFilter {
        configuration_id: Some(configuration_id(101)),
        source: Some(key("manual")),
        state: Some(octacity_server_factory::FactoryRunState::Admitted),
        admitted_from: Some(time(20)),
        admitted_before: Some(time(31)),
      },
      None,
      20,
      FactoryRunListVisibility::all(),
    )
    .unwrap(),
  )
  .await;
  assert_eq!(filtered.total, 3);
  assert_eq!(run_ids(&filtered), [run_id(502), run_id(501), run_id(503)]);

  let detail = postgres
    .factory_run_summary(
      run_id(501),
      FactoryRunListVisibility::restricted([run_id(501)]).unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(detail.id, run_id(501));
  assert!(
    postgres
      .factory_run_summary(run_id(501), FactoryRunListVisibility::none())
      .await
      .is_err()
  );

  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn representative_factory_discovery_plans_use_only_justified_indexes() {
  let database = TestDatabase::migrated().await;
  let foreign_project_id = project_id(2);
  let project_id = project_id(1);
  seed_projects(&database.pool, project_id, foreign_project_id).await;
  let repository = repository(project_id, repository_id(301));
  seed_repository(&database.pool, &repository).await;
  seed_plan_rows(&database.pool, project_id, repository.id).await;
  sqlx::query("ANALYZE factory_configurations, factory_configuration_versions, factory_work_envelopes, factory_runs")
    .execute(&database.pool)
    .await
    .unwrap();
  let mut transaction = database.pool.begin().await.unwrap();
  transaction.execute("SET LOCAL enable_seqscan = off").await.unwrap();

  let configuration_plan = explain(
    &mut transaction,
    "SELECT id FROM factory_configurations WHERE project_id = $1 \
     ORDER BY created_at DESC, id DESC LIMIT 10",
    project_id.as_uuid(),
  )
  .await;
  assert_plan_uses(&configuration_plan, "factory_configurations_discovery_idx");

  let run_plan = explain(
    &mut transaction,
    "SELECT id FROM factory_runs WHERE project_id = $1 \
     ORDER BY admitted_at DESC, id DESC LIMIT 10",
    project_id.as_uuid(),
  )
  .await;
  assert_plan_uses(&run_plan, "factory_runs_project_discovery_idx");

  let configuration_filter_plan = explain(
    &mut transaction,
    "SELECT id FROM factory_runs WHERE factory_configuration_id = $1 \
     ORDER BY admitted_at DESC, id DESC LIMIT 10",
    configuration_id(101).as_uuid(),
  )
  .await;
  assert_plan_uses(&configuration_filter_plan, "factory_runs_configuration_discovery_idx");

  let state_plan = explain_text(
    &mut transaction,
    "SELECT id FROM factory_runs WHERE state = $1 ORDER BY admitted_at DESC, id DESC LIMIT 10",
    "admitted",
  )
  .await;
  assert_plan_uses(&state_plan, "factory_runs_state_discovery_idx");

  let source_plan = explain_text(
    &mut transaction,
    "SELECT id FROM factory_work_envelopes WHERE source_kind = $1 \
     ORDER BY admitted_at DESC, id DESC LIMIT 10",
    "manual",
  )
  .await;
  assert_plan_uses(&source_plan, "factory_work_envelopes_source_discovery_idx");

  for (statement, index) in [
    (
      "SELECT id FROM factory_stage_attempts WHERE run_id = $1 ORDER BY id LIMIT 10",
      "factory_stage_attempts_diagnostics_idx",
    ),
    (
      "SELECT id FROM factory_decision_signal_receipts WHERE run_id = $1 ORDER BY id LIMIT 10",
      "factory_decision_signal_receipts_diagnostics_idx",
    ),
    (
      "SELECT id FROM factory_decisions WHERE run_id = $1 ORDER BY id LIMIT 10",
      "factory_decisions_diagnostics_idx",
    ),
    (
      "SELECT id FROM factory_reporting_attempts WHERE run_id = $1 ORDER BY id LIMIT 10",
      "factory_reporting_attempts_diagnostics_idx",
    ),
  ] {
    let plan = explain(&mut transaction, statement, run_id(501).as_uuid()).await;
    assert_plan_uses(&plan, index);
  }

  transaction.rollback().await.unwrap();
  database.cleanup().await;
}

async fn assert_run_parity(
  memory: &InMemoryFactoryConfigurationStore,
  postgres: &PostgresStore,
  request: ListFactoryRuns,
) -> FactoryRunPage {
  let expected = memory.list_factory_runs(request.clone()).await.unwrap();
  let actual = postgres.list_factory_runs(request).await.unwrap();
  assert_eq!(actual, expected);
  actual
}

struct Fixture {
  choices: FactoryConfigurationChoices,
  draft: FactoryConfigurationDraft,
}

fn fixture(project_id: ProjectId) -> Fixture {
  let references = [
    (FactoryChoiceKind::AdmissionPolicy, "admission", "manual.medium", 1),
    (FactoryChoiceKind::PermissionCeiling, "permissions", "restricted", 2),
    (FactoryChoiceKind::CriterionPack, "criteria", "quality", 3),
    (FactoryChoiceKind::Evaluator, "evaluator", "review", 4),
    (FactoryChoiceKind::DeliveryAdapter, "delivery-adapter", "github", 5),
    (FactoryChoiceKind::DeliveryPolicy, "delivery-policy", "human-review", 6),
  ]
  .into_iter()
  .map(|(kind, alias, identity, value)| FactoryReferenceChoice {
    kind,
    alias: key(alias),
    reference: ImmutableReference::new(key(identity), key("v1"), digest(value)),
  })
  .collect();
  let choices = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
    references,
    build_configurations: vec![(
      key("build"),
      BuildConfigurationRef::new(
        BuildConfigurationId::generate(),
        BuildConfigurationVersion::INITIAL,
        project_id,
        digest(7),
      ),
    )],
  })
  .unwrap();
  let budget = || BudgetLimit::new(20, 10_000, 1_000, 10_000, 10_000).unwrap();
  let stage = |name, kind| FactoryStageDraft {
    key: key(name),
    kind,
    build_configuration: key("build"),
    budget: budget(),
  };
  let draft = FactoryConfigurationDraft {
    admission_policy: key("admission"),
    stages: vec![
      stage("implement", FactoryStageKind::Implementation),
      stage("validate", FactoryStageKind::Validation),
      stage("evaluate", FactoryStageKind::Evaluation),
    ],
    wip_limits: FactoryWipLimits::new(20, 20).unwrap(),
    hard_budget: budget(),
    permission_ceiling: key("permissions"),
    credential_profiles: credential_profiles(),
    decision_signals: Vec::new(),
    evaluation: EvaluationPolicyDraft {
      criterion_packs: vec![key("criteria")],
      evaluators: vec![key("evaluator")],
      required_quorum: 1,
      budget: budget(),
    },
    rework: ReworkPolicyDraft {
      max_cycles: 0,
      stage: None,
    },
    delivery: DeliveryPolicyDraft {
      adapter: key("delivery-adapter"),
      policy: key("delivery-policy"),
    },
    enabled: true,
  };
  Fixture { choices, draft }
}

fn credential_profiles() -> FactoryCredentialProfiles {
  FactoryCredentialProfiles::new(
    key("model-coding"),
    key("model-evaluation"),
    key("source-read"),
    key("delivery-write"),
  )
  .unwrap()
}

fn admission(
  run: u128,
  work: u128,
  external: &str,
  admitted_at: i64,
  project_id: ProjectId,
  configuration_id: FactoryConfigurationId,
  repository: &PublishedRepository,
) -> AdmitFactoryWork {
  let external_identity = ExternalWorkIdentity::new(external).unwrap();
  let work = WorkEnvelope::new(
    work_id(work),
    octacity_server_factory::FactoryConfigurationRef::new(
      configuration_id,
      FactoryConfigurationVersion::INITIAL,
      project_id,
      digest(20),
    ),
    external_identity.clone(),
    ExactSubject::new(
      project_id,
      repository.id,
      ImmutableRevision::new(format!("revision-{run}")).unwrap(),
    ),
    WorkArtifacts::new(artifact(90), artifact(91), Vec::new()).unwrap(),
    WorkClassification::new(
      WorkPriority::new(20).unwrap(),
      RiskClass::Medium,
      FactoryMetadata::default(),
    ),
  )
  .unwrap();
  let run = FactoryRun::admitted(run_id(run), &work);
  AdmitFactoryWork {
    probe: FactoryAdmissionProbe {
      source_scope: FactoryWorkSourceScope {
        source: key("manual"),
        security_scope: octacity_server_store::ManagementSecurityScope::trusted_network(),
      },
      external_identity,
      intent_digest: digest(u8::try_from(run.id().as_uuid().as_u128() % 255).unwrap()),
      idempotency_key: idempotency(&format!("admission-{}", run.id())),
    },
    repository_version: repository.version,
    work,
    run,
    admitted_at: time(admitted_at),
  }
}

fn repository(project_id: ProjectId, id: RepositoryId) -> PublishedRepository {
  let reference = SourceReference::new("refs/heads/main").unwrap();
  PublishedRepository {
    id,
    project_id,
    name: RepositoryName::new("source").unwrap(),
    version: RepositoryVersion::INITIAL,
    definition: RepositoryDefinition {
      vcs_integration_id: IntegrationId::generate(),
      repository_locator: RepositoryLocator::new("https://example.test/source.git").unwrap(),
      selection: RepositorySelectionPolicy {
        allowed_references: BTreeSet::from([reference.clone()]),
        default_reference: Some(reference),
        allow_exact_revision: true,
      },
    },
    published_at: time(1),
  }
}

async fn seed_projects(pool: &sqlx::PgPool, first: ProjectId, second: ProjectId) {
  sqlx::query(
    "INSERT INTO projects (id, parent_id, name, version, created_at, updated_at) \
     VALUES ($1, NULL, 'factory-one', 1, to_timestamp(0), to_timestamp(0)), \
            ($2, NULL, 'factory-two', 1, to_timestamp(0), to_timestamp(0))",
  )
  .bind(first.as_uuid())
  .bind(second.as_uuid())
  .execute(pool)
  .await
  .unwrap();
}

async fn seed_repository(pool: &sqlx::PgPool, repository: &PublishedRepository) {
  sqlx::query("INSERT INTO repositories (id, project_id, name, created_at) VALUES ($1, $2, $3, to_timestamp(0))")
    .bind(repository.id.as_uuid())
    .bind(repository.project_id.as_uuid())
    .bind(repository.name.as_str())
    .execute(pool)
    .await
    .unwrap();
  sqlx::query(
    "INSERT INTO repository_versions \
       (repository_id, version, vcs_integration_id, repository_locator, selection_policy, published_at) \
     VALUES ($1, 1, $2, $3, $4, to_timestamp(0))",
  )
  .bind(repository.id.as_uuid())
  .bind(repository.definition.vcs_integration_id.as_uuid())
  .bind(repository.definition.repository_locator.as_str())
  .bind(Json(&repository.definition.selection))
  .execute(pool)
  .await
  .unwrap();
}

async fn seed_plan_rows(pool: &sqlx::PgPool, project_id: ProjectId, repository_id: RepositoryId) {
  let mut transaction = pool.begin().await.unwrap();
  sqlx::query(
    "INSERT INTO factory_configurations (id, project_id, current_version, created_at, updated_at) \
     VALUES ($1, $2, 1, to_timestamp(1), to_timestamp(1))",
  )
  .bind(configuration_id(101).as_uuid())
  .bind(project_id.as_uuid())
  .execute(&mut *transaction)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO factory_configuration_versions \
       (factory_configuration_id, version, definition_digest, definition, enabled, published_at) \
     VALUES ($1, 1, $2, '{}', true, to_timestamp(1))",
  )
  .bind(configuration_id(101).as_uuid())
  .bind(digest(20).as_bytes().as_slice())
  .execute(&mut *transaction)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO factory_work_envelopes \
       (id, project_id, factory_configuration_id, factory_configuration_version, source_kind, \
        security_scope_digest, external_identity, repository_id, repository_version, exact_revision, \
        intent_digest, envelope_digest, envelope, admitted_at) \
     VALUES ($1, $2, $3, 1, 'manual', $4, 'plan-work', $5, 1, 'revision', $6, $7, '{}', to_timestamp(2))",
  )
  .bind(work_id(601).as_uuid())
  .bind(project_id.as_uuid())
  .bind(configuration_id(101).as_uuid())
  .bind(digest(21).as_bytes().as_slice())
  .bind(repository_id.as_uuid())
  .bind(digest(22).as_bytes().as_slice())
  .bind(digest(23).as_bytes().as_slice())
  .execute(&mut *transaction)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO factory_runs \
       (id, project_id, work_envelope_id, factory_configuration_id, factory_configuration_version, state, \
        version, subject_digest, admitted_at, updated_at) \
     VALUES ($1, $2, $3, $4, 1, 'admitted', 1, $5, to_timestamp(2), to_timestamp(2))",
  )
  .bind(run_id(501).as_uuid())
  .bind(project_id.as_uuid())
  .bind(work_id(601).as_uuid())
  .bind(configuration_id(101).as_uuid())
  .bind(digest(24).as_bytes().as_slice())
  .execute(&mut *transaction)
  .await
  .unwrap();
  transaction.commit().await.unwrap();
}

async fn explain(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  statement: &'static str,
  parameter: uuid::Uuid,
) -> Value {
  let statement = format!("EXPLAIN (FORMAT JSON, COSTS OFF) {statement}");
  sqlx::query_scalar::<_, Json<Value>>(sqlx::AssertSqlSafe(statement))
    .bind(parameter)
    .fetch_one(&mut **transaction)
    .await
    .unwrap()
    .0
}

async fn explain_text(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  statement: &'static str,
  parameter: &'static str,
) -> Value {
  let statement = format!("EXPLAIN (FORMAT JSON, COSTS OFF) {statement}");
  sqlx::query_scalar::<_, Json<Value>>(sqlx::AssertSqlSafe(statement))
    .bind(parameter)
    .fetch_one(&mut **transaction)
    .await
    .unwrap()
    .0
}

fn assert_plan_uses(plan: &Value, index: &str) {
  assert!(
    plan.to_string().contains(index),
    "expected query plan to use {index}: {plan}"
  );
}

fn configuration_ids_from(
  items: &[octacity_server_store::CurrentFactoryConfigurationSummary],
) -> Vec<FactoryConfigurationId> {
  items.iter().map(|item| item.id).collect()
}

fn run_ids(page: &FactoryRunPage) -> Vec<FactoryRunId> {
  page.items.iter().map(|item| item.id).collect()
}

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}

fn digest(value: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([value; 32])
}

fn artifact(value: u8) -> FactoryArtifactReference {
  FactoryArtifactReference::new(ArtifactId::generate(), digest(value), 1).unwrap()
}

fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}

fn idempotency(value: &str) -> IdempotencyKey {
  IdempotencyKey::new(value).unwrap()
}

fn project_id(value: u128) -> ProjectId {
  ProjectId::from_uuid(uuid::Uuid::from_u128(value)).unwrap()
}

fn repository_id(value: u128) -> RepositoryId {
  RepositoryId::from_uuid(uuid::Uuid::from_u128(value)).unwrap()
}

fn configuration_id(value: u128) -> FactoryConfigurationId {
  FactoryConfigurationId::from_uuid(uuid::Uuid::from_u128(value)).unwrap()
}

fn run_id(value: u128) -> FactoryRunId {
  FactoryRunId::from_uuid(uuid::Uuid::from_u128(value)).unwrap()
}

fn work_id(value: u128) -> WorkEnvelopeId {
  WorkEnvelopeId::from_uuid(uuid::Uuid::from_u128(value)).unwrap()
}

mod run_contract {
  use octacity_server_factory::{
    BoundedSummary, BudgetUsage, ContextManifest, ContextManifestEntry, ContextManifestId, ContextSourceKind,
    DecisionSignalProgress, FactoryArtifactReference, FactoryClaim, FactoryClaimFence, FactoryContextReference,
    FactoryLifecycleProgress, FactoryRunState, FactoryRunVersion, FactorySafeText, FactoryStageProgress,
    FactoryStageTarget, FactoryTaskSubject, MacroCall, MacroCallDeclaration, MacroCallId, MacroCallKind, StageAttempt,
    StageAttemptId, StageAttemptNumber, StageHandoff, StageHandoffId, StageHandoffOutcome,
  };
  use octacity_server_store::{
    AuditActorKind, ClaimFactoryOutbox, ClaimFactoryRun, ClaimFactoryRuns, CommitFactoryRunTransition,
    FactoryAuditFact, FactoryBudgetRecord, FactoryLifecycleCheckpoint, FactoryOutboxRecord, FactoryOutboxSettlement,
    FactoryRunClaimRecord, FactoryRunDiagnosticKind, FactoryRunDiagnosticRecord, FactoryRunHistoryAppend,
    FactoryRunStore as _, ListFactoryRunDiagnostics, MutationDisposition, SettleFactoryOutbox, StoreError,
    testing::management_mutation_with_request,
  };

  use super::*;

  #[tokio::test]
  #[ignore = "requires an explicitly configured disposable PostgreSQL service"]
  async fn concurrent_reconcilers_claim_one_run_once_and_replay_exact_claims() {
    let setup = Setup::new().await;
    let exact = claim(&setup.admission.run, "worker.replay", 10, 10, 100);
    assert_eq!(
      setup
        .postgres
        .claim_factory_run(exact.clone())
        .await
        .unwrap()
        .disposition,
      MutationDisposition::Applied
    );
    assert_eq!(
      setup.postgres.claim_factory_run(exact).await.unwrap().disposition,
      MutationDisposition::Replayed
    );
    setup.cleanup().await;

    let setup = Setup::new().await;
    let first = setup.postgres.clone();
    let second = setup.postgres.clone();
    let (first, second) = tokio::join!(
      first.claim_factory_runs(ClaimFactoryRuns::new(key("worker.one"), time(10), time(20), 1).unwrap()),
      second.claim_factory_runs(ClaimFactoryRuns::new(key("worker.two"), time(10), time(20), 1).unwrap()),
    );
    assert_eq!(first.unwrap().len() + second.unwrap().len(), 1);
    let snapshot = setup
      .postgres
      .factory_run_snapshot(setup.admission.run.id())
      .await
      .unwrap();
    assert_eq!(snapshot.claims.len(), 1);
    setup.cleanup().await;
  }

  #[tokio::test]
  #[ignore = "requires an explicitly configured disposable PostgreSQL service"]
  async fn expired_claim_takeover_fences_the_previous_writer() {
    let setup = Setup::new().await;
    let first = setup
      .postgres
      .claim_factory_runs(ClaimFactoryRuns::new(key("worker.one"), time(10), time(20), 1).unwrap())
      .await
      .unwrap()
      .pop()
      .unwrap();
    let stale = transition(
      &setup.admission,
      &first.record,
      19,
      "factory.transition",
      "dispatch.stale",
    );
    let replacement = setup
      .postgres
      .claim_factory_runs(ClaimFactoryRuns::new(key("worker.two"), time(20), time(30), 1).unwrap())
      .await
      .unwrap();
    assert_eq!(replacement.len(), 1);
    assert!(matches!(
      setup.postgres.commit_factory_run_transition(stale).await,
      Err(StoreError::Conflict { .. })
    ));
    let snapshot = setup
      .postgres
      .factory_run_snapshot(setup.admission.run.id())
      .await
      .unwrap();
    assert_eq!(snapshot.run.version(), FactoryRunVersion::INITIAL);
    assert_eq!(snapshot.current_claim, Some(replacement[0].record.clone()));
    setup.cleanup().await;
  }

  #[tokio::test]
  #[ignore = "requires an explicitly configured disposable PostgreSQL service"]
  async fn expired_outbox_work_is_retried_and_terminal_settlement_replays() {
    let setup = Setup::new().await;
    let first = setup
      .postgres
      .claim_factory_outbox(ClaimFactoryOutbox::new(key("dispatcher.one"), time(10), time(20), 1).unwrap())
      .await
      .unwrap()
      .pop()
      .unwrap();
    let second = setup
      .postgres
      .claim_factory_outbox(ClaimFactoryOutbox::new(key("dispatcher.two"), time(20), time(30), 1).unwrap())
      .await
      .unwrap()
      .pop()
      .unwrap();
    assert_eq!(second.record.operation_id, first.record.operation_id);
    assert_eq!(second.record.attempt, 1);
    assert!(matches!(
      setup
        .postgres
        .settle_factory_outbox(SettleFactoryOutbox {
          operation_id: first.record.operation_id,
          owner: key("dispatcher.one"),
          fence: first.record.claim.unwrap().fence(),
          observed_at: time(21),
          settlement: FactoryOutboxSettlement::Delivered,
        })
        .await,
      Err(StoreError::Conflict { .. })
    ));
    let settlement = SettleFactoryOutbox {
      operation_id: second.record.operation_id,
      owner: key("dispatcher.two"),
      fence: second.record.claim.unwrap().fence(),
      observed_at: time(21),
      settlement: FactoryOutboxSettlement::Delivered,
    };
    let delivered = setup.postgres.settle_factory_outbox(settlement.clone()).await.unwrap();
    assert_eq!(
      setup.postgres.settle_factory_outbox(settlement).await.unwrap(),
      delivered
    );
    setup.cleanup().await;
  }

  #[tokio::test]
  #[ignore = "requires an explicitly configured disposable PostgreSQL service"]
  async fn transition_commits_all_authority_or_rolls_everything_back() {
    let setup = Setup::new().await;
    let transition_claim = claim(&setup.admission.run, "worker.transition", 30, 10, 100);
    setup
      .postgres
      .claim_factory_run(transition_claim.clone())
      .await
      .unwrap();
    let outcome = setup
      .postgres
      .commit_factory_run_transition(transition(
        &setup.admission,
        &transition_claim.record,
        20,
        "factory.transition",
        "dispatch.accepted",
      ))
      .await
      .unwrap();
    assert_eq!(outcome.version, FactoryRunVersion::new(2).unwrap());
    let committed = setup
      .postgres
      .factory_run_snapshot(setup.admission.run.id())
      .await
      .unwrap();
    assert_eq!(committed.budgets.len(), 2);
    assert_eq!(committed.lifecycle_checkpoints.len(), 2);
    assert_eq!(committed.audit.len(), 3);
    assert_eq!(committed.outbox.len(), 2);

    let setup = Setup::new().await;
    let rollback_claim = claim(&setup.admission.run, "worker.rollback", 40, 10, 100);
    setup.postgres.claim_factory_run(rollback_claim.clone()).await.unwrap();
    sqlx::query(
      "ALTER TABLE factory_outbox_records ADD CONSTRAINT factory_outbox_test_rollback \
       CHECK (kind <> 'rollback.fail')",
    )
    .execute(&setup.database.pool)
    .await
    .unwrap();
    let before = setup
      .postgres
      .factory_run_snapshot(setup.admission.run.id())
      .await
      .unwrap();
    assert!(
      setup
        .postgres
        .commit_factory_run_transition(transition(
          &setup.admission,
          &rollback_claim.record,
          20,
          "factory.rollback",
          "rollback.fail",
        ))
        .await
        .is_err()
    );
    assert_eq!(
      setup
        .postgres
        .factory_run_snapshot(setup.admission.run.id())
        .await
        .unwrap(),
      before
    );
    setup.cleanup().await;
  }

  #[tokio::test]
  #[ignore = "requires an explicitly configured disposable PostgreSQL service"]
  async fn append_only_stage_and_call_diagnostics_round_trip_and_page() {
    let setup = Setup::new().await;
    let transition_claim = claim(&setup.admission.run, "worker.history", 50, 10, 100);
    setup
      .postgres
      .claim_factory_run(transition_claim.clone())
      .await
      .unwrap();
    let ownership = octacity_server_factory::FactoryClaimOwnership::new(
      transition_claim.record.owner.clone(),
      transition_claim.record.claim,
    );
    let stage = StageAttempt::new(
      StageAttemptId::from_uuid(uuid::Uuid::new_v4()).unwrap(),
      &setup.admission.run,
      StageAttemptNumber::INITIAL,
      FactoryStageTarget::Implementation,
      BudgetLimit::new(2, 100, 100, 100, 100).unwrap(),
      digest(80),
      ownership,
    );
    let call_subject = FactoryTaskSubject::Exact(stage.subject().clone());
    let context_artifact_id = ArtifactId::generate();
    let context = ContextManifest::new(
      ContextManifestId::from_uuid(uuid::Uuid::new_v4()).unwrap(),
      call_subject.clone(),
      digest(81),
      vec![
        ContextManifestEntry::new(
          ContextSourceKind::Task,
          FactoryKey::new("task").unwrap(),
          call_subject.clone(),
          FactoryContextReference::Artifact(FactoryArtifactReference::new(context_artifact_id, digest(81), 1).unwrap()),
          digest(81),
          1,
          FactorySafeText::new("required task").unwrap(),
          digest(81),
        )
        .unwrap(),
      ],
    )
    .unwrap();
    let handoff_artifact_id = ArtifactId::generate();
    let handoff = StageHandoff::new(
      octacity_server_factory::StageHandoffDeclaration {
        id: StageHandoffId::from_uuid(uuid::Uuid::new_v4()).unwrap(),
        stage_attempt_id: stage.id(),
        subject: call_subject.clone(),
        outcome: StageHandoffOutcome::Succeeded,
      },
      octacity_server_factory::StageHandoffContent {
        summary: BoundedSummary::new(
          call_subject.clone(),
          FactorySafeText::new("stage result").unwrap(),
          digest(82),
        ),
        decisions: vec![],
        assumptions: vec![],
        unresolved_items: vec![],
        changed_components: vec![],
        validation_observations: vec![],
        prior_findings: vec![],
      },
      octacity_server_factory::StageHandoffReferences {
        artifacts: vec![FactoryArtifactReference::new(handoff_artifact_id, digest(85), 1).unwrap()],
        changeset_id: None,
        evidence_manifest_id: None,
        result_digest: digest(82),
        policy_digest: digest(83),
        provenance_digest: digest(84),
      },
    )
    .unwrap();
    let call = MacroCall::new(
      MacroCallDeclaration::new(
        MacroCallId::from_uuid(uuid::Uuid::new_v4()).unwrap(),
        call_subject,
        MacroCallKind::Implement,
        BudgetLimit::new(2, 100, 100, 100, 100).unwrap(),
        vec![],
        vec![],
        1,
      )
      .unwrap(),
      &stage,
      &context,
      None,
    )
    .unwrap();
    let child_context = ContextManifest::new(
      ContextManifestId::from_uuid(uuid::Uuid::new_v4()).unwrap(),
      call.subject().clone(),
      digest(86),
      vec![
        ContextManifestEntry::new(
          ContextSourceKind::Task,
          FactoryKey::new("task").unwrap(),
          call.subject().clone(),
          FactoryContextReference::Artifact(FactoryArtifactReference::new(context_artifact_id, digest(81), 1).unwrap()),
          digest(81),
          1,
          FactorySafeText::new("required task").unwrap(),
          digest(81),
        )
        .unwrap(),
      ],
    )
    .unwrap();
    let child_call = MacroCall::new(
      MacroCallDeclaration::new(
        MacroCallId::from_uuid(uuid::Uuid::new_v4()).unwrap(),
        call.subject().clone(),
        MacroCallKind::Summarize,
        BudgetLimit::new(2, 100, 100, 100, 100).unwrap(),
        vec![],
        vec![call.id()],
        2,
      )
      .unwrap(),
      &stage,
      &child_context,
      Some(&call),
    )
    .unwrap();
    let mut request = transition(
      &setup.admission,
      &transition_claim.record,
      20,
      "factory.history",
      "history.accepted",
    );
    request.current.stage_attempt_id = Some(stage.id());
    request.current.macro_call_id = Some(child_call.id());
    request.append.stage_attempts.push(stage.clone());
    request.append.stage_handoffs.push(handoff.clone());
    request.append.context_manifests.push(child_context.clone());
    request.append.context_manifests.push(context.clone());
    request.append.macro_calls.push(child_call.clone());
    request.append.macro_calls.push(call.clone());
    setup.postgres.commit_factory_run_transition(request).await.unwrap();

    let snapshot = setup
      .postgres
      .factory_run_snapshot(setup.admission.run.id())
      .await
      .unwrap();
    assert_eq!(snapshot.stage_attempts, vec![stage.clone()]);
    assert_eq!(snapshot.stage_handoffs, vec![handoff.clone()]);
    assert_eq!(snapshot.context_manifests.len(), 2);
    assert!(snapshot.context_manifests.contains(&context));
    assert!(snapshot.context_manifests.contains(&child_context));
    assert_eq!(snapshot.macro_calls.len(), 2);
    assert!(snapshot.macro_calls.contains(&call));
    assert!(snapshot.macro_calls.contains(&child_call));
    let retained_context_artifacts = sqlx::query_scalar::<_, i64>(
      "SELECT COUNT(*) FROM factory_artifact_references \
       WHERE run_id = $1 AND ((artifact_id = $2 AND role = 'call_context') \
         OR (artifact_id = $3 AND role = 'stage_handoff'))",
    )
    .bind(setup.admission.run.id().as_uuid())
    .bind(context_artifact_id.as_uuid())
    .bind(handoff_artifact_id.as_uuid())
    .fetch_one(&setup.database.pool)
    .await
    .unwrap();
    assert_eq!(retained_context_artifacts, 2);
    let page = setup
      .postgres
      .list_factory_run_diagnostics(
        ListFactoryRunDiagnostics::new(
          setup.admission.run.id(),
          FactoryRunDiagnosticKind::StageAttempt,
          None,
          1,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(
      page.items,
      vec![FactoryRunDiagnosticRecord::StageAttempt(Box::new(stage))]
    );
    assert!(page.next.is_none());
    let handoff_page = setup
      .postgres
      .list_factory_run_diagnostics(
        ListFactoryRunDiagnostics::new(
          setup.admission.run.id(),
          FactoryRunDiagnosticKind::StageHandoff,
          None,
          1,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(
      handoff_page.items,
      vec![FactoryRunDiagnosticRecord::StageHandoff(Box::new(handoff))]
    );
    setup.cleanup().await;
  }

  struct Setup {
    database: TestDatabase,
    postgres: PostgresStore,
    admission: AdmitFactoryWork,
  }

  impl Setup {
    async fn new() -> Self {
      let database = TestDatabase::migrated().await;
      let postgres = PostgresStore::new(database.pool.clone());
      let owner_project_id = project_id(uuid::Uuid::new_v4().as_u128());
      let foreign_project_id = project_id(uuid::Uuid::new_v4().as_u128());
      seed_projects(&database.pool, owner_project_id, foreign_project_id).await;
      let fixture = fixture(owner_project_id);
      let configuration_id = configuration_id(uuid::Uuid::new_v4().as_u128());
      let configuration = FactoryConfiguration::publish(
        configuration_id,
        FactoryConfigurationVersion::INITIAL,
        owner_project_id,
        digest(20),
        fixture.draft.clone(),
        &fixture.choices,
      )
      .unwrap();
      postgres
        .create_factory_configuration(management_mutation_with_request(
          CreateFactoryConfiguration {
            intent: FactoryConfigurationMutationIntent::Create {
              id: configuration_id,
              project_id: owner_project_id,
              definition_digest: digest(20),
              draft: fixture.draft,
            },
            configuration,
            idempotency_key: idempotency("factory-run-configuration"),
            published_at: time(1),
          },
          "factory-run-configuration",
        ))
        .await
        .unwrap();
      let repository = repository(owner_project_id, repository_id(uuid::Uuid::new_v4().as_u128()));
      seed_repository(&database.pool, &repository).await;
      let admission = admission(
        uuid::Uuid::new_v4().as_u128(),
        uuid::Uuid::new_v4().as_u128(),
        &format!("work-{}", uuid::Uuid::new_v4()),
        2,
        owner_project_id,
        configuration_id,
        &repository,
      );
      postgres
        .admit_factory_work(management_mutation_with_request(
          admission.clone(),
          "factory-run-admission",
        ))
        .await
        .unwrap();
      Self {
        database,
        postgres,
        admission,
      }
    }

    async fn cleanup(self) {
      self.database.cleanup().await;
    }
  }

  fn claim(run: &FactoryRun, owner: &str, fence: u8, claimed_at: i64, expires_at: i64) -> ClaimFactoryRun {
    let claim = FactoryClaim::new(
      FactoryClaimFence::new(digest(fence)),
      time(claimed_at),
      time(expires_at),
    )
    .unwrap();
    let record = FactoryRunClaimRecord::new(run.id(), key(owner), claim);
    ClaimFactoryRun {
      run_id: run.id(),
      expected_version: run.version(),
      audit: audit(run.id(), "factory.claimed", fence.wrapping_add(1), claimed_at),
      record,
    }
  }

  fn transition(
    admission: &AdmitFactoryWork,
    claim: &FactoryRunClaimRecord,
    committed_at: i64,
    operation: &str,
    outbox_kind: &str,
  ) -> CommitFactoryRunTransition {
    let version = FactoryRunVersion::new(2).unwrap();
    let budget = FactoryBudgetRecord::new(
      admission.run.id(),
      version,
      BudgetUsage {
        attempts: 1,
        elapsed_millis: 2,
        tokens: 3,
        cost_micro_units: 4,
        output_bytes: 5,
      },
      time(committed_at),
    );
    let lifecycle_checkpoint = FactoryLifecycleCheckpoint::new(
      admission.run.id(),
      version,
      FactoryLifecycleProgress::Stage {
        target: FactoryStageTarget::Implementation,
        progress: FactoryStageProgress::AttemptCreated,
      },
      DecisionSignalProgress::Disabled,
      false,
      time(committed_at),
    );
    let current = octacity_server_store::FactoryRunCurrentProjection {
      budget_id: budget.id,
      lifecycle_checkpoint_id: lifecycle_checkpoint.id,
      stage_attempt_id: None,
      macro_call_id: None,
      signal_request_id: None,
      signal_receipt_id: None,
      build_id: None,
      candidate_id: None,
      evidence_id: None,
      evaluation_plan_id: None,
      decision_id: None,
      escalation_id: None,
      delivery_attempt_id: None,
      reporting_attempt_id: None,
    };
    let next_run = FactoryRun::restore(
      admission.run.id(),
      admission.run.configuration().clone(),
      &admission.work,
      admission.run.subject().clone(),
      FactoryRunState::Implementing,
      version,
    )
    .unwrap();
    CommitFactoryRunTransition {
      run_id: admission.run.id(),
      expected_version: FactoryRunVersion::INITIAL,
      claim_id: claim.id,
      owner: claim.owner.clone(),
      fence: claim.claim.fence(),
      committed_at: time(committed_at),
      next_run,
      budget,
      lifecycle_checkpoint,
      append: FactoryRunHistoryAppend::default(),
      current,
      audit: audit(admission.run.id(), operation, 70, committed_at),
      outbox: vec![FactoryOutboxRecord::pending(
        digest(71),
        admission.run.id(),
        key(outbox_kind),
        digest(72),
        time(committed_at),
        time(committed_at),
      )],
    }
  }

  fn audit(run_id: FactoryRunId, operation: &str, value: u8, recorded_at: i64) -> FactoryAuditFact {
    FactoryAuditFact::new(
      run_id,
      AuditActorKind::Worker,
      Some(digest(value)),
      key(operation),
      digest(value.wrapping_add(1)),
      key("accepted"),
      time(recorded_at),
    )
  }
}
