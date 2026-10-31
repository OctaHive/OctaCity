use crate::flow_test_support::{execution_fixture, key, reference};
use crate::*;
use octacity_server_domain::ImmutableRevision;

fn two_node_verification_fixture(
  second_check: bool,
) -> (WorkEnvelope, AdmittedFlow, FlowDataSchema, FlowNodeInput, NodeAttempt) {
  let (work, prior, _, input, attempt) =
    crate::flow_test_support::verification_execution_fixture(FlowNodeKind::BuildCommand);
  let original = prior.closure().definition(input.definition()).unwrap();
  let schema = FlowDataSchema::new(
    key("auxiliary.input"),
    key("v1"),
    FlowValueSchema::String {
      min_bytes: 1,
      max_bytes: 64,
    },
  )
  .unwrap();
  let mut second = serde_json::to_value(&original.nodes()[0]).unwrap();
  second["key"] = serde_json::json!("operator-chosen-helper");
  second["input_schema"] = serde_json::to_value(schema.reference()).unwrap();
  second["input_binding"] = serde_json::json!({"sources":{"prior":{"source":"predecessor","view":"payload","path":""}},"mapping":{"value":{"mapping":"constant","value":"ordinary helper input"}}});
  let mut execution = serde_json::to_value(original.execution()).unwrap();
  if second_check {
    let mut check = execution["verification"]["checks"]["accessibility_audit"].clone();
    check["target"]["value"]["fields"]["public_contract"] =
      serde_json::json!({"mapping":"constant","value":work.artifacts().acceptance()});
    check["target"]["value"]["fields"]["evidence_inputs"] =
      serde_json::json!({"mapping":"array","items":[{"mapping":"pointer","path":"/context/entries/0/source/value"}]});
    execution["verification"]["checks"]["operator-chosen-helper"] = check;
  }
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: original.reference().id(),
    version: original.reference().version(),
    input_schema: original.input_schema().cloned(),
    entry: original.entry().clone(),
    nodes: vec![original.nodes()[0].clone(), serde_json::from_value(second).unwrap()],
    transitions: vec![
      FlowTransition::new(
        original.entry().clone(),
        original.nodes()[0].build().unwrap().result_outcome.clone(),
        FlowTransitionTarget::Node(key("operator-chosen-helper")),
      ),
      FlowTransition::new(
        key("operator-chosen-helper"),
        original.nodes()[0].build().unwrap().result_outcome.clone(),
        FlowTransitionTarget::Terminal(key("done")),
      ),
    ],
    terminals: original.terminals().to_vec(),
    context_projections: vec![],
    data_projections: vec![],
    execution: serde_json::from_value(execution).unwrap(),
  })
  .unwrap();
  let flow = FlowRun::root(
    &FactoryRun::admitted(prior.root_run().factory_run_id(), &work),
    definition.reference(),
  )
  .unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let mut schemas = prior.data_schemas().to_vec();
  schemas.push(schema.clone());
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    prior.limits().clone(),
    flow,
    cycle,
    prior.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(schemas)
  .unwrap();
  (work, admitted, schema, input, attempt)
}

#[test]
fn a_verification_flow_can_execute_an_ordinary_unselected_helper_build() {
  let (work, admitted, schema, original, previous) = two_node_verification_fixture(false);
  let input = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    key("operator-chosen-helper"),
    FlowPayload::new(&schema, serde_json::json!("ordinary helper input")).unwrap(),
  )
  .unwrap()
  .with_context(original.context().unwrap().clone(), vec![])
  .unwrap();
  let declared = admitted
    .closure()
    .definition(input.definition())
    .unwrap()
    .node(input.node())
    .unwrap();
  let attempt = NodeAttempt::new(
    admitted.root_run(),
    admitted.initial_cycle(),
    admitted.closure().definition(input.definition()).unwrap(),
    NodeAttemptInput {
      id: NodeAttemptId::generate(),
      node_key: input.node().clone(),
      node_kind: declared.kind(),
      number: NodeAttemptNumber::INITIAL,
      input_digest: input.digest().unwrap(),
      budget: declared.budget(),
      started_at: previous.started_at(),
      deadline: previous.deadline(),
      execution: previous.execution().clone(),
      ownership: FactoryClaimOwnership::new(previous.owner().clone(), previous.claim()),
    },
  )
  .unwrap();
  let intent = FlowBuildIntent::new(input, &admitted, attempt, Default::default(), 0)
    .expect("unselected helper keeps its ordinary input and output contract");
  assert!(intent.verification_plan().is_none());
}

