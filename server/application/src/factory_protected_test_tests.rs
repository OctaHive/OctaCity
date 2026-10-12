use crate::factory_node_test_support::{Builds, document_kind, fixtures::*};
use crate::*;
use octacity_server_factory::*;

fn time(n: i64) -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(n).unwrap()
}

#[test]
fn configured_test_policy_accepts_all_declared_families_and_new_project_classes() {
  let policy: FactoryProtectedTestPolicy =
    serde_json::from_str(include_str!("../../../config/factory/profiles/protected-tests.json")).unwrap();
  let mut wire = serde_json::to_value(policy).unwrap();
  let classes = wire["classes"].as_array().unwrap();
  for class in [
    "integration",
    "end_to_end",
    "performance",
    "ui_visual",
    "security_negative",
    "migration",
    "compatibility",
    "project.custom",
  ] {
    assert!(
      classes.contains(&serde_json::json!(class)),
      "the profile must declare {class} without a Rust stage/class enum"
    );
  }
  wire["classes"]
    .as_array_mut()
    .unwrap()
    .push(serde_json::json!("new.operator.check"));
  let extended: FactoryProtectedTestPolicy = serde_json::from_value(wire.clone()).unwrap();
  assert_eq!(
    serde_json::to_value(extended).unwrap()["classes"]
      .as_array()
      .unwrap()
      .len(),
    9
  );
  wire["classes"]
    .as_array_mut()
    .unwrap()
    .push(serde_json::json!("new.operator.check"));
  assert!(serde_json::from_value::<FactoryProtectedTestPolicy>(wire).is_err());
}

#[test]
fn protected_policy_reserves_input_slots_before_accepting_authoring_limits() {
  let make = |files, reserved| {
    serde_json::json!({
      "classes":["custom"], "max_files":files, "max_bytes":9000,
      "retention_millis":1000, "reserved_inputs":reserved
    })
  };
  assert!(serde_json::from_value::<FactoryProtectedTestPolicy>(make(249, 7)).is_ok());
  assert!(serde_json::from_value::<FactoryProtectedTestPolicy>(make(250, 7)).is_err());
  assert!(serde_json::from_value::<FactoryProtectedTestPolicy>(make(252, 4)).is_ok());
  assert!(serde_json::from_value::<FactoryProtectedTestPolicy>(make(253, 4)).is_err());
  assert!(serde_json::from_value::<FactoryProtectedTestPolicy>(make(255, 0)).is_err());
}

