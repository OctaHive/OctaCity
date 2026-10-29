//! Bounded analyst contracts and ordinary immutable result ownership.
use super::*;

fn analyst_assessment() -> serde_json::Value {
  serde_json::json!({
    "priority_assessment": 75, "work_kind": "feature_request", "severity": "advisory",
    "size": "small", "component": "server", "risk": "low", "dependencies": [],
    "recommended_route": "requirements", "reproducibility": "not_applicable"
  })
}

#[test]
fn triage_analyst_result_accepts_bounded_assessments_and_rejects_unknown_vocabulary() {
  let schemas = triage_template().unwrap().data_schemas().unwrap();
  let schema = &schemas[&key("classification_observations")];
  let valid = analyst_assessment();
  FlowPayload::new(schema, valid.clone()).expect("priority is an assessment, separate from scheduling");
  for priority in [0, 100] {
    let mut value = valid.clone();
    value["priority_assessment"] = priority.into();
    assert!(FlowPayload::new(schema, value).is_ok());
  }
  for (field, invalid) in [
    ("priority_assessment", serde_json::json!(-1)),
    ("priority_assessment", serde_json::json!(101)),
    ("priority_assessment", serde_json::json!(1.5)),
    ("work_kind", serde_json::json!("chore")),
    ("severity", serde_json::json!("urgent")),
    ("size", serde_json::json!("huge")),
    ("component", serde_json::json!("invented-component")),
    ("risk", serde_json::json!("none")),
    ("recommended_route", serde_json::json!("execute-this-command")),
    ("reproducibility", serde_json::json!("probably")),
    ("dependencies", serde_json::json!([{"run_id":"invented"}])),
  ] {
    let mut value = valid.clone();
    value[field] = invalid;
    assert!(FlowPayload::new(schema, value).is_err(), "must reject {field}");
  }
  let mut value = valid;
  value["labels"] = serde_json::json!(["provider-owned"]);
  assert!(
    FlowPayload::new(schema, value).is_err(),
    "no implicit Issue metadata contract"
  );
}

fn unobserved_gate_input() -> serde_json::Value {
  serde_json::json!({
    "assessment":analyst_assessment(), "admission_priority":20,"admission_risk":"low",
    "command_policy":[],"observed_policy":[],"expected_scope":[],"reported_scope":[],
    "allowed_dependencies":[],"result_fresh_until":2000,"fresh_until":2000,
    "finding":"absent","command_status":"absent","evidence":"absent"
  })
}

#[test]
fn triage_without_command_preserves_truthful_feature_and_unresolved_results() {
  for (kind, severity, reproduction, expected) in [
    ("feature_request", "advisory", "not_applicable", "requirements"),
    ("defect", "high", "unknown", "defect_research"),
    ("defect", "high", "needs_research", "defect_research"),
    ("defect", "high", "needs_input", "escalation"),
    ("defect", "high", "reproduced", "escalation"),
    ("defect", "high", "not_reproduced", "escalation"),
  ] {
    let mut input = unobserved_gate_input();
    input["assessment"]["work_kind"] = kind.into();
    input["assessment"]["severity"] = severity.into();
    input["assessment"]["reproducibility"] = reproduction.into();
    assert_eq!(
      select_classification(input).unwrap(),
      key(expected),
      "{kind}/{reproduction}"
    );
  }
}