#[test]
fn checks_with_distinct_inputs_and_later_evidence_share_one_immutable_plan() {
  let (work, admitted, schema, original, _) = two_node_verification_fixture(true);
  let first = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    original.node().clone(),
    original.payload().clone(),
  )
  .unwrap()
  .with_context(original.context().unwrap().clone(), vec![])
  .unwrap();
  let initial =
    VerificationPlan::for_input(&admitted, &first).expect("a check must not resolve another check's private input");
  let context = original.context().unwrap();
  let evidence = work.artifacts().task().clone();
  let mut entries = context.entries().to_vec();
  entries.push(
    ContextManifestEntry::new(
      ContextSourceKind::Evidence,
      key("verification-result"),
      context.subject().clone(),
      FactoryContextReference::Artifact(evidence.clone()),
      evidence.content_digest(),
      evidence.encoded_size(),
      FactorySafeText::new("Selected earlier evidence").unwrap(),
      FactoryDigest::from_bytes([8; 32]),
    )
    .unwrap(),
  );
  let second = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    key("operator-chosen-helper"),
    FlowPayload::new(&schema, serde_json::json!("ordinary helper input")).unwrap(),
  )
  .unwrap()
  .with_context(
    ContextManifest::new(
      ContextManifestId::generate(),
      context.subject().clone(),
      context.construction_policy_digest(),
      entries,
    )
    .unwrap(),
    vec![],
  )
  .unwrap();
  let following = VerificationPlan::for_input(&admitted, &second).unwrap();
  assert_eq!(
    initial, following,
    "closed node inputs and new evidence must not replace the admitted plan"
  );
  assert_eq!(initial.digest().unwrap(), following.digest().unwrap());
}

#[test]
fn verification_observation_cannot_be_reused_for_a_different_frozen_check_input() {
  let (_, admitted, _, input, _) = crate::flow_test_support::verification_execution_fixture(FlowNodeKind::BuildCommand);
  let selected = VerificationCheckInput::new(&admitted, &input).unwrap();
  let report = VerificationObservation {
    plan_digest: selected.plan().digest().unwrap(),
    input_digest: input.digest().unwrap(),
    check: input.node().clone(),
    outcome: DeterministicGateOutcome::Passed,
    measurements: vec![],
  };
  assert_eq!(report.verify(&selected).unwrap(), DeterministicGateOutcome::Passed);
  let retried = VerificationCheckInput::new(&admitted, &input.retry().unwrap()).unwrap();
  assert_eq!(selected.plan(), retried.plan());
  assert!(
    report.verify(&retried).is_err(),
    "same plan and node do not authorize another execution generation"
  );
}

