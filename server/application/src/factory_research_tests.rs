use crate::*;
use async_trait::async_trait;
use octacity_server_domain::{
  AttemptId, AttemptNumber, AttemptVersion, BuildId, BuildVersion, PipelineId, PipelineVersion, RepositoryVersion,
  TriggerId, TriggerIdentity, TriggerOccurrenceId, TriggerVersion,
};
use octacity_server_factory as factory;
use octacity_server_factory::*;
use octacity_server_orchestrator::{AttemptState, BuildState};
use octacity_server_store::{
  AttemptRecord, BuildQueryStore, BuildRecord, BuildRetentionDeadlines, ImmutableBuildInput,
  NormalizedTriggerOccurrence, StoreError, TriggerCause, TriggerDefinitionRef, TriggerMetadata, TriggerOccurrenceState,
  TriggerTarget,
};
use std::sync::{Arc, Mutex};

#[path = "../../core/octacity-server-factory/src/research/tests/fixtures.rs"]
pub(crate) mod fixtures;
use fixtures::*;

pub(crate) async fn intent_fixture() -> (
  octacity_server_store::FactoryRunSnapshot,
  ResearchPolicy,
  ResearchBuildIntent,
) {
  intent_fixture_for(FlowNodeKind::Reasoning, BudgetUsage::default()).await
}

async fn intent_fixture_for(
  node_kind: FlowNodeKind,
  usage: BudgetUsage,
) -> (
  octacity_server_store::FactoryRunSnapshot,
  ResearchPolicy,
  ResearchBuildIntent,
) {
  intent_fixture_with_budget(node_kind, usage, BudgetLimit::new(1, 1000, 100, 1000, 2500).unwrap()).await
}

async fn intent_fixture_with_budget(
  node_kind: FlowNodeKind,
  usage: BudgetUsage,
  node_budget: BudgetLimit,
) -> (
  octacity_server_store::FactoryRunSnapshot,
  ResearchPolicy,
  ResearchBuildIntent,
) {
  intent_fixture_for_kind(WorkKind::Defect, node_kind, usage, node_budget).await
}