fn observed_gate_input(finding: &str) -> serde_json::Value {
  let project = octacity_server_domain::ProjectId::generate();
  let binding = serde_json::to_value(binding(project)).unwrap();
  let pin = binding["tool"].clone();
  let policy = serde_json::json!({
    "command":binding["model_or_tool"], "tool":pin, "plugin":binding["plugin"],
    "task_digest":binding["task_digest"], "build_configuration":binding["build_configuration"],
    "result_output":"reproduction-report", "schema":pin,"environment":pin,"criterion":pin
  });
  let scope = serde_json::json!({"input_digest":"ab".repeat(32),
    "subject":{"project_id":project,"repository_id":octacity_server_domain::RepositoryId::generate(),"base_revision":"abc123"},
    "command":policy["command"],"output":policy["result_output"]});
  let evidence = serde_json::json!({"observed":{
    "policy":policy,"scope":scope,"owner_input_digest":"ab".repeat(32),
    "record_digest":"cd".repeat(32),"status":"completed","finding":finding,"fresh_until":2000,
    "producer":{"build_id":"build","attempt_id":"attempt","job_id":"job"}
  }});
  let mut value = unobserved_gate_input();
  value["command_policy"] = serde_json::json!([policy]);
  value["observed_policy"] = serde_json::json!([policy]);
  value["expected_scope"] = serde_json::json!([scope]);
  value["reported_scope"] = serde_json::json!([scope]);
  value["evidence"] = evidence;
  value["finding"] = finding.into();
  value["command_status"] = "completed".into();
  value["assessment"]["work_kind"] = "defect".into();
  value["assessment"]["severity"] = "high".into();
  value["assessment"]["reproducibility"] = if finding == "matched" {
    "reproduced"
  } else {
    "not_reproduced"
  }
  .into();
  value
}

#[test]
fn triage_accepts_only_applicable_completed_command_findings() {
  for finding in ["matched", "not_matched"] {
    let value = observed_gate_input(finding);
    assert_eq!(select_classification(value.clone()).unwrap(), key("requirements"));
    for (path, replacement) in [
      ("/finding", serde_json::json!("indeterminate")),
      ("/command_status", serde_json::json!("environment_unavailable")),
      ("/reported_scope/0/input_digest", serde_json::json!("ef".repeat(32))),
      ("/reported_scope/0/subject/base_revision", serde_json::json!("changed")),
      (
        "/observed_policy/0/environment/digest",
        serde_json::json!("ef".repeat(32)),
      ),
      (
        "/observed_policy/0/criterion/digest",
        serde_json::json!("ef".repeat(32)),
      ),
      ("/observed_policy/0/task_digest", serde_json::json!("ef".repeat(32))),
      ("/fresh_until", serde_json::json!(1000)),
    ] {
      let mut changed = value.clone();
      *changed.pointer_mut(path).unwrap() = replacement;
      assert_eq!(
        select_classification(changed).unwrap(),
        key("escalation"),
        "{finding}/{path}"
      );
    }
  }
}

#[test]
fn triage_projects_absent_command_and_independent_assessment_without_a_synthetic_facts_report() {
  let template = triage_template().unwrap();
  let schemas = template.data_schemas().unwrap();
  let work = crate::flow_test_support::work();
  let node = template
    .nodes
    .iter()
    .find(|n| n.key.as_str() == "classification")
    .unwrap();
  let binding = serde_json::to_value(&node.input_binding).unwrap();
  let mapping: FlowDataMapping = serde_json::from_value(binding["mapping"].clone()).unwrap();
  let input = mapping
    .project_value(
      &serde_json::json!({
        "task":work.artifacts().task(),"completion":work.artifacts().acceptance(),"priority":20,"risk":"low",
        "eligibility":{"accepted":true,"decision":"eligible","reason":"Reviewed fit","evidence":[]},
        "previous":{"build":null}
      }),
      &schemas[&key("analyst_input")],
    )
    .unwrap();
  assert_eq!(input.value()["command_policy"], serde_json::json!([]));
  assert_eq!(input.value()["observation"], "absent");
  let node = template
    .nodes
    .iter()
    .find(|n| n.key.as_str() == "classification_gate")
    .unwrap();
  let binding = serde_json::to_value(&node.input_binding).unwrap();
  let mapping: FlowDataMapping = serde_json::from_value(binding["mapping"].clone()).unwrap();
  let gate = mapping
    .project_value(
      &serde_json::json!({"observations":analyst_assessment(),
        "frozen":input.value(),"priority":20,"risk":"low","subject":work.subject(),"result_freshness":2000
      }),
      &schemas[&key("classification_gate_input")],
    )
    .unwrap();
  assert_eq!(gate.value(), &unobserved_gate_input());
  assert_eq!(
    node
      .gate
      .as_ref()
      .unwrap()
      .select_at(
        &gate,
        octacity_server_domain::Timestamp::from_unix_millis(1000).unwrap()
      )
      .unwrap(),
    key("requirements")
  );
}