#[test]
fn verification_rejects_impossible_complete_evidence_inventories_before_dispatch() {
  let (work, prior, _, input, _) = crate::flow_test_support::verification_execution_fixture(FlowNodeKind::BuildCommand);
  let schema=FlowDataSchema::new(key("inventory.input"),key("v1"),serde_json::from_value(serde_json::json!({"type":"object","fields":{"evidence":{"required":true,"value":{"type":"array","min_items":0,"max_items":128,"item":{"type":"object","fields":{"artifact_id":{"required":true,"value":{"type":"string","min_bytes":1,"max_bytes":256}},"content_digest":{"required":true,"value":{"type":"string","min_bytes":64,"max_bytes":64}},"encoded_size":{"required":true,"value":{"type":"integer","minimum":1,"maximum":16777216}}}}}}}})).unwrap()).unwrap();
  let original = prior.closure().definition(input.definition()).unwrap();
  let evidence: Vec<_> = (0..256)
    .map(|_| {
      FactoryArtifactReference::new(
        octacity_server_domain::ArtifactId::generate(),
        FactoryDigest::from_bytes([77; 32]),
        1,
      )
      .unwrap()
    })
    .collect();
  let duplicate =
    FactoryArtifactReference::new(evidence[0].artifact_id(), FactoryDigest::from_bytes([78; 32]), 1).unwrap();
  for (selected, valid) in [
    (evidence[..128].to_vec(), true),
    (vec![work.artifacts().acceptance().clone()], false),
    (vec![evidence[0].clone(), duplicate], false),
  ] {
    let mut execution = serde_json::to_value(original.execution()).unwrap();
    execution["verification"]["checks"][input.node().as_str()]["target"]["value"]["fields"]["evidence_inputs"] =
      serde_json::json!({"mapping":"pointer","path":"/payload/value/evidence"});
    let mut node = serde_json::to_value(&original.nodes()[0]).unwrap();
    node["input_schema"] = serde_json::to_value(schema.reference()).unwrap();
    let definition = FlowDefinition::new(FlowDefinitionInput {
      id: original.reference().id(),
      version: original.reference().version(),
      input_schema: Some(schema.reference().clone()),
      entry: original.entry().clone(),
      nodes: vec![serde_json::from_value(node).unwrap()],
      transitions: original.transitions().to_vec(),
      terminals: original.terminals().to_vec(),
      context_projections: vec![],
      data_projections: vec![],
      execution: serde_json::from_value(execution).unwrap(),
    })
    .unwrap();
    let flow = FlowRun::root(
      &FactoryRun::admitted(prior.root_run().factory_run_id(), &work),
      definition.reference(),
    )
    .unwrap();
    let cycle = WorkflowCycle::initial(&flow).unwrap();
    let mut schemas = prior.data_schemas().to_vec();
    schemas.push(schema.clone());
    let admitted = AdmittedFlow::new(
      PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
      prior.limits().clone(),
      flow,
      cycle,
      prior.wip_limits(),
    )
    .unwrap()
    .with_data_schemas(schemas)
    .unwrap();
    let current = FlowNodeInput::new(
      &work,
      &admitted,
      admitted.root_run(),
      admitted.initial_cycle(),
      input.node().clone(),
      FlowPayload::new(&schema, serde_json::json!({"evidence":selected})).unwrap(),
    )
    .unwrap()
    .with_context(input.context().unwrap().clone(), vec![])
    .unwrap();
    assert_eq!(
      VerificationCheckInput::new(&admitted, &current).is_ok(),
      valid,
      "public contract and evidence must form one valid complete inventory"
    );
  }
}

#[test]
fn a_verification_target_has_one_canonical_bounded_retained_input_inventory() {
  let (work, _, _, input, _) = crate::flow_test_support::verification_execution_fixture(FlowNodeKind::BuildCommand);
  let mut target = VerificationTarget {
    subject: input.context().unwrap().subject().candidate().unwrap().clone(),
    environment: reference("stage-v1"),
    public_contract: work.artifacts().acceptance().clone(),
    endpoint: None,
    evidence_inputs: (0..255)
      .map(|_| {
        FactoryArtifactReference::new(
          octacity_server_domain::ArtifactId::generate(),
          FactoryDigest::from_bytes([77; 32]),
          1,
        )
        .unwrap()
      })
      .collect(),
  };
  assert_eq!(target.retained_inputs().unwrap().len(), 256);
  target.evidence_inputs.push(work.artifacts().task().clone());
  assert!(
    target.retained_inputs().is_err(),
    "public contract counts against the complete 256-input ceiling"
  );
}

