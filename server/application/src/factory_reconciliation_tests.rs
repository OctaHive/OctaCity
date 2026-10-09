use std::{
  future::{Future, poll_fn},
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
  },
  task::{Context, Poll, Waker},
};

use crate::factory_reconciliation::{FactoryReconciliationClock, transition_for_action};
use crate::{FactoryReconciler, FactoryReconciliationError, FactoryReconciliationShutdown};
use async_trait::async_trait;
use octacity_server_domain::{ArtifactId, BuildConfigurationId, BuildConfigurationVersion, ProjectId, RepositoryId};
use octacity_server_factory::{
  BudgetLimit, BudgetUsage, BuildConfigurationRef, DecisionOutcome, DeliveryPolicyDraft, EvaluationPolicyDraft,
  ExternalWorkIdentity, FactoryArtifactReference, FactoryChoiceKind, FactoryConfiguration,
  FactoryConfigurationChoiceEntries, FactoryConfigurationChoices, FactoryConfigurationDraft, FactoryConfigurationId,
  FactoryConfigurationVersion, FactoryCredentialProfiles, FactoryDigest, FactoryKey, FactoryMetadata,
  FactoryNextAction, FactoryReferenceChoice, FactoryStageDraft, FactoryStageKind, FactoryStageTarget, FactoryWipLimits,
  ImmutableReference, ReworkPolicyDraft, RiskClass, StageAttempt, StageAttemptId, StageAttemptNumber, WorkArtifacts,
  WorkClassification, WorkEnvelope, WorkEnvelopeId, WorkPriority,
};
use octacity_server_store::{
  AuditActor, AuditActorKind, ClaimFactoryOutbox, ClaimFactoryRun, ClaimFactoryRunOutcome, ClaimFactoryRuns,
  ClaimedFactoryOutbox, ClaimedFactoryRun, CommitFactoryRunTransition, CommitFactoryRunTransitionOutcome,
  CreateFactoryConfiguration, FactoryConfigurationAvailability, FactoryConfigurationMutationOutcome,
  FactoryConfigurationStore, FactoryOutboxRecord, FactoryRunDiagnosticPage, FactoryRunSnapshot, FactoryRunStore,
  ListFactoryRunDiagnostics, ManagementMutation, ManagementSecurityScope, MutationAuditContext,
  PublishedFactoryAdmission, PublishedFactoryConfiguration, ReplaceFactoryConfiguration,
  ReplaceFactoryConfigurationError, ReplayFactoryConfigurationMutation, SettleFactoryOutbox, StoreError,
  testing::InMemoryFactoryConfigurationStore,
};

pub(super) fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).expect("fixture key is valid")
}

fn artifact(value: u8) -> FactoryArtifactReference {
  FactoryArtifactReference::new(ArtifactId::generate(), digest(value), 1).unwrap()
}

fn digest(value: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([value; 32])
}

fn exact(identity: &str, value: u8) -> ImmutableReference {
  ImmutableReference::new(key(identity), key("v1"), digest(value))
}

pub(super) fn time(value: i64) -> crate::Timestamp {
  crate::Timestamp::from_unix_millis(value).expect("fixture time is valid")
}

struct FixedClock(crate::Timestamp);

impl FactoryReconciliationClock for FixedClock {
  fn now(&self) -> Result<crate::Timestamp, FactoryReconciliationError> {
    Ok(self.0)
  }
}

pub(super) fn reconciler<S>(
  store: Arc<S>,
  owner: &str,
  batch_size: u16,
  concurrency: u16,
  now: i64,
) -> FactoryReconciler<S>
where
  S: FactoryRunStore + FactoryConfigurationStore + 'static,
{
  FactoryReconciler::with_clock(
    store,
    key(owner),
    batch_size,
    concurrency,
    Arc::new(FixedClock(time(now))),
  )
  .expect("worker bounds are valid")
}

