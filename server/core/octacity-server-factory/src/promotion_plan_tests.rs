use crate::flow_test_support::{key, reference};
use crate::*;

fn fixture() -> (AdmittedFlow, FlowNodeInput) {
  {
    let (_, admitted, input) = fixture_with_policy(|_| {});
    (admitted, input)
  }
}
fn fixture_with_policy(change: impl FnOnce(&mut serde_json::Value)) -> (WorkEnvelope, AdmittedFlow, FlowNodeInput) {
  let (work, admitted, _, input, _) =
    crate::flow_test_support::promotion_execution_fixture_with_policy(None, |policy| {
      policy["strategy"] = serde_json::json!("operator.rollout-choice");
      policy["environments"] = serde_json::json!({"sandbox":reference("environment.snapshot")});
      policy["allocation"] = serde_json::json!({"cohort":"pilot","basis_points":1000});
      policy["observation_window_millis"] = serde_json::json!(100);
      change(policy);
    });
  (work, admitted, input)
}

#[test]
fn promotion_plan_freezes_operator_policy_and_exact_candidate_against_replacement() {
  let (admitted, input) = fixture();
  let plan = PromotionPlan::freeze(&admitted, &input).unwrap();
  assert_eq!(plan.subject().candidate_revision().as_str(), "accepted-candidate");
  assert_eq!(plan.policy().strategy.as_str(), "operator.rollout-choice");
  assert_eq!(plan.policy().allocation["cohort"], "pilot");
  assert_eq!(plan.policy().observation_window_millis, 100);
  let bytes = serde_json::to_vec(&plan).unwrap();
  assert_eq!(
    PromotionPlan::restore(&bytes, &admitted, &input, plan.digest().unwrap()).unwrap(),
    plan
  );
  let (replacement, _) = fixture();
  assert!(PromotionPlan::restore(&bytes, &replacement, &input, plan.digest().unwrap()).is_err());
  let mut tampered = serde_json::to_value(&plan).unwrap();
  tampered["policy"]["allocation"]["basis_points"] = serde_json::json!(10000);
  assert!(
    PromotionPlan::restore(
      &serde_json::to_vec(&tampered).unwrap(),
      &admitted,
      &input,
      plan.digest().unwrap()
    )
    .is_err()
  );
}

#[test]
fn promotion_freezes_a_configured_human_disposition_without_a_dummy_deterministic_node() {
  let (work, previous, schema, old_input, _) = crate::flow_test_support::promotion_execution_fixture();
  let old = previous.closure().definition(old_input.definition()).unwrap();
  let mut human = serde_json::to_value(old.node(&key("decision")).unwrap()).unwrap();
  human["kind"] = serde_json::json!("human_gate");
  human.as_object_mut().unwrap().remove("gate");
  human["outcomes"].as_array_mut().unwrap().push(
    serde_json::to_value(FlowOutcomeDefinition::new(
      key("rollback"),
      FlowOutcomeKind::Success,
      schema.reference().clone(),
    ))
    .unwrap(),
  );
  let human: FlowNodeDefinition = serde_json::from_value(human).unwrap();
  let mut transitions = old.transitions().to_vec();
  transitions.push(FlowTransition::new(
    key("decision"),
    key("rollback"),
    FlowTransitionTarget::Terminal(key("done")),
  ));
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: old.input_schema().cloned(),
    entry: old.entry().clone(),
    nodes: vec![old.node(old.entry()).unwrap().clone(), human],
    transitions,
    terminals: old.terminals().to_vec(),
    context_projections: old.context_projections().to_vec(),
    data_projections: old.data_projections().to_vec(),
    execution: old.execution().clone(),
  })
  .unwrap();
  let flow = FlowRun::root(
    &FactoryRun::admitted(previous.root_run().factory_run_id(), &work),
    definition.reference(),
  )
  .unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    previous.limits().clone(),
    flow,
    cycle,
    previous.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(previous.data_schemas().to_vec())
  .unwrap();
  let input = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    old_input.node().clone(),
    old_input.payload().clone(),
  )
  .unwrap()
  .with_context(old_input.context().unwrap().clone(), vec![])
  .unwrap();
  let plan =
    PromotionPlan::freeze(&admitted, &input).expect("human continuation/rollback must be a valid configured criterion");
  plan.authorize(&work, &admitted, &input).unwrap();
  let public = serde_json::to_value(&plan).unwrap();
  assert_eq!(public["criteria"]["kind"], "human_gate");
  assert_eq!(public["criteria"]["outcomes"][0]["key"], "observed");
  assert_eq!(public["criteria"]["outcomes"][1]["key"], "rollback");
  assert_eq!(
    PromotionPlan::restore(
      &serde_json::to_vec(&plan).unwrap(),
      &admitted,
      &input,
      plan.digest().unwrap()
    )
    .unwrap(),
    plan
  );
}

