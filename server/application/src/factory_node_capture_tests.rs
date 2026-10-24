use crate::factory_node_test_support::{Builds, fixtures::*};
use crate::*;
use octacity_protocol::*;
use octacity_server_artifacts::*;
use octacity_server_factory::*;

fn time(value: i64) -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(value).unwrap()
}

#[test]
fn trusted_capture_rejects_missing_deliverables_despite_successful_execution() {
  let work = work();
  let binding = deliverable_binding(binding(work.subject().project_id()));
  let (_, admitted, _, input, node) =
    execution_fixture_for_work_with_binding(FlowNodeKind::BuildCommand, work, binding, None, None);
  let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
  let builds = Builds::new();
  let execution = FlowBuildExecution::new(
    &intent,
    builds.acceptance.build_id,
    builds.acceptance.attempt_id,
    builds.acceptance.job_ids.clone(),
  )
  .unwrap();
  let outputs = capture_outputs(&intent, &builds, "base");
  assert!(
    verify_factory_node_change_set(&intent, &execution, &outputs, &reference("git.capture"), time(30)).is_err(),
    "a successful model and capture cannot omit required public deliverables"
  );
}

#[test]
fn trusted_capture_rejects_direct_paths_outside_the_frozen_source_write_scope() {
  let work = work();
  let mut binding = binding(work.subject().project_id());
  binding.capture_write_paths = Some(vec![FactoryRepositoryPath::new("public/approved").unwrap()]);
  let (_, admitted, _, input, node) =
    execution_fixture_for_work_with_binding(FlowNodeKind::BuildCommand, work, binding, None, None);
  let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
  let builds = Builds::new();
  let execution = FlowBuildExecution::new(
    &intent,
    builds.acceptance.build_id,
    builds.acceptance.attempt_id,
    builds.acceptance.job_ids.clone(),
  )
  .unwrap();
  let mut outputs = capture_outputs(&intent, &builds, "base");
  assert!(verify_factory_node_change_set(&intent, &execution, &outputs, &reference("git.capture"), time(30)).is_ok());
  update_capture_manifest(&mut outputs, |value| {
    value["changed_paths"] = serde_json::json!([{
      "status":"A", "path":"private/disallowed.txt", "old_mode":"000000", "new_mode":"100644",
      "content_sha256":"a".repeat(64)
    }]);
  });
  assert!(
    verify_factory_node_change_set(&intent, &execution, &outputs, &reference("git.capture"), time(30)).is_err(),
    "valid retained bytes and matching producer pins cannot authorize changes outside the frozen write scope"
  );
}

#[test]
fn restored_legacy_capture_rejects_direct_paths_outside_the_frozen_source_write_scope() {
  crate::factory_admission_tests::run_ready(async {
    let work = work();
    let mut binding = binding(work.subject().project_id());
    let tool = reference("git.capture");
    binding.capture_write_paths = Some(vec![FactoryRepositoryPath::new("public/approved").unwrap()]);
    binding.evidence = [CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT]
      .into_iter()
      .map(|name| {
        EvidenceRequirement::new(
          key(name),
          EvidenceOutputKind::Artifact,
          reference("capture.artifacts"),
          tool.clone(),
          binding.plugin.clone(),
        )
      })
      .collect();
    let (work, admitted, schema, input, node) =
      execution_fixture_for_work_with_binding(FlowNodeKind::Reasoning, work, binding, None, None);
    let intent = FlowBuildIntent::new(input.clone(), &admitted, node.clone(), BudgetUsage::default(), 7).unwrap();
    let builds = std::sync::Arc::new(Builds::new());
    crate::factory_node_build_tests::admit_pending_build(&builds, &intent, &admitted, &schema).await;
    *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
    let mut outputs = capture_outputs(&intent, &builds, "base");
    update_capture_manifest(&mut outputs, |value| {
      value["changed_paths"] = serde_json::json!([{
        "status":"A", "path":"private/disallowed.txt", "old_mode":"000000", "new_mode":"100644",
        "content_sha256":"a".repeat(64)
      }]);
    });
    let manifest = outputs.documents[1].clone();
    let public = crate::factory_node_test_support::document(
      &builds,
      &intent.profile().result_output,
      schema.reference(),
      br#"{"count":9}"#.to_vec(),
      intent.profile().tool.clone(),
    );
    outputs.usage.output_bytes += public.bytes.len() as u64;
    outputs.documents.push(public);
    // The ordinary adapter constructs independently retained legacy facts;
    // this fixture does not use the now-corrected capture verifier to accept them.
    let adapter = FactoryNodeBuildAdapter::new(
      builds.clone(),
      std::sync::Arc::new(crate::factory_node_test_support::Outputs(Some(outputs))),
    );
    let step = adapter
      .observe(FactoryNodeExecutionRequest {
        run_id: input.factory_run_id(),
        admitted: &admitted,
        work: &work,
        input: &input,
        attempt: &node,
        build: Some(&intent),
        ownership: FactoryClaimOwnership::new(node.owner().clone(), node.claim()),
        observed_at: time(31),
      })
      .await
      .unwrap();
    let FactoryNodeExecutionStep::Completed {
      record,
      execution: Some(execution),
      ..
    } = step
    else {
      panic!("the public ordinary boundary must supply the independently retained legacy record");
    };
    assert!(
      restore_factory_node_change_set(&record, &intent, &execution, &manifest, &tool).is_err(),
      "recovery cannot revive legacy capture outside its original frozen source authority"
    );
  });
}
pub(crate) use crate::factory_node_test_support::capture_outputs;

