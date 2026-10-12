use octacity_server_domain::Timestamp;
use uuid::Uuid;

use crate::{
  BudgetLimit, BudgetUsage, FactoryClaim, FactoryClaimFence, FactoryClaimOwnership, FactoryDigest, FactoryError,
  FactoryKey, FactoryPermissionSet, FactoryRunId, FlowAdmissionLimits, FlowContextProjection, FlowDataProjection,
  FlowDefinition, FlowDefinitionId, FlowDefinitionInput, FlowDefinitionRef, FlowDefinitionVersion, FlowDirective,
  FlowDirectiveInputs, FlowExecutionPolicy, FlowInterpreter, FlowNodeDefinition, FlowNodeDefinitionInput, FlowNodeKind,
  FlowOutcomeDefinition, FlowOutcomeKind, FlowRun, FlowRunId, FlowRunParent, FlowRuntimeHistory,
  FlowTerminalDefinition, FlowTransition, FlowTransitionTarget, ImmutableReference, NodeAttempt, NodeAttemptCompletion,
  NodeAttemptCompletionInput, NodeAttemptId, NodeAttemptInput, NodeAttemptNumber, NodeExecutionIdentity,
  PinnedFlowDefinitionClosure, WorkflowCycle,
};

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}

#[test]
fn deserialization_rejects_subflow_calls_without_an_exact_child() {
  let valid = node(
    "operator-step",
    FlowNodeKind::TrustedAction,
    None,
    schema("output"),
    None,
    100,
  );
  let mut wire = serde_json::to_value(valid).unwrap();
  wire["kind"] = serde_json::json!("subflow_call");
  assert!(serde_json::from_value::<FlowNodeDefinition>(wire).is_err());
}

fn schema(value: &str) -> ImmutableReference {
  ImmutableReference::new(
    key(value),
    key("v1"),
    FactoryDigest::sha256("flow-test-schema", &[value.as_bytes()]),
  )
}

fn budget(value: u64) -> BudgetLimit {
  BudgetLimit::new(8, value, value, value, value).unwrap()
}

fn policy(max_active_nodes: u16, limit: u64) -> FlowExecutionPolicy {
  FlowExecutionPolicy::new(budget(limit), FactoryPermissionSet::deny_all(), max_active_nodes).unwrap()
}

fn node(
  name: &str,
  kind: FlowNodeKind,
  input_schema: Option<ImmutableReference>,
  output_schema: ImmutableReference,
  subflow: Option<FlowDefinitionRef>,
  limit: u64,
) -> FlowNodeDefinition {
  FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key(name),
    kind,
    input_schema,
    outcomes: vec![FlowOutcomeDefinition::new(
      key("accepted"),
      FlowOutcomeKind::Success,
      output_schema,
    )],
    budget: budget(limit),
    permissions: FactoryPermissionSet::deny_all(),
    required: true,
    subflow,
  })
  .unwrap()
}

fn definition(
  identity: u128,
  nodes: Vec<FlowNodeDefinition>,
  transitions: Vec<FlowTransition>,
  terminals: Vec<FlowTerminalDefinition>,
  projections: Vec<FlowDataProjection>,
  max_active_nodes: u16,
  limit: u64,
) -> Result<FlowDefinition, FactoryError> {
  FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::from_uuid(Uuid::from_u128(identity)).unwrap(),
    version: FlowDefinitionVersion::new(1).unwrap(),
    input_schema: nodes.first().and_then(FlowNodeDefinition::input_schema).cloned(),
    entry: nodes.first().unwrap().key().clone(),
    nodes,
    transitions,
    terminals,
    context_projections: Vec::new(),
    data_projections: projections,
    execution: policy(max_active_nodes, limit),
  })
}

fn terminal_route(node: &str, terminal: &str) -> FlowTransition {
  FlowTransition::new(
    key(node),
    key("accepted"),
    FlowTransitionTarget::Terminal(key(terminal)),
  )
}

fn limits(max_depth: u16, max_nodes: u32, max_fan_out: u16, max_wip: u16, limit: u64) -> FlowAdmissionLimits {
  FlowAdmissionLimits::new(
    max_depth,
    max_nodes,
    4,
    max_fan_out,
    max_wip,
    budget(limit),
    FactoryPermissionSet::deny_all(),
  )
  .unwrap()
}

fn ownership() -> FactoryClaimOwnership {
  FactoryClaimOwnership::new(
    key("worker"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::sha256("flow-test-fence", &[b"worker"])),
      Timestamp::from_unix_millis(1).unwrap(),
      Timestamp::from_unix_millis(100).unwrap(),
    )
    .unwrap(),
  )
}

fn attempt(
  id: u128,
  number: u64,
  flow_run: &FlowRun,
  cycle: &WorkflowCycle,
  definition: &FlowDefinition,
  node_key: &str,
  limit: u64,
) -> NodeAttempt {
  let node = definition.node(&key(node_key)).unwrap();
  let execution = match node.kind() {
    FlowNodeKind::Reasoning
    | FlowNodeKind::DecisionSignal
    | FlowNodeKind::TrustedAction
    | FlowNodeKind::BuildCommand => NodeExecutionIdentity::External(schema("test-executor")),
    _ => NodeExecutionIdentity::BuiltIn,
  };
  NodeAttempt::new(
    flow_run,
    cycle,
    definition,
    NodeAttemptInput {
      started_at: (ownership()).claim().claimed_at(),
      id: NodeAttemptId::from_uuid(Uuid::from_u128(id)).unwrap(),
      node_key: node.key().clone(),
      node_kind: node.kind(),
      number: NodeAttemptNumber::new(number).unwrap(),
      input_digest: FactoryDigest::sha256("flow-test-input", &[&id.to_be_bytes()]),
      budget: budget(limit),
      deadline: Timestamp::from_unix_millis(2).unwrap(),
      execution,
      ownership: ownership(),
    },
  )
  .unwrap()
}

