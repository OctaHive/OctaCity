use std::{
  collections::BTreeSet,
  future::Future,
  sync::{Arc, Mutex},
  task::{Context, Poll, Waker},
};

use async_trait::async_trait;
use octacity_server_domain::{
  ArtifactId, BuildConfigurationId, BuildConfigurationVersion, IntegrationId, ProjectId, RepositoryId,
  RepositoryLocator, RepositoryName, RepositoryVersion, SourceReference, Timestamp,
};
use octacity_server_factory::{
  BudgetLimit, BuildConfigurationRef, ExternalWorkIdentity, FactoryArtifactReference, FactoryChoiceKind,
  FactoryConfiguration, FactoryConfigurationChoiceEntries, FactoryConfigurationChoices, FactoryConfigurationDraft,
  FactoryConfigurationId, FactoryConfigurationVersion, FactoryDigest, FactoryKey, FactoryMetadata,
  FactoryReferenceChoice, FactoryStageKind, FactoryWipLimits, ImmutableReference, RiskClass, WorkArtifacts,
  WorkClassification, WorkPriority,
};
use octacity_server_store::{
  FactoryRunStore, IdempotencyKey, PublishedFactoryConfiguration, PublishedRepository, RepositoryDefinition,
  RepositorySelectionPolicy,
  testing::{InMemoryFactoryConfigurationStore, ManagementAuditProbe, MutationEvidenceProbe},
};
use uuid::Uuid;

use crate::{
  AdmitManualFactoryWorkCommand, ApplicationFailure, AuthorizedCommandHandler, FactoryAdmissionHandlers,
  ManagementAction, ManagementAuthorizationDenial, ManagementAuthorizationGrant, ManagementAuthorizationPolicy,
  ManagementCommandUseCase, ManagementHandlerError, ManagementRequestContext, ManagementRequestId, ManagementResource,
  ManagementVisibility, MutationDisposition, RepositorySourceSelection, RevisionResolutionError,
  RevisionResolutionRequest, RevisionResolver,
};

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}

fn digest(value: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([value; 32])
}

fn artifact(value: u8) -> FactoryArtifactReference {
  FactoryArtifactReference::new(ArtifactId::generate(), digest(value), 1).unwrap()
}

fn exact(identity: &str, value: u8) -> ImmutableReference {
  ImmutableReference::new(key(identity), key("v1"), digest(value))
}

pub(super) fn configuration(project_id: ProjectId, id: FactoryConfigurationId) -> FactoryConfiguration {
  configuration_with_flow(project_id, id, None)
}

pub(super) fn configuration_with_flow(
  project_id: ProjectId,
  id: FactoryConfigurationId,
  flow: Option<octacity_server_factory::FactoryFlowConfiguration>,
) -> FactoryConfiguration {
  let build = BuildConfigurationRef::new(
    BuildConfigurationId::generate(),
    BuildConfigurationVersion::INITIAL,
    project_id,
    digest(7),
  );
  let choices = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
    references: vec![
      FactoryReferenceChoice {
        kind: FactoryChoiceKind::AdmissionPolicy,
        alias: key("admission"),
        reference: exact("manual.medium", 1),
      },
      FactoryReferenceChoice {
        kind: FactoryChoiceKind::PermissionCeiling,
        alias: key("permissions"),
        reference: exact("restricted", 2),
      },
      FactoryReferenceChoice {
        kind: FactoryChoiceKind::CriterionPack,
        alias: key("criteria"),
        reference: exact("quality", 3),
      },
      FactoryReferenceChoice {
        kind: FactoryChoiceKind::Evaluator,
        alias: key("evaluator"),
        reference: exact("review", 4),
      },
      FactoryReferenceChoice {
        kind: FactoryChoiceKind::DeliveryAdapter,
        alias: key("delivery-adapter"),
        reference: exact("github", 5),
      },
      FactoryReferenceChoice {
        kind: FactoryChoiceKind::DeliveryPolicy,
        alias: key("delivery-policy"),
        reference: exact("human-review", 6),
      },
    ],
    build_configurations: vec![(key("build"), build)],
  })
  .unwrap();
  let limit = flow.as_ref().map_or_else(
    || BudgetLimit::new(10, 10_000, 1_000, 10_000, 10_000).unwrap(),
    |configuration| configuration.limits.budget(),
  );
  let budget = || limit;
  let stage = |name, kind| octacity_server_factory::testing::FactoryStageDraft {
    key: key(name),
    kind,
    build_configuration: key("build"),
    budget: budget(),
  };
  FactoryConfiguration::publish(
    id,
    FactoryConfigurationVersion::INITIAL,
    project_id,
    digest(20),
    octacity_server_factory::testing::with_contract_flow(
      FactoryConfigurationDraft {
        flow,
        admission_policy: key("admission"),
        wip_limits: FactoryWipLimits::new(2, 4).unwrap(),
        hard_budget: budget(),
        permission_ceiling: key("permissions"),
        enabled: true,
      },
      vec![
        stage("implement", FactoryStageKind::Implementation),
        stage("validate", FactoryStageKind::Validation),
        stage("evaluate", FactoryStageKind::Evaluation),
      ],
      &choices,
    ),
    &choices,
  )
  .unwrap()
}

