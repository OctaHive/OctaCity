use super::factory::*;

pub(crate) fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
pub(crate) fn reference(name: &str) -> ImmutableReference {
  ImmutableReference::new(key(name), key("v1"), FactoryDigest::from_bytes([60; 32]))
}
pub(crate) fn binding(project: octacity_server_domain::ProjectId) -> FlowBuildBinding {
  FlowBuildBinding {
    tool: reference("audit.command"),
    plugin: reference("runner"),
    model_or_tool: reference("audit.parameters"),
    task_digest: FactoryDigest::from_bytes([61; 32]),
    build_configuration: BuildConfigurationRef::new(
      octacity_server_domain::BuildConfigurationId::generate(),
      octacity_server_domain::BuildConfigurationVersion::INITIAL,
      project,
      FactoryDigest::from_bytes([62; 32]),
    ),
    result_output: key("audit.report"),
    result_output_kind: EvidenceOutputKind::Report,
    result_outcome: key("observed"),
    evidence: vec![],
  }
}
pub(crate) fn work() -> WorkEnvelope {
  let subject = ExactSubject::new(
    octacity_server_domain::ProjectId::generate(),
    octacity_server_domain::RepositoryId::generate(),
    octacity_server_domain::ImmutableRevision::new("base").unwrap(),
  );
  let artifact = |byte| {
    FactoryArtifactReference::new(
      octacity_server_domain::ArtifactId::generate(),
      FactoryDigest::from_bytes([byte; 32]),
      10,
    )
    .unwrap()
  };
  WorkEnvelope::new(
    WorkEnvelopeId::generate(),
    FactoryConfigurationRef::new(
      FactoryConfigurationId::generate(),
      FactoryConfigurationVersion::INITIAL,
      subject.project_id(),
      FactoryDigest::from_bytes([3; 32]),
    ),
    ExternalWorkIdentity::new("ticket").unwrap(),
    subject,
    WorkArtifacts::new(artifact(10), artifact(11), vec![]).unwrap(),
    WorkClassification::new(
      WorkPriority::new(1).unwrap(),
      RiskClass::Low,
      FactoryMetadata::default(),
    ),
  )
  .unwrap()
}

pub(crate) fn execution_fixture(
  kind: FlowNodeKind,
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  execution_fixture_with_evidence(kind, vec![])
}
pub(crate) fn execution_fixture_with_evidence(
  kind: FlowNodeKind,
  evidence: Vec<EvidenceRequirement>,
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  execution_fixture_with_pool(kind, evidence, None)
}
pub(crate) fn execution_fixture_with_pool(
  kind: FlowNodeKind,
  evidence: Vec<EvidenceRequirement>,
  pool: Option<FactoryKey>,
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  execution_fixture_for_work(kind, work(), evidence, pool)
}
pub(crate) fn execution_fixture_for_work(
  kind: FlowNodeKind,
  work: WorkEnvelope,
  evidence: Vec<EvidenceRequirement>,
  pool: Option<FactoryKey>,
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  execution_fixture_for_work_with_output_kind(kind, work, evidence, pool, EvidenceOutputKind::Report)
}

#[allow(dead_code)] // This shared fixture variation is exercised by application contracts.
pub(crate) fn execution_fixture_with_output_kind(
  kind: FlowNodeKind,
  output_kind: EvidenceOutputKind,
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  execution_fixture_for_work_with_output_kind(kind, work(), vec![], None, output_kind)
}

fn execution_fixture_for_work_with_output_kind(
  kind: FlowNodeKind,
  work: WorkEnvelope,
  evidence: Vec<EvidenceRequirement>,
  pool: Option<FactoryKey>,
  output_kind: EvidenceOutputKind,
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  let mut selection = binding(work.subject().project_id());
  selection.evidence = evidence;
  selection.result_output_kind = output_kind;
  execution_fixture_for_work_with_binding(kind, work, selection, pool, None)
}

