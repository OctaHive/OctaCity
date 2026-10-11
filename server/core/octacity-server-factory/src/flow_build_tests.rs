use crate::*;

use crate::flow_test_support::*;

#[test]
fn confirmed_failures_do_not_occupy_capacity_after_a_later_configured_retry_completes() {
  let time = |value| octacity_server_domain::Timestamp::from_unix_millis(value).unwrap();
  let (_, admitted, schema, input, original) = execution_fixture_with_retries(FlowNodeKind::Reasoning, 2);
  let definition = admitted.closure().definition(input.definition()).unwrap();
  let ownership = FactoryClaimOwnership::new(original.owner().clone(), original.claim());
  let retry = |input: &FlowNodeInput, previous: &NodeAttempt, number, started_at| {
    NodeAttempt::new(
      admitted.root_run(),
      admitted.initial_cycle(),
      definition,
      NodeAttemptInput {
        id: NodeAttemptId::generate(),
        node_key: previous.node_key().clone(),
        node_kind: previous.node_kind(),
        number: NodeAttemptNumber::new(number).unwrap(),
        input_digest: input.digest().unwrap(),
        budget: previous.budget(),
        started_at: time(started_at),
        deadline: time(started_at + 1000),
        execution: previous.execution().clone(),
        ownership: ownership.clone(),
      },
    )
    .unwrap()
    .with_retry_of(previous)
    .unwrap()
  };
  let second_input = input.retry().unwrap();
  let second = retry(&second_input, &original, 2, 20);
  let third_input = second_input.retry().unwrap();
  let third = retry(&third_input, &second, 3, 30);
  let stops = vec![
    FlowNodeStop::new(
      &original,
      ownership.clone(),
      FlowNodeStopReason::ExecutionFailed,
      time(15),
      BudgetUsage {
        tokens: 23,
        ..Default::default()
      },
      None,
    )
    .unwrap(),
    FlowNodeStop::new(
      &second,
      ownership.clone(),
      FlowNodeStopReason::ExecutionFailed,
      time(25),
      BudgetUsage {
        tokens: 7,
        ..Default::default()
      },
      None,
    )
    .unwrap(),
  ];
  let completed = NodeAttemptCompletion::new(
    &third,
    definition,
    NodeAttemptCompletionInput {
      outcome: key("observed"),
      output_schema: schema.reference().clone(),
      output_digest: FactoryDigest::content_sha256(br#"{"count":9}"#),
      ownership,
      usage: BudgetUsage::default(),
      observed_at: time(35),
    },
  )
  .unwrap();
  let source = third.id();
  let attempts = vec![original, second, third];
  let history = FlowRuntimeHistory {
    stops: &stops,
    flow_runs: std::slice::from_ref(admitted.root_run()),
    cycles: std::slice::from_ref(admitted.initial_cycle()),
    attempts: &attempts,
    completions: std::slice::from_ref(&completed),
  };
  validate_flow_runtime_history(&admitted, history).unwrap();
  assert_eq!(history.active_node_count(), 0);
  let validated = admitted.validated().unwrap();
  assert_eq!(
    FlowInterpreter::new(&validated)
      .progress(admitted.root_run(), admitted.initial_cycle(), history)
      .unwrap(),
    FlowProgress::Terminal {
      key: key("done"),
      source
    }
  );
}

#[test]
fn a_technical_retry_preserves_frozen_input_and_requires_a_confirmed_failed_predecessor() {
  let time = |value| octacity_server_domain::Timestamp::from_unix_millis(value).unwrap();
  let (work, admitted, _, input, original) = execution_fixture_with_retries(FlowNodeKind::Reasoning, 1);
  let definition = admitted.closure().definition(input.definition()).unwrap();
  let retry_input = input.retry().unwrap();
  assert_eq!(retry_input.generation(), 1);
  assert_eq!(retry_input.payload(), input.payload());
  assert_eq!(retry_input.context(), input.context());
  let retry = NodeAttempt::new(
    admitted.root_run(),
    admitted.initial_cycle(),
    definition,
    NodeAttemptInput {
      id: NodeAttemptId::generate(),
      node_key: original.node_key().clone(),
      node_kind: original.node_kind(),
      number: NodeAttemptNumber::new(2).unwrap(),
      input_digest: retry_input.digest().unwrap(),
      budget: original.budget(),
      started_at: time(20),
      deadline: time(1020),
      execution: original.execution().clone(),
      ownership: FactoryClaimOwnership::new(original.owner().clone(), original.claim()),
    },
  )
  .unwrap()
  .with_retry_of(&original)
  .unwrap();
  assert_eq!(retry.retry_of(), Some(original.id()));
  let intents = vec![
    FlowBuildIntent::new(input.clone(), &admitted, original.clone(), BudgetUsage::default(), 1).unwrap(),
    FlowBuildIntent::new(
      retry_input.clone(),
      &admitted,
      retry.clone(),
      BudgetUsage {
        attempts: 1,
        tokens: 23,
        ..Default::default()
      },
      1,
    )
    .unwrap(),
  ];
  assert_ne!(intents[0].operation_id().unwrap(), intents[1].operation_id().unwrap());
  let stops = vec![
    FlowNodeStop::new(
      &original,
      FactoryClaimOwnership::new(original.owner().clone(), original.claim()),
      FlowNodeStopReason::ExecutionFailed,
      time(15),
      BudgetUsage {
        tokens: 23,
        ..Default::default()
      },
      None,
    )
    .unwrap(),
  ];
  let attempts = vec![original, retry];
  let history = FlowRuntimeHistory {
    stops: &stops,
    flow_runs: std::slice::from_ref(admitted.root_run()),
    cycles: std::slice::from_ref(admitted.initial_cycle()),
    attempts: &attempts,
    completions: &[],
  };
  let data = FlowDataHistory {
    stops: stops.clone(),
    inputs: vec![input, retry_input],
    build_intents: intents,
    ..Default::default()
  };
  data.validate(&work, &admitted, history).unwrap();
  assert_eq!(history.active_node_count(), 1);
  assert!(
    validate_flow_runtime_history(&admitted, FlowRuntimeHistory { stops: &[], ..history }).is_err(),
    "retry cannot replace a still active operation"
  );
  let record = FlowNodeRecord::new(
    &data.inputs[1],
    &attempts[1],
    definition,
    FlowRecordObservation {
      outcome: key("observed"),
      payload: data.inputs[1].payload().clone(),
      producer: attempts[1].execution().clone(),
      observed_at: time(30),
    },
  )
  .unwrap();
  let completion = NodeAttemptCompletion::new(
    &attempts[1],
    definition,
    NodeAttemptCompletionInput {
      outcome: key("observed"),
      output_schema: record.payload().schema().clone(),
      output_digest: record.digest().unwrap(),
      ownership: FactoryClaimOwnership::new(attempts[1].owner().clone(), attempts[1].claim()),
      usage: BudgetUsage::default(),
      observed_at: time(30),
    },
  )
  .unwrap();
  let resolved = FlowRuntimeHistory {
    completions: std::slice::from_ref(&completion),
    ..history
  };
  validate_flow_runtime_history(&admitted, resolved).unwrap();
  let validated = admitted.validated().unwrap();
  assert_eq!(
    FlowInterpreter::new(&validated)
      .progress(admitted.root_run(), admitted.initial_cycle(), resolved)
      .unwrap(),
    FlowProgress::Terminal {
      key: key("done"),
      source: attempts[1].id()
    },
    "the old failed execution must not block the retry's declared terminal"
  );
}

#[test]
fn retry_capacity_is_explicit_configuration_and_part_of_the_admitted_execution_bound() {
  let (_, admitted, _, input, _) = execution_fixture(FlowNodeKind::Reasoning);
  let original = admitted.closure().definition(input.definition()).unwrap();
  assert_eq!(
    original.nodes()[0].max_execution_retries(),
    0,
    "no implicit retry permission"
  );
  let configured = original.nodes()[0].clone().with_execution_retries(2).unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: original.reference().id(),
    version: original.reference().version(),
    input_schema: original.input_schema().cloned(),
    entry: original.entry().clone(),
    nodes: vec![configured],
    transitions: original.transitions().to_vec(),
    terminals: original.terminals().to_vec(),
    context_projections: original.context_projections().to_vec(),
    data_projections: original.data_projections().to_vec(),
    execution: original.execution().clone(),
  })
  .unwrap();
  assert_ne!(definition.reference(), original.reference());
  let closure = PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap();
  assert_eq!(
    closure.validate(admitted.limits().clone()).unwrap().expanded_nodes(),
    3,
    "one original operation and two confirmed retries"
  );
  assert!(original.nodes()[0].clone().with_execution_retries(u16::MAX).is_err());
}

