//! Public journal and Artifact fixture shared by compiler and materializer contracts.
use crate::factory_node_test_support::{Builds, capture_outputs, fixtures::*};
use crate::*;
use octacity_protocol::*;
use octacity_server_factory::*;

pub(crate) struct ReadonlyCandidateFixtureProfile {
  pub work: WorkEnvelope,
  pub build_configuration: BuildConfigurationRef,
  pub toolchain: FactoryTaskToolchain,
  pub permissions: FactoryPermissionSet,
  pub credentials: FactoryCredentialProfiles,
  pub instructions: FactoryText,
}
fn time(value: i64) -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(value).unwrap()
}
pub(crate) struct CandidateFixtureBytes {
  values: std::collections::BTreeMap<octacity_server_domain::ArtifactId, Vec<u8>>,
  pub(crate) calls: std::sync::Mutex<Vec<octacity_server_domain::ArtifactId>>,
}
#[async_trait::async_trait]
impl octacity_artifact_store::ArtifactByteReader for CandidateFixtureBytes {
  async fn read_published(
    &self,
    object: &octacity_artifact_store::ArtifactObject,
    _: u64,
  ) -> Result<Vec<u8>, octacity_artifact_store::ArtifactStoreError> {
    self.calls.lock().unwrap().push(object.artifact_id());
    Ok(self.values[&object.artifact_id()].clone())
  }
}

pub(crate) struct CandidateContextFixture {
  pub context: FactoryCandidateContext,
  pub intent: FlowBuildIntent,
  pub admitted: AdmittedFlow,
  pub instructions: FactoryText,
  pub runs: std::sync::Arc<octacity_server_store::testing::InMemoryFactoryConfigurationStore>,
  pub source: std::sync::Arc<
    FactoryCandidateContextSource<
      octacity_server_store::testing::InMemoryArtifactRecordStore,
      Builds,
      CandidateFixtureBytes,
    >,
  >,
  pub(crate) text: FactoryNodeBuildOutputDocument,
  pub(crate) calls: Vec<octacity_server_domain::ArtifactId>,
  pub(crate) bundle_id: octacity_server_domain::ArtifactId,
  pub(crate) manifest_id: octacity_server_domain::ArtifactId,
}

fn rebound_attempt(admitted: &AdmittedFlow, input: &FlowNodeInput, previous: &NodeAttempt) -> NodeAttempt {
  let definition = admitted.closure().definition(input.definition()).unwrap();
  let node = definition
    .nodes()
    .iter()
    .find(|node| node.key() == input.node())
    .unwrap();
  NodeAttempt::new(
    admitted.root_run(),
    admitted.initial_cycle(),
    definition,
    NodeAttemptInput {
      id: NodeAttemptId::generate(),
      node_key: input.node().clone(),
      node_kind: node.kind(),
      number: NodeAttemptNumber::INITIAL,
      input_digest: input.digest().unwrap(),
      budget: node.budget(),
      started_at: previous.started_at(),
      deadline: previous.deadline(),
      execution: NodeExecutionIdentity::External(node.build().unwrap().tool.clone()),
      ownership: FactoryClaimOwnership::new(previous.owner().clone(), previous.claim()),
    },
  )
  .unwrap()
}

