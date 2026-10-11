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

fn specification_feature_input() -> serde_json::Value {
  serde_json::json!({
    "allowed_paths":["openspec/changes", "openspec/specs"],
    "artifacts":{"manifest":{"artifact_id":"manifest-42","content_digest":"c".repeat(64),"encoded_size":100},"bundle":{"artifact_id":"bundle-42","content_digest":"d".repeat(64),"encoded_size":100}}, "output_deadlines":{"capture":100,"handoff":100,"facts":100}, "work_id":"work-42", "original_base":"original", "work_kind":"feature_request", "expected_input":"a".repeat(64), "handoff_digest":"b".repeat(64),
    "manifest":{"format_version":1,"base_revision":"research-candidate","candidate_revision":"spec-candidate","node_attempt_id":"attempt-42","capture_tool":{"identity":"capture","version":"v1","sha256":"c".repeat(64)},"changed_paths":[{"status":"A","path":"openspec/specs/api/spec.md","old_mode":"000000","new_mode":"100644"},{"status":"A","path":"openspec/changes/new-api/proposal.md","old_mode":"000000","new_mode":"100644"}],"bundle":{"name":"change-set.bundle","size_bytes":100,"sha256":"d".repeat(64)}},
    "handoff":{"work_id":"work-42","original_base":"original","predecessor_revision":"research-candidate","candidate_revision":"spec-candidate","input_digest":"a".repeat(64),"work_kind":"feature_request","contract_path":"openspec/specs/api/spec.md","change_path":"openspec/changes/new-api/proposal.md","acceptance_criteria":["new API has documented behavior"],"regression_criteria":[]},
    "facts":{"accepted":true,"contract_exists":true,"input_digest":"a".repeat(64),"handoff_digest":"b".repeat(64),"fresh_until":100}
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
    &FlowTransitionTarget::Node(key("specification_author"))
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
  let choice = &author_only.schemas[&key("spec_author_input")];
  let incoming_schema =
    FlowDataSchema::new(choice.identity.clone(), choice.version.clone(), choice.shape.clone()).unwrap();
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
    serde_json::json!({"work_id":work.id(),"original_base":work.subject().base_revision(),"allowed_paths":["src"],"accepted_input":{"work_kind":"feature_request","size":"medium","risk":"low","admission_risk":"low","recommended_route":"development","research_required":false,"research_accepted":false,"component":"api","severity":"low","dependencies":[]}})).unwrap()).unwrap();
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
  let choice = &configured.schemas[&key("spec_gate_input")];
  let schema = FlowDataSchema::new(choice.identity.clone(), choice.version.clone(), choice.shape.clone()).unwrap();
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
  for (outcome, target) in [
    ("confirmed", "specification_reviewed"),
    ("declined", "specification_review_declined"),
  ] {
    transitions.push(serde_json::json!({"predecessor":"contract_owner_confirmation","outcome":outcome,"target":{"kind":"terminal","key":target},"max_repeats":0,"exhausted_terminal":null}));
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