#[test]
fn a_common_result_cannot_predate_the_frozen_node_execution_start() {
  let (_, admitted, schema, input, original) = execution_fixture(FlowNodeKind::Reasoning);
  let definition = admitted.closure().definition(input.definition()).unwrap();
  let attempt = NodeAttempt::new(
    admitted.root_run(),
    admitted.initial_cycle(),
    definition,
    NodeAttemptInput {
      id: original.id(),
      node_key: original.node_key().clone(),
      node_kind: original.node_kind(),
      number: original.number(),
      input_digest: original.input_digest(),
      budget: original.budget(),
      started_at: octacity_server_domain::Timestamp::from_unix_millis(11).unwrap(),
      deadline: original.deadline(),
      execution: original.execution().clone(),
      ownership: FactoryClaimOwnership::new(original.owner().clone(), original.claim()),
    },
  )
  .unwrap();
  assert!(
    FlowNodeRecord::new(
      &input,
      &attempt,
      definition,
      FlowRecordObservation {
        outcome: key("observed"),
        payload: FlowPayload::new(&schema, serde_json::json!({"count":9})).unwrap(),
        producer: attempt.execution().clone(),
        observed_at: octacity_server_domain::Timestamp::from_unix_millis(10).unwrap(),
      }
    )
    .is_err(),
    "an active creator claim does not permit results from before execution started"
  );
}