async fn intent_fixture_for_kind(
  kind: WorkKind,
  node_kind: FlowNodeKind,
  usage: BudgetUsage,
  node_budget: BudgetLimit,
) -> (
  octacity_server_store::FactoryRunSnapshot,
  ResearchPolicy,
  ResearchBuildIntent,
) {
  let snapshot = crate::factory_triage_tests::accepted_research_work_for(kind).await;
  let TriageJournalRecord::Classification(receipt) = snapshot.flow.triage.last().unwrap() else {
    panic!("classification required")
  };
  let input = fixtures::input(&snapshot.work, &snapshot.admitted_flow, *receipt.clone(), kind);
  let output_bytes = node_budget.max_output_bytes().max(2500);
  let attempt = BudgetLimit::new(1, 1000, 100, 1000, output_bytes).unwrap();
  let budget = BudgetLimit::new(3, 3000, 300, 3000, 3 * output_bytes).unwrap();
  let closure = research_closure_for_node(kind, &ResearchRoute::ALL, budget, node_budget, node_kind);
  let mut settings = settings(kind);
  settings.budget = budget;
  settings.attempt_budget = attempt;
  if kind == WorkKind::FeatureRequest {
    settings.max_proposal_bytes = 2000;
  }
  let mut profiles = profiles(&closure, input.subject().project_id());
  if node_kind == FlowNodeKind::BuildCommand {
    profiles[0].model_or_tool = reference("reproduction-command");
  }
  let policy = ResearchPolicy::new(input.configuration().clone(), &closure, kind, settings, profiles).unwrap();
  let input = ResearchInput::new(
    &snapshot.work,
    &snapshot.admitted_flow,
    input.accepted_triage().clone(),
    input.context().clone(),
    input.retrieval().to_vec(),
    input.details().clone(),
    policy.digest().unwrap(),
  )
  .unwrap();
  let flow = FlowRun::root(&snapshot.run, closure.closure().root()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let claim = snapshot.current_claim.as_ref().unwrap();
  let node = NodeAttempt::new(
    &flow,
    &cycle,
    closure.closure().definition(flow.definition()).unwrap(),
    NodeAttemptInput {
      id: NodeAttemptId::generate(),
      node_key: key("research"),
      node_kind,
      number: NodeAttemptNumber::new(u64::from(usage.attempts + 1)).unwrap(),
      input_digest: input.digest().unwrap(),
      budget: node_budget,
      deadline: time(99),
      execution: NodeExecutionIdentity::External(reference("research-tool")),
      ownership: FactoryClaimOwnership::new(claim.owner.clone(), claim.claim),
    },
  )
  .unwrap();
  let intent = ResearchBuildIntent::new(input, &policy, flow, node, usage, 7).unwrap();
  (snapshot, policy, intent)
}

pub(crate) struct Builds {
  pub(crate) requests: Mutex<Vec<CreateFactoryBuild>>,
  acceptance: FactoryBuildAcceptance,
  pub(crate) state: Mutex<BuildState>,
  latest: Mutex<Option<AttemptRecord>>,
  pub(crate) lose_response: std::sync::atomic::AtomicBool,
}
impl Builds {
  pub(crate) fn new() -> Self {
    Self {
      requests: Mutex::new(vec![]),
      acceptance: FactoryBuildAcceptance {
        build_id: octacity_server_domain::BuildId::generate(),
        attempt_id: octacity_server_domain::AttemptId::generate(),
        job_ids: vec![octacity_server_domain::JobId::generate()],
        effective_policy_digest: FactoryPermissionSet::deny_all().digest(),
      },
      state: Mutex::new(BuildState::Queued),
      latest: Mutex::new(None),
      lose_response: std::sync::atomic::AtomicBool::new(false),
    }
  }
}
#[async_trait]
impl OrdinaryBuildApplication for Builds {
  async fn create_factory_build(
    &self,
    request: CreateFactoryBuild,
  ) -> Result<FactoryBuildAcceptance, OrdinaryBuildApplicationError> {
    self.requests.lock().unwrap().push(request);
    if self.lose_response.swap(false, std::sync::atomic::Ordering::SeqCst) {
      return Err(OrdinaryBuildApplicationError::Unavailable);
    }
    Ok(self.acceptance.clone())
  }
}

#[async_trait]
impl BuildQueryStore for Builds {
  async fn build(&self, build_id: BuildId) -> Result<BuildRecord, StoreError> {
    let request = self.requests.lock().unwrap()[0].clone();
    let configuration = &request.build_configuration;
    if build_id != self.acceptance.build_id {
      return Err(StoreError::Unavailable);
    }
    let state = *self.state.lock().unwrap();
    let trigger = NormalizedTriggerOccurrence::root(
      TriggerOccurrenceId::generate(),
      TriggerDefinitionRef {
        id: TriggerId::generate(),
        version: TriggerVersion::INITIAL,
      },
      TriggerTarget {
        configuration_id: configuration.id(),
        configuration_version: configuration.version(),
      },
      TriggerIdentity::new("factory-build-fixture").unwrap(),
      TriggerCause::Manual {},
      TriggerMetadata::default(),
      time(30),
    )
    .unwrap();
    Ok(BuildRecord {
      build: ImmutableBuildInput {
        id: build_id,
        project_id: request.project_id,
        configuration_id: configuration.id(),
        configuration_version: configuration.version(),
        pipeline_id: PipelineId::generate(),
        pipeline_version: PipelineVersion::INITIAL,
        repository_id: request.repository_id,
        repository_version: RepositoryVersion::INITIAL,
        immutable_revision: request.immutable_revision,
        input_snapshot: serde_json::json!({}),
        effective_policy_snapshot: serde_json::json!({}),
        retention: BuildRetentionDeadlines {
          metadata: time(1_000),
          logs: time(1_000),
          artifacts: time(1_000),
          reports: time(1_000),
        },
        project_job_concurrency_limit: 1,
        priority: request.priority,
      },
      state,
      version: BuildVersion::INITIAL,
      trigger,
      trigger_state: TriggerOccurrenceState::Accepted,
      trigger_created_at: time(30),
      trigger_updated_at: time(30),
      created_at: time(30),
      updated_at: time(30),
    })
  }

  async fn attempt(&self, _attempt_id: AttemptId) -> Result<AttemptRecord, StoreError> {
    self.latest_attempt(self.acceptance.build_id).await
  }

  async fn job(&self, _job_id: octacity_server_domain::JobId) -> Result<octacity_server_store::JobRecord, StoreError> {
    Err(StoreError::Unavailable)
  }

  async fn latest_attempt(&self, build_id: BuildId) -> Result<AttemptRecord, StoreError> {
    if build_id != self.acceptance.build_id {
      return Err(StoreError::Unavailable);
    }
    if let Some(attempt) = self.latest.lock().unwrap().as_ref() {
      return Ok(attempt.clone());
    }
    let state = match *self.state.lock().unwrap() {
      BuildState::Queued => AttemptState::Created,
      BuildState::Running => AttemptState::Running,
      BuildState::Succeeded => AttemptState::Succeeded,
      BuildState::Failed => AttemptState::Failed,
      BuildState::Cancelled => AttemptState::Cancelled,
    };
    Ok(AttemptRecord {
      id: self.acceptance.attempt_id,
      build_id,
      number: AttemptNumber::FIRST,
      retry_of_attempt_id: None,
      state,
      version: AttemptVersion::INITIAL,
      created_at: time(30),
      updated_at: time(30),
      jobs: Vec::new(),
    })
  }
}

pub(crate) struct Outputs(pub(crate) Option<FactoryResearchBuildOutputs>);
#[async_trait]
impl FactoryResearchOutputSource for Outputs {
  async fn published_outputs(
    &self,
    build: octacity_server_domain::BuildId,
    attempt: octacity_server_domain::AttemptId,
    _: u64,
  ) -> Result<FactoryResearchBuildOutputs, OrdinaryBuildApplicationError> {
    let outputs = self.0.clone().expect("pending Build cannot publish research");
    assert!(
      outputs
        .documents
        .iter()
        .all(|doc| doc.record.identity().build_id == build && doc.record.identity().attempt_id == attempt)
    );
    Ok(outputs)
  }
}

#[test]
fn research_build_priority_is_persisted_with_the_operation_and_survives_restart() {
  crate::factory_admission_tests::run_ready(async {
    let (snapshot, policy, intent) = intent_fixture().await;
    let bytes = serde_json::to_vec(&intent).unwrap();
    let frozen: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(frozen["priority"], serde_json::json!(7));
    let restored = ResearchBuildIntent::restore(
      &bytes,
      &snapshot.work,
      &snapshot.admitted_flow,
      &policy,
      intent.flow(),
      intent.node(),
    )
    .unwrap();
    let builds = Arc::new(Builds::new());
    let adapter = FactoryResearchBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)));
    adapter
      .observe_or_dispatch(&intent, &policy, intent_ownership(&intent), time(30))
      .await
      .unwrap();
    adapter
      .observe_or_dispatch(&restored, &policy, intent_ownership(&restored), time(31))
      .await
      .unwrap();
    let requests = builds.requests.lock().unwrap();
    assert_eq!(requests[0], requests[1]);
    assert_eq!(requests[1].priority, 7);
  });
}

