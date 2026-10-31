use crate::flow_test_support::*;
use crate::*;
use std::collections::BTreeMap;

fn template() -> FlowDefinitionTemplate {
  crate::flow_test_support::supplied_base_flow_template()
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
fn configured_validation_requires_the_same_accepted_deliverable_policy_as_capture() {
  let flow = published();
  let implementation = flow.definition.node(&key("implementation")).unwrap().build().unwrap();
  let validation = flow
    .definition
    .node(&key("protected_validation"))
    .unwrap()
    .build()
    .unwrap();
  let capture = implementation.capture_deliverables.as_ref().unwrap();
  let validation = validation
    .capture_deliverables
    .as_ref()
    .expect("validation must qualify the frozen deliverable contract");
  assert_eq!(capture.policy, validation.policy);
  let node = flow.definition.node(&key("protected_validation")).unwrap();
  let schema = flow
    .schemas
    .iter()
    .find(|schema| Some(schema.reference()) == node.input_schema())
    .unwrap();
  let artifact = serde_json::json!({"artifact_id":"00000000-0000-0000-0000-000000000011","content_digest":"c".repeat(64),"encoded_size":100});
  let value = serde_json::json!({"summary":{"work_kind":"defect","size":"small","risk":"low","admission_risk":"low","component":"api","severity":"high","dependencies":[],"recommended_route":"development","research_required":false,"research_accepted":false},
    "candidate_bundle":artifact,"candidate_manifest":artifact,"candidate_revision":"candidate","candidate_digest":"b".repeat(64),"protected_inventory":artifact,"classes":["project.custom"],"accepted_contract":"a".repeat(64)});
  let payload = FlowPayload::new(schema, value).unwrap();
  let digest_schema = FlowDataSchema::new(
    key("test.digest"),
    key("v1"),
    FlowValueSchema::String {
      min_bytes: 64,
      max_bytes: 64,
    },
  )
  .unwrap();
  assert_eq!(
    validation
      .accepted_contract
      .project(&payload, &digest_schema)
      .unwrap()
      .value(),
    &serde_json::json!("a".repeat(64))
  );
}
#[test]
fn supplied_capture_schema_accepts_trusted_cumulative_work_observations() {
  let schema = template()
    .data_schemas()
    .unwrap()
    .remove(&key("spec_manifest"))
    .unwrap();
  let path = serde_json::json!({"status":"M","path":"src/api.rs","old_mode":"100644","new_mode":"100644","content_sha256":"a".repeat(64)});
  let manifest = serde_json::json!({"format_version":1,"base_revision":"previous-code","candidate_revision":"new-code","node_attempt_id":"attempt","capture_tool":{"identity":"git","version":"v1","sha256":"b".repeat(64)},
    "changed_paths":[path],"cumulative_changes":{"base_revision":"original-work","changed_paths":[path]},"bundle":{"name":"change-set.bundle","size_bytes":100,"sha256":"c".repeat(64)}});
  FlowPayload::new(&schema, manifest).expect("the configured primary capture result must admit the trusted wire index");
}
#[test]
fn configured_validation_classification_requires_exact_proof_and_only_declared_routes() {
  let input = serde_json::json!({
    "request":{"summary":{"work_kind":"defect","size":"small","risk":"low","admission_risk":"low","component":"api","severity":"high","dependencies":[],"recommended_route":"requirements","research_required":false,"research_accepted":false},
      "candidate_revision":"candidate-42","candidate_digest":"a".repeat(64),"validation_digest":"b".repeat(64),"classes":[{"class":"project.custom","outcome":"failed"}]},
    "classification":{"candidate_revision":"candidate-42","candidate_digest":"a".repeat(64),"validation_digest":"b".repeat(64),"input_digest":"c".repeat(64),"classification":"implementation_defect","remediation":"Preserve the public contract"},
    "expected_input":"c".repeat(64),"expected_report":"d".repeat(64),
    "facts":{"accepted":true,"input_digest":"c".repeat(64),"report_digest":"d".repeat(64),"fresh_until":100},
    "output_deadlines":{"report":100,"facts":100}
  });
  for (classification, route) in [
    ("implementation_defect", "implementation_rework"),
    ("requirements_defect", "requirements_correction"),
    ("test_defect", "test_correction"),
    ("indeterminate", "escalation"),
    ("reject", "rejection"),
  ] {
    let mut value = input.clone();
    value["classification"]["classification"] = serde_json::json!(classification);
    assert_eq!(select("validation_classification_gate", value), route);
  }
  for (pointer, value) in [
    ("/classification/candidate_revision", serde_json::json!("another")),
    ("/classification/candidate_digest", serde_json::json!("e".repeat(64))),
    ("/classification/validation_digest", serde_json::json!("e".repeat(64))),
    ("/classification/input_digest", serde_json::json!("e".repeat(64))),
    ("/facts/report_digest", serde_json::json!("e".repeat(64))),
    ("/facts/accepted", serde_json::json!(false)),
    ("/facts/fresh_until", serde_json::json!(30)),
    ("/output_deadlines/report", serde_json::json!(30)),
  ] {
    let mut value_to_check = input.clone();
    *value_to_check.pointer_mut(pointer).unwrap() = value;
    assert_eq!(
      select("validation_classification_gate", value_to_check),
      "invalid",
      "{pointer}"
    );
  }
  let mut invented = input;
  invented["classification"]["classification"] = serde_json::json!("publish_without_review");
  let flow = published();
  let node = flow.definition.node(&key("validation_classification_gate")).unwrap();
  let schema = flow
    .schemas
    .iter()
    .find(|s| Some(s.reference()) == node.input_schema())
    .unwrap();
  assert!(FlowPayload::new(schema, invented).is_err());
  let classifier = flow.definition.node(&key("validation_classification")).unwrap();
  assert!(
    classifier.context_binding().is_some(),
    "the classifier requires an explicit closed projection"
  );
}

#[test]
fn configured_implementation_handoff_selects_exact_parent_without_protected_grants() {
  let flow = published();
  let entry = flow.definition.node(&key("implementation_entry")).unwrap();
  let schema = flow
    .schemas
    .iter()
    .find(|s| Some(s.reference()) == entry.input_schema())
    .unwrap();
  let summary = serde_json::json!({"work_kind":"defect","size":"small","risk":"low","admission_risk":"low","component":"api","severity":"high","dependencies":[],"recommended_route":"development","research_required":false,"research_accepted":false});
  let task = serde_json::json!({"artifact_id":"00000000-0000-0000-0000-000000000011","content_digest":"c".repeat(64),"encoded_size":100});
  let input = FlowPayload::new(
    schema,
    serde_json::json!({"accepted":summary,"task":task,"accepted_contract":"a".repeat(64)}),
  )
  .unwrap();
  let target = flow.definition.node(&key("implementation")).unwrap();
  let target_schema = flow
    .schemas
    .iter()
    .find(|s| Some(s.reference()) == target.input_schema())
    .unwrap();
  let program = FlowGateProgram::from_node(entry).unwrap();
  let mapping: FlowDataMapping =
    serde_json::from_value(serde_json::to_value(program).unwrap()["default_output"].clone()).unwrap();
  let first = mapping.project(&input, target_schema).unwrap();
  assert_eq!(first.value()["accepted_contract"], "a".repeat(64));
  let binding = target.context_binding().unwrap();
  assert!(binding.candidate_revision(&first).unwrap().is_none());
  assert_eq!(binding.selected_sources(&first).unwrap().len(), 1);
  let mut rework = first.value().clone();
  rework["parent"] = serde_json::json!({"candidate_revision":"previous-candidate","candidate_digest":"d".repeat(64),"manifest":task,"bundle":task});
  rework["remediation"] = serde_json::json!("Keep the public API contract");
  let rework = FlowPayload::new(target_schema, rework).unwrap();
  assert_eq!(
    binding.candidate_revision(&rework).unwrap().unwrap().as_str(),
    "previous-candidate"
  );
  let selected = binding.selected_sources(&rework).unwrap();
  assert_eq!(selected.len(), 3);
  assert!(
    selected
      .values()
      .all(|(kind, _)| matches!(kind, ContextSourceKind::Task | ContextSourceKind::Evidence))
  );
  let mut leaked = rework.value().clone();
  leaked["protected_inventory"] = task;
  assert!(FlowPayload::new(target_schema, leaked).is_err());
}

#[test]
fn configured_code_review_requires_independent_exact_fresh_assessment() {
  let summary = serde_json::json!({"work_kind":"defect","size":"small","risk":"low","admission_risk":"low","component":"api","severity":"high","dependencies":[],"recommended_route":"development","research_required":false,"research_accepted":false});
  let artifact = serde_json::json!({"artifact_id":"00000000-0000-0000-0000-000000000011","content_digest":"c".repeat(64),"encoded_size":100});
  let input = serde_json::json!({"request":{"summary":summary,"candidate_revision":"code-candidate","candidate_digest":"a".repeat(64),"task":artifact,"manifest":artifact,"bundle":artifact},
    "review":{"candidate_revision":"code-candidate","candidate_digest":"a".repeat(64),"input_digest":"b".repeat(64),"quality":{"correctness":"satisfied","maintainability":"satisfied","public_tests":"satisfied","security":"satisfied"},"findings":[]},
    "expected_input":"b".repeat(64),"expected_report":"d".repeat(64),"facts":{"accepted":true,"input_digest":"b".repeat(64),"report_digest":"d".repeat(64),"fresh_until":100},"output_deadlines":{"review":100,"facts":100}});
  assert_eq!(select("code_review_gate", input.clone()), "accepted");
  for (path, value, outcome) in [
    ("/review/candidate_revision", serde_json::json!("another"), "invalid"),
    ("/review/candidate_digest", serde_json::json!("e".repeat(64)), "invalid"),
    ("/review/input_digest", serde_json::json!("e".repeat(64)), "invalid"),
    ("/facts/accepted", serde_json::json!(false), "invalid"),
    ("/facts/report_digest", serde_json::json!("e".repeat(64)), "invalid"),
    ("/output_deadlines/facts", serde_json::json!(30), "invalid"),
    (
      "/review/quality/correctness",
      serde_json::json!("violated"),
      "changes_required",
    ),
    (
      "/review/quality/public_tests",
      serde_json::json!("indeterminate"),
      "needs_human_input",
    ),
  ] {
    let mut changed = input.clone();
    *changed.pointer_mut(path).unwrap() = value;
    assert_eq!(select("code_review_gate", changed), outcome, "{path}");
  }
  let flow = published();
  let reviewer = flow.definition.node(&key("review")).unwrap();
  let schema = flow
    .schemas
    .iter()
    .find(|s| Some(s.reference()) == reviewer.input_schema())
    .unwrap();
  let request = FlowPayload::new(schema, input["request"].clone()).unwrap();
  let sources = reviewer.context_binding().unwrap().selected_sources(&request).unwrap();
  assert_eq!(
    sources.keys().map(FactoryKey::as_str).collect::<Vec<_>>(),
    vec!["bundle", "manifest", "task"]
  );
  for forbidden in ["implementation_transcript", "protected_inventory", "hidden_tests"] {
    let mut leaked = request.value().clone();
    leaked[forbidden] = artifact.clone();
    assert!(FlowPayload::new(schema, leaked).is_err());
  }
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
    ("observed", "implementation_entry"),
    ("ready", "implementation"),
    ("observed", "implementation_result_gate"),
    ("continue", "protected_validation"),
    ("observed", "protected_validation_gate"),
    ("passed", "review"),
    ("observed", "code_review_gate"),
    ("accepted", "verification"),
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
fn supplied_author_handoff_and_implementation_projection_exclude_private_outputs() {
  let template = template();
  let schemas = template.data_schemas().unwrap();
  let author = template
    .nodes
    .iter()
    .find(|node| node.key == key("protected_tests"))
    .unwrap();
  let handoff = &schemas[&author.outcomes[0].schema];
  assert!(
    FlowPayload::new(
      handoff,
      serde_json::json!({"accepted_contract":"a".repeat(64),"classes":["project.custom"]})
    )
    .is_ok()
  );
  assert!(
    FlowPayload::new(
      handoff,
      serde_json::json!({"accepted_contract":"a".repeat(64),"classes":["project.custom"],"assertions":"PRIVATE"})
    )
    .is_err()
  );
  let implementation = template
    .nodes
    .iter()
    .find(|node| node.key == key("implementation"))
    .unwrap();
  let entry = template
    .nodes
    .iter()
    .find(|node| node.key == key("implementation_entry"))
    .unwrap();
  let binding = serde_json::to_value(&entry.input_binding).unwrap();
  assert_eq!(binding["sources"]["original"]["view"], "input_payload");
  let validator = template
    .nodes
    .iter()
    .find(|node| node.key == key("protected_validation"))
    .unwrap();
  assert_eq!(validator.kind, FlowNodeKind::BuildCommand);
  assert!(validator.context_binding.is_some());
  let artifact = serde_json::json!({"artifact_id":"00000000-0000-0000-0000-000000000011","content_digest":"c".repeat(64),"encoded_size":100});
  let summary = serde_json::json!({"work_kind":"feature_request","size":"small","risk":"low","admission_risk":"low","component":"api","severity":"low","dependencies":[],"recommended_route":"development","research_required":false,"research_accepted":false});
  let input=FlowPayload::new(&schemas[&validator.input_schema],serde_json::json!({"summary":summary,"candidate_bundle":artifact,"candidate_manifest":artifact,"candidate_revision":"candidate","candidate_digest":"d".repeat(64),"protected_inventory":artifact,"classes":["project.custom"],"accepted_contract":"a".repeat(64)})).unwrap();
  assert_eq!(
    validator
      .context_binding
      .as_ref()
      .unwrap()
      .selected_sources(&input)
      .unwrap()
      .len(),
    3
  );
  assert!(
    implementation
      .context_binding
      .as_ref()
      .is_none_or(|binding| !serde_json::to_string(binding).unwrap().contains("protected_inventory"))
  );
}

#[test]
fn supplied_validation_gate_requires_exact_fresh_inputs_and_every_declared_class() {
  let summary = serde_json::json!({"work_kind":"feature_request","size":"small","risk":"low","admission_risk":"low","component":"api","severity":"low","dependencies":[],"recommended_route":"development","research_required":false,"research_accepted":false});
  let artifact = serde_json::json!({"artifact_id":"00000000-0000-0000-0000-000000000011","content_digest":"c".repeat(64),"encoded_size":100});
  let report = serde_json::json!({"changeset_id":"00000000-0000-0000-0000-000000000012","candidate":{"exact":{"project_id":"00000000-0000-0000-0000-000000000001","repository_id":"00000000-0000-0000-0000-000000000002","base_revision":"base"},"candidate_revision":"candidate","changeset_digest":"d".repeat(64)},"tests":artifact,"input_digest":"e".repeat(64),"classes":[{"class":"project.custom","outcome":"passed"}]});
  let input = serde_json::json!({"summary":summary,"report":report,"expected_input":"a".repeat(64),"expected_report":"b".repeat(64),"candidate_revision":"candidate","candidate_digest":"d".repeat(64),"protected_inventory":artifact,"classes":["project.custom"],"facts":{"accepted":true,"input_digest":"a".repeat(64),"report_digest":"b".repeat(64),"candidate_revision":"candidate","candidate_digest":"d".repeat(64),"protected_inventory":artifact,"covered_classes":["project.custom"],"fresh_until":100}});
  assert_eq!(select("protected_validation_gate", input.clone()), "passed");
  for (path, value) in [
    ("/facts/input_digest", serde_json::json!("z".repeat(64))),
    ("/report/tests/content_digest", serde_json::json!("z".repeat(64))),
    ("/facts/covered_classes", serde_json::json!(["other"])),
    ("/facts/fresh_until", serde_json::json!(20)),
  ] {
    let mut changed = input.clone();
    *changed.pointer_mut(path).unwrap() = value;
    assert_eq!(select("protected_validation_gate", changed), "invalid");
  }
  let mut failed = input;
  failed["report"]["classes"][0]["outcome"] = serde_json::json!("failed");
  assert_eq!(select("protected_validation_gate", failed), "failed");
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

fn specification_feature_input() -> serde_json::Value {
  serde_json::json!({
    "expected_predecessor":"research-candidate", "declared_requirements":[], "allowed_paths":["openspec/changes", "openspec/specs"],
    "artifacts":{"manifest":{"artifact_id":"00000000-0000-0000-0000-000000000001","content_digest":"c".repeat(64),"encoded_size":100},"bundle":{"artifact_id":"00000000-0000-0000-0000-000000000002","content_digest":"d".repeat(64),"encoded_size":100}}, "output_deadlines":{"capture":100,"handoff":100,"facts":100}, "work_id":"work-42", "original_base":"original", "work_kind":"feature_request", "expected_input":"a".repeat(64), "handoff_digest":"b".repeat(64),
    "manifest":{"format_version":1,"base_revision":"research-candidate","candidate_revision":"spec-candidate","node_attempt_id":"attempt-42","capture_tool":{"identity":"capture","version":"v1","sha256":"c".repeat(64)},"changed_paths":[{"status":"A","path":"openspec/specs/api/spec.md","old_mode":"000000","new_mode":"100644"},{"status":"A","path":"openspec/changes/new-api/proposal.md","old_mode":"000000","new_mode":"100644"}],"bundle":{"name":"change-set.bundle","size_bytes":100,"sha256":"d".repeat(64)}},
    "handoff":{"work_id":"work-42","original_base":"original","predecessor_revision":"research-candidate","candidate_revision":"spec-candidate","input_digest":"a".repeat(64),"work_kind":"feature_request","contract_path":"openspec/specs/api/spec.md","change_path":"openspec/changes/new-api/proposal.md","acceptance_criteria":["new API has documented behavior"],"regression_criteria":[],"declared_requirements":[]},
    "facts":{"accepted":true,"requirements_preserved":true,"contract_exists":true,"input_digest":"a".repeat(64),"handoff_digest":"b".repeat(64),"fresh_until":100}
  })
}

#[test]
fn configured_specification_authoring_requires_exact_capture_and_typed_handoff_before_review() {
  let feature = specification_feature_input();
  assert_eq!(select("specification_gate", feature.clone()), "review_pending");
  for (pointer, value) in [
    ("/handoff/work_id", serde_json::json!("other")),
    ("/handoff/original_base", serde_json::json!("other")),
    ("/handoff/predecessor_revision", serde_json::json!("other")),
    ("/handoff/candidate_revision", serde_json::json!("other")),
    ("/handoff/input_digest", serde_json::json!("e".repeat(64))),
    ("/facts/handoff_digest", serde_json::json!("e".repeat(64))),
    ("/facts/accepted", serde_json::json!(false)),
    ("/facts/fresh_until", serde_json::json!(20)),
    ("/facts/fresh_until", serde_json::json!(30)),
    ("/output_deadlines/capture", serde_json::json!(30)),
    ("/output_deadlines/handoff", serde_json::json!(20)),
    ("/output_deadlines/facts", serde_json::json!(30)),
    ("/manifest/changed_paths/0/path", serde_json::json!("src/lib.rs")),
    ("/manifest/changed_paths/1/status", serde_json::json!("M")),
  ] {
    let mut invalid = feature.clone();
    *invalid.pointer_mut(pointer).unwrap() = value;
    assert_eq!(select("specification_gate", invalid), "invalid", "{pointer}");
  }
  let mut defect = feature.clone();
  defect["work_kind"] = serde_json::json!("defect");
  defect["handoff"]["work_kind"] = serde_json::json!("defect");
  defect["handoff"]["regression_criteria"] = serde_json::json!(["prior behavior reproduces the bug"]);
  defect["manifest"]["changed_paths"][0]["status"] = serde_json::json!("M");
  assert_eq!(select("specification_gate", defect.clone()), "review_pending");
  defect["handoff"]["regression_criteria"] = serde_json::json!([]);
  assert_eq!(select("specification_gate", defect), "invalid");
  let definition = published().definition;
  assert_eq!(
    definition.node(&key("specification_author")).unwrap().kind(),
    FlowNodeKind::Reasoning
  );
  assert_eq!(
    serde_json::to_value(definition.node(&key("specification_author")).unwrap().build().unwrap()).unwrap()["capture_write_paths"],
    serde_json::json!(["openspec/changes", "openspec/specs"])
  );
  assert!(
    definition
      .node(&key("specification_gate"))
      .unwrap()
      .accepted_work_outcomes()
      .is_empty()
  );
  assert_eq!(
    definition
      .transitions()
      .iter()
      .find(|edge| edge.predecessor() == &key("requirements_needed") && edge.outcome() == &key("specification"))
      .unwrap()
      .target(),
    &FlowTransitionTarget::Node(key("author_subject"))
  );
}

#[test]
fn changing_one_published_scope_updates_author_input_and_capture_gate() {
  let work = work();
  let mut configured = template();
  let roots = vec![
    FactoryRepositoryPath::new("contracts").unwrap(),
    FactoryRepositoryPath::new("plans/changes").unwrap(),
  ];
  configured
    .nodes
    .iter_mut()
    .find(|node| node.key == key("specification_author"))
    .unwrap()
    .capture_write_paths = Some(roots);
  // This configured author accepts incoming data instead of a previous node.
  let mut author_only = configured.clone();
  author_only.nodes.retain(|node| node.key == key("specification_author"));
  author_only.entry = key("specification_author");
  author_only.input_schema = key("spec_author_input");
  author_only.transitions = vec![FlowTransition::new(
    key("specification_author"),
    key("observed"),
    FlowTransitionTarget::Terminal(key("done")),
  )];
  author_only.terminals = BTreeMap::from([(key("done"), key("spec_manifest"))]);
  let incoming_schema = author_only
    .data_schemas()
    .unwrap()
    .remove(&key("spec_author_input"))
    .unwrap();
  let mut binding_wire = serde_json::to_value(&author_only.nodes[0].input_binding).unwrap();
  binding_wire["sources"]["previous"] =
    serde_json::json!({"source":"incoming","schema":incoming_schema.reference(),"path":"/accepted_input"});
  author_only.nodes[0].input_binding = serde_json::from_value(binding_wire).unwrap();
  let published = author_only
    .instantiate(
      FlowDefinitionId::generate(),
      FlowDefinitionVersion::INITIAL,
      &BTreeMap::from([(key("specification_author"), binding(work.subject().project_id()))]),
    )
    .unwrap();
  let definition = &published.definition;
  let flow = FlowRun::root(
    &FactoryRun::admitted(FactoryRunId::generate(), &work),
    definition.reference(),
  )
  .unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition.clone()]).unwrap(),
    FlowAdmissionLimits::product_defaults(4, definition.execution().budget(), FactoryPermissionSet::deny_all())
      .unwrap(),
    flow,
    cycle,
    FactoryWipLimits::new(8, 16).unwrap(),
  )
  .unwrap()
  .with_data_schemas(published.schemas.clone())
  .unwrap();
  let incoming = FlowIncomingData::new(&work, &admitted, FlowPayload::new(&incoming_schema,
    serde_json::json!({"work_id":work.id(),"original_base":work.subject().repository().unwrap().base_revision(),"allowed_paths":["src"],"declared_requirements":[],"task":work.artifacts().task(),"original_acceptance":work.artifacts().acceptance(),"accepted_input":{"work_kind":"feature_request","size":"medium","risk":"low","admission_risk":"low","recommended_route":"development","research_required":false,"research_accepted":false,"component":"api","severity":"low","dependencies":[]}})).unwrap()).unwrap();
  let schema = published
    .schemas
    .iter()
    .find(|schema| Some(schema.reference()) == definition.nodes()[0].input_schema())
    .unwrap();
  let input = FlowNodeInput::prepare(FlowInputPreparation {
    work: &work,
    admitted: &admitted,
    flow: admitted.root_run(),
    cycle: admitted.initial_cycle(),
    node: key("specification_author"),
    schema,
    history: FlowRuntimeHistory {
      flow_runs: std::slice::from_ref(admitted.root_run()),
      cycles: std::slice::from_ref(admitted.initial_cycle()),
      attempts: &[],
      completions: &[],
      stops: &[],
    },
    records: &[],
    inputs: &[],
    incoming: Some(&incoming),
  })
  .unwrap();
  assert_eq!(
    input.payload().value()["allowed_paths"],
    serde_json::json!(["contracts", "plans/changes"])
  );
  let schema = configured
    .data_schemas()
    .unwrap()
    .remove(&key("spec_gate_input"))
    .unwrap();
  let gate = configured
    .nodes
    .iter()
    .find(|node| node.key == key("specification_gate"))
    .unwrap()
    .gate
    .as_ref()
    .unwrap();
  let mut feature = specification_feature_input();
  feature["allowed_paths"] = input.payload().value()["allowed_paths"].clone();
  for (index, path) in [(0, "contracts/api/spec.md"), (1, "plans/changes/new-api/proposal.md")] {
    feature["manifest"]["changed_paths"][index]["path"] = serde_json::json!(path);
  }
  feature["handoff"]["contract_path"] = serde_json::json!("contracts/api/spec.md");
  feature["handoff"]["change_path"] = serde_json::json!("plans/changes/new-api/proposal.md");
  let at = octacity_server_domain::Timestamp::from_unix_millis(30).unwrap();
  assert_eq!(
    gate
      .select_at(&FlowPayload::new(&schema, feature.clone()).unwrap(), at)
      .unwrap(),
    key("review_pending")
  );
  feature["manifest"]["changed_paths"][0]["path"] = serde_json::json!("openspec/specs/api/spec.md");
  feature["handoff"]["contract_path"] = serde_json::json!("openspec/specs/api/spec.md");
  assert_eq!(
    gate
      .select_at(&FlowPayload::new(&schema, feature).unwrap(), at)
      .unwrap(),
    key("invalid")
  );
}