#[test]
fn generic_build_execution_links_retain_the_exact_operation_attempt_and_complete_job_set() {
  let (_, admitted, _, input, node) = execution_fixture(FlowNodeKind::Reasoning);
  let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
  let build = octacity_server_domain::BuildId::generate();
  let attempt = octacity_server_domain::AttemptId::generate();
  let jobs = vec![
    octacity_server_domain::JobId::generate(),
    octacity_server_domain::JobId::generate(),
  ];
  let link = FlowBuildExecution::new(&intent, build, attempt, jobs.clone()).unwrap();
  assert_eq!(link.operation_id(), intent.operation_id().unwrap());
  assert_eq!(link.node_attempt_id(), intent.node().id());
  assert_eq!(link.build_id(), build);
  assert_eq!(link.attempt_id(), attempt);
  assert_eq!(link.jobs().len(), 2);
  assert!(FlowBuildExecution::new(&intent, build, attempt, vec![]).is_err());
  assert!(FlowBuildExecution::new(&intent, build, attempt, vec![jobs[0], jobs[0]]).is_err());
  let bytes = serde_json::to_vec(&link).unwrap();
  assert_eq!(
    FlowBuildExecution::restore(&bytes, &intent, link.digest().unwrap()).unwrap(),
    link
  );
  let mut changed = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
  changed["build_id"] = serde_json::json!(octacity_server_domain::BuildId::generate());
  assert!(
    FlowBuildExecution::restore(&serde_json::to_vec(&changed).unwrap(), &intent, link.digest().unwrap()).is_err()
  );
}

fn node(kind: FlowNodeKind) -> FlowNodeDefinition {
  FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("accessibility_audit"),
    kind,
    input_schema: Some(reference("audit.input")),
    outcomes: vec![FlowOutcomeDefinition::new(
      key("observed"),
      FlowOutcomeKind::Success,
      reference("audit.result"),
    )],
    budget: BudgetLimit::new(1, 100, 10, 10, 100).unwrap(),
    permissions: FactoryPermissionSet::deny_all(),
    required: true,
    subflow: None,
  })
  .unwrap()
}

#[test]
fn configured_build_profile_is_frozen_in_an_arbitrary_command_or_reasoning_node() {
  let binding = binding(octacity_server_domain::ProjectId::generate());
  for kind in [FlowNodeKind::BuildCommand, FlowNodeKind::Reasoning] {
    let configured = node(kind).with_build(binding.clone()).unwrap();
    let restored: FlowNodeDefinition = serde_json::from_slice(&serde_json::to_vec(&configured).unwrap()).unwrap();
    assert_eq!(restored, configured);
    assert_eq!(restored.build(), Some(&binding));
    assert_eq!(restored.build().unwrap().result_outcome.as_str(), "observed");
    assert_eq!(
      restored.build().unwrap().model_or_tool.identity().as_str(),
      "audit.parameters"
    );
  }
}

