use crate::flow_test_support::*;
use crate::*;

#[test]
fn modular_templates_publish_identically_without_interpreting_node_or_file_names() {
  let original: FlowDefinitionTemplate = crate::flow_test_support::supplied_base_flow_template();
  let mut root = serde_json::to_value(&original).unwrap();
  let mut parts = BTreeMap::new();
  for field in [
    "schemas",
    "nodes",
    "transitions",
    "terminals",
    "context_projections",
    "data_projections",
  ] {
    let value = root.as_object_mut().unwrap().remove(field).unwrap();
    parts.insert(
      format!("arbitrary/{field}.json"),
      serde_json::to_vec(&serde_json::json!({field:value})).unwrap(),
    );
  }
  root["parts"] = serde_json::json!(parts.keys().collect::<Vec<_>>());
  let manifest = serde_json::to_vec(&root).unwrap();
  let loaded = FlowDefinitionTemplate::from_manifest(&manifest, |path| Ok(parts[path.as_str()].clone())).unwrap();
  assert_eq!(loaded, original);
  let profiles = original
    .nodes
    .iter()
    .filter_map(|node| node.build_profile.as_ref())
    .map(|name| (name.clone(), binding(octacity_server_domain::ProjectId::generate())))
    .collect();
  let id = FlowDefinitionId::generate();
  let before = original
    .instantiate(id, FlowDefinitionVersion::INITIAL, &profiles)
    .unwrap();
  let after = loaded
    .instantiate(id, FlowDefinitionVersion::INITIAL, &profiles)
    .unwrap();
  assert_eq!(after.definition, before.definition);
  assert_eq!(after.schemas, before.schemas);
}

#[test]
fn modular_templates_reject_ambiguous_imports_and_unbounded_or_missing_parts() {
  let template = crate::flow_test_support::supplied_base_flow_template();
  let manifest = |paths: serde_json::Value| {
    serde_json::to_vec(&serde_json::json!({
      "input_schema":template.input_schema,"entry":template.entry,"execution":template.execution,"parts":paths
    }))
    .unwrap()
  };
  let part = serde_json::to_vec(
    &serde_json::json!({"schemas":template.schemas,"nodes":template.nodes,"terminals":template.terminals}),
  )
  .unwrap();
  for paths in [
    serde_json::json!(["a.json", "a.json"]),
    serde_json::json!(["a.json", "b.json"]),
    serde_json::json!(["../outside.json"]),
    serde_json::json!(["/absolute.json"]),
  ] {
    assert!(
      FlowDefinitionTemplate::from_manifest(&manifest(paths), |_| Ok(part.clone())).is_err(),
      "duplicate declarations and non-relative imports must never override configuration"
    );
  }
  assert!(
    FlowDefinitionTemplate::from_manifest(&manifest(serde_json::json!(["missing.json"])), |_| Err(
      FactoryError::InvalidConfiguration { field: "missing part" }
    ))
    .is_err()
  );
  assert!(
    FlowDefinitionTemplate::from_manifest(&manifest(serde_json::json!(["nested.json"])), |_| Ok(
      br#"{"parts":["other.json"]}"#.to_vec()
    ))
    .is_err()
  );
  assert!(
    FlowDefinitionTemplate::from_manifest(&manifest(serde_json::json!(["large.json"])), |_| Ok(
      vec![b' '; MAX_FLOW_DATA_BYTES]
    ))
    .is_err()
  );
}

#[test]
fn an_editable_template_publishes_the_exact_configured_retry_bound() {
  let mut wire: serde_json::Value =
    serde_json::to_value(crate::flow_test_support::supplied_base_flow_template()).unwrap();
  let index = wire["nodes"]
    .as_array()
    .unwrap()
    .iter()
    .position(|node| node["kind"] == "reasoning")
    .unwrap();
  wire["nodes"][index]["max_execution_retries"] = serde_json::json!(2);
  let template: FlowDefinitionTemplate = serde_json::from_value(wire).unwrap();
  let project = octacity_server_domain::ProjectId::generate();
  let profiles = template
    .nodes
    .iter()
    .filter_map(|node| node.build_profile.as_ref())
    .map(|choice| {
      let mut profile = binding(project);
      profile.result_output = key("observations");
      (choice.clone(), profile)
    })
    .collect::<BTreeMap<_, _>>();
  let published = template
    .instantiate(FlowDefinitionId::generate(), FlowDefinitionVersion::INITIAL, &profiles)
    .unwrap();
  assert_eq!(
    published
      .definition
      .node(&template.nodes[index].key)
      .unwrap()
      .max_execution_retries(),
    2
  );
}
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

#[test]
fn named_nested_schema_choices_expand_before_publication_and_reject_cycles_or_unknown_choices() {
  let original: FlowDefinitionTemplate = crate::flow_test_support::supplied_base_flow_template();
  let mut wire = serde_json::to_value(&original).unwrap();
  wire["schemas"]["deadline"] = serde_json::json!({"identity":"reused.deadline","version":"v1","shape":{"type":"integer","minimum":0,"maximum":86400000}});
  wire["schemas"]["execution_facts"]["shape"]["fields"]["fresh_until"]["value"] =
    serde_json::json!({"type":"reference","schema":"deadline"});
  let selected: FlowDefinitionTemplate = serde_json::from_value(wire.clone()).unwrap();
  let project = octacity_server_domain::ProjectId::generate();
  let profiles = original
    .nodes
    .iter()
    .filter_map(|node| node.build_profile.as_ref())
    .map(|name| (name.clone(), binding(project)))
    .collect();
  let id = FlowDefinitionId::generate();
  let before = original
    .instantiate(id, FlowDefinitionVersion::INITIAL, &profiles)
    .unwrap();
  let after = selected
    .instantiate(id, FlowDefinitionVersion::INITIAL, &profiles)
    .unwrap();
  assert_eq!(before.definition, after.definition);
  for expected in &before.schemas {
    assert!(after.schemas.contains(expected));
  }
  for alias in ["deadline", "missing"] {
    wire["schemas"]["deadline"]["shape"] = serde_json::json!({"type":"reference","schema":alias});
    let invalid: FlowDefinitionTemplate = serde_json::from_value(wire.clone()).unwrap();
    assert!(
      invalid
        .instantiate(id, FlowDefinitionVersion::INITIAL, &profiles)
        .is_err()
    );
  }
}