#[test]
fn configured_specification_review_checks_quality_and_exact_candidate_without_granting_readiness() {
  let input = serde_json::json!({
    "request":{"work_id":"work-42","original_base":"original","candidate_revision":"spec-candidate","manifest_digest":"a".repeat(64),"bundle_digest":"b".repeat(64)},
    "expected_input":"c".repeat(64),"review_digest":"d".repeat(64),"output_deadlines":{"review":100,"facts":100},
    "review":{"work_id":"work-42","original_base":"original","candidate_revision":"spec-candidate","manifest_digest":"a".repeat(64),"bundle_digest":"b".repeat(64),"input_digest":"c".repeat(64),
      "quality":{"complete":"satisfied","consistent":"satisfied","feasible":"satisfied","testable":"satisfied","in_scope":"satisfied"},"findings":[]},
    "facts":{"accepted":true,"input_digest":"c".repeat(64),"report_digest":"d".repeat(64),"fresh_until":100}
  });
  assert_eq!(
    select("specification_review_gate", input.clone()),
    "publication_pending"
  );
  for (pointer, value, outcome) in [
    ("/review/candidate_revision", serde_json::json!("other"), "invalid"),
    ("/review/manifest_digest", serde_json::json!("e".repeat(64)), "invalid"),
    ("/review/bundle_digest", serde_json::json!("e".repeat(64)), "invalid"),
    ("/review/work_id", serde_json::json!("other"), "invalid"),
    ("/review/original_base", serde_json::json!("other"), "invalid"),
    ("/review/input_digest", serde_json::json!("e".repeat(64)), "invalid"),
    ("/facts/accepted", serde_json::json!(false), "invalid"),
    ("/facts/report_digest", serde_json::json!("e".repeat(64)), "invalid"),
    ("/facts/input_digest", serde_json::json!("e".repeat(64)), "invalid"),
    ("/facts/fresh_until", serde_json::json!(30), "invalid"),
    ("/output_deadlines/review", serde_json::json!(20), "invalid"),
    ("/output_deadlines/facts", serde_json::json!(30), "invalid"),
    (
      "/review/quality/consistent",
      serde_json::json!("violated"),
      "changes_required",
    ),
    (
      "/review/quality/testable",
      serde_json::json!("indeterminate"),
      "needs_human_input",
    ),
  ] {
    let mut changed = input.clone();
    *changed.pointer_mut(pointer).unwrap() = value;
    assert_eq!(select("specification_review_gate", changed), outcome, "{pointer}");
  }
  let published = published();
  let definition = &published.definition;
  let reviewer = definition.node(&key("specification_review")).unwrap();
  assert_eq!(reviewer.kind(), FlowNodeKind::Reasoning);
  assert!(reviewer.context_binding().is_some());
  assert!(
    definition
      .node(&key("specification_review_gate"))
      .unwrap()
      .accepted_work_outcomes()
      .is_empty()
  );
  assert_eq!(
    definition
      .transitions()
      .iter()
      .find(|edge| edge.predecessor() == &key("specification_gate") && edge.outcome() == &key("review_pending"))
      .unwrap()
      .target(),
    &FlowTransitionTarget::Node(key("specification_review"))
  );
  let schema = published
    .schemas
    .iter()
    .find(|s| Some(s.reference()) == reviewer.input_schema())
    .unwrap();
  assert!(FlowPayload::new(schema, serde_json::json!({"prose":"ready for implementation"})).is_err());
}

