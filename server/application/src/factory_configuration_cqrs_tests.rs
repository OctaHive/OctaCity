use std::{
  future::Future,
  sync::Arc,
  task::{Context, Poll, Waker},
};

use async_trait::async_trait;
use octacity_server_domain::{BuildConfigurationId, BuildConfigurationVersion, ProjectId, Timestamp};
use octacity_server_factory::{
  BudgetLimit, BuildConfigurationRef, FactoryChoiceKind, FactoryConfigurationChoiceEntries,
  FactoryConfigurationChoices, FactoryConfigurationDraft, FactoryConfigurationId, FactoryConfigurationVersion,
  FactoryDigest, FactoryKey, FactoryReferenceChoice, FactoryStageKind, FactoryWipLimits, ImmutableReference,
};
use octacity_server_store::{
  IdempotencyKey,
  testing::{InMemoryFactoryConfigurationStore, ManagementAuditProbe, MutationEvidenceProbe},
};
use uuid::Uuid;

use crate::{
  ApplicationFailure, AuthorizedCommandHandler, AuthorizedQueryHandler, CreateFactoryConfigurationCommand,
  FactoryConfigurationHandlers, GetCurrentFactoryConfigurationQuery, GetFactoryConfigurationQuery, ManagementAction,
  ManagementAuthorizationDenial, ManagementAuthorizationGrant, ManagementAuthorizationPolicy, ManagementCommandUseCase,
  ManagementHandlerError, ManagementQueryUseCase, ManagementRequestContext, ManagementRequestId, ManagementResource,
  ManagementVisibility, MutationDisposition, ReplaceFactoryConfigurationCommand, TrustedNetworkManagementPolicy,
  ValidateFactoryConfigurationQuery,
};

struct Fixture {
  choices: FactoryConfigurationChoices,
  draft: FactoryConfigurationDraft,
}

#[path = "factory_manual_authoring_tests.rs"]
mod manual_authoring;

#[test]
fn configuration_hard_budget_rejection_retains_requested_value_maximum_and_owner() {
  run_ready(async {
    let project = ProjectId::generate();
    let mut fixture = fixture(project);
    fixture.draft.hard_budget = BudgetLimit::new(10, 10_000, 999, 10_000, 10_000).unwrap();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store.enable_project(project, fixture.choices).unwrap();
    let before = store.mutation_evidence_counts().await;
    let error = query(
      &FactoryConfigurationHandlers::new(store.clone()),
      ValidateFactoryConfigurationQuery {
        id: FactoryConfigurationId::generate(),
        project_id: project,
        definition_digest: digest(20),
        draft: fixture.draft,
      },
    )
    .await
    .unwrap_err();
    let crate::ApplicationError::FactoryValidation(report) = error else {
      panic!("budget failure must name its owner: {error:?}")
    };
    let report = serde_json::to_value(report).unwrap();
    assert_eq!(report["errors"][0]["code"], "budget_exceeds_ceiling");
    assert_eq!(report["errors"][0]["field"], "/flow/limits/budget");
    assert_eq!(report["errors"][0]["ceiling"], "configuration");
    assert_eq!(report["errors"][0]["resource"], "tokens");
    assert_eq!(report["errors"][0]["requested"], 1000);
    assert_eq!(report["errors"][0]["maximum"], 999);
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

#[test]
fn configuration_wip_rejection_names_the_global_node_ceiling_and_preserves_the_request() {
  run_ready(async {
    let project = ProjectId::generate();
    let mut fixture = fixture(project);
    fixture.draft.wip_limits = FactoryWipLimits::new(2, 3).unwrap();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store.enable_project(project, fixture.choices).unwrap();
    let before = store.mutation_evidence_counts().await;
    let error = query(
      &FactoryConfigurationHandlers::new(store.clone()),
      ValidateFactoryConfigurationQuery {
        id: FactoryConfigurationId::generate(),
        project_id: project,
        definition_digest: digest(20),
        draft: fixture.draft,
      },
    )
    .await
    .unwrap_err();
    let crate::ApplicationError::FactoryValidation(report) = error else {
      panic!("capacity failure must name its owner: {error:?}")
    };
    let report = serde_json::to_value(report).unwrap();
    assert_eq!(report["errors"][0]["code"], "concurrency_exceeds_ceiling");
    assert_eq!(report["errors"][0]["field"], "/flow/limits/max_wip");
    assert_eq!(report["errors"][0]["ceiling"], "configuration_wip");
    assert_eq!(report["errors"][0]["resource"], "active_nodes");
    assert_eq!(report["errors"][0]["requested"], 4);
    assert_eq!(report["errors"][0]["maximum"], 3);
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

#[test]
fn configuration_constraints_keep_global_and_per_run_concurrency_separate_without_publication() {
  run_ready(async {
    let project = ProjectId::generate();
    let mut fixture = fixture(project);
    fixture.draft.wip_limits = FactoryWipLimits::new(2, 8).unwrap();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store.enable_project(project, fixture.choices).unwrap();
    let before = store.mutation_evidence_counts().await;
    let validation = query(
      &FactoryConfigurationHandlers::new(store.clone()),
      ValidateFactoryConfigurationQuery {
        id: FactoryConfigurationId::generate(),
        project_id: project,
        definition_digest: digest(20),
        draft: fixture.draft,
      },
    )
    .await
    .unwrap();
    let diagnostics = serde_json::to_value(validation.diagnostics).unwrap();
    let concurrency = diagnostics["constraints"]["concurrency"].as_array().unwrap();
    assert!(concurrency.iter().any(|limit| limit["scope"] == "configuration_runs"
      && limit["requested"] == 2
      && limit["effective"] == 2
      && limit["unresolved"].is_null()));
    assert!(
      concurrency
        .iter()
        .any(|limit| limit["scope"] == "configuration_nodes" && limit["effective"] == 8)
    );
    assert!(
      concurrency
        .iter()
        .any(|limit| limit["scope"] == "run_nodes" && limit["effective"] == 4)
    );
    assert!(
      diagnostics["constraints"]["run_budget"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| source["source"] == "configuration")
    );
    assert_eq!(store.mutation_evidence_counts().await, before);
  });
}

#[test]
fn data_schema_normalization_returns_the_exact_content_identity_without_publishing_a_version() {
  run_ready(async {
    let project_id = ProjectId::generate();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project_id).unwrap();
    store.enable_project(project_id, fixture(project_id).choices).unwrap();
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let handlers = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(crate::FactoryFlowDefinitionHandlers::new(store.clone(), policy)),
    );
    let schema = handlers
      .handle_query(
        &context(),
        crate::ValidateFactoryFlowDataSchemaQuery {
          project_id,
          identity: key("operator.boolean"),
          version: key("v1"),
          shape: octacity_server_factory::FlowValueSchema::Boolean,
        },
      )
      .await
      .unwrap();
    assert_eq!(schema.reference().identity().as_str(), "operator.boolean");
    assert_eq!(schema.reference().version().as_str(), "v1");
    assert_eq!(
      schema.reference().digest().to_string(),
      "843fd3f60f5da95afe13dac9521ccdc83dbda0c664d61917c1b2dd321b350fc3"
    );
    assert_eq!(
      serde_json::to_value(&schema).unwrap()["shape"],
      serde_json::json!({"type":"boolean"})
    );
    assert!(store.management_audit_facts().await.is_empty());
  });
}

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).expect("fixture key is valid")
}

fn digest(value: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([value; 32])
}

fn budget(attempts: u32) -> BudgetLimit {
  BudgetLimit::new(attempts, 10_000, 1_000, 10_000, 10_000).expect("fixture budget is valid")
}

fn exact(identity: &str, version: &str, value: u8) -> ImmutableReference {
  ImmutableReference::new(key(identity), key(version), digest(value))
}

fn choice(kind: FactoryChoiceKind, alias: &str, identity: &str, value: u8) -> FactoryReferenceChoice {
  FactoryReferenceChoice {
    kind,
    alias: key(alias),
    reference: exact(identity, "v1", value),
  }
}

fn fixture(build_project_id: ProjectId) -> Fixture {
  let choices = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
    references: vec![
      choice(
        FactoryChoiceKind::NodeCapability,
        "contract-capability",
        "contract.capability",
        71,
      ),
      choice(FactoryChoiceKind::AdmissionPolicy, "admission", "manual.medium", 1),
      choice(FactoryChoiceKind::PermissionCeiling, "permissions", "restricted", 2),
      choice(FactoryChoiceKind::CriterionPack, "criteria", "quality", 3),
      choice(FactoryChoiceKind::Evaluator, "evaluator", "review", 4),
      choice(FactoryChoiceKind::DeliveryAdapter, "delivery-adapter", "github", 5),
      choice(FactoryChoiceKind::DeliveryPolicy, "delivery-policy", "human-review", 6),
    ],
    build_configurations: vec![(
      key("build"),
      BuildConfigurationRef::new(
        BuildConfigurationId::generate(),
        BuildConfigurationVersion::INITIAL,
        build_project_id,
        digest(7),
      ),
    )],
  })
  .expect("fixture choices are valid");
  let stage = |name, kind| octacity_server_factory::testing::FactoryStageDraft {
    key: key(name),
    kind,
    build_configuration: key("build"),
    budget: budget(2),
  };
  let draft = FactoryConfigurationDraft {
    flow: None,
    admission_policy: key("admission"),
    wip_limits: FactoryWipLimits::new(2, 4).unwrap(),
    hard_budget: budget(10),
    permission_ceiling: key("permissions"),
    enabled: true,
  };
  let draft = octacity_server_factory::testing::with_contract_flow(
    draft,
    vec![
      stage("implement", FactoryStageKind::Implementation),
      stage("validate", FactoryStageKind::Validation),
      stage("evaluate", FactoryStageKind::Evaluation),
    ],
    &choices,
  );
  Fixture { choices, draft }
}