#[test]
fn protected_capture_verifies_actual_files_and_frozen_plan_before_public_handoff() {
  let (_, admitted, _, input, node) = execution_fixture(FlowNodeKind::Reasoning);
  let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
  let builds = Builds::new();
  let execution = FlowBuildExecution::new(
    &intent,
    builds.acceptance.build_id,
    builds.acceptance.attempt_id,
    builds.acceptance.job_ids.clone(),
  )
  .unwrap();
  let plan = ProtectedTestPlan::new(
    intent.input().digest().unwrap(),
    intent.input().context().unwrap().subject().clone(),
    ProtectedTestCoverage::new(vec![key("project.contract")], 4, 9000, 4).unwrap(),
    time(900),
  )
  .unwrap();
  let tool = reference("tests.capture");
  let schema = reference("tests.inventory");
  let mut file = document_kind(
    &builds,
    &key("hidden.assertions"),
    &schema,
    b"assert_eq!(result, SECRET_EXPECTED_VALUE);".to_vec(),
    tool.clone(),
    EvidenceOutputKind::Artifact,
  );
  file.plugin = intent.profile().plugin.clone();
  let manifest = FactoryProtectedTestInventory {
    plan_digest: plan.digest().unwrap(),
    input_digest: intent.input().digest().unwrap(),
    node_attempt_id: intent.node().id(),
    files: vec![ProtectedTestFile {
      class: key("project.contract"),
      path: FactoryRepositoryPath::new("contract.rs").unwrap(),
      artifact: crate::factory_node_build::artifact_reference(&file).unwrap(),
    }],
  };
  let mut inventory = document_kind(
    &builds,
    &key("private.inventory"),
    &schema,
    serde_json::to_vec(&manifest).unwrap(),
    tool.clone(),
    EvidenceOutputKind::Artifact,
  );
  inventory.plugin = intent.profile().plugin.clone();
  let requirement = EvidenceRequirement::new(
    key("private.inventory"),
    EvidenceOutputKind::Artifact,
    schema,
    tool.clone(),
    intent.profile().plugin.clone(),
  );
  let mut identity = inventory.record.identity().clone();
  identity.lease_id = file.record.identity().lease_id;
  let record = octacity_server_artifacts::ArtifactRecord::pending(identity.clone(), time(30)).unwrap();
  let record = record
    .transition(
      &identity,
      record.version(),
      octacity_server_artifacts::ArtifactEvent::BeginVerification,
      time(30),
    )
    .unwrap();
  inventory.record = record
    .transition(
      &identity,
      record.version(),
      octacity_server_artifacts::ArtifactEvent::Publish,
      time(30),
    )
    .unwrap();
  let outputs = FactoryNodeBuildOutputs {
    execution: FactoryNodeBuildExecutionFacts {
      profile: intent.profile().clone(),
      input_digest: intent.input().digest().unwrap(),
      permissions: intent.permissions().clone(),
      budget: intent.budget(),
      deadline: intent.node().deadline(),
    },
    usage: BudgetUsage {
      attempts: 1,
      output_bytes: (file.bytes.len() + inventory.bytes.len()) as u64,
      ..Default::default()
    },
    documents: vec![inventory, file],
    verified_at: time(30),
    fresh_until: time(900),
  };
  let bundle =
    verify_factory_protected_test_bundle(&intent, &execution, &outputs, &plan, &requirement, time(31)).unwrap();
  assert_eq!(bundle.plan(), &plan);
  let public = serde_json::to_string(&bundle.public_handoff()).unwrap();
  assert!(!public.contains("SECRET_EXPECTED_VALUE") && !public.contains("contract.rs") && !public.contains("artifact"));
  let restored: ProtectedTestBundle = serde_json::from_slice(&bundle.canonical_bytes().unwrap()).unwrap();
  assert_eq!(restored, bundle);
  let mut changed = outputs.clone();
  changed.documents[1].bytes[0] ^= 1;
  assert!(verify_factory_protected_test_bundle(&intent, &execution, &changed, &plan, &requirement, time(31)).is_err());
  let mut missing = outputs.clone();
  missing.documents.pop();
  assert!(verify_factory_protected_test_bundle(&intent, &execution, &missing, &plan, &requirement, time(31)).is_err());
  let foreign = ProtectedTestPlan::new(
    FactoryDigest::from_bytes([98; 32]),
    plan.subject().clone(),
    ProtectedTestCoverage::new(plan.classes().to_vec(), 4, 9000, 4).unwrap(),
    time(900),
  )
  .unwrap();
  assert!(
    verify_factory_protected_test_bundle(&intent, &execution, &outputs, &foreign, &requirement, time(31)).is_err()
  );
}