#[test]
fn research_rejects_a_build_with_a_different_frozen_priority() {
  crate::factory_admission_tests::run_ready(async {
    let (_, policy, intent) = intent_fixture().await;
    let other = ResearchBuildIntent::new(
      intent.input().clone(),
      &policy,
      intent.flow().clone(),
      intent.node().clone(),
      intent.usage_before(),
      8,
    )
    .unwrap();
    assert_ne!(intent.operation_id().unwrap(), other.operation_id().unwrap());
    let builds = Arc::new(Builds::new());
    let adapter = FactoryResearchBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)));
    adapter
      .observe_or_dispatch(&intent, &policy, intent_ownership(&intent), time(30))
      .await
      .unwrap();
    // This public Build port incorrectly reuses the first immutable Build for the other operation.
    assert!(
      adapter
        .observe_or_dispatch(&other, &policy, intent_ownership(&other), time(31))
        .await
        .is_err()
    );
    assert_eq!(builds.requests.lock().unwrap()[1].priority, 8);
  });
}

#[test]
fn defect_research_dispatches_and_observes_the_same_ordinary_build_after_restart() {
  crate::factory_admission_tests::run_ready(async {
    let (snapshot, policy, intent) = intent_fixture().await;
    let builds = Arc::new(Builds::new());
    let executor = FactoryResearchBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)));
    assert!(matches!(
      executor
        .observe_or_dispatch(&intent, &policy, intent_ownership(&intent), time(30))
        .await
        .unwrap(),
      FactoryResearchStep::Waiting
    ));
    let bytes = serde_json::to_vec(&intent).unwrap();
    let restored = ResearchBuildIntent::restore(
      &bytes,
      &snapshot.work,
      &snapshot.admitted_flow,
      &policy,
      intent.flow(),
      intent.node(),
    )
    .unwrap();
    let restarted = FactoryResearchBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)));
    assert!(matches!(
      restarted
        .observe_or_dispatch(&restored, &policy, intent_ownership(&restored), time(31))
        .await
        .unwrap(),
      FactoryResearchStep::Waiting
    ));
    let requests = builds.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0], requests[1]);
    assert_eq!(requests[0].project_id, snapshot.work.subject().project_id());
    assert_eq!(requests[0].build_configuration, intent.profile().build_configuration);
    assert_eq!(requests[0].priority, 7);
    assert_eq!(requests[0].deadline, intent.node().deadline());
    let node = requests[0].causality.node().expect("generic node causality");
    assert_eq!(node.node_attempt_id, intent.node().id());
    assert_eq!(node.context, *intent.input().context());
    assert_eq!(node.profile.model_or_tool, reference("research-model"));
    let dispatched = ResearchInput::restore(
      &node.input,
      &snapshot.work,
      &snapshot.admitted_flow,
      policy.digest().unwrap(),
    )
    .unwrap();
    assert_eq!(dispatched, *intent.input());
  });
}

pub(crate) fn intent_ownership(intent: &ResearchBuildIntent) -> FactoryClaimOwnership {
  FactoryClaimOwnership::new(intent.node().owner().clone(), intent.node().claim())
}