pub(super) fn repository(
  project_id: ProjectId,
  id: RepositoryId,
  default_reference: Option<SourceReference>,
) -> PublishedRepository {
  let main = SourceReference::new("refs/heads/main").unwrap();
  PublishedRepository {
    id,
    project_id,
    name: RepositoryName::new("source").unwrap(),
    version: RepositoryVersion::INITIAL,
    definition: RepositoryDefinition {
      vcs_integration_id: IntegrationId::generate(),
      repository_locator: RepositoryLocator::new("https://example.test/source.git").unwrap(),
      selection: RepositorySelectionPolicy {
        allowed_references: BTreeSet::from([main]),
        default_reference,
        allow_exact_revision: true,
      },
    },
    published_at: time(1),
  }
}

pub(super) fn command(
  project_id: ProjectId,
  configuration_id: FactoryConfigurationId,
  repository_id: RepositoryId,
) -> AdmitManualFactoryWorkCommand {
  AdmitManualFactoryWorkCommand {
    project_id,
    configuration_id,
    configuration_version: FactoryConfigurationVersion::INITIAL,
    repository_id,
    repository_version: RepositoryVersion::INITIAL,
    external_identity: ExternalWorkIdentity::new("manual/work-42").unwrap(),
    source: RepositorySourceSelection::DefaultReference,
    artifacts: WorkArtifacts::new(artifact(90), artifact(91), Vec::new()).unwrap(),
    classification: WorkClassification::new(
      WorkPriority::new(20).unwrap(),
      RiskClass::Medium,
      FactoryMetadata::try_new([("tracker".to_owned(), "work-42".to_owned())]).unwrap(),
    ),
    idempotency_key: IdempotencyKey::new("admit-work-42").unwrap(),
    admitted_at: time(2),
  }
}

pub(super) fn context() -> ManagementRequestContext {
  ManagementRequestContext::trusted_network(
    ManagementRequestId::new(Uuid::from_u128(0x1d3c_437b_e606_4895_9677_0c86_3466_b5f4)).unwrap(),
  )
}

pub(super) fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}

pub(super) fn seeded_store(
  project_id: ProjectId,
  configuration_id: FactoryConfigurationId,
  repository: PublishedRepository,
) -> Arc<InMemoryFactoryConfigurationStore> {
  let store = Arc::new(InMemoryFactoryConfigurationStore::new());
  store.seed_project(project_id).unwrap();
  store
    .seed_factory_configuration_version(PublishedFactoryConfiguration {
      configuration: configuration(project_id, configuration_id),
      published_at: time(1),
    })
    .unwrap();
  store.seed_repository_version(repository).unwrap();
  store
}

#[derive(Default)]
pub(super) struct RecordingResolver {
  requests: Mutex<Vec<RevisionResolutionRequest>>,
}

impl RecordingResolver {
  pub(super) fn call_count(&self) -> usize {
    self.requests.lock().unwrap().len()
  }
}

#[async_trait]
impl RevisionResolver for RecordingResolver {
  async fn resolve(
    &self,
    request: RevisionResolutionRequest,
  ) -> Result<octacity_server_domain::ImmutableRevision, RevisionResolutionError> {
    self.requests.lock().unwrap().push(request);
    Ok(octacity_server_domain::ImmutableRevision::new("0123456789abcdef").unwrap())
  }
}

