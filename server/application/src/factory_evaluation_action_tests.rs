use crate::*;
use async_trait::async_trait;
use octacity_server_domain::{ArtifactId, AttemptId, BuildId, ImmutableRevision, JobId, Timestamp};
use octacity_server_factory::*;
use octacity_server_store::{testing as store_testing, *};
use std::{collections::BTreeMap, sync::Arc};

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn at(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}
fn reference(value: &str) -> ImmutableReference {
  ImmutableReference::new(key(value), key("v1"), FactoryDigest::from_bytes([1; 32]))
}
fn artifact(value: u8) -> FactoryArtifactReference {
  FactoryArtifactReference::new(ArtifactId::generate(), FactoryDigest::from_bytes([value; 32]), 1).unwrap()
}
fn budget() -> BudgetLimit {
  BudgetLimit::new(1, 1000, 100, 100, 100_000).unwrap()
}

struct Publication {
  identity: ImmutableReference,
  value: serde_json::Value,
  discovery: Option<FlowDiscoveryFacts>,
  outcome: FlowOutcomeDefinition,
}
#[async_trait]
impl FactoryFlowActionPlugin for Publication {
  fn identity(&self) -> ImmutableReference {
    self.identity.clone()
  }
  fn validate(&self, _: &FlowActionBinding, _: &[FlowOutcomeDefinition]) -> Result<(), ApplicationError> {
    Ok(())
  }
  async fn cancel(
    &self,
    _: FactoryFlowActionRequest<'_>,
    _: Timestamp,
  ) -> Result<Option<BudgetUsage>, ApplicationError> {
    Ok(Some(BudgetUsage::default()))
  }
  async fn execute(
    &self,
    request: FactoryFlowActionRequest<'_>,
  ) -> Result<FactoryFlowActionObservation, ApplicationError> {
    let schema = request
      .schemas
      .iter()
      .find(|schema| schema.reference() == self.outcome.schema())
      .unwrap();
    Ok(FactoryFlowActionObservation {
      discovery: self.discovery.clone(),
      outcome: self.outcome.key().clone(),
      payload: FlowPayload::new(schema, self.value.clone()).unwrap(),
      usage: BudgetUsage::default(),
      outbox: vec![],
    })
  }
}
struct Inputs(CandidateSubject);
#[async_trait]
impl FactoryFlowInputPreparer for Inputs {
  async fn prepare(
    &self,
    snapshot: &FactoryRunSnapshot,
    target: &FactoryNodeTarget,
  ) -> Result<FlowNodeInput, ApplicationError> {
    prepare_factory_flow_input(snapshot, target)?
      .with_context(
        ContextManifest::new(
          ContextManifestId::generate(),
          FactoryTaskSubject::Candidate(self.0.clone()),
          FactoryDigest::from_bytes([3; 32]),
          vec![],
        )
        .unwrap(),
        vec![],
      )
      .map_err(|_| ApplicationError::invalid())
  }
}
struct Fixture {
  store: Arc<store_testing::InMemoryFactoryConfigurationStore>,
  executors: Arc<FactoryNodeExecutorRegistry>,
  input: Arc<Inputs>,
  run: FactoryRunId,
  claim: FactoryDigest,
  plan: EvaluationPlan,
  assessment: Assessment,
  policy: DecisionPolicy,
}