pub(crate) fn deliverable_binding(mut binding: FlowBuildBinding) -> FlowBuildBinding {
  let mut wire = serde_json::to_value(&binding).unwrap();
  wire["capture_deliverables"] = serde_json::json!({"accepted_contract":{"value":{"mapping":"constant","value":"a".repeat(64)}},
    "policy":{"permitted_paths":["docs","src","tests/public","tools"],"forbidden_paths":["tests/private"],
      "required":[{"kind":"docs","paths":["docs"],"min_files":1},{"kind":"tests","paths":["tests/public"],"min_files":1},
        {"kind":"tools","paths":["tools"],"min_files":1}]}});
  binding = serde_json::from_value(wire).unwrap();
  binding
}
pub(crate) fn complete_deliverable_outputs(outputs: &mut FactoryNodeBuildOutputs, public_test_digest: FactoryDigest) {
  update_capture_manifest(outputs, |value| {
    value["changed_paths"]=serde_json::json!(["docs/api.md","src/api.rs","tests/public/api.rs","tools/deploy.sh"].map(|path|
      serde_json::json!({"status":"A","path":path,"old_mode":"000000","new_mode":"100644",
        "content_sha256":if path=="tests/public/api.rs" {public_test_digest}else{FactoryDigest::content_sha256(path.as_bytes())}})));
    value["cumulative_changes"] =
      serde_json::json!({"base_revision":"base","changed_paths":value["changed_paths"].clone()});
  });
}
fn update_capture_manifest(outputs: &mut FactoryNodeBuildOutputs, update: impl FnOnce(&mut serde_json::Value)) {
  let doc = outputs
    .documents
    .iter_mut()
    .find(|doc| doc.record.identity().logical_name.as_str() == CHANGE_SET_MANIFEST_OUTPUT)
    .unwrap();
  let mut value: serde_json::Value = serde_json::from_slice(&doc.bytes).unwrap();
  update(&mut value);
  let previous_size = doc.bytes.len() as u64;
  let manifest: CapturedChangeSetManifestV1 = serde_json::from_value(value).unwrap();
  doc.bytes = serde_json::to_vec(&manifest).unwrap();
  doc.bytes.push(b'\n');
  let mut identity = doc.record.identity().clone();
  identity.size_bytes = doc.bytes.len() as u64;
  identity.digest = ArtifactContentDigest::from_bytes(FactoryDigest::content_sha256(&doc.bytes).as_bytes());
  let record = ArtifactRecord::pending(identity.clone(), time(30)).unwrap();
  let record = record
    .transition(&identity, record.version(), ArtifactEvent::BeginVerification, time(30))
    .unwrap();
  doc.record = record
    .transition(&identity, record.version(), ArtifactEvent::Publish, time(30))
    .unwrap();
  outputs.usage.output_bytes = outputs.usage.output_bytes - previous_size + doc.bytes.len() as u64;
}

#[test]
fn trusted_rework_preserves_cumulative_deliverables_without_requiring_repeated_updates() {
  let work = work();
  let binding = deliverable_binding(binding(work.subject().project_id()));
  let (_, admitted, _, input, node) =
    execution_fixture_for_work_with_binding(FlowNodeKind::Reasoning, work, binding, None, None);
  let selected = CandidateSubject::new(
    input.subject().clone(),
    octacity_server_domain::ImmutableRevision::new("previous-code").unwrap(),
    FactoryDigest::from_bytes([42; 32]),
  );
  let (input, node) = crate::factory_node_test_support::input_with_candidate_context(&admitted, input, &node, selected);
  let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
  let builds = Builds::new();
  let execution = FlowBuildExecution::new(
    &intent,
    builds.acceptance.build_id,
    builds.acceptance.attempt_id,
    builds.acceptance.job_ids.clone(),
  )
  .unwrap();
  let mut outputs = capture_outputs(&intent, &builds, "previous-code");
  complete_deliverable_outputs(&mut outputs, FactoryDigest::content_sha256(b"public unit test"));
  update_capture_manifest(&mut outputs, |value| {
    value["cumulative_changes"] =
      serde_json::json!({"base_revision":"base", "changed_paths": value["changed_paths"].clone()});
    value["changed_paths"] = serde_json::json!([value["changed_paths"][1].clone()]);
  });
  let qualify = |outputs: &FactoryNodeBuildOutputs| {
    verify_factory_node_candidate_deliverables(&intent, &execution, outputs, &reference("git.capture"), time(31))
  };
  qualify(&outputs).expect("a code-only rework retains already captured public deliverables");
  let mut deleted = outputs.clone();
  update_capture_manifest(&mut deleted, |value| {
    value["cumulative_changes"]["changed_paths"]
      .as_array_mut()
      .unwrap()
      .remove(2);
  });
  assert!(
    qualify(&deleted).is_err(),
    "deleted mandatory tests cannot be inherited"
  );
  let mut foreign = outputs.clone();
  update_capture_manifest(&mut foreign, |value| {
    value["cumulative_changes"]["base_revision"] = serde_json::json!("foreign-base");
  });
  assert!(
    qualify(&foreign).is_err(),
    "cumulative observations must use the original exact Work base"
  );
}

#[test]
fn capture_can_keep_a_public_primary_report_and_restore_its_private_candidate_artifacts() {
  crate::factory_admission_tests::run_ready(async {
    let work = work();
    let mut binding = binding(work.subject().project_id());
    let tool = reference("git.capture");
    let artifact_schema = reference("capture.artifacts");
    binding.evidence = vec![CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT]
      .into_iter()
      .map(|name| {
        EvidenceRequirement::new(
          key(name),
          EvidenceOutputKind::Artifact,
          artifact_schema.clone(),
          tool.clone(),
          binding.plugin.clone(),
        )
      })
      .collect();
    let (work, admitted, schema, input, node) =
      execution_fixture_for_work_with_binding(FlowNodeKind::Reasoning, work, binding, None, None);
    let intent = FlowBuildIntent::new(input.clone(), &admitted, node.clone(), BudgetUsage::default(), 7).unwrap();
    let builds = std::sync::Arc::new(Builds::new());
    crate::factory_node_build_tests::admit_pending_build(&builds, &intent, &admitted, &schema).await;
    *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
    let mut outputs = capture_outputs(&intent, &builds, "base");
    // Model reports can precede the trusted Agent capture publications. The
    // common primary timestamp must remain the public report's publication.
    for document in &mut outputs.documents {
      let identity = document.record.identity().clone();
      let pending = ArtifactRecord::pending(identity.clone(), time(31)).unwrap();
      let verifying = pending
        .transition(&identity, pending.version(), ArtifactEvent::BeginVerification, time(31))
        .unwrap();
      document.record = verifying
        .transition(&identity, verifying.version(), ArtifactEvent::Publish, time(31))
        .unwrap();
    }
    outputs.verified_at = time(31);
    let manifest = outputs.documents[1].clone();
    let public = crate::factory_node_test_support::document(
      &builds,
      &intent.profile().result_output,
      schema.reference(),
      br#"{"count":9}"#.to_vec(),
      intent.profile().tool.clone(),
    );
    outputs.usage.output_bytes += public.bytes.len() as u64;
    outputs.documents.push(public);
    let adapter = FactoryNodeCaptureBuildAdapter::new(
      builds.clone(),
      std::sync::Arc::new(crate::factory_node_test_support::Outputs(Some(outputs))),
      tool.clone(),
    );
    let step = adapter
      .observe(FactoryNodeExecutionRequest {
        run_id: input.factory_run_id(),
        admitted: &admitted,
        work: &work,
        input: &input,
        attempt: &node,
        build: Some(&intent),
        ownership: FactoryClaimOwnership::new(node.owner().clone(), node.claim()),
        observed_at: time(32),
      })
      .await
      .unwrap();
    let FactoryNodeExecutionStep::Completed {
      record,
      execution: Some(execution),
      ..
    } = step
    else {
      panic!("expected verified completion")
    };
    assert_eq!(record.payload().value(), &serde_json::json!({"count":9}));
    assert_eq!(
      record.observed_at(),
      time(30),
      "primary publication remains independent of later evidence verification"
    );
    assert!(manifest.record.published_at().unwrap() > record.observed_at());
    let candidate = restore_factory_node_change_set(&record, &intent, &execution, &manifest, &tool).unwrap();
    assert_eq!(candidate.subject().candidate_revision().as_str(), "captured-revision");
    let provenance = record.build_provenance().unwrap();
    let mut late = provenance.outputs().to_vec();
    for output in &mut late {
      if [CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT].contains(&output.name.as_str()) {
        output.verified_at = intent.node().deadline();
        output.fresh_until = time(intent.node().deadline().unix_millis() + 1);
      }
    }
    let late = record
      .clone()
      .with_build_provenance(
        FlowBuildProvenance::new(provenance.profile().clone(), late).unwrap(),
        admitted.closure().definition(input.definition()).unwrap(),
      )
      .unwrap();
    assert!(
      restore_factory_node_change_set(&late, &intent, &execution, &manifest, &tool).is_err(),
      "evidence verified at the exclusive original Node deadline cannot become a candidate"
    );
    let mut changed = manifest;
    changed.bytes[0] ^= 1;
    assert!(restore_factory_node_change_set(&record, &intent, &execution, &changed, &tool).is_err());
  });
}
#[test]
fn capture_accepts_an_arbitrary_node_through_its_frozen_input_and_actual_execution() {
  let (_, admitted, _, input, node) = execution_fixture(FlowNodeKind::BuildCommand);
  let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
  let builds = Builds::new();
  let execution = FlowBuildExecution::new(
    &intent,
    builds.acceptance.build_id,
    builds.acceptance.attempt_id,
    builds.acceptance.job_ids.clone(),
  )
  .unwrap();
  let outputs = capture_outputs(&intent, &builds, intent.input().subject().base_revision().as_str());
  let candidate =
    verify_factory_node_change_set(&intent, &execution, &outputs, &reference("git.capture"), time(31)).unwrap();
  assert_eq!(candidate.node_attempt_id(), intent.node().id());
  assert_eq!(candidate.subject().exact(), intent.input().subject());
  assert_eq!(candidate.subject().candidate_revision().as_str(), "captured-revision");
  assert_eq!(
    candidate.bundle_artifact(),
    outputs.documents[0].record.identity().artifact_id
  );
  assert_eq!(
    candidate.manifest_artifact(),
    outputs.documents[1].record.identity().artifact_id
  );
  assert_eq!(
    verify_factory_node_change_set(&intent, &execution, &outputs, &reference("git.capture"), time(32)).unwrap(),
    candidate
  );
}