fn project_node(template: &FlowDefinitionTemplate, name: &str, sources: serde_json::Value) -> FlowPayload {
  let node = template.nodes.iter().find(|n| n.key.as_str() == name).unwrap();
  let binding = serde_json::to_value(&node.input_binding).unwrap();
  let mapping: FlowDataMapping = serde_json::from_value(binding["mapping"].clone()).unwrap();
  mapping
    .project_value(&sources, &template.data_schemas().unwrap()[&node.input_schema])
    .unwrap()
}

#[test]
fn triage_analyst_consumes_only_the_safe_owner_projection_and_retains_command_evidence() {
  let mut template = triage_template().unwrap();
  let expected = observed_gate_input("not_matched");
  let observed = &expected["evidence"]["observed"];
  let policy = &observed["policy"];
  let mut wire = serde_json::to_value(&template).unwrap();
  let analyst = wire["nodes"]
    .as_array_mut()
    .unwrap()
    .iter_mut()
    .find(|n| n["key"] == "classification")
    .unwrap();
  analyst["input_binding"]["mapping"]["value"]["fields"]["command_policy"]["value"] =
    expected["command_policy"].clone();
  template = serde_json::from_value(wire).unwrap();
  let previous = serde_json::json!({"record_digest":observed["record_digest"],"input_digest":observed["owner_input_digest"],"build":{
    "profile":{"model_or_tool":policy["command"],"task_digest":policy["task_digest"],"build_configuration":policy["build_configuration"],"result_output":"reproduction-report"},
    "outputs":{"reproduction-report":{
      "name":"reproduction-report","schema":policy["schema"],"fresh_until":2000,
      "producer":{"build_id":"build","attempt_id":"attempt","job_id":"job","tool":policy["tool"],"plugin":policy["plugin"]},
      "data":{"payload":{"value":{"input_digest":"ab".repeat(32),"subject":observed["scope"]["subject"],
        "command":policy["command"],"environment":policy["environment"],"criterion":policy["criterion"],
        "status":"completed","finding":"not_matched","observations":{"private_probe_output":"do not forward"}}}}
    }}
  }});
  let work = crate::flow_test_support::work();
  let input = project_node(
    &template,
    "classification",
    serde_json::json!({"task":work.artifacts().task(),"completion":work.artifacts().acceptance(),
    "priority":20,"risk":"low","previous":previous,"eligibility":{"accepted":true,"decision":"eligible","reason":"Reviewed fit","evidence":[]}}),
  );
  assert_eq!(input.value()["observation"], expected["evidence"]);
  assert!(
    !serde_json::to_string(input.value())
      .unwrap()
      .contains("private_probe_output")
  );
  let gate = project_node(
    &template,
    "classification_gate",
    serde_json::json!({"observations":expected["assessment"],"frozen":input.value(),
    "priority":20,"risk":"low","subject":observed["scope"]["subject"],"result_freshness":2000}),
  );
  assert_eq!(gate.value(), &expected);
  assert_eq!(
    select_classification(gate.value().clone()).unwrap(),
    key("requirements")
  );
}

fn select_classification(value: serde_json::Value) -> Result<FactoryKey, FactoryError> {
  let template = triage_template()?;
  let schemas = template.data_schemas()?;
  let input = FlowPayload::new(&schemas[&key("classification_gate_input")], value)?;
  template
    .nodes
    .iter()
    .find(|node| node.key.as_str() == "classification_gate")
    .unwrap()
    .gate
    .as_ref()
    .unwrap()
    .select_at(
      &input,
      octacity_server_domain::Timestamp::from_unix_millis(1000).unwrap(),
    )
}

#[test]
fn triage_analyst_gate_rejects_stale_results_and_unreviewed_dependencies() {
  let valid = unobserved_gate_input();
  assert_eq!(select_classification(valid.clone()).unwrap(), key("requirements"));
  for (path, replacement) in [
    ("/fresh_until", serde_json::json!(1000)),
    ("/result_fresh_until", serde_json::json!(1000)),
    (
      "/assessment/dependencies",
      serde_json::json!([{
        "run_id":"00000000-0000-0000-0000-000000000001", "work_id":"00000000-0000-0000-0000-000000000002",
        "work_digest":"ef".repeat(32), "terminal":"completed"
      }]),
    ),
  ] {
    let mut value = valid.clone();
    *value.pointer_mut(path).unwrap() = replacement;
    assert_eq!(select_classification(value).unwrap(), key("escalation"), "{path}");
  }
}