#[test]
fn a_configured_human_gate_can_follow_specification_review_without_engine_changes() {
  let mut wire = serde_json::to_value(template()).unwrap();
  let gate = wire["nodes"]
    .as_array()
    .unwrap()
    .iter()
    .find(|n| n["key"] == "specification_review_gate")
    .unwrap()
    .clone();
  wire["nodes"].as_array_mut().unwrap().push(serde_json::json!({
    "key":"contract_owner_confirmation","kind":"human_gate","input_schema":"spec_review",
    "outcomes":[{"key":"confirmed","kind":"success","schema":"spec_review"},{"key":"declined","kind":"failure","schema":"spec_review"}],
    "budget":gate["budget"],"required":true,
    "input_binding":{"sources":{"review":{"source":"predecessor","view":"payload","path":""}},"mapping":{"value":{"mapping":"pointer","path":"/review"}}}
  }));
  let transitions = wire["transitions"].as_array_mut().unwrap();
  let edge = transitions
    .iter_mut()
    .find(|edge| edge["predecessor"] == "specification_review_gate" && edge["outcome"] == "publication_pending")
    .unwrap();
  edge["target"] = serde_json::json!({"kind":"node","key":"contract_owner_confirmation"});
  for (outcome, kind, target) in [
    ("confirmed", "node", "specification_publication"),
    ("declined", "terminal", "specification_review_declined"),
  ] {
    transitions.push(serde_json::json!({"predecessor":"contract_owner_confirmation","outcome":outcome,"target":{"kind":kind,"key":target},"max_repeats":0,"exhausted_terminal":null}));
  }
  wire["terminals"]["specification_review_declined"] = serde_json::json!("spec_review");
  let template: FlowDefinitionTemplate = serde_json::from_value(wire).unwrap();
  let project = octacity_server_domain::ProjectId::generate();
  let profiles = template
    .nodes
    .iter()
    .filter_map(|node| node.build_profile.as_ref())
    .map(|name| (name.clone(), binding(project)))
    .collect();
  let published = template
    .instantiate(FlowDefinitionId::generate(), FlowDefinitionVersion::INITIAL, &profiles)
    .unwrap();
  PinnedFlowDefinitionClosure::new(published.definition.reference(), vec![published.definition.clone()])
    .unwrap()
    .validate(
      FlowAdmissionLimits::new(
        4,
        MAX_FLOW_EXPANDED_NODES,
        3,
        8,
        8,
        published.definition.execution().budget(),
        published.definition.execution().permissions().clone(),
      )
      .unwrap(),
    )
    .unwrap();
  let confirmation = published.definition.node(&key("contract_owner_confirmation")).unwrap();
  assert_eq!(confirmation.kind(), FlowNodeKind::HumanGate);
  assert!(confirmation.accepted_work_outcomes().is_empty());
  assert!(
    published
      .definition
      .node(&key("specification_review_gate"))
      .unwrap()
      .accepted_work_outcomes()
      .is_empty()
  );
}