#[test]
fn candidate_materialization_requires_the_explicit_frozen_context_and_retained_artifacts() {
  let (_, admitted, _, input, node) = execution_fixture(FlowNodeKind::BuildCommand);
  let intent = FlowBuildIntent::new(input.clone(), &admitted, node, BudgetUsage::default(), 7).unwrap();
  let builds = Builds::new();
  let execution = FlowBuildExecution::new(
    &intent,
    builds.acceptance.build_id,
    builds.acceptance.attempt_id,
    builds.acceptance.job_ids.clone(),
  )
  .unwrap();
  let outputs = capture_outputs(&intent, &builds, input.subject().base_revision().as_str());
  let candidate =
    verify_factory_node_change_set(&intent, &execution, &outputs, &reference("git.capture"), time(31)).unwrap();
  let subject = FactoryTaskSubject::Candidate(candidate.subject().clone());
  let previous = input.context().unwrap();
  let entries = outputs
    .documents
    .iter()
    .map(|doc| {
      let identity = doc.record.identity();
      let artifact = FactoryArtifactReference::new(
        identity.artifact_id,
        FactoryDigest::from_bytes(identity.digest.as_bytes()),
        identity.size_bytes,
      )
      .unwrap();
      ContextManifestEntry::new(
        ContextSourceKind::Evidence,
        key(identity.logical_name.as_str()),
        subject.clone(),
        FactoryContextReference::Artifact(artifact.clone()),
        artifact.content_digest(),
        artifact.encoded_size(),
        previous.entries()[0].inclusion_reason().clone(),
        FactoryDigest::from_bytes([72; 32]),
      )
      .unwrap()
    })
    .collect();
  let context = ContextManifest::new(
    ContextManifestId::generate(),
    subject,
    FactoryDigest::from_bytes([73; 32]),
    entries,
  )
  .unwrap();
  let selected = input.clone().with_context(context, vec![]).unwrap();
  let bundle = &outputs.documents[0].record;
  let manifest = &outputs.documents[1].record;
  let materialization =
    factory_node_candidate_materialization(&selected, &candidate, bundle, manifest, time(31)).unwrap();
  assert_eq!(materialization.change_set_id, candidate.id());
  assert_eq!(materialization.base_revision, input.subject().base_revision().clone());
  let (instruction, inputs) = materialization.wire_inputs();
  assert_eq!(instruction.candidate_revision, "captured-revision");
  assert_eq!(instruction.bundle_input, bundle.identity().artifact_id.to_string());
  assert_eq!(inputs.len(), 2);
  assert!(
    factory_node_candidate_materialization(&input, &candidate, bundle, manifest, time(31)).is_err(),
    "candidate artifacts must not be inferred from a current lifecycle pointer"
  );
  let pending = ArtifactRecord::pending(bundle.identity().clone(), time(30)).unwrap();
  assert!(factory_node_candidate_materialization(&selected, &candidate, &pending, manifest, time(31)).is_err());
  let pinned = selected.context().unwrap();
  let missing_bundle = input.clone().with_context(ContextManifest::new(pinned.id(), pinned.subject().clone(),
    pinned.construction_policy_digest(), pinned.entries().iter().filter(|entry| {
      matches!(entry.source(), FactoryContextReference::Artifact(reference) if reference.artifact_id() == candidate.manifest_artifact())
    }).cloned().collect()).unwrap(), vec![]).unwrap();
  assert!(
    factory_node_candidate_materialization(&missing_bundle, &candidate, bundle, manifest, time(31)).is_err(),
    "a retained artifact cannot be supplied unless the frozen context explicitly selected it"
  );
  let mut changed = bundle.identity().clone();
  changed.artifact_id = octacity_server_domain::ArtifactId::generate();
  let foreign = ArtifactRecord::pending(changed.clone(), time(30)).unwrap();
  let foreign = foreign
    .transition(
      &changed,
      foreign.version(),
      ArtifactEvent::BeginVerification,
      builds.created_at,
    )
    .unwrap();
  let foreign = foreign
    .transition(&changed, foreign.version(), ArtifactEvent::Publish, builds.created_at)
    .unwrap();
  assert!(factory_node_candidate_materialization(&selected, &candidate, &foreign, manifest, time(31)).is_err());
}

