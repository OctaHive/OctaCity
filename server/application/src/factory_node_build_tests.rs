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
fn ordinary_verification_build_receives_the_exact_admitted_plan_without_implementation_context() {
  crate::factory_admission_tests::run_ready(async {
    for kind in [FlowNodeKind::BuildCommand, FlowNodeKind::Reasoning] {
      let (work, admitted, schema, input, node) = verification_execution_fixture(kind);
      let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
      let builds = Arc::new(Builds::new());
      FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)))
        .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(31))
        .await
        .unwrap();
      let requests = builds.requests.lock().unwrap();
      let received = requests[0]
        .causality
        .verification_check
        .as_ref()
        .expect("verification plan must reach trusted task compilation");
      assert_eq!(Some(received), intent.verification_check());
      assert_eq!(received.target().public_contract, work.artifacts().acceptance().clone());
      assert_eq!(requests[0].causality.context.entries().len(), 1);
      assert!(requests[0].causality.action.is_none());
      assert!(requests[0].causality.action_input.is_none());
      assert!(requests[0].effective_permissions.mounts().next().is_none());
      assert!(requests[0].effective_permissions.secret_profiles().next().is_none());
    }
  });
}

#[test]
fn verification_endpoint_uses_the_existing_canonical_ipv6_host_grant() {
  crate::factory_admission_tests::run_ready(async {
    let (_, admitted, schema, input, node) =
      verification_execution_fixture_with_endpoint_host(FlowNodeKind::BuildCommand, "http://[::1]", "::1");
    let intent = FlowBuildIntent::new(input, &admitted, node, Default::default(), 0).unwrap();
    let builds = Arc::new(Builds::new());
    let _ = FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)))
      .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(31))
      .await;
    assert_eq!(
      builds.requests.lock().unwrap().len(),
      1,
      "URL brackets must not discard a declared canonical IP grant"
    );
  });
}

#[test]
fn verification_dispatch_rejects_endpoint_substitution_and_embedded_credentials() {
  crate::factory_admission_tests::run_ready(async {
    for endpoint in [
      "https://other.example.test",
      "https://user:secret@stage.example.test",
      "ftp://stage.example.test",
      "https://stage.example.test/#fragment",
    ] {
      let (_, admitted, schema, input, node) =
        verification_execution_fixture_with_endpoint(FlowNodeKind::BuildCommand, endpoint);
      let intent = FlowBuildIntent::new(input, &admitted, node, Default::default(), 0).unwrap();
      let builds = Arc::new(Builds::new());
      let _ = FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)))
        .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(31))
        .await;
      assert!(
        builds.requests.lock().unwrap().is_empty(),
        "an invalid endpoint must never reach ordinary Build dispatch"
      );
    }
  });
}

#[test]
fn black_box_build_rejects_source_and_other_undeclared_mounts_before_dispatch() {
  crate::factory_admission_tests::run_ready(async {
    for root in ["/workspace/source", "/workspace", "/secrets", "/octacity/protected"] {
      let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
        mounts: vec![MountPermission::new(
          FactoryPath::new(root).unwrap(),
          MountMode::ReadWrite,
        )],
        ..Default::default()
      })
      .unwrap();
      let (_, admitted, schema, input, node) =
        verification_execution_fixture_with_permissions(FlowNodeKind::BuildCommand, permissions);
      let intent = FlowBuildIntent::new(input, &admitted, node, Default::default(), 0).unwrap();
      let builds = Arc::new(Builds::new());
      assert!(
        FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)))
          .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(31))
          .await
          .is_err(),
        "black-box dispatch must reject {root}"
      );
      assert!(builds.requests.lock().unwrap().is_empty());
    }
  });
}

#[test]
fn dispatch_retains_the_current_observer_claim_separately_from_the_frozen_intent() {
  crate::factory_admission_tests::run_ready(async {
    let (_, admitted, schema, input, node) = execution_fixture(FlowNodeKind::BuildCommand);
    let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
    let current = FactoryClaimOwnership::new(
      intent.node().owner().clone(),
      FactoryClaim::new(
        FactoryClaimFence::new(FactoryDigest::from_bytes([55; 32])),
        time(31),
        time(100),
      )
      .unwrap(),
    );
    let builds = Arc::new(Builds::new());
    FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)))
      .observe_or_dispatch(&intent, &admitted, &schema, current.clone(), time(32))
      .await
      .unwrap();
    let requests = builds.requests.lock().unwrap();
    assert_eq!(requests[0].ownership, current);
    assert_eq!(requests[0].operation_id, intent.operation_id().unwrap());
  });
}

