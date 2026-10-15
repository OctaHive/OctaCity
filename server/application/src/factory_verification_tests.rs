use crate::factory_node_test_support::{Builds, Outputs, fixtures::*};
use crate::*;
use octacity_server_domain::{ArtifactId, ImmutableRevision, Timestamp};
use octacity_server_factory::*;
use octacity_server_orchestrator::BuildState;
use std::{collections::BTreeMap, sync::Arc};

fn time(ms: i64) -> Timestamp {
  Timestamp::from_unix_millis(ms).unwrap()
}

#[test]
fn configured_checks_publish_independent_evidence_and_reasoning_cannot_override_failure() {
  check_composition(false);
}

#[test]
fn configured_checks_accept_later_own_evidence_without_replacing_the_plan() {
  check_composition(true);
}

fn check_composition(own_evidence: bool) {
  crate::factory_admission_tests::run_ready(async {
    for required_outcome in [DeterministicGateOutcome::Failed, DeterministicGateOutcome::Passed] {
      let work = work();
      let mut template = supplied_verification_flow_template();
      if own_evidence {
        let mut wire = serde_json::to_value(&template).unwrap();
        let mut schema = wire["schemas"]["verification_input"].clone();
        schema["identity"] = serde_json::json!("operator.evidence-check");
        schema["shape"]["fields"]["evidence"] = serde_json::json!({"required":true,"value":{"type":"array","min_items":1,"max_items":16,"item":{"type":"reference","schema":"artifact_reference"}}});
        wire["schemas"]["evidence_check_input"] = schema;
        let reviewer = wire["nodes"]
          .as_array_mut()
          .unwrap()
          .iter_mut()
          .find(|node| node["kind"] == "reasoning")
          .unwrap();
        reviewer["input_schema"] = serde_json::json!("evidence_check_input");
        reviewer["input_binding"] = serde_json::json!({"sources":{"incoming":{"source":"caller_input","path":""},"earlier":{"source":"record","node":"contract_sequence","outcome":"observed","view":"metadata","path":"/build/outputs"}},"mapping":{"value":{"mapping":"object","fields":{"target":{"mapping":"pointer","path":"/incoming/target"},"evidence":{"mapping":"array","items":[{"mapping":"pointer","path":"/earlier/audit.report/artifact"}]}}}}});
        reviewer["context_binding"]["artifact_collections"] = serde_json::json!({"selected-evidence":{"source_kind":"evidence","source":{"value":{"mapping":"pointer","path":"/evidence"}}}});
        let reviewer_key = reviewer["key"].as_str().unwrap().to_owned();
        wire["execution"]["verification"]["checks"][&reviewer_key]["target"]["value"] = serde_json::json!({"mapping":"object","fields":{"subject":{"mapping":"pointer","path":"/payload/value/target/subject"},"environment":{"mapping":"pointer","path":"/payload/value/target/environment"},"public_contract":{"mapping":"pointer","path":"/payload/value/target/public_contract"},"endpoint":{"mapping":"pointer","path":"/payload/value/target/endpoint"},"evidence_inputs":{"mapping":"pointer","path":"/payload/value/evidence"}}});
        template = serde_json::from_value(wire).unwrap();
      }
      let profiles = template
        .nodes
        .iter()
        .filter_map(|node| node.build_profile.clone())
        .map(|name| (name, binding(work.subject().project_id())))
        .collect();
      let published = template
        .instantiate(FlowDefinitionId::generate(), FlowDefinitionVersion::INITIAL, &profiles)
        .unwrap();
      let input_schema = published
        .schemas
        .iter()
        .find(|schema| schema.reference().identity().as_str() == "verification.input")
        .unwrap()
        .clone();
      let output_schema = published
        .schemas
        .iter()
        .find(|schema| schema.reference().identity().as_str() == "verification.observation")
        .unwrap()
        .clone();
      let budget = published.definition.execution().budget();
      let reference = published.definition.reference();
      let flow = FlowRun::root(&FactoryRun::admitted(FactoryRunId::generate(), &work), reference).unwrap();
      let cycle = WorkflowCycle::initial(&flow).unwrap();
      let admitted = AdmittedFlow::new(
        PinnedFlowDefinitionClosure::new(reference, vec![published.definition]).unwrap(),
        FlowAdmissionLimits::product_defaults(4, budget, FactoryPermissionSet::deny_all()).unwrap(),
        flow,
        cycle,
        FactoryWipLimits::new(8, 16).unwrap(),
      )
      .unwrap()
      .with_data_schemas(published.schemas)
      .unwrap();
      let subject = CandidateSubject::new(
        work.subject().clone(),
        ImmutableRevision::new("candidate").unwrap(),
        FactoryDigest::from_bytes([95; 32]),
      );
      let contract = work.artifacts().acceptance().clone();
      let task_subject = FactoryTaskSubject::Candidate(subject.clone());
      let context = ContextManifest::new(
        ContextManifestId::generate(),
        task_subject.clone(),
        FactoryDigest::from_bytes([9; 32]),
        vec![
          ContextManifestEntry::new(
            ContextSourceKind::Specification,
            key("public-contract"),
            task_subject,
            FactoryContextReference::Artifact(contract.clone()),
            contract.content_digest(),
            contract.encoded_size(),
            FactorySafeText::new("Selected public deployed contract").unwrap(),
            FactoryDigest::from_bytes([9; 32]),
          )
          .unwrap(),
        ],
      )
      .unwrap();
      let payload = FlowPayload::new(&input_schema,serde_json::json!({"target":{"subject":subject,"environment":crate::factory_node_test_support::fixtures::reference("stage-receipt"),"public_contract":contract,"endpoint":"https://stage.example.test"}})).unwrap();
      let definition = admitted.closure().definition(reference).unwrap();
      let mut reports: Vec<VerifiedFactoryVerificationCheck> = Vec::new();
      let mut plan = None;
      let mut candidate = None;
      let mut outcomes = BTreeMap::new();
      let required_failure = definition
        .execution()
        .verification()
        .unwrap()
        .checks
        .iter()
        .find(|(_, check)| check.required)
        .unwrap()
        .0
        .clone();
      for declared in definition.nodes().iter().filter(|node| node.build().is_some()) {
        let mut check_payload = payload.clone();
        let mut check_context = context.clone();
        if own_evidence && declared.kind() == FlowNodeKind::Reasoning {
          let evidence = reports.first().unwrap().evidence().items()[0].artifact().clone();
          let schema = admitted
            .data_schemas()
            .iter()
            .find(|schema| schema.reference().identity().as_str() == "operator.evidence-check")
            .unwrap();
          let mut value = payload.value().clone();
          value["evidence"] = serde_json::json!([evidence.clone()]);
          check_payload = FlowPayload::new(schema, value).unwrap();
          let mut entries = context.entries().to_vec();
          entries.push(
            ContextManifestEntry::new(
              ContextSourceKind::Evidence,
              key("selected-evidence.0"),
              context.subject().clone(),
              FactoryContextReference::Artifact(evidence.clone()),
              evidence.content_digest(),
              evidence.encoded_size(),
              FactorySafeText::new("Exact retained earlier check output").unwrap(),
              context.construction_policy_digest(),
            )
            .unwrap(),
          );
          check_context = ContextManifest::new(
            ContextManifestId::generate(),
            context.subject().clone(),
            context.construction_policy_digest(),
            entries,
          )
          .unwrap();
        }
        let input = FlowNodeInput::new(
          &work,
          &admitted,
          admitted.root_run(),
          admitted.initial_cycle(),
          declared.key().clone(),
          check_payload,
        )
        .unwrap()
        .with_context(check_context, vec![])
        .unwrap();
        let ownership = FactoryClaimOwnership::new(
          key("worker"),
          FactoryClaim::new(
            FactoryClaimFence::new(FactoryDigest::from_bytes([64; 32])),
            time(10),
            time(100),
          )
          .unwrap(),
        );
        let node = NodeAttempt::new(
          admitted.root_run(),
          admitted.initial_cycle(),
          definition,
          NodeAttemptInput {
            id: NodeAttemptId::generate(),
            node_key: declared.key().clone(),
            node_kind: declared.kind(),
            number: NodeAttemptNumber::INITIAL,
            input_digest: input.digest().unwrap(),
            budget: declared.budget(),
            started_at: time(10),
            deadline: time(100),
            execution: NodeExecutionIdentity::External(declared.build().unwrap().tool.clone()),
            ownership: ownership.clone(),
          },
        )
        .unwrap();
        let change_set = candidate.get_or_insert_with(|| {
          ChangeSet::new(
            ChangeSetId::generate(),
            &input,
            &node,
            subject.clone(),
            ArtifactId::generate(),
            ArtifactId::generate(),
          )
          .unwrap()
        });
        let intent = FlowBuildIntent::new(input, &admitted, node, Default::default(), 0).unwrap();
        let frozen = intent.verification_plan().unwrap();
        if let Some(plan) = &plan {
          assert_eq!(frozen, plan, "one immutable plan must span all check inputs");
        } else {
          plan = Some(frozen.clone());
        }
        let outcome = if declared.key() == &required_failure {
          required_outcome
        } else {
          DeterministicGateOutcome::Passed
        };
        outcomes.insert(declared.kind(), outcome);
        let report = VerificationObservation {
          plan_digest: frozen.digest().unwrap(),
          input_digest: intent.input().digest().unwrap(),
          check: declared.key().clone(),
          outcome,
          measurements: vec![],
        };
        let bytes = serde_json::to_vec(&report).unwrap();
        let builds = Arc::new(Builds::new());
        crate::factory_node_build_tests::admit_pending_build(&builds, &intent, &admitted, &output_schema).await;
        *builds.state.lock().unwrap() = BuildState::Succeeded;
        let mut document = crate::factory_node_test_support::document(
          &builds,
          &intent.profile().result_output,
          output_schema.reference(),
          bytes.clone(),
          intent.profile().tool.clone(),
        );
        document.plugin = intent.profile().plugin.clone();
        let outputs = FactoryNodeBuildOutputs {
          execution: FactoryNodeBuildExecutionFacts {
            profile: intent.profile().clone(),
            input_digest: intent.input().digest().unwrap(),
            permissions: intent.permissions().clone(),
            budget: intent.budget(),
            deadline: intent.node().deadline(),
          },
          documents: vec![document.clone()],
          usage: BudgetUsage {
            attempts: 1,
            output_bytes: bytes.len() as u64,
            ..Default::default()
          },
          verified_at: time(30),
          fresh_until: time(1000),
        };
        let FactoryNodeBuildStep::Completed(completed) =
          FactoryNodeBuildAdapter::new(builds, Arc::new(Outputs(Some(outputs))))
            .observe_or_dispatch(&intent, &admitted, &output_schema, ownership, time(31))
            .await
            .unwrap()
        else {
          panic!("the ordinary Build must complete");
        };
        reports.push(
          verify_factory_verification_check(&intent, change_set, &completed, &document, time(33), time(40)).unwrap(),
        );
        let target = intent.verification_check().unwrap().target();
        assert_eq!(
          reports.last().unwrap().evidence().inputs(),
          target.retained_inputs().unwrap()
        );
      }
      assert_eq!(reports.len(), 7);
      assert_eq!(outcomes[&FlowNodeKind::Reasoning], DeterministicGateOutcome::Passed);
      assert_eq!(
        evaluate_factory_verification(plan.as_ref().unwrap(), &reports, time(34)).unwrap(),
        required_outcome
      );
      assert_eq!(
        evaluate_factory_verification(plan.as_ref().unwrap(), &reports, time(41)).unwrap(),
        DeterministicGateOutcome::Indeterminate
      );
      reports.push(reports[0].clone());
      assert!(
        evaluate_factory_verification(plan.as_ref().unwrap(), &reports, time(34)).is_err(),
        "a duplicate producer cannot inflate quorum"
      );
    }
  });
}
