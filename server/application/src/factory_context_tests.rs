use octacity_server_domain::{ArtifactId, ImmutableRevision, ProjectId, RepositoryId, Timestamp};
use octacity_server_factory::{
  AdmittedFlow, BoundedSummary, BudgetLimit, ContextManifest, ContextManifestEntry, ContextManifestId,
  ContextSourceKind, ExactSubject, ExternalWorkIdentity, FactoryArtifactReference, FactoryClaim, FactoryClaimFence,
  FactoryClaimOwnership, FactoryConfigurationId, FactoryConfigurationRef, FactoryConfigurationVersion,
  FactoryContextReference, FactoryDigest, FactoryKey, FactoryMetadata, FactoryRepositoryPath, FactoryRepositoryRange,
  FactoryRun, FactoryRunId, FactorySafeText, FactoryStageTarget, FactoryTaskSubject, FlowDefinition, FlowDefinitionId,
  FlowDefinitionInput, FlowDefinitionVersion, FlowExecutionPolicy, FlowNodeDefinition, FlowNodeDefinitionInput,
  FlowNodeKind, FlowOutcomeDefinition, FlowOutcomeKind, FlowRun, FlowRunId, FlowTerminalDefinition, FlowTransition,
  FlowTransitionTarget, ImmutableReference, MAX_CONTEXT_ENTRY_BYTES, MAX_MACRO_CALL_DEPTH, MacroCall,
  MacroCallCompletion, MacroCallDeclaration, MacroCallId, MacroCallKind, NodeAttempt, PinnedFlowDefinitionClosure,
  RepositoryFragment, RetrievalReceipt, RetrievalReceiptId, RiskClass, StageAttempt, StageAttemptId,
  StageAttemptNumber, StageHandoff, StageHandoffContent, StageHandoffDeclaration, StageHandoffId, StageHandoffOutcome,
  StageHandoffReferences, TaskEnvelopeId, WorkArtifacts, WorkClassification, WorkEnvelope, WorkEnvelopeId,
  WorkPriority, WorkflowCycle,
};
use octacity_server_store::{FactoryFlowHistory, FactoryRunCurrentProjection, FactoryRunSnapshot};
use uuid::Uuid;

use crate::{FactoryContextError, FactoryContextSelection, PrepareFactoryCallContext, prepare_factory_call_context};

fn digest(byte: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([byte; 32])
}

fn id<T: FixtureId>(value: u128) -> T {
  T::from_uuid(Uuid::from_u128(value))
}

trait FixtureId {
  fn from_uuid(value: Uuid) -> Self;
}

macro_rules! fixture_id {
  ($($kind:ty),+ $(,)?) => {
    $(impl FixtureId for $kind {
      fn from_uuid(value: Uuid) -> Self {
        <$kind>::from_uuid(value).expect("fixture identity")
      }
    })+
  };
}

fixture_id!(
  ArtifactId,
  ProjectId,
  RepositoryId,
  FactoryConfigurationId,
  WorkEnvelopeId,
  FactoryRunId,
  FlowDefinitionId,
  FlowRunId,
  StageAttemptId,
  StageHandoffId,
  ContextManifestId,
  MacroCallId,
  TaskEnvelopeId,
);

struct Fixture {
  snapshot: FactoryRunSnapshot,
  stage: StageAttempt,
  subject: FactoryTaskSubject,
}

