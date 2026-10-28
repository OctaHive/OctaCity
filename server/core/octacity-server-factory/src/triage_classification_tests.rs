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

fn classification_gate_input() -> serde_json::Value {
  let assessment = analyst_assessment();
  let producer = serde_json::json!({"build_id":"build", "attempt_id":"attempt", "job_id":"job"});
  serde_json::json!({
    "assessment": assessment, "verified_assessment": assessment,
    "expected_input": "ab".repeat(32), "evidence_input": "ab".repeat(32), "fresh_until": 2000,
    "result_fresh_until": 2000,
    "producer": producer, "evidence_producer": {"build_id":"build", "attempt_id":"attempt", "job_id":"verification-job"},
    "admission_priority": 20, "admission_risk": "low",
    "reproduced_verified": false, "already_fixed_verified": false, "verification_only_verified": false
  })
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
fn triage_analyst_gate_requires_applicable_matching_verified_assessments() {
  let valid = classification_gate_input();
  assert_eq!(select_classification(valid.clone()).unwrap(), key("requirements"));
  for (path, replacement) in [
    ("/evidence_input", serde_json::json!("cd".repeat(32))),
    ("/fresh_until", serde_json::json!(1000)),
    ("/result_fresh_until", serde_json::json!(1000)),
    ("/evidence_producer/build_id", serde_json::json!("another-build")),
    ("/evidence_producer/attempt_id", serde_json::json!("another-attempt")),
    ("/assessment/priority_assessment", serde_json::json!(60)),
    ("/assessment/component", serde_json::json!("agent")),
    ("/assessment/size", serde_json::json!("large")),
    ("/assessment/severity", serde_json::json!("low")),
    ("/assessment/risk", serde_json::json!("medium")),
    ("/assessment/recommended_route", serde_json::json!("research")),
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
    assert_eq!(
      select_classification(value).unwrap(),
      key("escalation"),
      "substitution must fail closed: {path}"
    );
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
    let mut value = classification_gate_input();
    for name in ["assessment", "verified_assessment"] {
      value[name]["work_kind"] = kind.into();
      value[name]["severity"] = severity.into();
      value[name]["reproducibility"] = reproduction.into();
      value[name]["recommended_route"] = route.into();
    }
    value["reproduced_verified"] = verified.into();
    value["already_fixed_verified"] = verified.into();
    assert_eq!(
      select_classification(value).unwrap(),
      key(expected),
      "{kind}/{severity}/{reproduction}/{route}"
    );
  }
  for field in ["admission_risk", "risk"] {
    let mut value = classification_gate_input();
    if field == "risk" {
      value["assessment"][field] = "high".into();
      value["verified_assessment"][field] = "high".into();
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
  let mut value = classification_gate_input();
  value["assessment"]["dependencies"] = serde_json::json!([dependency]);
  value["verified_assessment"] = value["assessment"].clone();
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
      "admission_priority":work.priority(), "admission_risk":work.risk()})
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
