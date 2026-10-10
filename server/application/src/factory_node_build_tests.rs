use crate::*;
use factory::*;
use octacity_server_factory as factory;
use std::sync::Arc;

use crate::factory_node_test_support::fixtures;
use crate::factory_node_test_support::{Builds, Outputs};
use fixtures::*;

fn time(value: i64) -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(value).unwrap()
}

#[test]
fn configured_node_accepts_only_retained_results_bound_to_its_actual_build_and_schema() {
  crate::factory_admission_tests::run_ready(async {
    for kind in [FlowNodeKind::BuildCommand, FlowNodeKind::Reasoning] {
      let (_, admitted, schema, input, node) = execution_fixture(kind);
      let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
      let builds = Arc::new(Builds::new());
      *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
      let mut document = crate::factory_node_test_support::document(
        &builds,
        &intent.profile().result_output,
        schema.reference(),
        serde_json::to_vec(&serde_json::json!({"count":9})).unwrap(),
        intent.profile().tool.clone(),
      );
      document.plugin = intent.profile().plugin.clone();
      let expected = document.record.identity().clone();
      let outputs = FactoryNodeBuildOutputs {
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
          output_bytes: expected.size_bytes,
          ..BudgetUsage::default()
        },
        verified_at: time(30),
        fresh_until: time(1000),
      };
      for altered in [0, 1, 2] {
        let mut substituted = outputs.clone();
        match altered {
          0 => substituted.execution.profile.model_or_tool = reference("other.model"),
          1 => substituted.execution.profile.task_digest = FactoryDigest::from_bytes([44; 32]),
          _ => substituted.execution.input_digest = FactoryDigest::from_bytes([45; 32]),
        }
        let adapter = FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(Some(substituted))));
        assert!(
          adapter
            .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(31))
            .await
            .is_err()
        );
      }
      let adapter = FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(Some(outputs))));
      let FactoryNodeBuildStep::Completed(done) = adapter
        .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(31))
        .await
        .unwrap()
      else {
        panic!("completion required")
      };
      assert_eq!(done.record.payload().value(), &serde_json::json!({"count":9}));
      assert_eq!(
        done.record.metadata().unwrap()["input_digest"],
        serde_json::json!(intent.input().digest().unwrap())
      );
      assert_eq!(done.completion.outcome().as_str(), "observed");
      assert_eq!(done.completion.output_digest(), done.record.digest().unwrap());
      assert_eq!(done.completion.usage().attempts, 0);
      assert_eq!(done.usage.attempts, 1);
      let proof = done.record.build_provenance().unwrap();
      assert_eq!(proof.outputs()[0].producer.build_id(), expected.build_id);
      assert_eq!(done.execution.build_id(), expected.build_id);
      assert_eq!(done.execution.attempt_id(), expected.attempt_id);
      assert!(done.execution.jobs().contains(&expected.job_id));
      assert_eq!(proof.outputs()[0].producer.attempt_id(), expected.attempt_id);
      assert_eq!(proof.outputs()[0].producer.job_id(), expected.job_id);
      assert_eq!(proof.outputs()[0].artifact.artifact_id(), expected.artifact_id);
      let successor = FactoryClaimOwnership::new(
        key("recovery.worker"),
        FactoryClaim::new(
          FactoryClaimFence::new(FactoryDigest::from_bytes([89; 32])),
          time(101),
          time(200),
        )
        .unwrap(),
      );
      let dispatched = builds.requests.lock().unwrap().len();
      let FactoryNodeBuildStep::Completed(recovered) = adapter
        .observe_or_dispatch(&intent, &admitted, &schema, successor, time(102))
        .await
        .expect("a successor must observe results published before the frozen deadline")
      else {
        panic!("retained completion required");
      };
      assert_eq!(recovered.record, done.record);
      assert_eq!(recovered.execution, done.execution);
      assert_eq!(builds.requests.lock().unwrap().len(), dispatched);
    }
  });
}
fn ownership(intent: &FlowBuildIntent) -> FactoryClaimOwnership {
  FactoryClaimOwnership::new(intent.node().owner().clone(), intent.node().claim())
}

