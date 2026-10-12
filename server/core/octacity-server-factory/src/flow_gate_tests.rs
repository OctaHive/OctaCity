use crate::*;

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn reference(name: &str) -> ImmutableReference {
  ImmutableReference::new(key(name), key("v1"), FactoryDigest::from_bytes([60; 32]))
}
fn gate(name: &str, kind: FlowNodeKind) -> FlowNodeDefinition {
  FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key(name),
    kind,
    input_schema: Some(reference("observation")),
    outcomes: vec![FlowOutcomeDefinition::new(
      key("ready"),
      FlowOutcomeKind::Success,
      reference("gate.result"),
    )],
    budget: BudgetLimit::new(1, 100, 10, 10, 100).unwrap(),
    permissions: FactoryPermissionSet::deny_all(),
    required: true,
    subflow: None,
  })
  .unwrap()
}

#[test]
fn operator_requests_bind_a_closed_purpose_to_arbitrary_human_outcomes() {
  use std::collections::BTreeMap;
  let binding = FlowHumanGateBinding::new(
    FlowHumanOperation::Delivery,
    BTreeMap::from([(FlowHumanDisposition::Confirm, key("ready"))]),
  )
  .unwrap();
  let node = gate("caller-selected-approval", FlowNodeKind::HumanGate)
    .with_operator_requests(binding.clone())
    .unwrap();
  assert_eq!(node.operator_requests(), Some(&binding));
  assert_eq!(binding.outcome(FlowHumanDisposition::Confirm), Some(&key("ready")));
  let restored: FlowNodeDefinition = serde_json::from_slice(&serde_json::to_vec(&node).unwrap()).unwrap();
  assert_eq!(restored, node);
  assert!(
    gate("arbitrary-model", FlowNodeKind::Reasoning)
      .with_operator_requests(binding)
      .is_err()
  );
  assert!(
    FlowHumanGateBinding::new(
      FlowHumanOperation::Delivery,
      BTreeMap::from([(FlowHumanDisposition::Resume, key("ready"))])
    )
    .is_err()
  );
  assert!(
    gate("another-approval", FlowNodeKind::HumanGate)
      .with_operator_requests(
        FlowHumanGateBinding::new(
          FlowHumanOperation::Promotion,
          BTreeMap::from([(FlowHumanDisposition::Confirm, key("undeclared"))])
        )
        .unwrap()
      )
      .is_err()
  );
}

#[test]
fn operator_request_publication_rejects_an_incompatible_response_contract() {
  use std::collections::BTreeMap;
  let schema = FlowDataSchema::new(key("gate.result"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let mut encoded = serde_json::to_value(gate("caller-approval", FlowNodeKind::HumanGate)).unwrap();
  encoded["input_schema"] = serde_json::Value::Null;
  encoded["outcomes"][0]["schema"] = serde_json::to_value(schema.reference()).unwrap();
  let node: FlowNodeDefinition = serde_json::from_value(encoded).unwrap();
  let node = node
    .with_operator_requests(
      FlowHumanGateBinding::new(
        FlowHumanOperation::Promotion,
        BTreeMap::from([(FlowHumanDisposition::Confirm, key("ready"))]),
      )
      .unwrap(),
    )
    .unwrap();
  let budget = node.budget();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: None,
    entry: node.key().clone(),
    nodes: vec![node],
    transitions: vec![FlowTransition::new(
      key("caller-approval"),
      key("ready"),
      FlowTransitionTarget::Terminal(key("end")),
    )],
    terminals: vec![FlowTerminalDefinition::new(key("end"), schema.reference().clone())],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(budget, FactoryPermissionSet::deny_all(), 1).unwrap(),
  })
  .unwrap();
  let publication = FactoryFlowConfiguration::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    FlowAdmissionLimits::product_defaults(1, budget, FactoryPermissionSet::deny_all()).unwrap(),
    vec![schema],
    BTreeMap::new(),
  );
  assert!(
    publication.is_err(),
    "a published operator request must accept its closed typed response envelope"
  );
}

#[test]
fn arbitrary_named_gate_keeps_exact_policy_parameters_without_business_phase_fields() {
  let binding = FlowGateBinding::new(
    reference("project.readiness"),
    serde_json::json!({"minimum_score": 80, "required_reports": ["security", "performance"]}),
  )
  .unwrap();
  let node = gate("my-release-check", FlowNodeKind::DeterministicGate)
    .with_gate(binding.clone())
    .unwrap();
  assert_eq!(node.key().as_str(), "my-release-check");
  assert_eq!(node.gate(), Some(&binding));
  assert_eq!(binding.policy(), &reference("project.readiness"));
  assert_eq!(binding.parameters()["minimum_score"], serde_json::json!(80));
  let restored: FlowNodeDefinition = serde_json::from_slice(&serde_json::to_vec(&node).unwrap()).unwrap();
  assert_eq!(restored, node);
  assert!(gate("my-model", FlowNodeKind::Reasoning).with_gate(binding).is_err());
  assert!(FlowGateBinding::new(reference("project.readiness"), serde_json::json!([])).is_err());
  assert!(
    FlowGateBinding::new(
      reference("project.readiness"),
      serde_json::json!({"description": "x".repeat(MAX_FLOW_GATE_PARAMETER_BYTES)})
    )
    .is_err()
  );
}