#[test]
fn cancelling_a_configured_build_never_dispatches_a_missing_operation_or_selects_an_outcome() {
  crate::factory_admission_tests::run_ready(async {
    let (_, admitted, schema, input, node) = execution_fixture(FlowNodeKind::BuildCommand);
    let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
    let builds = Arc::new(Builds::new());
    let adapter = FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)));
    assert_eq!(
      adapter
        .cancel(&intent, &admitted, ownership(&intent), time(31), time(31))
        .await
        .unwrap(),
      FactoryNodeBuildCancellation::Stopped { execution: None }
    );
    assert!(builds.requests.lock().unwrap().is_empty());
    adapter
      .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(31))
      .await
      .unwrap();
    let unrelated = FactoryClaimOwnership::new(
      key("unrelated.owner"),
      FactoryClaim::new(
        FactoryClaimFence::new(FactoryDigest::from_bytes([88; 32])),
        time(30),
        time(90),
      )
      .unwrap(),
    );
    assert!(
      adapter
        .cancel(&intent, &admitted, unrelated, time(32), time(32))
        .await
        .is_err()
    );
    assert_eq!(
      *builds.state.lock().unwrap(),
      octacity_server_orchestrator::BuildState::Queued
    );
    assert!(matches!(
      adapter
        .cancel(&intent, &admitted, ownership(&intent), time(32), time(32))
        .await
        .unwrap(),
      FactoryNodeBuildCancellation::Waiting(_)
    ));
    let restarted = FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)));
    let FactoryNodeBuildCancellation::Stopped {
      execution: Some(execution),
    } = restarted
      .cancel(&intent, &admitted, ownership(&intent), time(32), time(33))
      .await
      .unwrap()
    else {
      panic!("authoritative cancellation must retain the exact ordinary execution");
    };
    assert_eq!(execution.node_attempt_id(), intent.node().id());
    assert_eq!(execution.build_id(), builds.acceptance.build_id);
    assert_eq!(execution.operation_id(), intent.operation_id().unwrap());
    assert_eq!(builds.requests.lock().unwrap().len(), 1);
  });
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
      assert_eq!(
        requests.len(),
        1,
        "lost-response recovery observes the already accepted operation before creation"
      );
      assert_eq!(link.build_id(), builds.acceptance.build_id);
      assert_eq!(link.attempt_id(), builds.acceptance.attempt_id);
      assert_eq!(requests[0].operation_id, intent.operation_id().unwrap());
      let causality = requests[0].causality.as_ref();
      assert_eq!(causality.node_attempt_id, node.id());
      assert_eq!(causality.input_schema, *schema.reference());
      assert_eq!(causality.input, serde_json::to_vec(&input).unwrap());
      assert_eq!(causality.profile.node.as_str(), "accessibility_audit");
    }
  });
}

#[test]
fn a_factory_build_cannot_be_observed_under_another_node_operation_origin() {
  crate::factory_admission_tests::run_ready(async {
    let (_, admitted, schema, input, node) = execution_fixture(FlowNodeKind::Reasoning);
    let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
    let builds = Arc::new(Builds::new());
    let adapter = FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)));
    adapter
      .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(31))
      .await
      .unwrap();
    let mut foreign = octacity_server_store::FactoryBuildOrigin::from_intent(&intent).unwrap();
    foreign.operation_id = FactoryDigest::from_bytes([99; 32]);
    *builds.origin_override.lock().unwrap() = Some(octacity_server_store::BuildOrigin::Factory(foreign));
    assert!(
      adapter
        .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(32))
        .await
        .is_err()
    );
    assert_eq!(
      builds.requests.lock().unwrap().len(),
      1,
      "observation cannot redispatch"
    );
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

