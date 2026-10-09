use std::{
  future::Future,
  sync::Arc,
  task::{Context, Poll, Waker},
};

use async_trait::async_trait;
use octacity_server_domain::{BuildConfigurationId, BuildConfigurationVersion, ProjectId, Timestamp};
use octacity_server_factory::{
  BudgetLimit, BuildConfigurationRef, DecisionOutcome, DeliveryPolicyDraft, EvaluationPolicyDraft, FactoryChoiceKind,
  FactoryConfigurationChoiceEntries, FactoryConfigurationChoices, FactoryConfigurationDraft, FactoryConfigurationId,
  FactoryConfigurationVersion, FactoryCredentialProfiles, FactoryDigest, FactoryKey, FactoryReferenceChoice,
  FactoryStageDraft, FactoryStageKind, FactoryWipLimits, ImmutableReference, ReworkPolicyDraft,
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
  let stage = |name, kind| FactoryStageDraft {
    key: key(name),
    kind,
    build_configuration: key("build"),
    budget: budget(2),
  };
  let draft = FactoryConfigurationDraft {
    admission_policy: key("admission"),
    stages: vec![
      stage("implement", FactoryStageKind::Implementation),
      stage("validate", FactoryStageKind::Validation),
      stage("evaluate", FactoryStageKind::Evaluation),
    ],
    wip_limits: FactoryWipLimits::new(2, 4).unwrap(),
    hard_budget: budget(10),
    permission_ceiling: key("permissions"),
    credential_profiles: credential_profiles(),
    decision_signals: Vec::new(),
    evaluation: EvaluationPolicyDraft {
      criterion_packs: vec![key("criteria")],
      evaluators: vec![key("evaluator")],
      required_quorum: 1,
      budget: budget(2),
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
fn project_ownership_and_disabled_mode_fail_before_mutation_but_history_stays_readable() {
  run_ready(async {
    let project_id = ProjectId::generate();
    let other_project_id = ProjectId::generate();
    let wrong_owner = fixture(other_project_id);
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    store.seed_project(project_id).unwrap();
    store.enable_project(project_id, wrong_owner.choices).unwrap();
    let handlers = FactoryConfigurationHandlers::new(store.clone());
    let id = FactoryConfigurationId::generate();
    let invalid = query(
      &handlers,
      ValidateFactoryConfigurationQuery {
        id,
        project_id,
        definition_digest: digest(30),
        draft: wrong_owner.draft,
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
