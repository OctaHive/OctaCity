use super::factory::*;

#[allow(dead_code)] // Release-template contracts are shared by core and application owners.
pub(crate) fn supplied_triage_template() -> Result<FlowDefinitionTemplate, FactoryError> {
  let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .ancestors()
    .find(|path| path.join("config/factory").is_dir())
    .unwrap()
    .join("config/factory");
  let missing = || FactoryError::InvalidConfiguration {
    field: "versioned triage preset",
  };
  let manifest = std::fs::read(root.join("presets/triage/v1.json")).map_err(|_| missing())?;
  FlowDefinitionTemplate::from_manifest(&manifest, |path| {
    std::fs::read(root.join(path.as_str())).map_err(|_| missing())
  })
}

#[allow(dead_code)] // Shared core/application contracts load the supplied editable example.
pub(crate) fn supplied_base_flow_template() -> FlowDefinitionTemplate {
  supplied_flow_template("base-flow.json")
}
#[allow(dead_code)] // Common contracts also exercise the standalone nested verification example.
pub(crate) fn supplied_verification_flow_template() -> FlowDefinitionTemplate {
  supplied_flow_template("verification-flow.json")
}
fn supplied_flow_template(name: &str) -> FlowDefinitionTemplate {
  let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .ancestors()
    .find(|path| path.join("config/factory/flows").is_dir())
    .unwrap()
    .join("config/factory/flows");
  let manifest = std::fs::read(root.join(name)).unwrap();
  FlowDefinitionTemplate::from_manifest(&manifest, |path| {
    std::fs::read(root.join(path.as_str())).map_err(|_| FactoryError::InvalidConfiguration {
      field: "supplied configuration part",
    })
  })
  .unwrap()
}

