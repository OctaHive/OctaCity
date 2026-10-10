use crate::flow_test_support::*;
use crate::*;
#[test]
fn accepted_work_is_frozen_by_a_configured_deterministic_outcome_with_exact_work_and_policy() {
  let program = FlowGateProgram::new(vec![], key("observed"))
    .unwrap()
    .with_output_mapping(
      key("observed"),
      FlowDataMapping::new(FlowValueMapping::Constant {
        value: serde_json::json!({"count":4}),
      })
      .unwrap(),
    )
    .unwrap();
  let (work, admitted, schema) =
    crate::flow_data_tests::node_fixture_with_bindings(Some(program.binding().unwrap()), None);
  let original = admitted.closure().definition(admitted.closure().root()).unwrap();
  let declared = original.nodes()[0]
    .clone()
    .with_accepted_work_outcomes(vec![key("observed")])
    .unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: original.input_schema().cloned(),
    entry: declared.key().clone(),
    nodes: vec![declared],
    transitions: original.transitions().to_vec(),
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
  let at = octacity_server_domain::Timestamp::from_unix_millis(10).unwrap();
  let expiry = octacity_server_domain::Timestamp::from_unix_millis(100).unwrap();
  let owner = FactoryClaimOwnership::new(
    key("worker"),
    FactoryClaim::new(FactoryClaimFence::new(FactoryDigest::from_bytes([3; 32])), at, expiry).unwrap(),
  );
  let attempt = NodeAttempt::new(
    admitted.root_run(),
    admitted.initial_cycle(),
    &definition,
    NodeAttemptInput {
      id: NodeAttemptId::generate(),
      node_key: input.node().clone(),
      node_kind: FlowNodeKind::DeterministicGate,
      number: NodeAttemptNumber::INITIAL,
      input_digest: input.digest().unwrap(),
      budget: definition.nodes()[0].budget(),
      deadline: expiry,
      execution: NodeExecutionIdentity::BuiltIn,
      ownership: owner.clone(),
    },
  )
  .unwrap();
  let record = program.record(&input, &attempt, &definition, &schema, at).unwrap();
  let completion = record
    .completion(&attempt, &definition, owner, BudgetUsage::default(), at)
    .unwrap();
  let attempts = [attempt.clone()];
  let completions = [completion];
  let runtime = FlowRuntimeHistory {
    flow_runs: std::slice::from_ref(admitted.root_run()),
    cycles: std::slice::from_ref(admitted.initial_cycle()),
    attempts: &attempts,
    completions: &completions,
  };
  let journal = FlowDataHistory {
    inputs: vec![input],
    records: vec![record.clone()],
    ..Default::default()
  };
  let contract = AcceptedWorkContract::freeze(&work, &admitted, &journal, runtime, attempt.id()).unwrap();
  assert_eq!(contract.work(), &work);
  assert_eq!(contract.source_record(), attempt.id());
  assert_eq!(contract.records(), &[record]);
  assert_eq!(contract.work().subject().base_revision().as_str(), "base");
  let bytes = serde_json::to_vec(&contract).unwrap();
  assert_eq!(
    AcceptedWorkContract::restore(
      &bytes,
      &work,
      &admitted,
      &journal,
      runtime,
      attempt.id(),
      contract.digest().unwrap()
    )
    .unwrap(),
    contract
  );
  let mut forged = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
  forged["policy_digest"] = serde_json::json!(FactoryDigest::from_bytes([99; 32]));
  assert!(
    AcceptedWorkContract::restore(
      &serde_json::to_vec(&forged).unwrap(),
      &work,
      &admitted,
      &journal,
      runtime,
      attempt.id(),
      contract.digest().unwrap()
    )
    .is_err()
  );
  let mut forged = journal.clone();
  forged.records.clear();
  assert!(AcceptedWorkContract::freeze(&work, &admitted, &forged, runtime, attempt.id()).is_err());
}
#[test]
fn a_reasoning_node_or_undeclared_outcome_cannot_accept_work() {
  let (_, admitted, _, _, _) = execution_fixture(FlowNodeKind::Reasoning);
  let node = admitted
    .closure()
    .definition(admitted.closure().root())
    .unwrap()
    .nodes()[0]
    .clone();
  assert!(node.with_accepted_work_outcomes(vec![key("observed")]).is_err());
}