fn context() -> ManagementRequestContext {
  ManagementRequestContext::trusted_network(
    ManagementRequestId::new(Uuid::from_u128(0x5d5a_45e1_181b_43db_9823_8bbf_e060_e20c)).unwrap(),
  )
}

fn idempotency(value: &str) -> IdempotencyKey {
  IdempotencyKey::new(value).unwrap()
}

fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}

async fn command<C>(
  handlers: &FactoryConfigurationHandlers<InMemoryFactoryConfigurationStore>,
  command: C,
) -> Result<C::Outcome, crate::ApplicationError>
where
  C: crate::Command,
  FactoryConfigurationHandlers<InMemoryFactoryConfigurationStore>:
    ManagementCommandUseCase<C, Error = crate::ApplicationError>,
{
  handlers
    .execute_management_command(
      &context(),
      &ManagementAuthorizationGrant::new(ManagementVisibility::all()),
      command,
    )
    .await
}

async fn query<Q>(
  handlers: &FactoryConfigurationHandlers<InMemoryFactoryConfigurationStore>,
  query: Q,
) -> Result<Q::Outcome, crate::ApplicationError>
where
  Q: crate::Query,
  FactoryConfigurationHandlers<InMemoryFactoryConfigurationStore>:
    ManagementQueryUseCase<Q, Error = crate::ApplicationError>,
{
  handlers
    .execute_management_query(
      &context(),
      &ManagementAuthorizationGrant::new(ManagementVisibility::all()),
      query,
    )
    .await
}

#[test]
fn immutable_creation_replacement_discovery_and_replay_are_atomic() {
  run_ready(async {
    let project_id = ProjectId::generate();
    let fixture = fixture(project_id);
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project_id).unwrap();
    store.enable_project(project_id, fixture.choices.clone()).unwrap();
    let handlers = FactoryConfigurationHandlers::new(store.clone());
    let id = FactoryConfigurationId::generate();

    let validation = query(
      &handlers,
      ValidateFactoryConfigurationQuery {
        id,
        project_id,
        definition_digest: digest(20),
        draft: fixture.draft.clone(),
      },
    )
    .await
    .unwrap();
    assert_eq!(
      validation.configuration.reference().version(),
      FactoryConfigurationVersion::INITIAL
    );
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 0);

    let create = CreateFactoryConfigurationCommand {
      id,
      project_id,
      definition_digest: digest(20),
      draft: fixture.draft.clone(),
      idempotency_key: idempotency("create"),
      published_at: time(1),
    };
    let created = command(&handlers, create.clone()).await.unwrap();
    let replayed = command(&handlers, create.clone()).await.unwrap();
    assert_eq!(created.disposition, MutationDisposition::Applied);
    assert_eq!(replayed.disposition, MutationDisposition::Replayed);
    assert_eq!(created.configuration, replayed.configuration);
    let mismatched_replay = command(
      &handlers,
      CreateFactoryConfigurationCommand {
        definition_digest: digest(22),
        ..create.clone()
      },
    )
    .await
    .unwrap_err();
    assert_eq!(mismatched_replay.classification(), ApplicationFailure::Conflict);
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 1);
    store.disable_project(project_id).unwrap();
    let replay_after_capability_change = command(&handlers, create.clone()).await.unwrap();
    assert_eq!(
      replay_after_capability_change.disposition,
      MutationDisposition::Replayed
    );
    store.enable_project(project_id, fixture.choices.clone()).unwrap();

    let mut replacement_draft = fixture.draft;
    replacement_draft.enabled = false;
    let replace = ReplaceFactoryConfigurationCommand {
      id,
      expected_current_version: FactoryConfigurationVersion::INITIAL,
      definition_digest: digest(21),
      draft: replacement_draft,
      idempotency_key: idempotency("replace"),
      published_at: time(2),
    };
    let replaced = command(&handlers, replace.clone()).await.unwrap();
    let replacement_replay = command(&handlers, replace.clone()).await.unwrap();
    assert_eq!(replaced.disposition, MutationDisposition::Applied);
    assert_eq!(replacement_replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replaced.configuration.configuration.reference().version().get(), 2);
    assert!(!replaced.configuration.configuration.is_enabled());
    store.disable_project(project_id).unwrap();
    let replacement_replay_after_capability_change = command(&handlers, replace.clone()).await.unwrap();
    assert_eq!(
      replacement_replay_after_capability_change.disposition,
      MutationDisposition::Replayed
    );
    store.enable_project(project_id, fixture.choices).unwrap();

    let current = query(&handlers, GetCurrentFactoryConfigurationQuery { id })
      .await
      .unwrap();
    let original = query(
      &handlers,
      GetFactoryConfigurationQuery {
        id,
        version: FactoryConfigurationVersion::INITIAL,
      },
    )
    .await
    .unwrap();
    assert_eq!(current, replaced.configuration);
    assert!(original.configuration.is_enabled());

    let stale = command(
      &handlers,
      ReplaceFactoryConfigurationCommand {
        idempotency_key: idempotency("stale"),
        published_at: time(3),
        ..replace
      },
    )
    .await
    .unwrap_err();
    assert_eq!(stale.classification(), ApplicationFailure::PreconditionFailed);
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 2);
    assert_eq!(store.management_audit_facts().await.len(), 2);
  });
}