#[test]
fn protected_materialization_requires_an_explicit_exact_selection_and_retained_files() {
  // The implementation context deliberately contains only its declared task.
  let (_, admitted, _, input, node) = execution_fixture(FlowNodeKind::BuildCommand);
  let intent = FlowBuildIntent::new(input.clone(), &admitted, node, BudgetUsage::default(), 7).unwrap();
  let builds = Builds::new();
  let doc = document_kind(
    &builds,
    &key("tests"),
    &reference("private.inventory"),
    b"private assertions".to_vec(),
    reference("capture"),
    EvidenceOutputKind::Artifact,
  );
  let mut manifest = document_kind(
    &builds,
    &key("inventory"),
    &reference("private.inventory"),
    b"private inventory".to_vec(),
    reference("capture"),
    EvidenceOutputKind::Artifact,
  );
  let mut identity = manifest.record.identity().clone();
  identity.lease_id = doc.record.identity().lease_id;
  let row = octacity_server_artifacts::ArtifactRecord::pending(identity.clone(), time(30)).unwrap();
  let row = row
    .transition(
      &identity,
      row.version(),
      octacity_server_artifacts::ArtifactEvent::BeginVerification,
      time(30),
    )
    .unwrap();
  manifest.record = row
    .transition(
      &identity,
      row.version(),
      octacity_server_artifacts::ArtifactEvent::Publish,
      time(30),
    )
    .unwrap();
  let plan = ProtectedTestPlan::new(
    intent.input().digest().unwrap(),
    intent.input().context().unwrap().subject().clone(),
    ProtectedTestCoverage::new(vec![key("custom")], 2, 9000, 4).unwrap(),
    time(900),
  )
  .unwrap();
  let bundle = ProtectedTestBundle::new(
    plan,
    crate::factory_node_build::artifact_reference(&manifest).unwrap(),
    vec![ProtectedTestFile {
      class: key("custom"),
      path: FactoryRepositoryPath::new("acceptance.rs").unwrap(),
      artifact: crate::factory_node_build::artifact_reference(&doc).unwrap(),
    }],
    crate::factory_node_build::document_producer(&manifest),
    time(30),
    time(900),
  )
  .unwrap();
  assert!(
    factory_protected_test_materialization(
      &input,
      &bundle,
      &manifest.record,
      std::slice::from_ref(&doc.record),
      time(31)
    )
    .is_err(),
    "a sequenced implementation Job cannot acquire hidden files from an unselected bundle"
  );
  let prior = input.context().unwrap();
  let subject = prior.subject().clone();
  let artifact = bundle.artifact().clone();
  let entry = ContextManifestEntry::new(
    ContextSourceKind::Evidence,
    key("selected.tests"),
    subject.clone(),
    FactoryContextReference::Artifact(artifact.clone()),
    artifact.content_digest(),
    artifact.encoded_size(),
    FactorySafeText::new("Exact validation-only selection").unwrap(),
    FactoryDigest::from_bytes([1; 32]),
  )
  .unwrap();
  let context = ContextManifest::new(
    ContextManifestId::generate(),
    subject,
    prior.construction_policy_digest(),
    vec![entry],
  )
  .unwrap();
  let validator = input.clone().with_context(context, vec![]).unwrap();
  let inputs = factory_protected_test_materialization(
    &validator,
    &bundle,
    &manifest.record,
    std::slice::from_ref(&doc.record),
    time(31),
  )
  .unwrap();
  assert_eq!(inputs.len(), 2);
  assert!(
    inputs
      .iter()
      .any(|file| file.destination == "/octacity/protected/tests/acceptance.rs"
        && file.sha256 == FactoryDigest::content_sha256(&doc.bytes).to_string())
  );
  assert_eq!(
    factory_protected_test_materialization(
      &validator,
      &bundle,
      &manifest.record,
      std::slice::from_ref(&doc.record),
      time(32)
    )
    .unwrap(),
    inputs
  );
  assert!(factory_protected_test_materialization(&validator, &bundle, &manifest.record, &[], time(31)).is_err());
  assert!(
    factory_protected_test_materialization(&validator, &bundle, &manifest.record, &[doc.record], time(901)).is_err()
  );
}