fn fixture() -> Fixture {
  let exact = ExactSubject::new(
    id::<ProjectId>(1),
    id::<RepositoryId>(2),
    ImmutableRevision::new("base").unwrap(),
  );
  let configuration = FactoryConfigurationRef::new(
    id::<FactoryConfigurationId>(3),
    FactoryConfigurationVersion::INITIAL,
    exact.project_id(),
    digest(1),
  );
  let work = WorkEnvelope::new(
    id::<WorkEnvelopeId>(4),
    configuration,
    ExternalWorkIdentity::new("manual/context").unwrap(),
    exact.clone(),
    WorkArtifacts::new(
      FactoryArtifactReference::new(id::<ArtifactId>(5), digest(5), 5).unwrap(),
      FactoryArtifactReference::new(id::<ArtifactId>(6), digest(6), 6).unwrap(),
      (0_u128..5)
        .map(|offset| {
          FactoryArtifactReference::new(
            id::<ArtifactId>(60 + offset),
            digest(60 + u8::try_from(offset).unwrap()),
            MAX_CONTEXT_ENTRY_BYTES,
          )
          .unwrap()
        })
        .collect(),
    )
    .unwrap(),
    WorkClassification::new(
      WorkPriority::new(1).unwrap(),
      RiskClass::Low,
      FactoryMetadata::default(),
    ),
  )
  .unwrap();
  let run = FactoryRun::admitted(id::<FactoryRunId>(7), &work);
  let stage = StageAttempt::new(
    id::<StageAttemptId>(8),
    &run,
    StageAttemptNumber::INITIAL,
    FactoryStageTarget::Implementation,
    budget(),
    digest(8),
    FactoryClaimOwnership::new(
      FactoryKey::new("worker").unwrap(),
      FactoryClaim::new(
        FactoryClaimFence::new(digest(9)),
        Timestamp::from_unix_millis(1).unwrap(),
        Timestamp::from_unix_millis(100).unwrap(),
      )
      .unwrap(),
    ),
  );
  let subject = FactoryTaskSubject::Exact(exact);
  let result_schema = ImmutableReference::new(
    FactoryKey::new("result").unwrap(),
    FactoryKey::new("v1").unwrap(),
    digest(12),
  );
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: id::<FlowDefinitionId>(3),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: None,
    entry: FactoryKey::new("implement").unwrap(),
    nodes: vec![
      FlowNodeDefinition::new(FlowNodeDefinitionInput {
        key: FactoryKey::new("implement").unwrap(),
        kind: FlowNodeKind::BuildCommand,
        input_schema: None,
        outcomes: vec![FlowOutcomeDefinition::new(
          FactoryKey::new("succeeded").unwrap(),
          FlowOutcomeKind::Success,
          result_schema.clone(),
        )],
        budget: budget(),
        permissions: octacity_server_factory::FactoryPermissionSet::deny_all(),
        required: true,
        subflow: None,
      })
      .unwrap(),
    ],
    transitions: vec![FlowTransition::new(
      FactoryKey::new("implement").unwrap(),
      FactoryKey::new("succeeded").unwrap(),
      FlowTransitionTarget::Terminal(FactoryKey::new("succeeded").unwrap()),
    )],
    terminals: vec![FlowTerminalDefinition::new(
      FactoryKey::new("succeeded").unwrap(),
      result_schema,
    )],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(budget(), octacity_server_factory::FactoryPermissionSet::deny_all(), 1)
      .unwrap(),
  })
  .unwrap();
  let closure = PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap();
  let flow_run = FlowRun::root(&run, closure.root()).unwrap();
  let cycle = WorkflowCycle::initial(&flow_run).unwrap();
  let node = NodeAttempt::from_stage(stage.clone(), &flow_run, &cycle, FactoryKey::new("implement").unwrap()).unwrap();
  let root = closure.definition(closure.root()).unwrap();
  let limits = octacity_server_factory::FlowAdmissionLimits::product_defaults(
    root.execution().max_active_nodes(),
    root.execution().budget(),
    root.execution().permissions().clone(),
  )
  .unwrap();
  let admitted_flow = AdmittedFlow::new(closure, limits, flow_run.clone(), cycle.clone()).unwrap();
  let snapshot = FactoryRunSnapshot {
    work,
    run,
    admitted_flow,
    flow: FactoryFlowHistory {
      runs: vec![flow_run],
      cycles: vec![cycle],
      attempts: vec![node],
      completions: vec![],
    },
    current_claim: None,
    claims: vec![],
    budgets: vec![],
    lifecycle_checkpoints: vec![],
    stage_attempts: vec![stage.clone()],
    stage_attempt_completions: vec![],
    stage_handoffs: vec![],
    context_manifests: vec![],
    macro_calls: vec![],
    macro_call_completions: vec![],
    signal_requests: vec![],
    signal_receipts: vec![],
    linked_builds: vec![],
    build_observations: vec![],
    candidates: vec![],
    evidence: vec![],
    evaluation_plans: vec![],
    assessments: vec![],
    decisions: vec![],
    escalations: vec![],
    delivery_attempts: vec![],
    reporting_attempts: vec![],
    audit: vec![],
    controls: vec![],
    outbox: vec![],
    current: FactoryRunCurrentProjection {
      budget_id: digest(10),
      lifecycle_checkpoint_id: digest(11),
      stage_attempt_id: Some(stage.id()),
      macro_call_id: None,
      signal_request_id: None,
      signal_receipt_id: None,
      build_id: None,
      candidate_id: None,
      evidence_id: None,
      evaluation_plan_id: None,
      decision_id: None,
      escalation_id: None,
      delivery_attempt_id: None,
      reporting_attempt_id: None,
    },
  };
  Fixture {
    snapshot,
    stage,
    subject,
  }
}