#[test]
fn invalid_configuration_and_disabled_mode_fail_before_mutation_but_history_stays_readable() {
  run_ready(async {
    let project_id = ProjectId::generate();
    let mut invalid_configuration = fixture(project_id);
    invalid_configuration.draft.permission_ceiling = key("not-authorized");
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project_id).unwrap();
    store.enable_project(project_id, invalid_configuration.choices).unwrap();
    let handlers = FactoryConfigurationHandlers::new(store.clone());
    let id = FactoryConfigurationId::generate();
    let invalid = query(
      &handlers,
      ValidateFactoryConfigurationQuery {
        id,
        project_id,
        definition_digest: digest(30),
        draft: invalid_configuration.draft,
      },
    )
    .await
    .unwrap_err();
    assert_eq!(invalid.classification(), ApplicationFailure::Invalid);
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 0);

    let valid = fixture(project_id);
    store.enable_project(project_id, valid.choices).unwrap();
    command(
      &handlers,
      CreateFactoryConfigurationCommand {
        id,
        project_id,
        definition_digest: digest(31),
        draft: valid.draft.clone(),
        idempotency_key: idempotency("create-enabled"),
        published_at: time(1),
      },
    )
    .await
    .unwrap();
    store.disable_project(project_id).unwrap();
    let disabled = query(
      &handlers,
      ValidateFactoryConfigurationQuery {
        id: FactoryConfigurationId::generate(),
        project_id,
        definition_digest: digest(32),
        draft: valid.draft,
      },
    )
    .await
    .unwrap_err();
    assert_eq!(disabled.classification(), ApplicationFailure::CapabilityUnavailable);
    assert!(
      query(&handlers, GetCurrentFactoryConfigurationQuery { id })
        .await
        .is_ok()
    );
  });
}

#[derive(Clone, Copy)]
struct HideFactoryResources;

#[async_trait]
impl ManagementAuthorizationPolicy for HideFactoryResources {
  async fn authorize(
    &self,
    _context: &ManagementRequestContext,
    _action: ManagementAction,
    _resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    Err(ManagementAuthorizationDenial::forbidden())
  }
}

#[test]
fn hidden_project_and_configuration_are_rejected_before_dispatch() {
  run_ready(async {
    let project_id = ProjectId::generate();
    let fixture = fixture(project_id);
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project_id).unwrap();
    store.enable_project(project_id, fixture.choices).unwrap();
    let handlers = Arc::new(FactoryConfigurationHandlers::new(store.clone()));
    let id = FactoryConfigurationId::generate();
    let command_handler = AuthorizedCommandHandler::new(Arc::new(HideFactoryResources), handlers.clone());
    let denied = command_handler
      .handle_command(
        &context(),
        CreateFactoryConfigurationCommand {
          id,
          project_id,
          definition_digest: digest(40),
          draft: fixture.draft,
          idempotency_key: idempotency("hidden"),
          published_at: time(1),
        },
      )
      .await
      .unwrap_err();
    assert!(matches!(denied, ManagementHandlerError::Forbidden(_)));
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 0);

    let query_handler = AuthorizedQueryHandler::new(Arc::new(HideFactoryResources), handlers);
    let denied = query_handler
      .handle_query(&context(), GetCurrentFactoryConfigurationQuery { id })
      .await
      .unwrap_err();
    assert!(matches!(denied, ManagementHandlerError::Forbidden(_)));

    let trusted = TrustedNetworkManagementPolicy;
    assert!(
      trusted
        .authorize(
          &context(),
          ManagementAction::View,
          &ManagementResource::collection(crate::ManagementResourceKind::FactoryConfiguration).unwrap(),
        )
        .await
        .is_ok()
    );
  });
}

fn run_ready<T>(future: impl Future<Output = T>) -> T {
  let waker = Waker::noop();
  let mut context = Context::from_waker(waker);
  let mut future = std::pin::pin!(future);
  match future.as_mut().poll(&mut context) {
    Poll::Ready(value) => value,
    Poll::Pending => panic!("deterministic in-memory future unexpectedly yielded"),
  }
}