fn fixture(trusted_assessment: bool, failing_evidence: bool, complete_selection: bool) -> Fixture {
  let work = store_testing::factory_node_journal_contract_fixture().admission.work;
  let candidate = CandidateSubject::new(
    work.subject().clone(),
    ImmutableRevision::new("reviewed-candidate").unwrap(),
    FactoryDigest::from_bytes([9; 32]),
  );
  let (input, node) = octacity_server_factory::testing::candidate_node_fixture(&work);
  let changeset = ChangeSet::new(
    ChangeSetId::generate(),
    &input,
    &node,
    candidate.clone(),
    ArtifactId::generate(),
    ArtifactId::generate(),
  )
  .unwrap();
  let evidence = EvidenceManifest::new(
    EvidenceManifestId::generate(),
    &changeset,
    candidate.clone(),
    at(8),
    vec![EvidenceRequirement::new(
      key("tests"),
      EvidenceOutputKind::Report,
      reference("test-schema"),
      reference("validator"),
      reference("validation-plugin"),
    )],
    vec![EvidenceItem::new(EvidenceItemInput {
      subject: candidate.clone(),
      kind: key("tests"),
      output_kind: EvidenceOutputKind::Report,
      artifact: artifact(5),
      schema: reference("test-schema"),
      producer: EvidenceProducer::new(
        BuildId::generate(),
        AttemptId::generate(),
        JobId::generate(),
        reference("validator"),
        reference("validation-plugin"),
      ),
      outcome: if failing_evidence {
        DeterministicGateOutcome::Failed
      } else {
        DeterministicGateOutcome::Passed
      },
      published_at: at(7),
      fresh_until: at(5000),
    })],
  )
  .unwrap();
  let pack_artifact = artifact(1);
  let pack = CriterionPack::new(
    work.subject().project_id(),
    reference("quality"),
    reference("criteria-schema"),
    pack_artifact,
  )
  .unwrap();
  let evaluator = reference("selected-reviewer");
  let handling = reference("restricted-source");
  let planning =
    EvaluationPolicy::try_new(vec![pack.reference().clone()], vec![evaluator.clone()], 1, budget()).unwrap();
  let ReviewPlanPreparation::Ready(plan) = prepare_evaluation_plan(
    EvaluationPlanDefinition {
      id: EvaluationPlanId::generate(),
      purpose: ReviewPurpose::CodeReview,
      subject: candidate.clone(),
      criterion_packs: vec![pack],
      branches: vec![ReviewBranch::new(key("independent-check"), evaluator.clone(), true)],
      budget: budget(),
      created_at: at(9),
      deadline: at(1000),
      data_handling: handling.clone(),
    },
    &evidence,
    &planning,
    &[ReviewEvaluatorCapability::try_new(
      evaluator.clone(),
      vec![ReviewPurpose::CodeReview],
      vec![handling],
      budget(),
    )
    .unwrap()],
  )
  .unwrap() else {
    panic!("qualified review plan")
  };
  let plan = *plan;
  let assessment = Assessment::new(
    AssessmentId::generate(),
    &plan,
    AssessmentInput {
      subject: candidate.clone(),
      evaluator,
      outcome: AssessmentOutcome::Satisfied,
      summary: BoundedSummary::new(
        FactoryTaskSubject::Candidate(candidate.clone()),
        FactorySafeText::new("The selected criteria are satisfied.").unwrap(),
        FactoryDigest::from_bytes([2; 32]),
      ),
      findings: vec![],
      model: reference("review-model"),
      prompt_digest: FactoryDigest::from_bytes([4; 32]),
      result: artifact(6),
      provenance: artifact(7),
    },
  )
  .unwrap();
  let policy = DecisionPolicy::try_new(
    DecisionPolicyVersion::INITIAL,
    DecisionPolicyDefinition {
      required_evidence: vec![key("tests")],
      required_evaluators: vec![key("selected-reviewer")],
      quorum: 1,
      severity_threshold: FindingSeverity::High,
      indeterminate_policy: IndeterminatePolicy::RequiredOnly,
      failure_outcome: DecisionOutcome::Rework,
    },
  )
  .unwrap();
  let (review_input, review_node) = octacity_server_factory::testing::capability_node_fixture(
    &work,
    Some(
      ContextManifest::new(
        ContextManifestId::generate(),
        FactoryTaskSubject::Candidate(candidate.clone()),
        FactoryDigest::from_bytes([3; 32]),
        vec![],
      )
      .unwrap(),
    ),
  );
  let mut progress = EvaluationProgress::from_plan(&plan).unwrap();
  for state in [
    EvaluationBranchState::AttemptCreated,
    EvaluationBranchState::BuildActive,
    EvaluationBranchState::ResultPending,
  ] {
    progress = progress.advance_branch(&key("independent-check"), state).unwrap();
  }
  progress = progress
    .record_result(EvaluationBranchResult::new(&plan, &review_input, &review_node, &assessment).unwrap())
    .unwrap();
  let sample = FactoryEvaluationDecision::from_completed_join(
    DecisionId::generate(),
    &evidence,
    &plan,
    &policy,
    &progress,
    std::slice::from_ref(&assessment),
    &[],
  )
  .unwrap();
  let values = [
    serde_json::to_value(&evidence).unwrap(),
    serde_json::to_value(&plan).unwrap(),
    serde_json::to_value(&assessment).unwrap(),
  ];
  let schemas = ["manifest-contract", "plan-contract", "assessment-contract"]
    .into_iter()
    .zip(&values)
    .map(|(name, value)| {
      FlowDataSchema::new(key(name), key("v1"), crate::factory_review_planning_tests::shape(value)).unwrap()
    })
    .collect::<Vec<_>>();
  let decision_schema = FlowDataSchema::new(
    key("decision-contract"),
    key("v1"),
    crate::factory_review_planning_tests::shape(&serde_json::to_value(sample.decision()).unwrap()),
  )
  .unwrap();
  let input_value =
    serde_json::json!({"evidence":&evidence,"plan":&plan,"assessments":[&assessment],"signal_receipts":[]});
  let decision_input = FlowDataSchema::new(
    key("decision-input"),
    key("v1"),
    crate::factory_review_planning_tests::shape(&input_value),
  )
  .unwrap();
  let empty = FlowDataSchema::new(key("empty-input"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let dispositions = [
    DecisionOutcome::Accept,
    DecisionOutcome::Rework,
    DecisionOutcome::Escalate,
    DecisionOutcome::Reject,
    DecisionOutcome::Cancel,
  ];
  let profile = FactoryEvaluationDecisionProfile {
    action: key("apply-policy"),
    policy: policy.clone(),
    outcomes: dispositions
      .into_iter()
      .zip(["accepted", "revise", "operator", "rejected", "cancelled"])
      .map(|(disposition, name)| {
        (
          disposition,
          FlowOutcomeDefinition::new(key(name), FlowOutcomeKind::Success, decision_schema.reference().clone()),
        )
      })
      .collect(),
  };
  let decision_identity = ImmutableReference::new(key("installed-decision"), key("v1"), profile.digest().unwrap());
  let owners = [
    reference("evidence-owner"),
    reference("planning-owner"),
    reference("review-owner"),
  ];
  let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: owners.iter().cloned().chain([decision_identity.clone()]).collect(),
    ..Default::default()
  })
  .unwrap();
  let names = ["collect-facts", "freeze-checks", "judge-candidate"];
  let observed = schemas
    .iter()
    .map(|schema| FlowOutcomeDefinition::new(key("ready"), FlowOutcomeKind::Success, schema.reference().clone()))
    .collect::<Vec<_>>();
  let mut nodes = names
    .iter()
    .enumerate()
    .map(|(index, name)| {
      FlowNodeDefinition::new(FlowNodeDefinitionInput {
        key: key(name),
        kind: FlowNodeKind::TrustedAction,
        input_schema: Some(empty.reference().clone()),
        outcomes: vec![observed[index].clone()],
        budget: budget(),
        permissions: permissions.clone(),
        required: true,
        subflow: None,
      })
      .unwrap()
      .with_action(FlowActionBinding::new(owners[index].clone(), key("publish"), serde_json::json!({})).unwrap())
      .unwrap()
      .with_input_binding(
        FlowInputBinding::new(
          BTreeMap::new(),
          FlowDataMapping::new(FlowValueMapping::Constant {
            value: serde_json::json!(true),
          })
          .unwrap(),
        )
        .unwrap(),
      )
      .unwrap()
    })
    .collect::<Vec<_>>();
  let mut selections = BTreeMap::new();
  for (alias, name) in ["evidence", "plan", "assessment"].into_iter().zip(names) {
    selections.insert(
      key(alias),
      FlowInputSource::Record {
        node: key(name),
        outcome: key("ready"),
        view: FlowRecordView::Payload,
        path: String::new(),
      },
    );
  }
  let mut fields = BTreeMap::from([
    (
      key("evidence"),
      FlowValueMapping::Pointer {
        path: "/evidence".into(),
      },
    ),
    (key("plan"), FlowValueMapping::Pointer { path: "/plan".into() }),
    (
      key("assessments"),
      FlowValueMapping::Array {
        items: vec![FlowValueMapping::Pointer {
          path: "/assessment".into(),
        }],
      },
    ),
    (key("signal_receipts"), FlowValueMapping::Array { items: vec![] }),
  ]);
  if !complete_selection {
    selections.remove(&key("plan"));
    fields.insert(
      key("plan"),
      FlowValueMapping::Constant {
        value: serde_json::to_value(&plan).unwrap(),
      },
    );
  }
  nodes.push(
    FlowNodeDefinition::new(FlowNodeDefinitionInput {
      key: key("choose-next-action"),
      kind: FlowNodeKind::TrustedAction,
      input_schema: Some(decision_input.reference().clone()),
      outcomes: profile.outcomes.iter().map(|(_, outcome)| outcome.clone()).collect(),
      budget: budget(),
      permissions: permissions.clone(),
      required: true,
      subflow: None,
    })
    .unwrap()
    .with_action(
      FlowActionBinding::new(decision_identity.clone(), profile.action.clone(), serde_json::json!({})).unwrap(),
    )
    .unwrap()
    .with_input_binding(
      FlowInputBinding::new(
        selections,
        FlowDataMapping::new(FlowValueMapping::Object { fields }).unwrap(),
      )
      .unwrap(),
    )
    .unwrap(),
  );
  let mut transitions = vec![
    FlowTransition::new(key(names[0]), key("ready"), FlowTransitionTarget::Node(key(names[1]))),
    FlowTransition::new(key(names[1]), key("ready"), FlowTransitionTarget::Node(key(names[2]))),
    FlowTransition::new(
      key(names[2]),
      key("ready"),
      FlowTransitionTarget::Node(key("choose-next-action")),
    ),
  ];
  transitions.extend(profile.outcomes.iter().map(|(_, outcome)| {
    FlowTransition::new(
      key("choose-next-action"),
      outcome.key().clone(),
      FlowTransitionTarget::Terminal(key("finished")),
    )
  }));
  let total = BudgetLimit::new(8, 10_000, 1000, 1000, 1_000_000).unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(empty.reference().clone()),
    entry: key(names[0]),
    nodes,
    transitions,
    terminals: vec![FlowTerminalDefinition::new(
      key("finished"),
      decision_schema.reference().clone(),
    )],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(total, permissions.clone(), 1).unwrap(),
  })
  .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let flow = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    FlowAdmissionLimits::product_defaults(1, total, permissions).unwrap(),
    flow,
    cycle,
    FactoryWipLimits::new(8, 16).unwrap(),
  )
  .unwrap()
  .with_data_schemas(
    schemas
      .iter()
      .cloned()
      .chain([decision_schema, decision_input, empty])
      .collect(),
  )
  .unwrap();
  let store = Arc::new(store_testing::InMemoryFactoryConfigurationStore::new());
  let audit = store_testing::management_mutation_with_request((), "decision-admission")
    .audit()
    .clone();
  store
    .seed_factory_run(
      PublishedFactoryAdmission {
        work,
        run: run.clone(),
        flow: admitted,
        admitted_at: at(1),
      },
      FactoryDigest::from_bytes([1; 32]),
      &audit,
    )
    .unwrap();
  let claim = FactoryRunClaimRecord::new(
    run.id(),
    key("worker"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([2; 32])),
      at(10),
      at(5000),
    )
    .unwrap(),
  );
  crate::factory_admission_tests::run_ready(store.claim_factory_run(ClaimFactoryRun {
    run_id: run.id(),
    expected_version: run.version(),
    record: claim.clone(),
    audit: FactoryAuditFact::new(
      run.id(),
      AuditActorKind::Worker,
      None,
      key("factory.claimed"),
      FactoryDigest::from_bytes([1; 32]),
      key("accepted"),
      at(10),
    ),
  }))
  .unwrap();
  let discoveries = [
    Some(FlowDiscoveryFacts::evidence(&evidence).unwrap()),
    Some(FlowDiscoveryFacts::review_plan(&plan).unwrap()),
    trusted_assessment.then(|| FlowDiscoveryFacts::assessment(&assessment).unwrap()),
  ];
  let mut entries = owners
    .into_iter()
    .zip(values)
    .zip(discoveries)
    .zip(observed)
    .map(|(((identity, value), discovery), outcome)| {
      (
        FlowNodeKind::TrustedAction,
        NodeExecutionIdentity::External(identity.clone()),
        Arc::new(FactoryFlowActionAdapter::new(Arc::new(Publication {
          identity,
          value,
          discovery,
          outcome,
        }))) as Arc<dyn FactoryNodeExecutor>,
      )
    })
    .collect::<Vec<_>>();
  entries.push((
    FlowNodeKind::TrustedAction,
    NodeExecutionIdentity::External(decision_identity.clone()),
    Arc::new(FactoryFlowActionAdapter::new(Arc::new(
      FactoryEvaluationDecisionAction::new(
        decision_identity,
        store.clone(),
        profile,
        crate::factory_node_test_support::clock(),
      ),
    ))),
  ));
  Fixture {
    store,
    executors: Arc::new(FactoryNodeExecutorRegistry::new(entries).unwrap()),
    input: Arc::new(Inputs(candidate)),
    run: run.id(),
    claim: claim.id,
    plan,
    assessment,
    policy,
  }
}