fn completion(
  attempt: &NodeAttempt,
  definition: &FlowDefinition,
  output: &ImmutableReference,
  usage: BudgetUsage,
) -> NodeAttemptCompletion {
  NodeAttemptCompletion::new(
    attempt,
    definition,
    NodeAttemptCompletionInput {
      outcome: key("accepted"),
      output_schema: output.clone(),
      output_digest: FactoryDigest::sha256("flow-test-output", &[attempt.id().as_uuid().as_bytes()]),
      ownership: ownership(),
      usage,
      observed_at: Timestamp::from_unix_millis(2).unwrap(),
    },
  )
  .unwrap()
}

#[test]
fn admits_typed_control_data_and_exact_nested_flow_contracts() {
  let payload = schema("payload");
  let child = definition(
    2,
    vec![node(
      "child",
      FlowNodeKind::DeterministicGate,
      Some(payload.clone()),
      payload.clone(),
      None,
      10,
    )],
    vec![terminal_route("child", "accepted")],
    vec![FlowTerminalDefinition::new(key("accepted"), payload.clone())],
    vec![],
    1,
    10,
  )
  .unwrap();
  let child_ref = child.reference();
  let root = definition(
    1,
    vec![
      node("classify", FlowNodeKind::Reasoning, None, payload.clone(), None, 20),
      node(
        "requirements",
        FlowNodeKind::SubflowCall,
        Some(payload.clone()),
        payload.clone(),
        Some(child_ref),
        20,
      ),
    ],
    vec![
      FlowTransition::new(
        key("classify"),
        key("accepted"),
        FlowTransitionTarget::Node(key("requirements")),
      ),
      terminal_route("requirements", "accepted"),
    ],
    vec![FlowTerminalDefinition::new(key("accepted"), payload.clone())],
    vec![FlowDataProjection::new(
      key("classify"),
      key("accepted"),
      key("requirements"),
      payload,
    )],
    1,
    20,
  )
  .unwrap();
  let closure = PinnedFlowDefinitionClosure::new(root.reference(), vec![root, child]).unwrap();
  assert!(closure.clone().validate(limits(1, 3, 1, 1, 20)).is_err());
  let validated = closure.validate(limits(2, 3, 1, 1, 20)).unwrap();
  assert_eq!(validated.expanded_nodes(), 3);
  assert_eq!(validated.max_depth(), 2);
}

#[test]
fn nested_flow_returns_only_through_the_callers_declared_typed_outcome() {
  let payload = schema("subflow-result");
  let child = definition(
    50,
    vec![node(
      "inspect",
      FlowNodeKind::DeterministicGate,
      Some(payload.clone()),
      payload.clone(),
      None,
      20,
    )],
    vec![terminal_route("inspect", "accepted")],
    vec![FlowTerminalDefinition::new(key("accepted"), payload.clone())],
    vec![],
    1,
    20,
  )
  .unwrap();
  let child_ref = child.reference();
  let root = definition(
    51,
    vec![
      node(
        "requirements",
        FlowNodeKind::SubflowCall,
        Some(payload.clone()),
        payload.clone(),
        Some(child_ref),
        20,
      ),
      node(
        "publish",
        FlowNodeKind::DeterministicGate,
        Some(payload.clone()),
        payload.clone(),
        None,
        20,
      ),
    ],
    vec![
      FlowTransition::new(
        key("requirements"),
        key("accepted"),
        FlowTransitionTarget::Node(key("publish")),
      ),
      terminal_route("publish", "accepted"),
    ],
    vec![FlowTerminalDefinition::new(key("accepted"), payload.clone())],
    vec![FlowDataProjection::new(
      key("requirements"),
      key("accepted"),
      key("publish"),
      payload.clone(),
    )],
    1,
    20,
  )
  .unwrap();
  let root_ref = root.reference();
  let validated = PinnedFlowDefinitionClosure::new(root_ref, vec![root.clone(), child.clone()])
    .unwrap()
    .validate(limits(2, 3, 1, 1, 20))
    .unwrap();
  let factory_run_id = FactoryRunId::from_uuid(Uuid::from_u128(510)).unwrap();
  let root_run = FlowRun::nested(
    FlowRunId::from_uuid(Uuid::from_u128(511)).unwrap(),
    factory_run_id,
    root_ref,
    FlowRunParent::new(
      FlowRunId::from_uuid(Uuid::from_u128(512)).unwrap(),
      NodeAttemptId::from_uuid(Uuid::from_u128(513)).unwrap(),
    ),
  );
  let root_cycle = WorkflowCycle::initial(&root_run).unwrap();
  let call = attempt(514, 1, &root_run, &root_cycle, &root, "requirements", 20);
  assert_eq!(
    FlowInterpreter::new(&validated).start(&root_run).unwrap(),
    FlowDirective::EnterSubflow {
      node: key("requirements"),
      definition: child_ref,
      inputs: FlowDirectiveInputs::default(),
    }
  );

  let child_run = FlowRun::nested(
    FlowRunId::from_uuid(Uuid::from_u128(515)).unwrap(),
    factory_run_id,
    child_ref,
    FlowRunParent::new(root_run.id(), call.id()),
  );
  let child_cycle = WorkflowCycle::initial(&child_run).unwrap();
  let child_attempt = attempt(516, 1, &child_run, &child_cycle, &child, "inspect", 20);
  let child_completion = completion(&child_attempt, &child, &payload, BudgetUsage::default());
  assert_eq!(
    FlowInterpreter::new(&validated)
      .advance(
        &child_run,
        &child_cycle,
        &child_attempt,
        &child_completion,
        FlowRuntimeHistory {
          stops: &[],
          flow_runs: std::slice::from_ref(&child_run),
          cycles: std::slice::from_ref(&child_cycle),
          attempts: std::slice::from_ref(&child_attempt),
          completions: std::slice::from_ref(&child_completion),
        },
      )
      .unwrap(),
    vec![FlowDirective::Complete {
      terminal: key("accepted"),
      schema_digest: payload.digest(),
    }]
  );

  let returned = completion(&call, &root, &payload, BudgetUsage::default());
  assert_eq!(
    FlowInterpreter::new(&validated)
      .advance(
        &root_run,
        &root_cycle,
        &call,
        &returned,
        FlowRuntimeHistory {
          stops: &[],
          flow_runs: std::slice::from_ref(&root_run),
          cycles: std::slice::from_ref(&root_cycle),
          attempts: std::slice::from_ref(&call),
          completions: std::slice::from_ref(&returned),
        },
      )
      .unwrap(),
    vec![FlowDirective::ExecuteNode {
      definition: root_ref,
      node: key("publish"),
      kind: FlowNodeKind::DeterministicGate,
      inputs: FlowDirectiveInputs {
        context: Vec::new(),
        data: vec![FlowDataProjection::new(
          key("requirements"),
          key("accepted"),
          key("publish"),
          payload,
        )],
      },
    }]
  );
}