pub(crate) fn execution_fixture_for_work_with_binding(
  kind: FlowNodeKind,
  work: WorkEnvelope,
  selection: FlowBuildBinding,
  pool: Option<FactoryKey>,
  output_schema: Option<FlowDataSchema>,
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  execution_fixture_with_retry_policy(kind, work, selection, pool, output_schema, 0)
}
#[allow(dead_code)] // Shared consumers opt into retry behavior explicitly.
pub(crate) fn execution_fixture_with_retries(
  kind: FlowNodeKind,
  maximum: u16,
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  execution_fixture_with_retries_and_pool(kind, maximum, None)
}
#[allow(dead_code)] // Shared owner contracts also exercise queued technical retries.
pub(crate) fn execution_fixture_with_retries_and_pool(
  kind: FlowNodeKind,
  maximum: u16,
  pool: Option<FactoryKey>,
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  let work = work();
  let selection = binding(work.subject().project_id());
  execution_fixture_with_retry_policy(kind, work, selection, pool, None, maximum)
}
#[allow(dead_code)] // Capability contracts select their own pinned output schema and retry policy.
pub(crate) fn execution_fixture_with_retry_policy(
  kind: FlowNodeKind,
  work: WorkEnvelope,
  selection: FlowBuildBinding,
  pool: Option<FactoryKey>,
  output_schema: Option<FlowDataSchema>,
  maximum: u16,
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  let limits = FlowAdmissionLimits::product_defaults(
    4,
    BudgetLimit::new(16, 10000, 1000, 10000, 10000 * (1 + u64::from(maximum))).unwrap(),
    FactoryPermissionSet::deny_all(),
  )
  .unwrap();
  let schema = FlowDataSchema::new(
    key("custom.observations"),
    key("v1"),
    FlowValueSchema::Object {
      fields: std::collections::BTreeMap::from([(
        key("count"),
        FlowFieldSchema::required(FlowValueSchema::Integer {
          minimum: 0,
          maximum: 100,
        }),
      )]),
    },
  )
  .unwrap();
  let budget = BudgetLimit::new(1, 1000, 100, 1000, 10000).unwrap();
  let output_schema = output_schema.unwrap_or_else(|| schema.clone());
  let mut node = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("accessibility_audit"),
    kind,
    input_schema: Some(schema.reference().clone()),
    outcomes: vec![FlowOutcomeDefinition::new(
      selection.result_outcome.clone(),
      FlowOutcomeKind::Success,
      output_schema.reference().clone(),
    )],
    budget,
    permissions: FactoryPermissionSet::deny_all(),
    required: true,
    subflow: None,
  })
  .unwrap()
  .with_build(selection.clone())
  .unwrap();
  node = node.with_execution_retries(maximum).unwrap();
  if let Some(pool) = pool {
    node = node.with_phase_pool(pool);
  }
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(schema.reference().clone()),
    entry: node.key().clone(),
    nodes: vec![node],
    transitions: vec![FlowTransition::new(
      key("accessibility_audit"),
      selection.result_outcome.clone(),
      FlowTransitionTarget::Terminal(key("done")),
    )],
    terminals: vec![FlowTerminalDefinition::new(
      key("done"),
      output_schema.reference().clone(),
    )],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(limits.budget(), FactoryPermissionSet::deny_all(), 1).unwrap(),
  })
  .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let flow = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    limits,
    flow,
    cycle,
    FactoryWipLimits::new(8, 16).unwrap(),
  )
  .unwrap()
  .with_data_schemas(if output_schema == schema {
    vec![schema.clone()]
  } else {
    vec![schema.clone(), output_schema.clone()]
  })
  .unwrap();
  let task_subject = FactoryTaskSubject::Exact(work.subject().clone());
  let task = work.artifacts().task().clone();
  let context_entry = ContextManifestEntry::new(
    ContextSourceKind::Task,
    key("task"),
    task_subject.clone(),
    FactoryContextReference::Artifact(task.clone()),
    task.content_digest(),
    task.encoded_size(),
    FactorySafeText::new("Declared task input").unwrap(),
    FactoryDigest::from_bytes([63; 32]),
  )
  .unwrap();
  let input = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    key("accessibility_audit"),
    FlowPayload::new(&schema, serde_json::json!({"count": 3})).unwrap(),
  )
  .unwrap()
  .with_context(
    ContextManifest::new(
      ContextManifestId::generate(),
      task_subject,
      FactoryDigest::from_bytes([63; 32]),
      vec![context_entry],
    )
    .unwrap(),
    vec![],
  )
  .unwrap();
  let at = octacity_server_domain::Timestamp::from_unix_millis(10).unwrap();
  let deadline = octacity_server_domain::Timestamp::from_unix_millis(100).unwrap();
  let attempt = NodeAttempt::new(
    admitted.root_run(),
    admitted.initial_cycle(),
    admitted.closure().definition(admitted.closure().root()).unwrap(),
    NodeAttemptInput {
      started_at: at,
      id: NodeAttemptId::generate(),
      node_key: key("accessibility_audit"),
      node_kind: kind,
      number: NodeAttemptNumber::INITIAL,
      input_digest: input.digest().unwrap(),
      budget,
      deadline,
      execution: NodeExecutionIdentity::External(selection.tool.clone()),
      ownership: FactoryClaimOwnership::new(
        key("worker"),
        FactoryClaim::new(
          FactoryClaimFence::new(FactoryDigest::from_bytes([64; 32])),
          at,
          deadline,
        )
        .unwrap(),
      ),
    },
  )
  .unwrap();
  (work, admitted, output_schema, input, attempt)
}