#[test]
fn capture_uses_the_candidate_revision_explicitly_selected_in_frozen_context() {
  let (_, admitted, _, input, node) = execution_fixture(FlowNodeKind::BuildCommand);
  let selected = CandidateSubject::new(
    input.subject().clone(),
    octacity_server_domain::ImmutableRevision::new("selected-predecessor").unwrap(),
    FactoryDigest::from_bytes([92; 32]),
  );
  let (input, node) = crate::factory_node_test_support::input_with_candidate_context(&admitted, input, &node, selected);
  let intent = FlowBuildIntent::new(input, &admitted, node, BudgetUsage::default(), 7).unwrap();
  let builds = Builds::new();
  let execution = FlowBuildExecution::new(
    &intent,
    builds.acceptance.build_id,
    builds.acceptance.attempt_id,
    builds.acceptance.job_ids.clone(),
  )
  .unwrap();
  let outputs = capture_outputs(&intent, &builds, "selected-predecessor");
  let candidate =
    verify_factory_node_change_set(&intent, &execution, &outputs, &reference("git.capture"), time(31)).unwrap();
  assert_eq!(candidate.subject().candidate_revision().as_str(), "captured-revision");
  let original_base = capture_outputs(&intent, &builds, intent.input().subject().base_revision().as_str());
  assert!(
    verify_factory_node_change_set(&intent, &execution, &original_base, &reference("git.capture"), time(31)).is_err()
  );
  assert!(
    verify_factory_node_change_set(
      &intent,
      &execution,
      &outputs,
      &reference("other.capture.tool"),
      time(31)
    )
    .is_err()
  );
  let mut substituted = outputs.clone();
  substituted.execution.input_digest = FactoryDigest::from_bytes([93; 32]);
  assert!(
    verify_factory_node_change_set(&intent, &execution, &substituted, &reference("git.capture"), time(31)).is_err()
  );
  let mut substituted = outputs.clone();
  substituted.documents[0].bytes.push(0);
  assert!(
    verify_factory_node_change_set(&intent, &execution, &substituted, &reference("git.capture"), time(31)).is_err()
  );
}

fn capture_schema() -> FlowDataSchema {
  let text = || FlowValueSchema::String {
    min_bytes: 1,
    max_bytes: 256,
  };
  let object = |fields: Vec<(&str, FlowValueSchema)>| FlowValueSchema::Object {
    fields: fields
      .into_iter()
      .map(|(name, value)| (key(name), FlowFieldSchema::required(value)))
      .collect(),
  };
  let file = object(vec![
    ("name", text()),
    (
      "size_bytes",
      FlowValueSchema::Integer {
        minimum: 0,
        maximum: 10000,
      },
    ),
    ("sha256", text()),
  ]);
  FlowDataSchema::new(
    key("test.capture.manifest"),
    key("v1"),
    object(vec![
      (
        "format_version",
        FlowValueSchema::Enum {
          values: vec![serde_json::json!(1)],
        },
      ),
      ("base_revision", text()),
      ("candidate_revision", text()),
      ("node_attempt_id", text()),
      (
        "capture_tool",
        object(vec![("identity", text()), ("version", text()), ("sha256", text())]),
      ),
      (
        "changed_paths",
        FlowValueSchema::Array {
          item: Box::new(FlowValueSchema::Boolean),
          min_items: 0,
          max_items: 0,
        },
      ),
      ("bundle", file),
    ]),
  )
  .unwrap()
}

