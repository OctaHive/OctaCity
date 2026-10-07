use std::{
  collections::BTreeMap,
  sync::{Arc, Mutex},
};

use async_trait::async_trait;
use octacity_server_domain::{
  ArtifactId, AttemptId, AttemptNumber, AttemptVersion, BuildId, BuildVersion, PipelineId, PipelineVersion,
  RepositoryVersion, TriggerId, TriggerIdentity, TriggerOccurrenceId, TriggerVersion,
};
use octacity_server_factory::{
  Assessment, AssessmentId, AssessmentOutcome, CandidateSubject, ChangeSet, ChangeSetId, DecisionEngineInput,
  DecisionId, DecisionOutcome, DecisionPolicy, DecisionPolicyDefinition, DecisionPolicyVersion, DeliveryIntent,
  DeterministicGate, DeterministicGateOutcome, EvaluationBranch, EvaluationBranchState, EvaluationPlan,
  EvaluationPlanId, EvaluationProgress, EvaluationState, EvidenceItem, EvidenceManifest, EvidenceManifestId,
  FactoryDigest, FactoryLifecycleProgress, FactoryOutputPermissions, FactoryPermissionDraft, FactoryPermissionSet,
  FactoryResourceLimits, FactoryRun, FactoryRunState, FactoryRunVersion, FactoryStageProgress, FactoryStageTarget,
  FindingSeverity, IndeterminatePolicy, LocalPermissionCeiling, evaluate_decision,
};
use octacity_server_orchestrator::{AttemptState, BuildState};
use octacity_server_store::{
  AttemptRecord, AuditActorKind, BuildQueryStore, BuildRecord, BuildRetentionDeadlines, ClaimFactoryOutbox,
  ClaimFactoryRuns, ClaimedFactoryOutbox, CommitFactoryRunTransition, FactoryAuditFact, FactoryBudgetRecord,
  FactoryLifecycleCheckpoint, FactoryOutboxSettlement, FactoryRunHistoryAppend, FactoryRunStore, ImmutableBuildInput,
  NormalizedTriggerOccurrence, SettleFactoryOutbox, StoreError, TriggerCause, TriggerDefinitionRef, TriggerMetadata,
  TriggerOccurrenceState, TriggerTarget,
};

use crate::factory_build_bridge::FactoryBuildClock;
use crate::{
  CreateFactoryBuild, FactoryAdmissionHandlers, FactoryBuildAcceptance, FactoryBuildBridge, FactoryBuildOutputSource,
  FactoryBuildPolicyLayers, FactoryBuildPolicyRequest, FactoryBuildPolicySource, FactoryBuildPolicySourceError,
  FactoryReconciliationShutdown, MutationDisposition, OrdinaryBuildApplication, OrdinaryBuildApplicationError,
  factory_admission_tests::{RecordingResolver, admit, command, repository, seeded_store, time},
  factory_reconciliation_tests::{key, reconciler, run_to_completion},
};

struct FixedBuildClock;

impl FactoryBuildClock for FixedBuildClock {
  fn now(&self) -> Result<octacity_server_domain::Timestamp, crate::FactoryBuildBridgeError> {
    Ok(time(11))
  }
}

#[derive(Clone)]
struct AcceptedBuild {
  request: CreateFactoryBuild,
  attempt_id: AttemptId,
}

#[derive(Default)]
struct SucceedingBuildApplication {
  builds: Mutex<BTreeMap<BuildId, AcceptedBuild>>,
}

impl SucceedingBuildApplication {
  fn count(&self) -> usize {
    self.builds.lock().unwrap().len()
  }

  fn accepted(&self, build_id: BuildId) -> Result<AcceptedBuild, StoreError> {
    self
      .builds
      .lock()
      .unwrap()
      .get(&build_id)
      .cloned()
      .ok_or(StoreError::Unavailable)
  }
}

#[async_trait]
impl OrdinaryBuildApplication for SucceedingBuildApplication {
  async fn create_factory_build(
    &self,
    request: CreateFactoryBuild,
  ) -> Result<FactoryBuildAcceptance, OrdinaryBuildApplicationError> {
    let build_id = BuildId::generate();
    let attempt_id = AttemptId::generate();
    let job_ids = vec![octacity_server_domain::JobId::generate()];
    self.builds.lock().unwrap().insert(
      build_id,
      AcceptedBuild {
        request: request.clone(),
        attempt_id,
      },
    );
    Ok(FactoryBuildAcceptance {
      build_id,
      attempt_id,
      job_ids,
      effective_policy_digest: request.effective_permissions.digest(),
    })
  }
}