fn document(
  builds: &Builds,
  name: &FactoryKey,
  schema: &ImmutableReference,
  bytes: Vec<u8>,
  tool: ImmutableReference,
) -> FactoryResearchOutputDocument {
  use octacity_server_artifacts::*;
  let (attempt_id, job_id) = {
    let latest = builds.latest.lock().unwrap();
    latest.as_ref().map_or(
      (builds.acceptance.attempt_id, builds.acceptance.job_ids[0]),
      |attempt| (attempt.id, attempt.jobs[0].id()),
    )
  };
  let identity = ArtifactIdentity {
    artifact_id: octacity_server_domain::ArtifactId::generate(),
    build_id: builds.acceptance.build_id,
    attempt_id,
    job_id,
    lease_id: octacity_server_domain::LeaseId::generate(),
    logical_name: octacity_server_domain::ArtifactName::new(name.as_str()).unwrap(),
    artifact_type: ArtifactType::Report(ArtifactReportFormat::new(schema.identity().as_str()).unwrap()),
    media_type: ArtifactMediaType::new("application/json").unwrap(),
    size_bytes: bytes.len() as u64,
    digest: ArtifactContentDigest::from_bytes(FactoryDigest::content_sha256(&bytes).as_bytes()),
    retention: ArtifactRetentionPolicy::DeleteAfter(time(1000)),
  };
  let record = ArtifactRecord::pending(identity.clone(), time(29)).unwrap();
  let record = record
    .transition(&identity, record.version(), ArtifactEvent::BeginVerification, time(29))
    .unwrap();
  let record = record
    .transition(&identity, record.version(), ArtifactEvent::Publish, time(30))
    .unwrap();
  FactoryResearchOutputDocument {
    record,
    bytes,
    schema: schema.clone(),
    tool,
    plugin: reference("runner"),
  }
}

fn reproduced_outputs(
  intent: &ResearchBuildIntent,
  policy: &ResearchPolicy,
  builds: &Builds,
) -> FactoryResearchBuildOutputs {
  let ResearchDetails::Defect { environment, .. } = intent.input().details() else {
    panic!("defect expected")
  };
  observed_outputs(
    intent,
    policy,
    builds,
    vec![DefectReproductionCheck {
      environment_digest: environment.content_digest(),
      observation: DefectReproductionObservation::Reproduced,
    }],
    DefectResearchOutcome::Reproduced,
  )
}

pub(crate) fn observed_outputs(
  intent: &ResearchBuildIntent,
  policy: &ResearchPolicy,
  builds: &Builds,
  checks: Vec<DefectReproductionCheck>,
  outcome: DefectResearchOutcome,
) -> FactoryResearchBuildOutputs {
  let ResearchDetails::Defect { environment, .. } = intent.input().details() else {
    panic!("defect expected")
  };
  let report = DefectReproductionReport::new(
    intent.input().subject().clone(),
    intent.input().digest().unwrap(),
    environment.content_digest(),
    checks,
  )
  .unwrap();
  let requirement = &policy.settings().evidence[&ResearchEvidenceKind::Reproduction];
  let report = document(
    builds,
    requirement.kind(),
    requirement.schema(),
    serde_json::to_vec(&report).unwrap(),
    requirement.tool().clone(),
  );
  let identity = report.record.identity();
  let observations = DefectResearchResult {
    outcome,
    reproduction_report: FactoryArtifactReference::new(
      identity.artifact_id,
      FactoryDigest::from_bytes(identity.digest.as_bytes()),
      identity.size_bytes,
    )
    .unwrap(),
    summary: BoundedSummary::new(
      FactoryTaskSubject::Exact(intent.input().subject().clone()),
      FactorySafeText::new("Declared regression reproduced").unwrap(),
      digest(80),
    ),
    unresolved_items: if outcome == DefectResearchOutcome::NeedsHumanInput {
      vec![FactorySafeText::new("Provide the required environment inputs").unwrap()]
    } else {
      vec![]
    },
  };
  let result = document(
    builds,
    &intent.profile().result_output,
    &ResearchSchema::DefectResult.reference().unwrap(),
    serde_json::to_vec(&observations).unwrap(),
    intent.profile().tool.clone(),
  );
  FactoryResearchBuildOutputs {
    usage: BudgetUsage {
      attempts: 1,
      elapsed_millis: 10,
      tokens: 10,
      cost_micro_units: 10,
      output_bytes: report.record.identity().size_bytes + result.record.identity().size_bytes,
    },
    documents: vec![report, result],
    verified_at: time(30),
    fresh_until: time(80),
  }
}

#[test]
fn successful_research_requires_the_deterministic_report_and_returns_a_fenced_generic_completion() {
  crate::factory_admission_tests::run_ready(async {
    let (_, policy, intent) = intent_fixture().await;
    let builds = Arc::new(Builds::new());
    *builds.state.lock().unwrap() = BuildState::Succeeded;
    let outputs = reproduced_outputs(&intent, &policy, &builds);
    let executor = FactoryResearchBuildAdapter::new(builds.clone(), Arc::new(Outputs(Some(outputs))));
    let FactoryResearchStep::Completed(done) = executor
      .observe_or_dispatch(&intent, &policy, intent_ownership(&intent), time(31))
      .await
      .unwrap()
    else {
      panic!("research completion expected")
    };
    assert_eq!(
      done.result.outcome(),
      ResearchOutcome::Defect(DefectResearchOutcome::Reproduced)
    );
    assert_eq!(
      done.evidence.record().fact,
      ResearchEvidenceFact::Reproduction(DefectResearchOutcome::Reproduced)
    );
    // Admission risk remains authoritative even when classification reports low risk.
    assert_eq!(done.acceptance.decision.route, ResearchRoute::Requirements);
    assert_eq!(done.completion.node_attempt_id(), intent.node().id());
    assert_eq!(done.completion.output_digest(), done.result.digest().unwrap());
    assert_eq!(done.result.provenance().producer.build_id(), builds.acceptance.build_id);
    assert_eq!(
      done.result.provenance().context_digest,
      intent.input().context().digest().unwrap()
    );
  });
}