#[test]
fn interpreter_uses_only_the_declared_outcome_route_from_authoritative_facts() {
  let output = schema("result");
  let root = definition(
    4,
    vec![node(
      "gate",
      FlowNodeKind::DeterministicGate,
      None,
      output.clone(),
      None,
      10,
    )],
    vec![terminal_route("gate", "accepted")],
    vec![FlowTerminalDefinition::new(key("accepted"), output.clone())],
    vec![],
    1,
    10,
  )
  .unwrap();
  let reference = root.reference();
  let validated = PinnedFlowDefinitionClosure::new(reference, vec![root.clone()])
    .unwrap()
    .validate(limits(1, 1, 1, 1, 10))
    .unwrap();
  let flow_run = FlowRun::nested(
    FlowRunId::from_uuid(Uuid::from_u128(40)).unwrap(),
    FactoryRunId::from_uuid(Uuid::from_u128(41)).unwrap(),
    reference,
    FlowRunParent::new(
      FlowRunId::from_uuid(Uuid::from_u128(42)).unwrap(),
      NodeAttemptId::from_uuid(Uuid::from_u128(43)).unwrap(),
    ),
  );
  let cycle = WorkflowCycle::initial(&flow_run).unwrap();
  let ownership = FactoryClaimOwnership::new(
    key("worker"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::sha256("flow-test-fence", &[b"worker"])),
      Timestamp::from_unix_millis(1).unwrap(),
      Timestamp::from_unix_millis(100).unwrap(),
    )
    .unwrap(),
  );
  let attempt = NodeAttempt::new(
    &flow_run,
    &cycle,
    &root,
    NodeAttemptInput {
      started_at: (ownership.clone()).claim().claimed_at(),
      id: NodeAttemptId::from_uuid(Uuid::from_u128(44)).unwrap(),
      node_key: key("gate"),
      node_kind: FlowNodeKind::DeterministicGate,
      number: NodeAttemptNumber::INITIAL,
      input_digest: FactoryDigest::sha256("flow-test-input", &[b"input"]),
      budget: budget(10),
      deadline: Timestamp::from_unix_millis(2).unwrap(),
      execution: NodeExecutionIdentity::BuiltIn,
      ownership: ownership.clone(),
    },
  )
  .unwrap();
  let completion = NodeAttemptCompletion::new(
    &attempt,
    &root,
    NodeAttemptCompletionInput {
      outcome: key("accepted"),
      output_schema: output,
      output_digest: FactoryDigest::sha256("flow-test-output", &[b"output"]),
      ownership,
      usage: BudgetUsage::default(),
      observed_at: Timestamp::from_unix_millis(2).unwrap(),
    },
  )
  .unwrap();
  let mut provider_control = serde_json::to_value(&completion).unwrap();
  provider_control
    .as_object_mut()
    .unwrap()
    .insert("successor".to_owned(), serde_json::json!("provider-created"));
  assert!(serde_json::from_value::<NodeAttemptCompletion>(provider_control).is_err());
  let interpreter = FlowInterpreter::new(&validated);
  assert_eq!(
    interpreter.start(&flow_run).unwrap(),
    FlowDirective::ExecuteNode {
      definition: reference,
      node: key("gate"),
      kind: FlowNodeKind::DeterministicGate,
      inputs: FlowDirectiveInputs::default(),
    }
  );
  assert_eq!(
    interpreter
      .advance(
        &flow_run,
        &cycle,
        &attempt,
        &completion,
        FlowRuntimeHistory {
          stops: &[],
          flow_runs: std::slice::from_ref(&flow_run),
          cycles: std::slice::from_ref(&cycle),
          attempts: std::slice::from_ref(&attempt),
          completions: std::slice::from_ref(&completion),
        },
      )
      .unwrap(),
    vec![FlowDirective::Complete {
      terminal: key("accepted"),
      schema_digest: completion.output_schema().digest(),
    }]
  );
}

