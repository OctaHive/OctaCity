mod support;

use std::collections::BTreeSet;

use octacity_server_domain::{
  ArtifactId, AttemptId, BuildConfigurationId, BuildConfigurationVersion, BuildId, ImmutableRevision, IntegrationId,
  JobId, ProjectId, RepositoryId, RepositoryLocator, RepositoryName, RepositoryVersion, SourceReference, Timestamp,
};
use octacity_server_factory::{
  BudgetLimit, BuildConfigurationRef, DecisionOutcome, DeliveryPolicyDraft, EvaluationPolicyDraft, ExactSubject,
  ExternalWorkIdentity, FactoryArtifactReference, FactoryChoiceKind, FactoryConfiguration,
  FactoryConfigurationChoiceEntries, FactoryConfigurationChoices, FactoryConfigurationDraft, FactoryConfigurationId,
  FactoryConfigurationVersion, FactoryCredentialProfiles, FactoryDigest, FactoryKey, FactoryMetadata,
  FactoryReferenceChoice, FactoryRun, FactoryRunId, FactoryStageDraft, FactoryStageKind, FactoryWipLimits,
  ImmutableReference, ReworkPolicyDraft, RiskClass, WorkArtifacts, WorkClassification, WorkEnvelope, WorkEnvelopeId,
  WorkPriority,
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
    flow: None,
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
      exhausted_outcome: DecisionOutcome::Escalate,
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
  let projection_fixture = fixture(project_id);
  let configuration = FactoryConfiguration::publish(
    configuration_id,
    FactoryConfigurationVersion::INITIAL,
    project_id,
    digest(20),
    projection_fixture.draft,
    &projection_fixture.choices,
  )
  .unwrap();
  let flow = octacity_server_factory::AdmittedFlow::from_stage_projection(&configuration, &run).unwrap();
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
    flow,
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
        version, subject_digest, flow_admission_limits, admitted_at, updated_at) \
     VALUES ($1, $2, $3, $4, 1, 'admitted', 1, $5, '{}'::jsonb, to_timestamp(2), to_timestamp(2))",
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
    BoundedSummary, BudgetUsage, CandidateSubject, ChangeSet, ChangeSetId, ContextManifest, ContextManifestEntry,
    ContextManifestId, ContextSourceKind, DecisionSignalProgress, FactoryArtifactReference, FactoryClaim,
    FactoryClaimFence, FactoryContextReference, FactoryLifecycleProgress, FactoryRunState, FactoryRunVersion,
    FactorySafeText, FactoryStageProgress, FactoryStageTarget, FactoryTaskSubject, FlowInterpreter, MacroCall,
    MacroCallDeclaration, MacroCallId, MacroCallKind, NodeAttemptCompletion, NodeAttemptCompletionInput, StageAttempt,
    StageAttemptId, StageAttemptNumber, StageHandoff, StageHandoffId, StageHandoffOutcome,
  };
  use octacity_server_store::{
    AuditActorKind, ClaimFactoryOutbox, ClaimFactoryRun, ClaimFactoryRuns, CommitFactoryRunTransition,
    FactoryAuditFact, FactoryBudgetRecord, FactoryBuildLink, FactoryBuildLinkInput, FactoryLifecycleCheckpoint,
    FactoryOutboxRecord, FactoryOutboxSettlement, FactoryRunClaimRecord, FactoryRunDiagnosticKind,
    FactoryRunDiagnosticRecord, FactoryRunHistoryAppend, FactoryRunStore as _, ListFactoryRunDiagnostics,
    MutationDisposition, SettleFactoryOutbox, StoreError, testing::management_mutation_with_request,
  };

  use super::*;

  fn pool_policy() -> octacity_server_factory::PhasePoolPolicy {
    octacity_server_factory::PhasePoolPolicy {
      phase: key("development.v1"),
      order: vec![
        octacity_server_factory::PhasePoolOrder::Severity,
        octacity_server_factory::PhasePoolOrder::ProjectPriority,
        octacity_server_factory::PhasePoolOrder::Age,
      ],
      max_wip: 1,
      max_project_wip: 1,
      budget: BudgetLimit::new(10, 1_000, 1_000, 1_000, 1_000).unwrap(),
    }
  }

  fn pool_request(byte: u8) -> octacity_server_store::SelectPhasePool {
    octacity_server_store::SelectPhasePool {
      policy_digest: pool_policy().digest(),
      request_id: digest(byte),
      owner: key(&format!("pool.worker.{byte}")),
      observed_at: time(100),
      expires_at: time(200),
      capabilities: BTreeSet::new(),
      limit: 1,
    }
  }

  #[tokio::test]
  #[ignore = "requires an explicitly configured disposable PostgreSQL service"]
  async fn phase_pool_concurrent_admission_and_capacity_contention_preserve_the_total_order() {
    use octacity_server_factory::FindingSeverity;
    use octacity_server_store::{FactoryPhasePoolStore as _, testing::phase_pool_contract_input};
    let setup = Setup::new().await;
    let owner = setup.admission.run.subject().project_id();
    let repo = repository(owner, setup.admission.run.subject().repository_id());
    let first = admission(
      801,
      901,
      "concurrent-pool-one",
      2,
      owner,
      setup.admission.run.configuration().id(),
      &repo,
    );
    let second = admission(
      802,
      902,
      "concurrent-pool-two",
      2,
      owner,
      setup.admission.run.configuration().id(),
      &repo,
    );
    let (one, two) = tokio::join!(
      setup
        .postgres
        .admit_factory_work(management_mutation_with_request(first.clone(), "pool-admit-one")),
      setup
        .postgres
        .admit_factory_work(management_mutation_with_request(second.clone(), "pool-admit-two")),
    );
    one.unwrap();
    two.unwrap();
    for run in [second.run.id(), first.run.id()] {
      let snapshot = setup.postgres.factory_run_snapshot(run).await.unwrap();
      let input = phase_pool_contract_input(&snapshot, FindingSeverity::Medium);
      setup.postgres.publish_phase_ready(pool_policy(), input).await.unwrap();
    }
    let entries = setup
      .postgres
      .phase_ready_entries(pool_policy().digest(), None, 100)
      .await
      .unwrap();
    assert_eq!(
      entries.iter().map(|entry| entry.input.run_id).collect::<Vec<_>>(),
      [first.run.id(), second.run.id()]
    );
    let (one, two) = tokio::join!(
      setup.postgres.select_phase_ready(pool_request(110)),
      setup.postgres.select_phase_ready(pool_request(111))
    );
    let selected = [one.unwrap(), two.unwrap()].concat();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].entry.input.run_id, first.run.id());
    let restarted = PostgresStore::new(
      sqlx::PgPool::connect_with((*setup.database.pool.connect_options()).clone())
        .await
        .unwrap(),
    );
    let selected_request = if selected[0].request_id == digest(110) {
      pool_request(110)
    } else {
      pool_request(111)
    };
    assert_eq!(restarted.select_phase_ready(selected_request).await.unwrap(), selected);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM factory_phase_pool_selections")
      .fetch_one(&setup.database.pool)
      .await
      .unwrap();
    assert_eq!(count, 1);
    setup.cleanup().await;
  }

  #[tokio::test]
  #[ignore = "requires an explicitly configured disposable PostgreSQL service"]
  async fn phase_pool_discards_stale_version_and_candidate_inputs_before_claiming() {
    use octacity_server_store::{FactoryPhasePoolStore as _, testing::phase_pool_contract_input};
    let setup = Setup::new().await;
    let snapshot = setup
      .postgres
      .factory_run_snapshot(setup.admission.run.id())
      .await
      .unwrap();
    let input = phase_pool_contract_input(&snapshot, octacity_server_factory::FindingSeverity::High);
    let entry = setup
      .postgres
      .publish_phase_ready(pool_policy(), input.clone())
      .await
      .unwrap();
    let mut changed = input.clone();
    changed.project_priority += 1;
    assert!(matches!(
      setup.postgres.publish_phase_ready(pool_policy(), changed).await,
      Err(StoreError::Conflict { .. })
    ));
    let claim = claim(&setup.admission.run, "stale-pool.worker", 20, 10, 100);
    setup.postgres.claim_factory_run(claim.clone()).await.unwrap();
    let change = transition(
      &setup.admission,
      &claim.record,
      20,
      "factory.transition",
      "pool.stale.dispatch",
    );
    setup.postgres.commit_factory_run_transition(change).await.unwrap();
    assert!(setup.postgres.publish_phase_ready(pool_policy(), input).await.is_err());
    assert!(
      setup
        .postgres
        .phase_ready_entries(pool_policy().digest(), None, 100)
        .await
        .unwrap()
        .is_empty()
    );
    assert!(
      setup
        .postgres
        .select_phase_ready(pool_request(120))
        .await
        .unwrap()
        .is_empty()
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM factory_phase_pool_selections WHERE entry_id = $1")
      .bind(entry.id.as_bytes().as_slice())
      .fetch_one(&setup.database.pool)
      .await
      .unwrap();
    assert_eq!(count, 0);
    setup.cleanup().await;
  }

  async fn phase_pool_runs(setup: &Setup) -> Vec<octacity_server_factory::FactoryRunId> {
    let owner = setup.admission.run.subject().project_id();
    let repo = repository(owner, setup.admission.run.subject().repository_id());
    let mut runs = vec![setup.admission.run.id()];
    for index in 1..101_u128 {
      let input = admission(
        800 + index,
        900 + index,
        &format!("progress-{index}"),
        i64::try_from(index + 2).unwrap(),
        owner,
        setup.admission.run.configuration().id(),
        &repo,
      );
      setup
        .postgres
        .admit_factory_work(management_mutation_with_request(
          input.clone(),
          format!("progress-{index}"),
        ))
        .await
        .unwrap();
      runs.push(input.run.id());
    }
    runs
  }

  #[tokio::test]
  #[ignore = "requires an explicitly configured disposable PostgreSQL service"]
  async fn phase_pool_rechecks_priority_after_capability_or_lease_changes() {
    for lease_blocked in [false, true] {
      let setup = Setup::with_capacity(101).await;
      let runs = phase_pool_runs(&setup).await;
      let restarted = PostgresStore::new(setup.database.pool.clone());
      octacity_server_store::testing::verify_factory_phase_pool_scan_invalidation(
        &setup.postgres,
        &restarted,
        &runs,
        lease_blocked,
      )
      .await;
      setup.cleanup().await;
    }
  }

  #[tokio::test]
  #[ignore = "requires an explicitly configured disposable PostgreSQL service"]
  async fn phase_pool_progress_survives_restart_after_a_blocked_window() {
    let setup = Setup::with_capacity(101).await;
    let runs = phase_pool_runs(&setup).await;
    let restarted = PostgresStore::new(setup.database.pool.clone());
    octacity_server_store::testing::verify_factory_phase_pool_progress(&setup.postgres, &restarted, &runs).await;
    setup.cleanup().await;
  }

  #[tokio::test]
  #[ignore = "requires an explicitly configured disposable PostgreSQL service"]
  async fn durable_phase_pool_contract_matches_memory_and_survives_client_restart() {
    use octacity_server_store::{FactoryPhasePoolStore as _, testing::verify_factory_phase_pool_contract};
    let setup = Setup::new().await;
    let owner = setup.admission.run.subject().project_id();
    let repo = repository(owner, setup.admission.run.subject().repository_id());
    let mut runs = vec![setup.admission.run.id()];
    for index in 1..5_u128 {
      let input = admission(
        800 + index,
        900 + index,
        &format!("pool-work-{index}"),
        i64::try_from(index + 2).unwrap(),
        owner,
        setup.admission.run.configuration().id(),
        &repo,
      );
      setup
        .postgres
        .admit_factory_work(management_mutation_with_request(
          input.clone(),
          format!("pool-admission-{index}"),
        ))
        .await
        .unwrap();
      runs.push(input.run.id());
    }
    verify_factory_phase_pool_contract(&setup.postgres, &runs).await;
    let restarted = PostgresStore::new(setup.database.pool.clone());
    let policy = octacity_server_factory::PhasePoolPolicy {
      phase: key("research.v1"),
      order: vec![
        octacity_server_factory::PhasePoolOrder::Severity,
        octacity_server_factory::PhasePoolOrder::ProjectPriority,
        octacity_server_factory::PhasePoolOrder::Age,
      ],
      max_wip: 2,
      max_project_wip: 2,
      budget: BudgetLimit::new(10, 1_000, 10, 1_000, 1_000).unwrap(),
    };
    let request = octacity_server_store::SelectPhasePool {
      policy_digest: policy.digest(),
      request_id: digest(10),
      owner: key("worker.one"),
      observed_at: time(100),
      expires_at: time(200),
      capabilities: BTreeSet::new(),
      limit: 2,
    };
    let replay = restarted.select_phase_ready(request).await.unwrap();
    assert_eq!(
      replay.iter().map(|row| row.entry.input.run_id).collect::<Vec<_>>(),
      [runs[2], runs[4]]
    );
    use octacity_server_store::{
      AdvanceFactoryRetentionWork, ClaimFactoryRetentionWork, FactoryRetentionStore as _, WorkerOwner,
    };
    let owner = WorkerOwner::new("pool.retention").unwrap();
    for observed in 410..460 {
      let claims = setup
        .postgres
        .claim_factory_retention_work(
          ClaimFactoryRetentionWork::new(owner.clone(), time(observed), time(observed + 1), 10).unwrap(),
        )
        .await
        .unwrap();
      if claims.is_empty() {
        break;
      }
      for claim in claims {
        setup
          .postgres
          .advance_factory_retention_work(
            AdvanceFactoryRetentionWork::new(claim.run_id, owner.clone(), time(observed), 4).unwrap(),
          )
          .await
          .unwrap();
      }
    }
    for run in [runs[2], runs[4]] {
      let retained: i64 = sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM factory_phase_pool_entries WHERE run_id = $1) + (SELECT COUNT(*) FROM factory_phase_pool_selections WHERE run_id = $1) + (SELECT COUNT(*) FROM factory_run_claims WHERE run_id = $1)").bind(run.as_uuid()).fetch_one(&setup.database.pool).await.unwrap();
      assert_eq!(
        retained, 0,
        "retention must remove pool rows before their referenced Flow history and claims"
      );
    }
    setup.cleanup().await;
  }

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
    let root_definition = setup
      .admission
      .flow
      .closure()
      .definition(setup.admission.flow.root_run().definition())
      .unwrap();
    let stage_budget = root_definition.node(&key("implement")).unwrap().budget();
    let stage = StageAttempt::new(
      StageAttemptId::from_uuid(uuid::Uuid::new_v4()).unwrap(),
      &setup.admission.run,
      StageAttemptNumber::INITIAL,
      FactoryStageTarget::Implementation,
      stage_budget,
      digest(80),
      ownership.clone(),
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
    let build = setup.linked_build(&stage).await;
    let candidate = ChangeSet::new(
      ChangeSetId::from_uuid(uuid::Uuid::new_v4()).unwrap(),
      &stage,
      CandidateSubject::new(
        setup.admission.run.subject().clone(),
        ImmutableRevision::new("candidate-revision").unwrap(),
        digest(91),
      ),
      ArtifactId::generate(),
      ArtifactId::generate(),
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
    request.current.build_id = Some(build.build_id);
    request.current.candidate_id = Some(candidate.id());
    let node = setup.admission.flow.project_stage(stage.clone()).unwrap();
    let accepted = root_definition
      .node(node.node_key())
      .unwrap()
      .outcome(&key("succeeded"))
      .unwrap();
    let node_completion = NodeAttemptCompletion::new(
      &node,
      root_definition,
      NodeAttemptCompletionInput {
        outcome: key("succeeded"),
        output_schema: accepted.schema().clone(),
        output_digest: digest(92),
        ownership,
        usage: BudgetUsage::default(),
        observed_at: time(20),
      },
    )
    .unwrap();
    request.append.flow.attempts.push(node.clone());
    request.append.flow.completions.push(node_completion.clone());
    request.append.stage_attempts.push(stage.clone());
    request.append.stage_handoffs.push(handoff.clone());
    request.append.context_manifests.push(child_context.clone());
    request.append.context_manifests.push(context.clone());
    request.append.macro_calls.push(child_call.clone());
    request.append.macro_calls.push(call.clone());
    request.append.linked_builds.push(build.clone());
    request.append.candidates.push(candidate.clone());
    octacity_server_factory::validate_flow_runtime_history(
      &setup.admission.flow,
      octacity_server_factory::FlowRuntimeHistory {
        flow_runs: &[setup.admission.flow.root_run().clone()],
        cycles: &[setup.admission.flow.initial_cycle().clone()],
        attempts: &request.append.flow.attempts,
        completions: &request.append.flow.completions,
      },
    )
    .expect("appended Flow history matches the admitted definition");
    setup.postgres.commit_factory_run_transition(request).await.unwrap();

    let restarted = PostgresStore::new(setup.database.pool.clone());
    let snapshot = restarted.factory_run_snapshot(setup.admission.run.id()).await.unwrap();
    let validated = snapshot.admitted_flow.validated().unwrap();
    let directives = FlowInterpreter::new(&validated)
      .advance(
        snapshot.admitted_flow.root_run(),
        snapshot.admitted_flow.initial_cycle(),
        &node,
        &node_completion,
        &snapshot.flow.attempts,
        &snapshot.flow.completions,
      )
      .unwrap();
    assert!(!directives.is_empty(), "restart must resume from durable Flow facts");
    assert_eq!(snapshot.admitted_flow, setup.admission.flow);
    assert_eq!(snapshot.flow.runs, vec![snapshot.admitted_flow.root_run().clone()]);
    assert_eq!(
      snapshot.flow.cycles,
      vec![snapshot.admitted_flow.initial_cycle().clone()]
    );
    assert_eq!(snapshot.flow.attempts, vec![node]);
    assert_eq!(snapshot.flow.completions, vec![node_completion]);
    assert_eq!(snapshot.stage_attempts, vec![stage.clone()]);
    assert_eq!(snapshot.stage_handoffs, vec![handoff.clone()]);
    assert_eq!(snapshot.context_manifests.len(), 2);
    assert!(snapshot.context_manifests.contains(&context));
    assert!(snapshot.context_manifests.contains(&child_context));
    assert_eq!(snapshot.macro_calls.len(), 2);
    assert!(snapshot.macro_calls.contains(&call));
    assert!(snapshot.macro_calls.contains(&child_call));
    assert_eq!(snapshot.linked_builds, vec![build]);
    assert_eq!(snapshot.candidates, vec![candidate]);
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
      Self::with_capacity(20).await
    }

    async fn with_capacity(max_runs: u32) -> Self {
      let database = TestDatabase::migrated().await;
      let postgres = PostgresStore::new(database.pool.clone());
      let owner_project_id = project_id(uuid::Uuid::new_v4().as_u128());
      let foreign_project_id = project_id(uuid::Uuid::new_v4().as_u128());
      seed_projects(&database.pool, owner_project_id, foreign_project_id).await;
      let mut fixture = fixture(owner_project_id);
      fixture.draft.wip_limits = FactoryWipLimits::new(max_runs, 20).unwrap();
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

    async fn linked_build(&self, stage: &StageAttempt) -> FactoryBuildLink {
      let pipeline_id = uuid::Uuid::new_v4();
      let build_configuration_id = BuildConfigurationId::generate();
      let trigger_id = uuid::Uuid::new_v4();
      let occurrence_id = uuid::Uuid::new_v4();
      let build_id = BuildId::generate();
      let attempt_id = AttemptId::generate();
      let job_id = JobId::generate();
      let pool_id = uuid::Uuid::new_v4();
      let subject = self.admission.run.subject();
      let mut transaction = self.database.pool.begin().await.unwrap();

      sqlx::query("INSERT INTO pipelines (id, project_id, name, created_at) VALUES ($1, $2, $3, now())")
        .bind(pipeline_id)
        .bind(subject.project_id().as_uuid())
        .bind(format!("factory-linked-{build_id}"))
        .execute(&mut *transaction)
        .await
        .unwrap();
      sqlx::query(
        "INSERT INTO pipeline_versions (pipeline_id, version, dag_snapshot, published_at) \
         VALUES ($1, 1, '{}', now())",
      )
      .bind(pipeline_id)
      .execute(&mut *transaction)
      .await
      .unwrap();
      sqlx::query("INSERT INTO build_configurations (id, project_id, name, created_at) VALUES ($1, $2, $3, now())")
        .bind(build_configuration_id.as_uuid())
        .bind(subject.project_id().as_uuid())
        .bind(format!("factory-linked-{build_id}"))
        .execute(&mut *transaction)
        .await
        .unwrap();
      sqlx::query(
        "INSERT INTO build_configuration_versions \
           (build_configuration_id, version, enabled, repository_id, repository_version, pipeline_id, \
            pipeline_version, configuration_snapshot, job_concurrency_limit, allowed_pool_ids, retry_max_attempts, \
            retries_infrastructure, published_at) \
         VALUES ($1, 1, true, $2, $3, $4, 1, '{}', 1, ARRAY[$5::uuid], 1, false, now())",
      )
      .bind(build_configuration_id.as_uuid())
      .bind(subject.repository_id().as_uuid())
      .bind(i64::try_from(self.admission.repository_version.get()).unwrap())
      .bind(pipeline_id)
      .bind(pool_id)
      .execute(&mut *transaction)
      .await
      .unwrap();
      sqlx::query(
        "INSERT INTO triggers \
           (id, version, build_configuration_id, build_configuration_version, kind, enabled, definition, created_at) \
         VALUES ($1, 1, $2, 1, 'manual', true, '{}', now())",
      )
      .bind(trigger_id)
      .bind(build_configuration_id.as_uuid())
      .execute(&mut *transaction)
      .await
      .unwrap();
      sqlx::query(
        "INSERT INTO trigger_occurrences \
           (id, trigger_id, trigger_version, build_configuration_id, build_configuration_version, kind, \
            deduplication_identity, cause, causality, provider_metadata, source_time, state, request_digest, \
            created_at, updated_at) \
         VALUES ($1, $2, 1, $3, 1, 'manual', $4, '{\"kind\":\"manual\"}', \
                 jsonb_build_object('root_occurrence_id', $1::text, 'parent_occurrence_id', NULL, 'depth', 0), '{}', \
                 now(), 'accepted', decode(repeat('02', 32), 'hex'), now(), now())",
      )
      .bind(occurrence_id)
      .bind(trigger_id)
      .bind(build_configuration_id.as_uuid())
      .bind(format!("factory-linked-{build_id}"))
      .execute(&mut *transaction)
      .await
      .unwrap();
      sqlx::query(
        "INSERT INTO builds \
           (id, project_id, build_configuration_id, build_configuration_version, pipeline_id, pipeline_version, \
            repository_id, repository_version, trigger_occurrence_id, immutable_revision, input_snapshot, \
            effective_policy_snapshot, project_job_concurrency_limit, priority, state, version, created_at, updated_at, \
            metadata_retention_until, log_retention_until, artifact_retention_until, report_retention_until) \
         VALUES ($1, $2, $3, 1, $4, 1, $5, $6, $7, $8, '{}', '{}', 1, 0, 'running', 1, now(), now(), \
                 now() + interval '100 years', now() + interval '100 years', now() + interval '100 years', \
                 now() + interval '100 years')",
      )
      .bind(build_id.as_uuid())
      .bind(subject.project_id().as_uuid())
      .bind(build_configuration_id.as_uuid())
      .bind(pipeline_id)
      .bind(subject.repository_id().as_uuid())
      .bind(i64::try_from(self.admission.repository_version.get()).unwrap())
      .bind(occurrence_id)
      .bind(subject.base_revision().as_str())
      .execute(&mut *transaction)
      .await
      .unwrap();
      sqlx::query(
        "INSERT INTO attempts (id, build_id, attempt_number, state, version, created_at, updated_at) \
         VALUES ($1, $2, 1, 'running', 1, now(), now())",
      )
      .bind(attempt_id.as_uuid())
      .bind(build_id.as_uuid())
      .execute(&mut *transaction)
      .await
      .unwrap();
      sqlx::query(
        "INSERT INTO jobs \
           (id, attempt_id, pipeline_node_id, state, allowed_pool_ids, requirements, job_spec_template, \
            dependency_policy, signed_job_spec, version, created_at, updated_at) \
         VALUES ($1, $2, 'factory-linked', 'ready', ARRAY[$3]::uuid[], '{}', '{}', \
                 '\"all_succeeded\"', '{}', 1, now(), now())",
      )
      .bind(job_id.as_uuid())
      .bind(attempt_id.as_uuid())
      .bind(pool_id)
      .execute(&mut *transaction)
      .await
      .unwrap();
      transaction.commit().await.unwrap();

      FactoryBuildLink::new(
        stage,
        FactoryBuildLinkInput {
          build_id,
          attempt_id,
          job_ids: vec![job_id],
          factory_configuration: self.admission.run.configuration().clone(),
          target: FactoryStageTarget::Implementation,
          build_configuration: BuildConfigurationRef::new(
            build_configuration_id,
            BuildConfigurationVersion::INITIAL,
            subject.project_id(),
            digest(87),
          ),
          task_envelope_digest: digest(88),
          exact_revision: subject.base_revision().clone(),
          parent: None,
          effective_policy_digest: digest(89),
          input_digest: digest(90),
        },
      )
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

use octacity_server_factory as factory;
#[path = "../../../core/octacity-server-factory/src/triage/tests/journey.rs"]
mod triage_journey;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn configured_root_and_typed_eligibility_resolution_round_trip_after_restart() {
  use octacity_server_factory::*;
  use octacity_server_store::*;
  let database = TestDatabase::migrated().await;
  let postgres = PostgresStore::new(database.pool.clone());
  let project = ProjectId::generate();
  seed_projects(&database.pool, project, ProjectId::generate()).await;
  let mut fixture = fixture(project);
  let journey = triage_journey::journey(
    false,
    false,
    BudgetLimit::new(10, 10_000, 1_000, 10_000, 10_000).unwrap(),
  );
  fixture.draft.flow = Some(journey.clone());
  let config_id = FactoryConfigurationId::generate();
  let config = FactoryConfiguration::publish(
    config_id,
    FactoryConfigurationVersion::INITIAL,
    project,
    digest(20),
    fixture.draft.clone(),
    &fixture.choices,
  )
  .unwrap();
  postgres
    .create_factory_configuration(management_mutation_with_request(
      CreateFactoryConfiguration {
        configuration: config.clone(),
        idempotency_key: idempotency("triage.configuration"),
        published_at: time(1),
        intent: FactoryConfigurationMutationIntent::Create {
          id: config_id,
          project_id: project,
          definition_digest: digest(20),
          draft: fixture.draft,
        },
      },
      "triage.configuration",
    ))
    .await
    .unwrap();
  let repo = repository(project, RepositoryId::generate());
  seed_repository(&database.pool, &repo).await;
  let mut admission = admission(
    uuid::Uuid::new_v4().as_u128(),
    uuid::Uuid::new_v4().as_u128(),
    "configured-triage",
    2,
    project,
    config_id,
    &repo,
  );
  let forged = admission.clone();
  assert!(
    postgres
      .admit_factory_work(management_mutation_with_request(forged, "forged-stage-projection"))
      .await
      .is_err()
  );
  admission.flow = AdmittedFlow::from_configuration(&config, &admission.run).unwrap();
  let published = postgres
    .admit_factory_work(management_mutation_with_request(admission.clone(), "triage.admission"))
    .await
    .unwrap();
  let replayed = postgres
    .admit_factory_work(management_mutation_with_request(admission.clone(), "triage.replay"))
    .await
    .unwrap();
  assert_eq!(published.admission, replayed.admission);
  let claim = postgres
    .claim_factory_runs(ClaimFactoryRuns::new(key("triage.worker"), time(3), time(100), 1).unwrap())
    .await
    .unwrap()
    .remove(0)
    .record;
  let run_id = admission.run.id();
  let snapshot = postgres.factory_run_snapshot(run_id).await.unwrap();
  assert_eq!(snapshot.admitted_flow.closure(), &journey.closure);
  let input = EligibilityInput::new(&snapshot.work, journey.triage.project_goals.clone(), vec![]).unwrap();
  let ownership = FactoryClaimOwnership::new(claim.owner.clone(), claim.claim);
  let make = |flow: &FlowRun,
              cycle: &WorkflowCycle,
              node: &str,
              number: u64,
              input: FactoryDigest,
              execution: NodeExecutionIdentity| {
    let definition = journey.closure.definition(flow.definition()).unwrap();
    let declaration = definition.node(&key(node)).unwrap();
    NodeAttempt::new(
      flow,
      cycle,
      definition,
      NodeAttemptInput {
        id: NodeAttemptId::generate(),
        node_key: key(node),
        node_kind: declaration.kind(),
        number: NodeAttemptNumber::new(number).unwrap(),
        input_digest: input,
        budget: declaration.budget(),
        deadline: time(100),
        execution,
        ownership: ownership.clone(),
      },
    )
    .unwrap()
  };
  let root = snapshot.admitted_flow.root_run().clone();
  let root_cycle = snapshot.admitted_flow.initial_cycle().clone();
  let root_call = make(
    &root,
    &root_cycle,
    "triage",
    1,
    input.digest().unwrap(),
    NodeExecutionIdentity::BuiltIn,
  );
  let triage = FlowRun::nested(
    FlowRunId::generate(),
    run_id,
    journey.triage.definition,
    FlowRunParent::new(root.id(), root_call.id()),
  );
  let triage_cycle = WorkflowCycle::initial(&triage).unwrap();
  let eligibility_call = make(
    &triage,
    &triage_cycle,
    "eligibility",
    1,
    input.digest().unwrap(),
    NodeExecutionIdentity::BuiltIn,
  );
  let eligibility_definition = journey
    .closure
    .definition(triage.definition())
    .unwrap()
    .node(&key("eligibility"))
    .unwrap()
    .subflow_definition()
    .unwrap();
  let eligibility = FlowRun::nested(
    FlowRunId::generate(),
    run_id,
    eligibility_definition,
    FlowRunParent::new(triage.id(), eligibility_call.id()),
  );
  let eligibility_cycle = WorkflowCycle::initial(&eligibility).unwrap();
  let append = FactoryRunHistoryAppend {
    flow: FactoryFlowHistory {
      runs: vec![triage.clone(), eligibility.clone()],
      cycles: vec![triage_cycle.clone(), eligibility_cycle.clone()],
      attempts: vec![root_call.clone(), eligibility_call.clone()],
      triage: vec![TriageJournalRecord::Prepared(Box::new(input.clone()))],
      ..Default::default()
    },
    ..Default::default()
  };
  pg_triage_transition(
    &postgres,
    &snapshot,
    &claim,
    append,
    FactoryRunState::Admitted,
    FactoryLifecycleProgress::Admitted,
    BudgetUsage {
      attempts: 2,
      ..Default::default()
    },
  )
  .await
  .unwrap();
  let snapshot = postgres.factory_run_snapshot(run_id).await.unwrap();
  let profile = &journey.triage.eligibility;
  let leaf = make(
    &eligibility,
    &eligibility_cycle,
    "observe",
    1,
    input.digest().unwrap(),
    NodeExecutionIdentity::External(profile.producer.clone()),
  );
  let result = EligibilityResult::new(
    &input,
    TriageProvenance {
      input_digest: input.digest().unwrap(),
      definition: eligibility.definition(),
      node_attempt_id: leaf.id(),
      producer: profile.producer.clone(),
      model_or_tool: profile.model_or_tool.clone(),
      task_digest: profile.task_digest,
      result: artifact(93),
      observed_at: time(10),
    },
    TriageObservation::new(ProjectFit::OutOfScope, artifact(94)),
    vec![],
  )
  .unwrap();
  // The execution port may already have retained the same result bytes.
  sqlx::query("INSERT INTO factory_artifact_references (run_id, artifact_id, role, expected_sha256, created_at) VALUES ($1,$2,'call_context',$3,to_timestamp(0.009))")
    .bind(run_id.as_uuid()).bind(result.provenance().result.artifact_id().as_uuid())
    .bind(result.provenance().result.content_digest().as_bytes().as_slice())
    .execute(&database.pool).await.unwrap();
  let evidence = vec![
    AcceptedTriageEvidence::new(
      input.subject().clone(),
      input.digest().unwrap(),
      TriageEvidenceFact::ProjectFit(ProjectFit::OutOfScope),
      result.project_fit().evidence().clone(),
      DeterministicGateOutcome::Passed,
    )
    .unwrap(),
  ];
  let usage = BudgetUsage {
    attempts: 3,
    tokens: 1,
    ..Default::default()
  };
  let policy = TriagePolicy::for_flow(
    config.reference().clone(),
    &snapshot.admitted_flow.validated().unwrap(),
    journey.triage.definition,
    journey.triage.policy.clone(),
  )
  .unwrap();
  let decision = policy.eligibility(&input, &result, &evidence, usage).unwrap();
  assert_eq!(decision.reason, TriageReason::OutOfScope);
  let disposition = TriageDisposition::Eligibility(decision.clone());
  let disposition_digest = disposition.digest().unwrap();
  let gate = make(
    &triage,
    &triage_cycle,
    "eligibility_policy",
    2,
    result.digest().unwrap(),
    NodeExecutionIdentity::BuiltIn,
  );
  let complete = |flow: &FlowRun,
                  node: &NodeAttempt,
                  outcome: &str,
                  schema: TriageSchema,
                  digest: FactoryDigest,
                  usage: BudgetUsage| {
    NodeAttemptCompletion::new(
      node,
      journey.closure.definition(flow.definition()).unwrap(),
      NodeAttemptCompletionInput {
        outcome: key(outcome),
        output_schema: schema.reference().unwrap(),
        output_digest: digest,
        ownership: ownership.clone(),
        usage,
        observed_at: time(10),
      },
    )
    .unwrap()
  };
  let receipt = TriageJournalRecord::Eligibility(Box::new(EligibilityReceipt {
    input,
    result: result.clone(),
    evidence,
    usage,
    decision,
  }));
  let append = FactoryRunHistoryAppend {
    flow: FactoryFlowHistory {
      attempts: vec![leaf.clone(), gate.clone()],
      completions: vec![
        complete(
          &eligibility,
          &leaf,
          "observed",
          TriageSchema::EligibilityResult,
          result.digest().unwrap(),
          BudgetUsage {
            attempts: 1,
            tokens: 1,
            ..Default::default()
          },
        ),
        complete(
          &triage,
          &eligibility_call,
          "observed",
          TriageSchema::EligibilityResult,
          result.digest().unwrap(),
          BudgetUsage::default(),
        ),
        complete(
          &triage,
          &gate,
          "rejection",
          TriageSchema::Decision,
          disposition_digest,
          BudgetUsage::default(),
        ),
        complete(
          &root,
          &root_call,
          "rejection",
          TriageSchema::Decision,
          disposition_digest,
          BudgetUsage::default(),
        ),
      ],
      triage: vec![receipt.clone()],
      ..Default::default()
    },
    ..Default::default()
  };
  pg_triage_transition(
    &postgres,
    &snapshot,
    &claim,
    append,
    FactoryRunState::Rejected,
    FactoryLifecycleProgress::Rejected(ReportingProgress::Disabled),
    BudgetUsage {
      attempts: 4,
      tokens: 1,
      ..Default::default()
    },
  )
  .await
  .unwrap();
  let restarted = PostgresStore::new(database.pool.clone());
  let restored = restarted.factory_run_snapshot(run_id).await.unwrap();
  assert_eq!(restored.flow.triage.last(), Some(&receipt));
  assert_eq!(restored.run.state(), FactoryRunState::Rejected);
  assert!(restored.stage_attempts.is_empty());
  assert!(
    restored
      .flow
      .runs
      .iter()
      .all(|flow| journey.closure.definition(flow.definition()).is_some())
  );
  assert!(restored.flow.runs.iter().all(|flow| {
    flow.definition()
      != journey
        .closure
        .definition(triage.definition())
        .unwrap()
        .node(&key("classification"))
        .unwrap()
        .subflow_definition()
        .unwrap()
  }));
  for (artifact, expected) in [
    (&result.provenance().result, 9_i64),
    (result.project_fit().evidence(), 10_i64),
  ] {
    let first_retained: i64 = sqlx::query_scalar("SELECT (extract(epoch FROM created_at) * 1000)::bigint FROM factory_artifact_references WHERE run_id=$1 AND artifact_id=$2 AND role='call_context'")
      .bind(run_id.as_uuid()).bind(artifact.artifact_id().as_uuid()).fetch_one(&database.pool).await.unwrap();
    assert_eq!(first_retained, expected);
  }
  let references: i64 =
    sqlx::query_scalar("SELECT count(*) FROM factory_artifact_references WHERE run_id=$1 AND role='call_context'")
      .bind(run_id.as_uuid())
      .fetch_one(&database.pool)
      .await
      .unwrap();
  assert_eq!(references, 3);
  let owner = WorkerOwner::new("triage.retention").unwrap();
  let mut cleaned = false;
  for at in 20..120 {
    let claims = restarted
      .claim_factory_retention_work(ClaimFactoryRetentionWork::new(owner.clone(), time(at), time(at + 10), 1).unwrap())
      .await
      .unwrap();
    assert_eq!(claims.len(), 1);
    let progress = restarted
      .advance_factory_retention_work(AdvanceFactoryRetentionWork::new(run_id, owner.clone(), time(at), 2).unwrap())
      .await
      .unwrap();
    if progress.completed {
      cleaned = true;
      break;
    }
  }
  assert!(
    cleaned,
    "typed journal and nested Flow metadata must be removable in bounded pages"
  );
  let retained: i64 = sqlx::query_scalar("SELECT count(*) FROM factory_triage_records WHERE run_id=$1")
    .bind(run_id.as_uuid())
    .fetch_one(&database.pool)
    .await
    .unwrap();
  assert_eq!(retained, 0);
  database.cleanup().await;
}

async fn pg_triage_transition(
  postgres: &PostgresStore,
  snapshot: &octacity_server_store::FactoryRunSnapshot,
  claim: &octacity_server_store::FactoryRunClaimRecord,
  append: octacity_server_store::FactoryRunHistoryAppend,
  state: octacity_server_factory::FactoryRunState,
  progress: octacity_server_factory::FactoryLifecycleProgress,
  usage: octacity_server_factory::BudgetUsage,
) -> Result<(), octacity_server_store::StoreError> {
  use octacity_server_factory::*;
  use octacity_server_store::*;
  let version = FactoryRunVersion::new(snapshot.run.version().get() + 1).unwrap();
  let at = time(10);
  let budget = FactoryBudgetRecord::new(snapshot.run.id(), version, usage, at);
  let lifecycle = FactoryLifecycleCheckpoint::new(
    snapshot.run.id(),
    version,
    progress,
    DecisionSignalProgress::Disabled,
    false,
    at,
  );
  let mut current = snapshot.current.clone();
  current.budget_id = budget.id;
  current.lifecycle_checkpoint_id = lifecycle.id;
  let run = FactoryRun::restore(
    snapshot.run.id(),
    snapshot.run.configuration().clone(),
    &snapshot.work,
    snapshot.run.subject().clone(),
    state,
    version,
  )
  .unwrap();
  let audit = FactoryAuditFact::new(
    run.id(),
    AuditActorKind::Worker,
    None,
    key("factory.triage.advance"),
    digest(30),
    key(state.as_str()),
    at,
  );
  postgres
    .commit_factory_run_transition(CommitFactoryRunTransition {
      run_id: run.id(),
      expected_version: snapshot.run.version(),
      claim_id: claim.id,
      owner: claim.owner.clone(),
      fence: claim.claim.fence(),
      committed_at: at,
      next_run: run,
      budget,
      lifecycle_checkpoint: lifecycle,
      append,
      audit,
      outbox: vec![],
      current,
    })
    .await?;
  Ok(())
}