#[test]
fn supplied_publication_requires_exact_receipt_before_implementation_readiness() {
  let definition = published().definition;
  let publication = definition
    .node(&key("specification_publication"))
    .expect("trusted publication precedes readiness");
  assert_eq!(publication.kind(), FlowNodeKind::TrustedAction);
  assert!(publication.accepted_work_outcomes().is_empty());
  let edge = definition
    .transitions()
    .iter()
    .find(|edge| {
      edge.predecessor() == &key("specification_review_gate") && edge.outcome() == &key("publication_pending")
    })
    .unwrap();
  assert_eq!(edge.target(), &FlowTransitionTarget::Node(publication.key().clone()));
  let ready = definition.node(&key("specification_ready")).unwrap();
  assert_eq!(ready.accepted_work_outcomes(), &[key("implementation_ready")]);
  let summary = serde_json::json!({"work_kind":"feature_request","size":"large","risk":"low","admission_risk":"low","component":"api","severity":"low","dependencies":[],"recommended_route":"requirements","research_required":false,"research_accepted":false});
  let input = serde_json::json!({"summary":summary,"publication":{"receipt":"a".repeat(64),"candidate_revision":"reviewed-candidate","changeset_digest":"b".repeat(64),
    "external":{"branch_reference":"refs/heads/reviewed-candidate","remote_head":"reviewed-candidate","review_identity":"23","review_state":"open"}},
    "expected_candidate":"reviewed-candidate","expected_digest":"b".repeat(64),"review_deadline":100});
  assert_eq!(select("specification_ready", input.clone()), "implementation_ready");
  for (field, value) in [
    ("expected_candidate", serde_json::json!("other")),
    ("expected_digest", serde_json::json!("c".repeat(64))),
    ("review_deadline", serde_json::json!(20)),
  ] {
    let mut invalid = input.clone();
    invalid[field] = value;
    assert_eq!(select("specification_ready", invalid), "invalid");
  }
  let edge = definition
    .transitions()
    .iter()
    .find(|edge| edge.predecessor() == ready.key() && edge.outcome() == &key("implementation_ready"))
    .unwrap();
  assert_eq!(edge.target(), &FlowTransitionTarget::Node(key("protected_tests")));
}

