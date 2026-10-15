use crate::*;
use async_trait::async_trait;
use octacity_artifact_store::{ArtifactByteReader, ArtifactObject, ArtifactStoreError};
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use std::sync::Arc;
fn at(n: i64) -> Timestamp {
  Timestamp::from_unix_millis(n).unwrap()
}
fn key(s: &str) -> FactoryKey {
  FactoryKey::new(s).unwrap()
}
struct Plan(ProtectedTestPlan);
#[async_trait]
impl FactoryProtectedTestPlanSource for Plan {
  async fn frozen_plan(&self, _: &FlowBuildIntent) -> Result<ProtectedTestPlan, ApplicationError> {
    Ok(self.0.clone())
  }
}
// Frozen reader fixtures retain actual publication records from the existing lifecycle store.
struct RetainedRows(
  std::collections::BTreeMap<octacity_server_domain::ArtifactId, octacity_server_store::ArtifactUploadRecord>,
);
#[async_trait]
impl octacity_server_store::ArtifactRecordStore for RetainedRows {
  async fn published_artifact(
    &self,
    id: octacity_server_domain::ArtifactId,
  ) -> Result<octacity_server_store::ArtifactUploadRecord, octacity_server_store::StoreError> {
    self
      .0
      .get(&id)
      .cloned()
      .ok_or(octacity_server_store::StoreError::Unavailable)
  }
  async fn reserve_artifact(
    &self,
    _: octacity_server_store::ReserveArtifact,
  ) -> Result<octacity_server_store::ArtifactRecord, octacity_server_store::StoreError> {
    unreachable!("retained input resolution performs no publication")
  }
  async fn artifact(
    &self,
    _: octacity_server_domain::ArtifactId,
  ) -> Result<octacity_server_store::ArtifactRecord, octacity_server_store::StoreError> {
    unreachable!()
  }
  async fn transition_artifact(
    &self,
    _: octacity_server_store::TransitionArtifact,
  ) -> Result<octacity_server_store::ArtifactRecord, octacity_server_store::StoreError> {
    unreachable!()
  }
  async fn begin_artifact_upload(
    &self,
    _: octacity_server_store::BeginArtifactUpload,
  ) -> Result<octacity_server_store::BeginArtifactUploadOutcome, octacity_server_store::StoreError> {
    unreachable!()
  }
  async fn artifact_upload(
    &self,
    _: octacity_server_domain::ArtifactUploadId,
  ) -> Result<octacity_server_store::ArtifactUploadRecord, octacity_server_store::StoreError> {
    unreachable!()
  }
  async fn begin_artifact_verification(
    &self,
    _: octacity_server_store::VerifyArtifactUpload,
  ) -> Result<octacity_server_store::ArtifactUploadRecord, octacity_server_store::StoreError> {
    unreachable!()
  }
  async fn finish_artifact_verification(
    &self,
    _: octacity_server_store::VerifyArtifactUpload,
    _: octacity_server_store::ArtifactVerificationResult,
  ) -> Result<octacity_server_store::ArtifactUploadRecord, octacity_server_store::StoreError> {
    unreachable!()
  }
  async fn list_published_artifacts(
    &self,
    _: octacity_server_store::ListPublishedArtifacts,
  ) -> Result<Vec<octacity_server_store::ArtifactUploadRecord>, octacity_server_store::StoreError> {
    unreachable!()
  }
}
struct RetainedBuilds(std::collections::BTreeMap<octacity_server_domain::BuildId, octacity_server_store::BuildRecord>);
#[async_trait]
impl octacity_server_store::BuildQueryStore for RetainedBuilds {
  async fn build(
    &self,
    id: octacity_server_domain::BuildId,
  ) -> Result<octacity_server_store::BuildRecord, octacity_server_store::StoreError> {
    self
      .0
      .get(&id)
      .cloned()
      .ok_or(octacity_server_store::StoreError::Unavailable)
  }
  async fn attempt(
    &self,
    _: octacity_server_domain::AttemptId,
  ) -> Result<octacity_server_store::AttemptRecord, octacity_server_store::StoreError> {
    unreachable!()
  }
  async fn job(
    &self,
    _: octacity_server_domain::JobId,
  ) -> Result<octacity_server_store::JobRecord, octacity_server_store::StoreError> {
    unreachable!()
  }
  async fn latest_attempt(
    &self,
    _: octacity_server_domain::BuildId,
  ) -> Result<octacity_server_store::AttemptRecord, octacity_server_store::StoreError> {
    unreachable!()
  }
}
struct SelectedBytes {
  values: std::collections::BTreeMap<octacity_server_domain::ArtifactId, Vec<u8>>,
  calls: std::sync::Mutex<Vec<octacity_server_domain::ArtifactId>>,
}
#[async_trait]
impl ArtifactByteReader for SelectedBytes {
  async fn read_published(&self, object: &ArtifactObject, maximum: u64) -> Result<Vec<u8>, ArtifactStoreError> {
    let bytes = self
      .values
      .get(&object.artifact_id())
      .expect("only selected inventory and capture metadata may be read");
    assert!(bytes.len() as u64 <= maximum);
    self.calls.lock().unwrap().push(object.artifact_id());
    Ok(bytes.clone())
  }
}
fn rebound(
  input: &FlowNodeInput,
  previous: &NodeAttempt,
  admitted: &AdmittedFlow,
  start: i64,
  number: u64,
) -> NodeAttempt {
  let def = admitted.closure().definition(input.definition()).unwrap();
  let node = def.node(input.node()).unwrap();
  NodeAttempt::new(
    admitted.root_run(),
    admitted.initial_cycle(),
    def,
    NodeAttemptInput {
      id: NodeAttemptId::generate(),
      node_key: input.node().clone(),
      node_kind: node.kind(),
      number: NodeAttemptNumber::new(number).unwrap(),
      input_digest: input.digest().unwrap(),
      budget: node.budget(),
      started_at: at(start),
      deadline: at(1000),
      execution: NodeExecutionIdentity::External(node.build().unwrap().tool.clone()),
      ownership: FactoryClaimOwnership::new(previous.owner().clone(), previous.claim()),
    },
  )
  .unwrap()
}
fn after(node: FlowNodeDefinition, view: FlowRecordView, path: &str) -> FlowNodeDefinition {
  node
    .with_input_binding(
      FlowInputBinding::new(
        std::collections::BTreeMap::from([(
          key("selected"),
          FlowInputSource::Predecessor {
            view,
            path: path.to_owned(),
          },
        )]),
        FlowDataMapping::new(FlowValueMapping::Object {
          fields: std::collections::BTreeMap::from([(
            key("count"),
            FlowValueMapping::Pointer {
              path: "/selected".into(),
            },
          )]),
        })
        .unwrap(),
      )
      .unwrap(),
    )
    .unwrap()
}
fn renamed(node: &FlowNodeDefinition, name: &str) -> FlowNodeDefinition {
  FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key(name),
    kind: node.kind(),
    input_schema: node.input_schema().cloned(),
    outcomes: node.outcomes().to_vec(),
    budget: node.budget(),
    permissions: node.permissions().clone(),
    required: true,
    subflow: None,
  })
  .unwrap()
  .with_build(node.build().unwrap().clone())
  .unwrap()
}
struct CaptureFixture {
  intent: FlowBuildIntent,
  completion: FactoryNodeBuildCompletion,
  bundle: FactoryNodeBuildOutputDocument,
  manifest: FactoryNodeBuildOutputDocument,
  builds: Arc<crate::factory_node_test_support::Builds>,
}
fn captured_validation_inputs(
  deliverables: bool,
  reserved_inputs: u16,
  configure: impl FnOnce(&WorkEnvelope, &mut FlowBuildBinding, &mut FactoryPermissionSet),
) -> (
  crate::factory_protected_test_tests::RetainedAuthorFixture,
  CaptureFixture,
  FlowBuildIntent,
) {
  use crate::factory_node_test_support::{Builds, Outputs, fixtures::*};
  let work = work();
  let mut capture_binding = binding(work.subject().project_id());
  let capture_tool = reference("git.capture");
  capture_binding.evidence = [
    octacity_protocol::CHANGE_SET_BUNDLE_OUTPUT,
    octacity_protocol::CHANGE_SET_MANIFEST_OUTPUT,
  ]
  .into_iter()
  .map(|name| {
    EvidenceRequirement::new(
      key(name),
      EvidenceOutputKind::Artifact,
      reference("capture.artifacts"),
      capture_tool.clone(),
      capture_binding.plugin.clone(),
    )
  })
  .collect();
  if deliverables {
    capture_binding = crate::factory_node_capture_tests::deliverable_binding(capture_binding);
  }
  let (_, capture_shape, input_schema, _, _) =
    execution_fixture_for_work_with_binding(FlowNodeKind::Reasoning, work.clone(), capture_binding, None, None);
  let mut validator_binding = if deliverables {
    crate::factory_node_capture_tests::deliverable_binding(binding(work.subject().project_id()))
  } else {
    binding(work.subject().project_id())
  };
  let mut validator_permissions = FactoryPermissionSet::deny_all();
  configure(&work, &mut validator_binding, &mut validator_permissions);
  let (_, validator_shape, _, _, _) = execution_fixture_with_permissions(
    FlowNodeKind::BuildCommand,
    work.clone(),
    validator_binding,
    None,
    validator_permissions.clone(),
  );
  let capture = std::cell::RefCell::new(None);
  let original_work = work.clone();
  let author = crate::factory_protected_test_tests::retained_author_fixture_with_contract_and_reserved_inputs(
    work,
    deliverables.then(|| "a".repeat(64).parse().unwrap()),
    reserved_inputs,
    at(1900),
    |prior, previous_input, previous| {
      let capture_node = renamed(
        &capture_shape
          .closure()
          .definition(capture_shape.closure().root())
          .unwrap()
          .nodes()[0],
        "produce_source_73",
      );
      let author_node = after(
        renamed(
          &prior.closure().definition(prior.closure().root()).unwrap().nodes()[0],
          "produce_tests_91",
        ),
        FlowRecordView::Payload,
        "/count",
      );
      let validator_node = after(
        renamed(
          &validator_shape
            .closure()
            .definition(validator_shape.closure().root())
            .unwrap()
            .nodes()[0],
          "verify_candidate_47",
        ),
        FlowRecordView::Metadata,
        "/observed_at",
      );
      let budget = BudgetLimit::new(5, 5000, 1000, 5000, 100000).unwrap();
      let definition = FlowDefinition::new(FlowDefinitionInput {
        id: FlowDefinitionId::generate(),
        version: FlowDefinitionVersion::INITIAL,
        input_schema: Some(input_schema.reference().clone()),
        entry: capture_node.key().clone(),
        nodes: vec![capture_node.clone(), author_node.clone(), validator_node.clone()],
        transitions: vec![
          FlowTransition::new(
            capture_node.key().clone(),
            capture_node.outcomes()[0].key().clone(),
            FlowTransitionTarget::Node(author_node.key().clone()),
          ),
          FlowTransition::new(
            author_node.key().clone(),
            author_node.outcomes()[0].key().clone(),
            FlowTransitionTarget::Node(validator_node.key().clone()),
          ),
          FlowTransition::new(
            validator_node.key().clone(),
            validator_node.outcomes()[0].key().clone(),
            FlowTransitionTarget::Terminal(key("done")),
          ),
        ],
        terminals: vec![FlowTerminalDefinition::new(
          key("done"),
          input_schema.reference().clone(),
        )],
        context_projections: vec![],
        data_projections: vec![],
        execution: FlowExecutionPolicy::new(budget, validator_permissions.clone(), 1).unwrap(),
      })
      .unwrap();
      let run = FactoryRun::admitted(prior.root_run().factory_run_id(), &original_work);
      let flow = FlowRun::root(&run, definition.reference()).unwrap();
      let cycle = WorkflowCycle::initial(&flow).unwrap();
      let admitted = AdmittedFlow::new(
        PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
        FlowAdmissionLimits::product_defaults(5, budget, validator_permissions.clone()).unwrap(),
        flow,
        cycle,
        prior.wip_limits(),
      )
      .unwrap()
      .with_data_schemas(prior.data_schemas().to_vec())
      .unwrap();
      let capture_input = FlowNodeInput::new(
        &original_work,
        &admitted,
        admitted.root_run(),
        admitted.initial_cycle(),
        capture_node.key().clone(),
        previous_input.payload().clone(),
      )
      .unwrap()
      .with_context(previous_input.context().unwrap().clone(), vec![])
      .unwrap();
      let capture_attempt = rebound(&capture_input, &previous, &admitted, 20, 1);
      let intent = FlowBuildIntent::new(
        capture_input.clone(),
        &admitted,
        capture_attempt.clone(),
        BudgetUsage::default(),
        i64::from(original_work.priority().get()),
      )
      .unwrap();
      let builds = Arc::new(Builds::new());
      *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
      let mut outputs = crate::factory_node_capture_tests::capture_outputs(&intent, &builds, "base");
      if deliverables {
        crate::factory_node_capture_tests::complete_deliverable_outputs(
          &mut outputs,
          FactoryDigest::content_sha256(b"public unit test"),
        );
      }
      // Signed ordinary Jobs require an actual Git object identity, unlike the
      // logical revision accepted by the standalone capture-domain fixture.
      let document = &mut outputs.documents[1];
      let mut captured: octacity_protocol::CapturedChangeSetManifestV1 =
        serde_json::from_slice(&document.bytes).unwrap();
      captured.candidate_revision = "0123456789abcdef0123456789abcdef0123456789".to_owned();
      document.bytes = serde_json::to_vec(&captured).unwrap();
      document.bytes.push(b'\n');
      let mut identity = document.record.identity().clone();
      identity.size_bytes = document.bytes.len() as u64;
      identity.digest = octacity_server_artifacts::ArtifactContentDigest::from_bytes(
        FactoryDigest::content_sha256(&document.bytes).as_bytes(),
      );
      document.record = octacity_server_store::ArtifactRecord::restore(
        identity,
        document.record.state(),
        document.record.version(),
        document.record.created_at(),
        document.record.published_at(),
        document.record.deleted_at(),
      )
      .unwrap();
      outputs.usage.output_bytes = outputs
        .documents
        .iter()
        .map(|document| document.bytes.len() as u64)
        .sum();
      let bundle = outputs.documents[0].clone();
      let manifest = outputs.documents[1].clone();
      let result = crate::factory_node_test_support::document(
        &builds,
        &intent.profile().result_output,
        input_schema.reference(),
        br#"{"count":9}"#.to_vec(),
        intent.profile().tool.clone(),
      );
      outputs.usage.output_bytes += result.bytes.len() as u64;
      outputs.documents.push(result);
      crate::factory_admission_tests::run_ready(crate::factory_node_build_tests::admit_pending_build(
        &builds,
        &intent,
        &admitted,
        &input_schema,
      ));
      let executor =
        FactoryNodeCaptureBuildAdapter::new(builds.clone(), Arc::new(Outputs(Some(outputs))), capture_tool.clone());
      let step = crate::factory_admission_tests::run_ready(executor.observe(FactoryNodeExecutionRequest {
        run_id: capture_input.factory_run_id(),
        admitted: &admitted,
        work: &original_work,
        input: &capture_input,
        attempt: &capture_attempt,
        build: Some(&intent),
        ownership: FactoryClaimOwnership::new(capture_attempt.owner().clone(), capture_attempt.claim()),
        observed_at: at(31),
      }))
      .unwrap();
      let FactoryNodeExecutionStep::Completed {
        record,
        execution: Some(execution),
        usage,
        ..
      } = step
      else {
        panic!("trusted ordinary capture must complete")
      };
      let completion = record
        .completion(
          &capture_attempt,
          admitted.closure().definition(capture_input.definition()).unwrap(),
          FactoryClaimOwnership::new(capture_attempt.owner().clone(), capture_attempt.claim()),
          usage,
          at(31),
        )
        .unwrap();
      let mut preceding = completion.usage();
      preceding.attempts += 1;
      let input = FlowNodeInput::prepare(FlowInputPreparation {
        work: &original_work,
        admitted: &admitted,
        flow: admitted.root_run(),
        cycle: admitted.initial_cycle(),
        node: author_node.key().clone(),
        schema: admitted.data_schema(previous_input.payload().schema()).unwrap(),
        history: FlowRuntimeHistory {
          stops: &[],
          flow_runs: std::slice::from_ref(admitted.root_run()),
          cycles: std::slice::from_ref(admitted.initial_cycle()),
          attempts: std::slice::from_ref(&capture_attempt),
          completions: std::slice::from_ref(&completion),
        },
        records: std::slice::from_ref(&record),
        inputs: std::slice::from_ref(&capture_input),
        incoming: None,
      })
      .unwrap()
      .with_context(previous_input.context().unwrap().clone(), vec![])
      .unwrap();
      *capture.borrow_mut() = Some(CaptureFixture {
        intent,
        completion: FactoryNodeBuildCompletion {
          execution: *execution,
          record: *record,
          completion,
          usage,
        },
        bundle,
        manifest,
        builds,
      });
      let node = rebound(&input, &previous, &admitted, 31, 2);
      (admitted, input, node, preceding)
    },
  );
  let capture = capture.into_inner().unwrap();
  let candidate = restore_factory_node_change_set(
    &capture.completion.record,
    &capture.intent,
    &capture.completion.execution,
    &capture.manifest,
    &capture_tool,
  )
  .unwrap();
  let subject = FactoryTaskSubject::Candidate(candidate.subject().clone());
  let entries = [&capture.bundle, &capture.manifest, &author.manifest]
    .into_iter()
    .map(|doc| {
      let artifact = crate::factory_node_build::artifact_reference(doc).unwrap();
      ContextManifestEntry::new(
        ContextSourceKind::Evidence,
        key(doc.record.identity().logical_name.as_str()),
        subject.clone(),
        FactoryContextReference::Artifact(artifact.clone()),
        artifact.content_digest(),
        artifact.encoded_size(),
        FactorySafeText::new("Exact original validator input").unwrap(),
        FactoryDigest::from_bytes([3; 32]),
      )
      .unwrap()
    })
    .collect();
  let context = ContextManifest::new(
    ContextManifestId::generate(),
    subject,
    FactoryDigest::from_bytes([4; 32]),
    entries,
  )
  .unwrap();
  let input = FlowNodeInput::prepare(FlowInputPreparation {
    work: &author.work,
    admitted: &author.admitted,
    flow: author.admitted.root_run(),
    cycle: author.admitted.initial_cycle(),
    node: key("verify_candidate_47"),
    schema: author
      .admitted
      .data_schema(author.intent.input().payload().schema())
      .unwrap(),
    history: FlowRuntimeHistory {
      stops: &[],
      flow_runs: std::slice::from_ref(author.admitted.root_run()),
      cycles: std::slice::from_ref(author.admitted.initial_cycle()),
      attempts: &[capture.intent.node().clone(), author.intent.node().clone()],
      completions: &[capture.completion.completion.clone(), author.completion.clone()],
    },
    records: &[capture.completion.record.clone(), author.record.clone()],
    inputs: &[capture.intent.input().clone(), author.intent.input().clone()],
    incoming: None,
  })
  .unwrap()
  .with_context(context, vec![])
  .unwrap();
  let node = rebound(&input, author.intent.node(), &author.admitted, 40, 3);
  let mut preceding = capture
    .completion
    .completion
    .usage()
    .checked_add(author.completion.usage())
    .unwrap();
  preceding.attempts += 2;
  let intent = FlowBuildIntent::new(
    input,
    &author.admitted,
    node,
    preceding,
    i64::from(author.work.priority().get()),
  )
  .unwrap();
  (author, capture, intent)
}
async fn published_row(document: &FactoryNodeBuildOutputDocument) -> octacity_server_store::ArtifactUploadRecord {
  use octacity_server_domain::*;
  use octacity_server_store::{testing::*, *};
  let id = document.record.identity();
  let published_at = document.record.published_at().unwrap();
  let lease = LeaseAccess {
    lease_id: id.lease_id,
    fence: LeaseFence::from_bytes([1; 32]),
    agent_id: AgentId::generate(),
    registration_epoch: RegistrationEpoch::new(1).unwrap(),
  };
  let store = InMemoryArtifactRecordStore::new(ArtifactLeaseFixture {
    access: lease,
    build_id: id.build_id,
    attempt_id: id.attempt_id,
    attempt: AttemptNumber::FIRST,
    job_id: id.job_id,
    expires_at: at(1000),
  });
  let upload = store
    .begin_artifact_upload(BeginArtifactUpload {
      artifact_id: id.artifact_id,
      upload_id: ArtifactUploadId::generate(),
      idempotency_key: IdempotencyKey::new("retained").unwrap(),
      lease,
      job_id: id.job_id,
      attempt: AttemptNumber::FIRST,
      logical_name: id.logical_name.clone(),
      producer_run_id: 1,
      producer_task_id: 1,
      artifact_type: id.artifact_type.clone(),
      media_type: id.media_type.clone(),
      transport_media_type: id.media_type.clone(),
      size_bytes: id.size_bytes,
      digest: id.digest,
      reserved_at: published_at,
      capability_expires_at: at(1000),
    })
    .await
    .unwrap();
  let verify = VerifyArtifactUpload {
    upload_id: upload.upload.upload_id,
    lease,
    job_id: id.job_id,
    attempt: AttemptNumber::FIRST,
    observed_at: published_at,
  };
  store.begin_artifact_verification(verify).await.unwrap();
  store
    .finish_artifact_verification(verify, ArtifactVerificationResult::Verified)
    .await
    .unwrap();
  store.published_artifact(id.artifact_id).await.unwrap()
}
async fn append_intent(
  store: &octacity_server_store::testing::InMemoryFactoryConfigurationStore,
  claim: &octacity_server_store::FactoryRunClaimRecord,
  intent: &FlowBuildIntent,
  time: i64,
) {
  use octacity_server_store::*;
  let snapshot = store
    .factory_run_snapshot(intent.input().factory_run_id())
    .await
    .unwrap();
  let mut transition = testing::factory_flow_contract_transition(&snapshot, claim, time);
  transition.append.flow.attempts.push(intent.node().clone());
  transition.append.flow.data.inputs.push(intent.input().clone());
  transition.append.flow.data.build_intents.push(intent.clone());
  let usage = snapshot
    .budgets
    .iter()
    .find(|row| row.id == snapshot.current.budget_id)
    .unwrap()
    .usage
    .checked_add(BudgetUsage {
      attempts: 1,
      ..Default::default()
    })
    .unwrap();
  transition.budget = FactoryBudgetRecord::new(snapshot.run.id(), transition.next_run.version(), usage, at(time));
  transition.current.budget_id = transition.budget.id;
  store
    .commit_factory_run_transition(transition)
    .await
    .unwrap_or_else(|error| panic!("intent {} at {}: {error:?}", intent.input().node(), time));
}
async fn append_completed(
  store: &octacity_server_store::testing::InMemoryFactoryConfigurationStore,
  claim: &octacity_server_store::FactoryRunClaimRecord,
  completed: &FactoryNodeBuildCompletion,
  time: i64,
) {
  use octacity_server_store::*;
  let snapshot = store
    .factory_run_snapshot(completed.record.input().factory_run_id())
    .await
    .unwrap();
  let mut transition = testing::factory_flow_contract_transition(&snapshot, claim, time);
  transition
    .append
    .flow
    .data
    .build_executions
    .push(completed.execution.clone());
  transition.append.flow.data.records.push(completed.record.clone());
  transition.append.flow.completions.push(completed.completion.clone());
  let usage = snapshot
    .budgets
    .iter()
    .find(|row| row.id == snapshot.current.budget_id)
    .unwrap()
    .usage
    .checked_add(completed.completion.usage())
    .unwrap();
  transition.budget = FactoryBudgetRecord::new(snapshot.run.id(), transition.next_run.version(), usage, at(time));
  transition.current.budget_id = transition.budget.id;
  store.commit_factory_run_transition(transition).await.unwrap();
}
#[test]
fn original_validation_materialization_restores_candidate_and_private_tests_from_the_same_frozen_intent() {
  verify_original_materialization(false);
}
#[test]
fn original_validation_materialization_recovers_required_public_deliverable_proofs() {
  verify_original_materialization(true);
}
pub(crate) struct RetainedValidationFixture {
  pub runs: Arc<octacity_server_store::testing::InMemoryFactoryConfigurationStore>,
  pub intent: FlowBuildIntent,
  pub admitted: AdmittedFlow,
  pub materialized: FactoryProtectedValidationMaterialization,
  pub source: Arc<dyn FactoryProtectedValidationInputSource>,
  pub candidate_source: Arc<dyn FactoryNodeCandidateContextSource>,
  pub private_file: octacity_server_domain::ArtifactId,
  pub capture_bundle: octacity_server_domain::ArtifactId,
  pub initial_reads: Vec<octacity_server_domain::ArtifactId>,
}
/// Reuses the exact persisted producer chain with an explicitly selected validator.
pub(crate) async fn retained_validation_fixture(
  deliverables: bool,
  reserved_inputs: u16,
  configure: impl FnOnce(&WorkEnvelope, &mut FlowBuildBinding, &mut FactoryPermissionSet),
) -> RetainedValidationFixture {
  retained_validation_fixture_with_clock(
    deliverables,
    reserved_inputs,
    configure,
    crate::factory_node_test_support::clock(),
  )
  .await
}
pub(crate) async fn retained_validation_fixture_with_clock(
  deliverables: bool,
  reserved_inputs: u16,
  configure: impl FnOnce(&WorkEnvelope, &mut FlowBuildBinding, &mut FactoryPermissionSet),
  source_clock: Arc<dyn FactoryExecutionClock>,
) -> RetainedValidationFixture {
  use octacity_server_store::{testing::*, *};
  let (author, capture, intent) = captured_validation_inputs(deliverables, reserved_inputs, configure);
  let store = Arc::new(InMemoryFactoryConfigurationStore::new());
  let run = FactoryRun::admitted(intent.input().factory_run_id(), &author.work);
  let audit = management_mutation_with_request((), "original-validation-inputs")
    .audit()
    .clone();
  store
    .seed_factory_run(
      PublishedFactoryAdmission {
        work: author.work.clone(),
        run: run.clone(),
        flow: author.admitted.clone(),
        admitted_at: at(1),
      },
      FactoryDigest::from_bytes([1; 32]),
      &audit,
    )
    .unwrap();
  let claim = FactoryRunClaimRecord::new(run.id(), intent.node().owner().clone(), intent.node().claim());
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
        FactoryDigest::from_bytes([1; 32]),
        key("accepted"),
        at(10),
      ),
    })
    .await
    .unwrap();
  append_intent(&store, &claim, &capture.intent, 20).await;
  append_completed(&store, &claim, &capture.completion, 31).await;
  append_intent(&store, &claim, &author.intent, 32).await;
  let author_completion = FactoryNodeBuildCompletion {
    execution: author.execution.clone(),
    record: author.record.clone(),
    completion: author.completion.clone(),
    usage: author.completion.usage(),
  };
  append_completed(&store, &claim, &author_completion, 33).await;
  append_intent(&store, &claim, &intent, 40).await;
  let documents = [&capture.bundle, &capture.manifest, &author.manifest, &author.file];
  let mut uploads = std::collections::BTreeMap::new();
  for document in documents {
    let row = published_row(document).await;
    uploads.insert(row.artifact.identity().artifact_id, row);
  }
  let artifacts = Arc::new(RetainedRows(uploads));
  let builds = Arc::new(RetainedBuilds(std::collections::BTreeMap::from([
    (
      capture.completion.execution.build_id(),
      capture
        .builds
        .build(capture.completion.execution.build_id())
        .await
        .unwrap(),
    ),
    (
      author.execution.build_id(),
      author.builds.build(author.execution.build_id()).await.unwrap(),
    ),
  ])));
  let bytes = Arc::new(SelectedBytes {
    values: [&capture.manifest, &author.manifest]
      .into_iter()
      .map(|doc| (doc.record.identity().artifact_id, doc.bytes.clone()))
      .collect(),
    calls: std::sync::Mutex::new(vec![]),
  });
  let reader = Arc::new(FactoryArtifactReader::new(artifacts, builds, bytes.clone()));
  let candidates = Arc::new(FactoryStoredCandidateContextSource::new(
    store.clone(),
    Arc::new(FactoryCandidateContextSource::new(reader.clone())),
    crate::factory_node_test_support::fixtures::reference("git.capture"),
    source_clock.clone(),
  ));
  let tests = Arc::new(FactoryStoredProtectedTestContextSource::new(
    store.clone(),
    reader.clone(),
    Arc::new(Plan(author.bundle.plan().clone())),
    author.requirement.clone(),
    source_clock.clone(),
  ));
  let source = FactoryStoredProtectedValidationInputSource::new(
    store.clone(),
    candidates.clone(),
    tests.clone(),
    reader.clone(),
    crate::factory_node_test_support::fixtures::reference("git.capture"),
    source_clock.clone(),
  );
  let materialized = source.materialization(&intent).await.unwrap();
  let restarted = FactoryStoredProtectedValidationInputSource::new(
    store.clone(),
    candidates.clone(),
    tests,
    reader,
    crate::factory_node_test_support::fixtures::reference("git.capture"),
    source_clock,
  );
  let initial_reads = bytes.calls.lock().unwrap().clone();
  RetainedValidationFixture {
    runs: store,
    intent,
    admitted: author.admitted,
    materialized,
    source: Arc::new(restarted),
    candidate_source: candidates,
    private_file: author.file.record.identity().artifact_id,
    capture_bundle: capture.bundle.record.identity().artifact_id,
    initial_reads,
  }
}
fn verify_original_materialization(deliverables: bool) {
  use octacity_server_store::FactoryRunStore;
  crate::factory_admission_tests::run_ready(async {
    let fixture = retained_validation_fixture(deliverables, 4, |_, _, _| {}).await;
    assert_eq!(fixture.materialized.input(), fixture.intent.input());
    assert_eq!(
      fixture
        .runs
        .factory_run_snapshot(fixture.intent.input().factory_run_id())
        .await
        .unwrap()
        .admitted_flow,
      fixture.admitted
    );
    let (_, grants) = fixture.materialized.wire_inputs(&[]).unwrap();
    assert_eq!(grants.len(), 4);
    assert!(
      grants
        .iter()
        .any(|grant| grant.artifact_id == fixture.private_file.to_string()
          && grant.destination == "/octacity/protected/tests/hidden.rs"
          && grant.sha256 == FactoryDigest::content_sha256(b"PRIVATE_EXPECTATION").to_string())
    );
    assert!(
      grants
        .iter()
        .any(|grant| grant.artifact_id == fixture.capture_bundle.to_string())
    );
    assert!(!fixture.initial_reads.contains(&fixture.private_file));
    let restored = fixture.source.materialization(&fixture.intent).await.unwrap();
    assert_eq!(restored, fixture.materialized);
  });
}

