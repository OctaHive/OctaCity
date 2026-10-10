use crate::flow_test_support::key;
use crate::*;

#[test]
fn common_history_requires_frozen_inputs_and_exact_records_for_every_completed_node() {
  let (work, admitted, schema) = crate::flow_data_tests::node_fixture();
  let admitted = admitted.with_data_schemas(vec![schema.clone()]).unwrap();
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
  let ownership = FactoryClaimOwnership::new(
    key("worker"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([3; 32])),
      at,
      octacity_server_domain::Timestamp::from_unix_millis(100).unwrap(),
    )
    .unwrap(),
  );
  let definition = admitted.closure().definition(admitted.closure().root()).unwrap();
  let attempt = NodeAttempt::new(
    admitted.root_run(),
    admitted.initial_cycle(),
    definition,
    NodeAttemptInput {
      id: NodeAttemptId::generate(),
      node_key: key("accessibility_audit"),
      node_kind: FlowNodeKind::DeterministicGate,
      number: NodeAttemptNumber::INITIAL,
      input_digest: input.digest().unwrap(),
      budget: definition.nodes()[0].budget(),
      deadline: ownership.claim().expires_at(),
      execution: NodeExecutionIdentity::BuiltIn,
      ownership: ownership.clone(),
    },
  )
  .unwrap();
  let attempts = [attempt.clone()];
  let history = |completions| FlowRuntimeHistory {
    flow_runs: std::slice::from_ref(admitted.root_run()),
    cycles: std::slice::from_ref(admitted.initial_cycle()),
    attempts: &attempts,
    completions,
  };
  let mut journal = FlowDataHistory::default();
  assert!(journal.validate(&work, &admitted, history(&[])).is_err());
  journal.inputs.push(input.clone());
  journal.validate(&work, &admitted, history(&[])).unwrap();
  let record = FlowNodeRecord::new(
    &input,
    &attempt,
    definition,
    FlowRecordObservation {
      outcome: key("observed"),
      payload: FlowPayload::new(&schema, serde_json::json!({"count":9})).unwrap(),
      producer: NodeExecutionIdentity::BuiltIn,
      observed_at: at,
    },
  )
  .unwrap();
  let completion = record
    .completion(&attempt, definition, ownership, BudgetUsage::default(), at)
    .unwrap();
  let completions = [completion];
  assert!(journal.validate(&work, &admitted, history(&completions)).is_err());
  journal.records.push(record.clone());
  journal.validate(&work, &admitted, history(&completions)).unwrap();
  journal.records.push(record);
  assert!(journal.validate(&work, &admitted, history(&completions)).is_err());
}

#[test]
fn the_journal_recomputes_configured_gate_results_instead_of_accepting_supplied_decisions() {
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
  let admitted = admitted.with_data_schemas(vec![schema.clone()]).unwrap();
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
  let claim = FactoryClaim::new(
    FactoryClaimFence::new(FactoryDigest::from_bytes([3; 32])),
    at,
    octacity_server_domain::Timestamp::from_unix_millis(100).unwrap(),
  )
  .unwrap();
  let ownership = FactoryClaimOwnership::new(key("worker"), claim);
  let definition = admitted.closure().definition(admitted.closure().root()).unwrap();
  let attempt = NodeAttempt::new(
    admitted.root_run(),
    admitted.initial_cycle(),
    definition,
    NodeAttemptInput {
      id: NodeAttemptId::generate(),
      node_key: key("accessibility_audit"),
      node_kind: FlowNodeKind::DeterministicGate,
      number: NodeAttemptNumber::INITIAL,
      input_digest: input.digest().unwrap(),
      budget: definition.nodes()[0].budget(),
      deadline: claim.expires_at(),
      execution: NodeExecutionIdentity::BuiltIn,
      ownership: ownership.clone(),
    },
  )
  .unwrap();
  let forged = FlowNodeRecord::new(
    &input,
    &attempt,
    definition,
    FlowRecordObservation {
      outcome: key("observed"),
      payload: FlowPayload::new(&schema, serde_json::json!({"count":9})).unwrap(),
      producer: NodeExecutionIdentity::BuiltIn,
      observed_at: at,
    },
  )
  .unwrap();
  let validate = |record: FlowNodeRecord| {
    let completion = record
      .completion(&attempt, definition, ownership.clone(), BudgetUsage::default(), at)
      .unwrap();
    FlowDataHistory {
      inputs: vec![input.clone()],
      records: vec![record],
      incoming: vec![],
      ..FlowDataHistory::default()
    }
    .validate(
      &work,
      &admitted,
      FlowRuntimeHistory {
        flow_runs: std::slice::from_ref(admitted.root_run()),
        cycles: std::slice::from_ref(admitted.initial_cycle()),
        attempts: std::slice::from_ref(&attempt),
        completions: &[completion],
      },
    )
  };
  assert!(validate(forged).is_err());
  let accepted = program.record(&input, &attempt, definition, &schema, at).unwrap();
  assert_eq!(accepted.payload().value(), &serde_json::json!({"count":4}));
  validate(accepted).unwrap();
}

#[test]
fn configured_input_mapping_cannot_be_replaced_with_arbitrary_schema_valid_data() {
  let binding = FlowInputBinding::new(
    Default::default(),
    FlowDataMapping::new(FlowValueMapping::Constant {
      value: serde_json::json!({"count":4}),
    })
    .unwrap(),
  )
  .unwrap();
  let (work, admitted, schema) = crate::flow_data_tests::node_fixture_with_bindings(None, Some(binding));
  let admitted = admitted.with_data_schemas(vec![schema.clone()]).unwrap();
  let make = |count| {
    FlowNodeInput::new(
      &work,
      &admitted,
      admitted.root_run(),
      admitted.initial_cycle(),
      key("accessibility_audit"),
      FlowPayload::new(&schema, serde_json::json!({"count":count})).unwrap(),
    )
    .unwrap()
  };
  let history = FlowRuntimeHistory {
    flow_runs: std::slice::from_ref(admitted.root_run()),
    cycles: std::slice::from_ref(admitted.initial_cycle()),
    attempts: &[],
    completions: &[],
  };
  assert!(
    FlowDataHistory {
      inputs: vec![make(9)],
      ..Default::default()
    }
    .validate(&work, &admitted, history)
    .is_err()
  );
  FlowDataHistory {
    inputs: vec![make(4)],
    ..Default::default()
  }
  .validate(&work, &admitted, history)
  .unwrap();
}
