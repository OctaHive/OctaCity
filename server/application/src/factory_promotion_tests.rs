use crate::factory_node_test_support::{Builds, Outputs, fixtures::*};
use crate::*;
use octacity_server_factory::*;
use std::sync::Arc;
fn time(t: i64) -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(t).unwrap()
}

#[test]
fn promotion_observes_the_same_ordinary_build_after_lost_response_and_restart() {
  crate::factory_admission_tests::run_ready(async {
    let (work, admitted, _, input, attempt) = promotion_execution_fixture();
    let intent = FlowBuildIntent::new(input, &admitted, attempt, Default::default(), 7).unwrap();
    let mut configured = Builds::new();
    configured.acceptance.effective_policy_digest = intent.permissions().digest();
    let builds = Arc::new(configured);
    builds.lose_response.store(true, std::sync::atomic::Ordering::SeqCst);
    for t in [31, 32, 33] {
      let adapter = FactoryPromotionBuildAdapter::new(
        reference("trusted.deployment"),
        FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(None))),
      );
      let step = adapter
        .observe(FactoryNodeExecutionRequest {
          run_id: input_run(&intent),
          work: &work,
          admitted: &admitted,
          input: intent.input(),
          attempt: intent.node(),
          build: Some(&intent),
          ownership: FactoryClaimOwnership::new(intent.node().owner().clone(), intent.node().claim()),
          observed_at: time(t),
        })
        .await;
      if t == 31 {
        assert!(step.is_err());
      } else {
        assert!(matches!(
          step.unwrap(),
          FactoryNodeExecutionStep::Waiting { execution: Some(_) }
        ));
      }
    }
    let requests = builds.requests.lock().unwrap();
    assert_eq!(
      requests.len(),
      1,
      "recovery observes the deployment system instead of deploying twice"
    );
    assert_eq!(requests[0].operation_id, intent.operation_id().unwrap());
    let plan = PromotionPlan::freeze(&admitted, intent.input()).unwrap();
    let public = requests[0]
      .causality
      .action_input
      .as_ref()
      .expect("the deployment system must receive the frozen plan without reading Factory storage");
    assert_eq!(public["plan_digest"], serde_json::json!(plan.digest().unwrap()));
    assert_eq!(public["plan"]["subject"], serde_json::json!(plan.subject()));
    assert_eq!(public["plan"]["policy"]["observation_window_millis"], 50);
    assert_eq!(public["plan"]["criteria"], serde_json::json!(plan.criteria()));
    assert!(requests[0].effective_permissions.secret_profiles().next().is_none());
    assert!(
      requests[0]
        .effective_permissions
        .workload_identity_profiles()
        .next()
        .is_none()
    );
  });
}
fn input_run(intent: &FlowBuildIntent) -> FactoryRunId {
  intent.input().factory_run_id()
}

fn outputs(
  builds: &Builds,
  intent: &FlowBuildIntent,
  schema: &FlowDataSchema,
  value: serde_json::Value,
) -> FactoryNodeBuildOutputs {
  outputs_at(builds, intent, schema, value, time(30))
}
fn outputs_at(
  builds: &Builds,
  intent: &FlowBuildIntent,
  schema: &FlowDataSchema,
  value: serde_json::Value,
  published_at: octacity_server_domain::Timestamp,
) -> FactoryNodeBuildOutputs {
  let bytes = serde_json::to_vec(&value).unwrap();
  let size = bytes.len() as u64;
  let mut document = crate::factory_node_test_support::document_at(
    builds,
    &intent.profile().result_output,
    schema.reference(),
    bytes,
    intent.profile().tool.clone(),
    published_at,
  );
  document.plugin = intent.profile().plugin.clone();
  FactoryNodeBuildOutputs {
    execution: FactoryNodeBuildExecutionFacts {
      profile: intent.profile().clone(),
      input_digest: intent.input().digest().unwrap(),
      permissions: intent.permissions().clone(),
      budget: intent.budget(),
      deadline: intent.node().deadline(),
    },
    documents: vec![document],
    usage: BudgetUsage {
      attempts: 1,
      output_bytes: size,
      ..Default::default()
    },
    verified_at: published_at,
    fresh_until: time(99),
  }
}
#[test]
fn trusted_action_build_completion_retains_actual_build_provenance() {
  crate::factory_admission_tests::run_ready(async {
    let (_, admitted, schema, input, attempt) = promotion_execution_fixture();
    let intent = FlowBuildIntent::new(input, &admitted, attempt, Default::default(), 7).unwrap();
    let mut configured = Builds::new();
    configured.acceptance.effective_policy_digest = intent.permissions().digest();
    let builds = Arc::new(configured);
    *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
    let out = outputs(&builds, &intent, &schema, serde_json::json!({"count":9}));
    let adapter = FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(Some(out))));
    let FactoryNodeBuildStep::Completed(done) = adapter
      .observe_or_dispatch(
        &intent,
        &admitted,
        &schema,
        FactoryClaimOwnership::new(intent.node().owner().clone(), intent.node().claim()),
        time(31),
      )
      .await
      .unwrap()
    else {
      panic!("ordinary deployment Build must complete");
    };
    assert_eq!(
      done.record.build_provenance().unwrap().outputs()[0].producer.build_id(),
      builds.acceptance.build_id
    );
  });
}