#[test]
fn promotion_plan_rejects_unbounded_or_invented_operator_selections() {
  for (field, value) in [
    ("environments", serde_json::json!({})),
    ("allocation", serde_json::json!("unstructured commands")),
    ("observation_window_millis", serde_json::json!(0)),
    ("observation_window_millis", serde_json::json!(u64::MAX)),
    ("required_gates", serde_json::json!(["absent-gate"])),
    ("criteria", serde_json::json!("invented-route")),
  ] {
    let (_, admitted, input) = fixture_with_policy(|policy| policy[field] = value);
    assert!(
      PromotionPlan::freeze(&admitted, &input).is_err(),
      "invalid {field} must fail before deployment"
    );
  }
}

#[test]
fn production_without_gates_requires_explicit_low_risk_permission() {
  for permitted in [false, true] {
    let (work, admitted, input) = fixture_with_policy(|policy| {
      policy["production_environments"] = serde_json::json!(["sandbox"]);
      policy["allow_immediate_low_risk"] = serde_json::json!(permitted);
    });
    let plan = PromotionPlan::freeze(&admitted, &input).unwrap();
    assert_eq!(plan.authorize(&work, &admitted, &input).is_ok(), permitted);
    assert!(
      plan
        .authorize(&crate::flow_test_support::work(), &admitted, &input)
        .is_err()
    );
  }
  let (_, admitted, input) =
    fixture_with_policy(|policy| policy["production_environments"] = serde_json::json!(["unselected"]));
  assert!(PromotionPlan::freeze(&admitted, &input).is_err());
}

#[test]
fn trusted_actions_can_select_an_ordinary_build_only_for_the_exact_installed_capability() {
  let (admitted, input) = fixture();
  let node = admitted
    .closure()
    .definition(input.definition())
    .unwrap()
    .node(input.node())
    .unwrap();
  let mut build = crate::flow_test_support::binding(input.subject().project_id());
  assert!(
    node.clone().with_build(build.clone()).is_err(),
    "an unrelated tool cannot execute this action"
  );
  build.tool = node.action().unwrap().plugin().clone();
  let selected = node
    .clone()
    .with_build(build.clone())
    .expect("trusted action must reuse ordinary Build lifecycle");
  assert_eq!(selected.build(), Some(&build));
}

#[test]
fn promotion_gate_cannot_be_a_reasoning_or_action_result() {
  let (_, admitted, input) =
    fixture_with_policy(|policy| policy["required_gates"] = serde_json::json!(["accessibility_audit"]));
  assert!(
    PromotionPlan::freeze(&admitted, &input).is_err(),
    "only deterministic/human gates can authorize a write"
  );
}

#[test]
fn build_intent_freezes_action_parameters_for_trusted_task_compilation() {
  let (_, admitted, _, input, attempt) = crate::flow_test_support::promotion_execution_fixture();
  let expected = admitted
    .closure()
    .definition(input.definition())
    .unwrap()
    .node(input.node())
    .unwrap()
    .action()
    .unwrap();
  let intent = FlowBuildIntent::new(input.clone(), &admitted, attempt.clone(), BudgetUsage::default(), 7).unwrap();
  assert_eq!(intent.action(), Some(expected));
  let restored = FlowBuildIntent::restore(
    &serde_json::to_vec(&intent).unwrap(),
    &input,
    &admitted,
    &attempt,
    intent.operation_id().unwrap(),
  )
  .unwrap();
  assert_eq!(restored.action(), Some(expected));
}