#[test]
fn a_substituted_output_schema_is_rejected_before_creating_a_build() {
  crate::factory_admission_tests::run_ready(async {
    let (_, admitted, _schema, input, node) = execution_fixture(FlowNodeKind::Reasoning);
    let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
    let substituted = FlowDataSchema::new(key("different.result"), key("v1"), FlowValueSchema::Boolean).unwrap();
    let builds = Arc::new(Builds::new());
    let adapter = FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)));
    assert!(
      adapter
        .observe_or_dispatch(&intent, &admitted, &substituted, ownership(&intent), time(31))
        .await
        .is_err()
    );
    assert!(builds.requests.lock().unwrap().is_empty());
  });
}

#[test]
fn independently_verified_evidence_is_required_by_configuration_and_retained_with_the_result() {
  crate::factory_admission_tests::run_ready(async {
    let evidence_schema = FlowDataSchema::new(
      key("audit.evidence.schema"),
      key("v1"),
      FlowValueSchema::Object {
        fields: std::collections::BTreeMap::from([(
          key("verified"),
          FlowFieldSchema::required(FlowValueSchema::Boolean),
        )]),
      },
    )
    .unwrap();
    let requirement = EvidenceRequirement::new(
      key("audit.evidence"),
      EvidenceOutputKind::Report,
      evidence_schema.reference().clone(),
      reference("audit.verifier"),
      reference("verifier.plugin"),
    );
    let (_, admitted, schema, input, node) =
      execution_fixture_with_evidence(FlowNodeKind::Reasoning, vec![requirement.clone()]);
    let admitted = admitted
      .with_data_schemas(vec![schema.clone(), evidence_schema])
      .unwrap();
    let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
    let builds = Arc::new(Builds::new());
    *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
    let mut report = crate::factory_node_test_support::document(
      &builds,
      &intent.profile().result_output,
      schema.reference(),
      br#"{"count":9}"#.to_vec(),
      intent.profile().tool.clone(),
    );
    report.plugin = intent.profile().plugin.clone();
    let mut proof = crate::factory_node_test_support::document(
      &builds,
      requirement.kind(),
      requirement.schema(),
      br#"{"verified":true}"#.to_vec(),
      requirement.tool().clone(),
    );
    proof.plugin = requirement.plugin().clone();
    let outputs = |documents: Vec<FactoryNodeBuildOutputDocument>| FactoryNodeBuildOutputs {
      execution: FactoryNodeBuildExecutionFacts {
        profile: intent.profile().clone(),
        input_digest: intent.input().digest().unwrap(),
        permissions: intent.permissions().clone(),
        budget: intent.budget(),
        deadline: intent.node().deadline(),
      },
      usage: BudgetUsage {
        attempts: 1,
        output_bytes: documents.iter().map(|doc| doc.record.identity().size_bytes).sum(),
        ..BudgetUsage::default()
      },
      documents,
      verified_at: time(30),
      fresh_until: time(1000),
    };
    let adapter = FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(Some(outputs(vec![report.clone()])))));
    assert!(
      adapter
        .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(31))
        .await
        .is_err()
    );
    let mut forged = proof.clone();
    forged.tool = intent.profile().tool.clone();
    let adapter = FactoryNodeBuildAdapter::new(
      builds.clone(),
      Arc::new(Outputs(Some(outputs(vec![report.clone(), forged])))),
    );
    assert!(
      adapter
        .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(31))
        .await
        .is_err()
    );
    let adapter = FactoryNodeBuildAdapter::new(builds, Arc::new(Outputs(Some(outputs(vec![report, proof])))));
    let FactoryNodeBuildStep::Completed(done) = adapter
      .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(31))
      .await
      .unwrap()
    else {
      panic!("completion required")
    };
    let retained = done.record.build_provenance().unwrap().outputs();
    assert_eq!(retained.len(), 2);
    assert_eq!(retained[0].name.as_str(), "audit.evidence");
    assert_eq!(retained[0].producer.tool().identity().as_str(), "audit.verifier");
    assert_eq!(
      retained[0].data.as_ref().unwrap().payload().value(),
      &serde_json::json!({"verified":true})
    );
    assert_eq!(
      done.record.metadata().unwrap()["build"]["outputs"]["audit.evidence"]["data"]["payload"]["value"]["verified"],
      serde_json::json!(true)
    );
  });
}