#[expect(
  clippy::too_many_arguments,
  reason = "explicit caller-owned fixture contracts share one real capture lifecycle"
)]
fn with_readonly_consumer(
  profile: &ReadonlyCandidateFixtureProfile,
  work: &WorkEnvelope,
  prior: &AdmittedFlow,
  input: &FlowNodeInput,
  previous: &NodeAttempt,
  consumer: Option<(FlowNodeKind, FlowBuildBinding)>,
  extra_schemas: Vec<FlowDataSchema>,
  configure: impl FnOnce(&mut FlowDefinitionInput),
) -> (AdmittedFlow, FlowNodeInput, NodeAttempt, FactoryText, FactoryKey) {
  let grants = profile.permissions.clone();
  let instructions = profile.instructions.clone();
  let (kind, selection) = if let Some(consumer) = consumer {
    consumer
  } else {
    let mut selection = binding(work.subject().project_id());
    selection.build_configuration = profile.build_configuration.clone();
    selection.plugin = profile.toolchain.plugin.clone();
    selection.tool = profile.toolchain.executable.clone();
    selection.model_or_tool = profile.toolchain.model.clone();
    selection.result_output = key("codex-run-result");
    selection.task_digest = codex_flow_task_digest(
      &instructions,
      profile
        .credentials
        .profile(FactoryCredentialConsumer::ReadOnlyReview.purpose()),
      FactoryCredentialConsumer::ReadOnlyReview,
      &FactoryCodexUsagePolicy::ReportedCost,
    );
    (FlowNodeKind::Reasoning, selection)
  };
  let original = prior.closure().definition(input.definition()).unwrap();
  let producer = &original.nodes()[0];
  let consumer_key = key("operator-chosen-inspector-97");
  let consumer = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: consumer_key.clone(),
    kind,
    input_schema: producer.input_schema().cloned(),
    outcomes: producer.outcomes().to_vec(),
    budget: producer.budget(),
    permissions: grants.clone(),
    required: true,
    subflow: None,
  })
  .unwrap()
  .with_build(selection)
  .unwrap()
  .with_input_binding(
    FlowInputBinding::new(
      std::collections::BTreeMap::from([(
        key("captured"),
        FlowInputSource::Predecessor {
          view: FlowRecordView::Payload,
          path: String::new(),
        },
      )]),
      FlowDataMapping::new(FlowValueMapping::Pointer {
        path: "/captured".into(),
      })
      .unwrap(),
    )
    .unwrap(),
  )
  .unwrap();
  let prior_budget = prior.limits().budget();
  let budget = BudgetLimit::new(
    prior_budget.max_attempts(),
    prior_budget.max_elapsed_millis(),
    prior_budget.max_cost_micro_units(),
    prior_budget.max_tokens(),
    prior_budget.max_output_bytes() * 2,
  )
  .unwrap();
  let mut definition = FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: original.input_schema().cloned(),
    entry: original.entry().clone(),
    nodes: vec![producer.clone(), consumer],
    transitions: vec![
      FlowTransition::new(
        producer.key().clone(),
        producer.outcomes()[0].key().clone(),
        FlowTransitionTarget::Node(consumer_key.clone()),
      ),
      FlowTransition::new(
        consumer_key.clone(),
        producer.outcomes()[0].key().clone(),
        FlowTransitionTarget::Terminal(original.terminals()[0].key().clone()),
      ),
    ],
    terminals: original.terminals().to_vec(),
    context_projections: vec![],
    data_projections: vec![FlowDataProjection::new(
      producer.key().clone(),
      producer.outcomes()[0].key().clone(),
      consumer_key.clone(),
      producer.input_schema().unwrap().clone(),
    )],
    execution: FlowExecutionPolicy::new(budget, grants.clone(), 1).unwrap(),
  };
  configure(&mut definition);
  let definition = FlowDefinition::new(definition).unwrap();
  let budget = definition.execution().budget();
  let grants = definition.execution().permissions().clone();
  let root = FactoryRun::admitted(input.factory_run_id(), work);
  let flow = FlowRun::root(&root, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    FlowAdmissionLimits::product_defaults(prior.limits().max_wip(), budget, grants).unwrap(),
    flow,
    cycle,
    prior.wip_limits(),
  )
  .unwrap()
  .with_data_schemas(prior.data_schemas().iter().cloned().chain(extra_schemas).collect())
  .unwrap();
  let selected = FlowNodeInput::new(
    work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    input.node().clone(),
    input.payload().clone(),
  )
  .unwrap()
  .with_context(input.context().unwrap().clone(), vec![])
  .unwrap();
  let node = rebound_attempt(&admitted, &selected, previous);
  (admitted, selected, node, instructions, consumer_key)
}

