use crate::*;
use async_trait::async_trait;
use octacity_server_domain::{ArtifactId, AttemptId, BuildId, ImmutableRevision, JobId, Timestamp};
use octacity_server_factory::*;
use octacity_server_store::{testing as store_testing, *};
use std::{collections::BTreeMap, sync::Arc};

fn key(s: &str) -> FactoryKey {
  FactoryKey::new(s).unwrap()
}
fn reference(s: &str) -> ImmutableReference {
  ImmutableReference::new(key(s), key("v1"), FactoryDigest::from_bytes([1; 32]))
}
fn at(t: i64) -> Timestamp {
  Timestamp::from_unix_millis(t).unwrap()
}
fn budget() -> BudgetLimit {
  BudgetLimit::new(1, 1000, 10, 10, 10000).unwrap()
}
fn artifact(byte: u8) -> FactoryArtifactReference {
  FactoryArtifactReference::new(ArtifactId::generate(), FactoryDigest::from_bytes([byte; 32]), 1).unwrap()
}
// Published fixture contracts retain structured objects; IDs and timestamps remain variable.
pub(crate) fn shape(v: &serde_json::Value) -> FlowValueSchema {
  match v {
    serde_json::Value::Object(fields)
      if fields.len() == 1 && fields.keys().any(|name| FactoryKey::new(name).is_err()) =>
    {
      FlowValueSchema::ExternallyTagged {
        variants: fields
          .iter()
          .map(|(name, value)| (name.clone(), Some(Box::new(shape(value)))))
          .collect(),
      }
    }
    serde_json::Value::Object(fields) => FlowValueSchema::Object {
      fields: fields
        .iter()
        .map(|(k, v)| (key(k), FlowFieldSchema::required(shape(v))))
        .collect(),
    },
    serde_json::Value::Array(items) => FlowValueSchema::Array {
      item: Box::new(items.first().map(shape).unwrap_or(FlowValueSchema::Boolean)),
      min_items: items.len().try_into().unwrap(),
      max_items: items.len().try_into().unwrap(),
    },
    serde_json::Value::String(_) => FlowValueSchema::String {
      min_bytes: 1,
      max_bytes: 1024,
    },
    serde_json::Value::Number(_) => FlowValueSchema::Integer {
      minimum: 0,
      maximum: i64::MAX,
    },
    serde_json::Value::Bool(_) => FlowValueSchema::Boolean,
    serde_json::Value::Null => FlowValueSchema::Enum {
      values: vec![serde_json::Value::Null],
    },
  }
}
pub(crate) struct EvidenceAction {
  pub(crate) manifest: EvidenceManifest,
  pub(crate) outcome: FlowOutcomeDefinition,
  pub(crate) trusted: bool,
}
#[async_trait]
impl FactoryFlowActionPlugin for EvidenceAction {
  fn identity(&self) -> ImmutableReference {
    reference("qualified-evidence")
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
  async fn execute(&self, r: FactoryFlowActionRequest<'_>) -> Result<FactoryFlowActionObservation, ApplicationError> {
    let schema = r
      .schemas
      .iter()
      .find(|s| s.reference() == self.outcome.schema())
      .unwrap();
    Ok(FactoryFlowActionObservation {
      discovery: self
        .trusted
        .then(|| FlowDiscoveryFacts::evidence(&self.manifest).unwrap()),
      outcome: self.outcome.key().clone(),
      payload: FlowPayload::new(schema, serde_json::to_value(&self.manifest).unwrap()).unwrap(),
      usage: BudgetUsage::default(),
      outbox: vec![],
    })
  }
}
struct Inputs;
#[async_trait]
impl FactoryFlowInputPreparer for Inputs {
  async fn prepare(&self, s: &FactoryRunSnapshot, t: &FactoryNodeTarget) -> Result<FlowNodeInput, ApplicationError> {
    prepare_factory_flow_input(s, t)
  }
}
struct Fixture {
  store: Arc<store_testing::InMemoryFactoryConfigurationStore>,
  run: FactoryRunId,
  claim: FactoryDigest,
  manifest: EvidenceManifest,
  profile: FactoryReviewPlanningProfile,
  evidence_outcome: FlowOutcomeDefinition,
  probe: bool,
  reviewer: Option<FactoryCodexBuildProfile>,
}
fn fixture() -> Fixture {
  fixture_with_capability(true)
}
fn fixture_with_capability(available: bool) -> Fixture {
  fixture_with_limits(available, 10_000, true)
}
fn fixture_with_limits(available: bool, output_limit: u64, complete_projection: bool) -> Fixture {
  fixture_with_freshness(available, output_limit, complete_projection, 2000)
}
fn fixture_with_freshness(available: bool, output_limit: u64, complete_projection: bool, fresh_until: i64) -> Fixture {
  fixture_with_review_probe(available, output_limit, complete_projection, fresh_until, None, false)
}
fn fixture_with_review_probe(
  available: bool,
  output_limit: u64,
  complete_projection: bool,
  fresh_until: i64,
  probe: Option<bool>,
  codex: bool,
) -> Fixture {
  fixture_with_review_elapsed(
    available,
    output_limit,
    complete_projection,
    fresh_until,
    probe,
    codex,
    1000,
  )
}
fn fixture_with_review_elapsed(
  available: bool,
  output_limit: u64,
  complete_projection: bool,
  fresh_until: i64,
  probe: Option<bool>,
  codex: bool,
  plan_elapsed: u64,
) -> Fixture {
  let work = store_testing::factory_node_journal_contract_fixture().admission.work;
  let subject = CandidateSubject::new(
    work.subject().clone(),
    ImmutableRevision::new("reviewed-revision").unwrap(),
    FactoryDigest::from_bytes([9; 32]),
  );
  let (candidate_input, candidate_attempt) = octacity_server_factory::testing::candidate_node_fixture(&work);
  let changeset = ChangeSet::new(
    ChangeSetId::generate(),
    &candidate_input,
    &candidate_attempt,
    subject.clone(),
    ArtifactId::generate(),
    ArtifactId::generate(),
  )
  .unwrap();
  let schema = reference("verified-test-report");
  let tool = reference("test-runner");
  let plugin = reference("tests-plugin");
  let manifest = EvidenceManifest::new(
    EvidenceManifestId::generate(),
    &changeset,
    subject.clone(),
    at(8),
    vec![EvidenceRequirement::new(
      key("tests"),
      EvidenceOutputKind::Report,
      schema.clone(),
      tool.clone(),
      plugin.clone(),
    )],
    vec![EvidenceItem::new(EvidenceItemInput {
      subject: subject.clone(),
      kind: key("tests"),
      output_kind: EvidenceOutputKind::Report,
      artifact: artifact(5),
      schema,
      producer: EvidenceProducer::new(
        BuildId::generate(),
        AttemptId::generate(),
        JobId::generate(),
        tool,
        plugin,
      ),
      outcome: DeterministicGateOutcome::Passed,
      published_at: at(7),
      fresh_until: at(fresh_until),
    })],
  )
  .unwrap();
  let pack_bytes = b"Review the actual candidate against the selected deterministic evidence.";
  let pack_artifact = FactoryArtifactReference::new(
    ArtifactId::generate(),
    FactoryDigest::content_sha256(pack_bytes),
    pack_bytes.len() as u64,
  )
  .unwrap();
  let pack = CriterionPack::new(
    work.subject().project_id(),
    ImmutableReference::new(key("quality"), key("v1"), pack_artifact.content_digest()),
    reference("criterion-schema"),
    pack_artifact,
  )
  .unwrap();
  let evaluator = reference("independent-reviewer");
  let handling = reference("restricted-source");
  let escalation_schema = FlowDataSchema::new(
    key("planning-escalation"),
    key("v1"),
    FlowValueSchema::Object {
      fields: BTreeMap::from([
        (
          key("reason"),
          FlowFieldSchema::required(FlowValueSchema::String {
            min_bytes: 1,
            max_bytes: 128,
          }),
        ),
        (
          key("branch"),
          FlowFieldSchema::optional(FlowValueSchema::String {
            min_bytes: 1,
            max_bytes: 128,
          }),
        ),
        (
          key("required"),
          FlowFieldSchema::optional(FlowValueSchema::Integer {
            minimum: 1,
            maximum: 32,
          }),
        ),
        (
          key("available"),
          FlowFieldSchema::optional(FlowValueSchema::Integer {
            minimum: 0,
            maximum: 32,
          }),
        ),
      ]),
    },
  )
  .unwrap();
  let placeholder = FlowOutcomeDefinition::new(
    key("planned"),
    FlowOutcomeKind::Success,
    reference("temporary-plan-schema"),
  );
  let mut profile = FactoryReviewPlanningProfile {
    action: key("prepare"),
    ready: placeholder,
    escalation: FlowOutcomeDefinition::new(
      key("escalated"),
      FlowOutcomeKind::Failure,
      escalation_schema.reference().clone(),
    ),
    purpose: ReviewPurpose::CodeReview,
    criterion_packs: vec![pack.clone()],
    branches: vec![ReviewBranch::new(key("independent"), evaluator.clone(), true)],
    policy: EvaluationPolicy::try_new(
      vec![pack.reference().clone()],
      vec![evaluator.clone()],
      1,
      BudgetLimit::new(1, plan_elapsed, 10, 10, output_limit).unwrap(),
    )
    .unwrap(),
    data_handling: handling.clone(),
    capabilities: vec![
      ReviewEvaluatorCapability::try_new(
        evaluator,
        vec![ReviewPurpose::CodeReview],
        vec![handling],
        BudgetLimit::new(1, plan_elapsed, 10, 10, 10000).unwrap(),
      )
      .unwrap(),
    ],
  };
  let ReviewPlanPreparation::Ready(sample) = prepare_evaluation_plan(
    EvaluationPlanDefinition {
      id: EvaluationPlanId::generate(),
      purpose: profile.purpose,
      subject,
      criterion_packs: profile.criterion_packs.clone(),
      branches: profile.branches.clone(),
      budget: profile.policy.budget(),
      created_at: at(10),
      deadline: at(10 + plan_elapsed as i64),
      data_handling: profile.data_handling.clone(),
    },
    &manifest,
    &profile.policy,
    &profile.capabilities,
  )
  .unwrap() else {
    panic!("qualified fixture")
  };
  if !available {
    profile.capabilities.clear();
  }
  let evidence_schema = FlowDataSchema::new(
    key("retained-evidence"),
    key("v1"),
    shape(&serde_json::to_value(&manifest).unwrap()),
  )
  .unwrap();
  let plan_schema = FlowDataSchema::new(
    key("frozen-plan"),
    key("v1"),
    shape(&serde_json::to_value(&sample).unwrap()),
  )
  .unwrap();
  let initial_schema = FlowDataSchema::new(key("begin-input"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let evidence_outcome = FlowOutcomeDefinition::new(
    key("observed"),
    FlowOutcomeKind::Success,
    evidence_schema.reference().clone(),
  );
  let outcome = FlowOutcomeDefinition::new(
    key("planned"),
    FlowOutcomeKind::Success,
    plan_schema.reference().clone(),
  );
  profile.ready = outcome.clone();
  let planner_identity = planner_identity(&profile);
  let reviewer = codex.then(|| reviewer_profile(&work));
  let review_schema = probe.map(|full| {
    let selected_plan = if full {
      serde_json::to_value(&sample).unwrap()
    } else {
      serde_json::to_value(sample.subject()).unwrap()
    };
    let selected = serde_json::json!({
      "plan": selected_plan,
      "evidence": manifest,
      "evaluator": profile.branches[0].evaluator(),
    });
    FlowDataSchema::new(key("explicit-review-input"), key("v1"), shape(&selected)).unwrap()
  });
  let assessment_schema = reviewer.as_ref().map(|reviewer| {
    let assessment = Assessment::new(
      AssessmentId::generate(),
      &sample,
      AssessmentInput {
        subject: sample.subject().clone(),
        evaluator: profile.branches[0].evaluator().clone(),
        outcome: AssessmentOutcome::Satisfied,
        summary: BoundedSummary::new(
          FactoryTaskSubject::Candidate(sample.subject().clone()),
          FactorySafeText::new("Selected criteria satisfied.").unwrap(),
          FactoryDigest::from_bytes([2; 32]),
        ),
        findings: vec![],
        model: reviewer.toolchain.model.clone(),
        prompt_digest: FactoryDigest::from_bytes([3; 32]),
        result: artifact(6),
        provenance: artifact(7),
      },
    )
    .unwrap();
    FlowDataSchema::new(
      key("verified-assessment"),
      key("v1"),
      shape(&serde_json::to_value(assessment).unwrap()),
    )
    .unwrap()
  });
  let mut plugins = vec![reference("qualified-evidence"), planner_identity.clone()];
  if probe.is_some() {
    plugins.push(reference("review-probe"));
  }
  let permissions = if let Some(reviewer) = &reviewer {
    let mut permissions = serde_json::to_value(&reviewer.project_permissions).unwrap();
    permissions["plugins"]
      .as_array_mut()
      .unwrap()
      .extend(plugins.iter().map(|p| serde_json::to_value(p).unwrap()));
    serde_json::from_value(permissions).unwrap()
  } else {
    FactoryPermissionSet::try_new(FactoryPermissionDraft {
      plugins,
      ..Default::default()
    })
    .unwrap()
  };
  let node = |name: &str,
              input: &FlowDataSchema,
              result: FlowOutcomeDefinition,
              owner: &str,
              action: &str,
              binding: FlowInputBinding| {
    FlowNodeDefinition::new(FlowNodeDefinitionInput {
      key: key(name),
      kind: FlowNodeKind::TrustedAction,
      input_schema: Some(input.reference().clone()),
      outcomes: if owner == "installed-planner" {
        vec![result, profile.escalation.clone()]
      } else {
        vec![result]
      },
      budget: BudgetLimit::new(1, 2000, 10, 10, 10_000).unwrap(),
      permissions: permissions.clone(),
      required: true,
      subflow: None,
    })
    .unwrap()
    .with_action(
      FlowActionBinding::new(
        if owner == "installed-planner" {
          planner_identity.clone()
        } else {
          reference(owner)
        },
        key(action),
        serde_json::json!({}),
      )
      .unwrap(),
    )
    .unwrap()
    .with_input_binding(binding)
    .unwrap()
  };
  let evidence = node(
    "collect_observations",
    &initial_schema,
    evidence_outcome.clone(),
    "qualified-evidence",
    "publish",
    FlowInputBinding::new(
      BTreeMap::new(),
      FlowDataMapping::new(FlowValueMapping::Constant {
        value: serde_json::json!(true),
      })
      .unwrap(),
    )
    .unwrap(),
  );
  let planner_schema = if complete_projection {
    evidence_schema.clone()
  } else {
    FlowDataSchema::new(
      key("selected-subject"),
      key("v1"),
      shape(&serde_json::to_value(manifest.subject()).unwrap()),
    )
    .unwrap()
  };
  let planner = node(
    "freeze_independent_checks",
    &planner_schema,
    outcome.clone(),
    "installed-planner",
    "prepare",
    FlowInputBinding::new(
      BTreeMap::from([(
        key("selected"),
        FlowInputSource::Predecessor {
          view: FlowRecordView::Payload,
          path: String::new(),
        },
      )]),
      FlowDataMapping::new(FlowValueMapping::Pointer {
        path: if complete_projection {
          "/selected".into()
        } else {
          "/selected/subject".into()
        },
      })
      .unwrap(),
    )
    .unwrap(),
  );
  let total = BudgetLimit::new(4, 10000, 100, 100, 100000).unwrap();
  let mut definition = FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(initial_schema.reference().clone()),
    entry: evidence.key().clone(),
    nodes: vec![evidence, planner],
    transitions: vec![
      FlowTransition::new(
        key("collect_observations"),
        key("observed"),
        FlowTransitionTarget::Node(key("freeze_independent_checks")),
      ),
      FlowTransition::new(
        key("freeze_independent_checks"),
        key("escalated"),
        FlowTransitionTarget::Terminal(key("needs_operator")),
      ),
      FlowTransition::new(
        key("freeze_independent_checks"),
        key("planned"),
        FlowTransitionTarget::Terminal(key("finished")),
      ),
    ],
    terminals: vec![
      FlowTerminalDefinition::new(key("needs_operator"), escalation_schema.reference().clone()),
      FlowTerminalDefinition::new(key("finished"), plan_schema.reference().clone()),
    ],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(total, permissions.clone(), 1).unwrap(),
  };
  if let Some(schema) = &review_schema {
    let sources = BTreeMap::from([
      (
        key("plan"),
        FlowInputSource::Predecessor {
          view: FlowRecordView::Payload,
          path: if probe == Some(true) {
            String::new()
          } else {
            "/subject".into()
          },
        },
      ),
      (
        key("evidence"),
        FlowInputSource::Record {
          node: key("collect_observations"),
          outcome: key("observed"),
          view: FlowRecordView::Payload,
          path: String::new(),
        },
      ),
    ]);
    let review = node(
      "consume_frozen_review",
      schema,
      FlowOutcomeDefinition::new(
        key("checked"),
        FlowOutcomeKind::Success,
        initial_schema.reference().clone(),
      ),
      "review-probe",
      "inspect",
      FlowInputBinding::new(
        sources,
        FlowDataMapping::new(FlowValueMapping::Object {
          fields: BTreeMap::from([
            (key("plan"), FlowValueMapping::Pointer { path: "/plan".into() }),
            (
              key("evidence"),
              FlowValueMapping::Pointer {
                path: "/evidence".into(),
              },
            ),
            (
              key("evaluator"),
              FlowValueMapping::Constant {
                value: serde_json::to_value(profile.branches[0].evaluator()).unwrap(),
              },
            ),
          ]),
        })
        .unwrap(),
      )
      .unwrap(),
    );
    let review = if let Some(reviewer) = &reviewer {
      let binding = FlowBuildBinding {
        tool: reviewer.toolchain.executable.clone(),
        plugin: reviewer.toolchain.plugin.clone(),
        model_or_tool: reviewer.toolchain.model.clone(),
        build_configuration: reviewer.build_configuration.clone(),
        task_digest: reviewer.task_digest(),
        result_output: key("codex-run-result"),
        result_tool: None,
        result_output_kind: EvidenceOutputKind::Report,
        result_outcome: key("checked"),
        evidence: vec![],
        capture_write_paths: None,
        capture_deliverables: None,
      };
      FlowNodeDefinition::new(FlowNodeDefinitionInput {
        key: review.key().clone(),
        kind: FlowNodeKind::Reasoning,
        input_schema: review.input_schema().cloned(),
        outcomes: vec![FlowOutcomeDefinition::new(
          key("checked"),
          FlowOutcomeKind::Success,
          assessment_schema.as_ref().unwrap().reference().clone(),
        )],
        budget: BudgetLimit::new(1, plan_elapsed / 2, 10, 10, 10000).unwrap(),
        permissions: reviewer.project_permissions.clone(),
        required: true,
        subflow: None,
      })
      .unwrap()
      .with_build(binding)
      .unwrap()
      .with_input_binding(review.input_binding().unwrap().clone())
      .unwrap()
    } else {
      review
    };
    definition.nodes.push(review);
    definition.transitions[2] = FlowTransition::new(
      key("freeze_independent_checks"),
      key("planned"),
      FlowTransitionTarget::Node(key("consume_frozen_review")),
    );
    definition.transitions.push(FlowTransition::new(
      key("consume_frozen_review"),
      key("checked"),
      FlowTransitionTarget::Terminal(key("finished")),
    ));
    definition.terminals[1] = FlowTerminalDefinition::new(
      key("finished"),
      assessment_schema
        .as_ref()
        .unwrap_or(&initial_schema)
        .reference()
        .clone(),
    );
  }
  let definition = FlowDefinition::new(definition).unwrap();
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
  .with_data_schemas({
    let mut schemas = vec![initial_schema, evidence_schema, plan_schema, escalation_schema];
    if let Some(schema) = review_schema {
      schemas.push(schema);
    }
    if let Some(schema) = assessment_schema {
      schemas.push(schema);
    }
    if !complete_projection {
      schemas.push(planner_schema);
    }
    schemas
  })
  .unwrap();
  let store = Arc::new(store_testing::InMemoryFactoryConfigurationStore::new());
  let audit = store_testing::management_mutation_with_request((), "review-admission")
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
  Fixture {
    store,
    run: run.id(),
    claim: claim.id,
    manifest,
    profile,
    evidence_outcome,
    probe: probe.is_some(),
    reviewer,
  }
}
fn planner_identity(profile: &FactoryReviewPlanningProfile) -> ImmutableReference {
  ImmutableReference::new(key("installed-planner"), key("v1"), profile.digest().unwrap())
}
fn executor(h: &Fixture, trusted: bool) -> Arc<FactoryNodeExecutorRegistry> {
  executor_with_clock(h, trusted, crate::factory_node_test_support::clock())
}
fn executor_with_clock(
  h: &Fixture,
  trusted: bool,
  clock: Arc<dyn FactoryExecutionClock>,
) -> Arc<FactoryNodeExecutorRegistry> {
  let producer = Arc::new(FactoryFlowActionAdapter::new(Arc::new(EvidenceAction {
    manifest: h.manifest.clone(),
    outcome: h.evidence_outcome.clone(),
    trusted,
  })));
  let planner = Arc::new(FactoryFlowActionAdapter::new(Arc::new(
    FactoryReviewPlanningAction::new(planner_identity(&h.profile), h.store.clone(), h.profile.clone(), clock),
  )));
  let mut bindings: Vec<FactoryNodeExecutorBinding> = vec![
    (
      FlowNodeKind::TrustedAction,
      NodeExecutionIdentity::External(reference("qualified-evidence")),
      producer,
    ),
    (
      FlowNodeKind::TrustedAction,
      NodeExecutionIdentity::External(planner_identity(&h.profile)),
      planner,
    ),
  ];
  if h.probe {
    bindings.push((
      if h.reviewer.is_some() {
        FlowNodeKind::Reasoning
      } else {
        FlowNodeKind::TrustedAction
      },
      NodeExecutionIdentity::External(
        h.reviewer
          .as_ref()
          .map_or_else(|| reference("review-probe"), |p| p.toolchain.executable.clone()),
      ),
      Arc::new(ReviewProbe),
    ));
  }
  Arc::new(FactoryNodeExecutorRegistry::new(bindings).unwrap())
}
#[test]
fn review_planning_retains_one_complete_plan_and_replays_it_after_owner_restart() {
  crate::factory_admission_tests::run_ready(async {
    let h = fixture();
    let executors = executor(&h, true);
    let runner = FactoryFlowRunner::new(
      h.store.clone(),
      executors.clone(),
      Arc::new(Inputs),
      crate::factory_node_test_support::clock(),
    );
    let mut completed = false;
    for time in 11..30 {
      runner.reconcile_flow(h.run, h.claim, at(time)).await.unwrap();
      if h
        .store
        .factory_run_snapshot(h.run)
        .await
        .unwrap()
        .flow
        .data
        .records
        .len()
        == 2
      {
        completed = true;
        break;
      }
    }
    assert!(completed, "configured evidence and planning nodes must complete");
    let snapshot = h.store.factory_run_snapshot(h.run).await.unwrap();
    assert_eq!(snapshot.flow.data.records.len(), 2);
    let record = snapshot
      .flow
      .data
      .records
      .iter()
      .find(|r| r.discovery().is_some_and(|f| f.kind() == FlowDiscoveryKind::ReviewPlan))
      .expect("trusted plan fact");
    assert!(
      record.payload().value().is_object(),
      "complete plan is a structured value"
    );
    let plan: EvaluationPlan = serde_json::from_value(record.payload().value().clone()).unwrap();
    assert_eq!(plan.evidence_id(), h.manifest.id());
    assert_eq!(plan.subject(), h.manifest.subject());
    assert_eq!(plan.purpose(), ReviewPurpose::CodeReview);
    assert_eq!(plan.required_quorum(), 1);
    assert_eq!(plan.branches().len(), 1);
    assert_eq!(plan.branches()[0].key(), &key("independent"));
    assert_eq!(plan.criterion_packs(), h.profile.criterion_packs);
    assert_eq!(plan.budget(), budget());
    let completion = snapshot
      .flow
      .completions
      .iter()
      .find(|row| row.node_attempt_id() == record.node_attempt_id())
      .unwrap();
    assert!(
      completion.usage().output_bytes > 100,
      "published full plan must charge its measured output"
    );
    assert!(completion.usage().output_bytes <= budget().max_output_bytes());
    let target = FactoryNodeTarget {
      flow_run_id: record.input().flow_run_id(),
      cycle_id: record.input().cycle_id(),
      node: record.input().node().clone(),
    };
    assert_eq!(record.input().sources().len(), 1);
    let restarted = FactoryNodeRunner::new(h.store.clone(), executors, crate::factory_node_test_support::clock());
    assert_eq!(
      restarted.run_once(h.run, h.claim, target, None, at(40)).await.unwrap(),
      FactoryNodeStep::Completed(Box::new(record.clone()))
    );
    assert_eq!(
      h.store.factory_run_snapshot(h.run).await.unwrap().flow.data.records,
      snapshot.flow.data.records
    );
  });
}

#[test]
fn identical_evidence_json_without_trusted_facts_cannot_create_a_review_plan() {
  crate::factory_admission_tests::run_ready(async {
    let h = fixture();
    let runner = FactoryFlowRunner::new(
      h.store.clone(),
      executor(&h, false),
      Arc::new(Inputs),
      crate::factory_node_test_support::clock(),
    );
    let mut rejected = false;
    for time in 11..30 {
      match runner.reconcile_flow(h.run, h.claim, at(time)).await {
        Err(ApplicationError::InvalidInput) => {
          rejected = true;
          break;
        }
        Ok(_) => {}
        Err(other) => panic!("unexpected failure {other:?}"),
      }
    }
    assert!(
      rejected,
      "a model-shaped evidence document must not establish trusted provenance"
    );
    let snapshot = h.store.factory_run_snapshot(h.run).await.unwrap();
    assert_eq!(snapshot.flow.data.records.len(), 1, "no review result is committed");
    assert_eq!(
      snapshot.flow.data.records[0].payload().value(),
      &serde_json::to_value(&h.manifest).unwrap()
    );
    assert!(snapshot.flow.data.records[0].discovery().is_none());
  });
}

#[test]
fn unavailable_required_evaluator_retains_a_configured_escalation_without_review_dispatch() {
  crate::factory_admission_tests::run_ready(async {
    let h = fixture_with_capability(false);
    let executors = executor(&h, true);
    let runner = FactoryFlowRunner::new(
      h.store.clone(),
      executors.clone(),
      Arc::new(Inputs),
      crate::factory_node_test_support::clock(),
    );
    let mut completed = false;
    for time in 11..30 {
      runner
        .reconcile_flow(h.run, h.claim, at(time))
        .await
        .expect("missing capability must publish a configured escalation, not retry an unavailable evaluator");
      if h
        .store
        .factory_run_snapshot(h.run)
        .await
        .unwrap()
        .flow
        .data
        .records
        .len()
        == 2
      {
        completed = true;
        break;
      }
    }
    assert!(completed);
    let snapshot = h.store.factory_run_snapshot(h.run).await.unwrap();
    let record = snapshot
      .flow
      .data
      .records
      .iter()
      .find(|r| r.input().node() == &key("freeze_independent_checks"))
      .expect("retained escalation");
    assert_eq!(record.metadata().unwrap()["outcome"], serde_json::json!("escalated"));
    assert_eq!(
      record.payload().value(),
      &serde_json::json!({"reason":"required_capability_unavailable","branch":"independent"})
    );
    assert!(
      record.discovery().is_none(),
      "escalation cannot advertise a dispatchable plan"
    );
    let target = FactoryNodeTarget {
      flow_run_id: record.input().flow_run_id(),
      cycle_id: record.input().cycle_id(),
      node: record.input().node().clone(),
    };
    let restarted = FactoryNodeRunner::new(h.store.clone(), executors, crate::factory_node_test_support::clock());
    assert_eq!(
      restarted.run_once(h.run, h.claim, target, None, at(40)).await.unwrap(),
      FactoryNodeStep::Completed(Box::new(record.clone()))
    );
    assert_eq!(
      h.store.factory_run_snapshot(h.run).await.unwrap().flow.data.records,
      snapshot.flow.data.records
    );
  });
}

async fn assert_selected_input_is_rejected(h: &Fixture) -> FactoryRunSnapshot {
  let runner = FactoryFlowRunner::new(
    h.store.clone(),
    executor(h, true),
    Arc::new(Inputs),
    crate::factory_node_test_support::clock(),
  );
  let mut rejected = false;
  for time in 11..30 {
    match runner.reconcile_flow(h.run, h.claim, at(time)).await {
      Err(ApplicationError::InvalidInput) => {
        rejected = true;
        break;
      }
      Ok(_) => {}
      Err(other) => panic!("unexpected failure {other:?}"),
    }
  }
  assert!(rejected, "planner must refuse unqualified input or output authority");
  let snapshot = h.store.factory_run_snapshot(h.run).await.unwrap();
  assert_eq!(
    snapshot.flow.data.records.len(),
    1,
    "only producing evidence remains retained"
  );
  snapshot
}
#[test]
fn a_partial_input_projection_does_not_authorize_reading_its_full_source_record() {
  crate::factory_admission_tests::run_ready(async {
    let h = fixture_with_limits(true, 10_000, false);
    let snapshot = assert_selected_input_is_rejected(&h).await;
    let input = snapshot
      .flow
      .data
      .inputs
      .iter()
      .find(|row| row.node() == &key("freeze_independent_checks"))
      .unwrap();
    assert_eq!(
      input.sources().len(),
      1,
      "valid source provenance does not widen the selected view"
    );
    assert_eq!(
      input.payload().value(),
      &serde_json::to_value(h.manifest.subject()).unwrap()
    );
    assert!(snapshot.flow.data.records[0].discovery().is_some());
  });
}
#[test]
fn a_plan_cannot_publish_more_bytes_than_its_frozen_output_budget() {
  crate::factory_admission_tests::run_ready(async {
    let h = fixture_with_limits(true, 16, true);
    assert_eq!(h.profile.policy.budget().max_output_bytes(), 16);
    let snapshot = assert_selected_input_is_rejected(&h).await;
    assert!(snapshot.flow.data.records[0].discovery().is_some());
  });
}

#[test]
fn a_frozen_review_deadline_cannot_outlive_its_required_evidence() {
  crate::factory_admission_tests::run_ready(async {
    let h = fixture_with_freshness(true, 10_000, true, 80);
    let runner = FactoryFlowRunner::new(
      h.store.clone(),
      executor(&h, true),
      Arc::new(Inputs),
      crate::factory_node_test_support::clock(),
    );
    for time in 11..30 {
      runner.reconcile_flow(h.run, h.claim, at(time)).await.unwrap();
      if h
        .store
        .factory_run_snapshot(h.run)
        .await
        .unwrap()
        .flow
        .data
        .records
        .len()
        == 2
      {
        break;
      }
    }
    let snapshot = h.store.factory_run_snapshot(h.run).await.unwrap();
    let record = snapshot
      .flow
      .data
      .records
      .iter()
      .find(|r| r.discovery().is_some_and(|f| f.kind() == FlowDiscoveryKind::ReviewPlan))
      .unwrap();
    let plan: EvaluationPlan = serde_json::from_value(record.payload().value().clone()).unwrap();
    assert_eq!(
      plan.deadline(),
      at(80),
      "authority ends at the original evidence freshness boundary"
    );
    let attempt = snapshot
      .flow
      .attempts
      .iter()
      .find(|row| row.id() == record.node_attempt_id())
      .unwrap();
    assert_eq!(plan.created_at(), attempt.started_at());
    assert!(plan.deadline() <= attempt.deadline());
  });
}

#[test]
fn evidence_expiring_during_snapshot_observation_cannot_publish_a_plan() {
  struct CurrentClock;
  impl FactoryExecutionClock for CurrentClock {
    fn now(&self) -> Result<Timestamp, ApplicationError> {
      Ok(at(2000))
    }
  }
  crate::factory_admission_tests::run_ready(async {
    let h = fixture();
    let runner = FactoryFlowRunner::new(
      h.store.clone(),
      executor_with_clock(&h, true, Arc::new(CurrentClock)),
      Arc::new(Inputs),
      crate::factory_node_test_support::clock(),
    );
    let mut rejected = false;
    for time in 11..30 {
      match runner.reconcile_flow(h.run, h.claim, at(time)).await {
        Err(ApplicationError::InvalidInput) => {
          rejected = true;
          break;
        }
        Ok(_) => {}
        Err(other) => panic!("unexpected failure {other:?}"),
      }
    }
    assert!(
      rejected,
      "freshness must be checked with trusted time after the asynchronous snapshot read"
    );
    assert_eq!(
      h.store
        .factory_run_snapshot(h.run)
        .await
        .unwrap()
        .flow
        .data
        .records
        .len(),
      1
    );
  });
}

#[test]
fn a_larger_node_window_cannot_reopen_an_expired_review_policy_window() {
  struct CurrentClock;
  impl FactoryExecutionClock for CurrentClock {
    fn now(&self) -> Result<Timestamp, ApplicationError> {
      Ok(at(1100))
    }
  }
  crate::factory_admission_tests::run_ready(async {
    let h = fixture();
    let runner = FactoryFlowRunner::new(
      h.store.clone(),
      executor_with_clock(&h, true, Arc::new(CurrentClock)),
      Arc::new(Inputs),
      crate::factory_node_test_support::clock(),
    );
    let mut rejected = false;
    for time in 11..30 {
      match runner.reconcile_flow(h.run, h.claim, at(time)).await {
        Err(ApplicationError::InvalidInput) => {
          rejected = true;
          break;
        }
        Ok(_) => {}
        Err(other) => panic!("unexpected failure {other:?}"),
      }
    }
    assert!(
      rejected,
      "the original installed policy window must end authority even while node, claim and evidence remain valid"
    );
    let snapshot = h.store.factory_run_snapshot(h.run).await.unwrap();
    assert_eq!(snapshot.flow.data.records.len(), 1);
    let attempt = snapshot
      .flow
      .attempts
      .iter()
      .find(|a| a.node_key() == &key("freeze_independent_checks"))
      .unwrap();
    assert!(at(1100) < attempt.deadline());
    assert!(at(1100) < h.manifest.items()[0].fresh_until());
  });
}

struct ReviewProbe;
#[async_trait]
impl FactoryNodeExecutor for ReviewProbe {
  async fn observe(&self, _: FactoryNodeExecutionRequest<'_>) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    Ok(FactoryNodeExecutionStep::Waiting { execution: None })
  }
  async fn cancel(
    &self,
    _: FactoryNodeExecutionRequest<'_>,
    _: Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    Ok(FactoryNodeCancellationStep::Stopped {
      execution: None,
      usage: BudgetUsage::default(),
    })
  }
}
/// Real evidence/planning actions and owner-frozen direct consumer selections.
pub(crate) async fn prepared_review_invocation(
  full: bool,
) -> (
  Arc<store_testing::InMemoryFactoryConfigurationStore>,
  FlowNodeInput,
  ImmutableReference,
) {
  let h = fixture_with_review_probe(true, 10_000, true, 2000, Some(full), false);
  let runner = FactoryFlowRunner::new(
    h.store.clone(),
    executor(&h, true),
    Arc::new(Inputs),
    crate::factory_node_test_support::clock(),
  );
  for time in 11..40 {
    runner.reconcile_flow(h.run, h.claim, at(time)).await.unwrap();
    let snapshot = h.store.factory_run_snapshot(h.run).await.unwrap();
    if let Some(input) = snapshot
      .flow
      .data
      .inputs
      .iter()
      .find(|input| input.node() == &key("consume_frozen_review"))
    {
      return (
        h.store.clone(),
        input.clone(),
        h.profile.branches[0].evaluator().clone(),
      );
    }
  }
  panic!("qualified planning must yield its actual owner-prepared consumer input");
}
#[test]
fn review_fixture_retains_both_explicit_sources_without_fabricating_an_evaluator_result() {
  crate::factory_admission_tests::run_ready(async {
    let (store, input, evaluator) = prepared_review_invocation(true).await;
    assert_eq!(input.sources().len(), 2);
    let snapshot = store.factory_run_snapshot(input.factory_run_id()).await.unwrap();
    assert_eq!(snapshot.flow.data.records.len(), 2);
    let plan: EvaluationPlan = serde_json::from_value(input.payload().value()["plan"].clone()).unwrap();
    let evidence: EvidenceManifest = serde_json::from_value(input.payload().value()["evidence"].clone()).unwrap();
    assert_eq!(plan.evidence_id(), evidence.id());
    assert!(plan.has_evaluator(&evaluator));
  });
}

fn reviewer_profile(work: &WorkEnvelope) -> FactoryCodexBuildProfile {
  let mut grants = serde_json::to_value(crate::factory_job_test_support::permissions(false)).unwrap();
  for mount in grants["mounts"].as_array_mut().unwrap() {
    if mount["root"] == "/workspace/source" {
      mount["mode"] = serde_json::json!("read_only");
    }
  }
  grants["outputs"]["kinds"] = serde_json::json!(["codex-run-trace", "codex-run-provenance", "codex-run-result"]);
  let grants: FactoryPermissionSet = serde_json::from_value(grants).unwrap();
  FactoryCodexBuildProfile {
    authentication: crate::FactoryCodexAuthentication::default(),
    mcp_configurations: vec![],
    toolchain: FactoryTaskToolchain {
      plugin: crate::factory_job_test_support::reference("codex", "0.5.0"),
      executable: crate::factory_job_test_support::reference("codex-cli", "0.116.0"),
      model: reference("installed-review-model"),
    },
    build_configuration: crate::factory_node_test_support::fixtures::binding(work.subject().project_id())
      .build_configuration,
    instructions: FactoryText::new("Evaluate only the selected criterion policy and actual readonly candidate.")
      .unwrap(),
    credentials: FactoryCredentialProfiles::new(
      key("author-only"),
      key("model-coding"),
      key("source-only"),
      key("delivery-only"),
    )
    .unwrap(),
    consumer: FactoryCredentialConsumer::ReadOnlyReview,
    usage_policy: FactoryCodexUsagePolicy::ReportedCost,
    tool_risk: None,
    tool_response_limits: None,
    candidate_capture_tool: Some(reference("installed-capture")),
    capture: None,
    reviewer: Some(FactoryCodexReviewerPolicy {
      evaluator: reference("independent-reviewer"),
    }),
    pipeline_node: octacity_server_domain::PipelineNodeId::new("installed-review-job").unwrap(),
    parameters: BTreeMap::new(),
    project_permissions: grants.clone(),
    local_permissions: LocalPermissionCeiling::fully_enforced(grants),
  }
}
pub(crate) async fn prepared_codex_review_invocation() -> (
  Arc<store_testing::InMemoryFactoryConfigurationStore>,
  FlowBuildIntent,
  AdmittedFlow,
  FactoryCodexBuildProfile,
  BTreeMap<ArtifactId, Vec<u8>>,
) {
  struct ReviewInputs(CandidateSubject, CriterionPack);
  #[async_trait]
  impl FactoryFlowInputPreparer for ReviewInputs {
    async fn prepare(
      &self,
      snapshot: &FactoryRunSnapshot,
      target: &FactoryNodeTarget,
    ) -> Result<FlowNodeInput, ApplicationError> {
      let input = prepare_factory_flow_input(snapshot, target)?;
      if input.node() != &key("consume_frozen_review") {
        return Ok(input);
      }
      let subject = FactoryTaskSubject::Candidate(self.0.clone());
      input
        .with_context(
          ContextManifest::new(
            ContextManifestId::generate(),
            subject.clone(),
            FactoryDigest::from_bytes([3; 32]),
            vec![
              ContextManifestEntry::new(
                ContextSourceKind::Policy,
                self.1.reference().identity().clone(),
                subject,
                FactoryContextReference::Artifact(self.1.artifact().clone()),
                self.1.artifact().content_digest(),
                self.1.artifact().encoded_size(),
                FactorySafeText::new("Explicit installed criterion pack").unwrap(),
                self.1.reference().digest(),
              )
              .unwrap(),
            ],
          )
          .unwrap(),
          vec![],
        )
        .map_err(|_| ApplicationError::invalid())
    }
  }
  let h = fixture_with_review_probe(true, 10_000, true, 2000, Some(true), true);
  let reviewer = h.reviewer.as_ref().unwrap().clone();
  let pack = h.profile.criterion_packs[0].clone();
  let context = BTreeMap::from([(
    pack.artifact().artifact_id(),
    b"Review the actual candidate against the selected deterministic evidence.".to_vec(),
  )]);
  let runner = FactoryFlowRunner::new(
    h.store.clone(),
    executor(&h, true),
    Arc::new(ReviewInputs(h.manifest.subject().clone(), pack)),
    crate::factory_node_test_support::clock(),
  );
  for time in 11..40 {
    runner.reconcile_flow(h.run, h.claim, at(time)).await.unwrap();
    let snapshot = h.store.factory_run_snapshot(h.run).await.unwrap();
    if let Some(intent) = snapshot.flow.data.build_intents.first() {
      return (
        h.store.clone(),
        intent.clone(),
        snapshot.admitted_flow,
        reviewer,
        context,
      );
    }
  }
  panic!("the owner must freeze a real readonly evaluator intent");
}
#[test]
fn review_fixture_freezes_a_real_readonly_evaluator_intent_with_only_the_selected_criterion_pack() {
  crate::factory_admission_tests::run_ready(async {
    let (runs, intent, admitted, profile, bytes) = prepared_codex_review_invocation().await;
    let proof = FactoryStoredReviewInvocationSource::new(runs.clone())
      .invocation(intent.input(), &profile.reviewer.as_ref().unwrap().evaluator, at(31))
      .await
      .unwrap();
    assert_eq!(intent.node().node_kind(), FlowNodeKind::Reasoning);
    assert!(intent.budget().fits_within(proof.plan().budget()));
    assert!(intent.node().deadline() <= proof.plan().deadline());
    assert_eq!(bytes.len(), 1);
    assert_eq!(intent.input().context().unwrap().entries().len(), 1);
    assert_eq!(
      FactoryDigest::content_sha256(bytes.values().next().unwrap()),
      proof.plan().criterion_packs()[0].artifact().content_digest()
    );
    assert_eq!(
      runs
        .factory_run_snapshot(intent.input().factory_run_id())
        .await
        .unwrap()
        .flow
        .data
        .records
        .len(),
      2
    );
    assert_eq!(
      admitted
        .data_schema(intent.input().payload().schema())
        .unwrap()
        .reference(),
      intent.input().payload().schema()
    );
  });
}

/// Reuses installed contracts while each caller executes its own authoritative journal.
pub(crate) fn capture_review_contract() -> (
  FactoryRunSnapshot,
  FactoryReviewPlanningProfile,
  FactoryCodexBuildProfile,
) {
  let fixture = fixture_with_review_elapsed(true, 10_000, true, 2000, Some(true), true, 2000);
  let snapshot = crate::factory_admission_tests::run_ready(fixture.store.factory_run_snapshot(fixture.run)).unwrap();
  (snapshot, fixture.profile, fixture.reviewer.unwrap())
}