fn shape(value: &serde_json::Value) -> FlowValueSchema {
  match value {
    serde_json::Value::Object(fields) => FlowValueSchema::Object {
      fields: fields
        .iter()
        .map(|(key_, value)| (key(key_), FlowFieldSchema::required(shape(value))))
        .collect(),
    },
    serde_json::Value::String(_) => FlowValueSchema::String {
      min_bytes: 1,
      max_bytes: 1024,
    },
    serde_json::Value::Number(_) => FlowValueSchema::Integer {
      minimum: 0,
      maximum: i64::MAX,
    },
    _ => panic!("receipt fixture uses closed objects, exact strings and integers"),
  }
}
fn observation(plan: &PromotionPlan, input: &FlowNodeInput) -> PromotionObservation {
  PromotionObservation {
    plan_digest: plan.digest().unwrap(),
    input_digest: input.digest().unwrap(),
    subject: plan.subject().clone(),
    environment: key("pilot"),
    environment_contract: plan.policy().environments[&key("pilot")].clone(),
    deployment: reference("actual.deployment"),
    outcome: DeterministicGateOutcome::Passed,
    started_at: time(10),
    finished_at: time(60),
    measurements: std::collections::BTreeMap::from([(key("errors"), 0)]),
  }
}
#[test]
fn promotion_completion_rejects_a_substituted_or_partial_observation_before_commit() {
  crate::factory_admission_tests::run_ready(async {
    let (_, first, _, input, _) = promotion_execution_fixture();
    let sample = serde_json::to_value(observation(&PromotionPlan::freeze(&first, &input).unwrap(), &input)).unwrap();
    let schema = FlowDataSchema::new(key("deployment.receipt"), key("v1"), shape(&sample)).unwrap();
    let (work, admitted, schema, input, attempt) = promotion_execution_fixture_with_schema(Some(schema));
    let plan = PromotionPlan::freeze(&admitted, &input).unwrap();
    let intent = FlowBuildIntent::new(input, &admitted, attempt, Default::default(), 7).unwrap();
    for bad in 0..4 {
      let mut configured = Builds::new();
      configured.acceptance.effective_policy_digest = intent.permissions().digest();
      let builds = Arc::new(configured);
      *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
      let mut result = observation(&plan, intent.input());
      if bad == 0 {
        result.input_digest = FactoryDigest::from_bytes([0; 32]);
      }
      if bad == 2 {
        result.finished_at = time(59);
      }
      let mut out = outputs_at(
        &builds,
        &intent,
        &schema,
        serde_json::to_value(result).unwrap(),
        time(60),
      );
      if bad == 1 {
        out.fresh_until = time(60);
      }
      let adapter = FactoryPromotionBuildAdapter::new(
        reference("trusted.deployment"),
        FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(Some(out)))),
      );
      let step = adapter
        .observe(FactoryNodeExecutionRequest {
          run_id: input_run(&intent),
          work: &work,
          admitted: &admitted,
          input: intent.input(),
          attempt: intent.node(),
          build: Some(&intent),
          ownership: FactoryClaimOwnership::new(intent.node().owner().clone(), intent.node().claim()),
          observed_at: time(61),
        })
        .await;
      if bad != 3 {
        assert!(
          step.is_err(),
          "substitution, stale proof or partial window cannot qualify this deployment: case {bad}"
        );
      } else {
        assert!(
          matches!(step.unwrap(),FactoryNodeExecutionStep::Completed{record,..} if record.build_provenance().is_some())
        );
      }
    }
  });
}