fn admitted_verification(environment: &str) -> (AdmittedFlow, CandidateSubject) {
  let (work, prior, _, _, _) = execution_fixture(FlowNodeKind::BuildCommand);
  let original = prior.closure().definition(prior.closure().root()).unwrap();
  let subject = CandidateSubject::new(
    work.subject().repository().unwrap().clone(),
    ImmutableRevision::new("candidate").unwrap(),
    FactoryDigest::from_bytes([95; 32]),
  );
  let mut execution = serde_json::to_value(original.execution()).unwrap();
  execution["verification"] = serde_json::json!({
    "data_handling": reference("internal-only"), "required_quorum": 1,
    "checks": {
      "accessibility_audit": {
        "class": "custom-visual-contract",
        "source_visibility": "black_box",
        "target":{"value":{"mapping":"constant","value":{"subject":subject,"environment":reference(environment),"public_contract":work.artifacts().acceptance(),"endpoint":"https://stage.example.test/deployment-a"}}},
        "endpoints": ["stage.example.test"], "secret_profiles": [], "identity_profiles": [],
        "required": true
      }
    }
  });
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: original.reference().id(),
    version: original.reference().version(),
    input_schema: original.input_schema().cloned(),
    entry: original.entry().clone(),
    nodes: original.nodes().to_vec(),
    transitions: original.transitions().to_vec(),
    terminals: original.terminals().to_vec(),
    context_projections: original.context_projections().to_vec(),
    data_projections: original.data_projections().to_vec(),
    execution: serde_json::from_value(execution).unwrap(),
  })
  .unwrap();
  let run = FactoryRun::admitted(prior.root_run().factory_run_id(), &work);
  let flow = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    prior.limits().clone(),
    flow,
    cycle,
    prior.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(prior.data_schemas().to_vec())
  .unwrap();
  (admitted, subject)
}

#[test]
fn verification_plan_freezes_the_admitted_flow_environment_policy_and_exact_subject() {
  let (admitted, subject) = admitted_verification("stage-v1");
  let plan = VerificationPlan::freeze(&admitted, admitted.closure().root(), subject.clone()).unwrap();
  assert_eq!(plan.subject(), &subject);
  assert_eq!(
    serde_json::to_value(&plan.policy().checks[&key("accessibility_audit")].target).unwrap()["value"]["value"]["environment"],
    serde_json::to_value(reference("stage-v1")).unwrap()
  );
  assert_eq!(
    plan.budget(),
    admitted
      .closure()
      .definition(admitted.closure().root())
      .unwrap()
      .execution()
      .budget()
  );
  let bytes = serde_json::to_vec(&plan).unwrap();
  let restored =
    VerificationPlan::restore(&bytes, &admitted, plan.definition(), &subject, plan.digest().unwrap()).unwrap();
  assert_eq!(restored, plan);
  let (replacement, _) = admitted_verification("stage-v2");
  assert!(
    VerificationPlan::restore(
      &bytes,
      &replacement,
      replacement.closure().root(),
      &subject,
      plan.digest().unwrap()
    )
    .is_err()
  );
  let changed = CandidateSubject::new(
    subject.exact().clone(),
    ImmutableRevision::new("another-candidate").unwrap(),
    FactoryDigest::from_bytes([96; 32]),
  );
  assert!(VerificationPlan::restore(&bytes, &admitted, plan.definition(), &changed, plan.digest().unwrap()).is_err());
  let mut wire = serde_json::to_value(&plan).unwrap();
  wire["policy"]["required_quorum"] = serde_json::json!(0);
  assert!(
    VerificationPlan::restore(
      &serde_json::to_vec(&wire).unwrap(),
      &admitted,
      plan.definition(),
      &subject,
      plan.digest().unwrap()
    )
    .is_err()
  );
}

#[test]
fn verification_plan_binds_the_exact_endpoint_and_not_only_its_host_grant() {
  let (admitted, subject) = admitted_verification("stage-v1");
  let plan = VerificationPlan::freeze(&admitted, admitted.closure().root(), subject.clone()).unwrap();
  assert_eq!(
    serde_json::to_value(&plan.policy().checks[&key("accessibility_audit")].target).unwrap()["value"]["value"]["endpoint"],
    serde_json::json!("https://stage.example.test/deployment-a")
  );
  let mut wire = serde_json::to_value(&plan).unwrap();
  wire["policy"]["checks"]["accessibility_audit"]["target"]["value"]["value"]["endpoint"] =
    serde_json::json!("https://stage.example.test/deployment-b");
  assert!(
    VerificationPlan::restore(
      &serde_json::to_vec(&wire).unwrap(),
      &admitted,
      plan.definition(),
      &subject,
      plan.digest().unwrap()
    )
    .is_err()
  );
}