#[test]
fn triage_analyst_gate_checks_cross_field_rules_and_keeps_uncertainty() {
  for (kind, severity, reproduction, route, verified, expected) in [
    (
      "feature_request",
      "advisory",
      "not_applicable",
      "requirements",
      false,
      "requirements",
    ),
    (
      "feature_request",
      "advisory",
      "not_applicable",
      "research",
      false,
      "feature_research",
    ),
    (
      "feature_request",
      "critical",
      "not_applicable",
      "requirements",
      false,
      "escalation",
    ),
    (
      "feature_request",
      "advisory",
      "reproduced",
      "requirements",
      true,
      "escalation",
    ),
    (
      "feature_request",
      "advisory",
      "not_applicable",
      "already_fixed",
      true,
      "escalation",
    ),
    ("defect", "high", "not_applicable", "requirements", false, "escalation"),
    ("defect", "high", "unknown", "research", false, "defect_research"),
    (
      "defect",
      "high",
      "needs_research",
      "requirements",
      false,
      "defect_research",
    ),
    ("defect", "high", "needs_input", "requirements", false, "escalation"),
    ("defect", "high", "reproduced", "requirements", false, "escalation"),
    ("defect", "high", "not_reproduced", "requirements", false, "escalation"),
    ("defect", "high", "reproduced", "requirements", true, "requirements"),
    ("defect", "high", "unknown", "already_fixed", false, "escalation"),
  ] {
    let mut value = if verified {
      observed_gate_input("matched")
    } else {
      unobserved_gate_input()
    };
    value["assessment"]["work_kind"] = kind.into();
    value["assessment"]["severity"] = severity.into();
    value["assessment"]["reproducibility"] = reproduction.into();
    value["assessment"]["recommended_route"] = route.into();
    assert_eq!(
      select_classification(value).unwrap(),
      key(expected),
      "{kind}/{severity}/{reproduction}/{route}"
    );
  }
  for field in ["admission_risk", "risk"] {
    let mut value = unobserved_gate_input();
    if field == "risk" {
      value["assessment"][field] = "high".into();
    } else {
      value[field] = "high".into();
    }
    assert_eq!(select_classification(value).unwrap(), key("escalation"));
  }
}

#[test]
fn triage_public_results_are_explicit_immutable_outcome_choices() {
  let template = triage_template().unwrap();
  let profiles = ["eligibility", "classification"]
    .into_iter()
    .map(|name| (key(name), binding(octacity_server_domain::ProjectId::generate())))
    .collect();
  let published = template
    .instantiate(FlowDefinitionId::generate(), FlowDefinitionVersion::INITIAL, &profiles)
    .unwrap();
  for node in published.definition.nodes() {
    for outcome in node.outcomes() {
      let wire = serde_json::to_value(outcome).unwrap();
      if node.key().as_str() == "classification_gate" {
        assert_eq!(wire["public_result"], true, "only reviewed summaries are public");
      } else {
        assert!(
          wire.get("public_result").is_none(),
          "historical/default outcomes retain their bytes"
        );
      }
    }
  }
}

