use octacity_server_domain::Timestamp;
use uuid::Uuid;

use crate::{
  BudgetLimit, BudgetUsage, FactoryClaim, FactoryClaimFence, FactoryClaimOwnership, FactoryDigest, FactoryError,
  FactoryKey, FactoryPermissionSet, FactoryRunId, FlowAdmissionLimits, FlowContextProjection, FlowDataProjection,
  FlowDefinition, FlowDefinitionId, FlowDefinitionInput, FlowDefinitionRef, FlowDefinitionVersion, FlowDirective,
  FlowDirectiveInputs, FlowExecutionPolicy, FlowInterpreter, FlowNodeDefinition, FlowNodeDefinitionInput, FlowNodeKind,
  FlowOutcomeDefinition, FlowOutcomeKind, FlowRun, FlowRunId, FlowRunParent, FlowTerminalDefinition, FlowTransition,
  FlowTransitionTarget, ImmutableReference, NodeAttempt, NodeAttemptCompletion, NodeAttemptCompletionInput,
  NodeAttemptId, NodeAttemptInput, NodeAttemptNumber, NodeExecutionIdentity, PinnedFlowDefinitionClosure,
  WorkflowCycle,
};

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
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
      id: NodeAttemptId::from_uuid(Uuid::from_u128(id)).unwrap(),
      node_key: node.key().clone(),
      node_kind: node.kind(),
      number: NodeAttemptNumber::new(number).unwrap(),
      input_digest: FactoryDigest::sha256("flow-test-input", &[&id.to_be_bytes()]),
      budget: budget(limit),
      deadline: Timestamp::from_unix_millis(99).unwrap(),
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
        std::slice::from_ref(&child_attempt),
        std::slice::from_ref(&child_completion),
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
        std::slice::from_ref(&call),
        std::slice::from_ref(&returned),
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
      id: NodeAttemptId::from_uuid(Uuid::from_u128(44)).unwrap(),
      node_key: key("gate"),
      node_kind: FlowNodeKind::DeterministicGate,
      number: NodeAttemptNumber::INITIAL,
      input_digest: FactoryDigest::sha256("flow-test-input", &[b"input"]),
      budget: budget(10),
      deadline: Timestamp::from_unix_millis(99).unwrap(),
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
        std::slice::from_ref(&attempt),
        std::slice::from_ref(&completion),
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
        &[split.clone(), active_sibling],
        std::slice::from_ref(&completed_split),
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
        std::slice::from_ref(&source_attempt),
        std::slice::from_ref(&source_completion),
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
        &[first.clone(), second.clone()],
        &[first_zero, second_zero.clone()],
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
        &[first, second.clone()],
        &[first_completion, second_completion.clone()],
      )
      .is_err()
  );
}