#[test]
fn configured_capture_executor_returns_the_verified_manifest_through_the_common_node_contract() {
  crate::factory_admission_tests::run_ready(async {
    for kind in [FlowNodeKind::BuildCommand, FlowNodeKind::Reasoning] {
      let work = work();
      let mut binding = binding(work.subject().project_id());
      let capture_tool = reference("git.capture");
      if kind == FlowNodeKind::BuildCommand {
        binding.tool = capture_tool.clone();
      } else {
        // A reasoning harness remains the executed capability; the independent
        // trusted capture tool supplies its primary verified result.
        let mut wire = serde_json::to_value(&binding).unwrap();
        wire["result_tool"] = serde_json::to_value(&capture_tool).unwrap();
        binding = serde_json::from_value(wire).expect("independent result verifier must be configurable");
      }
      binding.result_output = key(CHANGE_SET_MANIFEST_OUTPUT);
      binding.result_output_kind = EvidenceOutputKind::Artifact;
      binding.evidence = vec![EvidenceRequirement::new(
        key(CHANGE_SET_BUNDLE_OUTPUT),
        EvidenceOutputKind::Artifact,
        reference("capture.artifacts"),
        capture_tool.clone(),
        binding.plugin.clone(),
      )];
      let template: FlowDefinitionTemplate = crate::factory_node_test_support::fixtures::supplied_base_flow_template();
      let handoff_schema = template.data_schemas().unwrap().remove(&key("spec_handoff")).unwrap();
      if kind == FlowNodeKind::Reasoning {
        binding.evidence.push(EvidenceRequirement::new(
          key("specification_handoff"),
          EvidenceOutputKind::Report,
          handoff_schema.reference().clone(),
          binding.tool.clone(),
          binding.plugin.clone(),
        ));
      }
      let (work, mut admitted, schema, input, node) =
        execution_fixture_for_work_with_binding(kind, work, binding, None, Some(capture_schema()));
      let mut contracts = admitted.data_schemas().to_vec();
      contracts.push(handoff_schema.clone());
      admitted = admitted.with_data_schemas(contracts).unwrap();
      let (input, node) = if kind == FlowNodeKind::Reasoning {
        let predecessor = CandidateSubject::new(
          input.subject().clone(),
          octacity_server_domain::ImmutableRevision::new("research-candidate").unwrap(),
          FactoryDigest::from_bytes([92; 32]),
        );
        crate::factory_node_test_support::input_with_candidate_context(&admitted, input, &node, predecessor)
      } else {
        (input, node)
      };
      let capture_base = if kind == FlowNodeKind::Reasoning {
        "research-candidate"
      } else {
        input.subject().base_revision().as_str()
      };
      let intent = FlowBuildIntent::new(input.clone(), &admitted, node.clone(), BudgetUsage::default(), 7).unwrap();
      let builds = std::sync::Arc::new(Builds::new());
      crate::factory_node_build_tests::admit_pending_build(&builds, &intent, &admitted, &schema).await;
      *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
      let mut outputs = capture_outputs(&intent, &builds, capture_base);
      outputs.documents[1].schema = schema.reference().clone();
      if kind == FlowNodeKind::Reasoning {
        let handoff = serde_json::json!({"work_id":work.id().to_string(), "original_base":input.subject().base_revision().as_str(), "predecessor_revision":capture_base, "candidate_revision":"captured-revision", "input_digest":input.digest().unwrap().to_string(), "work_kind":"feature_request", "contract_path":"openspec/specs/example/spec.md", "change_path":"openspec/changes/example/proposal.md", "acceptance_criteria":["new behavior has an exact contract"], "regression_criteria":[],"declared_requirements":[]});
        let mut document = crate::factory_node_test_support::document(
          &builds,
          &key("specification_handoff"),
          handoff_schema.reference(),
          serde_json::to_vec(&handoff).unwrap(),
          intent.profile().tool.clone(),
        );
        document.plugin = intent.profile().plugin.clone();
        outputs.usage.output_bytes += document.bytes.len() as u64;
        outputs.documents.push(document);
      }
      let accepted_outputs = outputs.clone();
      let execution = FlowBuildExecution::new(
        &intent,
        builds.acceptance.build_id,
        builds.acceptance.attempt_id,
        builds.acceptance.job_ids.clone(),
      )
      .unwrap();
      let expected =
        verify_factory_node_change_set(&intent, &execution, &outputs, &reference("git.capture"), time(31)).unwrap();
      let adapter = FactoryNodeCaptureBuildAdapter::new(
        builds.clone(),
        std::sync::Arc::new(crate::factory_node_test_support::Outputs(Some(outputs))),
        reference("git.capture"),
      );
      let result = FactoryNodeExecutor::observe(
        &adapter,
        FactoryNodeExecutionRequest {
          run_id: input.factory_run_id(),
          work: &work,
          admitted: &admitted,
          input: &input,
          attempt: &node,
          build: Some(&intent),
          ownership: FactoryClaimOwnership::new(node.owner().clone(), node.claim()),
          observed_at: time(31),
        },
      )
      .await
      .unwrap();
      let FactoryNodeExecutionStep::Completed {
        execution: Some(execution),
        record,
        usage,
        ..
      } = result
      else {
        panic!("trusted capture must publish a common result rather than a lifecycle checkpoint");
      };
      assert_eq!(expected.subject().exact(), work.subject());
      assert_eq!(
        record.discovery().and_then(FlowDiscoveryFacts::published_candidate),
        Some(expected.subject()),
        "a completed trusted capture must publish its exact candidate for downstream context selection"
      );
      assert_eq!(record.payload().value()["base_revision"], capture_base);
      assert_eq!(execution.node_attempt_id(), node.id());
      assert_eq!(
        builds.requests.lock().unwrap()[0].causality.profile.tool,
        intent.profile().tool
      );
      assert_eq!(record.node_attempt_id(), node.id());
      assert_eq!(
        record.payload().value()["candidate_revision"],
        serde_json::json!("captured-revision")
      );
      assert_eq!(
        record.payload().value()["node_attempt_id"],
        serde_json::json!(node.id().to_string())
      );
      assert_eq!(
        record.build_provenance().unwrap().outputs().len(),
        if kind == FlowNodeKind::Reasoning { 3 } else { 2 }
      );
      if kind == FlowNodeKind::Reasoning {
        let handoff = record
          .build_provenance()
          .unwrap()
          .outputs()
          .iter()
          .find(|output| output.name == key("specification_handoff"))
          .unwrap();
        assert_eq!(
          handoff.data.as_ref().unwrap().payload().value()["work_id"],
          work.id().to_string()
        );
      }
      assert!(usage.output_bytes > 0);
      assert_eq!(builds.requests.lock().unwrap().len(), 1);
      let definition = admitted.closure().definition(input.definition()).unwrap();
      let completion = record
        .completion(
          &node,
          definition,
          FactoryClaimOwnership::new(node.owner().clone(), node.claim()),
          usage,
          time(32),
        )
        .unwrap();
      let restored = FlowNodeRecord::restore(
        &serde_json::to_vec(&record).unwrap(),
        &input,
        &node,
        definition,
        &schema,
        &completion,
      )
      .unwrap();
      assert_eq!(
        factory_node_change_set(&restored, &intent, &execution, &reference("git.capture")).unwrap(),
        expected
      );
      assert!(factory_node_change_set(&restored, &intent, &execution, &reference("another.capture")).is_err());
      let foreign = FlowBuildExecution::new(
        &intent,
        octacity_server_domain::BuildId::generate(),
        execution.attempt_id(),
        execution.jobs().to_vec(),
      )
      .unwrap();
      assert!(factory_node_change_set(&restored, &intent, &foreign, &reference("git.capture")).is_err());
      if kind == FlowNodeKind::Reasoning {
        for omitted in [
          CHANGE_SET_BUNDLE_OUTPUT,
          CHANGE_SET_MANIFEST_OUTPUT,
          "specification_handoff",
        ] {
          let mut incomplete = accepted_outputs.clone();
          incomplete
            .documents
            .retain(|doc| doc.record.identity().logical_name.as_str() != omitted);
          let incomplete = FactoryNodeCaptureBuildAdapter::new(
            builds.clone(),
            std::sync::Arc::new(crate::factory_node_test_support::Outputs(Some(incomplete))),
            reference("git.capture"),
          );
          assert!(
            FactoryNodeExecutor::observe(
              &incomplete,
              FactoryNodeExecutionRequest {
                run_id: input.factory_run_id(),
                work: &work,
                admitted: &admitted,
                input: &input,
                attempt: &node,
                build: Some(&intent),
                ownership: FactoryClaimOwnership::new(node.owner().clone(), node.claim()),
                observed_at: time(31)
              }
            )
            .await
            .is_err(),
            "{omitted} cannot be replaced by a model's prose or handoff"
          );
        }
      }
    }
  });
}