#[test]
fn configured_specification_handoff_preserves_every_declared_requirement_reference() {
  let mut input = specification_feature_input();
  let declared = (1..=6).map(|number| serde_json::json!({"artifact_id":format!("00000000-0000-0000-0000-{number:012}"),"content_digest":format!("{number:064x}"),"encoded_size":100})).collect::<Vec<_>>();
  input["declared_requirements"] = serde_json::json!(declared);
  input["handoff"]["declared_requirements"] = input["declared_requirements"].clone();
  input["facts"]["requirements_preserved"] = serde_json::json!(true);
  assert_eq!(select("specification_gate", input.clone()), "review_pending");
  for number in 0..6 {
    let mut missing = input.clone();
    missing["handoff"]["declared_requirements"]
      .as_array_mut()
      .unwrap()
      .remove(number);
    assert_eq!(select("specification_gate", missing), "invalid");
  }
  input["facts"]["requirements_preserved"] = serde_json::json!(false);
  assert_eq!(select("specification_gate", input), "invalid");
}

#[test]
fn supplied_execution_results_require_qualified_defects_and_declare_bounded_cycle_returns() {
  let summary = serde_json::json!({"work_kind":"defect","size":"small","risk":"low","admission_risk":"low","component":"api","severity":"high","dependencies":[],"recommended_route":"development","research_required":false,"research_accepted":false,"requirements_defect":{"reason":"Missing retry contract","candidate_revision":"current-candidate","evidence":[{"artifact_id":"00000000-0000-0000-0000-000000000012","content_digest":"c".repeat(64),"encoded_size":100}]}});
  let input = serde_json::json!({"trusted_input":summary,"execution_revision":"current-candidate","result":summary,"expected_input":"a".repeat(64),"expected_report":"b".repeat(64),"facts":{"input_digest":"a".repeat(64),"report_digest":"b".repeat(64),"fresh_until":100,"outcome":"requirements_defect","candidate_revision":"current-candidate","requirements_evidence":summary["requirements_defect"]["evidence"]}});
  for node in ["implementation_result_gate", "verification_result_gate"] {
    assert_eq!(select(node, input.clone()), "requirements_defect");
    for (path, value) in [
      ("/facts/input_digest", serde_json::json!("c".repeat(64))),
      ("/facts/report_digest", serde_json::json!("c".repeat(64))),
      ("/facts/fresh_until", serde_json::json!(20)),
      ("/facts/candidate_revision", serde_json::json!("old-candidate")),
    ] {
      let mut invalid = input.clone();
      *invalid.pointer_mut(path).unwrap() = value;
      assert_eq!(select(node, invalid), "invalid");
    }
    let mut prose = input.clone();
    prose["facts"]["outcome"] = serde_json::json!("observed");
    assert_eq!(select(node, prose), "continue");
    let definition = published().definition;
    let edge = definition
      .transitions()
      .iter()
      .find(|edge| edge.predecessor() == &key(node) && edge.outcome() == &key("requirements_defect"))
      .unwrap();
    assert!(edge.starts_cycle());
    assert!(edge.max_repeats() > 0);
    assert_eq!(edge.target(), &FlowTransitionTarget::Node(key("requirements_needed")));
  }
  assert_eq!(select("requirements_needed", input["result"].clone()), "specification");
}