#[test]
fn triage_analyst_gate_retains_results_and_rejects_substituted_dependency_references() {
  let dependency = serde_json::json!({"run_id":"00000000-0000-0000-0000-000000000001",
    "work_id":"00000000-0000-0000-0000-000000000002", "work_digest":"ef".repeat(32), "terminal":"completed"});
  let mut value = observed_gate_input("matched");
  value["assessment"]["dependencies"] = serde_json::json!([dependency]);
  value["allowed_dependencies"] = value["assessment"]["dependencies"].clone();
  assert_eq!(select_classification(value.clone()).unwrap(), key("requirements"));
  for (field, replacement) in [
    ("run_id", serde_json::json!("00000000-0000-0000-0000-000000000003")),
    ("work_id", serde_json::json!("00000000-0000-0000-0000-000000000004")),
    ("work_digest", serde_json::json!("cd".repeat(32))),
    ("terminal", serde_json::json!("rejected")),
  ] {
    let mut changed = value.clone();
    changed["assessment"]["dependencies"][0][field] = replacement;
    assert_eq!(
      select_classification(changed).unwrap(),
      key("escalation"),
      "exact dependency {field}"
    );
  }
  // Isolate the released gate to qualify its ordinary record/restore boundary;
  // eligibility-to-analyst admission is covered by the separate owner lifecycle test.
  let mut template = triage_template().unwrap();
  template.entry = key("classification_gate");
  template.input_schema = key("classification_gate_input");
  template.nodes.retain(|node| node.key == template.entry);
  template
    .transitions
    .retain(|transition| transition.predecessor() == &template.entry);
  template
    .terminals
    .retain(|name, _| matches!(name.as_str(), "human_review" | "triaged"));
  let schemas = template.data_schemas().unwrap();
  let published = template
    .instantiate(
      FlowDefinitionId::generate(),
      FlowDefinitionVersion::INITIAL,
      &BTreeMap::new(),
    )
    .unwrap();
  let definition = published.definition;
  let work = crate::flow_test_support::work();
  let frozen_work = serde_json::to_vec(&work).unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let flow = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition.clone()]).unwrap(),
    FlowAdmissionLimits::product_defaults(1, template.execution.budget(), FactoryPermissionSet::deny_all()).unwrap(),
    flow,
    cycle,
    FactoryWipLimits::new(1, 1).unwrap(),
  )
  .unwrap()
  .with_data_schemas(published.schemas)
  .unwrap();
  let at = |n| octacity_server_domain::Timestamp::from_unix_millis(n).unwrap();
  let ownership = FactoryClaimOwnership::new(
    key("worker"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([42; 32])),
      at(1000),
      at(1100),
    )
    .unwrap(),
  );
  for accepted in [true, false] {
    let mut input_value = value.clone();
    input_value["admission_priority"] = work.priority().get().into();
    if !accepted {
      input_value["fresh_until"] = 1000.into();
    }
    let input = FlowNodeInput::new(
      &work,
      &admitted,
      admitted.root_run(),
      admitted.initial_cycle(),
      template.entry.clone(),
      FlowPayload::new(&schemas[&key("classification_gate_input")], input_value).unwrap(),
    )
    .unwrap();
    let attempt = NodeAttempt::new(
      admitted.root_run(),
      admitted.initial_cycle(),
      &definition,
      NodeAttemptInput {
        id: NodeAttemptId::generate(),
        node_key: template.entry.clone(),
        node_kind: FlowNodeKind::DeterministicGate,
        number: NodeAttemptNumber::INITIAL,
        input_digest: input.digest().unwrap(),
        budget: template.nodes[0].budget,
        started_at: at(1000),
        deadline: at(1100),
        execution: NodeExecutionIdentity::BuiltIn,
        ownership: ownership.clone(),
      },
    )
    .unwrap();
    let schema = &schemas[&key("summary")];
    let record = template.nodes[0]
      .gate
      .as_ref()
      .unwrap()
      .record(&input, &attempt, &definition, schema, at(1001))
      .unwrap();
    assert_eq!(
      record.payload().value(),
      &serde_json::json!({"accepted":accepted,"assessment":value["assessment"],
      "admission_priority":work.priority(), "admission_risk":work.risk(), "evidence":value["evidence"]})
    );
    let completion = record
      .completion(
        &attempt,
        &definition,
        ownership.clone(),
        BudgetUsage::default(),
        at(1002),
      )
      .unwrap();
    let bytes = serde_json::to_vec(&record).unwrap();
    let restored = FlowNodeRecord::restore(&bytes, &input, &attempt, &definition, schema, &completion).unwrap();
    assert_eq!(restored, record);
    let mut substituted: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    substituted["payload"]["value"]["assessment"]["priority_assessment"] = 100.into();
    assert!(
      FlowNodeRecord::restore(
        &serde_json::to_vec(&substituted).unwrap(),
        &input,
        &attempt,
        &definition,
        schema,
        &completion
      )
      .is_err()
    );
    assert_eq!(record.payload().schema(), schema.reference());
    assert_eq!(
      record.outcome(),
      &key(if accepted { "requirements" } else { "escalation" })
    );
  }
  assert_eq!(serde_json::to_vec(&work).unwrap(), frozen_work);
}