pub(super) async fn admit(
  handlers: &FactoryAdmissionHandlers<InMemoryFactoryConfigurationStore>,
  command: AdmitManualFactoryWorkCommand,
) -> Result<crate::FactoryAdmissionOutcome, crate::ApplicationError> {
  handlers
    .execute_management_command(
      &context(),
      &ManagementAuthorizationGrant::new(ManagementVisibility::all()),
      command,
    )
    .await
}

#[test]
fn exact_scoped_replay_returns_one_run_before_resolving_mutable_state_again() {
  run_ready(async {
    let project_id = ProjectId::generate();
    let configuration_id = FactoryConfigurationId::generate();
    let repository_id = RepositoryId::generate();
    let main = SourceReference::new("refs/heads/main").unwrap();
    let store = seeded_store(
      project_id,
      configuration_id,
      repository(project_id, repository_id, Some(main)),
    );
    let resolver = Arc::new(RecordingResolver::default());
    let handlers = FactoryAdmissionHandlers::new(store.clone(), resolver.clone());
    let request = command(project_id, configuration_id, repository_id);

    let applied = admit(&handlers, request.clone()).await.unwrap();
    let replayed = admit(
      &handlers,
      AdmitManualFactoryWorkCommand {
        idempotency_key: IdempotencyKey::new("another-transport-attempt").unwrap(),
        ..request.clone()
      },
    )
    .await
    .unwrap();
    assert_eq!(applied.disposition, MutationDisposition::Applied);
    assert_eq!(replayed.disposition, MutationDisposition::Replayed);
    assert_eq!(applied.factory_run_id, replayed.factory_run_id);
    assert_eq!(applied.work_envelope_id, replayed.work_envelope_id);
    assert_eq!(resolver.call_count(), 1);
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 1);
    assert_eq!(store.management_audit_facts().await.len(), 1);
    let snapshot = store.factory_run_snapshot(applied.factory_run_id).await.unwrap();
    assert_eq!(snapshot.work.id(), applied.work_envelope_id);
    assert_eq!(snapshot.run.id(), applied.factory_run_id);
    assert_eq!(snapshot.budgets.len(), 1);
    let audit = store
      .list_factory_run_diagnostics(
        octacity_server_store::ListFactoryRunDiagnostics::new(
          snapshot.run.id(),
          octacity_server_store::FactoryRunDiagnosticKind::Audit,
          None,
          100,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(audit.items.len(), 1);
    assert_eq!(snapshot.outbox.len(), 1);

    let mismatch = admit(
      &handlers,
      AdmitManualFactoryWorkCommand {
        source: RepositorySourceSelection::ExactRevision(
          octacity_server_domain::ImmutableRevision::new("different-revision").unwrap(),
        ),
        ..request
      },
    )
    .await
    .unwrap_err();
    assert_eq!(mismatch.classification(), ApplicationFailure::Conflict);
    assert_eq!(resolver.call_count(), 1);
  });
}

#[test]
fn configuration_active_run_limit_is_enforced_atomically_without_blocking_replay() {
  run_ready(async {
    let project_id = ProjectId::generate();
    let configuration_id = FactoryConfigurationId::generate();
    let repository_id = RepositoryId::generate();
    let main = SourceReference::new("refs/heads/main").unwrap();
    let store = seeded_store(
      project_id,
      configuration_id,
      repository(project_id, repository_id, Some(main)),
    );
    let resolver = Arc::new(RecordingResolver::default());
    let handlers = FactoryAdmissionHandlers::new(store.clone(), resolver);
    let first = command(project_id, configuration_id, repository_id);
    let second = AdmitManualFactoryWorkCommand {
      external_identity: ExternalWorkIdentity::new("manual/work-43").unwrap(),
      idempotency_key: IdempotencyKey::new("admit-work-43").unwrap(),
      ..first.clone()
    };
    let third = AdmitManualFactoryWorkCommand {
      external_identity: ExternalWorkIdentity::new("manual/work-44").unwrap(),
      idempotency_key: IdempotencyKey::new("admit-work-44").unwrap(),
      ..first.clone()
    };

    assert_eq!(
      admit(&handlers, first.clone()).await.unwrap().disposition,
      MutationDisposition::Applied
    );
    assert_eq!(
      admit(&handlers, second).await.unwrap().disposition,
      MutationDisposition::Applied
    );
    assert_eq!(
      admit(&handlers, third).await.unwrap_err().classification(),
      ApplicationFailure::Conflict
    );
    assert_eq!(
      admit(&handlers, first).await.unwrap().disposition,
      MutationDisposition::Replayed
    );
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 2);
  });
}