#[test]
fn failed_or_cancelled_ordinary_build_is_not_a_negative_reproduction() {
  crate::factory_admission_tests::run_ready(async {
    for kind in [WorkKind::Defect, WorkKind::FeatureRequest] {
      let (_, policy, intent) = intent_fixture_for_kind(
        kind,
        FlowNodeKind::Reasoning,
        BudgetUsage::default(),
        BudgetLimit::new(1, 1000, 100, 1000, 2500).unwrap(),
      )
      .await;
      for state in [BuildState::Failed, BuildState::Cancelled] {
        let builds = Arc::new(Builds::new());
        *builds.state.lock().unwrap() = state;
        let executor = FactoryResearchBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)));
        let FactoryResearchStep::ExecutionStopped {
          build_id,
          attempt_id,
          state: actual,
        } = executor
          .observe_or_dispatch(&intent, &policy, intent_ownership(&intent), time(31))
          .await
          .unwrap()
        else {
          panic!("execution failure must remain separate")
        };
        assert_eq!(actual, state);
        assert_eq!(build_id, builds.acceptance.build_id);
        assert_eq!(attempt_id, builds.acceptance.attempt_id);
      }
    }
  });
}

#[test]
fn research_rejects_a_foreign_live_claim_before_dispatch() {
  crate::factory_admission_tests::run_ready(async {
    let (_, policy, intent) = intent_fixture().await;
    let builds = Arc::new(Builds::new());
    let adapter = FactoryResearchBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)));
    let foreign = FactoryClaimOwnership::new(
      key("foreign.worker"),
      FactoryClaim::new(FactoryClaimFence::new(digest(99)), time(25), time(90)).unwrap(),
    );
    assert!(
      adapter
        .observe_or_dispatch(&intent, &policy, foreign, time(30))
        .await
        .is_err()
    );
    assert!(builds.requests.lock().unwrap().is_empty());
  });
}

#[test]
fn research_counts_external_builds_independently_of_intervening_flow_nodes() {
  crate::factory_admission_tests::run_ready(async {
    let (_, policy, intent) = intent_fixture().await;
    // The owner numbered Build #1, a deterministic gate #2, then this Build #3.
    // Only the first external execution is charged to the Research budget.
    let node = NodeAttempt::new(
      intent.flow(),
      &WorkflowCycle::initial(intent.flow()).unwrap(),
      policy.closure().definition(intent.flow().definition()).unwrap(),
      NodeAttemptInput {
        id: NodeAttemptId::generate(),
        node_key: intent.node().node_key().clone(),
        node_kind: intent.node().node_kind(),
        number: NodeAttemptNumber::new(3).unwrap(),
        input_digest: intent.node().input_digest(),
        budget: intent.node().budget(),
        deadline: intent.node().deadline(),
        execution: intent.node().execution().clone(),
        ownership: intent_ownership(&intent),
      },
    )
    .unwrap();
    let intent = ResearchBuildIntent::new(
      intent.input().clone(),
      &policy,
      intent.flow().clone(),
      node,
      BudgetUsage {
        attempts: 1,
        ..Default::default()
      },
      7,
    )
    .unwrap();
    let builds = Arc::new(Builds::new());
    *builds.state.lock().unwrap() = BuildState::Succeeded;
    let outputs = reproduced_outputs(&intent, &policy, &builds);
    let adapter = FactoryResearchBuildAdapter::new(builds, Arc::new(Outputs(Some(outputs))));
    let FactoryResearchStep::Completed(done) = adapter
      .observe_or_dispatch(&intent, &policy, intent_ownership(&intent), time(31))
      .await
      .unwrap()
    else {
      panic!("second external Build must complete")
    };
    assert_eq!(done.result.provenance().attempt.get(), 3);
    assert_eq!(
      serde_json::to_value(done.result.provenance()).unwrap()["execution_attempt"],
      serde_json::json!(2)
    );
    assert_eq!(done.usage.attempts, 2);
    assert_eq!(done.acceptance.decision.route, ResearchRoute::Requirements);
  });
}

#[test]
fn research_cannot_dispatch_after_external_execution_budget_is_exhausted() {
  crate::factory_admission_tests::run_ready(async {
    let (_, policy, intent) = intent_fixture().await;
    let mut node = serde_json::to_value(intent.node()).unwrap();
    node["number"] = serde_json::json!(3);
    let node = serde_json::from_value(node).unwrap();
    assert!(
      ResearchBuildIntent::new(
        intent.input().clone(),
        &policy,
        intent.flow().clone(),
        node,
        BudgetUsage {
          attempts: 3,
          ..Default::default()
        },
        7
      )
      .is_err()
    );
  });
}