pub(crate) fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
pub(crate) fn reference(name: &str) -> ImmutableReference {
  ImmutableReference::new(key(name), key("v1"), FactoryDigest::from_bytes([60; 32]))
}
pub(crate) fn binding(project: octacity_server_domain::ProjectId) -> FlowBuildBinding {
  FlowBuildBinding {
    tool: reference("audit.command"),
    result_tool: None,
    capture_write_paths: None,
    capture_deliverables: None,
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

#[allow(dead_code)] // Shared command/reasoning contracts opt into independent verification.
pub(crate) fn verification_execution_fixture(
  kind: FlowNodeKind,
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  verification_execution_fixture_with_permissions(kind, FactoryPermissionSet::deny_all())
}
#[allow(dead_code)] // Verification isolation tests vary the admitted mount authority.
pub(crate) fn verification_execution_fixture_with_permissions(
  kind: FlowNodeKind,
  permissions: FactoryPermissionSet,
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  verification_execution_fixture_with_schema(kind, permissions, None)
}
#[allow(dead_code)] // Typed verification reports select their own closed output schema.
pub(crate) fn verification_execution_fixture_with_schema(
  kind: FlowNodeKind,
  permissions: FactoryPermissionSet,
  output_schema: Option<FlowDataSchema>,
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  verification_fixture(kind, permissions, output_schema, None)
}
#[allow(dead_code)] // Endpoint authority contracts intentionally vary selected URLs.
pub(crate) fn verification_execution_fixture_with_endpoint(
  kind: FlowNodeKind,
  endpoint: &str,
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  verification_fixture(
    kind,
    FactoryPermissionSet::deny_all(),
    None,
    Some((endpoint, "stage.example.test")),
  )
}
#[allow(dead_code)] // Endpoint normalization tests use existing canonical IP grants.
pub(crate) fn verification_execution_fixture_with_endpoint_host(
  kind: FlowNodeKind,
  endpoint: &str,
  host: &str,
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  verification_fixture(kind, FactoryPermissionSet::deny_all(), None, Some((endpoint, host)))
}
fn verification_fixture(
  kind: FlowNodeKind,
  permissions: FactoryPermissionSet,
  output_schema: Option<FlowDataSchema>,
  endpoint: Option<(&str, &str)>,
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  let work = work();
  let selection = binding(work.subject().project_id());
  let (work, prior, schema, previous_input, previous) =
    execution_fixture_with_permissions(kind, work, selection, output_schema, permissions);
  let original = prior.closure().definition(prior.closure().root()).unwrap();
  let mut execution = serde_json::to_value(original.execution()).unwrap();
  execution["verification"] = serde_json::json!({
    "data_handling": reference("internal-only"), "required_quorum":1,
    "checks":{"accessibility_audit":{
      "class":"custom-contract", "source_visibility":"black_box", "endpoints":[],
      "secret_profiles":[], "identity_profiles":[], "required":true,
      "target":{"value":{"mapping":"object","fields":{
        "environment":{"mapping":"constant","value":reference("stage-v1")},
        "public_contract":{"mapping":"pointer","path":"/context/entries/0/source/value"},
        "subject":{"mapping":"pointer","path":"/context/subject/value"}
      }}}
    }}
  });
  if let Some((endpoint, host)) = endpoint {
    let check = &mut execution["verification"]["checks"]["accessibility_audit"];
    check["endpoints"] = serde_json::json!([host]);
    check["target"]["value"]["fields"]["endpoint"] = serde_json::json!({"mapping":"constant","value":endpoint});
  }
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: original.reference().id(),
    version: original.reference().version(),
    input_schema: original.input_schema().cloned(),
    entry: original.entry().clone(),
    nodes: original.nodes().to_vec(),
    transitions: original.transitions().to_vec(),
    terminals: original.terminals().to_vec(),
    context_projections: vec![],
    data_projections: vec![],
    execution: serde_json::from_value(execution).unwrap(),
  })
  .unwrap();
  let run = FactoryRun::admitted(prior.root_run().factory_run_id(), &work);
  let flow = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    prior.limits().clone(),
    flow,
    cycle,
    prior.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(prior.data_schemas().to_vec())
  .unwrap();
  let subject = FactoryTaskSubject::Candidate(CandidateSubject::new(
    work.subject().clone(),
    octacity_server_domain::ImmutableRevision::new("candidate").unwrap(),
    FactoryDigest::from_bytes([95; 32]),
  ));
  let contract = work.artifacts().acceptance().clone();
  let entry = ContextManifestEntry::new(
    ContextSourceKind::Task,
    key("public-contract"),
    subject.clone(),
    FactoryContextReference::Artifact(contract.clone()),
    contract.content_digest(),
    contract.encoded_size(),
    FactorySafeText::new("Explicit deployed contract").unwrap(),
    FactoryDigest::from_bytes([9; 32]),
  )
  .unwrap();
  let context = ContextManifest::new(
    ContextManifestId::generate(),
    subject,
    FactoryDigest::from_bytes([9; 32]),
    vec![entry],
  )
  .unwrap();
  let input = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    original.entry().clone(),
    previous_input.payload().clone(),
  )
  .unwrap()
  .with_context(context, vec![])
  .unwrap();
  let definition = admitted.closure().definition(input.definition()).unwrap();
  let attempt = NodeAttempt::new(
    admitted.root_run(),
    admitted.initial_cycle(),
    definition,
    NodeAttemptInput {
      id: NodeAttemptId::generate(),
      node_key: input.node().clone(),
      node_kind: kind,
      number: NodeAttemptNumber::INITIAL,
      input_digest: input.digest().unwrap(),
      budget: previous.budget(),
      started_at: previous.started_at(),
      deadline: previous.deadline(),
      execution: previous.execution().clone(),
      ownership: FactoryClaimOwnership::new(previous.owner().clone(), previous.claim()),
    },
  )
  .unwrap();
  (work, admitted, schema, input, attempt)
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
  execution_fixture_with_authority(
    kind,
    work,
    selection,
    pool,
    output_schema,
    ExecutionFixtureOptions {
      maximum,
      public_result: false,
    },
    FactoryPermissionSet::deny_all(),
  )
}
#[allow(dead_code)] // Application capability contracts vary admitted filesystem authority.
pub(crate) fn execution_fixture_with_permissions(
  kind: FlowNodeKind,
  work: WorkEnvelope,
  selection: FlowBuildBinding,
  output_schema: Option<FlowDataSchema>,
  permissions: FactoryPermissionSet,
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  execution_fixture_with_authority(
    kind,
    work,
    selection,
    None,
    output_schema,
    ExecutionFixtureOptions::default(),
    permissions,
  )
}

#[allow(dead_code)] // Read contracts vary explicit public disclosure of the same retained payload.
pub(crate) fn public_result_execution_fixture(
  schema: FlowDataSchema,
  public_result: bool,
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  let work = work();
  let selection = binding(work.subject().project_id());
  execution_fixture_with_authority(
    FlowNodeKind::Reasoning,
    work,
    selection,
    None,
    Some(schema),
    ExecutionFixtureOptions {
      maximum: 0,
      public_result,
    },
    FactoryPermissionSet::deny_all(),
  )
}

#[derive(Default)]
struct ExecutionFixtureOptions {
  maximum: u16,
  public_result: bool,
}