#[test]
fn configured_node_dispatch_and_restart_use_the_same_ordinary_build_without_a_stage_type() {
  crate::factory_admission_tests::run_ready(async {
    for kind in [FlowNodeKind::BuildCommand, FlowNodeKind::Reasoning] {
      let (_, admitted, schema, input, node) = execution_fixture(kind);
      let intent = FlowBuildIntent::new(input.clone(), &admitted, node.clone(), BudgetUsage::default(), 7).unwrap();
      let builds = Arc::new(Builds::new());
      builds.lose_response.store(true, std::sync::atomic::Ordering::SeqCst);
      let adapter = FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)));
      assert!(
        adapter
          .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(31))
          .await
          .is_err()
      );
      let restored = FlowBuildIntent::restore(
        &serde_json::to_vec(&intent).unwrap(),
        &input,
        &admitted,
        &node,
        intent.operation_id().unwrap(),
      )
      .unwrap();
      let restarted = FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)));
      let FactoryNodeBuildStep::Waiting(link) = restarted
        .observe_or_dispatch(&restored, &admitted, &schema, ownership(&intent), time(32))
        .await
        .unwrap()
      else {
        panic!("waiting execution must retain its Build identity");
      };
      assert_eq!(link.operation_id(), intent.operation_id().unwrap());
      assert_eq!(link.node_attempt_id(), node.id());
      assert_eq!(link.jobs().len(), 1);
      let requests = builds.requests.lock().unwrap();
      assert_eq!(requests.len(), 2);
      assert_eq!(requests[0], requests[1]);
      assert_eq!(requests[0].operation_id, intent.operation_id().unwrap());
      let causality = requests[0].causality.node().unwrap();
      assert_eq!(causality.node_attempt_id, node.id());
      assert_eq!(causality.input_schema, *schema.reference());
      assert_eq!(causality.input, serde_json::to_vec(&input).unwrap());
      assert_eq!(causality.profile.node.as_str(), "accessibility_audit");
    }
  });
}

#[test]
fn expired_semantic_evidence_is_retained_for_configured_gates_to_dispose() {
  crate::factory_admission_tests::run_ready(async {
    let (_, admitted, schema, input, node) = execution_fixture(FlowNodeKind::Reasoning);
    let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
    let builds = Arc::new(Builds::new());
    *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
    let mut report = crate::factory_node_test_support::document(
      &builds,
      &intent.profile().result_output,
      schema.reference(),
      br#"{"count":9}"#.to_vec(),
      intent.profile().tool.clone(),
    );
    report.plugin = intent.profile().plugin.clone();
    let outputs = FactoryNodeBuildOutputs {
      execution: FactoryNodeBuildExecutionFacts {
        profile: intent.profile().clone(),
        input_digest: intent.input().digest().unwrap(),
        permissions: intent.permissions().clone(),
        budget: intent.budget(),
        deadline: intent.node().deadline(),
      },
      documents: vec![report],
      usage: BudgetUsage {
        attempts: 1,
        output_bytes: 11,
        ..Default::default()
      },
      verified_at: time(30),
      fresh_until: time(31),
    };
    let adapter = FactoryNodeBuildAdapter::new(builds, Arc::new(Outputs(Some(outputs))));
    let FactoryNodeBuildStep::Completed(done) = adapter
      .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(32))
      .await
      .unwrap()
    else {
      panic!("retained observation required");
    };
    assert_eq!(
      done.record.build_provenance().unwrap().outputs()[0].fresh_until,
      time(31)
    );
  });
}