#[async_trait]
impl FactoryBuildOutputSource for SucceedingBuildApplication {
  async fn published_outputs(
    &self,
    _build_id: BuildId,
    _attempt_id: AttemptId,
  ) -> Result<Vec<octacity_server_artifacts::ArtifactIdentity>, OrdinaryBuildApplicationError> {
    Ok(Vec::new())
  }
}

#[async_trait]
impl BuildQueryStore for SucceedingBuildApplication {
  async fn build(&self, build_id: BuildId) -> Result<BuildRecord, StoreError> {
    let accepted = self.accepted(build_id)?;
    let trigger = NormalizedTriggerOccurrence::root(
      TriggerOccurrenceId::generate(),
      TriggerDefinitionRef {
        id: TriggerId::generate(),
        version: TriggerVersion::INITIAL,
      },
      TriggerTarget {
        configuration_id: accepted.request.build_configuration.id(),
        configuration_version: accepted.request.build_configuration.version(),
      },
      TriggerIdentity::new("factory-vertical-slice").unwrap(),
      TriggerCause::Manual {},
      TriggerMetadata::default(),
      time(11),
    )
    .unwrap();
    Ok(BuildRecord {
      build: ImmutableBuildInput {
        id: build_id,
        project_id: accepted.request.project_id,
        configuration_id: accepted.request.build_configuration.id(),
        configuration_version: accepted.request.build_configuration.version(),
        pipeline_id: PipelineId::generate(),
        pipeline_version: PipelineVersion::INITIAL,
        repository_id: accepted.request.repository_id,
        repository_version: RepositoryVersion::INITIAL,
        immutable_revision: accepted.request.immutable_revision,
        input_snapshot: serde_json::json!({}),
        effective_policy_snapshot: serde_json::json!({}),
        retention: BuildRetentionDeadlines {
          metadata: time(1_000),
          logs: time(1_000),
          artifacts: time(1_000),
          reports: time(1_000),
        },
        project_job_concurrency_limit: 1,
        priority: accepted.request.priority,
      },
      state: BuildState::Succeeded,
      version: BuildVersion::INITIAL,
      trigger,
      trigger_state: TriggerOccurrenceState::Accepted,
      trigger_created_at: time(11),
      trigger_updated_at: time(11),
      created_at: time(11),
      updated_at: time(11),
    })
  }

  async fn attempt(&self, attempt_id: AttemptId) -> Result<AttemptRecord, StoreError> {
    let build_id = self
      .builds
      .lock()
      .unwrap()
      .iter()
      .find_map(|(build_id, accepted)| (accepted.attempt_id == attempt_id).then_some(*build_id))
      .ok_or(StoreError::Unavailable)?;
    self.latest_attempt(build_id).await
  }

  async fn job(&self, _job_id: octacity_server_domain::JobId) -> Result<octacity_server_store::JobRecord, StoreError> {
    Err(StoreError::Unavailable)
  }

  async fn latest_attempt(&self, build_id: BuildId) -> Result<AttemptRecord, StoreError> {
    let accepted = self.accepted(build_id)?;
    Ok(AttemptRecord {
      id: accepted.attempt_id,
      build_id,
      number: AttemptNumber::FIRST,
      retry_of_attempt_id: None,
      state: AttemptState::Succeeded,
      version: AttemptVersion::INITIAL,
      created_at: time(11),
      updated_at: time(11),
      jobs: Vec::new(),
    })
  }
}

struct FixedPolicies;

#[async_trait]
impl FactoryBuildPolicySource for FixedPolicies {
  async fn policy_layers(
    &self,
    _request: FactoryBuildPolicyRequest,
  ) -> Result<FactoryBuildPolicyLayers, FactoryBuildPolicySourceError> {
    let permissions = permission_set();
    Ok(FactoryBuildPolicyLayers {
      project: permissions.clone(),
      configuration: permissions.clone(),
      task: permissions.clone(),
      local: LocalPermissionCeiling::fully_enforced(permissions),
    })
  }
}

fn permission_set() -> FactoryPermissionSet {
  FactoryPermissionSet::try_new(FactoryPermissionDraft {
    resources: FactoryResourceLimits::new(100, 100, 100, 10, 100).unwrap(),
    outputs: FactoryOutputPermissions::try_new(Vec::new(), 10, 100, 10, 100).unwrap(),
    ..FactoryPermissionDraft::default()
  })
  .unwrap()
}