#[test]
fn accepting_gate_cannot_commit_an_oversized_accepted_work_projection() {
  let work = work();
  let schema = FlowDataSchema::new(
    key("bounded.acceptance"),
    key("v1"),
    FlowValueSchema::String {
      min_bytes: 0,
      max_bytes: MAX_FLOW_DATA_BYTES,
    },
  )
  .unwrap();
  let program = FlowGateProgram::new(vec![], key("observed"))
    .unwrap()
    .with_output_mapping(
      key("observed"),
      FlowDataMapping::new(FlowValueMapping::Pointer { path: String::new() }).unwrap(),
    )
    .unwrap();
  let budget = BudgetLimit::new(4, 10000, 100, 10000, MAX_FLOW_DATA_BYTES as u64).unwrap();
  let node = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("accept"),
    kind: FlowNodeKind::DeterministicGate,
    input_schema: Some(schema.reference().clone()),
    outcomes: vec![FlowOutcomeDefinition::new(
      key("observed"),
      FlowOutcomeKind::Success,
      schema.reference().clone(),
    )],
    budget,
    permissions: FactoryPermissionSet::deny_all(),
    required: true,
    subflow: None,
  })
  .unwrap()
  .with_gate(program.binding().unwrap())
  .unwrap()
  .with_accepted_work_outcomes(vec![key("observed")])
  .unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(schema.reference().clone()),
    entry: key("accept"),
    nodes: vec![node],
    transitions: vec![FlowTransition::new(
      key("accept"),
      key("observed"),
      FlowTransitionTarget::Terminal(key("done")),
    )],
    terminals: vec![FlowTerminalDefinition::new(key("done"), schema.reference().clone())],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(budget, FactoryPermissionSet::deny_all(), 1).unwrap(),
  })
  .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let flow = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition.clone()]).unwrap(),
    FlowAdmissionLimits::product_defaults(1, budget, FactoryPermissionSet::deny_all()).unwrap(),
    flow,
    cycle,
  )
  .unwrap()
  .with_data_schemas(vec![schema.clone()])
  .unwrap();
  let at = octacity_server_domain::Timestamp::from_unix_millis(10).unwrap();
  let expiry = octacity_server_domain::Timestamp::from_unix_millis(100).unwrap();
  let owner = FactoryClaimOwnership::new(
    key("worker"),
    FactoryClaim::new(FactoryClaimFence::new(FactoryDigest::from_bytes([3; 32])), at, expiry).unwrap(),
  );
  let prepare = |size| {
    let input = FlowNodeInput::new(
      &work,
      &admitted,
      admitted.root_run(),
      admitted.initial_cycle(),
      key("accept"),
      FlowPayload::new(&schema, serde_json::json!("a".repeat(size))).unwrap(),
    )
    .unwrap();
    let attempt = NodeAttempt::new(
      admitted.root_run(),
      admitted.initial_cycle(),
      &definition,
      NodeAttemptInput {
        id: NodeAttemptId::generate(),
        node_key: key("accept"),
        node_kind: FlowNodeKind::DeterministicGate,
        number: NodeAttemptNumber::INITIAL,
        input_digest: input.digest().unwrap(),
        budget,
        deadline: expiry,
        execution: NodeExecutionIdentity::BuiltIn,
        ownership: owner.clone(),
      },
    )
    .unwrap();
    let record = program.record(&input, &attempt, &definition, &schema, at).unwrap();
    let completion = record
      .completion(&attempt, &definition, owner.clone(), BudgetUsage::default(), at)
      .unwrap();
    (input, attempt, record, completion)
  };
  let (_, _, empty, _) = prepare(0);
  let overhead = serde_json::to_vec(&empty).unwrap().len();
  let (input, attempt, record, completion) = prepare((MAX_FLOW_DATA_BYTES - overhead - 200) / 2);
  assert!(serde_json::to_vec(&record).unwrap().len() < MAX_FLOW_DATA_BYTES);
  let journal = FlowDataHistory {
    inputs: vec![input],
    records: vec![record],
    ..Default::default()
  };
  let runtime = FlowRuntimeHistory {
    flow_runs: std::slice::from_ref(admitted.root_run()),
    cycles: std::slice::from_ref(admitted.initial_cycle()),
    attempts: std::slice::from_ref(&attempt),
    completions: std::slice::from_ref(&completion),
  };
  assert!(
    journal.validate(&work, &admitted, runtime).is_err(),
    "the accepting completion must fail atomically when its frozen contract is too large"
  );
}