#[test]
fn configured_correction_author_selects_only_the_preceding_accepted_candidate() {
  let authored = specification_feature_input();
  let accepted = serde_json::json!({"work_id":authored["work_id"],"original_base":authored["original_base"],"manifest":authored["manifest"],"handoff":authored["handoff"],"readiness":"review_pending","artifacts":authored["artifacts"]});
  let mut summary = serde_json::json!({"work_kind":"feature_request","size":"large","risk":"low","admission_risk":"low","component":"api","severity":"low","dependencies":[],"recommended_route":"requirements","research_required":false,"research_accepted":false});
  assert_eq!(select("author_subject", summary.clone()), "original");
  summary["specification"] = accepted;
  assert_eq!(select("author_subject", summary.clone()), "correction");
  let published = published();
  let node = published.definition.node(&key("specification_correction")).unwrap();
  let schema = published
    .schemas
    .iter()
    .find(|s| Some(s.reference()) == node.input_schema())
    .unwrap();
  let payload=FlowPayload::new(schema,serde_json::json!({"work_id":"work-42","original_base":"original","accepted_input":summary,"declared_requirements":[],"task":{"artifact_id":"00000000-0000-0000-0000-000000000003","content_digest":"d".repeat(64),"encoded_size":100},"original_acceptance":{"artifact_id":"00000000-0000-0000-0000-000000000004","content_digest":"e".repeat(64),"encoded_size":100},"allowed_paths":["openspec/specs","openspec/changes"]})).unwrap();
  let context = node.context_binding().unwrap();
  assert_eq!(
    context.candidate_revision(&payload).unwrap().unwrap().as_str(),
    "spec-candidate"
  );
  let selected = context.selected_sources(&payload).unwrap();
  assert_eq!(selected.len(), 4);
  let FactoryContextReference::Artifact(manifest) = &selected[&key("manifest")].1 else {
    panic!("exact artifact required")
  };
  assert_eq!(manifest.content_digest().to_string(), "c".repeat(64));
}