#[test]
fn verification_plan_does_not_reuse_evidence_in_a_new_workflow_cycle() {
  let (work, admitted, _, input, node) =
    crate::flow_test_support::verification_execution_fixture(FlowNodeKind::BuildCommand);
  let initial = VerificationPlan::for_input(&admitted, &input).unwrap();
  let cycle = WorkflowCycle::next(
    WorkflowCycleId::generate(),
    admitted.root_run().id(),
    WorkflowCycleNumber::new(2).unwrap(),
    admitted.initial_cycle().id(),
    node.id(),
  );
  let next = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    &cycle,
    input.node().clone(),
    input.payload().clone(),
  )
  .unwrap()
  .with_context(input.context().unwrap().clone(), vec![])
  .unwrap();
  let repeated = VerificationPlan::for_input(&admitted, &next).unwrap();
  assert_eq!(initial.subject(), repeated.subject());
  assert_ne!(
    initial.digest().unwrap(),
    repeated.digest().unwrap(),
    "a fresh workflow cycle cannot reuse an earlier verification proof"
  );
}

#[test]
fn verification_observation_binds_the_plan_and_an_arbitrary_declared_check() {
  let (_, admitted, _, input, _) = crate::flow_test_support::verification_execution_fixture(FlowNodeKind::BuildCommand);
  let selected = VerificationCheckInput::new(&admitted, &input).unwrap();
  let report = VerificationObservation {
    plan_digest: selected.plan().digest().unwrap(),
    input_digest: input.digest().unwrap(),
    check: input.node().clone(),
    outcome: DeterministicGateOutcome::Failed,
    measurements: vec![VerificationMeasurement {
      name: key("request-duration-ms"),
      value: 37,
    }],
  };
  assert_eq!(report.verify(&selected).unwrap(), DeterministicGateOutcome::Failed);
  let mut changed = report.clone();
  changed.plan_digest = FactoryDigest::from_bytes([99; 32]);
  assert!(changed.verify(&selected).is_err());
  changed = report;
  changed.check = key("undeclared-check");
  assert!(changed.verify(&selected).is_err());
}

#[test]
fn supplied_verification_flow_composes_configured_checks_and_cannot_route_a_failure_into_summary_acceptance() {
  let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .ancestors()
    .find(|path| path.join("config/factory/flows").is_dir())
    .unwrap()
    .join("config/factory/flows");
  let manifest = std::fs::read(root.join("verification-flow.json")).expect("supplied modular verification template");
  let template = FlowDefinitionTemplate::from_manifest(&manifest, |path| {
    std::fs::read(root.join(path.as_str())).map_err(|_| FactoryError::InvalidConfiguration {
      field: "test configuration",
    })
  })
  .unwrap();
  let profiles = template
    .nodes
    .iter()
    .filter_map(|node| node.build_profile.clone())
    .map(|name| {
      (
        name,
        crate::flow_test_support::binding(octacity_server_domain::ProjectId::generate()),
      )
    })
    .collect();
  let published = template
    .instantiate(FlowDefinitionId::generate(), FlowDefinitionVersion::INITIAL, &profiles)
    .unwrap();
  let policy = published.definition.execution().verification().unwrap();
  assert_eq!(policy.checks.len(), 7);
  assert!(
    policy
      .checks
      .values()
      .all(|check| check.source_visibility == VerificationSourceVisibility::BlackBox)
  );
  assert_eq!(policy.checks.values().filter(|check| check.required).count(), 6);
  for (key, check) in &policy.checks {
    assert!(published.definition.node(key).unwrap().build().is_some());
    if check.required {
      let gate_key = FactoryKey::new(format!("{key}.gate")).unwrap();
      assert!(
        published
          .definition
          .transitions()
          .iter()
          .any(|transition| transition.predecessor() == &gate_key
            && transition.outcome().as_str() == "failed"
            && transition.target() == &FlowTransitionTarget::Terminal(crate::flow_test_support::key("rejected")))
      );
    }
  }
}