#[test]
fn ambiguous_or_disallowed_repository_references_never_admit_work() {
  run_ready(async {
    let project_id = ProjectId::generate();
    let configuration_id = FactoryConfigurationId::generate();
    let repository_id = RepositoryId::generate();
    let store = seeded_store(
      project_id,
      configuration_id,
      repository(project_id, repository_id, None),
    );
    let resolver = Arc::new(RecordingResolver::default());
    let handlers = FactoryAdmissionHandlers::new(store.clone(), resolver.clone());

    let missing_default = admit(&handlers, command(project_id, configuration_id, repository_id))
      .await
      .unwrap_err();
    assert_eq!(missing_default.classification(), ApplicationFailure::Invalid);
    let disallowed = admit(
      &handlers,
      AdmitManualFactoryWorkCommand {
        external_identity: ExternalWorkIdentity::new("manual/work-43").unwrap(),
        idempotency_key: IdempotencyKey::new("admit-work-43").unwrap(),
        source: RepositorySourceSelection::Reference(SourceReference::new("refs/heads/other").unwrap()),
        ..command(project_id, configuration_id, repository_id)
      },
    )
    .await
    .unwrap_err();
    assert_eq!(disallowed.classification(), ApplicationFailure::Invalid);
    assert_eq!(resolver.call_count(), 0);
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 0);
  });
}

#[derive(Clone, Copy)]
struct HideProjects;

#[async_trait]
impl ManagementAuthorizationPolicy for HideProjects {
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
fn hidden_project_is_rejected_before_context_or_revision_access() {
  run_ready(async {
    let project_id = ProjectId::generate();
    let configuration_id = FactoryConfigurationId::generate();
    let repository_id = RepositoryId::generate();
    let main = SourceReference::new("refs/heads/main").unwrap();
    let store = seeded_store(
      project_id,
      configuration_id,
      repository(project_id, repository_id, Some(main)),
    );
    let resolver = Arc::new(RecordingResolver::default());
    let handler = AuthorizedCommandHandler::new(
      Arc::new(HideProjects),
      Arc::new(FactoryAdmissionHandlers::new(store.clone(), resolver.clone())),
    );
    let denied = handler
      .handle_command(&context(), command(project_id, configuration_id, repository_id))
      .await
      .unwrap_err();
    assert!(matches!(denied, ManagementHandlerError::Forbidden(_)));
    assert_eq!(resolver.call_count(), 0);
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 0);
  });
}

#[test]
fn work_metadata_is_bounded_and_rejects_sensitive_keys_before_admission() {
  assert!(FactoryMetadata::try_new([("token".to_owned(), "must-not-persist".to_owned())]).is_err());
  assert!(
    FactoryMetadata::try_new(
      (0..=octacity_server_factory::MAX_FACTORY_METADATA_ENTRIES)
        .map(|index| (format!("key-{index}"), "value".to_owned())),
    )
    .is_err()
  );
}

pub(super) fn run_ready<T>(future: impl Future<Output = T>) -> T {
  let waker = Waker::noop();
  let mut context = Context::from_waker(waker);
  let mut future = std::pin::pin!(future);
  match future.as_mut().poll(&mut context) {
    Poll::Ready(value) => value,
    Poll::Pending => panic!("deterministic in-memory future unexpectedly yielded"),
  }
}

struct HideSelectedRepository;
#[async_trait]
impl ManagementAuthorizationPolicy for HideSelectedRepository {
  async fn authorize(
    &self,
    _context: &ManagementRequestContext,
    _action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if resource.kind() == crate::ManagementResourceKind::Repository {
      Err(ManagementAuthorizationDenial::forbidden())
    } else {
      Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
    }
  }
}
#[test]
fn hidden_selected_repository_is_rejected_before_factory_revision_resolution() {
  run_ready(async {
    let project = ProjectId::generate();
    let configuration = FactoryConfigurationId::generate();
    let repository_id = RepositoryId::generate();
    let store = seeded_store(
      project,
      configuration,
      repository(
        project,
        repository_id,
        Some(SourceReference::new("refs/heads/main").unwrap()),
      ),
    );
    let resolver = Arc::new(RecordingResolver::default());
    let policy = Arc::new(HideSelectedRepository);
    let handler = AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(FactoryAdmissionHandlers::with_policy(
        store.clone(),
        resolver.clone(),
        policy,
      )),
    );
    let error = handler
      .handle_command(&context(), command(project, configuration, repository_id))
      .await
      .unwrap_err();
    assert!(matches!(error, ManagementHandlerError::Application(_)));
    assert_eq!(resolver.call_count(), 0);
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 0);
  });
}