#[test]
fn completed_validation_materialization_covers_real_read_time_without_renewing_private_retention() {
  use crate::factory_node_test_support::{Builds, Outputs, document_at};
  use octacity_server_store::FactoryRunStore;
  use std::sync::atomic::{AtomicI64, Ordering};
  struct MovingClock {
    now: AtomicI64,
    step: AtomicI64,
  }
  impl FactoryExecutionClock for MovingClock {
    fn now(&self) -> Result<Timestamp, ApplicationError> {
      Ok(at(
        self.now.fetch_add(self.step.load(Ordering::SeqCst), Ordering::SeqCst),
      ))
    }
  }
  crate::factory_admission_tests::run_ready(async {
    let clock = Arc::new(MovingClock {
      now: AtomicI64::new(0),
      step: AtomicI64::new(0),
    });
    let fixture = retained_validation_fixture_with_clock(false, 4, |_, _, _| {}, clock.clone()).await;
    let snapshot = fixture
      .runs
      .factory_run_snapshot(fixture.intent.input().factory_run_id())
      .await
      .unwrap();
    let claim = snapshot.current_claim.unwrap();
    let builds = Arc::new(Builds::new_at(at(41)));
    *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
    let schema = fixture
      .admitted
      .closure()
      .definition(fixture.intent.input().definition())
      .unwrap()
      .node(fixture.intent.input().node())
      .unwrap()
      .outcome(fixture.intent.result_outcome())
      .unwrap()
      .schema();
    let output = document_at(
      &builds,
      &fixture.intent.profile().result_output,
      schema,
      br#"{"count":9}"#.to_vec(),
      fixture.intent.profile().tool.clone(),
      at(45),
    );
    let outputs = FactoryNodeBuildOutputs {
      execution: FactoryNodeBuildExecutionFacts {
        profile: fixture.intent.profile().clone(),
        input_digest: fixture.intent.input().digest().unwrap(),
        permissions: fixture.intent.permissions().clone(),
        budget: fixture.intent.budget(),
        deadline: fixture.intent.node().deadline(),
      },
      documents: vec![output],
      usage: BudgetUsage {
        attempts: 1,
        elapsed_millis: 4,
        output_bytes: 11,
        ..Default::default()
      },
      verified_at: at(45),
      fresh_until: at(1900),
    };
    let admission = FactoryNodeBuildAdapter::new(builds.clone(), Arc::new(Outputs(None)));
    let FactoryNodeBuildStep::Waiting(accepted) = admission
      .observe_or_dispatch(
        &fixture.intent,
        &fixture.admitted,
        fixture.admitted.data_schema(schema).unwrap(),
        FactoryClaimOwnership::new(fixture.intent.node().owner().clone(), fixture.intent.node().claim()),
        at(50),
      )
      .await
      .unwrap()
    else {
      panic!("validator admission must retain its accepted execution");
    };
    assert_eq!(accepted.build_id(), builds.acceptance.build_id);
    assert_eq!(accepted.attempt_id(), builds.acceptance.attempt_id);
    assert_eq!(builds.requests.lock().unwrap().len(), 1);
    let adapter = FactoryNodeBuildAdapter::new(builds, Arc::new(Outputs(Some(outputs))));
    let ownership = FactoryClaimOwnership::new(fixture.intent.node().owner().clone(), fixture.intent.node().claim());
    let FactoryNodeBuildStep::Completed(completed) = adapter
      .observe_or_dispatch(
        &fixture.intent,
        &fixture.admitted,
        fixture.admitted.data_schema(schema).unwrap(),
        ownership,
        at(50),
      )
      .await
      .unwrap()
    else {
      panic!("ordinary validator completion")
    };
    append_completed(&fixture.runs, &claim, &completed, 50).await;
    clock.now.store(1100, Ordering::SeqCst);
    clock.step.store(10, Ordering::SeqCst);
    assert!(at(1100) > fixture.intent.node().deadline());
    let recovered = fixture
      .source
      .materialization(&fixture.intent)
      .await
      .expect("retained observation tolerates real I/O elapsed time after execution completion");
    assert_eq!(
      recovered.input_digest().unwrap(),
      fixture.materialized.input_digest().unwrap()
    );
    assert_eq!(
      recovered.wire_inputs(&[]).unwrap(),
      fixture.materialized.wire_inputs(&[]).unwrap()
    );
    clock.now.store(1865, Ordering::SeqCst);
    assert!(
      matches!(
        fixture.source.materialization(&fixture.intent).await,
        Err(ApplicationError::InvalidInput)
      ),
      "crossing the original private-test expiry during I/O must still reject"
    );
  });
}