#[test]
fn rejects_incompatible_schema_unreachable_required_node_and_missing_terminal() {
  let first = schema("first");
  let second = schema("second");
  let incompatible = definition(
    10,
    vec![
      node("source", FlowNodeKind::Reasoning, None, first.clone(), None, 10),
      node(
        "sink",
        FlowNodeKind::DeterministicGate,
        Some(second),
        first.clone(),
        None,
        10,
      ),
    ],
    vec![
      FlowTransition::new(key("source"), key("accepted"), FlowTransitionTarget::Node(key("sink"))),
      terminal_route("sink", "accepted"),
    ],
    vec![FlowTerminalDefinition::new(key("accepted"), first.clone())],
    vec![FlowDataProjection::new(
      key("source"),
      key("accepted"),
      key("sink"),
      first.clone(),
    )],
    1,
    10,
  )
  .unwrap();
  assert!(
    PinnedFlowDefinitionClosure::new(incompatible.reference(), vec![incompatible])
      .unwrap()
      .validate(limits(1, 2, 1, 1, 10))
      .is_err()
  );

  let unreachable = definition(
    11,
    vec![
      node("entry", FlowNodeKind::Reasoning, None, first.clone(), None, 10),
      node("orphan", FlowNodeKind::Reasoning, None, first.clone(), None, 10),
    ],
    vec![
      terminal_route("entry", "accepted"),
      terminal_route("orphan", "accepted"),
    ],
    vec![FlowTerminalDefinition::new(key("accepted"), first.clone())],
    vec![],
    1,
    10,
  )
  .unwrap();
  assert!(
    PinnedFlowDefinitionClosure::new(unreachable.reference(), vec![unreachable])
      .unwrap()
      .validate(limits(1, 2, 1, 1, 10))
      .is_err()
  );

  let missing_terminal = definition(
    12,
    vec![node("entry", FlowNodeKind::Reasoning, None, first, None, 10)],
    vec![],
    vec![],
    vec![],
    1,
    10,
  )
  .unwrap();
  assert!(
    PinnedFlowDefinitionClosure::new(missing_terminal.reference(), vec![missing_terminal])
      .unwrap()
      .validate(limits(1, 1, 1, 1, 10))
      .is_err()
  );
}

#[test]
fn rejects_provider_created_edges_recursion_and_unbounded_cycles() {
  let recursive_ref = FlowDefinitionRef::new(
    FlowDefinitionId::from_uuid(Uuid::from_u128(20)).unwrap(),
    FlowDefinitionVersion::new(1).unwrap(),
    FactoryDigest::sha256("forged", &[b"recursive"]),
  );
  let recursive = definition(
    20,
    vec![node(
      "self",
      FlowNodeKind::SubflowCall,
      Some(schema("payload")),
      schema("payload"),
      Some(recursive_ref),
      10,
    )],
    vec![terminal_route("self", "accepted")],
    vec![FlowTerminalDefinition::new(key("accepted"), schema("payload"))],
    vec![],
    1,
    10,
  );
  assert!(recursive.is_err());

  let cycle = definition(
    21,
    vec![node(
      "again",
      FlowNodeKind::Reasoning,
      None,
      schema("payload"),
      None,
      10,
    )],
    vec![FlowTransition::new(
      key("again"),
      key("accepted"),
      FlowTransitionTarget::Node(key("again")),
    )],
    vec![FlowTerminalDefinition::new(key("accepted"), schema("payload"))],
    vec![],
    1,
    10,
  )
  .unwrap();
  assert!(
    PinnedFlowDefinitionClosure::new(cycle.reference(), vec![cycle])
      .unwrap()
      .validate(limits(1, 8, 1, 1, 10))
      .is_err()
  );
}

#[test]
fn rejects_node_fanout_wip_and_budget_beyond_admission_bounds() {
  let payload = schema("payload");
  let bounded = definition(
    30,
    vec![
      node("split", FlowNodeKind::FanOut, None, payload.clone(), None, 20),
      node("left", FlowNodeKind::Join, None, payload.clone(), None, 20),
      node("right", FlowNodeKind::Join, None, payload.clone(), None, 20),
    ],
    vec![
      FlowTransition::new(key("split"), key("accepted"), FlowTransitionTarget::Node(key("left"))),
      FlowTransition::new(key("split"), key("accepted"), FlowTransitionTarget::Node(key("right"))),
      terminal_route("left", "accepted"),
      terminal_route("right", "accepted"),
    ],
    vec![FlowTerminalDefinition::new(key("accepted"), payload)],
    vec![],
    2,
    20,
  )
  .unwrap();
  let closure = PinnedFlowDefinitionClosure::new(bounded.reference(), vec![bounded]).unwrap();
  assert!(closure.clone().validate(limits(1, 2, 2, 2, 20)).is_err());
  assert!(closure.clone().validate(limits(1, 3, 1, 2, 20)).is_err());
  assert!(closure.clone().validate(limits(1, 3, 2, 1, 20)).is_err());
  assert!(closure.validate(limits(1, 3, 2, 2, 10)).is_err());
}

#[test]
fn interpreter_enforces_aggregate_wip_without_discounting_an_active_sibling() {
  let output = schema("fanout-result");
  let root = definition(
    31,
    vec![
      node("split", FlowNodeKind::FanOut, None, output.clone(), None, 10),
      node("left", FlowNodeKind::Join, None, output.clone(), None, 10),
      node("right", FlowNodeKind::Join, None, output.clone(), None, 10),
    ],
    vec![
      FlowTransition::new(key("split"), key("accepted"), FlowTransitionTarget::Node(key("left"))),
      FlowTransition::new(key("split"), key("accepted"), FlowTransitionTarget::Node(key("right"))),
      terminal_route("left", "accepted"),
      terminal_route("right", "accepted"),
    ],
    vec![FlowTerminalDefinition::new(key("accepted"), output.clone())],
    vec![],
    2,
    10,
  )
  .unwrap();
  let reference = root.reference();
  let validated = PinnedFlowDefinitionClosure::new(reference, vec![root.clone()])
    .unwrap()
    .validate(limits(1, 3, 2, 2, 10))
    .unwrap();
  let flow_run = FlowRun::nested(
    FlowRunId::from_uuid(Uuid::from_u128(310)).unwrap(),
    FactoryRunId::from_uuid(Uuid::from_u128(311)).unwrap(),
    reference,
    FlowRunParent::new(
      FlowRunId::from_uuid(Uuid::from_u128(312)).unwrap(),
      NodeAttemptId::from_uuid(Uuid::from_u128(313)).unwrap(),
    ),
  );
  let cycle = WorkflowCycle::initial(&flow_run).unwrap();
  let split = attempt(314, 1, &flow_run, &cycle, &root, "split", 10);
  let active_sibling = attempt(315, 2, &flow_run, &cycle, &root, "left", 10);
  let completed_split = completion(&split, &root, &output, BudgetUsage::default());

  assert!(
    FlowInterpreter::new(&validated)
      .advance(
        &flow_run,
        &cycle,
        &split,
        &completed_split,
        FlowRuntimeHistory {
          stops: &[],
          flow_runs: std::slice::from_ref(&flow_run),
          cycles: std::slice::from_ref(&cycle),
          attempts: &[split.clone(), active_sibling],
          completions: std::slice::from_ref(&completed_split),
        },
      )
      .is_err()
  );
}