#[test]
fn research_rejects_outputs_produced_after_the_attempt_deadline() {
  crate::factory_admission_tests::run_ready(async {
    let (_, policy, intent) = intent_fixture().await;
    let mut node = serde_json::to_value(intent.node()).unwrap();
    node["deadline"] = serde_json::json!(29);
    let node = serde_json::from_value(node).unwrap();
    let intent = ResearchBuildIntent::new(
      intent.input().clone(),
      &policy,
      intent.flow().clone(),
      node,
      BudgetUsage::default(),
      7,
    )
    .unwrap();
    let builds = Arc::new(Builds::new());
    *builds.state.lock().unwrap() = BuildState::Succeeded;
    let outputs = reproduced_outputs(&intent, &policy, &builds);
    let adapter = FactoryResearchBuildAdapter::new(builds, Arc::new(Outputs(Some(outputs))));
    assert!(
      adapter
        .observe_or_dispatch(&intent, &policy, intent_ownership(&intent), time(31))
        .await
        .is_err()
    );
  });
}

fn retry_intent(
  intent: &ResearchBuildIntent,
  policy: &ResearchPolicy,
  usage: BudgetUsage,
) -> Result<ResearchBuildIntent, FactoryError> {
  let node = NodeAttempt::new(
    intent.flow(),
    &WorkflowCycle::initial(intent.flow())?,
    policy.closure().definition(intent.flow().definition()).unwrap(),
    NodeAttemptInput {
      id: NodeAttemptId::generate(),
      node_key: intent.node().node_key().clone(),
      node_kind: intent.node().node_kind(),
      number: NodeAttemptNumber::new(u64::from(usage.attempts + 1))?,
      input_digest: intent.node().input_digest(),
      budget: intent.node().budget(),
      deadline: intent.node().deadline(),
      execution: intent.node().execution().clone(),
      ownership: intent_ownership(intent),
    },
  )?;
  ResearchBuildIntent::new(intent.input().clone(), policy, intent.flow().clone(), node, usage, 7)
}

#[test]
fn all_defect_observations_survive_ordinary_retries_restart_exhaustion_and_owner_takeover() {
  crate::factory_admission_tests::run_ready(async {
    use DefectReproductionObservation::{MissingInput, NotReproduced, Reproduced};
    for (expected, checks) in [
      (DefectResearchOutcome::Reproduced, vec![(false, Reproduced)]),
      (
        DefectResearchOutcome::Intermittent,
        vec![(false, Reproduced), (false, NotReproduced)],
      ),
      (
        DefectResearchOutcome::EnvironmentSpecific,
        vec![(false, NotReproduced), (true, Reproduced)],
      ),
      (DefectResearchOutcome::CannotReproduce, vec![(false, NotReproduced)]),
      (DefectResearchOutcome::NeedsHumanInput, vec![(false, MissingInput)]),
    ] {
      let (snapshot, policy, mut intent) = intent_fixture().await;
      let mut operations = std::collections::BTreeSet::new();
      for number in 1..=3 {
        assert!(operations.insert(intent.operation_id().unwrap()));
        assert_eq!(intent.node().number().get(), number);
        let builds = Arc::new(Builds::new());
        *builds.state.lock().unwrap() = BuildState::Succeeded;
        let ResearchDetails::Defect { environment, .. } = intent.input().details() else {
          panic!("defect expected")
        };
        let checks = checks
          .iter()
          .map(|(alternate, observation)| DefectReproductionCheck {
            environment_digest: if *alternate {
              digest(99)
            } else {
              environment.content_digest()
            },
            observation: *observation,
          })
          .collect();
        let mut outputs = observed_outputs(&intent, &policy, &builds, checks, expected);
        outputs.fresh_until = time(800);
        let adapter = FactoryResearchBuildAdapter::new(builds.clone(), Arc::new(Outputs(Some(outputs.clone()))));
        let FactoryResearchStep::Completed(done) = adapter
          .observe_or_dispatch(&intent, &policy, intent_ownership(&intent), time(31))
          .await
          .unwrap()
        else {
          panic!("typed completion expected")
        };
        assert_eq!(done.result.outcome(), ResearchOutcome::Defect(expected));
        assert_eq!(
          done.evidence.record().fact,
          ResearchEvidenceFact::Reproduction(expected)
        );
        assert_eq!(done.usage.attempts, number as u32);
        assert_eq!(done.result.provenance().producer.build_id(), builds.acceptance.build_id);
        let expected_route = match expected {
          DefectResearchOutcome::Reproduced | DefectResearchOutcome::Intermittent => ResearchRoute::Requirements,
          DefectResearchOutcome::EnvironmentSpecific | DefectResearchOutcome::NeedsHumanInput => {
            ResearchRoute::Escalation
          }
          DefectResearchOutcome::CannotReproduce if number == 3 => ResearchRoute::Escalation,
          DefectResearchOutcome::CannotReproduce => ResearchRoute::Rejection,
        };
        assert_eq!(done.acceptance.decision.route, expected_route);
        if expected == DefectResearchOutcome::CannotReproduce && number == 3 {
          assert_eq!(done.acceptance.decision.reason, ResearchReason::AttemptsExhausted);
        }
        if expected == DefectResearchOutcome::NeedsHumanInput {
          assert_eq!(done.acceptance.decision.reason, ResearchReason::HumanInputRequired);
        }
        let bytes = serde_json::to_vec(&intent).unwrap();
        let restored = ResearchBuildIntent::restore(
          &bytes,
          &snapshot.work,
          &snapshot.admitted_flow,
          &policy,
          intent.flow(),
          intent.node(),
        )
        .unwrap();
        let takeover = FactoryClaimOwnership::new(
          key("replacement.worker"),
          FactoryClaim::new(FactoryClaimFence::new(digest(91)), time(100), time(200)).unwrap(),
        );
        let restarted = FactoryResearchBuildAdapter::new(builds.clone(), Arc::new(Outputs(Some(outputs))));
        let FactoryResearchStep::Completed(replayed) = restarted
          .observe_or_dispatch(&restored, &policy, takeover.clone(), time(101))
          .await
          .unwrap()
        else {
          panic!("restored completion expected")
        };
        assert_eq!(done.result, replayed.result);
        assert_eq!(done.evidence, replayed.evidence);
        assert_eq!(done.acceptance, replayed.acceptance);
        assert_eq!(replayed.completion.claim(), takeover.claim());
        assert_eq!(replayed.completion.node_attempt_id(), done.completion.node_attempt_id());
        let persisted: ResearchResult = serde_json::from_slice(&serde_json::to_vec(&done.result).unwrap()).unwrap();
        assert_eq!(persisted, done.result);
        let requests = builds.requests.lock().unwrap();
        assert_eq!(requests[0], requests[1]);
        drop(requests);
        if number < 3 {
          intent = retry_intent(&intent, &policy, done.usage).unwrap();
        } else {
          assert!(matches!(
            policy
              .next_attempt(intent.input(), done.usage, intent.permissions())
              .unwrap(),
            ResearchAttemptDisposition::Exhausted(ResearchRoute::Escalation)
          ));
          assert!(retry_intent(&intent, &policy, done.usage).is_err());
        }
      }
    }
  });
}

