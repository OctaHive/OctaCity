use crate::flow_test_support::*;
use crate::*;
use std::collections::BTreeMap;

#[test]
fn editable_templates_resolve_schema_and_profile_choices_into_immutable_arbitrary_nodes() {
  let wire = serde_json::json!({
    "entry":"accessibility_audit",
    "input_schema":"observations",
    "schemas":{"observations":{"identity":"audit.observations","version":"v1","shape":{"type":"object","fields":{"count":{"required":true,"value":{"type":"integer","minimum":0,"maximum":100}}}}}},
    "nodes":[{"key":"accessibility_audit","kind":"reasoning","input_schema":"observations","outcomes":[{"key":"observed","kind":"success","schema":"observations"}],"budget":{"max_attempts":1,"max_elapsed_millis":1000,"max_tokens":100,"max_cost_micro_units":1000,"max_output_bytes":10000},"permissions":FactoryPermissionSet::deny_all(),"required":true,
      "input_binding":{"sources":{},"mapping":{"value":{"mapping":"constant","value":{"count":3}}}},"build_profile":"audit"}],
    "transitions":[{"predecessor":"accessibility_audit","outcome":"observed","target":{"kind":"terminal","key":"done"},"max_repeats":0,"exhausted_terminal":null}],
    "terminals":{"done":"observations"},
    "execution":FlowExecutionPolicy::new(BudgetLimit::new(16,10000,1000,10000,10000).unwrap(),FactoryPermissionSet::deny_all(),1).unwrap()
  });
  let template: FlowDefinitionTemplate = serde_json::from_value(wire).unwrap();
  let published = template
    .instantiate(
      FlowDefinitionId::generate(),
      FlowDefinitionVersion::INITIAL,
      &BTreeMap::from([(key("audit"), binding(octacity_server_domain::ProjectId::generate()))]),
    )
    .unwrap();
  assert_eq!(published.definition.nodes()[0].key().as_str(), "accessibility_audit");
  assert_eq!(
    published.definition.nodes()[0].input_schema(),
    Some(published.schemas[0].reference())
  );
  assert_eq!(
    published.definition.nodes()[0]
      .build()
      .unwrap()
      .model_or_tool
      .identity()
      .as_str(),
    "audit.parameters"
  );
  assert!(
    template
      .instantiate(
        FlowDefinitionId::generate(),
        FlowDefinitionVersion::INITIAL,
        &BTreeMap::new()
      )
      .is_err()
  );
  let decoded: FlowDefinition = serde_json::from_slice(&serde_json::to_vec(&published.definition).unwrap()).unwrap();
  assert_eq!(decoded, published.definition);
}