#[test]
fn interpreter_carries_only_declared_context_and_data_into_the_successor() {
  let output = schema("projected-result");
  let source = node("source", FlowNodeKind::Reasoning, None, output.clone(), None, 10);
  let sink = node(
    "sink",
    FlowNodeKind::DeterministicGate,
    Some(output.clone()),
    output.clone(),
    None,
    10,
  );
  let context = FlowContextProjection::new(
    key("source"),
    key("accepted"),
    key("sink"),
    vec![key("requirements-summary")],
  )
  .unwrap();
  let data = FlowDataProjection::new(key("source"), key("accepted"), key("sink"), output.clone());
  let root = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::from_uuid(Uuid::from_u128(33)).unwrap(),
    version: FlowDefinitionVersion::new(1).unwrap(),
    input_schema: None,
    entry: key("source"),
    nodes: vec![source, sink],
    transitions: vec![
      FlowTransition::new(key("source"), key("accepted"), FlowTransitionTarget::Node(key("sink"))),
      terminal_route("sink", "accepted"),
    ],
    terminals: vec![FlowTerminalDefinition::new(key("accepted"), output.clone())],
    context_projections: vec![context.clone()],
    data_projections: vec![data.clone()],
    execution: policy(1, 10),
  })
  .unwrap();
  let reference = root.reference();
  let validated = PinnedFlowDefinitionClosure::new(reference, vec![root.clone()])
    .unwrap()
    .validate(limits(1, 2, 1, 1, 10))
    .unwrap();
  let flow_run = FlowRun::nested(
    FlowRunId::from_uuid(Uuid::from_u128(330)).unwrap(),
    FactoryRunId::from_uuid(Uuid::from_u128(331)).unwrap(),
    reference,
    FlowRunParent::new(
      FlowRunId::from_uuid(Uuid::from_u128(332)).unwrap(),
      NodeAttemptId::from_uuid(Uuid::from_u128(333)).unwrap(),
    ),
  );
  let cycle = WorkflowCycle::initial(&flow_run).unwrap();
  let source_attempt = attempt(334, 1, &flow_run, &cycle, &root, "source", 10);
  let source_completion = completion(&source_attempt, &root, &output, BudgetUsage::default());

  assert_eq!(
    FlowInterpreter::new(&validated)
      .advance(
        &flow_run,
        &cycle,
        &source_attempt,
        &source_completion,
        FlowRuntimeHistory {
          stops: &[],
          flow_runs: std::slice::from_ref(&flow_run),
          cycles: std::slice::from_ref(&cycle),
          attempts: std::slice::from_ref(&source_attempt),
          completions: std::slice::from_ref(&source_completion),
        },
      )
      .unwrap(),
    vec![FlowDirective::ExecuteNode {
      definition: reference,
      node: key("sink"),
      kind: FlowNodeKind::DeterministicGate,
      inputs: FlowDirectiveInputs {
        context: vec![context],
        data: vec![data],
      },
    }]
  );
}

#[test]
fn interpreter_counts_budget_and_repeats_across_workflow_cycles() {
  let output = schema("repeat-result");
  let repeated = FlowTransition::repeated(
    key("again"),
    key("accepted"),
    FlowTransitionTarget::Node(key("again")),
    1,
    key("exhausted"),
  )
  .unwrap();
  let root = definition(
    32,
    vec![node("again", FlowNodeKind::Reasoning, None, output.clone(), None, 10)],
    vec![repeated],
    vec![FlowTerminalDefinition::new(key("exhausted"), output.clone())],
    vec![],
    1,
    10,
  )
  .unwrap();
  let reference = root.reference();
  let validated = PinnedFlowDefinitionClosure::new(reference, vec![root.clone()])
    .unwrap()
    .validate(limits(1, 2, 1, 1, 10))
    .unwrap();
  let flow_run = FlowRun::nested(
    FlowRunId::from_uuid(Uuid::from_u128(320)).unwrap(),
    FactoryRunId::from_uuid(Uuid::from_u128(321)).unwrap(),
    reference,
    FlowRunParent::new(
      FlowRunId::from_uuid(Uuid::from_u128(322)).unwrap(),
      NodeAttemptId::from_uuid(Uuid::from_u128(323)).unwrap(),
    ),
  );
  let first_cycle = WorkflowCycle::initial(&flow_run).unwrap();
  let second_cycle = WorkflowCycle::next(
    crate::WorkflowCycleId::from_uuid(Uuid::from_u128(324)).unwrap(),
    flow_run.id(),
    crate::WorkflowCycleNumber::new(2).unwrap(),
    first_cycle.id(),
    NodeAttemptId::from_uuid(Uuid::from_u128(325)).unwrap(),
  );
  let first = attempt(325, 1, &flow_run, &first_cycle, &root, "again", 10);
  let second = attempt(326, 2, &flow_run, &second_cycle, &root, "again", 10);
  let first_zero = completion(&first, &root, &output, BudgetUsage::default());
  let second_zero = completion(&second, &root, &output, BudgetUsage::default());
  assert_eq!(
    FlowInterpreter::new(&validated)
      .advance(
        &flow_run,
        &second_cycle,
        &second,
        &second_zero,
        FlowRuntimeHistory {
          stops: &[],
          flow_runs: std::slice::from_ref(&flow_run),
          cycles: std::slice::from_ref(&second_cycle),
          attempts: &[first.clone(), second.clone()],
          completions: &[first_zero, second_zero.clone()],
        },
      )
      .unwrap(),
    vec![FlowDirective::Complete {
      terminal: key("exhausted"),
      schema_digest: output.digest(),
    }]
  );
  let first_completion = completion(
    &first,
    &root,
    &output,
    BudgetUsage {
      elapsed_millis: 6,
      ..BudgetUsage::default()
    },
  );
  let second_completion = completion(
    &second,
    &root,
    &output,
    BudgetUsage {
      elapsed_millis: 5,
      ..BudgetUsage::default()
    },
  );

  assert!(
    FlowInterpreter::new(&validated)
      .advance(
        &flow_run,
        &second_cycle,
        &second,
        &second_completion,
        FlowRuntimeHistory {
          stops: &[],
          flow_runs: std::slice::from_ref(&flow_run),
          cycles: std::slice::from_ref(&second_cycle),
          attempts: &[first, second.clone()],
          completions: &[first_completion, second_completion.clone()],
        },
      )
      .is_err()
  );
}