pub(crate) async fn candidate_context_fixture(profile: ReadonlyCandidateFixtureProfile) -> CandidateContextFixture {
  candidate_context_fixture_with_consumer(profile, None).await
}

/// Reuses the same retained producer with an explicit caller-owned consumer contract.
pub(crate) async fn candidate_context_fixture_with_consumer(
  profile: ReadonlyCandidateFixtureProfile,
  consumer: Option<(FlowNodeKind, FlowBuildBinding)>,
) -> CandidateContextFixture {
  candidate_context_fixture_with_definition(profile, consumer, |_| {}).await
}

/// Composes caller-owned action/gate contracts around the same retained producer.
pub(crate) async fn candidate_context_fixture_with_definition(
  profile: ReadonlyCandidateFixtureProfile,
  consumer: Option<(FlowNodeKind, FlowBuildBinding)>,
  configure: impl FnOnce(&mut FlowDefinitionInput),
) -> CandidateContextFixture {
  candidate_context_fixture_with_preparation(profile, consumer, configure, vec![], None, async |_, _, _, _| time(32))
    .await
}

/// Runs explicit caller-owned metadata steps before preparing the retained consumer.
/// The existing producer and Artifact lifecycle are shared with the unchanged wrapper.
pub(crate) async fn candidate_context_fixture_with_preparation(
  profile: ReadonlyCandidateFixtureProfile,
  consumer: Option<(FlowNodeKind, FlowBuildBinding)>,
  configure: impl FnOnce(&mut FlowDefinitionInput),
  extra_schemas: Vec<FlowDataSchema>,
  policy_context: Option<(FactoryArtifactReference, Vec<u8>)>,
  before_consumer: impl for<'a> std::ops::AsyncFnOnce(
    std::sync::Arc<octacity_server_store::testing::InMemoryFactoryConfigurationStore>,
    FactoryDigest,
    FactoryRunId,
    &'a ChangeSet,
  ) -> octacity_server_domain::Timestamp,
) -> CandidateContextFixture {
  use async_trait::async_trait;
  use octacity_server_store::testing::{
    ArtifactLeaseFixture, InMemoryArtifactRecordStore, authoritative_store_contract_fixture,
  };
  use octacity_server_store::*;
  use std::sync::{Arc, Mutex};
  fn replace_bytes(doc: &mut FactoryNodeBuildOutputDocument, bytes: Vec<u8>) {
    doc.bytes = bytes;
    let mut identity = doc.record.identity().clone();
    identity.size_bytes = doc.bytes.len() as u64;
    identity.digest = ArtifactContentDigest::from_bytes(FactoryDigest::content_sha256(&doc.bytes).as_bytes());
    let pending = ArtifactRecord::pending(identity.clone(), time(30)).unwrap();
    let checking = pending
      .transition(&identity, pending.version(), ArtifactEvent::BeginVerification, time(30))
      .unwrap();
    doc.record = checking
      .transition(&identity, checking.version(), ArtifactEvent::Publish, time(30))
      .unwrap();
  }
  let work = profile.work.clone();
  let mut binding = binding(work.subject().project_id());
  binding.build_configuration = profile.build_configuration.clone();
  let tool = reference("git.capture");
  let schema = reference("capture.artifacts");
  binding.evidence = [CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT]
    .into_iter()
    .map(|name| {
      EvidenceRequirement::new(
        key(name),
        EvidenceOutputKind::Artifact,
        schema.clone(),
        tool.clone(),
        binding.plugin.clone(),
      )
    })
    .collect();
  let (work, admitted, public_schema, input, node) =
    execution_fixture_for_work_with_binding(FlowNodeKind::Reasoning, work, binding, None, None);
  let (admitted, input, node, instructions, consumer_key) = with_readonly_consumer(
    &profile,
    &work,
    &admitted,
    &input,
    &node,
    consumer,
    extra_schemas,
    configure,
  );
  struct PreparationOnly;
  #[async_trait]
  impl FactoryNodeExecutor for PreparationOnly {
    async fn observe(&self, _: FactoryNodeExecutionRequest<'_>) -> Result<FactoryNodeExecutionStep, ApplicationError> {
      panic!("preparation must not dispatch a capability")
    }
    async fn cancel(
      &self,
      _: FactoryNodeExecutionRequest<'_>,
      _: octacity_server_domain::Timestamp,
    ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
      unreachable!()
    }
  }
  let runs = Arc::new(octacity_server_store::testing::InMemoryFactoryConfigurationStore::new());
  let run = FactoryRun::admitted(input.factory_run_id(), &work);
  let audit = octacity_server_store::testing::management_mutation_with_request((), "candidate-consumer")
    .audit()
    .clone();
  runs
    .seed_factory_run(
      PublishedFactoryAdmission {
        work: work.clone(),
        run: run.clone(),
        flow: admitted.clone(),
        admitted_at: time(1),
      },
      FactoryDigest::from_bytes([76; 32]),
      &audit,
    )
    .unwrap();
  let claim = FactoryRunClaimRecord::new(run.id(), node.owner().clone(), node.claim());
  runs
    .claim_factory_run(ClaimFactoryRun {
      run_id: run.id(),
      expected_version: run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        run.id(),
        AuditActorKind::Worker,
        None,
        key("factory.claimed"),
        claim.id,
        key("claimed"),
        time(10),
      ),
    })
    .await
    .unwrap();
  let producer_target = FactoryNodeTarget {
    flow_run_id: input.flow_run_id(),
    cycle_id: input.cycle_id(),
    node: input.node().clone(),
  };
  let owner = FactoryNodeRunner::new(
    runs.clone(),
    Arc::new(PreparationOnly),
    crate::factory_node_test_support::clock(),
  );
  owner
    .run_once(run.id(), claim.id, producer_target.clone(), Some(input), time(10))
    .await
    .unwrap();
  let snapshot = runs.factory_run_snapshot(run.id()).await.unwrap();
  let intent = snapshot.flow.data.build_intents[0].clone();
  let builds = Arc::new(Builds::new());
  *builds.state.lock().unwrap() = octacity_server_orchestrator::BuildState::Succeeded;
  let mut outputs = capture_outputs(&intent, &builds, "base");
  replace_bytes(
    &mut outputs.documents[0],
    b"\xff\0binary-private-bundle-canary".to_vec(),
  );
  let mut manifest: CapturedChangeSetManifestV1 = serde_json::from_slice(&outputs.documents[1].bytes).unwrap();
  manifest.candidate_revision = "0123456789abcdef0123456789abcdef0123456789".to_owned();
  manifest.bundle.size_bytes = outputs.documents[0].bytes.len() as u64;
  manifest.bundle.sha256 = FactoryDigest::content_sha256(&outputs.documents[0].bytes).to_string();
  let mut encoded = serde_json::to_vec(&manifest).unwrap();
  encoded.push(b'\n');
  replace_bytes(&mut outputs.documents[1], encoded);
  let capture = outputs.documents.clone();
  let public = crate::factory_node_test_support::document(
    &builds,
    &intent.profile().result_output,
    public_schema.reference(),
    br#"{"count":9}"#.to_vec(),
    intent.profile().tool.clone(),
  );
  outputs.documents.push(public);
  outputs.usage.output_bytes = outputs.documents.iter().map(|doc| doc.bytes.len() as u64).sum();
  let adapter = FactoryNodeCaptureBuildAdapter::new(
    builds.clone(),
    Arc::new(crate::factory_node_test_support::Outputs(Some(outputs))),
    tool.clone(),
  );
  let observer = FactoryNodeRunner::new(
    runs.clone(),
    Arc::new(adapter),
    crate::factory_node_test_support::clock(),
  );
  assert_eq!(
    observer
      .run_once(run.id(), claim.id, producer_target.clone(), None, time(31))
      .await
      .unwrap(),
    FactoryNodeStep::Waiting
  );
  let dispatched = runs.factory_run_snapshot(run.id()).await.unwrap();
  assert_eq!(dispatched.flow.data.build_executions.len(), 1);
  assert_eq!(
    dispatched.flow.data.build_executions[0].build_id(),
    builds.acceptance.build_id
  );
  assert_eq!(
    dispatched.flow.data.build_executions[0].attempt_id(),
    builds.acceptance.attempt_id
  );
  let step = observer
    .run_once(run.id(), claim.id, producer_target, None, time(31))
    .await
    .unwrap();
  assert_eq!(builds.requests.lock().unwrap().len(), 1);
  let FactoryNodeStep::Completed(record) = step else {
    panic!("trusted capture must complete")
  };
  let snapshot = runs.factory_run_snapshot(run.id()).await.unwrap();
  let execution = snapshot
    .flow
    .data
    .build_executions
    .iter()
    .find(|row| row.node_attempt_id() == intent.node().id())
    .unwrap()
    .clone();
  let candidate = restore_factory_node_change_set(&record, &intent, &execution, &capture[1], &tool).unwrap();
  let mut text = crate::factory_node_test_support::document_kind(
    &builds,
    &key("declared-text-73"),
    &schema,
    b"Declared public model task only.".to_vec(),
    tool.clone(),
    EvidenceOutputKind::Artifact,
  );
  let mut identity = text.record.identity().clone();
  let policy_selected = policy_context.is_some();
  if let Some((reference, bytes)) = policy_context {
    assert_eq!(FactoryDigest::content_sha256(&bytes), reference.content_digest());
    assert_eq!(bytes.len() as u64, reference.encoded_size());
    identity.artifact_id = reference.artifact_id();
    identity.digest = ArtifactContentDigest::from_bytes(reference.content_digest().as_bytes());
    identity.size_bytes = reference.encoded_size();
    text.bytes = bytes;
  }
  identity.lease_id = capture[0].record.identity().lease_id;
  let pending = ArtifactRecord::pending(identity.clone(), time(30)).unwrap();
  let checking = pending
    .transition(&identity, pending.version(), ArtifactEvent::BeginVerification, time(30))
    .unwrap();
  text.record = checking
    .transition(&identity, checking.version(), ArtifactEvent::Publish, time(30))
    .unwrap();
  let subject = FactoryTaskSubject::Candidate(candidate.subject().clone());
  let docs = [capture[0].clone(), capture[1].clone(), text.clone()];
  let entries = docs
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
        if identity.artifact_id == text.record.identity().artifact_id {
          if policy_selected {
            ContextSourceKind::Policy
          } else {
            ContextSourceKind::Task
          }
        } else {
          ContextSourceKind::Evidence
        },
        key(identity.logical_name.as_str()),
        subject.clone(),
        FactoryContextReference::Artifact(artifact.clone()),
        artifact.content_digest(),
        artifact.encoded_size(),
        FactorySafeText::new("Explicit immutable selection").unwrap(),
        FactoryDigest::from_bytes([90; 32]),
      )
      .unwrap()
    })
    .collect();
  let consumer_at = before_consumer(runs.clone(), claim.id, run.id(), &candidate).await;
  let snapshot = runs.factory_run_snapshot(run.id()).await.unwrap();
  let consumer_target = FactoryNodeTarget {
    flow_run_id: admitted.root_run().id(),
    cycle_id: admitted.initial_cycle().id(),
    node: consumer_key,
  };
  let selected = FactoryNodeRunner::<octacity_server_store::testing::InMemoryFactoryConfigurationStore, PreparationOnly>
    ::prepare_input(&snapshot, &consumer_target).unwrap()
  .with_context(
    ContextManifest::new(
      ContextManifestId::generate(),
      subject,
      FactoryDigest::from_bytes([91; 32]),
      entries,
    )
    .unwrap(),
    vec![],
  )
  .unwrap();
  owner
    .run_once(run.id(), claim.id, consumer_target, Some(selected), consumer_at)
    .await
    .unwrap();
  let snapshot = runs.factory_run_snapshot(run.id()).await.unwrap();
  let consumer_intent = snapshot
    .flow
    .data
    .build_intents
    .iter()
    .find(|row| row.node().id() != intent.node().id())
    .unwrap()
    .clone();
  let selected = consumer_intent.input().clone();
  let ordinary = authoritative_store_contract_fixture();
  let access = LeaseAccess {
    lease_id: capture[0].record.identity().lease_id,
    fence: octacity_server_store::LeaseFence::from_bytes([7; 32]),
    agent_id: ordinary.agent_id,
    registration_epoch: ordinary.registration_epoch,
  };
  let artifacts = Arc::new(InMemoryArtifactRecordStore::new(ArtifactLeaseFixture {
    access,
    build_id: execution.build_id(),
    attempt_id: execution.attempt_id(),
    attempt: octacity_server_domain::AttemptNumber::new(1).unwrap(),
    job_id: execution.jobs()[0],
    expires_at: time(10000),
  }));
  for (index, doc) in docs.iter().enumerate() {
    let identity = doc.record.identity();
    let begun = artifacts
      .begin_artifact_upload(BeginArtifactUpload {
        artifact_id: identity.artifact_id,
        upload_id: octacity_server_domain::ArtifactUploadId::generate(),
        idempotency_key: octacity_server_store::IdempotencyKey::new(format!("candidate-input-{index}")).unwrap(),
        lease: access,
        job_id: identity.job_id,
        attempt: octacity_server_domain::AttemptNumber::new(1).unwrap(),
        logical_name: identity.logical_name.clone(),
        producer_run_id: 1,
        producer_task_id: index as u64 + 1,
        artifact_type: identity.artifact_type.clone(),
        media_type: identity.media_type.clone(),
        transport_media_type: identity.media_type.clone(),
        size_bytes: identity.size_bytes,
        digest: identity.digest,
        reserved_at: time(30),
        capability_expires_at: time(1000),
      })
      .await
      .unwrap();
    let verify = VerifyArtifactUpload {
      upload_id: begun.upload.upload_id,
      lease: access,
      job_id: identity.job_id,
      attempt: octacity_server_domain::AttemptNumber::new(1).unwrap(),
      observed_at: time(30),
    };
    artifacts.begin_artifact_verification(verify).await.unwrap();
    artifacts
      .finish_artifact_verification(verify, ArtifactVerificationResult::Verified)
      .await
      .unwrap();
  }
  let bytes = Arc::new(CandidateFixtureBytes {
    values: docs
      .iter()
      .map(|doc| (doc.record.identity().artifact_id, doc.bytes.clone()))
      .collect(),
    calls: Mutex::new(vec![]),
  });
  let reader = Arc::new(FactoryArtifactReader::new(artifacts, builds, bytes.clone()));
  let source = Arc::new(FactoryCandidateContextSource::new(reader));
  let result = source
    .read(ReadFactoryCandidateContext {
      input: &selected,
      producer_record: &record,
      producer_intent: &intent,
      producer_execution: &execution,
      capture_tool: &tool,
      observed_at: consumer_at,
      required_until: time(100),
    })
    .await
    .unwrap();
  CandidateContextFixture {
    intent: consumer_intent,
    runs,
    source,
    admitted,
    instructions,
    context: result,
    text,
    calls: bytes.calls.lock().unwrap().clone(),
    bundle_id: capture[0].record.identity().artifact_id,
    manifest_id: capture[1].record.identity().artifact_id,
  }
}