#[test]
fn factory_run_detail_cannot_disclose_a_run_owned_by_a_hidden_project() {
  run_ready(async {
    let project = ProjectId::generate();
    let configuration = FactoryConfigurationId::generate();
    let repository_id = RepositoryId::generate();
    let store = seeded_store(
      project,
      configuration,
      repository(project, repository_id, Some("refs/heads/main".parse().unwrap())),
    );
    let admission = admit(
      &FactoryAdmissionHandlers::new(store.clone(), Arc::new(RecordingResolver::default())),
      command(project, configuration, repository_id),
    )
    .await
    .unwrap();
    let policy = Arc::new(HideFactoryOwnerProject);
    let queries = crate::AuthorizedQueryHandler::new(
      policy.clone(),
      Arc::new(crate::FactoryDiscoveryHandlers::with_policy(
        store.clone(),
        policy.clone(),
      )),
    );
    let error = queries
      .handle_query(
        &context(),
        crate::GetFactoryRunQuery {
          run_id: admission.factory_run_id,
        },
      )
      .await
      .unwrap_err();
    assert_eq!(
      error.application().unwrap().classification(),
      ApplicationFailure::NotFound
    );
    for collection in crate::FactoryDiagnosticCollection::ALL {
      let error = queries
        .handle_query(
          &context(),
          crate::ListFactoryDiagnosticsQuery::try_new(admission.factory_run_id, collection, None, 1).unwrap(),
        )
        .await
        .unwrap_err();
      assert_eq!(
        error.application().unwrap().classification(),
        ApplicationFailure::NotFound,
        "{collection:?}"
      );
    }
    let before = store.mutation_evidence_counts().await;
    let operators = crate::AuthorizedCommandHandler::new(
      policy.clone(),
      Arc::new(crate::FactoryOperatorHandlers::new(store.clone(), policy)),
    );
    let error = operators
      .handle_command(
        &context(),
        crate::CancelFactoryRunCommand {
          run_id: admission.factory_run_id,
          expected_version: octacity_server_factory::FactoryRunVersion::INITIAL,
          idempotency_key: IdempotencyKey::new("hidden-run-cancellation").unwrap(),
          requested_at: time(12),
        },
      )
      .await
      .unwrap_err();
    assert_eq!(
      error.application().unwrap().classification(),
      ApplicationFailure::NotFound
    );
    let node = octacity_server_factory::NodeAttemptId::generate();
    let error = operators
      .handle_command(
        &context(),
        crate::RetryFactoryNodeCommand {
          run_id: admission.factory_run_id,
          node_attempt_id: node,
          expected_version: octacity_server_factory::FactoryRunVersion::INITIAL,
          idempotency_key: IdempotencyKey::new("hidden-node-retry").unwrap(),
          requested_at: time(12),
        },
      )
      .await
      .unwrap_err();
    assert_eq!(
      error.application().unwrap().classification(),
      ApplicationFailure::NotFound
    );
    let error = operators
      .handle_command(
        &context(),
        crate::SubmitFactoryNodeResponseCommand {
          run_id: admission.factory_run_id,
          node_attempt_id: node,
          outcome: "unseen".parse().unwrap(),
          value: serde_json::json!(true),
          expected_version: octacity_server_factory::FactoryRunVersion::INITIAL,
          idempotency_key: IdempotencyKey::new("hidden-node-response").unwrap(),
          requested_at: time(12),
        },
      )
      .await
      .unwrap_err();
    assert_eq!(
      error.application().unwrap().classification(),
      ApplicationFailure::NotFound
    );
    let error = operators
      .handle_command(
        &context(),
        crate::ConfirmFactoryNodeOperationCommand {
          run_id: admission.factory_run_id,
          node_attempt_id: node,
          operation: octacity_server_factory::FlowHumanOperation::Promotion,
          disposition: octacity_server_factory::FlowHumanDisposition::Confirm,
          reason: octacity_server_factory::FactoryText::new("unseen request").unwrap(),
          expected_version: octacity_server_factory::FactoryRunVersion::INITIAL,
          idempotency_key: IdempotencyKey::new("hidden-node-request").unwrap(),
          requested_at: time(12),
        },
      )
      .await
      .unwrap_err();
    assert_eq!(
      error.application().unwrap().classification(),
      ApplicationFailure::NotFound
    );
    assert_eq!(
      store.mutation_evidence_counts().await,
      before,
      "hidden parent must reject before mutation or audit"
    );
  });
}