#[test]
fn verification_definition_rejects_unknown_checks_and_impossible_quorum_before_admission() {
  let (admitted, _) = admitted_verification("stage-v1");
  let original = admitted.closure().definition(admitted.closure().root()).unwrap();
  for invalid_policy in [
    serde_json::json!({"required_quorum":0}),
    serde_json::json!({"required_quorum":2}),
    serde_json::json!({"checks":{}}),
    serde_json::json!({"checks":{"invented":original.execution().verification().unwrap().checks[&key("accessibility_audit")].clone()}}),
  ] {
    let mut execution = serde_json::to_value(original.execution()).unwrap();
    for (name, value) in invalid_policy.as_object().unwrap() {
      execution["verification"][name] = value.clone();
    }
    assert!(
      FlowDefinition::new(FlowDefinitionInput {
        id: original.reference().id(),
        version: original.reference().version(),
        input_schema: original.input_schema().cloned(),
        entry: original.entry().clone(),
        nodes: original.nodes().to_vec(),
        transitions: original.transitions().to_vec(),
        terminals: original.terminals().to_vec(),
        context_projections: original.context_projections().to_vec(),
        data_projections: original.data_projections().to_vec(),
        execution: serde_json::from_value(execution).unwrap(),
      })
      .is_err(),
      "invalid verification choices must fail before publication"
    );
  }
}

#[test]
fn verification_definition_rejects_undeclared_network_and_credential_authority() {
  let (admitted, _) = admitted_verification("stage-v1");
  let original = admitted.closure().definition(admitted.closure().root()).unwrap();
  for category in ["network_hosts", "secret_profiles", "workload_identity_profiles"] {
    let mut wire = serde_json::to_value(&original.nodes()[0]).unwrap();
    wire["permissions"][category] = serde_json::json!(["undeclared.example.test"]);
    let node: FlowNodeDefinition = serde_json::from_value(wire).unwrap();
    assert!(
      FlowDefinition::new(FlowDefinitionInput {
        id: original.reference().id(),
        version: original.reference().version(),
        input_schema: original.input_schema().cloned(),
        entry: original.entry().clone(),
        nodes: vec![node],
        transitions: original.transitions().to_vec(),
        terminals: original.terminals().to_vec(),
        context_projections: vec![],
        data_projections: vec![],
        execution: original.execution().clone(),
      })
      .is_err(),
      "check authority must stay within its declared {category}"
    );
  }
}