fn execution_fixture_with_authority(
  kind: FlowNodeKind,
  work: WorkEnvelope,
  selection: FlowBuildBinding,
  pool: Option<FactoryKey>,
  output_schema: Option<FlowDataSchema>,
  options: ExecutionFixtureOptions,
  permissions: FactoryPermissionSet,
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  let maximum = options.maximum;
  let limits = FlowAdmissionLimits::product_defaults(
    4,
    BudgetLimit::new(16, 10000, 1000, 10000, 10000 * (1 + u64::from(maximum))).unwrap(),
    permissions.clone(),
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
    outcomes: vec![
      FlowOutcomeDefinition::new(
        selection.result_outcome.clone(),
        FlowOutcomeKind::Success,
        output_schema.reference().clone(),
      )
      .with_public_result(options.public_result),
    ],
    budget,
    permissions: permissions.clone(),
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
    execution: FlowExecutionPolicy::new(limits.budget(), permissions, 1).unwrap(),
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

#[allow(dead_code)] // Shared promotion contracts use the same ordinary Build boundary.
pub(crate) fn promotion_execution_fixture() -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt)
{
  promotion_execution_fixture_with_schema(None)
}
#[allow(dead_code)]
pub(crate) fn promotion_execution_fixture_with_schema(
  output: Option<FlowDataSchema>,
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  promotion_execution_fixture_with_policy(output, |_| {})
}
#[allow(dead_code)]
pub(crate) fn promotion_execution_fixture_with_policy(
  output: Option<FlowDataSchema>,
  change: impl FnOnce(&mut serde_json::Value),
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  promotion_execution_fixture_for_work(work(), output, change)
}
#[allow(dead_code)] // Production authorization tests vary immutable admitted risk.
pub(crate) fn promotion_execution_fixture_for_work(
  work: WorkEnvelope,
  output: Option<FlowDataSchema>,
  change: impl FnOnce(&mut serde_json::Value),
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  let (work, old, schema, prior, previous) = execution_fixture_for_work(FlowNodeKind::BuildCommand, work, vec![], None);
  let original = old.closure().definition(prior.definition()).unwrap();
  let mut node = serde_json::to_value(&original.nodes()[0]).unwrap();
  let output = output.unwrap_or_else(|| schema.clone());
  node["outcomes"][0]["schema"] = serde_json::to_value(output.reference()).unwrap();
  let capability = reference("trusted.deployment");
  let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: vec![capability.clone()],
    ..Default::default()
  })
  .unwrap();
  node["permissions"] = serde_json::to_value(&permissions).unwrap();
  let mut execution = serde_json::to_value(original.execution()).unwrap();
  execution["permissions"] = serde_json::to_value(&permissions).unwrap();
  let mut limits = serde_json::to_value(old.limits()).unwrap();
  limits["permissions"] = serde_json::to_value(&permissions).unwrap();
  node["kind"] = serde_json::json!("trusted_action");
  node["build"]["tool"] = serde_json::to_value(&capability).unwrap();
  node["action"] = serde_json::json!({"plugin":capability,"action":"deploy_exact_candidate","parameters":{"plan":{
    "strategy":"operator.strategy", "environments":{"pilot":reference("exact.environment")},
    "allocation":{"basis_points":1000}, "observation_window_millis":50,
    "evidence_requirements":[], "required_gates":[],
    "criteria":"decision"
  }}});
  change(&mut node["action"]["parameters"]["plan"]);
  let mut gate = node.clone();
  gate["key"] = serde_json::json!("decision");
  gate["kind"] = serde_json::json!("deterministic_gate");
  gate["input_schema"] = serde_json::to_value(output.reference()).unwrap();
  gate["input_binding"] = serde_json::json!({"sources":{"result":{"source":"predecessor","view":"payload","path":""}},"mapping":{"value":{"mapping":"pointer","path":"/result"}}});
  gate.as_object_mut().unwrap().remove("action");
  gate.as_object_mut().unwrap().remove("build");
  gate["gate"] = serde_json::to_value(
    FlowGateProgram::new(vec![], key("observed"))
      .unwrap()
      .binding()
      .unwrap(),
  )
  .unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: original.input_schema().cloned(),
    entry: original.entry().clone(),
    nodes: vec![
      serde_json::from_value(node).unwrap(),
      serde_json::from_value(gate).unwrap(),
    ],
    transitions: vec![
      FlowTransition::new(
        original.entry().clone(),
        key("observed"),
        FlowTransitionTarget::Node(key("decision")),
      ),
      FlowTransition::new(
        key("decision"),
        key("observed"),
        FlowTransitionTarget::Terminal(key("done")),
      ),
    ],
    terminals: original
      .terminals()
      .iter()
      .map(|row| FlowTerminalDefinition::new(row.key().clone(), output.reference().clone()))
      .collect(),
    context_projections: vec![],
    data_projections: vec![],
    execution: serde_json::from_value(execution).unwrap(),
  })
  .unwrap();
  let flow = FlowRun::root(
    &FactoryRun::admitted(old.root_run().factory_run_id(), &work),
    definition.reference(),
  )
  .unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    serde_json::from_value(limits).unwrap(),
    flow,
    cycle,
    old.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(if schema == output {
    vec![schema.clone()]
  } else {
    vec![schema.clone(), output.clone()]
  })
  .unwrap();
  let candidate = CandidateSubject::new(
    work.subject().clone(),
    octacity_server_domain::ImmutableRevision::new("accepted-candidate").unwrap(),
    FactoryDigest::from_bytes([91; 32]),
  );
  let subject = FactoryTaskSubject::Candidate(candidate);
  let contract = work.artifacts().acceptance().clone();
  let entry = ContextManifestEntry::new(
    ContextSourceKind::Specification,
    key("public-contract"),
    subject.clone(),
    FactoryContextReference::Artifact(contract.clone()),
    contract.content_digest(),
    contract.encoded_size(),
    FactorySafeText::new("Exact public contract").unwrap(),
    FactoryDigest::from_bytes([7; 32]),
  )
  .unwrap();
  let input = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    prior.node().clone(),
    prior.payload().clone(),
  )
  .unwrap()
  .with_context(
    ContextManifest::new(
      ContextManifestId::generate(),
      subject,
      FactoryDigest::from_bytes([7; 32]),
      vec![entry],
    )
    .unwrap(),
    vec![],
  )
  .unwrap();
  let attempt = NodeAttempt::new(
    admitted.root_run(),
    admitted.initial_cycle(),
    admitted.closure().definition(input.definition()).unwrap(),
    NodeAttemptInput {
      id: NodeAttemptId::generate(),
      node_key: input.node().clone(),
      node_kind: FlowNodeKind::TrustedAction,
      number: NodeAttemptNumber::INITIAL,
      input_digest: input.digest().unwrap(),
      budget: previous.budget(),
      started_at: previous.started_at(),
      deadline: previous.deadline(),
      execution: NodeExecutionIdentity::External(capability),
      ownership: FactoryClaimOwnership::new(previous.owner().clone(), previous.claim()),
    },
  )
  .unwrap();
  (work, admitted, output, input, attempt)
}