#[test]
fn partial_capture_retry_uses_the_frozen_base_and_restores_the_candidate_from_the_common_journal() {
  use octacity_server_store::{testing, *};
  use std::sync::Arc;
  crate::factory_admission_tests::run_ready(async {
    let work = work();
    let mut selection = binding(work.subject().project_id());
    selection.tool = reference("git.capture");
    selection.result_output = key(CHANGE_SET_MANIFEST_OUTPUT);
    selection.result_output_kind = EvidenceOutputKind::Artifact;
    selection.evidence = vec![EvidenceRequirement::new(
      key(CHANGE_SET_BUNDLE_OUTPUT),
      EvidenceOutputKind::Artifact,
      reference("capture.artifacts"),
      selection.tool.clone(),
      selection.plugin.clone(),
    )];
    let (work, admitted, schema, input, initial) = execution_fixture_with_retry_policy(
      FlowNodeKind::BuildCommand,
      work,
      selection,
      None,
      Some(capture_schema()),
      1,
    );
    let run = FactoryRun::admitted(input.factory_run_id(), &work);
    let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
    let audit = testing::management_mutation_with_request((), "capture-retry")
      .audit()
      .clone();
    store
      .seed_factory_run(
        PublishedFactoryAdmission {
          work,
          run: run.clone(),
          flow: admitted,
          admitted_at: time(1),
        },
        FactoryDigest::from_bytes([1; 32]),
        &audit,
      )
      .unwrap();
    let claim = FactoryRunClaimRecord::new(run.id(), initial.owner().clone(), initial.claim());
    store
      .claim_factory_run(ClaimFactoryRun {
        run_id: run.id(),
        expected_version: run.version(),
        record: claim.clone(),
        audit: FactoryAuditFact::new(
          run.id(),
          AuditActorKind::Worker,
          None,
          key("factory.claimed"),
          FactoryDigest::from_bytes([2; 32]),
          key("accepted"),
          time(10),
        ),
      })
      .await
      .unwrap();
    let target = FactoryNodeTarget {
      flow_run_id: input.flow_run_id(),
      cycle_id: input.cycle_id(),
      node: input.node().clone(),
    };
    // A failed ordinary Build may have uploaded partial capture bytes. They are
    // never read or promoted to a semantic result by the common capture executor.
    struct FailedOutputs;
    #[async_trait::async_trait]
    impl FactoryNodeBuildOutputSource for FailedOutputs {
      async fn terminal_usage(
        &self,
        _: octacity_server_domain::BuildId,
        _: octacity_server_domain::AttemptId,
      ) -> Result<BudgetUsage, OrdinaryBuildApplicationError> {
        Ok(BudgetUsage {
          output_bytes: 11,
          ..Default::default()
        })
      }
      async fn published_outputs(
        &self,
        _: octacity_server_domain::BuildId,
        _: octacity_server_domain::AttemptId,
        _: u64,
      ) -> Result<FactoryNodeBuildOutputObservation, OrdinaryBuildApplicationError> {
        panic!("partial capture bytes from a failed execution cannot become a candidate")
      }
    }
    let failed_build = Arc::new(Builds::new());
    *failed_build.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Failed;
    let first = FactoryNodeRunner::new(
      store.clone(),
      Arc::new(FactoryNodeCaptureBuildAdapter::new(
        failed_build.clone(),
        Arc::new(FailedOutputs),
        reference("git.capture"),
      )),
      crate::factory_node_test_support::clock(),
    );
    assert_eq!(
      first
        .run_once(run.id(), claim.id, target.clone(), Some(input.clone()), time(11))
        .await
        .unwrap(),
      FactoryNodeStep::Advanced
    );
    assert_eq!(
      first
        .run_once(run.id(), claim.id, target.clone(), None, time(31))
        .await
        .unwrap(),
      FactoryNodeStep::Waiting
    );
    assert_eq!(
      first
        .run_once(run.id(), claim.id, target.clone(), None, time(31))
        .await
        .unwrap(),
      FactoryNodeStep::ExecutionStopped
    );
    let stopped = store.factory_run_snapshot(run.id()).await.unwrap();
    assert!(stopped.flow.data.records.is_empty());
    assert_eq!(stopped.flow.data.stops.len(), 1);
    let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
    let grant = ManagementAuthorizationGrant::new(ManagementVisibility::all());
    FactoryControlHandlers::new(store.clone())
      .execute_management_command(
        &context,
        &grant,
        RetryFactoryNodeCommand {
          run_id: run.id(),
          node_attempt_id: stopped.flow.attempts[0].id(),
          expected_version: stopped.run.version(),
          idempotency_key: IdempotencyKey::new("capture-exact-retry").unwrap(),
          requested_at: time(32),
        },
      )
      .await
      .unwrap();
    let requested = store.factory_run_snapshot(run.id()).await.unwrap();
    let successor = FactoryRunClaimRecord::new(
      run.id(),
      initial.owner().clone(),
      FactoryClaim::new(
        FactoryClaimFence::new(FactoryDigest::from_bytes([3; 32])),
        time(33),
        time(200),
      )
      .unwrap(),
    );
    store
      .claim_factory_run(ClaimFactoryRun {
        run_id: run.id(),
        expected_version: requested.run.version(),
        record: successor.clone(),
        audit: FactoryAuditFact::new(
          run.id(),
          AuditActorKind::Worker,
          None,
          key("factory.claimed"),
          FactoryDigest::from_bytes([3; 32]),
          key("accepted"),
          time(33),
        ),
      })
      .await
      .unwrap();
    let successful = Arc::new(Builds::new_at(time(40)));
    *successful.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
    let owner = FactoryNodeRunner::new(
      store.clone(),
      Arc::new(FactoryNodeCaptureBuildAdapter::new(
        successful.clone(),
        Arc::new(crate::factory_node_test_support::Outputs(None)),
        reference("git.capture"),
      )),
      crate::factory_node_test_support::clock(),
    );
    assert_eq!(
      owner
        .run_once(run.id(), successor.id, target.clone(), None, time(34))
        .await
        .unwrap(),
      FactoryNodeStep::Advanced
    );
    let retried = store.factory_run_snapshot(run.id()).await.unwrap();
    let intent = retried
      .flow
      .data
      .build_intents
      .iter()
      .find(|row| row.node().retry_of().is_some())
      .unwrap();
    assert_eq!(intent.input(), &input.retry().unwrap());
    let mut outputs = capture_outputs(intent, &successful, input.subject().base_revision().as_str());
    outputs.documents[1].schema = schema.reference().clone();
    let recovered = FactoryNodeRunner::new(
      store.clone(),
      Arc::new(FactoryNodeCaptureBuildAdapter::new(
        successful.clone(),
        Arc::new(crate::factory_node_test_support::Outputs(Some(outputs))),
        reference("git.capture"),
      )),
      crate::factory_node_test_support::clock(),
    );
    assert_eq!(
      recovered
        .run_once(run.id(), successor.id, target.clone(), None, time(41))
        .await
        .unwrap(),
      FactoryNodeStep::Waiting
    );
    assert!(matches!(
      recovered
        .run_once(run.id(), successor.id, target.clone(), None, time(41))
        .await
        .unwrap(),
      FactoryNodeStep::Completed(_)
    ));
    let completed = store.factory_run_snapshot(run.id()).await.unwrap();
    let record = &completed.flow.data.records[0];
    let execution = completed
      .flow
      .data
      .build_executions
      .iter()
      .find(|row| row.node_attempt_id() == record.node_attempt_id())
      .unwrap();
    let candidate = factory_node_change_set(record, intent, execution, &reference("git.capture")).unwrap();
    assert_eq!(candidate.subject().exact(), input.subject());
    assert_eq!(candidate.subject().candidate_revision().as_str(), "captured-revision");
    assert_eq!(completed.flow.data.stops, stopped.flow.data.stops);
    assert_eq!(completed.flow.attempts.len(), 2);
    for builds in [&failed_build, &successful] {
      let requests = builds.requests.lock().unwrap();
      assert_eq!(requests.len(), 1);
      assert_eq!(requests[0].immutable_revision, input.subject().base_revision().clone());
    }
    assert!(matches!(
      recovered
        .run_once(run.id(), successor.id, target, None, time(42))
        .await
        .unwrap(),
      FactoryNodeStep::Completed(_)
    ));
    assert_eq!(store.factory_run_snapshot(run.id()).await.unwrap(), completed);
    assert_eq!(successful.requests.lock().unwrap().len(), 1);
  });
}

