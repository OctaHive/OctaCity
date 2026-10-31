//! Ordinary Build evidence retains the selected step resource independently of Work.
use super::*;
use crate::factory_node_test_support::{Builds, Outputs, document};
use crate::*;
use octacity_server_orchestrator::BuildState;

#[test]
fn ordinary_build_evidence_uses_the_selected_candidate_without_a_work_repository() {
  run_ready(async {
    let fixture = candidate_fixture_with_build(None, true).await;
    let admitted = &fixture.snapshot.admitted_flow;
    let root = admitted.closure().definition(admitted.closure().root()).unwrap();
    let input = FlowNodeInput::prepare(fixture.preparation()).unwrap();
    let subject = input.task_subject().unwrap();
    let input = input
      .with_context(
        ContextManifest::new(ContextManifestId::generate(), subject, digest(140), vec![]).unwrap(),
        vec![],
      )
      .unwrap();
    let node = root.node(input.node()).unwrap();
    let owner = FactoryClaimOwnership::new(
      key("resource-validator"),
      FactoryClaim::new(FactoryClaimFence::new(digest(141)), time(30), time(100)).unwrap(),
    );
    let attempt = NodeAttempt::new(
      admitted.root_run(),
      admitted.initial_cycle(),
      root,
      NodeAttemptInput {
        id: NodeAttemptId::generate(),
        node_key: node.key().clone(),
        node_kind: node.kind(),
        number: NodeAttemptNumber::INITIAL,
        input_digest: input.digest().unwrap(),
        budget: node.budget(),
        started_at: time(30),
        deadline: time(100),
        execution: NodeExecutionIdentity::External(node.build().unwrap().tool.clone()),
        ownership: owner.clone(),
      },
    )
    .unwrap();
    let schema = admitted.data_schema(node.outcomes()[0].schema()).unwrap();
    let intent = FlowBuildIntent::new(input, admitted, attempt, BudgetUsage::default(), 0).unwrap();
    assert!(intent.input().subject().repository().is_err());
    let builds = Arc::new(Builds::new());
    *builds.state.lock().unwrap() = BuildState::Succeeded;
    let bytes = b"true".to_vec();
    let mut output = document(
      &builds,
      &intent.profile().result_output,
      schema.reference(),
      bytes.clone(),
      intent.profile().tool.clone(),
    );
    output.plugin = intent.profile().plugin.clone();
    let retained = output.record.clone();
    let outputs = FactoryNodeBuildOutputs {
      execution: FactoryNodeBuildExecutionFacts {
        profile: intent.profile().clone(),
        input_digest: intent.input().digest().unwrap(),
        permissions: intent.permissions().clone(),
        budget: intent.budget(),
        deadline: intent.node().deadline(),
      },
      documents: vec![output],
      usage: BudgetUsage {
        attempts: 1,
        output_bytes: 4,
        ..Default::default()
      },
      verified_at: time(32),
      fresh_until: time(100),
    };
    crate::factory_node_build_tests::admit_pending_build(&builds, &intent, admitted, schema).await;
    let FactoryNodeBuildStep::Completed(completed) =
      FactoryNodeBuildAdapter::new(builds, Arc::new(Outputs(Some(outputs))))
        .observe_or_dispatch(&intent, admitted, schema, owner, time(33))
        .await
        .unwrap()
    else {
      panic!("verified ordinary Build must complete");
    };
    let changes = ChangeSet::new(
      ChangeSetId::generate(),
      &fixture.inputs[0],
      &fixture.attempts[0],
      fixture.candidate.clone(),
      ArtifactId::generate(),
      ArtifactId::generate(),
    )
    .unwrap();
    let requirement = EvidenceRequirement::new(
      intent.profile().result_output.clone(),
      EvidenceOutputKind::Report,
      schema.reference().clone(),
      intent.profile().tool.clone(),
      intent.profile().plugin.clone(),
    );
    let verified = VerifiedFactoryEvidence::new(
      retained,
      &bytes,
      FactoryEvidenceAttestation::new(
        fixture.candidate.clone(),
        requirement.kind().clone(),
        schema.reference().clone(),
        intent.profile().tool.clone(),
        intent.profile().plugin.clone(),
        DeterministicGateOutcome::Passed,
        time(90),
      ),
    )
    .unwrap();
    let manifest = construct_factory_evidence_manifest(
      EvidenceManifestId::generate(),
      &changes,
      &completed,
      vec![requirement],
      vec![verified],
      time(34),
    );
    assert!(
      manifest.is_ok(),
      "evidence binds the exact selected resource: {manifest:?}"
    );
    assert_eq!(manifest.unwrap().subject(), &fixture.candidate);
  });
}