#[test]
fn configured_build_binding_rejects_non_build_nodes_and_undeclared_observation_outcomes() {
  let binding = binding(octacity_server_domain::ProjectId::generate());
  assert!(
    node(FlowNodeKind::DeterministicGate)
      .with_build(binding.clone())
      .is_err()
  );
  let mut wrong_outcome = binding.clone();
  wrong_outcome.result_outcome = key("provider-selected-route");
  assert!(node(FlowNodeKind::Reasoning).with_build(wrong_outcome).is_err());
  let mut wire = serde_json::to_value(node(FlowNodeKind::Reasoning).with_build(binding).unwrap()).unwrap();
  wire["kind"] = serde_json::json!("deterministic_gate");
  let restored: FlowNodeDefinition = serde_json::from_value(wire).unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: restored.input_schema().cloned(),
    entry: restored.key().clone(),
    nodes: vec![restored],
    transitions: vec![FlowTransition::new(
      key("accessibility_audit"),
      key("observed"),
      FlowTransitionTarget::Terminal(key("done")),
    )],
    terminals: vec![FlowTerminalDefinition::new(key("done"), reference("audit.result"))],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(
      BudgetLimit::new(1, 100, 10, 10, 100).unwrap(),
      FactoryPermissionSet::deny_all(),
      1,
    )
    .unwrap(),
  });
  assert!(definition.is_err());
}

#[test]
fn configured_evidence_is_frozen_and_cannot_alias_the_observation_report() {
  let mut selected = binding(octacity_server_domain::ProjectId::generate());
  selected.evidence = vec![EvidenceRequirement::new(
    key("audit.evidence"),
    EvidenceOutputKind::Report,
    reference("audit.evidence.schema"),
    reference("audit.verifier"),
    reference("verifier.plugin"),
  )];
  let configured = node(FlowNodeKind::Reasoning).with_build(selected.clone()).unwrap();
  assert_eq!(
    configured.build().unwrap().evidence[0].tool().identity().as_str(),
    "audit.verifier"
  );
  let restored: FlowNodeDefinition = serde_json::from_slice(&serde_json::to_vec(&configured).unwrap()).unwrap();
  assert_eq!(restored, configured);
  selected.evidence.push(selected.evidence[0].clone());
  assert!(node(FlowNodeKind::Reasoning).with_build(selected.clone()).is_err());
  selected.evidence = vec![EvidenceRequirement::new(
    selected.result_output.clone(),
    EvidenceOutputKind::Report,
    reference("audit.result"),
    reference("audit.verifier"),
    reference("verifier.plugin"),
  )];
  assert!(node(FlowNodeKind::Reasoning).with_build(selected).is_err());
}

#[test]
fn generic_build_intent_replays_the_exact_node_input_profile_priority_and_resource_reservation() {
  for kind in [FlowNodeKind::BuildCommand, FlowNodeKind::Reasoning] {
    let (_, admitted, _, input, attempt) = execution_fixture(kind);
    let usage_before = BudgetUsage {
      attempts: 1,
      elapsed_millis: 20,
      ..BudgetUsage::default()
    };
    let intent = FlowBuildIntent::new(input.clone(), &admitted, attempt.clone(), usage_before, 7).unwrap();
    assert_eq!(intent.profile().node.as_str(), "accessibility_audit");
    assert_eq!(intent.profile().definition, admitted.closure().root());
    assert_eq!(intent.profile().tool.identity().as_str(), "audit.command");
    assert_eq!(intent.input(), &input);
    assert_eq!(intent.priority(), 7);
    assert_eq!(intent.usage_before(), usage_before);
    assert_eq!(intent.budget(), attempt.budget());
    assert_eq!(intent.result_outcome().as_str(), "observed");
    let bytes = serde_json::to_vec(&intent).unwrap();
    let restored =
      FlowBuildIntent::restore(&bytes, &input, &admitted, &attempt, intent.operation_id().unwrap()).unwrap();
    assert_eq!(restored, intent);
    assert_eq!(restored.operation_id().unwrap(), intent.operation_id().unwrap());
  }
}