#[test]
fn verification_check_targets_are_selected_from_each_frozen_input_without_changing_plan() {
  let (work, prior, schema, _, original_attempt) = execution_fixture(FlowNodeKind::BuildCommand);
  let subject = CandidateSubject::new(
    work.subject().repository().unwrap().clone(),
    ImmutableRevision::new("candidate").unwrap(),
    FactoryDigest::from_bytes([95; 32]),
  );
  let original = prior.closure().definition(prior.closure().root()).unwrap();
  let mut execution = serde_json::to_value(original.execution()).unwrap();
  execution["verification"] = serde_json::json!({
    "data_handling":reference("internal-only"),"required_quorum":1,"checks":{
      "accessibility_audit": {
        "class":"custom-check", "source_visibility":"black_box", "endpoints":[],
        "secret_profiles":[], "identity_profiles":[],"required":true,
        "target":{"value":{"mapping":"object","fields":{
          "environment":{"mapping":"constant","value":reference("stage-v1")},
          "public_contract":{"mapping":"pointer","path":"/context/entries/0/source/value"},
          "subject":{"mapping":"pointer","path":"/context/subject/value"}
        }}}
      }
    }
  });
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: original.reference().id(),
    version: original.reference().version(),
    input_schema: original.input_schema().cloned(),
    entry: original.entry().clone(),
    nodes: original.nodes().to_vec(),
    transitions: original.transitions().to_vec(),
    terminals: original.terminals().to_vec(),
    context_projections: vec![],
    data_projections: vec![],
    execution: serde_json::from_value(execution).unwrap(),
  })
  .unwrap();
  let run = FactoryRun::admitted(prior.root_run().factory_run_id(), &work);
  let flow = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    prior.limits().clone(),
    flow,
    cycle,
    prior.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(vec![schema.clone()])
  .unwrap();
  let mut plans = Vec::new();
  for contract in [work.artifacts().task(), work.artifacts().acceptance()] {
    let task_subject = FactoryTaskSubject::Candidate(subject.clone());
    let entry = ContextManifestEntry::new(
      ContextSourceKind::Task,
      key("public-contract"),
      task_subject.clone(),
      FactoryContextReference::Artifact(contract.clone()),
      contract.content_digest(),
      contract.encoded_size(),
      FactorySafeText::new("Explicit deployed contract").unwrap(),
      FactoryDigest::from_bytes([9; 32]),
    )
    .unwrap();
    let input = FlowNodeInput::new(
      &work,
      &admitted,
      admitted.root_run(),
      admitted.initial_cycle(),
      key("accessibility_audit"),
      FlowPayload::new(&schema, serde_json::json!({"count":3})).unwrap(),
    )
    .unwrap()
    .with_context(
      ContextManifest::new(
        ContextManifestId::generate(),
        task_subject,
        FactoryDigest::from_bytes([9; 32]),
        vec![entry],
      )
      .unwrap(),
      vec![],
    )
    .unwrap();
    let plan = VerificationPlan::for_input(&admitted, &input).unwrap();
    let selected = VerificationCheckInput::new(&admitted, &input).unwrap();
    assert_eq!(&selected.target().public_contract, contract);
    assert_eq!(&selected.target().subject, &subject);
    let definition = admitted.closure().definition(input.definition()).unwrap();
    let node = NodeAttempt::new(
      admitted.root_run(),
      admitted.initial_cycle(),
      definition,
      NodeAttemptInput {
        id: NodeAttemptId::generate(),
        node_key: input.node().clone(),
        node_kind: FlowNodeKind::BuildCommand,
        number: NodeAttemptNumber::INITIAL,
        input_digest: input.digest().unwrap(),
        budget: original_attempt.budget(),
        started_at: original_attempt.started_at(),
        deadline: original_attempt.deadline(),
        execution: original_attempt.execution().clone(),
        ownership: FactoryClaimOwnership::new(original_attempt.owner().clone(), original_attempt.claim()),
      },
    )
    .unwrap();
    let intent = FlowBuildIntent::new(input.clone(), &admitted, node.clone(), BudgetUsage::default(), 1).unwrap();
    assert_eq!(
      intent.verification_plan(),
      Some(&plan),
      "the same frozen plan must reach ordinary Build dispatch"
    );
    let restored = FlowBuildIntent::restore(
      &serde_json::to_vec(&intent).unwrap(),
      &input,
      &admitted,
      &node,
      intent.operation_id().unwrap(),
    )
    .unwrap();
    assert_eq!(restored.verification_plan(), Some(&plan));
    let mut wire = serde_json::to_value(&intent).unwrap();
    wire["verification_check"]["target"]["environment"] = serde_json::to_value(reference("substituted-stage")).unwrap();
    assert!(
      FlowBuildIntent::restore(
        &serde_json::to_vec(&wire).unwrap(),
        &input,
        &admitted,
        &node,
        intent.operation_id().unwrap()
      )
      .is_err()
    );
    plans.push(selected);
  }
  assert_ne!(
    plans[0].input_digest(),
    plans[1].input_digest(),
    "different public contracts cannot reuse verification evidence"
  );
  assert_eq!(
    plans[0].plan(),
    plans[1].plan(),
    "one configured plan resolves different immutable input contracts"
  );
}