fn budget() -> BudgetLimit {
  BudgetLimit::new(2, 10_000, 10_000, 10_000, 10_000).unwrap()
}

fn entry(
  kind: ContextSourceKind,
  identity: impl Into<String>,
  subject: &FactoryTaskSubject,
  artifact: u128,
  content_digest: FactoryDigest,
  encoded_size: u64,
) -> ContextManifestEntry {
  ContextManifestEntry::new(
    kind,
    FactoryKey::new(identity).unwrap(),
    subject.clone(),
    FactoryContextReference::Artifact(
      FactoryArtifactReference::new(id::<ArtifactId>(artifact), content_digest, encoded_size).unwrap(),
    ),
    content_digest,
    encoded_size,
    FactorySafeText::new("declared immutable input").unwrap(),
    digest(90),
  )
  .unwrap()
}

fn request(
  fixture: &Fixture,
  call_id: MacroCallId,
  selections: Vec<FactoryContextSelection>,
) -> PrepareFactoryCallContext {
  PrepareFactoryCallContext {
    call_id,
    stage_attempt_id: fixture.stage.id(),
    subject: fixture.subject.clone(),
    kind: MacroCallKind::Implement,
    parent_call_id: None,
    stage_dependencies: vec![],
    call_dependencies: vec![],
    construction_policy_digest: digest(20),
    budget: budget(),
    selections,
  }
}

#[test]
fn immutable_inputs_reproduce_the_same_ordered_manifest_and_digest() {
  let fixture = fixture();
  let call_id = id::<MacroCallId>(20);
  let task = FactoryContextSelection::Required(entry(
    ContextSourceKind::Task,
    "task",
    &fixture.subject,
    5,
    digest(5),
    5,
  ));
  let policy = FactoryContextSelection::Required(entry(
    ContextSourceKind::Policy,
    "factory-configuration",
    &fixture.subject,
    22,
    digest(1),
    22,
  ));

  let left = prepare_factory_call_context(
    &fixture.snapshot,
    request(&fixture, call_id, vec![task.clone(), policy.clone()]),
  )
  .unwrap();
  let right = prepare_factory_call_context(&fixture.snapshot, request(&fixture, call_id, vec![policy, task])).unwrap();

  assert_eq!(
    left.manifest.canonical_bytes().unwrap(),
    right.manifest.canonical_bytes().unwrap()
  );
  assert_eq!(left.manifest.digest().unwrap(), right.manifest.digest().unwrap());
  assert_eq!(left.call, right.call);
}

#[test]
fn required_work_input_must_reference_an_admitted_artifact() {
  let fixture = fixture();
  let result = prepare_factory_call_context(
    &fixture.snapshot,
    request(
      &fixture,
      id::<MacroCallId>(25),
      vec![FactoryContextSelection::Required(entry(
        ContextSourceKind::Task,
        "unadmitted-task",
        &fixture.subject,
        25,
        digest(25),
        25,
      ))],
    ),
  );

  assert_eq!(result.unwrap_err(), FactoryContextError::UndeclaredSelection);
}

#[test]
fn policy_and_repository_context_are_bound_to_the_admitted_subject() {
  let fixture = fixture();
  let wrong_policy = FactoryContextSelection::Required(entry(
    ContextSourceKind::Policy,
    "factory-configuration",
    &fixture.subject,
    26,
    digest(26),
    26,
  ));
  assert_eq!(
    prepare_factory_call_context(
      &fixture.snapshot,
      request(&fixture, id::<MacroCallId>(26), vec![wrong_policy]),
    )
    .unwrap_err(),
    FactoryContextError::UndeclaredSelection
  );

  let wrong_range = FactoryRepositoryRange::new(
    id::<RepositoryId>(27),
    fixture.subject.exact().base_revision().clone(),
    FactoryRepositoryPath::new("server/application/src/lib.rs").unwrap(),
    1,
    2,
  )
  .unwrap();
  assert_eq!(
    ContextManifestEntry::new(
      ContextSourceKind::RepositoryRange,
      FactoryKey::new("repository-range").unwrap(),
      fixture.subject.clone(),
      FactoryContextReference::RepositoryRange(wrong_range),
      digest(27),
      27,
      FactorySafeText::new("exact source range").unwrap(),
      digest(28),
    ),
    Err(octacity_server_factory::FactoryError::InconsistentSubject)
  );
}