#[test]
fn published_capture_scopes_require_portable_signed_job_paths() {
  let selected = binding(octacity_server_domain::ProjectId::generate());
  for (paths, accepted) in [
    (serde_json::json!(["contracts", "plans/changes"]), true),
    (serde_json::json!(["contracts:next"]), false),
    (serde_json::json!(["C:/contracts"]), false),
  ] {
    let mut wire = serde_json::to_value(&selected).unwrap();
    wire["capture_write_paths"] = paths.clone();
    let published = serde_json::from_value::<FlowBuildBinding>(wire)
      .ok()
      .and_then(|binding| node(FlowNodeKind::Reasoning).with_build(binding).ok());
    assert_eq!(published.is_some(), accepted, "capture scope {paths}");
  }
}

#[test]
fn generic_build_intent_rejects_substitution_expired_reservations_and_tampered_restart_bytes() {
  let (_, admitted, _, input, attempt) = execution_fixture(FlowNodeKind::Reasoning);
  let mut wire = serde_json::to_value(&attempt).unwrap();
  // Start is 10ms and the declared execution ceiling is 1000ms; the worker
  // lease ending at 100ms is an independent observation fence.
  wire["deadline"] = serde_json::to_value(octacity_server_domain::Timestamp::from_unix_millis(1011).unwrap()).unwrap();
  let changed: NodeAttempt = serde_json::from_value(wire).unwrap();
  assert!(FlowBuildIntent::new(input.clone(), &admitted, changed, BudgetUsage::default(), 7).is_err());
  let intent = FlowBuildIntent::new(input.clone(), &admitted, attempt.clone(), BudgetUsage::default(), 7).unwrap();
  for (field, value) in [
    ("priority", serde_json::json!(8)),
    ("result_outcome", serde_json::json!("provider-route")),
    (
      "usage_before",
      serde_json::json!({"attempts": 1,"elapsed_millis":0,"tokens":0,"cost_micro_units":0,"output_bytes":0}),
    ),
  ] {
    let mut wire = serde_json::to_value(&intent).unwrap();
    wire[field] = value;
    assert!(
      FlowBuildIntent::restore(
        &serde_json::to_vec(&wire).unwrap(),
        &input,
        &admitted,
        &attempt,
        intent.operation_id().unwrap()
      )
      .is_err()
    );
  }
  let exhausted = BudgetUsage {
    attempts: admitted.limits().budget().max_attempts(),
    ..BudgetUsage::default()
  };
  assert!(FlowBuildIntent::new(input, &admitted, attempt, exhausted, 7).is_err());
}

#[test]
fn an_external_build_record_cannot_complete_without_verified_retained_provenance() {
  let (_, admitted, schema, input, attempt) = execution_fixture(FlowNodeKind::Reasoning);
  let definition = admitted.closure().definition(admitted.closure().root()).unwrap();
  let at = octacity_server_domain::Timestamp::from_unix_millis(30).unwrap();
  let record = FlowNodeRecord::new(
    &input,
    &attempt,
    definition,
    FlowRecordObservation {
      outcome: key("observed"),
      payload: FlowPayload::new(&schema, serde_json::json!({"count":9})).unwrap(),
      producer: attempt.execution().clone(),
      observed_at: at,
    },
  )
  .unwrap();
  assert!(
    record
      .completion(
        &attempt,
        definition,
        FactoryClaimOwnership::new(attempt.owner().clone(), attempt.claim()),
        BudgetUsage::default(),
        at
      )
      .is_err()
  );
}

