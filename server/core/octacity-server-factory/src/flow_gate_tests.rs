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