#[test]
fn candidate_context_keeps_binary_source_inputs_out_of_declared_model_text() {
  crate::factory_admission_tests::run_ready(async {
    let fixture = candidate_context_fixture().await;
    assert_eq!(fixture.context.input_digest(), fixture.intent.input().digest().unwrap());
    assert_eq!(
      fixture.intent,
      FlowBuildIntent::new(
        fixture.intent.input().clone(),
        &fixture.admitted,
        fixture.intent.node().clone(),
        fixture.intent.usage_before(),
        fixture.intent.priority()
      )
      .unwrap()
    );
    assert!(!fixture.instructions.as_str().is_empty());
    let persisted = octacity_server_store::FactoryRunStore::factory_run_snapshot(
      fixture.runs.as_ref(),
      fixture.intent.input().factory_run_id(),
    )
    .await
    .unwrap();
    assert!(persisted.flow.data.build_intents.contains(&fixture.intent));
    assert_eq!(persisted.flow.data.records.len(), 1);
    let producer = &persisted.flow.data.records[0];
    let producer_intent = persisted
      .flow
      .data
      .build_intents
      .iter()
      .find(|row| row.node().id() == producer.node_attempt_id())
      .unwrap();
    let producer_execution = persisted
      .flow
      .data
      .build_executions
      .iter()
      .find(|row| row.node_attempt_id() == producer.node_attempt_id())
      .unwrap();
    let recovered = fixture
      .source
      .read(ReadFactoryCandidateContext {
        input: fixture.intent.input(),
        producer_record: producer,
        producer_intent,
        producer_execution,
        capture_tool: &reference("git.capture"),
        observed_at: time(32),
        required_until: time(100),
      })
      .await
      .unwrap();
    assert_eq!(recovered.input_digest(), fixture.context.input_digest());
    assert_eq!(
      fixture.context.model_context(),
      &std::collections::BTreeMap::from([(fixture.text.record.identity().artifact_id, fixture.text.bytes)])
    );
    assert_eq!(fixture.context.bundle().identity().artifact_id, fixture.bundle_id);
    assert_eq!(fixture.context.manifest().identity().artifact_id, fixture.manifest_id);
    let _opaque_verified_proof = fixture.context.candidate().clone();
    assert!(
      !fixture.calls.contains(&fixture.bundle_id),
      "the binary bundle is never server model-context I/O"
    );
    assert!(fixture.calls.contains(&fixture.manifest_id));
    assert!(fixture.calls.contains(&fixture.text.record.identity().artifact_id));
    assert_eq!(fixture.calls.len(), 2);
  });
}

pub(super) async fn candidate_context_fixture() -> crate::factory_candidate_context_test_support::CandidateContextFixture
{
  use crate::factory_candidate_context_test_support::{ReadonlyCandidateFixtureProfile, candidate_context_fixture};
  use crate::factory_codex_tests::{credential_profiles, permissions};
  let work = work();
  let plugin = ImmutableReference::new(key("codex"), key("v1"), FactoryDigest::from_bytes([10; 32]));
  let executable = ImmutableReference::new(key("codex-cli"), key("v1"), FactoryDigest::from_bytes([11; 32]));
  candidate_context_fixture(ReadonlyCandidateFixtureProfile {
    build_configuration: binding(work.subject().project_id()).build_configuration,
    work,
    permissions: permissions(FactoryTaskMode::Evaluate, plugin.clone(), executable.clone(), true),
    toolchain: FactoryTaskToolchain {
      plugin,
      executable,
      model: ImmutableReference::new(key("an-exact-model"), key("v1"), FactoryDigest::from_bytes([12; 32])),
    },
    credentials: credential_profiles(),
    instructions: FactoryText::new("Measure the declared candidate and return the configured observation.").unwrap(),
  })
  .await
}

#[test]
fn stored_candidate_context_recovers_only_the_persisted_selected_producer() {
  crate::factory_admission_tests::run_ready(async {
    let fixture = candidate_context_fixture().await;
    let source = FactoryStoredCandidateContextSource::new(
      fixture.runs.clone(),
      fixture.source.clone(),
      reference("git.capture"),
      crate::factory_node_test_support::clock(),
    );
    let context = source
      .read_candidate_context(
        &fixture.intent,
        &FactoryClaimOwnership::new(fixture.intent.node().owner().clone(), fixture.intent.node().claim()),
        time(32),
        fixture.intent.node().deadline(),
      )
      .await
      .unwrap();
    assert_eq!(context.input_digest(), fixture.intent.input().digest().unwrap());
    assert_eq!(context.bundle(), fixture.context.bundle());
    assert_eq!(context.manifest(), fixture.context.manifest());
    assert_eq!(context.model_context(), fixture.context.model_context());
  });
}

fn installed_candidate_profile(
  fixture: &crate::factory_candidate_context_test_support::CandidateContextFixture,
) -> FactoryCodexBuildProfile {
  let frozen = fixture.intent.profile();
  FactoryCodexBuildProfile {
    mcp_configurations: vec![],
    toolchain: FactoryTaskToolchain {
      plugin: frozen.plugin.clone(),
      executable: frozen.tool.clone(),
      model: frozen.model_or_tool.clone(),
    },
    build_configuration: frozen.build_configuration.clone(),
    instructions: fixture.instructions.clone(),
    credentials: crate::factory_codex_tests::credential_profiles(),
    consumer: FactoryCredentialConsumer::ReadOnlyReview,
    usage_policy: FactoryCodexUsagePolicy::ReportedCost,
    tool_risk: None,
    tool_response_limits: None,
    reviewer: None,
    candidate_capture_tool: Some(reference("git.capture")),
    capture: None,
    pipeline_node: octacity_server_domain::PipelineNodeId::new("explicit-consumer-94").unwrap(),
    parameters: std::collections::BTreeMap::new(),
    project_permissions: fixture.intent.permissions().clone(),
    local_permissions: LocalPermissionCeiling::fully_enforced(fixture.intent.permissions().clone()),
  }
}