#[test]
fn corrected_specification_must_amend_the_exact_execution_predecessor() {
  let mut input = specification_feature_input();
  input["expected_predecessor"] = serde_json::json!("accepted-candidate");
  input["manifest"]["base_revision"] = serde_json::json!("accepted-candidate");
  input["handoff"]["predecessor_revision"] = serde_json::json!("accepted-candidate");
  input["manifest"]["changed_paths"][0]["status"] = serde_json::json!("M");
  input["manifest"]["changed_paths"][1]["status"] = serde_json::json!("M");
  assert_eq!(select("specification_gate", input.clone()), "review_pending");
  input["handoff"]["predecessor_revision"] = serde_json::json!("superseded-candidate");
  input["manifest"]["base_revision"] = serde_json::json!("superseded-candidate");
  assert_eq!(select("specification_gate", input), "invalid");
}

#[test]
fn supplied_reviewer_selects_all_declared_requirements_as_closed_immutable_context() {
  let artifact = |number| serde_json::json!({"artifact_id":format!("00000000-0000-0000-0000-{number:012}"),"content_digest":format!("{number:064x}"),"encoded_size":100});
  let declared = (10..16).map(artifact).collect::<Vec<_>>();
  let published = published();
  let node = published.definition.node(&key("specification_review")).unwrap();
  let schema = published
    .schemas
    .iter()
    .find(|schema| Some(schema.reference()) == node.input_schema())
    .unwrap();
  let input=FlowPayload::new(schema,serde_json::json!({"request":{"work_id":"work-42","original_base":"original","candidate_revision":"reviewed","manifest_digest":"a".repeat(64),"bundle_digest":"b".repeat(64)},"original_acceptance":artifact(3),"artifacts":{"manifest":artifact(1),"bundle":artifact(2)},"acceptance_criteria":["specified behavior"],"regression_criteria":[],"declared_requirements":declared})).unwrap();
  let selected = node.context_binding().unwrap().selected_sources(&input).unwrap();
  assert_eq!(selected.len(), 9);
  for index in 0..6 {
    assert!(selected.contains_key(&key(&format!("requirements.{index}"))));
  }
  assert!(!selected.contains_key(&key("task")));
}