#[test]
fn a_declared_bounded_restart_selects_a_new_cycle_at_an_arbitrary_node() {
  let output = schema("correction.result");
  let correction = node(
    "rewrite_contract",
    FlowNodeKind::Reasoning,
    None,
    output.clone(),
    None,
    100,
  );
  // No business-stage name has any runtime meaning.
  let transitions=vec![FlowTransition::new(key("rewrite_contract"),key("accepted"),FlowTransitionTarget::Node(key("inspect_result"))),
    serde_json::from_value(serde_json::json!({"predecessor":"inspect_result","outcome":"accepted","target":{"kind":"node","key":"rewrite_contract"},"max_repeats":1,"exhausted_terminal":"needs_operator","starts_cycle":true})).expect("cycle boundaries are explicitly configured")];
  let root = definition(
    801,
    vec![
      correction,
      node(
        "inspect_result",
        FlowNodeKind::Reasoning,
        None,
        output.clone(),
        None,
        100,
      ),
    ],
    transitions,
    vec![FlowTerminalDefinition::new(key("needs_operator"), output.clone())],
    vec![],
    1,
    100,
  )
  .unwrap();
  let validated = PinnedFlowDefinitionClosure::new(root.reference(), vec![root.clone()])
    .unwrap()
    .validate(limits(1, 10, 1, 1, 100))
    .unwrap();
  let flow = FlowRun::nested(
    FlowRunId::generate(),
    FactoryRunId::generate(),
    root.reference(),
    FlowRunParent::new(FlowRunId::generate(), NodeAttemptId::generate()),
  );
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let first = attempt(802, 1, &flow, &cycle, &root, "rewrite_contract", 100);
  let second = attempt(803, 2, &flow, &cycle, &root, "inspect_result", 100);
  let done_first = completion(&first, &root, &output, BudgetUsage::default());
  let done_second = completion(&second, &root, &output, BudgetUsage::default());
  let history = FlowRuntimeHistory {
    stops: &[],
    flow_runs: std::slice::from_ref(&flow),
    cycles: std::slice::from_ref(&cycle),
    attempts: &[first, second.clone()],
    completions: &[done_first, done_second.clone()],
  };
  let interpreter = FlowInterpreter::new(&validated);
  assert_eq!(
    interpreter
      .advance(&flow, &cycle, &second, &done_second, history)
      .unwrap(),
    vec![FlowDirective::RestartCycle {
      node: key("rewrite_contract"),
      source: second.id()
    }]
  );
  assert_eq!(
    interpreter.progress(&flow, &cycle, history).unwrap(),
    crate::FlowProgress::RestartCycle {
      node: key("rewrite_contract"),
      source: second.id()
    }
  );
}