#[test]
fn repository_fragments_require_a_future_concrete_producer_selection() {
  let fixture = fixture();
  let revision = fixture.subject.exact().base_revision().clone();
  let receipt = RetrievalReceipt::new(
    RetrievalReceiptId::generate(),
    fixture.subject.clone(),
    revision.clone(),
    immutable_reference("index", 80),
    immutable_reference("embedding", 81),
    FactorySafeText::new("find the bounded context contract").unwrap(),
    immutable_reference("retrieval-policy", 82),
    vec![
      RepositoryFragment::new(
        1,
        FactoryRepositoryRange::new(
          fixture.subject.exact().repository_id(),
          revision,
          FactoryRepositoryPath::new("server/core/src/lib.rs").unwrap(),
          1,
          20,
        )
        .unwrap(),
        FactoryArtifactReference::new(id::<ArtifactId>(83), digest(83), 83).unwrap(),
      )
      .unwrap(),
    ],
    digest(84),
  )
  .unwrap();
  let fragment = receipt.fragment_reference(1).unwrap();
  let entry = ContextManifestEntry::new(
    ContextSourceKind::RepositoryFragment,
    FactoryKey::new("repository-fragment").unwrap(),
    fixture.subject.clone(),
    FactoryContextReference::repository_fragment(fragment.clone()),
    fragment.artifact().content_digest(),
    fragment.artifact().encoded_size(),
    FactorySafeText::new("Supplemental untrusted repository context").unwrap(),
    receipt.digest().unwrap(),
  )
  .unwrap();

  let result = prepare_factory_call_context(
    &fixture.snapshot,
    request(
      &fixture,
      id::<MacroCallId>(85),
      vec![FactoryContextSelection::Required(entry)],
    ),
  );

  assert_eq!(result.unwrap_err(), FactoryContextError::UndeclaredSelection);
}

fn immutable_reference(identity: &str, byte: u8) -> ImmutableReference {
  ImmutableReference::new(
    FactoryKey::new(identity).unwrap(),
    FactoryKey::new("v1").unwrap(),
    digest(byte),
  )
}

#[test]
fn retry_and_lost_provider_session_reuse_the_frozen_manifest() {
  let mut fixture = fixture();
  let call_id = id::<MacroCallId>(30);
  let original = prepare_factory_call_context(
    &fixture.snapshot,
    request(
      &fixture,
      call_id,
      vec![FactoryContextSelection::Required(entry(
        ContextSourceKind::Task,
        "task",
        &fixture.subject,
        5,
        digest(5),
        5,
      ))],
    ),
  )
  .unwrap();
  fixture.snapshot.context_manifests.push(original.manifest.clone());
  fixture.snapshot.macro_calls.push(original.call.clone());

  let resumed = prepare_factory_call_context(
    &fixture.snapshot,
    request(
      &fixture,
      call_id,
      vec![FactoryContextSelection::Required(entry(
        ContextSourceKind::Task,
        "mutable-discovery-result",
        &fixture.subject,
        32,
        digest(32),
        32,
      ))],
    ),
  )
  .unwrap();

  assert!(resumed.reused);
  assert_eq!(resumed.manifest, original.manifest);
  assert_eq!(resumed.call, original.call);
}

#[test]
fn undeclared_sibling_output_cannot_enter_an_evaluator_context() {
  let mut fixture = fixture();
  let sibling_id = id::<MacroCallId>(40);
  let sibling = prepare_factory_call_context(
    &fixture.snapshot,
    request(
      &fixture,
      sibling_id,
      vec![FactoryContextSelection::Required(entry(
        ContextSourceKind::Task,
        "task",
        &fixture.subject,
        5,
        digest(5),
        5,
      ))],
    ),
  )
  .unwrap();
  fixture.snapshot.context_manifests.push(sibling.manifest);
  fixture.snapshot.macro_calls.push(sibling.call);

  let result = prepare_factory_call_context(
    &fixture.snapshot,
    request(
      &fixture,
      id::<MacroCallId>(42),
      vec![FactoryContextSelection::CallOutput {
        call_id: sibling_id,
        entry: entry(
          ContextSourceKind::TypedResult,
          sibling_id.to_string(),
          &fixture.subject,
          42,
          digest(42),
          42,
        ),
      }],
    ),
  );

  assert_eq!(result.unwrap_err(), FactoryContextError::UndeclaredSelection);
}

