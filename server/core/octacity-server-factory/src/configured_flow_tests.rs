use crate::flow_test_support::*;
use crate::*;
use std::collections::BTreeMap;

fn template() -> FlowDefinitionTemplate {
  serde_json::from_str(include_str!("../../../../config/factory/flows/base-intake.json")).unwrap()
}
fn published() -> PublishedFlowTemplate {
  let template = template();
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
  template
    .instantiate(FlowDefinitionId::generate(), FlowDefinitionVersion::INITIAL, &profiles)
    .unwrap()
}
fn select(node: &str, value: serde_json::Value) -> String {
  let published = published();
  let node = published.definition.node(&key(node)).unwrap();
  let schema = published
    .schemas
    .iter()
    .find(|schema| Some(schema.reference()) == node.input_schema())
    .unwrap();
  let input = FlowPayload::new(schema, value).unwrap();
  FlowGateProgram::from_node(node)
    .unwrap()
    .select_at(&input, octacity_server_domain::Timestamp::from_unix_millis(30).unwrap())
    .unwrap()
    .as_str()
    .to_owned()
}
#[test]
fn supplied_eligibility_configuration_stops_duplicates_uncertainty_and_out_of_scope_work() {
  let input = serde_json::json!({"fit":"fits","duplicate":false,"trusted_fit":"fits","trusted_duplicate":false,"expected_input":"a".repeat(64),"evidence_input":"a".repeat(64),"fresh_until":100});
  assert_eq!(select("eligibility_gate", input.clone()), "classify");
  for (field, value, outcome) in [
    ("trusted_duplicate", serde_json::json!(true), "rejection"),
    ("trusted_fit", serde_json::json!("out_of_scope"), "rejection"),
    ("trusted_fit", serde_json::json!("inconclusive"), "escalation"),
    ("evidence_input", serde_json::json!("b".repeat(64)), "escalation"),
    ("fresh_until", serde_json::json!(20), "escalation"),
  ] {
    let mut changed = input.clone();
    changed[field] = value;
    assert_eq!(select("eligibility_gate", changed), outcome);
  }
}
#[test]
fn supplied_classification_configuration_preserves_finite_routing_without_model_authority() {
  let input = serde_json::json!({"work_kind":"feature_request","size":"small","risk":"low","admission_risk":"low","component":"api","severity":"low","dependencies":[],"recommended_route":"development","reproducibility":"not_applicable","reproduced_verified":false,"already_fixed_verified":false,"verification_only_verified":false,"expected_input":"a".repeat(64),"evidence_input":"a".repeat(64),"fresh_until":100});
  assert_eq!(select("classification_gate", input.clone()), "requirements");
  for (changes, outcome) in [
    (
      serde_json::json!({"work_kind":"defect","reproducibility":"not_reproduced"}),
      "defect_research",
    ),
    (serde_json::json!({"recommended_route":"research"}), "feature_research"),
    (serde_json::json!({"recommended_route":"already_fixed"}), "escalation"),
    (
      serde_json::json!({"recommended_route":"already_fixed","already_fixed_verified":true}),
      "already_fixed",
    ),
    (
      serde_json::json!({"recommended_route":"verification_only","verification_only_verified":true}),
      "verification_only",
    ),
    (serde_json::json!({"recommended_route":"rejection"}), "rejection"),
    (serde_json::json!({"risk":"high"}), "escalation"),
    (serde_json::json!({"admission_risk":"high"}), "escalation"),
    (serde_json::json!({"recommended_route":"invented-route"}), "escalation"),
  ] {
    let mut changed = input.clone();
    for (field, value) in changes.as_object().unwrap() {
      changed[field] = value.clone();
    }
    assert_eq!(select("classification_gate", changed), outcome);
  }
}

