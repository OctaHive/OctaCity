use crate::*;

use crate::flow_test_support::*;

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
fn generic_build_intent_rejects_substitution_expired_reservations_and_tampered_restart_bytes() {
  let (_, admitted, _, input, attempt) = execution_fixture(FlowNodeKind::Reasoning);
  let mut wire = serde_json::to_value(&attempt).unwrap();
  wire["deadline"] = serde_json::to_value(octacity_server_domain::Timestamp::from_unix_millis(101).unwrap()).unwrap();
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