struct TerminalConsumption {
  build: octacity_server_domain::BuildId,
  attempt: octacity_server_domain::AttemptId,
  available: bool,
}
#[test]
fn failed_build_executor_requires_measured_consumption_without_provider_results() {
  crate::factory_admission_tests::run_ready(async {
    let (work, admitted, _, input, node) = execution_fixture(FlowNodeKind::BuildCommand);
    let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
    let builds = Arc::new(Builds::new());
    *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Failed;
    for available in [false, true] {
      let adapter = FactoryNodeBuildAdapter::new(
        builds.clone(),
        Arc::new(TerminalConsumption {
          build: builds.acceptance.build_id,
          attempt: builds.acceptance.attempt_id,
          available,
        }),
      );
      let observed = adapter
        .observe(FactoryNodeExecutionRequest {
          run_id: intent.input().factory_run_id(),
          admitted: &admitted,
          work: &work,
          input: intent.input(),
          attempt: intent.node(),
          build: Some(&intent),
          ownership: ownership(&intent),
          observed_at: time(31),
        })
        .await;
      if available {
        assert!(
          matches!(observed.unwrap(), FactoryNodeExecutionStep::ExecutionStopped { usage, .. } if usage.tokens == 23 && usage.elapsed_millis == 2)
        );
      } else {
        assert_eq!(
          observed.unwrap_err().classification(),
          ApplicationFailure::Unavailable,
          "missing failure usage must remain unavailable, never zero"
        );
      }
    }
    assert_eq!(builds.requests.lock().unwrap().len(), 1);
  });
}
#[test]
fn a_failed_build_keeps_its_execution_slot_until_actual_jobs_have_stopped() {
  use octacity_server_store::BuildQueryStore;
  crate::factory_admission_tests::run_ready(async {
    let (work, admitted, schema, input, node) = execution_fixture(FlowNodeKind::BuildCommand);
    let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
    let builds = Arc::new(Builds::new());
    let adapter = FactoryNodeBuildAdapter::new(
      builds.clone(),
      Arc::new(TerminalConsumption {
        build: builds.acceptance.build_id,
        attempt: builds.acceptance.attempt_id,
        available: true,
      }),
    );
    adapter
      .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(31))
      .await
      .unwrap();
    *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Failed;
    let mut terminal = builds.latest_attempt(builds.acceptance.build_id).await.unwrap();
    terminal.jobs[0].state = octacity_server_job::JobState::Running;
    *builds.latest.lock().unwrap() = Some(terminal);
    let request = |at| FactoryNodeExecutionRequest {
      run_id: intent.input().factory_run_id(),
      work: &work,
      admitted: &admitted,
      input: intent.input(),
      attempt: intent.node(),
      build: Some(&intent),
      ownership: ownership(&intent),
      observed_at: time(at),
    };
    assert!(matches!(
      adapter.observe(request(32)).await.unwrap(),
      FactoryNodeExecutionStep::Waiting { execution: Some(_) }
    ));
    builds.latest.lock().unwrap().as_mut().unwrap().jobs[0].state = octacity_server_job::JobState::Failed;
    assert!(
      matches!(adapter.observe(request(33)).await.unwrap(), FactoryNodeExecutionStep::ExecutionStopped { usage, .. } if usage.tokens == 23)
    );
    assert_eq!(builds.requests.lock().unwrap().len(), 1);
  });
}
#[async_trait::async_trait]
impl FactoryNodeBuildOutputSource for TerminalConsumption {
  async fn terminal_usage(
    &self,
    build: octacity_server_domain::BuildId,
    attempt: octacity_server_domain::AttemptId,
  ) -> Result<BudgetUsage, OrdinaryBuildApplicationError> {
    assert_eq!((build, attempt), (self.build, self.attempt));
    if !self.available {
      return Err(OrdinaryBuildApplicationError::Unavailable);
    }
    Ok(BudgetUsage {
      tokens: 23,
      elapsed_millis: 2,
      ..Default::default()
    })
  }
  async fn published_outputs(
    &self,
    _: octacity_server_domain::BuildId,
    _: octacity_server_domain::AttemptId,
    _: u64,
  ) -> Result<FactoryNodeBuildOutputs, OrdinaryBuildApplicationError> {
    panic!("technical cancellation cannot parse provider results")
  }
}
#[test]
fn node_build_executor_cancellation_requires_actual_terminal_consumption_without_parsing_results() {
  crate::factory_admission_tests::run_ready(async {
    let (work, admitted, schema, input, node) = execution_fixture(FlowNodeKind::BuildCommand);
    let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
    let builds = Arc::new(Builds::new());
    let adapter = FactoryNodeBuildAdapter::new(
      builds.clone(),
      Arc::new(TerminalConsumption {
        build: builds.acceptance.build_id,
        attempt: builds.acceptance.attempt_id,
        available: true,
      }),
    );
    let request = |at| FactoryNodeExecutionRequest {
      run_id: intent.input().factory_run_id(),
      work: &work,
      admitted: &admitted,
      input: intent.input(),
      attempt: intent.node(),
      build: Some(&intent),
      ownership: ownership(&intent),
      observed_at: time(at),
    };
    assert_eq!(
      FactoryNodeExecutor::cancel(&adapter, request(31), time(31))
        .await
        .unwrap(),
      FactoryNodeCancellationStep::Stopped {
        execution: None,
        usage: BudgetUsage::default()
      }
    );
    adapter
      .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(31))
      .await
      .unwrap();
    assert!(matches!(
      FactoryNodeExecutor::cancel(&adapter, request(32), time(32))
        .await
        .unwrap(),
      FactoryNodeCancellationStep::Waiting { .. }
    ));
    let unavailable = FactoryNodeBuildAdapter::new(
      builds.clone(),
      Arc::new(TerminalConsumption {
        build: builds.acceptance.build_id,
        attempt: builds.acceptance.attempt_id,
        available: false,
      }),
    );
    assert!(
      FactoryNodeExecutor::cancel(&unavailable, request(33), time(32))
        .await
        .is_err(),
      "missing measurement cannot produce a stop receipt with invented zero consumption"
    );
    let FactoryNodeCancellationStep::Stopped {
      execution: Some(execution),
      usage,
    } = FactoryNodeExecutor::cancel(&adapter, request(33), time(32))
      .await
      .unwrap()
    else {
      panic!("actual terminal execution required")
    };
    assert_eq!(usage.tokens, 23);
    assert_eq!(execution.build_id(), builds.acceptance.build_id);
    assert_eq!(builds.requests.lock().unwrap().len(), 1);
  });
}