#[test]
fn supplied_research_configuration_uses_verified_facts_and_keeps_unresolved_results_distinct() {
  let defect = serde_json::json!({"outcome":"reproduced","verified_outcome":"reproduced","expected_input":"a".repeat(64),"evidence_input":"a".repeat(64),"fresh_until":100,"work_kind":"defect","size":"small","risk":"low","admission_risk":"low","component":"api","severity":"low","dependencies":[],"recommended_route":"development"});
  assert_eq!(select("defect_research_gate", defect.clone()), "accepted");
  for (reported, verified, outcome) in [
    ("reproduced", "not_reproducible", "escalation"),
    ("intermittent", "intermittent", "accepted"),
    ("environment_specific", "environment_specific", "accepted"),
    ("cannot_reproduce", "not_reproducible", "escalation"),
    ("already_fixed", "already_fixed", "escalation"),
    ("needs_human_input", "needs_human_input", "needs_human_input"),
  ] {
    let mut changed = defect.clone();
    changed["outcome"] = serde_json::json!(reported);
    changed["verified_outcome"] = serde_json::json!(verified);
    assert_eq!(select("defect_research_gate", changed), outcome);
  }
  for (reported, verified) in [
    ("cannot_reproduce", "not_reproducible"),
    ("already_fixed", "already_fixed"),
  ] {
    let mut resolved = defect.clone();
    resolved["outcome"] = serde_json::json!(reported);
    resolved["verified_outcome"] = serde_json::json!(verified);
    resolved["terminal_accepted"] = serde_json::json!(true);
    resolved["terminal_outcome"] = serde_json::json!(reported);
    resolved["terminal_input"] = serde_json::json!("a".repeat(64));
    resolved["terminal_fresh_until"] = serde_json::json!(100);
    assert_eq!(select("defect_research_gate", resolved.clone()), reported);
    for (field, value) in [
      ("terminal_accepted", serde_json::json!(false)),
      ("terminal_outcome", serde_json::json!("other")),
      ("terminal_input", serde_json::json!("b".repeat(64))),
      ("terminal_fresh_until", serde_json::json!(20)),
    ] {
      let mut invalid = resolved.clone();
      invalid[field] = value;
      assert_eq!(select("defect_research_gate", invalid), "escalation");
    }
  }
  let feature = serde_json::json!({"outcome":"proposed","proposal_verified":true,"already_fixed_verified":false,"proposal_bytes":100,"expected_input":"a".repeat(64),"evidence_input":"a".repeat(64),"fresh_until":100,"work_kind":"feature_request","size":"small","risk":"low","admission_risk":"low","component":"api","severity":"low","dependencies":[],"recommended_route":"development"});
  assert_eq!(select("feature_research_gate", feature.clone()), "accepted");
  let mut resolved_feature = feature.clone();
  resolved_feature["outcome"] = serde_json::json!("already_fixed");
  resolved_feature["already_fixed_verified"] = serde_json::json!(true);
  assert_eq!(select("feature_research_gate", resolved_feature.clone()), "escalation");
  resolved_feature["terminal_accepted"] = serde_json::json!(true);
  resolved_feature["terminal_outcome"] = serde_json::json!("already_fixed");
  resolved_feature["terminal_input"] = serde_json::json!("a".repeat(64));
  resolved_feature["terminal_fresh_until"] = serde_json::json!(100);
  assert_eq!(
    select("feature_research_gate", resolved_feature.clone()),
    "already_fixed"
  );
  for (field, value) in [
    ("terminal_accepted", serde_json::json!(false)),
    ("terminal_outcome", serde_json::json!("other")),
    ("terminal_input", serde_json::json!("b".repeat(64))),
    ("terminal_fresh_until", serde_json::json!(20)),
  ] {
    let mut invalid = resolved_feature.clone();
    invalid[field] = value;
    assert_eq!(select("feature_research_gate", invalid), "escalation");
  }

  for (field, value) in [
    ("proposal_verified", serde_json::json!(false)),
    ("proposal_bytes", serde_json::json!(2049)),
    ("fresh_until", serde_json::json!(20)),
  ] {
    let mut changed = feature.clone();
    changed[field] = value;
    assert_eq!(select("feature_research_gate", changed), "escalation");
  }
}
#[test]
fn configured_requirements_bypass_preserves_every_protected_downstream_gate() {
  let input = serde_json::json!({"work_kind":"feature_request","size":"small","risk":"low","admission_risk":"low","component":"api","severity":"low","dependencies":[],"recommended_route":"development","research_required":false,"research_accepted":false});
  assert_eq!(select("requirements_needed", input.clone()), "bypass");
  for changes in [
    serde_json::json!({"size":"large"}),
    serde_json::json!({"risk":"high"}),
    serde_json::json!({"admission_risk":"high"}),
    serde_json::json!({"recommended_route":"requirements"}),
    serde_json::json!({"research_required":true,"research_accepted":false}),
  ] {
    let mut changed = input.clone();
    for (field, value) in changes.as_object().unwrap() {
      changed[field] = value.clone();
    }
    assert_eq!(select("requirements_needed", changed), "specification");
  }
  let definition = published().definition;
  let mut current = key("requirements_needed");
  for (outcome, next) in [
    ("bypass", "protected_tests"),
    ("observed", "implementation"),
    ("observed", "review"),
    ("observed", "verification"),
  ] {
    let edge = definition
      .transitions()
      .iter()
      .find(|edge| edge.predecessor() == &current && edge.outcome() == &key(outcome))
      .unwrap();
    assert_eq!(edge.target(), &FlowTransitionTarget::Node(key(next)));
    assert!(definition.node(&key(next)).unwrap().is_required());
    current = key(next);
  }
}