#[test]
fn an_ordinary_build_cannot_select_another_declared_semantic_outcome() {
  let (work, admitted, schema, _, original_attempt) = execution_fixture(FlowNodeKind::Reasoning);
  let original = admitted.closure().definition(admitted.closure().root()).unwrap();
  let mut node = serde_json::to_value(&original.nodes()[0]).unwrap();
  node["outcomes"].as_array_mut().unwrap().push(
    serde_json::to_value(FlowOutcomeDefinition::new(
      key("provider_accepts"),
      FlowOutcomeKind::Success,
      schema.reference().clone(),
    ))
    .unwrap(),
  );
  let node: FlowNodeDefinition = serde_json::from_value(node).unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: original.input_schema().cloned(),
    entry: node.key().clone(),
    nodes: vec![node],
    transitions: vec![
      FlowTransition::new(
        key("accessibility_audit"),
        key("observed"),
        FlowTransitionTarget::Terminal(key("done")),
      ),
      FlowTransition::new(
        key("accessibility_audit"),
        key("provider_accepts"),
        FlowTransitionTarget::Terminal(key("done")),
      ),
    ],
    terminals: original.terminals().to_vec(),
    context_projections: vec![],
    data_projections: vec![],
    execution: original.execution().clone(),
  })
  .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let flow = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition.clone()]).unwrap(),
    admitted.limits().clone(),
    flow,
    cycle,
    crate::FactoryWipLimits::new(8, 16).unwrap(),
  )
  .unwrap()
  .with_data_schemas(vec![schema.clone()])
  .unwrap();
  let input = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    key("accessibility_audit"),
    FlowPayload::new(&schema, serde_json::json!({"count":3})).unwrap(),
  )
  .unwrap();
  let attempt = NodeAttempt::new(
    admitted.root_run(),
    admitted.initial_cycle(),
    &definition,
    NodeAttemptInput {
      started_at: original_attempt.started_at(),
      id: NodeAttemptId::generate(),
      node_key: input.node().clone(),
      node_kind: FlowNodeKind::Reasoning,
      number: NodeAttemptNumber::INITIAL,
      input_digest: input.digest().unwrap(),
      budget: original_attempt.budget(),
      deadline: original_attempt.deadline(),
      execution: original_attempt.execution().clone(),
      ownership: FactoryClaimOwnership::new(original_attempt.owner().clone(), original_attempt.claim()),
    },
  )
  .unwrap();
  assert!(
    FlowNodeRecord::new(
      &input,
      &attempt,
      &definition,
      FlowRecordObservation {
        outcome: key("provider_accepts"),
        payload: FlowPayload::new(&schema, serde_json::json!({"count":9})).unwrap(),
        producer: attempt.execution().clone(),
        observed_at: octacity_server_domain::Timestamp::from_unix_millis(30).unwrap()
      }
    )
    .is_err()
  );
}

#[test]
fn interpretation_charges_confirmed_failure_usage_before_advancing_a_successful_retry() {
  let time = |value| octacity_server_domain::Timestamp::from_unix_millis(value).unwrap();
  let (work, template, schema, _, original) = execution_fixture_with_retries(FlowNodeKind::Reasoning, 1);
  let old = &template.closure().definitions()[0];
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: old.reference().id(),
    version: old.reference().version(),
    input_schema: old.input_schema().cloned(),
    entry: old.entry().clone(),
    nodes: old.nodes().to_vec(),
    transitions: old.transitions().to_vec(),
    terminals: old.terminals().to_vec(),
    context_projections: old.context_projections().to_vec(),
    data_projections: old.data_projections().to_vec(),
    execution: FlowExecutionPolicy::new(
      BudgetLimit::new(16, 10000, 100, 10000, 10000).unwrap(),
      FactoryPermissionSet::deny_all(),
      1,
    )
    .unwrap(),
  })
  .unwrap();
  let run = FactoryRun::admitted(template.root_run().factory_run_id(), &work);
  let flow = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition.clone()]).unwrap(),
    template.limits().clone(),
    flow.clone(),
    cycle.clone(),
    template.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(vec![schema.clone()])
  .unwrap();
  let input = FlowNodeInput::new(
    &work,
    &admitted,
    &flow,
    &cycle,
    definition.entry().clone(),
    FlowPayload::new(&schema, serde_json::json!({"count":3})).unwrap(),
  )
  .unwrap();
  let ownership = FactoryClaimOwnership::new(original.owner().clone(), original.claim());
  let attempt = |input: &FlowNodeInput, number, started_at| {
    NodeAttempt::new(
      &flow,
      &cycle,
      &definition,
      NodeAttemptInput {
        id: NodeAttemptId::generate(),
        node_key: definition.entry().clone(),
        node_kind: FlowNodeKind::Reasoning,
        number: NodeAttemptNumber::new(number).unwrap(),
        input_digest: input.digest().unwrap(),
        budget: original.budget(),
        started_at: time(started_at),
        deadline: time(started_at + 1000),
        execution: original.execution().clone(),
        ownership: ownership.clone(),
      },
    )
    .unwrap()
  };
  let first = attempt(&input, 1, 10);
  let retry_input = input.retry().unwrap();
  let second = attempt(&retry_input, 2, 20).with_retry_of(&first).unwrap();
  let stop = FlowNodeStop::new(
    &first,
    ownership.clone(),
    FlowNodeStopReason::ExecutionFailed,
    time(15),
    BudgetUsage {
      tokens: 100,
      ..Default::default()
    },
    None,
  )
  .unwrap();
  let completion = NodeAttemptCompletion::new(
    &second,
    &definition,
    NodeAttemptCompletionInput {
      outcome: key("observed"),
      output_schema: schema.reference().clone(),
      output_digest: FactoryDigest::content_sha256(br#"{"count":9}"#),
      ownership,
      usage: BudgetUsage {
        tokens: 1,
        ..Default::default()
      },
      observed_at: time(25),
    },
  )
  .unwrap();
  let attempts = [first, second];
  let history = FlowRuntimeHistory {
    stops: std::slice::from_ref(&stop),
    flow_runs: std::slice::from_ref(&flow),
    cycles: std::slice::from_ref(&cycle),
    attempts: &attempts,
    completions: std::slice::from_ref(&completion),
  };
  assert!(matches!(
    validate_flow_runtime_history(&admitted, history),
    Err(FactoryError::BudgetExceeded {
      resource: BudgetResource::Tokens
    })
  ));
  let validated = admitted.validated().unwrap();
  assert!(
    matches!(
      FlowInterpreter::new(&validated).advance(&flow, &cycle, &attempts[1], &completion, history),
      Err(FactoryError::BudgetExceeded {
        resource: BudgetResource::Tokens
      })
    ),
    "100 tokens spent by a failed attempt plus one retry token exceed the configured Flow budget"
  );
}

