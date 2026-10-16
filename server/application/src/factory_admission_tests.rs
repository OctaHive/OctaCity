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

#[path = "factory_source_admission_tests.rs"]
mod installed_sources;

use crate::{
  AdmitManualFactoryWorkCommand, ApplicationFailure, AuthorizedCommandHandler, FactoryAdmissionHandlers,
  ManagementAction, ManagementAuthorizationDenial, ManagementAuthorizationGrant, ManagementAuthorizationPolicy,
  ManagementCommandUseCase, ManagementHandlerError, ManagementRequestContext, ManagementRequestId, ManagementResource,
  ManagementVisibility, MutationDisposition, RepositorySourceSelection, RevisionResolutionError,
  RevisionResolutionRequest, RevisionResolver,
};

fn artifact(value: u8) -> FactoryArtifactReference {
  FactoryArtifactReference::new(ArtifactId::generate(), digest(value), 1).unwrap()
}

include!("factory_admission_test_fixture.rs");

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

struct UnavailableResolver;

#[async_trait]
impl RevisionResolver for UnavailableResolver {
  async fn resolve(
    &self,
    _request: RevisionResolutionRequest,
  ) -> Result<octacity_server_domain::ImmutableRevision, RevisionResolutionError> {
    Err(RevisionResolutionError::Unavailable)
  }
}

#[test]
fn replay_after_handler_restart_retains_the_original_intent_when_vcs_is_unavailable() {
  run_ready(async {
    let project = ProjectId::generate();
    let configuration = FactoryConfigurationId::generate();
    let repository_id = RepositoryId::generate();
    let store = seeded_store(
      project,
      configuration,
      repository(project, repository_id, Some("refs/heads/main".parse().unwrap())),
    );
    let request = command(project, configuration, repository_id);
    let handlers = FactoryAdmissionHandlers::new(store.clone(), Arc::new(RecordingResolver::default()));
    let applied = admit(&handlers, request.clone()).await.unwrap();
    let before = store.factory_run_snapshot(applied.factory_run_id).await.unwrap();
    let restarted = FactoryAdmissionHandlers::new(store.clone(), Arc::new(UnavailableResolver));

    let replayed = admit(
      &restarted,
      AdmitManualFactoryWorkCommand {
        idempotency_key: IdempotencyKey::new("replay-after-restart").unwrap(),
        admitted_at: time(100),
        ..request.clone()
      },
    )
    .await
    .unwrap();
    assert_eq!(
      replayed,
      crate::FactoryAdmissionOutcome {
        disposition: MutationDisposition::Replayed,
        ..applied.clone()
      }
    );

    let fresh = admit(
      &restarted,
      AdmitManualFactoryWorkCommand {
        external_identity: ExternalWorkIdentity::new("manual/fresh-work").unwrap(),
        idempotency_key: IdempotencyKey::new("fresh-after-restart").unwrap(),
        ..request
      },
    )
    .await
    .unwrap_err();
    assert_eq!(fresh.classification(), ApplicationFailure::Unavailable);
    assert_eq!(
      store.factory_run_snapshot(applied.factory_run_id).await.unwrap(),
      before
    );
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 1);
    assert_eq!(store.management_audit_facts().await.len(), 1);
  });
}

#[test]
fn replacing_admitted_artifacts_conflicts_without_resolving_or_replacing_the_original_work() {
  run_ready(async {
    let project = ProjectId::generate();
    let configuration = FactoryConfigurationId::generate();
    let repository_id = RepositoryId::generate();
    let store = seeded_store(
      project,
      configuration,
      repository(project, repository_id, Some("refs/heads/main".parse().unwrap())),
    );
    let resolver = Arc::new(RecordingResolver::default());
    let handlers = FactoryAdmissionHandlers::new(store.clone(), resolver.clone());
    let request = command(project, configuration, repository_id);
    let applied = admit(&handlers, request.clone()).await.unwrap();
    let before = store.factory_run_snapshot(applied.factory_run_id).await.unwrap();
    let task = request.artifacts.task().clone();
    let acceptance = request.artifacts.acceptance().clone();

    for artifacts in [
      WorkArtifacts::new(artifact(92), acceptance.clone(), vec![]).unwrap(),
      WorkArtifacts::new(task.clone(), artifact(93), vec![]).unwrap(),
      WorkArtifacts::new(task, acceptance, vec![artifact(94)]).unwrap(),
    ] {
      for idempotency_key in [
        request.idempotency_key.clone(),
        IdempotencyKey::new("changed-input-transport-attempt").unwrap(),
      ] {
        let error = admit(
          &handlers,
          AdmitManualFactoryWorkCommand {
            artifacts: artifacts.clone(),
            idempotency_key,
            ..request.clone()
          },
        )
        .await
        .unwrap_err();
        assert_eq!(error.classification(), ApplicationFailure::Conflict);
      }
    }
    assert_eq!(resolver.call_count(), 1);
    assert_eq!(
      store.factory_run_snapshot(applied.factory_run_id).await.unwrap(),
      before
    );
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 1);
    assert_eq!(store.management_audit_facts().await.len(), 1);
    assert_eq!(
      admit(&handlers, request).await.unwrap().disposition,
      MutationDisposition::Replayed
    );
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

struct HideSelectedDependency(ManagementResource);
#[async_trait]
impl ManagementAuthorizationPolicy for HideSelectedDependency {
  async fn authorize(
    &self,
    _context: &ManagementRequestContext,
    _action: ManagementAction,
    resource: &ManagementResource,
  ) -> Result<ManagementAuthorizationGrant, ManagementAuthorizationDenial> {
    if resource == &self.0 {
      Err(ManagementAuthorizationDenial::forbidden())
    } else {
      Ok(ManagementAuthorizationGrant::new(ManagementVisibility::all()))
    }
  }
}
#[test]
fn hidden_selected_dependencies_are_rejected_before_factory_revision_resolution() {
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
    let mut request = command(project, configuration, repository_id);
    request.artifacts = WorkArtifacts::new(
      request.artifacts.task().clone(),
      request.artifacts.acceptance().clone(),
      vec![artifact(92)],
    )
    .unwrap();
    use crate::ManagementResourceKind::{Artifact, FactoryConfiguration, Repository};
    for (kind, identity) in [
      (FactoryConfiguration, configuration.to_string()),
      (Repository, repository_id.to_string()),
      (Artifact, request.artifacts.task().artifact_id().to_string()),
      (Artifact, request.artifacts.acceptance().artifact_id().to_string()),
      (
        Artifact,
        request.artifacts.specifications()[0].artifact_id().to_string(),
      ),
    ] {
      let resource =
        ManagementResource::instance(kind, crate::ManagementResourceIdentity::new(identity).unwrap()).unwrap();
      let policy = Arc::new(HideSelectedDependency(resource));
      let handler = AuthorizedCommandHandler::new(
        policy.clone(),
        Arc::new(FactoryAdmissionHandlers::with_policy(
          store.clone(),
          resolver.clone(),
          policy,
        )),
      );
      let error = handler.handle_command(&context(), request.clone()).await.unwrap_err();
      assert_eq!(
        error.application().unwrap().classification(),
        ApplicationFailure::Invalid
      );
    }
    assert_eq!(resolver.call_count(), 0);
    assert_eq!(store.mutation_evidence_counts().await.idempotency, 0);
    assert!(store.management_audit_facts().await.is_empty());
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