#[test]
fn cancellation_waits_until_the_actual_jobs_stop_even_when_the_build_and_attempt_are_terminal() {
  crate::factory_admission_tests::run_ready(async {
    let (_, admitted, schema, input, node) = execution_fixture(FlowNodeKind::BuildCommand);
    let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
    let builds = Arc::new(Builds::new());
    let adapter = FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)));
    adapter
      .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(31))
      .await
      .unwrap();
    crate::factory_node_test_support::complete_ordinary_retry(&builds);
    *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Cancelled;
    {
      let mut latest = builds.latest.lock().unwrap();
      let attempt = latest.as_mut().unwrap();
      attempt.state = octacity_server_orchestrator::AttemptState::Cancelled;
      attempt.jobs[0].state = octacity_server_job::JobState::Cancelling;
    }
    assert!(
      matches!(
        adapter
          .cancel(&intent, &admitted, ownership(&intent), time(32), time(32))
          .await
          .unwrap(),
        FactoryNodeBuildCancellation::Waiting(_)
      ),
      "terminal aggregate state cannot stand in for stopped processes"
    );
    builds.latest.lock().unwrap().as_mut().unwrap().jobs[0].state = octacity_server_job::JobState::Cancelled;
    assert!(matches!(
      adapter
        .cancel(&intent, &admitted, ownership(&intent), time(32), time(33))
        .await
        .unwrap(),
      FactoryNodeBuildCancellation::Stopped { execution: Some(_) }
    ));
  });
}