#[test]
fn black_box_verification_rejects_implementation_context_before_plan_dispatch() {
  let (work, admitted, _, input, _) =
    crate::flow_test_support::verification_execution_fixture(FlowNodeKind::BuildCommand);
  assert!(VerificationPlan::for_input(&admitted, &input).is_ok());
  for kind in [
    ContextSourceKind::TypedResult,
    ContextSourceKind::BoundedSummary,
    ContextSourceKind::RepositoryRange,
  ] {
    let context = input.context().unwrap();
    let artifact = work.artifacts().task().clone();
    let source = if kind == ContextSourceKind::RepositoryRange {
      FactoryContextReference::RepositoryRange(
        FactoryRepositoryRange::new(
          work.subject().repository().unwrap().repository_id(),
          work.subject().repository().unwrap().base_revision().clone(),
          FactoryRepositoryPath::new("src/private.rs").unwrap(),
          1,
          10,
        )
        .unwrap(),
      )
    } else {
      FactoryContextReference::Artifact(artifact.clone())
    };
    let extra = ContextManifestEntry::new(
      kind,
      key("implementation-private"),
      context.subject().clone(),
      source,
      artifact.content_digest(),
      artifact.encoded_size(),
      FactorySafeText::new("Undeclared implementation input").unwrap(),
      FactoryDigest::from_bytes([8; 32]),
    )
    .unwrap();
    let mut entries = context.entries().to_vec();
    entries.push(extra);
    let changed = input
      .clone()
      .with_context(
        ContextManifest::new(
          ContextManifestId::generate(),
          context.subject().clone(),
          context.construction_policy_digest(),
          entries,
        )
        .unwrap(),
        vec![],
      )
      .unwrap();
    assert!(
      VerificationPlan::for_input(&admitted, &changed).is_err(),
      "control ordering cannot grant implementation or repository context"
    );
  }
}

#[test]
fn black_box_verification_can_inspect_only_explicitly_selected_own_evidence() {
  let (work, prior, _, input, _) = crate::flow_test_support::verification_execution_fixture(FlowNodeKind::BuildCommand);
  let original = prior.closure().definition(input.definition()).unwrap();
  let evidence = work.artifacts().task().clone();
  let mut execution = serde_json::to_value(original.execution()).unwrap();
  execution["verification"]["checks"]["accessibility_audit"]["target"]["value"]["fields"]["evidence_inputs"] =
    serde_json::json!({"mapping":"constant","value":[evidence.clone()]});
  execution["verification"]["checks"]["accessibility_audit"]["target"]["value"]["fields"]["public_contract"] =
    serde_json::json!({"mapping":"constant","value":work.artifacts().acceptance()});
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: original.reference().id(),
    version: original.reference().version(),
    input_schema: original.input_schema().cloned(),
    entry: original.entry().clone(),
    nodes: original.nodes().to_vec(),
    transitions: original.transitions().to_vec(),
    terminals: original.terminals().to_vec(),
    context_projections: vec![],
    data_projections: vec![],
    execution: serde_json::from_value(execution).unwrap(),
  })
  .unwrap();
  let run = FactoryRun::admitted(prior.root_run().factory_run_id(), &work);
  let flow = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    prior.limits().clone(),
    flow,
    cycle,
    prior.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(prior.data_schemas().to_vec())
  .unwrap();
  let context = input.context().unwrap();
  let mut entries = context.entries().to_vec();
  entries.push(
    ContextManifestEntry::new(
      ContextSourceKind::Evidence,
      key("own-result"),
      context.subject().clone(),
      FactoryContextReference::Artifact(evidence.clone()),
      evidence.content_digest(),
      evidence.encoded_size(),
      FactorySafeText::new("Explicit prior evidence from this verification").unwrap(),
      FactoryDigest::from_bytes([8; 32]),
    )
    .unwrap(),
  );
  let selected = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    input.node().clone(),
    input.payload().clone(),
  )
  .unwrap()
  .with_context(
    ContextManifest::new(
      ContextManifestId::generate(),
      context.subject().clone(),
      context.construction_policy_digest(),
      entries,
    )
    .unwrap(),
    vec![],
  )
  .unwrap();
  assert!(
    VerificationPlan::for_input(&admitted, &selected).is_ok(),
    "declared evidence is visible without opening implementation context"
  );
}
