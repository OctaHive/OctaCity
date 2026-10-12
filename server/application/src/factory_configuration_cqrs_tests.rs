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