#[test]
fn cancellation_also_waits_for_still_stopping_jobs_in_previous_ordinary_attempts() {
  crate::factory_admission_tests::run_ready(async {
    let (_, admitted, schema, input, node) = execution_fixture(FlowNodeKind::BuildCommand);
    let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
    let builds = Arc::new(Builds::new());
    let adapter = FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)));
    adapter
      .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(31))
      .await
      .unwrap();
    crate::factory_node_test_support::complete_ordinary_retry(&builds);
    *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Cancelled;
    {
      let mut latest = builds.latest.lock().unwrap();
      let attempt = latest.as_mut().unwrap();
      attempt.state = octacity_server_orchestrator::AttemptState::Cancelled;
      attempt.jobs[0].state = octacity_server_job::JobState::Cancelled;
    }
    builds.previous.lock().unwrap()[0].jobs[0].state = octacity_server_job::JobState::Cancelling;
    assert!(
      matches!(
        adapter
          .cancel(&intent, &admitted, ownership(&intent), time(32), time(32))
          .await
          .unwrap(),
        FactoryNodeBuildCancellation::Waiting(_)
      ),
      "a retry does not prove that earlier processes stopped"
    );
    builds.previous.lock().unwrap()[0].jobs[0].state = octacity_server_job::JobState::Cancelled;
    assert!(matches!(
      adapter
        .cancel(&intent, &admitted, ownership(&intent), time(32), time(33))
        .await
        .unwrap(),
      FactoryNodeBuildCancellation::Stopped { execution: Some(_) }
    ));
  });
}

#[test]
fn a_configured_json_artifact_can_supply_the_node_result_without_becoming_a_report() {
  crate::factory_admission_tests::run_ready(async {
    let (_, admitted, schema, input, node) =
      execution_fixture_with_output_kind(FlowNodeKind::BuildCommand, EvidenceOutputKind::Artifact);
    let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
    let builds = Arc::new(Builds::new());
    *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
    let mut document = crate::factory_node_test_support::document_kind(
      &builds,
      &intent.profile().result_output,
      schema.reference(),
      br#"{"count":9}"#.to_vec(),
      intent.profile().tool.clone(),
      EvidenceOutputKind::Artifact,
    );
    document.plugin = intent.profile().plugin.clone();
    let artifact = document.record.identity().artifact_id;
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
        output_bytes: 11,
        ..Default::default()
      },
      verified_at: time(30),
      fresh_until: time(1000),
    };
    let adapter = FactoryNodeBuildAdapter::new(builds, Arc::new(Outputs(Some(outputs))));
    let FactoryNodeBuildStep::Completed(done) = adapter
      .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(31))
      .await
      .unwrap()
    else {
      panic!("the declared JSON artifact must complete the node");
    };
    assert_eq!(done.record.payload().value(), &serde_json::json!({"count":9}));
    let source = &done.record.build_provenance().unwrap().outputs()[0];
    assert_eq!(source.artifact.artifact_id(), artifact);
    assert_eq!(source.output_kind, EvidenceOutputKind::Artifact);
  });
}

#[test]
fn capability_observation_is_bound_to_actual_common_execution_without_a_stage_projection() {
  use octacity_server_store::BuildQueryStore;
  crate::factory_admission_tests::run_ready(async {
    let (_, admitted, schema, input, node) = execution_fixture(FlowNodeKind::BuildCommand);
    let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
    let builds = Arc::new(Builds::new());
    let adapter = FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)));
    let FactoryNodeBuildStep::Waiting(execution) = adapter
      .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(31))
      .await
      .unwrap()
    else {
      panic!("a newly accepted queued Build must retain its common operation identity");
    };
    *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
    let build = builds.build(execution.build_id()).await.unwrap();
    let attempt = builds.attempt(execution.attempt_id()).await.unwrap();
    let document = crate::factory_node_test_support::document(
      &builds,
      &intent.profile().result_output,
      schema.reference(),
      br#"{"count":9}"#.to_vec(),
      intent.profile().tool.clone(),
    );
    let outputs = vec![document.record.identity().clone()];
    let observation =
      FactoryBuildObservation::from_execution(&execution, &build, &attempt, outputs.clone(), time(32)).unwrap();
    assert_eq!(observation.build_id(), execution.build_id());
    assert_eq!(observation.attempt_id(), execution.attempt_id());
    assert_eq!(observation.job_ids(), execution.jobs());
    assert_eq!(observation.state(), octacity_server_orchestrator::BuildState::Succeeded);
    assert_eq!(observation.outputs(), outputs);
    let mut foreign = attempt.clone();
    foreign.build_id = octacity_server_domain::BuildId::generate();
    assert!(FactoryBuildObservation::from_execution(&execution, &build, &foreign, outputs.clone(), time(32)).is_err());
    let mut stopping = attempt.clone();
    stopping.jobs[0].state = octacity_server_job::JobState::Running;
    assert!(FactoryBuildObservation::from_execution(&execution, &build, &stopping, outputs.clone(), time(32)).is_err());
    let mut foreign_output = outputs[0].clone();
    foreign_output.job_id = octacity_server_domain::JobId::generate();
    assert!(
      FactoryBuildObservation::from_execution(&execution, &build, &attempt, vec![foreign_output], time(32)).is_err()
    );
    assert!(FactoryBuildObservation::from_execution(&execution, &build, &attempt, outputs, time(29)).is_err());
  });
}