#[test]
fn deterministic_command_profiles_use_the_same_build_boundary() {
  crate::factory_admission_tests::run_ready(async {
    let (_, policy, intent) = intent_fixture_for(FlowNodeKind::BuildCommand, BudgetUsage::default()).await;
    let builds = Arc::new(Builds::new());
    *builds.state.lock().unwrap() = BuildState::Succeeded;
    let outputs = reproduced_outputs(&intent, &policy, &builds);
    let adapter = FactoryResearchBuildAdapter::new(builds.clone(), Arc::new(Outputs(Some(outputs))));
    let FactoryResearchStep::Completed(done) = adapter
      .observe_or_dispatch(&intent, &policy, intent_ownership(&intent), time(31))
      .await
      .unwrap()
    else {
      panic!("typed completion expected")
    };
    assert_eq!(
      done.result.provenance().model_or_tool,
      reference("reproduction-command")
    );
    let requests = builds.requests.lock().unwrap();
    assert_eq!(
      requests[0].causality.node().unwrap().profile.model_or_tool,
      reference("reproduction-command")
    );
  });
}

#[test]
fn successful_build_cannot_substitute_forged_missing_or_unretained_research_outputs() {
  crate::factory_admission_tests::run_ready(async {
    let (_, policy, intent) = intent_fixture().await;
    for corruption in 0..5 {
      let builds = Arc::new(Builds::new());
      *builds.state.lock().unwrap() = BuildState::Succeeded;
      let mut outputs = reproduced_outputs(&intent, &policy, &builds);
      match corruption {
        0 => outputs.documents[0].bytes[0] = b'!',
        1 => {
          outputs.documents.remove(0);
        }
        2 => outputs.documents[0].tool = reference("invented-validator"),
        3 => outputs.fresh_until = time(1001),
        _ => {
          let ResearchDetails::Defect { environment, .. } = intent.input().details() else {
            panic!("defect expected")
          };
          outputs = observed_outputs(
            &intent,
            &policy,
            &builds,
            vec![DefectReproductionCheck {
              environment_digest: environment.content_digest(),
              observation: DefectReproductionObservation::NotReproduced,
            }],
            DefectResearchOutcome::Reproduced,
          );
        }
      }
      let adapter = FactoryResearchBuildAdapter::new(builds, Arc::new(Outputs(Some(outputs))));
      assert!(
        adapter
          .observe_or_dispatch(&intent, &policy, intent_ownership(&intent), time(31))
          .await
          .is_err(),
        "corruption {corruption}"
      );
    }
  });
}