#[test]
fn promotion_cannot_accept_measurements_after_the_retained_receipt_was_published() {
  crate::factory_admission_tests::run_ready(async {
    let (_, first, _, input, _) = promotion_execution_fixture();
    let sample = serde_json::to_value(observation(&PromotionPlan::freeze(&first, &input).unwrap(), &input)).unwrap();
    let schema = FlowDataSchema::new(key("deployment.receipt"), key("v1"), shape(&sample)).unwrap();
    let (work, admitted, schema, input, attempt) = promotion_execution_fixture_with_schema(Some(schema));
    let plan = PromotionPlan::freeze(&admitted, &input).unwrap();
    let intent = FlowBuildIntent::new(input, &admitted, attempt, Default::default(), 7).unwrap();
    for verified_at in [30, 60] {
      let mut configured = Builds::new();
      configured.acceptance.effective_policy_digest = intent.permissions().digest();
      let builds = Arc::new(configured);
      *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
      let mut out = outputs(
        &builds,
        &intent,
        &schema,
        serde_json::to_value(observation(&plan, intent.input())).unwrap(),
      );
      assert_eq!(out.documents[0].record.published_at(), Some(time(30)));
      out.verified_at = time(verified_at);
      let adapter = FactoryPromotionBuildAdapter::new(
        reference("trusted.deployment"),
        FactoryNodeBuildAdapter::new(builds, Arc::new(Outputs(Some(out)))),
      );
      assert!(
        adapter
          .observe(FactoryNodeExecutionRequest {
            run_id: input_run(&intent),
            work: &work,
            admitted: &admitted,
            input: intent.input(),
            attempt: intent.node(),
            build: Some(&intent),
            ownership: FactoryClaimOwnership::new(intent.node().owner().clone(), intent.node().claim()),
            observed_at: time(61),
          })
          .await
          .is_err(),
        "measurements ending at 60 cannot appear in immutable bytes published at 30, even when reverified later"
      );
    }
  });
}

#[test]
fn build_completion_charges_aggregate_usage_to_admission_instead_of_the_local_flow() {
  crate::factory_admission_tests::run_ready(async {
    let (work, old, schema, previous, original) = promotion_execution_fixture();
    let local = old
      .closure()
      .definition(previous.definition())
      .unwrap()
      .execution()
      .budget();
    let mut wire = serde_json::to_value(&old).unwrap();
    wire["limits"]["budget"]["max_attempts"] = serde_json::json!(local.max_attempts() + 10);
    let admitted: AdmittedFlow = serde_json::from_value(wire).unwrap();
    let input = FlowNodeInput::new(
      &work,
      &admitted,
      admitted.root_run(),
      admitted.initial_cycle(),
      previous.node().clone(),
      previous.payload().clone(),
    )
    .unwrap()
    .with_context(previous.context().unwrap().clone(), vec![])
    .unwrap();
    let definition = admitted.closure().definition(input.definition()).unwrap();
    let attempt = NodeAttempt::new(
      admitted.root_run(),
      admitted.initial_cycle(),
      definition,
      NodeAttemptInput {
        id: original.id(),
        node_key: input.node().clone(),
        node_kind: original.node_kind(),
        number: original.number(),
        input_digest: input.digest().unwrap(),
        budget: original.budget(),
        started_at: original.started_at(),
        deadline: original.deadline(),
        execution: original.execution().clone(),
        ownership: FactoryClaimOwnership::new(original.owner().clone(), original.claim()),
      },
    )
    .unwrap();
    let before = BudgetUsage {
      attempts: local.max_attempts() + 1,
      ..Default::default()
    };
    let intent = FlowBuildIntent::new(input, &admitted, attempt, before, 7).unwrap();
    let mut configured = Builds::new();
    configured.acceptance.effective_policy_digest = intent.permissions().digest();
    let builds = Arc::new(configured);
    *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
    let out = outputs(&builds, &intent, &schema, serde_json::json!({"count":9}));
    let adapter = FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(Some(out))));
    let FactoryNodeBuildStep::Completed(done) = adapter
      .observe_or_dispatch(
        &intent,
        &admitted,
        &schema,
        FactoryClaimOwnership::new(intent.node().owner().clone(), intent.node().claim()),
        time(31),
      )
      .await
      .expect("owning flow must not charge sibling consumption")
    else {
      panic!("accepted Build must complete")
    };
    assert_eq!(done.usage.attempts, before.attempts + 1);
    assert_eq!(
      done.record.build_provenance().unwrap().outputs()[0].producer.build_id(),
      builds.acceptance.build_id
    );
  });
}