#[test]
fn author_executor_keeps_private_inventory_out_of_the_common_result_and_restores_it_after_restart() {
  crate::factory_admission_tests::run_ready(async {
    use std::sync::Arc;
    let work = work();
    let tool = reference("tests.capture");
    let requirement = EvidenceRequirement::new(
      key("private.inventory"),
      EvidenceOutputKind::Artifact,
      reference("tests.inventory"),
      tool.clone(),
      reference("runner"),
    );
    let text = || FlowValueSchema::String {
      min_bytes: 1,
      max_bytes: 64,
    };
    let schema = FlowDataSchema::new(
      key("public.coverage"),
      key("v1"),
      FlowValueSchema::Object {
        fields: std::collections::BTreeMap::from([
          (key("accepted_contract"), FlowFieldSchema::required(text())),
          (
            key("classes"),
            FlowFieldSchema::required(FlowValueSchema::Array {
              item: Box::new(text()),
              min_items: 1,
              max_items: 4,
            }),
          ),
        ]),
      },
    )
    .unwrap();
    let mut binding = binding(work.subject().project_id());
    binding.result_tool = Some(tool.clone());
    binding.result_output = key("public.coverage");
    binding.evidence = vec![requirement.clone()];
    let (work, admitted, schema, input, node) =
      execution_fixture_for_work_with_binding(FlowNodeKind::Reasoning, work, binding, None, Some(schema));
    let intent = FlowBuildIntent::new(input.clone(), &admitted, node.clone(), BudgetUsage::default(), 7).unwrap();
    let plan = ProtectedTestPlan::new(
      input.digest().unwrap(),
      input.context().unwrap().subject().clone(),
      ProtectedTestCoverage::new(vec![key("custom")], 4, 9000, 4).unwrap(),
      time(900),
    )
    .unwrap();
    struct Plan(ProtectedTestPlan);
    #[async_trait::async_trait]
    impl FactoryProtectedTestPlanSource for Plan {
      async fn frozen_plan(&self, _: &FlowBuildIntent) -> Result<ProtectedTestPlan, ApplicationError> {
        Ok(self.0.clone())
      }
    }
    let builds = Arc::new(Builds::new());
    *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
    let mut file = document_kind(
      &builds,
      &key("hidden.file"),
      requirement.schema(),
      b"PRIVATE_EXPECTATION".to_vec(),
      tool.clone(),
      EvidenceOutputKind::Artifact,
    );
    file.plugin = intent.profile().plugin.clone();
    let inventory = FactoryProtectedTestInventory {
      plan_digest: plan.digest().unwrap(),
      input_digest: input.digest().unwrap(),
      node_attempt_id: node.id(),
      files: vec![ProtectedTestFile {
        class: key("custom"),
        path: FactoryRepositoryPath::new("hidden.rs").unwrap(),
        artifact: crate::factory_node_build::artifact_reference(&file).unwrap(),
      }],
    };
    let mut manifest = document_kind(
      &builds,
      requirement.kind(),
      requirement.schema(),
      serde_json::to_vec(&inventory).unwrap(),
      tool.clone(),
      EvidenceOutputKind::Artifact,
    );
    let mut identity = manifest.record.identity().clone();
    identity.lease_id = file.record.identity().lease_id;
    let row = octacity_server_artifacts::ArtifactRecord::pending(identity.clone(), time(30)).unwrap();
    let row = row
      .transition(
        &identity,
        row.version(),
        octacity_server_artifacts::ArtifactEvent::BeginVerification,
        time(30),
      )
      .unwrap();
    manifest.record = row
      .transition(
        &identity,
        row.version(),
        octacity_server_artifacts::ArtifactEvent::Publish,
        time(30),
      )
      .unwrap();
    let public = ProtectedTestHandoff {
      accepted_contract: plan.accepted_contract(),
      classes: plan.classes().to_vec(),
    };
    let handoff = document_kind(
      &builds,
      &intent.profile().result_output,
      schema.reference(),
      serde_json::to_vec(&public).unwrap(),
      tool,
      EvidenceOutputKind::Report,
    );
    let outputs = FactoryNodeBuildOutputs {
      execution: FactoryNodeBuildExecutionFacts {
        profile: intent.profile().clone(),
        input_digest: input.digest().unwrap(),
        permissions: intent.permissions().clone(),
        budget: intent.budget(),
        deadline: node.deadline(),
      },
      usage: BudgetUsage {
        attempts: 1,
        output_bytes: (file.bytes.len() + manifest.bytes.len() + handoff.bytes.len()) as u64,
        ..Default::default()
      },
      documents: vec![manifest.clone(), file, handoff],
      verified_at: time(30),
      fresh_until: time(900),
    };
    let source = Arc::new(crate::factory_node_test_support::Outputs(Some(outputs)));
    let executor = FactoryProtectedTestAuthoringAdapter::new(
      builds.clone(),
      source,
      Arc::new(Plan(plan.clone())),
      requirement.clone(),
    );
    let request = || FactoryNodeExecutionRequest {
      run_id: input.factory_run_id(),
      admitted: &admitted,
      work: &work,
      input: &input,
      attempt: &node,
      build: Some(&intent),
      ownership: FactoryClaimOwnership::new(node.owner().clone(), node.claim()),
      observed_at: time(31),
    };
    let step = executor.observe(request()).await.unwrap();
    let FactoryNodeExecutionStep::Completed {
      record,
      execution: Some(execution),
      usage,
      ..
    } = step
    else {
      panic!("author must complete through ordinary Build")
    };
    assert_eq!(record.payload().value(), &serde_json::to_value(public).unwrap());
    let metadata = record.metadata().unwrap();
    assert!(metadata["build"]["outputs"]["private.inventory"]["data"].is_null());
    assert!(!serde_json::to_string(&record).unwrap().contains("PRIVATE_EXPECTATION"));
    let definition = admitted.closure().definition(input.definition()).unwrap();
    let completion = record
      .completion(
        &node,
        definition,
        FactoryClaimOwnership::new(node.owner().clone(), node.claim()),
        usage,
        time(31),
      )
      .unwrap();
    let recovered = FlowNodeRecord::restore(
      &serde_json::to_vec(&record).unwrap(),
      &input,
      &node,
      definition,
      &schema,
      &completion,
    )
    .unwrap();
    assert_eq!(&recovered, record.as_ref());
    let recovered_intent = FlowBuildIntent::restore(
      &serde_json::to_vec(&intent).unwrap(),
      &input,
      &admitted,
      &node,
      intent.operation_id().unwrap(),
    )
    .unwrap();
    let recovered_plan: ProtectedTestPlan = serde_json::from_slice(&serde_json::to_vec(&plan).unwrap()).unwrap();
    let restored = restore_factory_protected_test_bundle(
      &recovered,
      &recovered_intent,
      &execution,
      &manifest,
      &recovered_plan,
      &requirement,
      time(32),
    )
    .unwrap();
    assert_eq!(restored.files(), inventory.files);
    assert_eq!(
      restored.public_handoff(),
      serde_json::from_value(record.payload().value().clone()).unwrap()
    );
  });
}