#[test]
fn factory_configuration_collection_applies_visibility_before_count_order_and_cursor() {
  run_ready(async {
    let project_id = ProjectId::generate();
    let fixture = fixture(project_id);
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project_id).unwrap();
    store.enable_project(project_id, fixture.choices).unwrap();
    let handlers = FactoryConfigurationHandlers::new(store);
    let ids = [
      FactoryConfigurationId::generate(),
      FactoryConfigurationId::generate(),
      FactoryConfigurationId::generate(),
    ];
    for (index, id) in ids.iter().enumerate() {
      command(
        &handlers,
        CreateFactoryConfigurationCommand {
          id: *id,
          project_id,
          definition_digest: digest(70),
          draft: fixture.draft.clone(),
          idempotency_key: idempotency(&format!("list-{index}")),
          published_at: time(index as i64 + 1),
        },
      )
      .await
      .unwrap();
    }
    let grant = ManagementAuthorizationGrant::new(
      ManagementVisibility::restricted([ids[0], ids[2]].into_iter().map(|id| {
        ManagementResource::instance(
          crate::ManagementResourceKind::FactoryConfiguration,
          crate::ManagementResourceIdentity::new(id.to_string()).unwrap(),
        )
        .unwrap()
      }))
      .unwrap(),
    );
    let first = handlers
      .execute_management_query(
        &context(),
        &grant,
        crate::ListProjectFactoryConfigurationsQuery::try_new(project_id, None, 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(first.total, 2);
    assert_eq!(first.items.len(), 1);
    assert_eq!(first.items[0].id, ids[2]);
    let second = handlers
      .execute_management_query(
        &context(),
        &grant,
        crate::ListProjectFactoryConfigurationsQuery::try_new(project_id, first.next_cursor, 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(second.total, 2);
    assert_eq!(second.items[0].id, ids[0]);
    assert!(second.next_cursor.is_none());
    assert!(crate::ListProjectFactoryConfigurationsQuery::try_new(project_id, None, 0).is_err());
    assert!(crate::ListProjectFactoryConfigurationsQuery::try_new(project_id, None, 201).is_err());
  });
}

#[test]
fn configuration_choices_apply_reference_visibility_before_filter_count_and_cursor() {
  run_ready(async {
    let project_id = ProjectId::generate();
    let mut fixture = fixture(project_id);
    let mut entries = fixture.choices.entries();
    entries.build_configurations.push((
      key("foreign-build"),
      BuildConfigurationRef::new(
        BuildConfigurationId::generate(),
        BuildConfigurationVersion::INITIAL,
        ProjectId::generate(),
        digest(9),
      ),
    ));
    fixture.choices = FactoryConfigurationChoices::try_new(entries).unwrap();
    fixture.choices = fixture
      .choices
      .with_choice_metadata(
        FactoryChoiceKind::CriterionPack,
        key("criteria"),
        octacity_server_factory::FactoryChoiceMetadata {
          permission_declarations: None,
          profile_selection: Some(octacity_server_factory::FactoryProfileSelectorMetadata {
            provider: exact("private-provider", "v1", 42),
            adapter: exact("private-adapter", "v1", 43),
            model: exact("private-model", "v1", 44),
            question_set: exact("private-criteria", "v1", 45),
            calibration: exact("calibration", "v1", 46),
            probability_semantics: exact("semantics", "v1", 47),
            mode: octacity_server_factory::DecisionSignalMode::Shadow,
            baseline: key("default"),
            candidates: octacity_server_factory::DecisionSignalChoices::try_new(vec![key("default"), key("pro")])
              .unwrap(),
            minimum_probability: octacity_server_factory::DecisionSignalProbability::new(800_000).unwrap(),
            minimum_margin: octacity_server_factory::DecisionSignalProbability::new(100_000).unwrap(),
          }),
          node_kinds: vec![octacity_server_factory::FlowNodeKind::Reasoning],
          signal_purposes: vec![],
          enforcement: vec![],
          compatible_choices: vec![
            octacity_server_factory::FactoryChoiceCompatibility {
              kind: FactoryChoiceKind::BuildConfiguration,
              alias: key("foreign-build"),
            },
            octacity_server_factory::FactoryChoiceCompatibility {
              kind: FactoryChoiceKind::Evaluator,
              alias: key("evaluator"),
            },
            octacity_server_factory::FactoryChoiceCompatibility {
              kind: FactoryChoiceKind::DeliveryAdapter,
              alias: key("delivery-adapter"),
            },
          ],
        },
      )
      .unwrap();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project_id).unwrap();
    store.enable_project(project_id, fixture.choices).unwrap();
    let handlers = FactoryConfigurationHandlers::new(store);
    let grant = ManagementAuthorizationGrant::new(
      ManagementVisibility::restricted([
        ManagementResource::instance(
          crate::ManagementResourceKind::FactoryChoice,
          crate::ManagementResourceIdentity::new(format!("{project_id}:build_configuration:foreign-build")).unwrap(),
        )
        .unwrap(),
        ManagementResource::instance(
          crate::ManagementResourceKind::FactoryChoice,
          crate::ManagementResourceIdentity::new(format!("{project_id}:criterion_pack:criteria")).unwrap(),
        )
        .unwrap(),
        ManagementResource::instance(
          crate::ManagementResourceKind::FactoryChoice,
          crate::ManagementResourceIdentity::new(format!("{project_id}:evaluator:evaluator")).unwrap(),
        )
        .unwrap(),
      ])
      .unwrap(),
    );
    let first = handlers
      .execute_management_query(
        &context(),
        &grant,
        crate::ListFactoryConfigurationChoicesQuery::try_new(project_id, None, None, 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(first.total, 2);
    assert_eq!(first.items.len(), 1);
    assert_eq!(
      first.items[0].identity.to_string(),
      format!("{project_id}:criterion_pack:criteria")
    );
    let metadata = first.items[0].metadata.as_ref().expect("declared safe metadata");
    assert_eq!(metadata.node_kinds, [octacity_server_factory::FlowNodeKind::Reasoning]);
    assert!(
      metadata.profile_selection.is_none(),
      "hidden selector dependencies must not be disclosed"
    );
    assert_eq!(metadata.compatible_choices.len(), 1);
    assert_eq!(metadata.compatible_choices[0].kind, FactoryChoiceKind::Evaluator);
    assert_eq!(metadata.compatible_choices[0].alias, key("evaluator"));
    let second = handlers
      .execute_management_query(
        &context(),
        &grant,
        crate::ListFactoryConfigurationChoicesQuery::try_new(project_id, None, first.next_cursor, 1).unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(second.total, 2);
    assert_eq!(
      second.items[0].identity.to_string(),
      format!("{project_id}:evaluator:evaluator")
    );
    assert!(second.next_cursor.is_none());
    let filtered = handlers
      .execute_management_query(
        &context(),
        &grant,
        crate::ListFactoryConfigurationChoicesQuery::try_new(
          project_id,
          Some(FactoryChoiceKind::DeliveryAdapter),
          None,
          10,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(filtered.total, 0);
    assert!(filtered.items.is_empty());
    assert!(crate::ListFactoryConfigurationChoicesQuery::try_new(project_id, None, None, 0).is_err());
    assert!(crate::ListFactoryConfigurationChoicesQuery::try_new(project_id, None, None, 201).is_err());
  });
}

#[test]
fn permission_choice_declarations_are_visible_only_with_their_exact_dependencies() {
  run_ready(async {
    let project = ProjectId::generate();
    let mut fixture = fixture(project);
    let executable = fixture
      .choices
      .entries()
      .references
      .into_iter()
      .find(|entry| entry.kind == FactoryChoiceKind::NodeCapability)
      .unwrap()
      .reference;
    let mut permissions = serde_json::to_value(fixture.draft.flow.as_ref().unwrap().limits.permissions()).unwrap();
    permissions["network_hosts"] = serde_json::json!(["api.allowed.example"]);
    permissions["mounts"] = serde_json::json!([{"root":"/workspace","mode":"read_only"}]);
    permissions["outputs"]["kinds"] = serde_json::json!(["report"]);
    permissions["executables"] = serde_json::json!([executable]);
    permissions["commands"] =
      serde_json::json!([{"executable":executable,"arguments":[{"kind":"exact","value":"--verify"}]}]);
    let metadata = serde_json::from_value(serde_json::json!({
      "node_kinds":[], "signal_purposes":[], "enforcement":[], "compatible_choices":[],
      "permission_declarations":permissions
    }))
    .expect("typed permission declarations are supported");
    fixture.choices = fixture
      .choices
      .with_choice_metadata(FactoryChoiceKind::PermissionCeiling, key("permissions"), metadata)
      .unwrap();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store.enable_project(project, fixture.choices).unwrap();
    let handlers = FactoryConfigurationHandlers::new(store);
    let all = query(
      &handlers,
      crate::ListFactoryConfigurationChoicesQuery::try_new(
        project,
        Some(FactoryChoiceKind::PermissionCeiling),
        None,
        25,
      )
      .unwrap(),
    )
    .await
    .unwrap();
    let declaration = serde_json::to_value(all.items[0].metadata.as_ref().unwrap()).unwrap();
    assert_eq!(
      declaration["permission_declarations"]["network_hosts"],
      serde_json::json!(["api.allowed.example"])
    );
    assert_eq!(
      declaration["permission_declarations"]["commands"]
        .as_array()
        .unwrap()
        .len(),
      1
    );
    let restricted = ManagementAuthorizationGrant::new(
      ManagementVisibility::restricted([ManagementResource::instance(
        crate::ManagementResourceKind::FactoryChoice,
        crate::ManagementResourceIdentity::new(format!("{project}:permission_ceiling:permissions")).unwrap(),
      )
      .unwrap()])
      .unwrap(),
    );
    let hidden = handlers
      .execute_management_query(
        &context(),
        &restricted,
        crate::ListFactoryConfigurationChoicesQuery::try_new(
          project,
          Some(FactoryChoiceKind::PermissionCeiling),
          None,
          25,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    let declaration = serde_json::to_value(hidden.items[0].metadata.as_ref().unwrap()).unwrap();
    assert!(declaration.get("permission_declarations").is_none());
    assert!(!declaration.to_string().contains("contract.capability"));
  });
}

#[test]
fn published_flow_discovery_identifies_native_gate_policies_without_catalog_aliases() {
  use octacity_server_factory::*;
  let project = ProjectId::generate();
  let flow = fixture(project).draft.flow.unwrap();
  let original = flow.closure.definition(flow.closure.root()).unwrap();
  let program = FlowGateProgram::new(vec![], key("succeeded"))
    .unwrap()
    .with_default_output_mapping(
      FlowDataMapping::new(FlowValueMapping::Constant {
        value: serde_json::json!(true),
      })
      .unwrap(),
    )
    .unwrap();
  let binding = program.binding().unwrap();
  let mut nodes = original.nodes().to_vec();
  let mut node = serde_json::to_value(&nodes[0]).unwrap();
  node["kind"] = serde_json::json!("deterministic_gate");
  node["action"] = serde_json::Value::Null;
  node["gate"] = serde_json::to_value(&binding).unwrap();
  nodes[0] = serde_json::from_value(node).unwrap();
  let root = FlowDefinition::new(FlowDefinitionInput {
    id: original.reference().id(),
    version: original.reference().version(),
    input_schema: original.input_schema().cloned(),
    entry: original.entry().clone(),
    nodes,
    transitions: original.transitions().to_vec(),
    terminals: original.terminals().to_vec(),
    context_projections: original.context_projections().to_vec(),
    data_projections: original.data_projections().to_vec(),
    execution: original.execution().clone(),
  })
  .unwrap();
  let flow = FactoryFlowConfiguration::new(
    PinnedFlowDefinitionClosure::new(root.reference(), vec![root]).unwrap(),
    flow.limits,
    flow.data_schemas,
    flow.pools,
  )
  .unwrap();
  assert_eq!(
    crate::factory_native_gate_policies(&flow),
    vec![binding.policy().clone()]
  );
}

#[test]
fn management_configuration_input_publishes_an_explicit_graph_and_replays_without_capability_resolution() {
  run_ready(async {
    let project_id = ProjectId::generate();
    let fixture = fixture(project_id);
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project_id).unwrap();
    store.enable_project(project_id, fixture.choices).unwrap();
    let handlers = FactoryConfigurationHandlers::new(store.clone());
    let inputs = crate::ManagementInputFactory::new(
      ["native".to_owned()],
      std::time::Duration::from_secs(900),
      crate::AgentEnrollmentSecretKey::new([7; 32]),
    )
    .unwrap();
    let id = FactoryConfigurationId::generate();
    let draft = serde_json::to_value(&fixture.draft).unwrap();
    let create = inputs
      .create_factory_configuration(id.as_uuid(), &project_id.to_string(), draft.clone(), "wire-create", 1)
      .unwrap();
    let original = command(&handlers, create).await.unwrap();
    assert_eq!(original.configuration.configuration.reference().id(), id);
    assert_eq!(
      original.configuration.configuration.flow(),
      fixture.draft.flow.as_ref().unwrap()
    );
    store.disable_project(project_id).unwrap();
    let replay = command(
      &handlers,
      inputs
        .create_factory_configuration(id.as_uuid(), &project_id.to_string(), draft.clone(), "wire-create", 2)
        .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.configuration, original.configuration);
    let mut unknown = draft;
    unknown["provider_payload"] = serde_json::json!({"token":"unsupported"});
    assert!(
      inputs
        .create_factory_configuration(id.as_uuid(), &project_id.to_string(), unknown, "wire-invalid", 3)
        .is_err()
    );
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 1);
  });
}

#[test]
fn management_configuration_replacement_preserves_original_graph_and_strong_preconditions() {
  run_ready(async {
    let project_id = ProjectId::generate();
    let fixture = fixture(project_id);
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project_id).unwrap();
    store.enable_project(project_id, fixture.choices).unwrap();
    let handlers = FactoryConfigurationHandlers::new(store.clone());
    let inputs = crate::ManagementInputFactory::new(
      ["native".to_owned()],
      std::time::Duration::from_secs(900),
      crate::AgentEnrollmentSecretKey::new([7; 32]),
    )
    .unwrap();
    let id = FactoryConfigurationId::generate();
    let mut draft = serde_json::to_value(&fixture.draft).unwrap();
    let original = command(
      &handlers,
      inputs
        .create_factory_configuration(id.as_uuid(), &project_id.to_string(), draft.clone(), "wire-create", 1)
        .unwrap(),
    )
    .await
    .unwrap();
    draft["enabled"] = serde_json::json!(false);
    let replace = inputs
      .replace_factory_configuration(&id.to_string(), 1, draft.clone(), "wire-replace", 2)
      .unwrap();
    let replaced = command(&handlers, replace).await.unwrap();
    assert_eq!(replaced.configuration.configuration.reference().version().get(), 2);
    assert!(!replaced.configuration.configuration.is_enabled());
    let first = query(
      &handlers,
      GetFactoryConfigurationQuery {
        id,
        version: FactoryConfigurationVersion::INITIAL,
      },
    )
    .await
    .unwrap();
    assert_eq!(first, original.configuration);
    let stale = command(
      &handlers,
      inputs
        .replace_factory_configuration(&id.to_string(), 1, draft, "wire-stale", 3)
        .unwrap(),
    )
    .await
    .unwrap_err();
    assert_eq!(stale.classification(), ApplicationFailure::PreconditionFailed);
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 2);
    assert!(
      inputs
        .replace_factory_configuration(
          &id.to_string(),
          0,
          serde_json::to_value(&fixture.draft).unwrap(),
          "wire-invalid",
          4
        )
        .is_err()
    );
  });
}

#[test]
fn management_choices_accept_node_execution_enforcement_and_credential_categories() {
  let choices = crate::factory_configuration_choice_kinds().collect::<std::collections::BTreeSet<_>>();
  for category in [
    "node_capability",
    "execution_policy",
    "execution_target",
    "enforcement_capability",
    "secret_profile",
    "workload_identity_profile",
  ] {
    assert!(choices.contains(category), "missing public choice category {category}");
  }
}

struct HideConfigurationChoices;
#[async_trait]
impl ManagementAuthorizationPolicy for HideConfigurationChoices {
  async fn authorize(
    &self,
    _context: &ManagementRequestContext,
    _action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    Ok(ManagementAuthorizationGrant::new(
      if resource.kind() == crate::ManagementResourceKind::FactoryChoice {
        ManagementVisibility::none()
      } else {
        ManagementVisibility::all()
      },
    ))
  }
}
#[test]
fn configuration_validation_cannot_resolve_an_alias_hidden_from_the_actor() {
  run_ready(async {
    let project = ProjectId::generate();
    let fixture = fixture(project);
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store.enable_project(project, fixture.choices).unwrap();
    let handlers = FactoryConfigurationHandlers::with_policy(store, Arc::new(HideConfigurationChoices));
    let error = handlers
      .execute_management_query(
        &context(),
        &ManagementAuthorizationGrant::new(ManagementVisibility::all()),
        ValidateFactoryConfigurationQuery {
          id: FactoryConfigurationId::generate(),
          project_id: project,
          definition_digest: digest(11),
          draft: fixture.draft,
        },
      )
      .await
      .unwrap_err();
    assert_eq!(error.classification(), ApplicationFailure::Invalid);
  });
}

#[test]
fn configuration_validation_rejects_an_unlisted_exact_node_capability() {
  run_ready(async {
    let project = ProjectId::generate();
    let fixture = fixture(project);
    let mut entries = fixture.choices.entries();
    entries
      .references
      .retain(|choice| choice.kind != FactoryChoiceKind::NodeCapability);
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store
      .enable_project(project, FactoryConfigurationChoices::try_new(entries).unwrap())
      .unwrap();
    let handlers = FactoryConfigurationHandlers::new(store);
    let error = query(
      &handlers,
      ValidateFactoryConfigurationQuery {
        id: FactoryConfigurationId::generate(),
        project_id: project,
        definition_digest: digest(11),
        draft: fixture.draft,
      },
    )
    .await
    .unwrap_err();
    assert_eq!(error.classification(), ApplicationFailure::Invalid);
  });
}

#[test]
fn configuration_validation_rejects_a_capability_from_another_choice_category() {
  run_ready(async {
    let project = ProjectId::generate();
    let fixture = fixture(project);
    let mut entries = fixture.choices.entries();
    for reference in &mut entries.references {
      if reference.kind == FactoryChoiceKind::NodeCapability {
        reference.kind = FactoryChoiceKind::CriterionPack;
      }
    }
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store
      .enable_project(project, FactoryConfigurationChoices::try_new(entries).unwrap())
      .unwrap();
    let handlers = FactoryConfigurationHandlers::new(store);
    let error = query(
      &handlers,
      ValidateFactoryConfigurationQuery {
        id: FactoryConfigurationId::generate(),
        project_id: project,
        definition_digest: digest(11),
        draft: fixture.draft,
      },
    )
    .await
    .unwrap_err();
    assert_eq!(error.classification(), ApplicationFailure::Invalid);
    let crate::ApplicationError::FactoryValidation(report) = error else {
      panic!("expected capability field diagnostics")
    };
    assert_eq!(report.errors[0].field, "/action/plugin");
    assert_eq!(report.errors[0].code, "unavailable_reference");
  });
}

#[test]
fn stale_configuration_replacement_is_rejected_before_current_capability_resolution() {
  run_ready(async {
    let project = ProjectId::generate();
    let fixture = fixture(project);
    let id = FactoryConfigurationId::generate();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store.enable_project(project, fixture.choices).unwrap();
    let handlers = FactoryConfigurationHandlers::new(store.clone());
    command(
      &handlers,
      CreateFactoryConfigurationCommand {
        id,
        project_id: project,
        definition_digest: digest(10),
        draft: fixture.draft.clone(),
        idempotency_key: idempotency("initial"),
        published_at: time(1),
      },
    )
    .await
    .unwrap();
    command(
      &handlers,
      ReplaceFactoryConfigurationCommand {
        id,
        expected_current_version: FactoryConfigurationVersion::INITIAL,
        definition_digest: digest(11),
        draft: fixture.draft.clone(),
        idempotency_key: idempotency("replacement"),
        published_at: time(2),
      },
    )
    .await
    .unwrap();
    store.disable_project(project).unwrap();
    let error = command(
      &handlers,
      ReplaceFactoryConfigurationCommand {
        id,
        expected_current_version: FactoryConfigurationVersion::INITIAL,
        definition_digest: digest(12),
        draft: fixture.draft,
        idempotency_key: idempotency("stale-baseline"),
        published_at: time(3),
      },
    )
    .await
    .unwrap_err();
    assert_eq!(error.classification(), ApplicationFailure::PreconditionFailed);
  });
}

#[test]
fn configuration_validation_preserves_plugin_parameter_objects_that_resemble_references() {
  run_ready(async {
    let project = ProjectId::generate();
    let mut fixture = fixture(project);
    let flow = fixture.draft.flow.take().unwrap();
    let root = flow.closure.definition(flow.closure.root()).unwrap();
    let mut nodes = root.nodes().to_vec();
    let action = nodes[0].action().unwrap();
    nodes[0] = nodes[0]
      .clone()
      .with_action(
        octacity_server_factory::FlowActionBinding::new(
          action.plugin().clone(),
          action.action().clone(),
          serde_json::json!({"identity": "business-object", "version": "draft", "digest": "opaque-text"}),
        )
        .unwrap(),
      )
      .unwrap();
    let root = octacity_server_factory::FlowDefinition::new(octacity_server_factory::FlowDefinitionInput {
      id: root.reference().id(),
      version: root.reference().version(),
      input_schema: root.input_schema().cloned(),
      entry: root.entry().clone(),
      nodes,
      transitions: root.transitions().to_vec(),
      terminals: root.terminals().to_vec(),
      context_projections: root.context_projections().to_vec(),
      data_projections: root.data_projections().to_vec(),
      execution: root.execution().clone(),
    })
    .unwrap();
    fixture.draft.flow = Some(
      octacity_server_factory::FactoryFlowConfiguration::new(
        octacity_server_factory::PinnedFlowDefinitionClosure::new(root.reference(), vec![root]).unwrap(),
        flow.limits,
        flow.data_schemas,
        flow.pools,
      )
      .unwrap(),
    );
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store.enable_project(project, fixture.choices).unwrap();
    let handlers = FactoryConfigurationHandlers::new(store);
    assert!(
      query(
        &handlers,
        ValidateFactoryConfigurationQuery {
          id: FactoryConfigurationId::generate(),
          project_id: project,
          definition_digest: digest(11),
          draft: fixture.draft,
        }
      )
      .await
      .is_ok()
    );
  });
}

#[test]
fn validation_explains_an_unavailable_alias_without_disclosing_catalogue_choices() {
  run_ready(async {
    let project = ProjectId::generate();
    let mut fixture = fixture(project);
    fixture.draft.admission_policy = key("private-policy");
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store.enable_project(project, fixture.choices).unwrap();
    let error = query(
      &FactoryConfigurationHandlers::new(store),
      ValidateFactoryConfigurationQuery {
        id: FactoryConfigurationId::generate(),
        project_id: project,
        definition_digest: digest(11),
        draft: fixture.draft,
      },
    )
    .await
    .unwrap_err();
    let crate::ApplicationError::FactoryValidation(report) = error else {
      panic!("expected field diagnostics")
    };
    assert_eq!(report.errors[0].field, "/admission_policy");
    assert_eq!(report.errors[0].code, "unavailable_reference");
    assert_eq!(report.unavailable_references, ["/admission_policy"]);
  });
}

#[test]
fn configuration_validation_rejects_an_unavailable_typed_data_handling_policy() {
  run_ready(async {
    use octacity_server_factory::*;
    let project = ProjectId::generate();
    let mut fixture = fixture(project);
    let flow = fixture.draft.flow.take().unwrap();
    let root = flow.closure.definition(flow.closure.root()).unwrap();
    let mut nodes = root.nodes().to_vec();
    let plugin = nodes[0].action().unwrap().plugin().clone();
    nodes[0] = nodes[0]
      .clone()
      .with_build(FlowBuildBinding {
        tool: plugin.clone(),
        result_tool: None,
        capture_write_paths: None,
        capture_deliverables: None,
        plugin: plugin.clone(),
        model_or_tool: plugin,
        task_digest: digest(12),
        build_configuration: fixture.choices.entries().build_configurations[0].1.clone(),
        result_output: key("report"),
        result_output_kind: EvidenceOutputKind::Report,
        result_outcome: key("succeeded"),
        evidence: vec![],
      })
      .unwrap();
    let verification = VerificationPolicy {
      data_handling: exact("unavailable-handling", "v1", 13),
      required_quorum: 1,
      checks: std::collections::BTreeMap::from([(
        nodes[0].key().clone(),
        VerificationCheck {
          class: key("public-contract"),
          target: FlowDataMapping::new(FlowValueMapping::Constant {
            value: serde_json::json!(true),
          })
          .unwrap(),
          source_visibility: VerificationSourceVisibility::BlackBox,
          endpoints: vec![],
          secret_profiles: vec![],
          identity_profiles: vec![],
          required: true,
        },
      )]),
    };
    let mut execution = serde_json::to_value(root.execution()).unwrap();
    execution["verification"] = serde_json::to_value(verification).unwrap();
    let root = FlowDefinition::new(FlowDefinitionInput {
      id: root.reference().id(),
      version: root.reference().version(),
      input_schema: root.input_schema().cloned(),
      entry: root.entry().clone(),
      nodes,
      transitions: root.transitions().to_vec(),
      terminals: root.terminals().to_vec(),
      context_projections: root.context_projections().to_vec(),
      data_projections: root.data_projections().to_vec(),
      execution: serde_json::from_value(execution).unwrap(),
    })
    .unwrap();
    fixture.draft.flow = Some(
      FactoryFlowConfiguration::new(
        PinnedFlowDefinitionClosure::new(root.reference(), vec![root]).unwrap(),
        flow.limits,
        flow.data_schemas,
        flow.pools,
      )
      .unwrap(),
    );
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store.enable_project(project, fixture.choices).unwrap();
    let error = query(
      &FactoryConfigurationHandlers::new(store),
      ValidateFactoryConfigurationQuery {
        id: FactoryConfigurationId::generate(),
        project_id: project,
        definition_digest: digest(11),
        draft: fixture.draft,
      },
    )
    .await
    .unwrap_err();
    let crate::ApplicationError::FactoryValidation(report) = error else {
      panic!("expected policy diagnostics")
    };
    assert_eq!(report.errors[0].field, "/execution/verification/data_handling");
  });
}

#[test]
fn schema_normalization_rejects_hidden_owners_and_unbounded_shapes_without_writes() {
  run_ready(async {
    let project_id = ProjectId::generate();
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project_id).unwrap();
    store.enable_project(project_id, fixture(project_id).choices).unwrap();
    let policy = Arc::new(HideFactoryResources);
    let hidden = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(crate::FactoryFlowDefinitionHandlers::new(store.clone(), policy)),
    );
    let query = crate::ValidateFactoryFlowDataSchemaQuery {
      project_id,
      identity: key("operator.boolean"),
      version: key("v1"),
      shape: octacity_server_factory::FlowValueSchema::Boolean,
    };
    assert!(matches!(
      hidden.handle_query(&context(), query.clone()).await,
      Err(ManagementHandlerError::Forbidden(_))
    ));
    let policy = Arc::new(TrustedNetworkManagementPolicy);
    let visible = AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(crate::FactoryFlowDefinitionHandlers::new(store.clone(), policy)),
    );
    let oversized = crate::ValidateFactoryFlowDataSchemaQuery {
      shape: octacity_server_factory::FlowValueSchema::String {
        min_bytes: 0,
        max_bytes: usize::MAX,
      },
      ..query
    };
    assert!(visible.handle_query(&context(), oversized).await.is_err());
    assert!(store.management_audit_facts().await.is_empty());
  });
}

#[test]
fn configuration_publication_rejects_stage_source_effects_without_an_installed_authorizer() {
  run_ready(async {
    use octacity_server_factory::{
      FactoryFlowConfiguration, FlowDefinition, FlowDefinitionInput, PinnedFlowDefinitionClosure,
    };
    let project = ProjectId::generate();
    let mut fixture = fixture(project);
    let flow = fixture.draft.flow.take().unwrap();
    let root = flow.closure.definition(flow.closure.root()).unwrap();
    let mut nodes = root.nodes().to_vec();
    let action = nodes[0].action().unwrap();
    let source = octacity_server_factory::WorkSourceBinding {
      installation: exact("tracker", "v1", 80),
      capability: exact("reader", "v1", 81),
      configuration: exact("reader-settings", "v1", 82),
      resource: exact("tickets", "v1", 83),
    };
    let mut reporter = source.clone();
    reporter.capability = action.plugin().clone();
    reporter.configuration = exact("writer-settings", "v1", 84);
    let stage_update = serde_json::json!({
      "source":source, "reporter":reporter, "connection":null,
      "effects":{"status":"review-next","add":["review-label"],"remove":["fresh-label"],"state":null}
    });
    nodes[0] = nodes[0]
      .clone()
      .with_action(
        octacity_server_factory::FlowActionBinding::new(
          action.plugin().clone(),
          key("set_status"),
          serde_json::json!({
            "status":"review-next", "outbox":{"max_attempts":2,"retry_delay_millis":5}, "source_update":stage_update
          }),
        )
        .unwrap(),
      )
      .unwrap();
    let root = FlowDefinition::new(FlowDefinitionInput {
      id: root.reference().id(),
      version: root.reference().version(),
      input_schema: root.input_schema().cloned(),
      entry: root.entry().clone(),
      nodes,
      transitions: root.transitions().to_vec(),
      terminals: root.terminals().to_vec(),
      context_projections: root.context_projections().to_vec(),
      data_projections: root.data_projections().to_vec(),
      execution: root.execution().clone(),
    })
    .unwrap();
    fixture.draft.flow = Some(
      FactoryFlowConfiguration::new(
        PinnedFlowDefinitionClosure::new(root.reference(), vec![root]).unwrap(),
        flow.limits,
        flow.data_schemas,
        flow.pools,
      )
      .unwrap(),
    );
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store.enable_project(project, fixture.choices).unwrap();
    let candidate = ValidateFactoryConfigurationQuery {
      id: FactoryConfigurationId::generate(),
      project_id: project,
      definition_digest: digest(33),
      draft: fixture.draft,
    };
    let error = query(&FactoryConfigurationHandlers::new(store.clone()), candidate.clone())
      .await
      .unwrap_err();
    assert_eq!(error.classification(), ApplicationFailure::CapabilityUnavailable);
    struct InstalledAuthorizer {
      project: ProjectId,
      expected: crate::FactoryStageSourceUpdate,
    }
    #[async_trait]
    impl crate::FactoryStageSourceAuthorizer for InstalledAuthorizer {
      async fn validate_source_update(
        &self,
        _: &ManagementRequestContext,
        project: ProjectId,
        update: &crate::FactoryStageSourceUpdate,
      ) -> Result<(), crate::ApplicationError> {
        assert_eq!(project, self.project);
        assert_eq!(update, &self.expected);
        Ok(())
      }
    }
    let authorized =
      FactoryConfigurationHandlers::new(store.clone()).with_source_actions(Arc::new(InstalledAuthorizer {
        project,
        expected: serde_json::from_value(stage_update).unwrap(),
      }));
    let validated = query(&authorized, candidate).await.unwrap();
    let pinned = validated.configuration.flow().closure.definitions()[0].nodes()[0]
      .action()
      .unwrap();
    assert_eq!(
      pinned.parameters()["source_update"]["effects"]["add"],
      serde_json::json!(["review-label"])
    );
    assert_eq!(
      pinned.parameters()["source_update"]["effects"]["remove"],
      serde_json::json!(["fresh-label"])
    );
    assert!(store.management_audit_facts().await.is_empty());
  });
}

#[test]
fn issue_status_action_publication_requires_workflow_authorization() {
  run_ready(async {
    let project = ProjectId::generate();
    let mut fixture = fixture(project);
    fixture.draft.flow = Some(crate::factory_admission_tests::with_issue_status_action(
      fixture.draft.flow.as_ref().unwrap(),
      octacity_server_store::IssuePipelineStatusSelection {
        workflow_id: "10000000-0000-4000-8000-000000000010".parse().unwrap(),
        workflow_version: octacity_server_domain::IssueWorkflowVersion::INITIAL,
        transition_id: "10000000-0000-4000-8000-000000000011".parse().unwrap(),
        outcome: "succeeded".parse().unwrap(),
      },
    ));
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store.enable_project(project, fixture.choices).unwrap();
    let error = query(
      &FactoryConfigurationHandlers::new(store.clone()),
      ValidateFactoryConfigurationQuery {
        id: FactoryConfigurationId::generate(),
        project_id: project,
        definition_digest: digest(33),
        draft: fixture.draft,
      },
    )
    .await
    .expect_err("a local status action must be checked against visible workflows");
    assert_eq!(
      error.classification(),
      ApplicationFailure::CapabilityUnavailable,
      "{error:?}"
    );
    assert!(store.management_audit_facts().await.is_empty());
  });
}

#[test]
fn issue_status_publication_uses_visible_retained_workflows_and_rejects_model_selected_moves() {
  run_ready(async {
    use octacity_server_store::{IssueWorkflowStore, ProjectStore};
    let project = ProjectId::generate();
    let issues = Arc::new(octacity_server_store::testing::InMemoryProjectStore::new());
    issues
      .create_project(octacity_server_store::testing::management_mutation(
        octacity_server_store::CreateProject {
          id: project,
          parent_id: None,
          name: octacity_server_domain::ProjectName::new("Manual policies").unwrap(),
          idempotency_key: idempotency("policy-project"),
          created_at: time(1),
        },
      ))
      .await
      .unwrap();
    let workflow = issues.default_issue_workflow(project).await.unwrap().workflow;
    let edge = workflow
      .definition
      .transitions
      .iter()
      .find(|edge| {
        edge
          .origins
          .contains(&octacity_server_store::IssueTransitionOrigin::Pipeline)
      })
      .unwrap();
    let selection = octacity_server_store::IssuePipelineStatusSelection {
      workflow_id: workflow.id,
      workflow_version: workflow.version,
      transition_id: edge.id,
      outcome: key("succeeded"),
    };
    let mut f = fixture(project);
    let original = f.draft.flow.as_ref().unwrap().clone();
    f.draft.flow = Some(crate::factory_admission_tests::with_issue_status_action(
      &original,
      selection.clone(),
    ));
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project).unwrap();
    store.enable_project(project, f.choices).unwrap();
    let queries = Arc::new(crate::AuthorizedQueryHandler::new(
      Arc::new(TrustedNetworkManagementPolicy),
      Arc::new(crate::IssueWorkflowHandlers::new(issues)),
    ));
    let handlers = FactoryConfigurationHandlers::new(store.clone()).with_issue_status_actions(queries);
    let candidate = ValidateFactoryConfigurationQuery {
      id: FactoryConfigurationId::generate(),
      project_id: project,
      definition_digest: digest(38),
      draft: f.draft,
    };
    let validated = query(&handlers, candidate.clone()).await.unwrap();
    assert_eq!(
      validated.configuration.flow().closure.definitions()[0].nodes()[0]
        .action()
        .unwrap()
        .parameters()["transition_id"],
      serde_json::json!(edge.id)
    );
    for invalid in [
      octacity_server_store::IssuePipelineStatusSelection {
        outcome: key("completion-prose"),
        ..selection.clone()
      },
      octacity_server_store::IssuePipelineStatusSelection {
        transition_id: octacity_server_domain::IssueTransitionId::generate(),
        ..selection
      },
    ] {
      let mut candidate = candidate.clone();
      candidate.draft.flow = Some(crate::factory_admission_tests::with_issue_status_action(
        &original, invalid,
      ));
      assert!(query(&handlers, candidate).await.is_err());
    }
    assert!(store.management_audit_facts().await.is_empty());
  });
}

mod profile_selection;
