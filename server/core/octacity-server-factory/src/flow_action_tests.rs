use crate::*;

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn reference(value: &str) -> ImmutableReference {
  ImmutableReference::new(
    key(value),
    FactoryKey::new("v1").unwrap(),
    FactoryDigest::from_bytes([1; 32]),
  )
}
fn node(kind: FlowNodeKind) -> FlowNodeDefinition {
  FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("finish"),
    kind,
    input_schema: None,
    outcomes: vec![FlowOutcomeDefinition::new(
      key("reported"),
      FlowOutcomeKind::Success,
      reference("action.result"),
    )],
    budget: BudgetLimit::new(1, 1000, 1, 1, 1024).unwrap(),
    permissions: FactoryPermissionSet::deny_all(),
    required: true,
    subflow: None,
  })
  .unwrap()
}

#[test]
fn trusted_action_pins_extensible_plugin_parameters_without_a_status_enum() {
  let parameters = serde_json::json!({"status": "Ожидает уточнения", "notify": false});
  let binding = FlowActionBinding::new(reference("work-reporter"), key("set_status"), parameters.clone()).unwrap();
  let finish = node(FlowNodeKind::TrustedAction).with_action(binding.clone()).unwrap();
  let restored: FlowNodeDefinition = serde_json::from_slice(&serde_json::to_vec(&finish).unwrap()).unwrap();
  assert_eq!(restored.action(), Some(&binding));
  assert_eq!(restored.action().unwrap().parameters(), &parameters);
  assert!(node(FlowNodeKind::Reasoning).with_action(binding).is_err());
  assert!(FlowActionBinding::new(reference("other-plugin"), key("notify"), serde_json::json!([])).is_err());
}