#[test]
fn manual_admission_reaches_delivery_approval_without_a_provider_or_delivery_dispatch() {
  run_to_completion(async {
    let project_id = octacity_server_domain::ProjectId::generate();
    let configuration_id = octacity_server_factory::FactoryConfigurationId::generate();
    let repository_id = octacity_server_domain::RepositoryId::generate();
    let main = octacity_server_domain::SourceReference::new("refs/heads/main").unwrap();
    let store = seeded_store(
      project_id,
      configuration_id,
      repository(project_id, repository_id, Some(main)),
    );
    let resolver = Arc::new(RecordingResolver::default());
    let admission = FactoryAdmissionHandlers::new(store.clone(), resolver.clone());
    let admitted = admit(&admission, command(project_id, configuration_id, repository_id))
      .await
      .unwrap();
    let builds = Arc::new(SucceedingBuildApplication::default());
    let bridge = FactoryBuildBridge::new_with_clock(
      store.clone(),
      builds.clone(),
      Arc::new(FixedPolicies),
      Arc::new(FixedBuildClock),
    );
    let shutdown = FactoryReconciliationShutdown::new();
    let worker = reconciler(store.clone(), "factory.slice", 1, 1, 11);

    reconcile(&worker, &shutdown).await;
    reconcile(&worker, &shutdown).await;
    run_build(&store, &bridge).await;

    reconcile(&worker, &shutdown).await;
    let candidate_outbox = claim_outbox(&store, "candidate.capture").await;
    let snapshot = store.factory_run_snapshot(admitted.factory_run_id).await.unwrap();
    let stage = current_stage(&snapshot);
    let candidate = ChangeSet::new(
      ChangeSetId::generate(),
      stage,
      CandidateSubject::new(
        snapshot.run.subject().clone(),
        octacity_server_domain::ImmutableRevision::new("candidate-revision").unwrap(),
        digest(1),
      ),
      ArtifactId::generate(),
      ArtifactId::generate(),
    )
    .unwrap();
    commit_worker_result(
      &store,
      candidate_outbox,
      FactoryRunState::Implementing,
      FactoryLifecycleProgress::Stage {
        target: FactoryStageTarget::Implementation,
        progress: FactoryStageProgress::CandidateCaptured,
      },
      FactoryRunHistoryAppend {
        candidates: vec![candidate.clone()],
        ..FactoryRunHistoryAppend::default()
      },
      |current| current.candidate_id = Some(candidate.id()),
    )
    .await;

    reconcile(&worker, &shutdown).await;
    reconcile(&worker, &shutdown).await;
    run_build(&store, &bridge).await;

    reconcile(&worker, &shutdown).await;
    let evidence_outbox = claim_outbox(&store, "evidence.construct").await;
    let evidence_item = EvidenceItem::new(key("tests"), ArtifactId::generate(), digest(2));
    let evidence = EvidenceManifest::new(
      EvidenceManifestId::generate(),
      &candidate,
      candidate.subject().clone(),
      vec![evidence_item.clone()],
    )
    .unwrap();
    commit_worker_result(
      &store,
      evidence_outbox,
      FactoryRunState::Validating,
      FactoryLifecycleProgress::Stage {
        target: FactoryStageTarget::Validation,
        progress: FactoryStageProgress::EvidenceConstructed,
      },
      FactoryRunHistoryAppend {
        evidence: vec![evidence.clone()],
        ..FactoryRunHistoryAppend::default()
      },
      |current| current.evidence_id = Some(evidence.id()),
    )
    .await;

    reconcile(&worker, &shutdown).await;
    let plan_outbox = claim_outbox(&store, "evaluation.plan").await;
    let plan = EvaluationPlan::new(
      EvaluationPlanId::generate(),
      &evidence,
      evidence.subject().clone(),
      vec![key("quality")],
      vec![key("review")],
    )
    .unwrap();
    let branches = EvaluationProgress::try_new(
      vec![EvaluationBranch::new(
        key("review"),
        true,
        EvaluationBranchState::Pending,
      )],
      1,
    )
    .unwrap();
    commit_worker_result(
      &store,
      plan_outbox,
      FactoryRunState::Evaluating,
      FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(branches)),
      FactoryRunHistoryAppend {
        evaluation_plans: vec![plan.clone()],
        ..FactoryRunHistoryAppend::default()
      },
      |current| current.evaluation_plan_id = Some(plan.id()),
    )
    .await;

    reconcile(&worker, &shutdown).await;
    reconcile(&worker, &shutdown).await;
    run_build(&store, &bridge).await;

    reconcile(&worker, &shutdown).await;
    let decision_outbox = claim_outbox(&store, "decision.compute").await;
    let assessment = Assessment::new(
      AssessmentId::generate(),
      &plan,
      plan.subject().clone(),
      key("review"),
      AssessmentOutcome::Satisfied,
      Vec::new(),
    )
    .unwrap();
    let policy = DecisionPolicy::try_new(
      DecisionPolicyVersion::INITIAL,
      DecisionPolicyDefinition {
        required_evidence: vec![key("tests")],
        required_evaluators: vec![key("review")],
        quorum: 1,
        severity_threshold: FindingSeverity::High,
        indeterminate_policy: IndeterminatePolicy::RequiredOnly,
        failure_outcome: DecisionOutcome::Escalate,
      },
    )
    .unwrap();
    let gates = [DeterministicGate::new(
      key("tests"),
      evidence_item.digest(),
      DeterministicGateOutcome::Passed,
    )];
    let assessments = [assessment.clone()];
    let decision = evaluate_decision(
      DecisionId::generate(),
      DecisionEngineInput::new(&evidence, &plan, &policy, &gates, &assessments),
    )
    .unwrap();
    assert_eq!(decision.outcome(), DecisionOutcome::Accept);
    commit_worker_result(
      &store,
      decision_outbox,
      FactoryRunState::Evaluating,
      FactoryLifecycleProgress::Evaluating(EvaluationState::DecisionRecorded {
        outcome: decision.outcome(),
        completed_rework_cycles: 0,
        max_rework_cycles: 0,
      }),
      FactoryRunHistoryAppend {
        assessments: vec![assessment],
        decisions: vec![decision.clone()],
        ..FactoryRunHistoryAppend::default()
      },
      |current| current.decision_id = Some(decision.id()),
    )
    .await;

    reconcile(&worker, &shutdown).await;
    let waiting = worker.run_once(time(10), time(100), &shutdown).await.unwrap();
    assert_eq!(waiting.waiting, 1);

    let final_snapshot = store.factory_run_snapshot(admitted.factory_run_id).await.unwrap();
    assert_eq!(final_snapshot.run.state(), FactoryRunState::ReadyForDelivery);
    assert!(matches!(
      current_checkpoint(&final_snapshot).progress,
      FactoryLifecycleProgress::ReadyForDelivery(DeliveryIntent::AwaitingApproval)
    ));
    assert_eq!(builds.count(), 3);
    assert_eq!(resolver.call_count(), 1);
    assert!(final_snapshot.signal_requests.is_empty());
    assert!(final_snapshot.signal_receipts.is_empty());
    assert!(final_snapshot.delivery_attempts.is_empty());
    assert!(
      !final_snapshot
        .outbox
        .iter()
        .any(|record| record.kind == key("delivery.request"))
    );
  });
}