#[test]
fn declared_call_output_must_match_the_persisted_typed_result_exactly() {
  let mut fixture = fixture();
  let call_id = id::<MacroCallId>(43);
  let prepared = prepare_factory_call_context(
    &fixture.snapshot,
    request(
      &fixture,
      call_id,
      vec![FactoryContextSelection::Required(entry(
        ContextSourceKind::Task,
        "task",
        &fixture.subject,
        5,
        digest(5),
        5,
      ))],
    ),
  )
  .unwrap();
  let result = FactoryArtifactReference::new(id::<ArtifactId>(44), digest(44), 44).unwrap();
  let summary = BoundedSummary::new(
    fixture.subject.clone(),
    FactorySafeText::new("bounded child result").unwrap(),
    digest(45),
  );
  let summary_bytes = summary.canonical_bytes().unwrap();
  let summary_artifact = FactoryArtifactReference::new(
    id::<ArtifactId>(45),
    FactoryDigest::content_sha256(&summary_bytes),
    u64::try_from(summary_bytes.len()).unwrap(),
  )
  .unwrap();
  let completion: MacroCallCompletion = serde_json::from_value(serde_json::json!({
    "schema_version": 1,
    "call_id": call_id,
    "subject": fixture.subject,
    "task_envelope_id": id::<TaskEnvelopeId>(46),
    "task_envelope_digest": fixture.stage.input_digest(),
    "plugin": immutable_reference("codex", 46),
    "executable": immutable_reference("codex-cli", 47),
    "model": immutable_reference("model", 48),
    "prompt_digest": digest(49),
    "result_schema": "implementation_v1",
    "terminal": "succeeded",
    "usage": {"attempts": 1, "elapsed_millis": 1, "tokens": 1, "cost_micro_units": 0, "output_bytes": 1},
    "result": result,
    "summary": summary,
    "summary_artifact": summary_artifact,
    "trace": FactoryArtifactReference::new(id::<ArtifactId>(47), digest(47), 1).unwrap(),
    "provenance": FactoryArtifactReference::new(id::<ArtifactId>(48), digest(48), 1).unwrap(),
    "completed_at": Timestamp::from_unix_millis(50).unwrap(),
  }))
  .unwrap();
  fixture.snapshot.context_manifests.push(prepared.manifest);
  fixture.snapshot.macro_calls.push(prepared.call);
  fixture.snapshot.macro_call_completions.push(completion);

  let selection = ContextManifestEntry::new(
    ContextSourceKind::TypedResult,
    FactoryKey::new(call_id.to_string()).unwrap(),
    fixture.subject.clone(),
    FactoryContextReference::Artifact(result.clone()),
    result.content_digest(),
    result.encoded_size(),
    FactorySafeText::new("declared child result").unwrap(),
    digest(50),
  )
  .unwrap();
  let mut successor = request(
    &fixture,
    id::<MacroCallId>(49),
    vec![FactoryContextSelection::CallOutput {
      call_id,
      entry: selection.clone(),
    }],
  );
  successor.call_dependencies = vec![call_id];
  assert!(prepare_factory_call_context(&fixture.snapshot, successor.clone()).is_ok());

  let drifted = entry(
    ContextSourceKind::TypedResult,
    call_id.to_string(),
    &fixture.subject,
    44,
    digest(99),
    44,
  );
  successor.selections = vec![FactoryContextSelection::CallOutput {
    call_id,
    entry: drifted,
  }];
  assert_eq!(
    prepare_factory_call_context(&fixture.snapshot, successor).unwrap_err(),
    FactoryContextError::UndeclaredSelection
  );
}