#[test]
fn promotion_observations_bind_exact_scope_and_require_a_complete_passing_window() {
  let (admitted, input) = fixture();
  let plan = PromotionPlan::freeze(&admitted, &input).unwrap();
  let time = |t| octacity_server_domain::Timestamp::from_unix_millis(t).unwrap();
  let observation = PromotionObservation {
    plan_digest: plan.digest().unwrap(),
    input_digest: input.digest().unwrap(),
    subject: plan.subject().clone(),
    environment: key("sandbox"),
    environment_contract: plan.policy().environments[&key("sandbox")].clone(),
    deployment: reference("retained.deployment"),
    outcome: DeterministicGateOutcome::Passed,
    started_at: time(10),
    finished_at: time(110),
    measurements: std::collections::BTreeMap::from([(key("errors"), 0)]),
  };
  observation
    .verify(&plan, &input, time(10), time(120), time(111))
    .unwrap();
  for mutate in 0..7 {
    let mut wrong = observation.clone();
    match mutate {
      0 => wrong.plan_digest = FactoryDigest::from_bytes([0; 32]),
      1 => wrong.input_digest = FactoryDigest::from_bytes([0; 32]),
      2 => wrong.environment = key("other"),
      3 => wrong.environment_contract = reference("replacement"),
      4 => wrong.finished_at = time(109),
      5 => wrong.finished_at = time(121),
      _ => wrong
        .measurements
        .insert(key("unbounded"), u64::MAX)
        .map(|_| ())
        .unwrap_or(()),
    }
    assert!(
      wrong.verify(&plan, &input, time(10), time(120), time(111)).is_err(),
      "substitution/bounds case {mutate}"
    );
  }
  let mut failed = observation;
  failed.outcome = DeterministicGateOutcome::Failed;
  failed.finished_at = time(20);
  failed.verify(&plan, &input, time(10), time(120), time(21)).unwrap();
}

#[test]
fn a_build_backed_trusted_action_cannot_commit_a_provider_only_record() {
  let (_, admitted, schema, input, attempt) = crate::flow_test_support::promotion_execution_fixture();
  let definition = admitted.closure().definition(input.definition()).unwrap();
  let at = octacity_server_domain::Timestamp::from_unix_millis(31).unwrap();
  let record = FlowNodeRecord::new(
    &input,
    &attempt,
    definition,
    FlowRecordObservation {
      outcome: key("observed"),
      payload: FlowPayload::new(&schema, serde_json::json!({"count":9})).unwrap(),
      producer: attempt.execution().clone(),
      observed_at: at,
    },
  )
  .unwrap();
  assert!(
    record
      .completion(
        &attempt,
        definition,
        FactoryClaimOwnership::new(attempt.owner().clone(), attempt.claim()),
        BudgetUsage::default(),
        at
      )
      .is_err(),
    "Build-backed action requires ordinary retained proof"
  );
}

#[test]
fn promotion_criteria_are_the_actual_admitted_decision_program_and_budget() {
  let (_, admitted, _, input, attempt) = crate::flow_test_support::promotion_execution_fixture();
  let plan = PromotionPlan::freeze(&admitted, &input).unwrap();
  assert_eq!(plan.budget(), attempt.budget());
  assert_eq!(
    FlowGateProgram::from_node(plan.criteria()).unwrap(),
    FlowGateProgram::new(vec![], key("observed")).unwrap()
  );
}