#[test]
fn ordinary_retries_keep_actual_attempt_and_job_provenance_and_failures_do_not_select_outcomes() {
  crate::factory_admission_tests::run_ready(async {
    for kind in [FlowNodeKind::BuildCommand, FlowNodeKind::Reasoning] {
      let (_, admitted, schema, input, node) = execution_fixture(kind);
      let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
      let builds = Arc::new(Builds::new());
      *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
      let (attempt, job) = crate::factory_node_test_support::complete_ordinary_retry(&builds);
      let mut report = crate::factory_node_test_support::document(
        &builds,
        &intent.profile().result_output,
        schema.reference(),
        br#"{"count":9}"#.to_vec(),
        intent.profile().tool.clone(),
      );
      report.plugin = intent.profile().plugin.clone();
      let outputs = FactoryNodeBuildOutputs {
        execution: FactoryNodeBuildExecutionFacts {
          profile: intent.profile().clone(),
          input_digest: intent.input().digest().unwrap(),
          permissions: intent.permissions().clone(),
          budget: intent.budget(),
          deadline: intent.node().deadline(),
        },
        documents: vec![report],
        usage: BudgetUsage {
          attempts: 1,
          output_bytes: 11,
          ..Default::default()
        },
        verified_at: time(30),
        fresh_until: time(1000),
      };
      let adapter = FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(Some(outputs))));
      let FactoryNodeBuildStep::Completed(done) = adapter
        .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(31))
        .await
        .unwrap()
      else {
        panic!("successful retry must complete the original node operation");
      };
      assert_eq!(done.execution.attempt_id(), attempt);
      assert_eq!(done.execution.jobs(), &[job]);
      assert_eq!(
        done.record.build_provenance().unwrap().outputs()[0]
          .producer
          .attempt_id(),
        attempt
      );
      assert_eq!(
        done.record.build_provenance().unwrap().outputs()[0].producer.job_id(),
        job
      );
      *builds.latest.lock().unwrap() = None;
      for state in [
        octacity_server_orchestrator::BuildState::Failed,
        octacity_server_orchestrator::BuildState::Cancelled,
      ] {
        *builds.state.lock().unwrap() = state;
        assert!(
          matches!(adapter.observe_or_dispatch(&intent,&admitted,&schema,ownership(&intent),time(32)).await.unwrap(),FactoryNodeBuildStep::ExecutionStopped { state:actual,.. } if actual==state)
        );
      }
    }
  });
}
#[test]
fn ownership_and_frozen_deadlines_are_checked_before_any_ordinary_dispatch() {
  crate::factory_admission_tests::run_ready(async {
    let (_, admitted, schema, input, node) = execution_fixture(FlowNodeKind::Reasoning);
    let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
    let builds = Arc::new(Builds::new());
    let adapter = FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)));
    let foreign = FactoryClaimOwnership::new(
      key("foreign"),
      FactoryClaim::new(
        FactoryClaimFence::new(FactoryDigest::from_bytes([88; 32])),
        time(10),
        time(100),
      )
      .unwrap(),
    );
    assert!(
      adapter
        .observe_or_dispatch(&intent, &admitted, &schema, foreign, time(31))
        .await
        .is_err()
    );
    assert!(
      adapter
        .observe_or_dispatch(
          &intent,
          &admitted,
          &schema,
          ownership(&intent),
          intent.node().deadline()
        )
        .await
        .is_err()
    );
    assert!(builds.requests.lock().unwrap().is_empty());
    let successor = FactoryClaimOwnership::new(
      key("successor"),
      FactoryClaim::new(
        FactoryClaimFence::new(FactoryDigest::from_bytes([89; 32])),
        time(101),
        time(200),
      )
      .unwrap(),
    );
    assert!(
      adapter
        .observe_or_dispatch(&intent, &admitted, &schema, successor, time(101))
        .await
        .is_err(),
      "owner takeover cannot extend the frozen execution deadline"
    );
    assert!(builds.requests.lock().unwrap().is_empty());
  });
}