#[test]
fn configured_specification_review_dispatches_only_its_frozen_candidate_and_reviewer_context() {
  crate::factory_admission_tests::run_ready(async {
    use std::collections::BTreeMap;
    let work = work();
    let template: FlowDefinitionTemplate = crate::factory_node_test_support::fixtures::supplied_base_flow_template();
    let mut selection = binding(work.subject().project_id());
    selection.model_or_tool = reference("reviewer.parameters");
    selection.task_digest = FactoryDigest::from_bytes([84; 32]);
    selection.result_output = key("specification_review");
    let profiles = template
      .nodes
      .iter()
      .filter_map(|node| node.build_profile.as_ref())
      .map(|name| (name.clone(), selection.clone()))
      .collect::<BTreeMap<_, _>>();
    let published = template
      .instantiate(FlowDefinitionId::generate(), FlowDefinitionVersion::INITIAL, &profiles)
      .unwrap();
    let reviewer = published.definition.node(&key("specification_review")).unwrap();
    let input_schema = published
      .schemas
      .iter()
      .find(|schema| Some(schema.reference()) == reviewer.input_schema())
      .unwrap();
    let result_schema = published
      .schemas
      .iter()
      .find(|schema| schema.reference() == reviewer.outcomes()[0].schema())
      .unwrap();
    let manifest = FactoryArtifactReference::new(
      octacity_server_domain::ArtifactId::generate(),
      FactoryDigest::from_bytes([81; 32]),
      200,
    )
    .unwrap();
    let bundle = FactoryArtifactReference::new(
      octacity_server_domain::ArtifactId::generate(),
      FactoryDigest::from_bytes([82; 32]),
      400,
    )
    .unwrap();
    let skills = FactoryArtifactReference::new(
      octacity_server_domain::ArtifactId::generate(),
      FactoryDigest::from_bytes([83; 32]),
      100,
    )
    .unwrap();
    // Publication adds a required exact reviewer-owned document to the closed contract.
    let mut wire = serde_json::to_value(reviewer).unwrap();
    wire["context_binding"]["entries"]["reviewer_skills"] = serde_json::json!({"source_kind":"policy","source":{"value":{"mapping":"constant","value":{"kind":"artifact","value":skills}}}});
    let reviewer: FlowNodeDefinition = serde_json::from_value(wire).unwrap();
    let definition = FlowDefinition::new(FlowDefinitionInput {
      id: FlowDefinitionId::generate(),
      version: FlowDefinitionVersion::INITIAL,
      input_schema: reviewer.input_schema().cloned(),
      entry: reviewer.key().clone(),
      nodes: vec![reviewer.clone()],
      transitions: vec![FlowTransition::new(
        reviewer.key().clone(),
        key("observed"),
        FlowTransitionTarget::Terminal(key("review_observed")),
      )],
      terminals: vec![FlowTerminalDefinition::new(
        key("review_observed"),
        result_schema.reference().clone(),
      )],
      context_projections: vec![],
      data_projections: vec![],
      execution: template.execution.clone(),
    })
    .unwrap();
    let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
    let flow = FlowRun::root(&run, definition.reference()).unwrap();
    let cycle = WorkflowCycle::initial(&flow).unwrap();
    let admitted = AdmittedFlow::new(
      PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition.clone()]).unwrap(),
      FlowAdmissionLimits::product_defaults(4, template.execution.budget(), FactoryPermissionSet::deny_all()).unwrap(),
      flow,
      cycle,
      FactoryWipLimits::new(8, 16).unwrap(),
    )
    .unwrap()
    .with_data_schemas(published.schemas.clone())
    .unwrap();
    let request = serde_json::json!({"work_id":work.id().to_string(),"original_base":work.subject().base_revision().as_str(),"candidate_revision":"spec-candidate","manifest_digest":manifest.content_digest(),"bundle_digest":bundle.content_digest()});
    let payload = serde_json::json!({"request":request,"original_acceptance":work.artifacts().acceptance(),"artifacts":{"manifest":manifest,"bundle":bundle},"acceptance_criteria":["specified behavior has a reproducible acceptance check"],"regression_criteria":[],"declared_requirements":[]});
    let subject = FactoryTaskSubject::Candidate(CandidateSubject::new(
      work.subject().clone(),
      octacity_server_domain::ImmutableRevision::new("spec-candidate").unwrap(),
      FactoryDigest::from_bytes([85; 32]),
    ));
    let entries = [
      ("manifest", ContextSourceKind::Specification, manifest),
      ("bundle", ContextSourceKind::Evidence, bundle),
      ("reviewer_skills", ContextSourceKind::Policy, skills),
      (
        "original_acceptance",
        ContextSourceKind::Task,
        work.artifacts().acceptance().clone(),
      ),
    ]
    .into_iter()
    .map(|(name, kind, artifact)| {
      ContextManifestEntry::new(
        kind,
        key(name),
        subject.clone(),
        FactoryContextReference::Artifact(artifact.clone()),
        artifact.content_digest(),
        artifact.encoded_size(),
        FactorySafeText::new("Exact independent review input").unwrap(),
        FactoryDigest::from_bytes([86; 32]),
      )
      .unwrap()
    })
    .collect();
    let context = ContextManifest::new(
      ContextManifestId::generate(),
      subject,
      FactoryDigest::from_bytes([86; 32]),
      entries,
    )
    .unwrap();
    let input = FlowNodeInput::new(
      &work,
      &admitted,
      admitted.root_run(),
      admitted.initial_cycle(),
      reviewer.key().clone(),
      FlowPayload::new(input_schema, payload).unwrap(),
    )
    .unwrap()
    .with_context(context.clone(), vec![])
    .unwrap();
    let node = NodeAttempt::new(
      admitted.root_run(),
      admitted.initial_cycle(),
      &definition,
      NodeAttemptInput {
        id: NodeAttemptId::generate(),
        node_key: reviewer.key().clone(),
        node_kind: reviewer.kind(),
        number: NodeAttemptNumber::INITIAL,
        input_digest: input.digest().unwrap(),
        budget: reviewer.budget(),
        started_at: time(10),
        deadline: time(100),
        execution: NodeExecutionIdentity::External(selection.tool.clone()),
        ownership: FactoryClaimOwnership::new(
          key("review.owner"),
          FactoryClaim::new(
            FactoryClaimFence::new(FactoryDigest::from_bytes([87; 32])),
            time(10),
            time(100),
          )
          .unwrap(),
        ),
      },
    )
    .unwrap();
    let intent = FlowBuildIntent::new(input.clone(), &admitted, node, BudgetUsage::default(), 0).unwrap();
    let builds = Arc::new(Builds::new());
    let adapter = FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)));
    adapter
      .observe_or_dispatch(&intent, &admitted, result_schema, ownership(&intent), time(31))
      .await
      .unwrap();
    {
      let requests = builds.requests.lock().unwrap();
      let dispatched = &requests[0];
      assert_eq!(dispatched.causality.context, context);
      assert_eq!(
        dispatched.causality.profile.model_or_tool.identity().as_str(),
        "reviewer.parameters"
      );
      assert_eq!(
        dispatched.causality.profile.task_digest,
        FactoryDigest::from_bytes([84; 32])
      );
      let frozen: serde_json::Value = serde_json::from_slice(&dispatched.causality.input).unwrap();
      assert_eq!(
        frozen["payload"]["value"]["acceptance_criteria"][0],
        "specified behavior has a reproducible acceptance check"
      );
      assert_eq!(dispatched.causality.context.entries().len(), 4);
      assert!(
        dispatched.causality.context.entries().iter().all(
          |entry| entry.logical_identity() != &key("task") && entry.logical_identity() != &key("author_transcript")
        )
      );
    }
    let intent = FlowBuildIntent::restore(
      &serde_json::to_vec(&intent).unwrap(),
      &input,
      &admitted,
      intent.node(),
      intent.operation_id().unwrap(),
    )
    .unwrap();
    assert_eq!(intent.input().context(), Some(&context));
    *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
    let mut review = request.clone();
    review["input_digest"] = serde_json::json!(input.digest().unwrap());
    review["quality"] = serde_json::json!({"complete":"satisfied","consistent":"satisfied","feasible":"satisfied","testable":"satisfied","in_scope":"satisfied"});
    review["findings"] = serde_json::json!([]);
    let mut document = crate::factory_node_test_support::document(
      &builds,
      &selection.result_output,
      result_schema.reference(),
      serde_json::to_vec(&review).unwrap(),
      selection.tool.clone(),
    );
    document.plugin = selection.plugin.clone();
    let size = document.bytes.len() as u64;
    let outputs = FactoryNodeBuildOutputs {
      execution: FactoryNodeBuildExecutionFacts {
        profile: intent.profile().clone(),
        input_digest: input.digest().unwrap(),
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
      verified_at: time(30),
      fresh_until: time(1000),
    };
    let adapter = FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(Some(outputs))));
    let FactoryNodeBuildStep::Completed(done) = adapter
      .observe_or_dispatch(&intent, &admitted, result_schema, ownership(&intent), time(32))
      .await
      .unwrap()
    else {
      panic!("retained reviewer observations required")
    };
    assert_eq!(done.record.payload().value(), &review);
    assert_eq!(done.completion.outcome().as_str(), "observed");
    assert!(reviewer.accepted_work_outcomes().is_empty());
  });
}