#[test]
fn supplied_classification_retains_bounded_component_severity_and_dependencies() {
  let published = published();
  let schema = published
    .schemas
    .iter()
    .find(|schema| schema.reference().identity() == &key("factory.base.classification_observations"))
    .unwrap();
  let observations = serde_json::json!({"work_kind":"defect","size":"small","risk":"low","recommended_route":"development","reproducibility":"reproduced","component":"api","severity":"high","dependencies":[]});
  FlowPayload::new(schema, observations.clone()).unwrap();
  let mut oversized = observations.clone();
  oversized["component"] = serde_json::json!("a".repeat(129));
  assert!(FlowPayload::new(schema, oversized).is_err());
  let gate_input = serde_json::json!({"work_kind":"defect","size":"small","risk":"low","admission_risk":"low","recommended_route":"development","reproducibility":"reproduced","reproduced_verified":true,"already_fixed_verified":false,"verification_only_verified":false,"expected_input":"a".repeat(64),"evidence_input":"a".repeat(64),"fresh_until":100,"component":"api","severity":"high","dependencies":[]});
  let node = published.definition.node(&key("classification_gate")).unwrap();
  let input_schema = published
    .schemas
    .iter()
    .find(|schema| Some(schema.reference()) == node.input_schema())
    .unwrap();
  let result_schema = published
    .schemas
    .iter()
    .find(|schema| schema.reference().identity() == &key("factory.base.summary"))
    .unwrap();
  let input = FlowPayload::new(input_schema, gate_input).unwrap();
  let program = FlowGateProgram::from_node(node).unwrap();
  assert_eq!(
    program
      .select_at(&input, octacity_server_domain::Timestamp::from_unix_millis(30).unwrap())
      .unwrap(),
    key("requirements")
  );
  let mapping: FlowDataMapping =
    serde_json::from_value(serde_json::to_value(&program).unwrap()["default_output"].clone()).unwrap();
  let output = mapping.project(&input, result_schema).unwrap();
  assert_eq!(output.value()["component"], "api");
  assert_eq!(output.value()["severity"], "high");
  assert_eq!(output.value()["dependencies"], serde_json::json!([]));
}

#[test]
fn publishing_another_gate_policy_cannot_change_the_pinned_small_work_decision() {
  let original = published();
  let original_node = original.definition.node(&key("requirements_needed")).unwrap();
  let schema = original
    .schemas
    .iter()
    .find(|schema| Some(schema.reference()) == original_node.input_schema())
    .unwrap();
  let input=FlowPayload::new(schema,serde_json::json!({"work_kind":"defect","size":"small","risk":"low","admission_risk":"low","component":"api","severity":"high","dependencies":[],"recommended_route":"development","research_required":false,"research_accepted":false})).unwrap();
  let frozen = FlowGateProgram::from_node(original_node).unwrap();
  assert_eq!(frozen.select(&input).unwrap(), key("bypass"));
  let mut replacement = template();
  let gate = replacement
    .nodes
    .iter_mut()
    .find(|node| node.key == key("requirements_needed"))
    .unwrap();
  gate.gate = Some(
    FlowGateProgram::new(vec![], key("specification"))
      .unwrap()
      .with_default_output_mapping(FlowDataMapping::new(FlowValueMapping::Pointer { path: String::new() }).unwrap())
      .unwrap(),
  );
  let profiles = replacement
    .nodes
    .iter()
    .filter_map(|node| node.build_profile.as_ref())
    .map(|choice| (choice.clone(), binding(octacity_server_domain::ProjectId::generate())))
    .collect::<BTreeMap<_, _>>();
  let replacement = replacement
    .instantiate(
      original.definition.reference().id(),
      FlowDefinitionVersion::new(2).unwrap(),
      &profiles,
    )
    .unwrap();
  let updated = FlowGateProgram::from_node(replacement.definition.node(&key("requirements_needed")).unwrap()).unwrap();
  assert_eq!(updated.select(&input).unwrap(), key("specification"));
  assert_ne!(original.definition.reference(), replacement.definition.reference());
  assert_eq!(frozen.select(&input).unwrap(), key("bypass"));
}