#[test]
fn installed_codex_candidate_profile_uses_one_explicit_capture_pin_for_dispatch_and_observation() {
  crate::factory_admission_tests::run_ready(async {
    let fixture = candidate_context_fixture().await;
    let profile = installed_candidate_profile(&fixture);
    let source = FactoryCodexCandidateContextSource::new(
      fixture.runs.clone(),
      fixture.source.clone(),
      vec![profile],
      crate::factory_node_test_support::clock(),
    )
    .unwrap();
    let dispatch = source
      .read_candidate_context(
        &fixture.intent,
        &FactoryClaimOwnership::new(fixture.intent.node().owner().clone(), fixture.intent.node().claim()),
        time(32),
        fixture.intent.node().deadline(),
      )
      .await
      .unwrap();
    assert_eq!(dispatch.input_digest(), fixture.intent.input().digest().unwrap());
    assert_eq!(dispatch.bundle(), fixture.context.bundle());
    assert_eq!(dispatch.model_context(), fixture.context.model_context());
    let observation = source
      .read_retained_candidate_context(&fixture.intent, time(2000), time(2001))
      .await
      .unwrap();
    assert_eq!(observation.manifest(), fixture.context.manifest());
    assert_eq!(observation.input_digest(), dispatch.input_digest());
    assert_eq!(observation.model_context(), dispatch.model_context());
  });
}

#[test]
fn installed_candidate_source_rejects_absent_unknown_or_ambiguous_profiles_before_io() {
  struct NoIoClock;
  impl FactoryExecutionClock for NoIoClock {
    fn now(&self) -> Result<octacity_server_domain::Timestamp, ApplicationError> {
      panic!("profile rejection must precede trusted reader I/O")
    }
  }
  crate::factory_admission_tests::run_ready(async {
    let fixture = candidate_context_fixture().await;
    let profile = installed_candidate_profile(&fixture);
    let mut disabled = profile.clone();
    disabled.candidate_capture_tool = None;
    let mut unknown = profile.clone();
    unknown.toolchain.model = reference("other-installed-model");
    for profiles in [vec![disabled], vec![unknown], vec![profile.clone(), profile]] {
      let source = FactoryCodexCandidateContextSource::new(
        fixture.runs.clone(),
        fixture.source.clone(),
        profiles,
        std::sync::Arc::new(NoIoClock),
      )
      .unwrap();
      let ownership = FactoryClaimOwnership::new(fixture.intent.node().owner().clone(), fixture.intent.node().claim());
      assert!(
        source
          .read_candidate_context(&fixture.intent, &ownership, time(32), fixture.intent.node().deadline())
          .await
          .is_err()
      );
      assert!(
        source
          .read_retained_candidate_context(&fixture.intent, time(2000), time(2001))
          .await
          .is_err()
      );
    }
  });
}

#[test]
fn retained_candidate_context_survives_the_execution_deadline_without_renewing_authority() {
  struct ObservationClock;
  impl FactoryExecutionClock for ObservationClock {
    fn now(&self) -> Result<octacity_server_domain::Timestamp, ApplicationError> {
      Ok(time(2000))
    }
  }
  crate::factory_admission_tests::run_ready(async {
    let fixture = candidate_context_fixture().await;
    assert!(time(2000) > fixture.intent.node().deadline());
    let source = FactoryStoredCandidateContextSource::new(
      fixture.runs.clone(),
      fixture.source.clone(),
      reference("git.capture"),
      std::sync::Arc::new(ObservationClock),
    );
    let before = octacity_server_store::FactoryRunStore::factory_run_snapshot(
      fixture.runs.as_ref(),
      fixture.intent.input().factory_run_id(),
    )
    .await
    .unwrap();
    let context = source
      .read_retained_candidate_context(&fixture.intent, time(2000), time(2001))
      .await
      .expect("retained source verification requires no renewed execution claim");
    assert_eq!(context.input_digest(), fixture.intent.input().digest().unwrap());
    assert_eq!(context.bundle(), fixture.context.bundle());
    assert_eq!(context.manifest(), fixture.context.manifest());
    assert_eq!(context.model_context(), fixture.context.model_context());
    assert_eq!(
      octacity_server_store::FactoryRunStore::factory_run_snapshot(
        fixture.runs.as_ref(),
        fixture.intent.input().factory_run_id(),
      )
      .await
      .unwrap(),
      before,
      "readonly recovery cannot acquire or renew execution authority"
    );
  });
}

#[test]
fn retained_candidate_observation_accepts_io_duration_within_actual_artifact_retention() {
  use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
  };
  struct AdvancingClock(AtomicUsize);
  impl FactoryExecutionClock for AdvancingClock {
    fn now(&self) -> Result<octacity_server_domain::Timestamp, ApplicationError> {
      Ok(time(if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
        2000
      } else {
        2002
      }))
    }
  }
  crate::factory_admission_tests::run_ready(async {
    let fixture = candidate_context_fixture().await;
    let source = FactoryStoredCandidateContextSource::new(
      fixture.runs.clone(),
      fixture.source.clone(),
      reference("git.capture"),
      Arc::new(AdvancingClock(AtomicUsize::new(0))),
    );
    let context = source
      .read_retained_candidate_context(&fixture.intent, time(2000), time(2001))
      .await
      .expect("a minimum retention guarantee must not become a one-millisecond I/O timeout");
    assert_eq!(context.input_digest(), fixture.intent.input().digest().unwrap());
    assert_eq!(context.model_context(), fixture.context.model_context());
  });
}

#[test]
fn stored_candidate_context_rejects_short_retention_foreign_capture_and_late_owner_expiry() {
  use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
  };
  struct ExpiringClock(AtomicUsize);
  impl FactoryExecutionClock for ExpiringClock {
    fn now(&self) -> Result<octacity_server_domain::Timestamp, ApplicationError> {
      Ok(time(if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
        32
      } else {
        100
      }))
    }
  }
  crate::factory_admission_tests::run_ready(async {
    let fixture = candidate_context_fixture().await;
    let ownership = FactoryClaimOwnership::new(fixture.intent.node().owner().clone(), fixture.intent.node().claim());
    let source = FactoryStoredCandidateContextSource::new(
      fixture.runs.clone(),
      fixture.source.clone(),
      reference("git.capture"),
      crate::factory_node_test_support::clock(),
    );
    assert!(
      source
        .read_candidate_context(&fixture.intent, &ownership, time(32), time(100))
        .await
        .is_err(),
      "retention cannot stop before the original Job deadline"
    );
    let foreign = FactoryStoredCandidateContextSource::new(
      fixture.runs.clone(),
      fixture.source.clone(),
      reference("different.operator.capture"),
      crate::factory_node_test_support::clock(),
    );
    assert!(
      foreign
        .read_candidate_context(&fixture.intent, &ownership, time(32), fixture.intent.node().deadline())
        .await
        .is_err(),
      "stored facts do not authorize another installed capture implementation"
    );
    let expired = FactoryStoredCandidateContextSource::new(
      fixture.runs.clone(),
      fixture.source.clone(),
      reference("git.capture"),
      Arc::new(ExpiringClock(AtomicUsize::new(0))),
    );
    assert!(
      expired
        .read_candidate_context(&fixture.intent, &ownership, time(32), fixture.intent.node().deadline())
        .await
        .is_err(),
      "owner expiry during asynchronous context I/O is checked at actual time"
    );
  });
}