#[test]
fn a_node_context_contract_excludes_undeclared_sources_and_substituted_candidates() {
  let (work, original, schema, _, previous) = execution_fixture(FlowNodeKind::Reasoning);
  let old = &original.closure().definitions()[0];
  let task = work.artifacts().task().clone();
  let skills = FactoryArtifactReference::new(
    octacity_server_domain::ArtifactId::generate(),
    FactoryDigest::from_bytes([74; 32]),
    42,
  )
  .unwrap();
  let source = |artifact: &FactoryArtifactReference| serde_json::json!({"value":{"mapping":"constant","value":{"kind":"artifact","value":artifact}}});
  let mut wire = serde_json::to_value(&old.nodes()[0]).unwrap();
  wire["context_binding"] = serde_json::json!({
    "candidate_revision":{"value":{"mapping":"constant","value":"spec-candidate"}},
    "entries":{
      "requirements":{"source_kind":"specification","source":source(&task)},
      "reviewer_skills":{"source_kind":"policy","source":source(&skills)}
    }
  });
  let selected: FlowNodeDefinition =
    serde_json::from_value(wire).expect("node context is configured independently of its name");
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: old.reference().id(),
    version: old.reference().version(),
    input_schema: old.input_schema().cloned(),
    entry: old.entry().clone(),
    nodes: vec![selected],
    transitions: old.transitions().to_vec(),
    terminals: old.terminals().to_vec(),
    context_projections: old.context_projections().to_vec(),
    data_projections: old.data_projections().to_vec(),
    execution: old.execution().clone(),
  })
  .unwrap();
  let run = FactoryRun::admitted(original.root_run().factory_run_id(), &work);
  let flow = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition.clone()]).unwrap(),
    original.limits().clone(),
    flow,
    cycle,
    FactoryWipLimits::new(8, 16).unwrap(),
  )
  .unwrap()
  .with_data_schemas(vec![schema.clone()])
  .unwrap();
  let input = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    key("accessibility_audit"),
    FlowPayload::new(&schema, serde_json::json!({"count":3})).unwrap(),
  )
  .unwrap();
  let subject = FactoryTaskSubject::Candidate(CandidateSubject::new(
    work.subject().clone(),
    octacity_server_domain::ImmutableRevision::new("spec-candidate").unwrap(),
    FactoryDigest::from_bytes([75; 32]),
  ));
  let entry = |name, kind, subject: &FactoryTaskSubject, artifact: &FactoryArtifactReference| {
    ContextManifestEntry::new(
      kind,
      key(name),
      subject.clone(),
      FactoryContextReference::Artifact(artifact.clone()),
      artifact.content_digest(),
      artifact.encoded_size(),
      FactorySafeText::new("Explicit independent input").unwrap(),
      FactoryDigest::from_bytes([76; 32]),
    )
    .unwrap()
  };
  let entries = vec![
    entry("requirements", ContextSourceKind::Specification, &subject, &task),
    entry("reviewer_skills", ContextSourceKind::Policy, &subject, &skills),
  ];
  let attach = |subject: FactoryTaskSubject, entries| {
    input
      .clone()
      .with_context(
        ContextManifest::new(
          ContextManifestId::generate(),
          subject,
          FactoryDigest::from_bytes([76; 32]),
          entries,
        )
        .unwrap(),
        vec![],
      )
      .unwrap()
  };
  let intent = |input: FlowNodeInput| {
    let node = NodeAttempt::new(
      admitted.root_run(),
      admitted.initial_cycle(),
      &definition,
      NodeAttemptInput {
        id: NodeAttemptId::generate(),
        node_key: input.node().clone(),
        node_kind: previous.node_kind(),
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
    FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 0)
  };
  let declared = definition.node(input.node()).unwrap().context_binding().unwrap();
  let sources = declared.selected_sources(input.payload()).unwrap();
  assert_eq!(sources.len(), 2);
  assert_eq!(
    sources[&key("reviewer_skills")],
    (
      ContextSourceKind::Policy,
      FactoryContextReference::Artifact(skills.clone())
    )
  );
  assert_eq!(
    declared.candidate_revision(input.payload()).unwrap().unwrap().as_str(),
    "spec-candidate"
  );
  let valid = attach(subject.clone(), entries.clone());
  assert!(intent(valid.clone()).is_ok());
  let mut leaked = entries.clone();
  leaked.push(entry(
    "author_transcript",
    ContextSourceKind::BoundedSummary,
    &subject,
    &task,
  ));
  assert!(
    intent(attach(subject.clone(), leaked)).is_err(),
    "control order grants no transcript"
  );
  let mut substituted = entries.clone();
  substituted[1] = entry("reviewer_skills", ContextSourceKind::Policy, &subject, &task);
  assert!(
    intent(attach(subject.clone(), substituted)).is_err(),
    "reviewer skills are exact immutable bytes"
  );
  assert!(
    intent(attach(subject.clone(), vec![entries[0].clone()])).is_err(),
    "required context cannot be omitted"
  );
  let foreign = FactoryTaskSubject::Candidate(CandidateSubject::new(
    work.subject().clone(),
    octacity_server_domain::ImmutableRevision::new("other-candidate").unwrap(),
    FactoryDigest::from_bytes([75; 32]),
  ));
  let foreign_entries = vec![
    entry("requirements", ContextSourceKind::Specification, &foreign, &task),
    entry("reviewer_skills", ContextSourceKind::Policy, &foreign, &skills),
  ];
  assert!(intent(attach(foreign, foreign_entries)).is_err());
  assert!(intent(input).is_err());
  let restored = FlowNodeInput::restore(
    &serde_json::to_vec(&valid).unwrap(),
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    &schema,
    valid.digest().unwrap(),
  )
  .unwrap();
  assert_eq!(restored, valid);
}

#[test]
fn a_context_contract_requires_a_typed_input_at_publication() {
  let (_, admitted, _, _, _) = execution_fixture(FlowNodeKind::Reasoning);
  let old = &admitted.closure().definitions()[0];
  let mut wire = serde_json::to_value(&old.nodes()[0]).unwrap();
  wire["input_schema"] = serde_json::Value::Null;
  wire["build"] = serde_json::Value::Null;
  wire["context_binding"] = serde_json::json!({"entries":{"declared":{"source_kind":"task","source":{"value":{"mapping":"constant","value":{}}}}}});
  let node = serde_json::from_value(wire).unwrap();
  assert!(
    FlowDefinition::new(FlowDefinitionInput {
      id: old.reference().id(),
      version: old.reference().version(),
      input_schema: None,
      entry: old.entry().clone(),
      nodes: vec![node],
      transitions: old.transitions().to_vec(),
      terminals: old.terminals().to_vec(),
      context_projections: vec![],
      data_projections: vec![],
      execution: old.execution().clone(),
    })
    .is_err(),
    "a source mapping cannot read undeclared untyped input"
  );
}