fn supplied_rollout() -> PublishedFlowTemplate {
  let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .ancestors()
    .find(|path| path.join("config/factory/flows").is_dir())
    .unwrap()
    .join("config/factory/flows");
  let bytes = std::fs::read(root.join("promotion-flow.json")).unwrap_or_default();
  let result = FlowDefinitionTemplate::from_manifest(&bytes, |path| {
    std::fs::read(root.join(path.as_str())).map_err(|_| FactoryError::InvalidConfiguration { field: "example part" })
  });
  assert!(
    result.is_ok(),
    "the configurable rollout example must be available and valid: {result:?}"
  );
  let template = result.unwrap();
  let project = octacity_server_domain::ProjectId::generate();
  let profiles = template
    .nodes
    .iter()
    .filter_map(|node| {
      node.build_profile.as_ref().map(|name| {
        let mut profile = crate::flow_test_support::binding(project);
        profile.tool = node.action.as_ref().unwrap().plugin().clone();
        profile.result_output = key("deployment_receipt");
        (name.clone(), profile)
      })
    })
    .collect();
  template
    .instantiate(FlowDefinitionId::generate(), FlowDefinitionVersion::INITIAL, &profiles)
    .unwrap()
}

#[test]
fn supplied_rollout_flow_uses_configured_actions_and_finite_observation_routes() {
  let published = supplied_rollout();
  assert_eq!(
    published
      .definition
      .nodes()
      .iter()
      .filter(|node| node.build().is_some())
      .count(),
    2
  );
  assert!(
    published
      .definition
      .nodes()
      .iter()
      .filter(|node| node.build().is_some())
      .all(|node| node.kind() == FlowNodeKind::TrustedAction)
  );
  assert!(
    published
      .definition
      .terminals()
      .iter()
      .any(|terminal| terminal.key() == &key("rolled_back"))
  );
}

fn observation_policy_input(observed_outcome: DeterministicGateOutcome) -> (FlowGateProgram, FlowPayload) {
  let published = supplied_rollout();
  let gate = published.definition.node(&key("observation_policy")).unwrap();
  let receipt = published
    .schemas
    .iter()
    .find(|schema| schema.reference().identity().as_str() == "factory.promotion.deployment_receipt")
    .unwrap();
  let receipt_shape: FlowValueSchema =
    serde_json::from_value(serde_json::to_value(receipt).unwrap()["shape"].clone()).unwrap();
  let source_schema = FlowDataSchema::new(
    key("fixture.observation_sources"),
    key("v1"),
    FlowValueSchema::Object {
      fields: std::collections::BTreeMap::from([
        (key("observation"), FlowFieldSchema::required(receipt_shape)),
        (
          key("freshness"),
          FlowFieldSchema::required(FlowValueSchema::Integer {
            minimum: 0,
            maximum: 10000,
          }),
        ),
      ]),
    },
  )
  .unwrap();
  let (_, admitted, input) = fixture_with_policy(|policy| policy["observation_window_millis"] = serde_json::json!(50));
  let plan = PromotionPlan::freeze(&admitted, &input).unwrap();
  let time = |t| octacity_server_domain::Timestamp::from_unix_millis(t).unwrap();
  let observation = PromotionObservation {
    plan_digest: plan.digest().unwrap(),
    input_digest: input.digest().unwrap(),
    subject: plan.subject().clone(),
    environment: key("sandbox"),
    environment_contract: reference("environment.snapshot"),
    deployment: reference("retained.deployment"),
    outcome: observed_outcome,
    started_at: time(10),
    finished_at: time(60),
    measurements: std::collections::BTreeMap::from([(key("errors"), 0)]),
  };
  let source = FlowPayload::new(
    &source_schema,
    serde_json::json!({"observation":observation,"freshness":100}),
  )
  .unwrap();
  let binding = serde_json::to_value(gate.input_binding().unwrap()).unwrap();
  let mapping: FlowDataMapping = serde_json::from_value(binding["mapping"].clone()).unwrap();
  let schema = published
    .schemas
    .iter()
    .find(|schema| Some(schema.reference()) == gate.input_schema())
    .unwrap();
  let selected = mapping.project(&source, schema).unwrap();
  let program = FlowGateProgram::from_node(gate).unwrap();
  (program, selected)
}