async fn reconcile<S>(worker: &crate::FactoryReconciler<S>, shutdown: &FactoryReconciliationShutdown)
where
  S: FactoryRunStore + octacity_server_store::FactoryConfigurationStore + 'static,
{
  let outcome = worker.run_once(time(10), time(100), shutdown).await.unwrap();
  assert_eq!(outcome.actions_committed, 1);
}

async fn run_build(
  store: &Arc<octacity_server_store::testing::InMemoryFactoryConfigurationStore>,
  bridge: &FactoryBuildBridge<
    octacity_server_store::testing::InMemoryFactoryConfigurationStore,
    SucceedingBuildApplication,
    FixedPolicies,
  >,
) {
  let outbox = claim_outbox(store, "build.create").await;
  bridge.dispatch(outbox).await.unwrap();
  let claimed = store
    .claim_factory_runs(ClaimFactoryRuns::new(key("factory.slice"), time(10), time(100), 1).unwrap())
    .await
    .unwrap()
    .pop()
    .unwrap();
  let observation = bridge.observe(&claimed).await.unwrap();
  assert_eq!(observation.state, BuildState::Succeeded);
  assert_eq!(observation.disposition, Some(MutationDisposition::Applied));
}

async fn claim_outbox(
  store: &octacity_server_store::testing::InMemoryFactoryConfigurationStore,
  kind: &str,
) -> ClaimedFactoryOutbox {
  let claimed = store
    .claim_factory_outbox(ClaimFactoryOutbox::new(key("factory.slice"), time(11), time(100), 32).unwrap())
    .await
    .unwrap();
  let mut selected = None;
  for record in claimed {
    if record.record.kind.as_str() == kind {
      selected = Some(record);
    } else {
      settle_outbox(store, &record).await;
    }
  }
  selected.unwrap_or_else(|| panic!("expected pending {kind} operation"))
}