#[test]
fn execution_feedback_cannot_replace_the_frozen_specification_or_its_candidate() {
  let authored = specification_feature_input();
  let specification = serde_json::json!({"work_id":authored["work_id"],"original_base":authored["original_base"],"manifest":authored["manifest"],"handoff":authored["handoff"],"readiness":"review_pending","artifacts":authored["artifacts"]});
  let trusted = serde_json::json!({"work_kind":"feature_request","size":"large","risk":"low","admission_risk":"low","component":"api","severity":"high","dependencies":[],"recommended_route":"requirements","research_required":false,"research_accepted":false,"specification":specification});
  let rework = serde_json::json!({"trusted_input":trusted,"execution_revision":"previous-code-candidate","result":trusted,"expected_input":"a".repeat(64),"expected_report":"b".repeat(64),"facts":{"input_digest":"a".repeat(64),"report_digest":"b".repeat(64),"fresh_until":100,"outcome":"observed","candidate_revision":"previous-code-candidate","requirements_evidence":[]}});
  assert_eq!(select("implementation_result_gate", rework), "continue");
  let mut result = trusted.clone();
  result["requirements_defect"] = serde_json::json!({"reason":"Missing acceptance case","candidate_revision":"spec-candidate","evidence":[authored["artifacts"]["manifest"].clone()]});
  let input = serde_json::json!({"trusted_input":trusted,"execution_revision":"original","result":result,"expected_input":"a".repeat(64),"expected_report":"b".repeat(64),"facts":{"input_digest":"a".repeat(64),"report_digest":"b".repeat(64),"fresh_until":100,"outcome":"requirements_defect","candidate_revision":"spec-candidate","requirements_evidence":result["requirements_defect"]["evidence"]}});
  for node in ["implementation_result_gate", "verification_result_gate"] {
    assert_eq!(select(node, input.clone()), "requirements_defect");
    for path in [
      "/result/specification/manifest/candidate_revision",
      "/result/specification/artifacts/manifest/content_digest",
    ] {
      let mut replaced = input.clone();
      *replaced.pointer_mut(path).unwrap() = if path.ends_with("content_digest") {
        serde_json::json!("e".repeat(64))
      } else {
        serde_json::json!("older-candidate")
      };
      assert_eq!(select(node, replaced), "invalid");
    }
    let mut wrong_candidate = input.clone();
    wrong_candidate["facts"]["candidate_revision"] = serde_json::json!("other-candidate");
    wrong_candidate["result"]["requirements_defect"]["candidate_revision"] = serde_json::json!("other-candidate");
    assert_eq!(select(node, wrong_candidate), "invalid");
    let published = published();
    let wire = serde_json::to_value(published.definition.node(&key(node)).unwrap()).unwrap();
    assert_eq!(
      wire["input_binding"]["sources"]["trusted_input"]["view"],
      "input_payload"
    );
  }
}

#[test]
fn a_requirements_defect_requires_exact_independently_verified_evidence_references() {
  let t = template();
  let schemas = t.data_schemas().unwrap();
  let summary = serde_json::json!({"work_kind":"defect","size":"small","risk":"low","admission_risk":"low","component":"api","severity":"high","dependencies":[],"recommended_route":"development","research_required":false,"research_accepted":false});
  let mut report = summary.clone();
  report["requirements_defect"] =
    serde_json::json!({"reason":"Contradictory requirement","candidate_revision":"current-candidate"});
  assert!(
    FlowPayload::new(&schemas[&key("summary")], report.clone()).is_err(),
    "a reason alone does not identify requirements evidence"
  );
  let evidence = serde_json::json!([{"artifact_id":"00000000-0000-0000-0000-000000000012","content_digest":"c".repeat(64),"encoded_size":100}]);
  report["requirements_defect"]["evidence"] = evidence.clone();
  let input = serde_json::json!({"trusted_input":summary,"execution_revision":"current-candidate","result":report,"expected_input":"a".repeat(64),"expected_report":"b".repeat(64),"facts":{"input_digest":"a".repeat(64),"report_digest":"b".repeat(64),"fresh_until":100,"outcome":"requirements_defect","candidate_revision":"current-candidate","requirements_evidence":evidence}});
  for node in ["implementation_result_gate", "verification_result_gate"] {
    assert_eq!(select(node, input.clone()), "requirements_defect");
    let mut substituted = input.clone();
    substituted["result"]["requirements_defect"]["evidence"][0]["content_digest"] = serde_json::json!("d".repeat(64));
    assert_eq!(select(node, substituted), "invalid");
    let mut empty = input["result"].clone();
    empty["requirements_defect"]["evidence"] = serde_json::json!([]);
    assert!(FlowPayload::new(&schemas[&key("summary")], empty).is_err());
  }
}

#[test]
fn template_deliverables_cannot_weaken_an_installed_capture_policy() {
  let original = template();
  let project = octacity_server_domain::ProjectId::generate();
  let installed = published()
    .definition
    .node(&key("implementation"))
    .unwrap()
    .build()
    .unwrap()
    .capture_deliverables
    .clone()
    .unwrap();
  let profiles = original
    .nodes
    .iter()
    .filter_map(|node| node.build_profile.as_ref())
    .map(|choice| {
      let mut binding = binding(project);
      if choice == &key("implementation") {
        binding.capture_deliverables = Some(installed.clone());
      }
      (choice.clone(), binding)
    })
    .collect::<BTreeMap<_, _>>();
  original
    .instantiate(FlowDefinitionId::generate(), FlowDefinitionVersion::INITIAL, &profiles)
    .unwrap();
  for weakened in ["extra_path", "missing_required"] {
    let mut wire = serde_json::to_value(&original).unwrap();
    let index = wire["nodes"]
      .as_array()
      .unwrap()
      .iter()
      .position(|node| node["key"] == "implementation")
      .unwrap();
    let reference = wire["nodes"][index]["capture_deliverables"]["policy"]
      .get("reference")
      .and_then(serde_json::Value::as_str)
      .map(str::to_owned);
    let policy = if let Some(reference) = reference {
      &mut wire["deliverable_policies"][reference]
    } else {
      &mut wire["nodes"][index]["capture_deliverables"]["policy"]
    };
    if weakened == "extra_path" {
      policy["permitted_paths"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!("vendor"));
    } else {
      policy["required"].as_array_mut().unwrap().pop();
    }
    let changed: FlowDefinitionTemplate = serde_json::from_value(wire).unwrap();
    assert!(
      changed
        .instantiate(FlowDefinitionId::generate(), FlowDefinitionVersion::INITIAL, &profiles)
        .is_err(),
      "{weakened} cannot weaken the installed policy"
    );
  }
}