#[test]
fn an_unknown_build_acceptance_response_replays_the_same_operation() {
  crate::factory_admission_tests::run_ready(async {
    let (_, policy, intent) = intent_fixture().await;
    let builds = Arc::new(Builds::new());
    builds.lose_response.store(true, std::sync::atomic::Ordering::SeqCst);
    let adapter = FactoryResearchBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)));
    let error = adapter
      .observe_or_dispatch(&intent, &policy, intent_ownership(&intent), time(30))
      .await
      .unwrap_err();
    assert_eq!(error.classification(), ApplicationFailure::Unavailable);
    assert_eq!(
      adapter
        .observe_or_dispatch(&intent, &policy, intent_ownership(&intent), time(31))
        .await
        .unwrap(),
      FactoryResearchStep::Waiting
    );
    let requests = builds.requests.lock().unwrap();
    assert_eq!(requests[0], requests[1]);
  });
}

#[test]
fn ordinary_build_retry_uses_the_latest_attempt_and_its_actual_jobs() {
  crate::factory_admission_tests::run_ready(async {
    for kind in [WorkKind::Defect, WorkKind::FeatureRequest] {
      let (_, policy, intent) = intent_fixture_for_kind(
        kind,
        FlowNodeKind::Reasoning,
        BudgetUsage::default(),
        BudgetLimit::new(1, 1000, 100, 1000, 2500).unwrap(),
      )
      .await;
      let builds = Arc::new(Builds::new());
      *builds.state.lock().unwrap() = BuildState::Succeeded;
      let (attempt_id, job_id) = complete_ordinary_retry(&builds);
      let outputs = if kind == WorkKind::Defect {
        reproduced_outputs(&intent, &policy, &builds)
      } else {
        feature_outputs(&intent, &policy, &builds)
      };
      let adapter = FactoryResearchBuildAdapter::new(builds.clone(), Arc::new(Outputs(Some(outputs))));
      let FactoryResearchStep::Completed(done) = adapter
        .observe_or_dispatch(&intent, &policy, intent_ownership(&intent), time(31))
        .await
        .unwrap()
      else {
        panic!("latest ordinary Attempt must complete research")
      };
      assert_eq!(done.result.provenance().producer.attempt_id(), attempt_id);
      assert_eq!(done.result.provenance().producer.job_id(), job_id);
      assert_eq!(
        done.usage.attempts, 1,
        "ordinary retries stay inside one bounded Node Attempt"
      );
    }
  });
}

#[test]
fn research_build_keeps_a_nodes_narrower_budget_than_the_policy_reservation() {
  crate::factory_admission_tests::run_ready(async {
    let narrow = BudgetLimit::new(1, 500, 50, 500, 2000).unwrap();
    let (_, policy, intent) = intent_fixture_with_budget(FlowNodeKind::Reasoning, BudgetUsage::default(), narrow).await;
    assert_eq!(intent.budget(), narrow);
    let builds = Arc::new(Builds::new());
    let adapter = FactoryResearchBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)));
    assert_eq!(
      adapter
        .observe_or_dispatch(&intent, &policy, intent_ownership(&intent), time(30))
        .await
        .unwrap(),
      FactoryResearchStep::Waiting
    );
    assert_eq!(builds.requests.lock().unwrap()[0].budget, narrow);
  });
}

#[path = "factory_research_tests/feature.rs"]
mod feature;
pub(crate) use feature::feature_outputs;

// Models the existing ordinary Build retry boundary, shared by adapter and intake tests.
pub(crate) fn complete_ordinary_retry(builds: &Builds) -> (AttemptId, octacity_server_domain::JobId) {
  let attempt_id = AttemptId::generate();
  let job_id = octacity_server_domain::JobId::generate();
  let job = octacity_server_store::MaterializedJob::new(
    job_id,
    octacity_server_domain::PipelineNodeId::new("research").unwrap(),
    vec![],
    octacity_server_pipeline::DependencyPolicy::AllSucceeded,
    vec![octacity_server_domain::PoolId::generate()],
    octacity_server_store::MaterializedJobPayload::new(
      octacity_server_job::JobRequirements {
        capabilities: Default::default(),
        labels: Default::default(),
        minimum_cpu_millis: 0,
        minimum_memory_bytes: 0,
        minimum_disk_bytes: 0,
        runtime_class: octacity_server_domain::RuntimeClass::Native,
        operating_system: octacity_protocol::PlatformOs::Linux,
        architecture: octacity_protocol::PlatformArchitecture::Amd64,
        host_platform: None,
        required_guarantees: Default::default(),
      },
      octacity_server_store::testing::job_spec_template(builds.acceptance.build_id, "research"),
    )
    .unwrap(),
  )
  .unwrap();
  *builds.latest.lock().unwrap() = Some(AttemptRecord {
    id: attempt_id,
    build_id: builds.acceptance.build_id,
    number: AttemptNumber::new(2).unwrap(),
    retry_of_attempt_id: Some(builds.acceptance.attempt_id),
    state: AttemptState::Succeeded,
    version: AttemptVersion::INITIAL,
    created_at: time(29),
    updated_at: time(30),
    jobs: vec![octacity_server_store::JobRecord {
      attempt_id,
      job,
      state: octacity_server_job::JobState::Succeeded,
      version: octacity_server_domain::JobVersion::INITIAL,
      created_at: time(29),
      updated_at: time(30),
      queue: None,
      assignment: None,
      terminal: None,
      event_cursor: 0,
    }],
  });
  (attempt_id, job_id)
}