#[test]
fn correction_cycles_retain_their_trigger_and_resume_only_the_declared_entry() {
  let work = crate::flow_test_support::work();
  let data_schema =
    crate::FlowDataSchema::new(key("correction.result"), key("v1"), crate::FlowValueSchema::Boolean).unwrap();
  let output = data_schema.reference().clone();
  let transitions=vec![FlowTransition::new(key("intake"),key("accepted"),FlowTransitionTarget::Node(key("rewrite_contract"))),FlowTransition::new(key("rewrite_contract"),key("accepted"),FlowTransitionTarget::Node(key("inspect_result"))),serde_json::from_value(serde_json::json!({"predecessor":"inspect_result","outcome":"accepted","target":{"kind":"node","key":"rewrite_contract"},"max_repeats":1,"exhausted_terminal":"needs_operator","starts_cycle":true})).unwrap()];
  let root = definition(
    811,
    vec![
      node("intake", FlowNodeKind::Reasoning, None, output.clone(), None, 100),
      node(
        "rewrite_contract",
        FlowNodeKind::Reasoning,
        None,
        output.clone(),
        None,
        100,
      ),
      node(
        "inspect_result",
        FlowNodeKind::Reasoning,
        None,
        output.clone(),
        None,
        100,
      ),
    ],
    transitions,
    vec![FlowTerminalDefinition::new(key("needs_operator"), output.clone())],
    vec![],
    1,
    100,
  )
  .unwrap();
  let run = crate::FactoryRun::admitted(FactoryRunId::generate(), &work);
  let flow = FlowRun::root(&run, root.reference()).unwrap();
  let first = WorkflowCycle::initial(&flow).unwrap();
  let admitted = crate::AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(root.reference(), vec![root.clone()]).unwrap(),
    limits(1, 10, 1, 1, 100),
    flow.clone(),
    first.clone(),
    crate::FactoryWipLimits::new(1, 1).unwrap(),
  )
  .unwrap()
  .with_data_schemas(vec![data_schema])
  .unwrap();
  let attempts = [
    attempt(812, 1, &flow, &first, &root, "intake", 100),
    attempt(813, 2, &flow, &first, &root, "rewrite_contract", 100),
    attempt(814, 3, &flow, &first, &root, "inspect_result", 100),
  ];
  let completions = attempts
    .iter()
    .map(|row| completion(row, &root, &output, BudgetUsage::default()))
    .collect::<Vec<_>>();
  let next = WorkflowCycle::restart(crate::WorkflowCycleId::generate(), &first, &attempts[2])
    .expect("accepted restart has a causal source");
  assert_eq!(next.source_attempt_id(), Some(attempts[2].id()));
  let cycles = [first.clone(), next.clone()];
  let history = FlowRuntimeHistory {
    stops: &[],
    flow_runs: std::slice::from_ref(&flow),
    cycles: &cycles,
    attempts: &attempts,
    completions: &completions,
  };
  crate::validate_flow_runtime_history(&admitted, history).unwrap();
  assert!(!history.is_current_cycle(first.id()));
  assert!(history.is_current_cycle(next.id()));
  assert_eq!(
    FlowInterpreter::new(&admitted.validated().unwrap())
      .progress(&flow, &next, history)
      .unwrap(),
    crate::FlowProgress::Node(key("rewrite_contract"))
  );
  let skipped = attempt(815, 4, &flow, &next, &root, "inspect_result", 100);
  let mut bypassed = attempts.to_vec();
  bypassed.push(skipped);
  assert!(
    crate::validate_flow_runtime_history(
      &admitted,
      FlowRuntimeHistory {
        attempts: &bypassed,
        ..history
      }
    )
    .is_err(),
    "a new cycle must rerun its declared entry rather than reuse a prior review edge"
  );
  let stopped_attempt = attempt(816, 4, &flow, &next, &root, "rewrite_contract", 100);
  let stop = crate::FlowNodeStop::new(
    &stopped_attempt,
    ownership(),
    crate::FlowNodeStopReason::ExecutionFailed,
    Timestamp::from_unix_millis(2).unwrap(),
    BudgetUsage::default(),
    None,
  )
  .unwrap();
  let mut with_stop = attempts.to_vec();
  with_stop.push(stopped_attempt);
  crate::validate_flow_runtime_history(
    &admitted,
    FlowRuntimeHistory {
      attempts: &with_stop,
      stops: &[stop],
      ..history
    },
  )
  .unwrap();
  let forged = WorkflowCycle::restart(crate::WorkflowCycleId::generate(), &first, &attempts[0]).unwrap();
  assert!(
    crate::validate_flow_runtime_history(
      &admitted,
      FlowRuntimeHistory {
        cycles: &[first, forged],
        ..history
      }
    )
    .is_err(),
    "an accepted result without a declared restart cannot create a cycle"
  );
}

#[test]
fn a_cycle_boundary_cannot_share_an_outcome_with_another_fanout_route() {
  let output = schema("payload");
  let root = definition(
    901,
    vec![
      node("split", FlowNodeKind::FanOut, None, output.clone(), None, 100),
      node("left", FlowNodeKind::Join, None, output.clone(), None, 100),
    ],
    vec![
      serde_json::from_value(serde_json::json!({"predecessor":"split","outcome":"accepted","target":{"kind":"node","key":"split"},"max_repeats":1,"exhausted_terminal":"exhausted","starts_cycle":true})).unwrap(),
      FlowTransition::new(key("split"), key("accepted"), FlowTransitionTarget::Node(key("left"))),
      terminal_route("left", "done"),
    ],
    vec![FlowTerminalDefinition::new(key("exhausted"), output.clone()), FlowTerminalDefinition::new(key("done"), output)],
    vec![],
    2,
    100,
  ).unwrap();
  assert!(
    PinnedFlowDefinitionClosure::new(root.reference(), vec![root])
      .unwrap()
      .validate(limits(1, 10, 2, 2, 100))
      .is_err()
  );
}