struct HideFactoryOwnerProject;
#[async_trait]
impl ManagementAuthorizationPolicy for HideFactoryOwnerProject {
  async fn authorize(
    &self,
    _context: &ManagementRequestContext,
    _action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    Ok(ManagementAuthorizationGrant::new(
      if resource.kind() == crate::ManagementResourceKind::Project {
        ManagementVisibility::none()
      } else {
        ManagementVisibility::all()
      },
    ))
  }
}

#[test]
fn factory_cycle_collection_preserves_exact_causal_identity_and_rejects_foreign_cursors() {
  run_ready(async {
    let project = ProjectId::generate();
    let configuration = FactoryConfigurationId::generate();
    let repository_id = RepositoryId::generate();
    let store = seeded_store(
      project,
      configuration,
      repository(project, repository_id, Some("refs/heads/main".parse().unwrap())),
    );
    let admission = admit(
      &FactoryAdmissionHandlers::new(store.clone(), Arc::new(RecordingResolver::default())),
      command(project, configuration, repository_id),
    )
    .await
    .unwrap();
    let snapshot = store.factory_run_snapshot(admission.factory_run_id).await.unwrap();
    let queries = crate::AuthorizedQueryHandler::new(
      Arc::new(crate::TrustedNetworkManagementPolicy),
      Arc::new(crate::FactoryDiscoveryHandlers::new(store)),
    );
    let collection = crate::FactoryDiagnosticCollection::WorkflowCycles;
    let query = crate::ListFactoryDiagnosticsQuery::try_new(admission.factory_run_id, collection, None, 1).unwrap();
    let page = queries.handle_query(&context(), query).await.unwrap();
    let nodes = queries
      .handle_query(
        &context(),
        crate::ListFactoryDiagnosticsQuery::try_new(
          admission.factory_run_id,
          crate::FactoryDiagnosticCollection::NodeAttempts,
          None,
          1,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    assert!(nodes.items.is_empty());
    let budgets = queries
      .handle_query(
        &context(),
        crate::ListFactoryDiagnosticsQuery::try_new(
          admission.factory_run_id,
          crate::FactoryDiagnosticCollection::Budgets,
          None,
          1,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(
      serde_json::to_value(&budgets.items[0]).unwrap()["record"]["usage"]["attempts"],
      0
    );
    let builds = queries
      .handle_query(
        &context(),
        crate::ListFactoryDiagnosticsQuery::try_new(
          admission.factory_run_id,
          crate::FactoryDiagnosticCollection::Builds,
          None,
          1,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    assert!(builds.items.is_empty());
    let audit = queries
      .handle_query(
        &context(),
        crate::ListFactoryDiagnosticsQuery::try_new(
          admission.factory_run_id,
          crate::FactoryDiagnosticCollection::Audit,
          None,
          1,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(audit.items.len(), 1);
    assert!(
      serde_json::to_value(&audit.items[0]).unwrap()["record"]
        .get("actor_identity")
        .is_none()
    );
    let json = serde_json::to_value(page).unwrap();
    assert_eq!(json["items"][0]["kind"], "workflow_cycle");
    assert_eq!(
      json["items"][0]["record"]["id"],
      snapshot.admitted_flow.initial_cycle().id().to_string()
    );
    assert_eq!(json["items"][0]["record"]["number"], 1);
    assert!(json["items"][0]["record"].get("input").is_none());
    assert!(
      crate::ListFactoryDiagnosticsQuery::try_new(
        admission.factory_run_id,
        collection,
        Some(&format!(
          "1:{}:flow-runs:{}",
          admission.factory_run_id,
          snapshot.admitted_flow.root_run().id()
        )),
        1
      )
      .is_err()
    );
  });
}