#[test]
fn validation_freezes_both_inputs_and_rejects_replayed_or_incomplete_reports() {
  verify_validation_materialization(false);
}

#[test]
fn validation_requires_captured_public_deliverables_and_rejects_protected_file_copying() {
  verify_validation_materialization(true);
}
fn verify_validation_materialization(require_deliverables: bool) {
  let template: FlowDefinitionTemplate = crate::factory_node_test_support::fixtures::supplied_base_flow_template();
  let report_schema = template
    .data_schemas()
    .unwrap()
    .remove(&key("protected_validation_report"))
    .unwrap();
  let work = work();
  let mut binding = binding(work.subject().project_id());
  if require_deliverables {
    binding = crate::factory_node_capture_tests::deliverable_binding(binding);
  }
  let requirement = EvidenceRequirement::new(
    key("validation.report"),
    EvidenceOutputKind::Report,
    report_schema.reference().clone(),
    binding.tool.clone(),
    binding.plugin.clone(),
  );
  let facts_schema = template
    .data_schemas()
    .unwrap()
    .remove(&key("protected_validation_facts"))
    .unwrap();
  let facts_requirement = EvidenceRequirement::new(
    key("validation.facts"),
    EvidenceOutputKind::Report,
    facts_schema.reference().clone(),
    binding.tool.clone(),
    binding.plugin.clone(),
  );
  binding.evidence.push(requirement.clone());
  binding.evidence.push(facts_requirement.clone());
  let (_, admitted, schema, input, node) =
    execution_fixture_for_work_with_binding(FlowNodeKind::BuildCommand, work.clone(), binding.clone(), None, None);
  let admitted = admitted
    .with_data_schemas(vec![schema.clone(), report_schema, facts_schema])
    .unwrap();
  let capture_intent = FlowBuildIntent::new(input.clone(), &admitted, node.clone(), BudgetUsage::default(), 7).unwrap();
  let builds = Builds::new();
  let execution = FlowBuildExecution::new(
    &capture_intent,
    builds.acceptance.build_id,
    builds.acceptance.attempt_id,
    builds.acceptance.job_ids.clone(),
  )
  .unwrap();
  let mut captured = crate::factory_node_capture_tests::capture_outputs(&capture_intent, &builds, "base");
  if require_deliverables {
    crate::factory_node_capture_tests::complete_deliverable_outputs(
      &mut captured,
      FactoryDigest::content_sha256(b"public unit test"),
    );
  }
  let candidate = verify_factory_node_change_set(
    &capture_intent,
    &execution,
    &captured,
    &reference("git.capture"),
    time(31),
  )
  .unwrap();
  let author = Builds::new();
  let file = document_kind(
    &author,
    &key("private.file"),
    &reference("tests.inventory"),
    b"PRIVATE_VALUE".to_vec(),
    reference("capture"),
    EvidenceOutputKind::Artifact,
  );
  let mut manifest = document_kind(
    &author,
    &key("private.inventory"),
    &reference("tests.inventory"),
    b"inventory".to_vec(),
    reference("capture"),
    EvidenceOutputKind::Artifact,
  );
  let mut identity = manifest.record.identity().clone();
  identity.lease_id = file.record.identity().lease_id;
  let row = octacity_server_artifacts::ArtifactRecord::pending(identity.clone(), time(30)).unwrap();
  let row = row
    .transition(
      &identity,
      row.version(),
      octacity_server_artifacts::ArtifactEvent::BeginVerification,
      time(30),
    )
    .unwrap();
  manifest.record = row
    .transition(
      &identity,
      row.version(),
      octacity_server_artifacts::ArtifactEvent::Publish,
      time(30),
    )
    .unwrap();
  let plan = ProtectedTestPlan::new(
    if require_deliverables {
      "a".repeat(64).parse().unwrap()
    } else {
      input.digest().unwrap()
    },
    input.context().unwrap().subject().clone(),
    ProtectedTestCoverage::new(vec![key("project.custom")], 4, 9000, 4).unwrap(),
    time(900),
  )
  .unwrap();
  let bundle = ProtectedTestBundle::new(
    plan,
    crate::factory_node_build::artifact_reference(&manifest).unwrap(),
    vec![ProtectedTestFile {
      class: key("project.custom"),
      path: FactoryRepositoryPath::new("acceptance.rs").unwrap(),
      artifact: crate::factory_node_build::artifact_reference(&file).unwrap(),
    }],
    crate::factory_node_build::document_producer(&manifest),
    time(30),
    time(900),
  )
  .unwrap();
  let deliverables = if require_deliverables {
    let checked = verify_factory_node_candidate_deliverables(
      &capture_intent,
      &execution,
      &captured,
      &reference("git.capture"),
      time(31),
    )
    .unwrap();
    checked.verify_protected(&bundle).unwrap();
    let mut copied = captured.clone();
    crate::factory_node_capture_tests::complete_deliverable_outputs(
      &mut copied,
      FactoryDigest::content_sha256(b"PRIVATE_VALUE"),
    );
    let copied = verify_factory_node_candidate_deliverables(
      &capture_intent,
      &execution,
      &copied,
      &reference("git.capture"),
      time(31),
    )
    .unwrap();
    assert!(
      copied.verify_protected(&bundle).is_err(),
      "public-path disclosure must fail despite model success"
    );
    Some(checked)
  } else {
    None
  };
  let subject = FactoryTaskSubject::Candidate(candidate.subject().clone());
  let entries = captured
    .documents
    .iter()
    .chain([&manifest])
    .map(|doc| {
      let artifact = crate::factory_node_build::artifact_reference(doc).unwrap();
      ContextManifestEntry::new(
        ContextSourceKind::Evidence,
        key(doc.record.identity().logical_name.as_str()),
        subject.clone(),
        FactoryContextReference::Artifact(artifact.clone()),
        artifact.content_digest(),
        artifact.encoded_size(),
        FactorySafeText::new("Explicit exact validator input").unwrap(),
        FactoryDigest::from_bytes([1; 32]),
      )
      .unwrap()
    })
    .collect();
  let context = ContextManifest::new(
    ContextManifestId::generate(),
    subject,
    FactoryDigest::from_bytes([1; 32]),
    entries,
  )
  .unwrap();
  let (selected, node) = crate::factory_node_test_support::input_with_candidate_context(
    &admitted,
    input,
    &node,
    candidate.subject().clone(),
  );
  let selected = selected.with_context(context.clone(), vec![]).unwrap();
  let mut wire = serde_json::to_value(&node).unwrap();
  wire["input_digest"] = serde_json::to_value(selected.digest().unwrap()).unwrap();
  let node: NodeAttempt = serde_json::from_value(wire).unwrap();
  let intent = FlowBuildIntent::new(selected, &admitted, node, BudgetUsage::default(), 7).unwrap();
  let request = || FactoryProtectedValidationRequest {
    deliverables: deliverables.as_ref(),
    intent: &intent,
    candidate: &candidate,
    candidate_bundle: &captured.documents[0].record,
    candidate_manifest: &captured.documents[1].record,
    tests: &bundle,
    test_manifest: &manifest.record,
    test_files: std::slice::from_ref(&file.record),
    observed_at: time(31),
  };
  let materialized = factory_protected_validation_materialization(request()).unwrap();
  if require_deliverables {
    assert!(
      factory_protected_validation_materialization(FactoryProtectedValidationRequest {
        deliverables: None,
        ..request()
      })
      .is_err()
    );
  }
  for root in [octacity_protocol::FACTORY_SOURCE_ROOT, "/workspace", "/octacity", "/"] {
    let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
      mounts: vec![MountPermission::new(
        FactoryPath::new(root).unwrap(),
        MountMode::ReadWrite,
      )],
      ..Default::default()
    })
    .unwrap();
    let (_, unsafe_admitted, _, unsafe_input, unsafe_node) = execution_fixture_with_permissions(
      FlowNodeKind::BuildCommand,
      work.clone(),
      binding.clone(),
      None,
      permissions,
    );
    let unsafe_input = unsafe_input.with_context(context.clone(), vec![]).unwrap();
    let (unsafe_input, unsafe_node) = crate::factory_node_test_support::input_with_candidate_context(
      &unsafe_admitted,
      unsafe_input,
      &unsafe_node,
      candidate.subject().clone(),
    );
    let unsafe_intent =
      FlowBuildIntent::new(unsafe_input, &unsafe_admitted, unsafe_node, BudgetUsage::default(), 7).unwrap();
    let unsafe_request = FactoryProtectedValidationRequest {
      intent: &unsafe_intent,
      ..request()
    };
    assert!(
      factory_protected_validation_materialization(unsafe_request).is_err(),
      "writable {root} must not alter the exact candidate or protected tests"
    );
  }
  let managed = octacity_protocol::ProtectedInputV3 {
    artifact_id: octacity_server_domain::ArtifactId::generate().to_string(),
    size_bytes: 1,
    sha256: "ab".repeat(32),
    media_type: "application/json".to_owned(),
    destination: "/octacity/protected/managed.json".to_owned(),
  };
  assert_eq!(
    materialized
      .wire_inputs(std::slice::from_ref(&managed))
      .unwrap()
      .1
      .len(),
    5
  );
  let mut extra = managed.clone();
  extra.artifact_id = octacity_server_domain::ArtifactId::generate().to_string();
  extra.destination = "/octacity/protected/extra.json".to_owned();
  assert!(
    materialized.wire_inputs(&[managed, extra]).is_err(),
    "compiler-owned documents must fit the same reserved capacity as candidate and inventory"
  );
  let repeated = factory_protected_validation_materialization(request()).unwrap();
  assert_eq!(
    materialized.wire_inputs(&[]).unwrap(),
    repeated.wire_inputs(&[]).unwrap()
  );
  assert_eq!(materialized.input_digest().unwrap(), repeated.input_digest().unwrap());
  let report = FactoryProtectedValidationReport {
    changeset_id: candidate.id(),
    candidate: candidate.subject().clone(),
    tests: bundle.artifact().clone(),
    input_digest: materialized.input_digest().unwrap(),
    classes: vec![FactoryProtectedValidationClassResult {
      class: key("project.custom"),
      outcome: DeterministicGateOutcome::Passed,
    }],
  };
  materialized.verify_report(&report).unwrap();
  assert_eq!(
    repeated.verify_report(&report).unwrap(),
    DeterministicGateOutcome::Passed
  );
  let mut changed = report.clone();
  changed.tests = FactoryArtifactReference::new(
    report.tests.artifact_id(),
    FactoryDigest::from_bytes([89; 32]),
    report.tests.encoded_size(),
  )
  .unwrap();
  assert!(materialized.verify_report(&changed).is_err());
  let mut changed = report.clone();
  changed.candidate = CandidateSubject::new(
    candidate.subject().exact().clone(),
    octacity_server_domain::ImmutableRevision::new("other").unwrap(),
    FactoryDigest::from_bytes([88; 32]),
  );
  assert!(materialized.verify_report(&changed).is_err());
  let mut missing = report.clone();
  missing.classes.clear();
  assert!(materialized.verify_report(&missing).is_err());
  crate::factory_admission_tests::run_ready(async {
    let validator = std::sync::Arc::new(Builds::new());
    *validator.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
    let report_doc = document_kind(
      &validator,
      requirement.kind(),
      requirement.schema(),
      serde_json::to_vec(&report).unwrap(),
      requirement.tool().clone(),
      EvidenceOutputKind::Report,
    );
    let facts = serde_json::json!({"accepted":true,"input_digest":intent.input().digest().unwrap(),"report_digest":FactoryDigest::content_sha256(&report_doc.bytes),
      "candidate_revision":candidate.subject().candidate_revision(),"candidate_digest":candidate.subject().changeset_digest(),"protected_inventory":bundle.artifact(),
      "covered_classes":bundle.plan().classes(),"fresh_until":500});
    let facts_doc = document_kind(
      &validator,
      facts_requirement.kind(),
      facts_requirement.schema(),
      serde_json::to_vec(&facts).unwrap(),
      facts_requirement.tool().clone(),
      EvidenceOutputKind::Report,
    );
    let public = document_kind(
      &validator,
      &intent.profile().result_output,
      schema.reference(),
      br#"{"count":9}"#.to_vec(),
      intent.profile().tool.clone(),
      EvidenceOutputKind::Report,
    );
    let outputs = FactoryNodeBuildOutputs {
      execution: FactoryNodeBuildExecutionFacts {
        profile: intent.profile().clone(),
        input_digest: intent.input().digest().unwrap(),
        permissions: intent.permissions().clone(),
        budget: intent.budget(),
        deadline: intent.node().deadline(),
      },
      usage: BudgetUsage {
        attempts: 1,
        output_bytes: (public.bytes.len() + report_doc.bytes.len() + facts_doc.bytes.len()) as u64,
        ..Default::default()
      },
      documents: vec![public, report_doc.clone(), facts_doc],
      verified_at: time(30),
      fresh_until: time(900),
    };
    #[derive(Clone)]
    struct Selected(FactoryProtectedValidationMaterialization);
    #[async_trait::async_trait]
    impl FactoryProtectedValidationInputSource for Selected {
      async fn materialization(
        &self,
        _: &FlowBuildIntent,
      ) -> Result<FactoryProtectedValidationMaterialization, ApplicationError> {
        Ok(self.0.clone())
      }
    }
    let request = || FactoryNodeExecutionRequest {
      run_id: intent.input().factory_run_id(),
      admitted: &admitted,
      work: &work,
      input: intent.input(),
      attempt: intent.node(),
      build: Some(&intent),
      ownership: FactoryClaimOwnership::new(intent.node().owner().clone(), intent.node().claim()),
      observed_at: time(31),
    };
    let installed = FactoryProtectedValidationAdapter::new(
      validator.clone(),
      std::sync::Arc::new(crate::factory_node_test_support::Outputs(Some(outputs.clone()))),
      std::sync::Arc::new(Selected(materialized.clone())),
      requirement.clone(),
      facts_requirement.clone(),
    );
    assert!(matches!(
      installed.observe(request()).await.unwrap(),
      FactoryNodeExecutionStep::Completed { .. }
    ));
    let mut substituted = outputs.clone();
    let mut replayed = report.clone();
    replayed.input_digest = FactoryDigest::from_bytes([99; 32]);
    substituted.documents[1] = document_kind(
      &validator,
      requirement.kind(),
      requirement.schema(),
      serde_json::to_vec(&replayed).unwrap(),
      requirement.tool().clone(),
      EvidenceOutputKind::Report,
    );
    let installed = FactoryProtectedValidationAdapter::new(
      validator.clone(),
      std::sync::Arc::new(crate::factory_node_test_support::Outputs(Some(substituted))),
      std::sync::Arc::new(Selected(materialized.clone())),
      requirement.clone(),
      facts_requirement.clone(),
    );
    assert!(
      installed.observe(request()).await.is_err(),
      "a replayed command result must be rejected before the common owner can commit it"
    );
    let adapter = FactoryNodeBuildAdapter::new(
      validator,
      std::sync::Arc::new(crate::factory_node_test_support::Outputs(Some(outputs))),
    );
    let step = adapter
      .observe_or_dispatch(
        &intent,
        &admitted,
        &schema,
        FactoryClaimOwnership::new(intent.node().owner().clone(), intent.node().claim()),
        time(31),
      )
      .await
      .unwrap();
    let FactoryNodeBuildStep::Completed(completed) = step else {
      panic!("validation must complete as an ordinary Build")
    };
    let id = EvidenceManifestId::generate();
    let evidence = construct_factory_protected_validation_evidence_manifest(
      id,
      &materialized,
      &completed,
      &report_doc,
      &requirement,
      &facts_requirement,
      time(33),
    )
    .unwrap();
    assert_eq!(evidence.inputs().len(), 4);
    assert_eq!(evidence.items()[0].outcome(), DeterministicGateOutcome::Passed);
    assert_eq!(evidence.items()[0].fresh_until(), time(500));
    assert!(
      construct_factory_protected_validation_evidence_manifest(
        id,
        &materialized,
        &completed,
        &report_doc,
        &requirement,
        &facts_requirement,
        time(600)
      )
      .is_err(),
      "expired validation qualification cannot be republished as fresh evidence"
    );
    let repeated = construct_factory_protected_validation_evidence_manifest(
      id,
      &repeated,
      &completed,
      &report_doc,
      &requirement,
      &facts_requirement,
      time(33),
    )
    .unwrap();
    assert_eq!(repeated.digest().unwrap(), evidence.digest().unwrap());
    let mut drift = report_doc;
    drift.bytes[0] ^= 1;
    assert!(
      construct_factory_protected_validation_evidence_manifest(
        id,
        &materialized,
        &completed,
        &drift,
        &requirement,
        &facts_requirement,
        time(33)
      )
      .is_err()
    );
  });
}