async fn run(fixture: &Fixture) -> Result<FactoryRunSnapshot, ApplicationError> {
  let runner = FactoryFlowRunner::new(
    fixture.store.clone(),
    fixture.executors.clone(),
    fixture.input.clone(),
    crate::factory_node_test_support::clock(),
  );
  for time in 11..70 {
    runner.reconcile_flow(fixture.run, fixture.claim, at(time)).await?;
    let snapshot = fixture.store.factory_run_snapshot(fixture.run).await?;
    if snapshot.flow.data.records.len() == 4 {
      return Ok(snapshot);
    }
  }
  panic!("configured decision must finish")
}

#[test]
fn selected_qualified_review_records_produce_one_deterministic_decision_and_replay_after_restart() {
  crate::factory_admission_tests::run_ready(async {
    let h = fixture(true, false, true);
    let snapshot = run(&h).await.unwrap();
    let record = snapshot
      .flow
      .data
      .records
      .iter()
      .find(|record| {
        record
          .discovery()
          .is_some_and(|facts| facts.kind() == FlowDiscoveryKind::Decision)
      })
      .unwrap();
    let decision: Decision = serde_json::from_value(record.payload().value().clone()).unwrap();
    assert_eq!(decision.outcome(), DecisionOutcome::Accept);
    assert_eq!(decision.plan_id(), h.plan.id());
    assert_eq!(decision.subject(), h.plan.subject());
    assert_eq!(decision.assessment_ids(), &[h.assessment.id()]);
    assert_eq!(decision.policy_digest(), h.policy.digest());
    assert_eq!(decision.reasons(), &[DecisionReason::PolicySatisfied]);
    let restarted = FactoryNodeRunner::new(h.store.clone(), h.executors, crate::factory_node_test_support::clock());
    let target = FactoryNodeTarget {
      flow_run_id: record.input().flow_run_id(),
      cycle_id: record.input().cycle_id(),
      node: record.input().node().clone(),
    };
    assert_eq!(
      restarted.run_once(h.run, h.claim, target, None, at(80)).await.unwrap(),
      FactoryNodeStep::Completed(Box::new(record.clone()))
    );
    assert_eq!(
      h.store.factory_run_snapshot(h.run).await.unwrap().flow.data.records,
      snapshot.flow.data.records
    );
  });
}