#[test]
fn successor_can_select_only_the_declared_persisted_stage_handoff() {
  let mut fixture = fixture();
  let handoff = StageHandoff::new(
    StageHandoffDeclaration {
      id: id::<StageHandoffId>(50),
      stage_attempt_id: fixture.stage.id(),
      subject: fixture.subject.clone(),
      outcome: StageHandoffOutcome::Succeeded,
    },
    StageHandoffContent {
      summary: BoundedSummary::new(
        fixture.subject.clone(),
        FactorySafeText::new("implemented bounded change").unwrap(),
        digest(50),
      ),
      decisions: vec![],
      assumptions: vec![],
      unresolved_items: vec![],
      changed_components: vec![],
      validation_observations: vec![],
      prior_findings: vec![],
    },
    StageHandoffReferences {
      artifacts: vec![],
      changeset_id: None,
      evidence_manifest_id: None,
      result_digest: digest(51),
      policy_digest: digest(52),
      provenance_digest: digest(53),
    },
  )
  .unwrap();
  let bytes = handoff.canonical_bytes().unwrap();
  let selected = entry(
    ContextSourceKind::StageHandoff,
    handoff.id().to_string(),
    &fixture.subject,
    54,
    FactoryDigest::content_sha256(&bytes),
    u64::try_from(bytes.len()).unwrap(),
  );
  fixture.snapshot.stage_handoffs.push(handoff.clone());
  let mut declared = request(
    &fixture,
    id::<MacroCallId>(55),
    vec![FactoryContextSelection::StageHandoff {
      handoff_id: handoff.id(),
      entry: selected,
    }],
  );
  declared.stage_dependencies = vec![fixture.stage.id()];

  let prepared = prepare_factory_call_context(&fixture.snapshot, declared).unwrap();
  assert_eq!(prepared.call.stage_dependencies(), &[fixture.stage.id()]);
  assert_eq!(
    prepared.manifest.entries()[0].source_kind(),
    ContextSourceKind::StageHandoff
  );
}

#[test]
fn aggregate_context_bytes_and_call_depth_are_bounded() {
  let mut fixture = fixture();
  let oversized = (0_u128..5)
    .map(|offset| {
      FactoryContextSelection::Required(entry(
        ContextSourceKind::Specification,
        format!("large-{offset}"),
        &fixture.subject,
        60 + offset,
        digest(60 + u8::try_from(offset).unwrap()),
        MAX_CONTEXT_ENTRY_BYTES,
      ))
    })
    .collect();
  assert_eq!(
    prepare_factory_call_context(&fixture.snapshot, request(&fixture, id::<MacroCallId>(60), oversized)).unwrap_err(),
    FactoryContextError::InvalidContext
  );

  let mut parent = None;
  for depth in 1..=MAX_MACRO_CALL_DEPTH {
    let call_id = id::<MacroCallId>(100 + u128::from(depth));
    let manifest = ContextManifest::new(
      id::<ContextManifestId>(200 + u128::from(depth)),
      fixture.subject.clone(),
      digest(70),
      vec![entry(
        ContextSourceKind::Task,
        format!("depth-{depth}"),
        &fixture.subject,
        300 + u128::from(depth),
        digest(70),
        1,
      )],
    )
    .unwrap();
    let dependencies = parent.iter().map(MacroCall::id).collect::<Vec<_>>();
    let call = MacroCall::new(
      MacroCallDeclaration::new(
        call_id,
        fixture.subject.clone(),
        MacroCallKind::Summarize,
        budget(),
        vec![],
        dependencies,
        depth,
      )
      .unwrap(),
      &fixture.stage,
      &manifest,
      parent.as_ref(),
    )
    .unwrap();
    fixture.snapshot.context_manifests.push(manifest);
    fixture.snapshot.macro_calls.push(call.clone());
    parent = Some(call);
  }
  let parent = parent.unwrap();
  let mut too_deep = request(
    &fixture,
    id::<MacroCallId>(500),
    vec![FactoryContextSelection::CallOutput {
      call_id: parent.id(),
      entry: entry(
        ContextSourceKind::BoundedSummary,
        parent.id().to_string(),
        &fixture.subject,
        501,
        digest(71),
        1,
      ),
    }],
  );
  too_deep.parent_call_id = Some(parent.id());
  too_deep.call_dependencies = vec![parent.id()];
  assert_eq!(
    prepare_factory_call_context(&fixture.snapshot, too_deep).unwrap_err(),
    FactoryContextError::DepthExceeded
  );
}