#[test]
fn ordinary_build_receives_the_frozen_public_deliverable_contract() {
  crate::factory_admission_tests::run_ready(async {
    let work = work();
    let binding = crate::factory_node_capture_tests::deliverable_binding(binding(work.subject().project_id()));
    let (_, admitted, schema, input, node) =
      execution_fixture_for_work_with_binding(FlowNodeKind::Reasoning, work, binding, None, None);
    let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
    let builds = Arc::new(Builds::new());
    FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)))
      .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(31))
      .await
      .unwrap();
    let requests = builds.requests.lock().unwrap();
    let contract = requests[0]
      .causality
      .deliverable_contract
      .as_ref()
      .expect("task compilation must receive the exact public requirements, not only a policy selector");
    assert_eq!(contract.accepted_contract(), "a".repeat(64).parse().unwrap());
    assert_eq!(contract.subject(), intent.input().context().unwrap().subject());
    assert_eq!(
      contract.policy(),
      &intent.profile().capture_deliverables.as_ref().unwrap().policy
    );
    let wire = serde_json::to_string(contract).unwrap();
    assert!(!wire.contains("protected_inventory") && !wire.contains("author_transcript"));
  });
}

#[test]
fn ordinary_build_receives_the_exact_frozen_trusted_action_selection() {
  crate::factory_admission_tests::run_ready(async {
    let (_, admitted, schema, input, node) = promotion_execution_fixture();
    let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
    let mut configured = Builds::new();
    configured.acceptance.effective_policy_digest = intent.permissions().digest();
    let builds = Arc::new(configured);
    FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)))
      .observe_or_dispatch(&intent, &admitted, &schema, ownership(&intent), time(31))
      .await
      .unwrap();
    let requests = builds.requests.lock().unwrap();
    assert_eq!(requests[0].causality.action.as_ref(), intent.action());
    assert_eq!(
      requests[0]
        .causality
        .context
        .subject()
        .candidate()
        .unwrap()
        .candidate_revision()
        .as_str(),
      "accepted-candidate"
    );
  });
}