#[test]
fn supplied_observation_policy_checks_freshness_at_the_owner_decision_time() {
  let (program, selected) = observation_policy_input(DeterministicGateOutcome::Passed);
  let time = |t| octacity_server_domain::Timestamp::from_unix_millis(t).unwrap();
  assert_eq!(program.select_at(&selected, time(99)).unwrap(), key("continue"));
  assert_eq!(
    program.select_at(&selected, time(100)).unwrap(),
    key("rollback"),
    "expired deployment evidence cannot authorize continuation even after a passing Build"
  );
}

#[test]
fn rollout_strategies_are_operator_values_and_do_not_select_engine_branches() {
  for strategy in [
    "stage-only",
    "stage-human",
    "stage-evidence",
    "canary",
    "ab",
    "progressive",
    "immediate-low-risk",
    "plugin.custom-rollout",
  ] {
    let (work, admitted, input) = fixture_with_policy(|policy| {
      policy["strategy"] = serde_json::json!(strategy);
      if strategy == "immediate-low-risk" {
        policy["production_environments"] = serde_json::json!(["sandbox"]);
        policy["allow_immediate_low_risk"] = serde_json::json!(true);
      }
    });
    let plan = PromotionPlan::freeze(&admitted, &input).unwrap();
    plan.authorize(&work, &admitted, &input).unwrap();
    assert_eq!(plan.policy().strategy.as_str(), strategy);
    assert_eq!(
      FlowGateProgram::from_node(plan.criteria()).unwrap(),
      FlowGateProgram::new(vec![], key("observed")).unwrap()
    );
    assert_eq!(
      plan.policy().environments[&key("sandbox")],
      reference("environment.snapshot")
    );
  }
}

#[test]
fn immediate_production_cannot_use_a_model_claim_to_replace_the_admitted_risk() {
  for risk in [RiskClass::Low, RiskClass::Medium, RiskClass::High, RiskClass::Critical] {
    let mut wire = serde_json::to_value(crate::flow_test_support::work()).unwrap();
    wire["risk"] = serde_json::json!(risk);
    let work: WorkEnvelope = serde_json::from_value(wire).unwrap();
    let (work, admitted, _, input, _) =
      crate::flow_test_support::promotion_execution_fixture_for_work(work, None, |policy| {
        policy["production_environments"] = serde_json::json!(["pilot"]);
        policy["allow_immediate_low_risk"] = serde_json::json!(true);
      });
    let plan = PromotionPlan::freeze(&admitted, &input).unwrap();
    assert_eq!(plan.authorize(&work, &admitted, &input).is_ok(), risk == RiskClass::Low);
  }
}

#[test]
fn a_bounded_signal_cannot_invent_a_promotion_route_after_failed_evidence() {
  let (program, observation) = observation_policy_input(DeterministicGateOutcome::Failed);
  let baseline = program
    .select_at(
      &observation,
      octacity_server_domain::Timestamp::from_unix_millis(99).unwrap(),
    )
    .unwrap();
  assert_eq!(baseline, key("rollback"));
  let (_, _, _, input, attempt) = crate::flow_test_support::execution_fixture(FlowNodeKind::Reasoning);
  let request = crate::decision_signal_tests::provider_request_for_node(
    &(input.clone(), attempt.clone()),
    DecisionSignalPurpose::Routing,
    DecisionSignalMode::BoundedControl,
    DecisionSignalFallback::Deny,
    &[baseline.as_str(), "escalate"],
    attempt.budget(),
    600000,
    100000,
  );
  let signal = consume_routing_signal(
    DecisionSignalReceiptId::generate(),
    &request,
    crate::decision_signal_tests::observation(&request, "continue", 900000, 100000),
    baseline,
  )
  .unwrap();
  assert_eq!(signal.state(), DecisionSignalState::Invalid);
  assert_eq!(
    signal.consumption().final_disposition(),
    &DecisionSignalDisposition::Deny
  );
  assert_eq!(request.request().frozen_input_digest(), input.digest().unwrap());
  assert!(request.request().budget().max_attempts() > 0);
}