/// Reuses an ordinary root-stage fixture with one complete, pinned role candidate.
#[allow(dead_code)] // Consumed by the application contract modules that share this fixture file.
pub(crate) fn with_selected_profile(
  work: &WorkEnvelope,
  admitted: &AdmittedFlow,
  input: &FlowNodeInput,
  candidate: FlowProfileCandidate,
) -> (AdmittedFlow, FlowNodeInput) {
  let root = admitted.closure().definition(input.definition()).unwrap();
  let mut node = serde_json::to_value(root.node(input.node()).unwrap()).unwrap();
  node.as_object_mut().unwrap().remove("build");
  node["profile_selection"] = serde_json::json!({"role":candidate.profile.role(),
    "candidates":{"chosen":candidate},"policy":{"mode":"manual","candidate":"chosen"},
    "failure":{"kind":"manual_selection"},"selection_budget":BudgetUsage::default()});
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: root.reference().id(),
    version: root.reference().version(),
    input_schema: root.input_schema().cloned(),
    entry: root.entry().clone(),
    nodes: vec![serde_json::from_value(node).unwrap()],
    transitions: root.transitions().to_vec(),
    terminals: root.terminals().to_vec(),
    context_projections: root.context_projections().to_vec(),
    data_projections: root.data_projections().to_vec(),
    execution: root.execution().clone(),
  })
  .unwrap();
  let run = FactoryRun::admitted(input.factory_run_id(), work);
  let flow = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    admitted.limits().clone(),
    flow,
    cycle,
    admitted.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(admitted.data_schemas().to_vec())
  .unwrap();
  let input = FlowNodeInput::new(
    work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    input.node().clone(),
    input.payload().clone(),
  )
  .unwrap()
  .with_context(input.context().unwrap().clone(), vec![])
  .unwrap();
  (admitted, input)
}