#[test]
fn a_restart_remains_restorable_after_a_declared_sibling_finishes() {
  for right_restarts in [false, true] {
    let data_schema = crate::FlowDataSchema::new(key("output"), key("v1"), crate::FlowValueSchema::Boolean).unwrap();
    let out = data_schema.reference().clone();
    let restart = |name| {
      serde_json::from_value(serde_json::json!({"predecessor":name,"outcome":"accepted","target":{"kind":"node","key":"split"},"max_repeats":1,"exhausted_terminal":"exhausted","starts_cycle":true})).unwrap()
    };
    let mut terminals = vec![FlowTerminalDefinition::new(key("exhausted"), out.clone())];
    if !right_restarts {
      terminals.push(FlowTerminalDefinition::new(key("done"), out.clone()));
    }
    let root = definition(
      9801,
      vec![
        node("split", FlowNodeKind::FanOut, None, out.clone(), None, 100),
        node("left", FlowNodeKind::Reasoning, None, out.clone(), None, 100),
        node("right", FlowNodeKind::Reasoning, None, out.clone(), None, 100),
      ],
      vec![
        FlowTransition::new(key("split"), key("accepted"), FlowTransitionTarget::Node(key("left"))),
        FlowTransition::new(key("split"), key("accepted"), FlowTransitionTarget::Node(key("right"))),
        restart("left"),
        if right_restarts {
          restart("right")
        } else {
          terminal_route("right", "done")
        },
      ],
      terminals,
      vec![],
      2,
      100,
    )
    .unwrap();
    let v = PinnedFlowDefinitionClosure::new(root.reference(), vec![root.clone()])
      .unwrap()
      .validate(limits(1, 20, 2, 2, 100))
      .unwrap();
    let run = crate::FactoryRun::admitted(FactoryRunId::generate(), &crate::flow_test_support::work());
    let flow = FlowRun::root(&run, root.reference()).unwrap();
    let cycle = WorkflowCycle::initial(&flow).unwrap();
    let admitted = crate::AdmittedFlow::new(
      PinnedFlowDefinitionClosure::new(root.reference(), vec![root.clone()]).unwrap(),
      limits(1, 20, 2, 2, 100),
      flow.clone(),
      cycle.clone(),
      crate::FactoryWipLimits::new(2, 2).unwrap(),
    )
    .unwrap()
    .with_data_schemas(vec![data_schema])
    .unwrap();
    let mut attempts = vec![
      attempt(9802, 1, &flow, &cycle, &root, "split", 100),
      attempt(9803, 2, &flow, &cycle, &root, "left", 100),
      attempt(9804, 3, &flow, &cycle, &root, "right", 100),
    ];
    let done = attempts
      .iter()
      .map(|a| completion(a, &root, &out, BudgetUsage::default()))
      .collect::<Vec<_>>();
    let source = attempts[1].clone();
    let other = attempts[2].clone();
    // Restored rows can arrive in identity order rather than execution order.
    attempts.reverse();
    let h = FlowRuntimeHistory {
      stops: &[],
      flow_runs: std::slice::from_ref(&flow),
      cycles: std::slice::from_ref(&cycle),
      attempts: &attempts,
      completions: &done,
    };
    let engine = FlowInterpreter::new(&v);
    assert_eq!(
      engine.progress(&flow, &cycle, h).unwrap(),
      crate::FlowProgress::RestartCycle {
        node: key("split"),
        source: source.id()
      }
    );
    let next = WorkflowCycle::restart(crate::WorkflowCycleId::generate(), &cycle, &source).unwrap();
    assert_eq!(engine.cycle_entry(&flow, &next, h).unwrap(), key("split"));
    let cycles = [cycle.clone(), next];
    let restored = FlowRuntimeHistory { cycles: &cycles, ..h };
    crate::validate_flow_runtime_history(&admitted, restored).unwrap();
    let stale = attempt(9805, 4, &flow, &cycle, &root, "split", 100);
    let mut stale_completions = done.clone();
    stale_completions.push(completion(&stale, &root, &out, BudgetUsage::default()));
    let mut stale_attempts = attempts.clone();
    stale_attempts.push(stale);
    assert!(
      crate::validate_flow_runtime_history(
        &admitted,
        FlowRuntimeHistory {
          attempts: &stale_attempts,
          completions: &stale_completions,
          ..restored
        }
      )
      .is_err(),
      "a restart edge cannot authorize its target in the preceding cycle"
    );
    if right_restarts {
      let substituted = WorkflowCycle::restart(crate::WorkflowCycleId::generate(), &cycle, &other).unwrap();
      assert!(
        engine.cycle_entry(&flow, &substituted, h).is_err(),
        "restoration must retain the same selected cause"
      );
    }
  }
}

#[test]
fn every_ready_fanout_sibling_remains_visible_without_reserving_existing_work_again() {
  let output = schema("fanout.output");
  let root = definition(
    9901,
    vec![
      node("split", FlowNodeKind::FanOut, None, output.clone(), None, 100),
      node("left", FlowNodeKind::Reasoning, None, output.clone(), None, 100),
      node("right", FlowNodeKind::Reasoning, None, output.clone(), None, 100),
    ],
    vec![
      FlowTransition::new(key("split"), key("accepted"), FlowTransitionTarget::Node(key("left"))),
      FlowTransition::new(key("split"), key("accepted"), FlowTransitionTarget::Node(key("right"))),
      terminal_route("left", "done"),
      terminal_route("right", "done"),
    ],
    vec![FlowTerminalDefinition::new(key("done"), output.clone())],
    vec![],
    2,
    100,
  )
  .unwrap();
  let validated = PinnedFlowDefinitionClosure::new(root.reference(), vec![root.clone()])
    .unwrap()
    .validate(limits(1, 10, 2, 2, 100))
    .unwrap();
  let flow = FlowRun::nested(
    FlowRunId::generate(),
    FactoryRunId::generate(),
    root.reference(),
    FlowRunParent::new(FlowRunId::generate(), NodeAttemptId::generate()),
  );
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let split = attempt(9902, 1, &flow, &cycle, &root, "split", 100);
  let completed = completion(&split, &root, &output, BudgetUsage::default());
  let left = attempt(9903, 2, &flow, &cycle, &root, "left", 100);
  let interpreter = FlowInterpreter::new(&validated);
  let expected = std::collections::BTreeSet::from([key("left"), key("right")]);
  for attempts in [vec![split.clone()], vec![split.clone(), left]] {
    let history = FlowRuntimeHistory {
      stops: &[],
      flow_runs: std::slice::from_ref(&flow),
      cycles: std::slice::from_ref(&cycle),
      attempts: &attempts,
      completions: std::slice::from_ref(&completed),
    };
    assert_eq!(interpreter.ready_nodes(&flow, &cycle, history).unwrap(), expected);
    assert_eq!(
      interpreter.progress(&flow, &cycle, history).unwrap(),
      crate::FlowProgress::Node(key("left"))
    );
  }
}