#[test]
fn failed_verified_evidence_selects_the_installed_nonaccepting_outcome() {
  crate::factory_admission_tests::run_ready(async {
    let h = fixture(true, true, true);
    let snapshot = run(&h).await.unwrap();
    let record = snapshot
      .flow
      .data
      .records
      .iter()
      .find(|record| {
        record
          .discovery()
          .is_some_and(|facts| facts.kind() == FlowDiscoveryKind::Decision)
      })
      .unwrap();
    let decision: Decision = serde_json::from_value(record.payload().value().clone()).unwrap();
    assert_eq!(decision.outcome(), DecisionOutcome::Rework);
    assert_eq!(
      decision.reasons(),
      &[DecisionReason::RequiredEvidenceFailed(key("tests"))]
    );
    assert_eq!(record.metadata().unwrap()["outcome"], serde_json::json!("revise"));
  });
}

#[test]
fn model_shaped_assessment_and_unselected_plan_cannot_authorize_a_decision() {
  crate::factory_admission_tests::run_ready(async {
    for (trusted, selected) in [(false, true), (true, false)] {
      let h = fixture(trusted, false, selected);
      assert!(matches!(run(&h).await, Err(ApplicationError::InvalidInput)));
      let snapshot = h.store.factory_run_snapshot(h.run).await.unwrap();
      assert_eq!(snapshot.flow.data.records.len(), 3);
      assert!(!snapshot.flow.data.records.iter().any(|record| {
        record
          .discovery()
          .is_some_and(|facts| facts.kind() == FlowDiscoveryKind::Decision)
      }));
    }
  });
}