async fn settle_outbox(
  store: &octacity_server_store::testing::InMemoryFactoryConfigurationStore,
  claimed: &ClaimedFactoryOutbox,
) {
  store
    .settle_factory_outbox(SettleFactoryOutbox {
      operation_id: claimed.record.operation_id,
      owner: claimed.record.owner.clone().unwrap(),
      fence: claimed.record.claim.unwrap().fence(),
      observed_at: time(11),
      settlement: FactoryOutboxSettlement::Delivered,
    })
    .await
    .unwrap();
}

async fn commit_worker_result(
  store: &octacity_server_store::testing::InMemoryFactoryConfigurationStore,
  outbox: ClaimedFactoryOutbox,
  state: FactoryRunState,
  progress: FactoryLifecycleProgress,
  append: FactoryRunHistoryAppend,
  update_current: impl FnOnce(&mut octacity_server_store::FactoryRunCurrentProjection),
) {
  let operation_kind = outbox.record.kind.clone();
  let snapshot = store.factory_run_snapshot(outbox.record.run_id).await.unwrap();
  let claim = snapshot.current_claim.clone().unwrap();
  let current_budget = snapshot
    .budgets
    .iter()
    .find(|budget| budget.id == snapshot.current.budget_id)
    .unwrap();
  let checkpoint = current_checkpoint(&snapshot);
  let version = FactoryRunVersion::new(snapshot.run.version().get() + 1).unwrap();
  let budget = FactoryBudgetRecord::new(snapshot.run.id(), version, current_budget.usage, time(11));
  let lifecycle = FactoryLifecycleCheckpoint::new(
    snapshot.run.id(),
    version,
    progress,
    checkpoint.signal,
    checkpoint.cancellation_requested,
    time(11),
  );
  let next_progress = lifecycle.progress.clone();
  let previous_progress = checkpoint.progress.clone();
  let mut current = snapshot.current;
  current.budget_id = budget.id;
  current.lifecycle_checkpoint_id = lifecycle.id;
  update_current(&mut current);
  store
    .commit_factory_run_transition(CommitFactoryRunTransition {
      run_id: snapshot.run.id(),
      expected_version: snapshot.run.version(),
      claim_id: claim.id,
      owner: claim.owner.clone(),
      fence: claim.claim.fence(),
      committed_at: time(11),
      next_run: FactoryRun::restore(
        snapshot.run.id(),
        snapshot.run.configuration().clone(),
        &snapshot.work,
        snapshot.run.subject().clone(),
        state,
        version,
      )
      .unwrap(),
      budget,
      lifecycle_checkpoint: lifecycle,
      append,
      current,
      audit: FactoryAuditFact::new(
        snapshot.run.id(),
        AuditActorKind::Worker,
        Some(digest(9)),
        key("factory.fixture.completed"),
        outbox.record.operation_id,
        key("accepted"),
        time(11),
      ),
      outbox: Vec::new(),
    })
    .await
    .unwrap_or_else(|error| {
      panic!(
        "{operation_kind:?} worker transition failed: {error:?}; previous={previous_progress:?} next={:?}",
        next_progress
      )
    });
  settle_outbox(store, &outbox).await;
}

fn current_stage(snapshot: &octacity_server_store::FactoryRunSnapshot) -> &octacity_server_factory::StageAttempt {
  let id = snapshot.current.stage_attempt_id.unwrap();
  snapshot.stage_attempts.iter().find(|stage| stage.id() == id).unwrap()
}

fn current_checkpoint(
  snapshot: &octacity_server_store::FactoryRunSnapshot,
) -> &octacity_server_store::FactoryLifecycleCheckpoint {
  snapshot
    .lifecycle_checkpoints
    .iter()
    .find(|checkpoint| checkpoint.id == snapshot.current.lifecycle_checkpoint_id)
    .unwrap()
}

fn digest(value: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([value; 32])
}
