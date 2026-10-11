use crate::factory_node_test_support::{Builds, fixtures::*};
use crate::*;
use octacity_protocol::*;
use octacity_server_artifacts::*;
use octacity_server_factory::*;

fn time(value: i64) -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(value).unwrap()
}
fn capture_outputs(intent: &FlowBuildIntent, builds: &Builds, base: &str) -> FactoryNodeBuildOutputs {
  let tool = reference("git.capture");
  let bundle = b"trusted fixture bundle\n".to_vec();
  let manifest = CapturedChangeSetManifestV1 {
    format_version: 1,
    base_revision: base.to_owned(),
    candidate_revision: "captured-revision".to_owned(),
    node_attempt_id: intent.node().id().to_string(),
    capture_tool: FactoryImmutableReferenceV3 {
      identity: tool.identity().as_str().to_owned(),
      version: tool.version().as_str().to_owned(),
      sha256: tool.digest().to_string(),
    },
    changed_paths: vec![],
    bundle: CapturedChangeSetFileV1 {
      name: CHANGE_SET_BUNDLE_OUTPUT.to_owned(),
      size_bytes: bundle.len() as u64,
      sha256: FactoryDigest::content_sha256(&bundle).to_string(),
    },
    patch: None,
  };
  let mut manifest = serde_json::to_vec(&manifest).unwrap();
  manifest.push(b'\n');
  let lease = octacity_server_domain::LeaseId::generate();
  let schema = reference("capture.artifacts");
  let documents = [
    (CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_BUNDLE_MEDIA_TYPE, bundle),
    (CHANGE_SET_MANIFEST_OUTPUT, CHANGE_SET_MANIFEST_MEDIA_TYPE, manifest),
  ]
  .into_iter()
  .map(|(name, media, bytes)| {
    let mut doc = crate::factory_node_test_support::document_kind(
      builds,
      &key(name),
      &schema,
      bytes,
      tool.clone(),
      EvidenceOutputKind::Artifact,
    );
    doc.plugin = intent.profile().plugin.clone();
    let mut identity = doc.record.identity().clone();
    identity.lease_id = lease;
    identity.media_type = ArtifactMediaType::new(media).unwrap();
    let record = ArtifactRecord::pending(identity.clone(), builds.created_at).unwrap();
    let record = record
      .transition(
        &identity,
        record.version(),
        ArtifactEvent::BeginVerification,
        builds.created_at,
      )
      .unwrap();
    doc.record = record
      .transition(&identity, record.version(), ArtifactEvent::Publish, builds.created_at)
      .unwrap();
    doc
  })
  .collect::<Vec<_>>();
  FactoryNodeBuildOutputs {
    execution: FactoryNodeBuildExecutionFacts {
      profile: intent.profile().clone(),
      input_digest: intent.input().digest().unwrap(),
      permissions: intent.permissions().clone(),
      budget: intent.budget(),
      deadline: intent.node().deadline(),
    },
    usage: BudgetUsage {
      attempts: 1,
      output_bytes: documents.iter().map(|doc| doc.bytes.len() as u64).sum(),
      ..Default::default()
    },
    documents,
    verified_at: builds.created_at,
    fresh_until: time(1000),
  }
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
      let template: FlowDefinitionTemplate =
        serde_json::from_str(include_str!("../../../config/factory/flows/base-intake.json")).unwrap();
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
      } = result
      else {
        panic!("trusted capture must publish a common result rather than a lifecycle checkpoint");
      };
      assert_eq!(expected.subject().exact(), work.subject());
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
      ) -> Result<FactoryNodeBuildOutputs, OrdinaryBuildApplicationError> {
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