fn configuration(project_id: ProjectId, id: FactoryConfigurationId) -> FactoryConfiguration {
  let choices = FactoryConfigurationChoices::try_new(FactoryConfigurationChoiceEntries {
    references: vec![
      FactoryReferenceChoice {
        kind: FactoryChoiceKind::AdmissionPolicy,
        alias: key("admission"),
        reference: exact("manual", 1),
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
  .expect("fixture choices are valid");
  let budget = BudgetLimit::new(20, 20_000, 20_000, 20_000, 20_000).expect("fixture budget is valid");
  let stage = |name, kind| FactoryStageDraft {
    key: key(name),
    kind,
    build_configuration: key("build"),
    budget,
  };
  FactoryConfiguration::publish(
    id,
    FactoryConfigurationVersion::INITIAL,
    project_id,
    digest(20),
    FactoryConfigurationDraft {
      admission_policy: key("admission"),
      stages: vec![
        stage("implement", FactoryStageKind::Implementation),
        stage("validate", FactoryStageKind::Validation),
        stage("evaluate", FactoryStageKind::Evaluation),
        stage("rework", FactoryStageKind::Rework),
      ],
      wip_limits: FactoryWipLimits::new(10, 10).expect("fixture WIP is valid"),
      hard_budget: budget,
      permission_ceiling: key("permissions"),
      credential_profiles: credential_profiles(),
      decision_signals: Vec::new(),
      evaluation: EvaluationPolicyDraft {
        criterion_packs: vec![key("criteria")],
        evaluators: vec![key("evaluator")],
        required_quorum: 1,
        budget,
      },
      rework: ReworkPolicyDraft {
        max_cycles: 1,
        stage: Some(key("rework")),
        exhausted_outcome: DecisionOutcome::Escalate,
      },
      delivery: DeliveryPolicyDraft {
        adapter: key("delivery-adapter"),
        policy: key("delivery-policy"),
      },
      enabled: true,
    },
    &choices,
  )
  .expect("fixture configuration is valid")
}

#[test]
fn rework_transition_appends_a_new_attempt_without_replacing_prior_attempts() {
  run_to_completion(async {
    let store = seeded_store(1);
    let claimed = store
      .claim_factory_runs(ClaimFactoryRuns::new(key("worker.rework"), time(10), time(20), 1).unwrap())
      .await
      .unwrap()
      .pop()
      .expect("fixture run is claimed");
    let mut snapshot = store.factory_run_snapshot(claimed.run_id).await.unwrap();
    let configuration = store
      .factory_configuration_version(
        snapshot.run.configuration().id(),
        snapshot.run.configuration().version(),
      )
      .await
      .unwrap()
      .configuration;
    let prior = StageAttempt::new(
      StageAttemptId::generate(),
      &snapshot.run,
      StageAttemptNumber::INITIAL,
      FactoryStageTarget::Implementation,
      configuration.stages()[0].budget(),
      digest(60),
      octacity_server_factory::FactoryClaimOwnership::new(claimed.record.owner.clone(), claimed.record.claim),
    );
    snapshot.stage_attempts.push(prior.clone());

    let (_, ready, _, request_append, request_stage) = transition_for_action(
      &snapshot,
      &configuration,
      &claimed,
      &FactoryNextAction::RequestRework,
      digest(61),
      BudgetUsage::default(),
    )
    .expect("rework is requested");
    assert!(request_append.stage_attempts.is_empty());
    assert_eq!(request_stage, None);
    assert_eq!(
      ready,
      octacity_server_factory::FactoryLifecycleProgress::Stage {
        target: FactoryStageTarget::Rework,
        progress: octacity_server_factory::FactoryStageProgress::Ready,
      }
    );

    let (_, _, _, append, stage_id) = transition_for_action(
      &snapshot,
      &configuration,
      &claimed,
      &FactoryNextAction::CreateStageAttempt(FactoryStageTarget::Rework),
      digest(62),
      BudgetUsage::default(),
    )
    .expect("new rework attempt is created");
    let rework = append.stage_attempts.first().expect("one rework attempt");
    assert_eq!(append.stage_attempts.len(), 1);
    assert_eq!(rework.target(), &FactoryStageTarget::Rework);
    assert_eq!(rework.number(), StageAttemptNumber::new(2).unwrap());
    assert_eq!(stage_id, Some(rework.id()));
    assert_eq!(snapshot.stage_attempts, vec![prior]);
  });
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

pub(super) fn seeded_store(run_count: usize) -> Arc<InMemoryFactoryConfigurationStore> {
  let store = Arc::new(InMemoryFactoryConfigurationStore::new());
  let project_id = ProjectId::generate();
  let configuration = configuration(project_id, FactoryConfigurationId::generate());
  store.seed_project(project_id).expect("fixture Project is seeded");
  store
    .seed_factory_configuration_version(PublishedFactoryConfiguration {
      configuration: configuration.clone(),
      published_at: time(1),
    })
    .expect("fixture Factory Configuration is seeded");
  let audit = MutationAuditContext::try_new(
    AuditActor {
      kind: AuditActorKind::UnauthenticatedManagement,
      identity: None,
    },
    ManagementSecurityScope::trusted_network(),
    "factory-reconciliation-fixture",
  )
  .expect("fixture audit is valid");
  let repository_id = RepositoryId::generate();
  for ordinal in 0..run_count {
    let work = WorkEnvelope::new(
      WorkEnvelopeId::generate(),
      configuration.reference().clone(),
      ExternalWorkIdentity::new(format!("manual/work-{ordinal}")).expect("fixture identity is valid"),
      octacity_server_factory::ExactSubject::new(
        project_id,
        repository_id,
        octacity_server_domain::ImmutableRevision::new("exact-revision").expect("fixture revision is valid"),
      ),
      WorkArtifacts::new(artifact(90), artifact(91), Vec::new()).expect("fixture artifacts are valid"),
      WorkClassification::new(
        WorkPriority::new(10).expect("fixture priority is valid"),
        RiskClass::Medium,
        FactoryMetadata::default(),
      ),
    )
    .expect("fixture Work Envelope is valid");
    let run = octacity_server_factory::FactoryRun::admitted(octacity_server_factory::FactoryRunId::generate(), &work);
    let flow = octacity_server_factory::AdmittedFlow::from_stage_projection(&configuration, &run)
      .expect("fixture Flow projection is valid");
    store
      .seed_factory_run(
        PublishedFactoryAdmission {
          work,
          run,
          flow,
          admitted_at: time(1),
        },
        digest(u8::try_from(ordinal + 30).expect("fixture ordinal is bounded")),
        &audit,
      )
      .expect("fixture Factory Run is seeded");
  }
  store
}

#[test]
fn lifecycle_progresses_before_restart_replays_one_external_action() {
  run_to_completion(async {
    let store = seeded_store(1);
    let first = reconciler(store.clone(), "worker.one", 1, 1, 11);
    let running = FactoryReconciliationShutdown::new();
    let committed = first.run_once(time(10), time(20), &running).await.unwrap();
    assert_eq!(committed.claimed, 1);
    assert_eq!(committed.actions_committed, 1);

    let external_action = first.run_once(time(10), time(20), &running).await.unwrap();
    assert_eq!(external_action.actions_committed, 1);
    let replayed = first.run_once(time(10), time(20), &running).await.unwrap();
    assert_eq!(replayed.actions_replayed, 1);

    let replacement = reconciler(store.clone(), "worker.two", 1, 1, 21);
    let recovered = replacement.run_once(time(20), time(30), &running).await.unwrap();
    assert_eq!(recovered.actions_replayed, 1);
    let stale = reconciler(store.clone(), "worker.one", 1, 1, 22);
    let no_stale_reentry = stale.run_once(time(21), time(31), &running).await.unwrap();
    assert_eq!(no_stale_reentry.claimed, 0);

    let claims = store
      .claim_factory_runs(ClaimFactoryRuns::new(key("observer"), time(30), time(40), 1).unwrap())
      .await
      .unwrap();
    let snapshot = store.factory_run_snapshot(claims[0].run_id).await.unwrap();
    assert_eq!(snapshot.claims.len(), 3);
    assert_eq!(snapshot.outbox.len(), 2, "admission plus one stable next action");
    assert!(snapshot.outbox.iter().any(|record| record.kind == key("build.create")));
  });
}

#[test]
fn batch_concurrency_and_graceful_shutdown_are_bounded() {
  run_to_completion(async {
    let measured = Arc::new(MeasuredStore::new(seeded_store(4)));
    let worker = reconciler(measured.clone(), "worker.bounded", 3, 2, 11);
    let outcome = worker
      .run_once(time(10), time(20), &FactoryReconciliationShutdown::new())
      .await
      .unwrap();
    assert_eq!(outcome.claimed, 3);
    assert_eq!(outcome.actions_committed, 3);
    assert_eq!(measured.max_active.load(Ordering::SeqCst), 2);

    let shutdown = Arc::new(FactoryReconciliationShutdown::new());
    let draining = Arc::new(MeasuredStore::with_shutdown(seeded_store(3), Arc::clone(&shutdown)));
    let worker = reconciler(draining, "worker.draining", 3, 1, 11);
    let outcome = worker.run_once(time(10), time(20), shutdown.as_ref()).await.unwrap();
    assert_eq!(outcome.actions_committed, 1);
    assert_eq!(outcome.shutdown_skipped, 2);
  });
}

#[test]
fn invalid_worker_bounds_fail_before_claiming() {
  let store = seeded_store(0);
  assert!(matches!(
    FactoryReconciler::new(store.clone(), key("worker"), 0, 1),
    Err(FactoryReconciliationError::InvalidBounds)
  ));
  assert!(matches!(
    FactoryReconciler::new(store, key("worker"), 2, 3),
    Err(FactoryReconciliationError::InvalidBounds)
  ));
}

struct MeasuredStore {
  inner: Arc<InMemoryFactoryConfigurationStore>,
  active: AtomicUsize,
  max_active: AtomicUsize,
  cancel_once: AtomicBool,
  fail_snapshot_once: AtomicBool,
  committed: AtomicUsize,
  shutdown: Option<Arc<FactoryReconciliationShutdown>>,
}

impl MeasuredStore {
  fn new(inner: Arc<InMemoryFactoryConfigurationStore>) -> Self {
    Self {
      inner,
      active: AtomicUsize::new(0),
      max_active: AtomicUsize::new(0),
      cancel_once: AtomicBool::new(false),
      fail_snapshot_once: AtomicBool::new(false),
      committed: AtomicUsize::new(0),
      shutdown: None,
    }
  }

  fn with_snapshot_failure(inner: Arc<InMemoryFactoryConfigurationStore>) -> Self {
    Self {
      fail_snapshot_once: AtomicBool::new(true),
      ..Self::new(inner)
    }
  }

  fn with_shutdown(
    inner: Arc<InMemoryFactoryConfigurationStore>,
    shutdown: Arc<FactoryReconciliationShutdown>,
  ) -> Self {
    Self {
      inner,
      active: AtomicUsize::new(0),
      max_active: AtomicUsize::new(0),
      cancel_once: AtomicBool::new(true),
      fail_snapshot_once: AtomicBool::new(false),
      committed: AtomicUsize::new(0),
      shutdown: Some(shutdown),
    }
  }
}

#[async_trait]
impl FactoryRunStore for MeasuredStore {
  async fn factory_run_snapshot(
    &self,
    run_id: octacity_server_factory::FactoryRunId,
  ) -> Result<FactoryRunSnapshot, StoreError> {
    let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
    self.max_active.fetch_max(active, Ordering::SeqCst);
    if self.cancel_once.swap(false, Ordering::SeqCst) {
      self.shutdown.as_ref().expect("shutdown token is configured").request();
    }
    let mut yielded = false;
    poll_fn(|context| {
      if yielded {
        Poll::Ready(())
      } else {
        yielded = true;
        context.waker().wake_by_ref();
        Poll::Pending
      }
    })
    .await;
    let result = if self.fail_snapshot_once.swap(false, Ordering::SeqCst) {
      Err(StoreError::Unavailable)
    } else {
      self.inner.factory_run_snapshot(run_id).await
    };
    self.active.fetch_sub(1, Ordering::SeqCst);
    result
  }

  async fn list_factory_run_diagnostics(
    &self,
    request: ListFactoryRunDiagnostics,
  ) -> Result<FactoryRunDiagnosticPage, StoreError> {
    self.inner.list_factory_run_diagnostics(request).await
  }

  async fn claim_factory_run(&self, request: ClaimFactoryRun) -> Result<ClaimFactoryRunOutcome, StoreError> {
    self.inner.claim_factory_run(request).await
  }

  async fn claim_factory_runs(&self, request: ClaimFactoryRuns) -> Result<Vec<ClaimedFactoryRun>, StoreError> {
    self.inner.claim_factory_runs(request).await
  }

  async fn commit_factory_run_transition(
    &self,
    request: CommitFactoryRunTransition,
  ) -> Result<CommitFactoryRunTransitionOutcome, StoreError> {
    let outcome = self.inner.commit_factory_run_transition(request).await?;
    self.committed.fetch_add(1, Ordering::SeqCst);
    Ok(outcome)
  }

  async fn claim_factory_outbox(&self, request: ClaimFactoryOutbox) -> Result<Vec<ClaimedFactoryOutbox>, StoreError> {
    self.inner.claim_factory_outbox(request).await
  }

  async fn settle_factory_outbox(&self, request: SettleFactoryOutbox) -> Result<FactoryOutboxRecord, StoreError> {
    self.inner.settle_factory_outbox(request).await
  }
}

#[test]
fn an_in_flight_transition_finishes_after_a_peer_snapshot_fails() {
  run_to_completion(async {
    let measured = Arc::new(MeasuredStore::with_snapshot_failure(seeded_store(2)));
    let worker = reconciler(measured.clone(), "worker.failure", 2, 2, 11);

    assert!(matches!(
      worker
        .run_once(time(10), time(20), &FactoryReconciliationShutdown::new())
        .await,
      Err(FactoryReconciliationError::Store(StoreError::Unavailable))
    ));
    assert_eq!(measured.committed.load(Ordering::SeqCst), 1);
    assert_eq!(measured.active.load(Ordering::SeqCst), 0);
  });
}

#[async_trait]
impl FactoryConfigurationStore for MeasuredStore {
  async fn replay_factory_configuration_mutation(
    &self,
    request: ManagementMutation<ReplayFactoryConfigurationMutation>,
  ) -> Result<Option<FactoryConfigurationMutationOutcome>, StoreError> {
    self.inner.replay_factory_configuration_mutation(request).await
  }

  async fn factory_configuration_availability(
    &self,
    project_id: ProjectId,
  ) -> Result<FactoryConfigurationAvailability, StoreError> {
    self.inner.factory_configuration_availability(project_id).await
  }

  async fn create_factory_configuration(
    &self,
    request: ManagementMutation<CreateFactoryConfiguration>,
  ) -> Result<FactoryConfigurationMutationOutcome, StoreError> {
    self.inner.create_factory_configuration(request).await
  }

  async fn replace_factory_configuration(
    &self,
    request: ManagementMutation<ReplaceFactoryConfiguration>,
  ) -> Result<FactoryConfigurationMutationOutcome, ReplaceFactoryConfigurationError> {
    self.inner.replace_factory_configuration(request).await
  }

  async fn factory_configuration_version(
    &self,
    id: FactoryConfigurationId,
    version: FactoryConfigurationVersion,
  ) -> Result<PublishedFactoryConfiguration, StoreError> {
    self.inner.factory_configuration_version(id, version).await
  }

  async fn current_factory_configuration(
    &self,
    id: FactoryConfigurationId,
  ) -> Result<PublishedFactoryConfiguration, StoreError> {
    self.inner.current_factory_configuration(id).await
  }
}

pub(super) fn run_to_completion<F: Future>(future: F) -> F::Output {
  let mut future = std::pin::pin!(future);
  let mut context = Context::from_waker(Waker::noop());
  for _ in 0..10_000 {
    if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
      return value;
    }
  }
  panic!("in-memory Factory reconciliation did not settle")
}
