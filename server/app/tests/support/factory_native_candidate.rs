//! Retained native producer facts for readonly candidate composition contracts.
//! These fixture publications do not qualify an Agent Git capture execution.
use super::{managed, postgres};
use octacity_artifact_s3::{S3ArtifactStore, S3ArtifactStoreConfig};
use octacity_artifact_store::*;
use octacity_protocol::{
  CHANGE_SET_BUNDLE_MEDIA_TYPE, CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_MEDIA_TYPE, CHANGE_SET_MANIFEST_OUTPUT,
  CapturedChangeSetFileV1, CapturedChangeSetManifestV1, ExecutionCapabilityV2, ExecutionContractRange,
  ExecutionEvidenceV2, ExecutionProviderId, FactoryEnforcementCapabilityV3, FactoryImmutableReferenceV3,
  ManagedOctaExecutionV3,
};
use octacity_server_application::*;
use octacity_server_domain::*;
use octacity_server_factory::*;
use octacity_server_store::*;
use octacity_server_store_postgres::{PostgresAuthoritativeStore, PostgresStore};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc, time::Duration};

pub(super) const CANDIDATE: &str = "0123456789abcdef0123456789abcdef0123456789";
const BUNDLE: &[u8] = b"\xff\0retained native candidate bundle canary";
pub(super) fn key(value: &str) -> FactoryKey {
  value.parse().unwrap()
}
pub(super) fn now() -> Timestamp {
  Timestamp::from_unix_millis(
    i64::try_from(
      std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis(),
    )
    .unwrap(),
  )
  .unwrap()
}
fn later(at: Timestamp, millis: i64) -> Timestamp {
  Timestamp::from_unix_millis(at.unix_millis().checked_add(millis).unwrap()).unwrap()
}
pub(super) fn capture_tool() -> ImmutableReference {
  ImmutableReference::new(
    key("git"),
    key("fixture-capture-v1"),
    FactoryDigest::from_bytes([67; 32]),
  )
}

pub(super) fn objects() -> Arc<S3ArtifactStore> {
  Arc::new(
    S3ArtifactStore::new(S3ArtifactStoreConfig {
      endpoint: std::env::var("OCTACITY_MINIO_ENDPOINT").unwrap(),
      region: "us-east-1".to_owned(),
      bucket: "octacity-artifacts".to_owned(),
      prefix: String::new(),
      access_key: zeroize::Zeroizing::new(std::env::var("OCTACITY_MINIO_ACCESS_KEY").unwrap()),
      secret_key: zeroize::Zeroizing::new(std::env::var("OCTACITY_MINIO_SECRET_KEY").unwrap()),
      force_path_style: true,
      operation_timeout: Duration::from_secs(5),
      capability_recheck_interval: Duration::from_secs(1),
    })
    .unwrap(),
  )
}

pub(super) fn toolchain() -> JobSpecToolchainPolicy {
  let canonical = managed::canonical_v3_spec();
  JobSpecToolchainPolicy {
    source: octacity_server_job::SourcePluginPolicy::new("git", "0.1.0", canonical.source.plugin_sha256, "url")
      .unwrap(),
    octa: canonical.octa,
    validity: octacity_server_job::JobSpecValidity::new(3600).unwrap(),
  }
}

/// Publishes a real lease-authorized ordinary output into PostgreSQL and S3.
async fn publish_output(
  store: &PostgresStore,
  objects: &S3ArtifactStore,
  access: LeaseAccess,
  job_id: JobId,
  document: &(ArtifactId, FactoryKey, EvidenceOutputKind, String, Vec<u8>),
  schema: &FlowDataSchema,
) -> ArtifactRecord {
  let (artifact_id, name, kind, media, bytes) = document;
  let upload_id = ArtifactUploadId::generate();
  let observed_at = now();
  let digest = ArtifactContentDigest::from_bytes(FactoryDigest::content_sha256(bytes).as_bytes());
  store
    .begin_artifact_upload(BeginArtifactUpload {
      artifact_id: *artifact_id,
      upload_id,
      idempotency_key: IdempotencyKey::new(artifact_id.to_string()).unwrap(),
      lease: access,
      job_id,
      attempt: AttemptNumber::FIRST,
      logical_name: ArtifactName::new(name.as_str()).unwrap(),
      producer_run_id: 1,
      producer_task_id: 1,
      artifact_type: match kind {
        EvidenceOutputKind::Artifact => ArtifactType::Artifact,
        EvidenceOutputKind::Report => {
          ArtifactType::Report(ArtifactReportFormat::new(schema.reference().identity().as_str()).unwrap())
        }
      },
      media_type: ArtifactMediaType::new(media).unwrap(),
      transport_media_type: ArtifactMediaType::new(media).unwrap(),
      size_bytes: bytes.len() as u64,
      digest,
      reserved_at: observed_at,
      capability_expires_at: later(observed_at, 30_000),
    })
    .await
    .unwrap();
  let object = ArtifactObject::new(*artifact_id, upload_id, bytes.len() as u64, digest.to_string(), media).unwrap();
  let authorization = objects
    .authorize_upload(&object, Duration::from_secs(30))
    .await
    .unwrap();
  let mut request = reqwest::Client::new().put(&authorization.url).body(bytes.clone());
  for (name, value) in authorization.required_headers {
    request = request.header(name, value);
  }
  request.send().await.unwrap().error_for_status().unwrap();
  objects.complete_upload(&object).await.unwrap();
  objects.verify_published(&object).await.unwrap();
  let verify = VerifyArtifactUpload {
    upload_id,
    lease: access,
    job_id,
    attempt: AttemptNumber::FIRST,
    observed_at: now(),
  };
  store.begin_artifact_verification(verify).await.unwrap();
  store
    .finish_artifact_verification(verify, ArtifactVerificationResult::Verified)
    .await
    .unwrap()
    .artifact
}

// Ordinary placement has a separate native contract. This retained assignment
// fixture uses its persisted shape; all output grants still go through public
// lease/upload operations and the current signed managed Job permissions.
async fn retained_assignment(database: &postgres::TestDatabase, job: &MaterializedJob) -> LeaseAccess {
  use sha2::Digest as _;
  let agent = AgentId::generate();
  let registration = uuid::Uuid::new_v4();
  let access = LeaseAccess {
    lease_id: LeaseId::generate(),
    fence: LeaseFence::from_bytes([81; 32]),
    agent_id: agent,
    registration_epoch: RegistrationEpoch::new(1).unwrap(),
  };
  let target = managed::canonical_v3_spec().runtime.target;
  let mut inventory = octacity_server_store::testing::compatible_inventory(agent);
  inventory.host_platform = target.host_platform;
  inventory.execution_contract = ExecutionContractRange { min: 2, max: 3 };
  inventory.executions = vec![ExecutionCapabilityV2 {
    provider: ExecutionProviderId::new("retained-fixture-provider").unwrap(),
    mode: target.mode,
    host_platform: target.host_platform,
    target_platform: target.target_platform,
    guarantees: target.required_guarantees,
    immutable_images: true,
  }];
  sqlx::query("INSERT INTO agents (id,name,pool_id,pool_version,state,inventory,version,created_at,updated_at) VALUES ($1,'native-candidate-fixture',$2,1,'online',$3,1,now(),now())")
    .bind(agent.as_uuid()).bind(job.allowed_pools[0].as_uuid()).bind(sqlx::types::Json(&inventory)).execute(&database.pool).await.unwrap();
  sqlx::query("INSERT INTO agent_registrations (id,agent_id,epoch,credential_hash,inventory,registered_at,expires_at,execution_contract_version) VALUES ($1,$2,1,$3,$4,now(),now()+interval '1 hour',3)")
    .bind(registration).bind(agent.as_uuid()).bind(vec![81_u8;32]).bind(sqlx::types::Json(&inventory)).execute(&database.pool).await.unwrap();
  sqlx::query("INSERT INTO leases (id,job_id,pool_id,pool_version,registration_id,registration_epoch,fence_hash,state,version,leased_at,expires_at) VALUES ($1,$2,$3,1,$4,1,$5,'active',1,now(),now()+interval '1 hour')")
    .bind(access.lease_id.as_uuid()).bind(job.id.as_uuid()).bind(job.allowed_pools[0].as_uuid()).bind(registration).bind(sha2::Sha256::digest(access.fence.expose()).to_vec()).execute(&database.pool).await.unwrap();
  sqlx::query("UPDATE jobs SET state='leased' WHERE id=$1")
    .bind(job.id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  sqlx::query("DELETE FROM ready_queue_entries WHERE job_id=$1")
    .bind(job.id.as_uuid())
    .execute(&database.pool)
    .await
    .unwrap();
  access
}
pub(super) struct CandidatePlan {
  pub flow: AdmittedFlow,
  pub producer_id: NodeAttemptId,
  pub consumer: FactoryKey,
  pub documents: Vec<(ArtifactId, FactoryKey, EvidenceOutputKind, String, Vec<u8>)>,
  pub schema: FlowDataSchema,
  pub capture_schema: ImmutableReference,
}

/// Appends a completed producer through the same native fenced journal and
/// ordinary Build admission ports used by the owner. No producer executor is
/// installed in the runtime under test.
pub(super) async fn precomplete_producer(
  database: &postgres::TestDatabase,
  store: Arc<PostgresStore>,
  objects: Arc<S3ArtifactStore>,
  profile: &FactoryCodexBuildProfile,
  admission: &PublishedFactoryAdmission,
  plan: &CandidatePlan,
) -> Vec<ArtifactRecord> {
  let clock = Arc::new(SystemFactoryExecutionClock);
  let authority = Arc::new(PostgresAuthoritativeStore::new(
    database.pool.clone(),
    postgres::test_signer(),
  ));
  let claimed = store
    .claim_factory_runs(
      ClaimFactoryRuns::new(key("fixture-retained-producer"), now(), later(now(), 30_000), 1).unwrap(),
    )
    .await
    .unwrap();
  let claim = &claimed[0].record;
  let ownership = FactoryClaimOwnership::new(claim.owner.clone(), claim.claim);
  let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  let definition = plan.flow.closure().definition(plan.flow.closure().root()).unwrap();
  let node = definition.node(definition.entry()).unwrap();
  let reader = Arc::new(FactoryArtifactReader::new(
    store.clone(),
    authority.clone(),
    objects.clone(),
  ));
  let input = FactoryArtifactFlowInputs::new(reader, clock.clone())
    .prepare(
      &snapshot,
      &FactoryNodeTarget {
        flow_run_id: plan.flow.root_run().id(),
        cycle_id: plan.flow.initial_cycle().id(),
        node: node.key().clone(),
      },
    )
    .await
    .unwrap();
  let started_at = now();
  let attempt = NodeAttempt::new(
    plan.flow.root_run(),
    plan.flow.initial_cycle(),
    definition,
    NodeAttemptInput {
      id: plan.producer_id,
      node_key: node.key().clone(),
      node_kind: node.kind(),
      number: NodeAttemptNumber::INITIAL,
      input_digest: input.digest().unwrap(),
      budget: node.budget(),
      started_at,
      deadline: later(started_at, 600_000),
      execution: NodeExecutionIdentity::External(node.build().unwrap().tool.clone()),
      ownership: ownership.clone(),
    },
  )
  .unwrap();
  let intent = FlowBuildIntent::new(
    input.clone(),
    &plan.flow,
    attempt.clone(),
    BudgetUsage::default(),
    i64::from(admission.work.priority().get()),
  )
  .unwrap();
  let mut start =
    octacity_server_store::testing::factory_flow_contract_transition(&snapshot, claim, started_at.unix_millis());
  start.append.flow.attempts.push(attempt.clone());
  start.append.flow.data.inputs.push(input.clone());
  start.append.flow.data.build_intents.push(intent.clone());
  start.budget = FactoryBudgetRecord::new(
    admission.run.id(),
    start.next_run.version(),
    BudgetUsage {
      attempts: 1,
      ..Default::default()
    },
    started_at,
  );
  start.current.budget_id = start.budget.id;
  store.commit_factory_run_transition(start).await.unwrap();
  let context = StoreBackedBuildExecutionContext::new(
    store.clone(),
    store.clone(),
    Arc::new(StoreBackedEffectiveProjectPolicySource::new(store.clone())),
    toolchain(),
  )
  .load(
    admission.work.subject().project_id(),
    profile.build_configuration.id(),
    profile.build_configuration.version(),
  )
  .await
  .unwrap();
  let protected = FactoryExecutionInputPublisher::new(store.clone(), objects.clone(), clock)
    .publish(
      &intent,
      &ownership,
      vec![
        FactoryExecutionDocument::new(
          "/octacity/protected/Octafile.yml",
          "application/yaml",
          b"version: 1\ntasks:\n  retained:\n    shell: |\n      true\n".to_vec(),
        )
        .unwrap(),
      ],
    )
    .await
    .unwrap();
  let grants = intent.permissions().clone();
  let accepted_at = now();
  let materialized = materialize_factory_managed_build(FactoryManagedBuildInput {
    context,
    intent: intent.clone(),
    ownership: ownership.clone(),
    parameters: Default::default(),
    programs: BTreeMap::from([(
      profile.pipeline_node.clone(),
      FactoryManagedJobProgram {
        execution: ManagedOctaExecutionV3 {
          workspace_root: None,
          octafile_input: protected.inputs[0].artifact_id.clone(),
          tasks: vec!["retained".to_owned()],
          credential_profile: Some(
            profile
              .credentials
              .profile(profile.consumer.purpose())
              .as_str()
              .to_owned(),
          ),
          tool_control: None,
        },
        protected_inputs: protected,
        required_enforcement: FactoryEnforcementCapabilityV3::ALL.to_vec(),
      },
    )]),
    policy_layers: FactoryBuildPolicyLayers {
      project: grants.clone(),
      configuration: grants.clone(),
      task: grants.clone(),
      local: LocalPermissionCeiling::fully_enforced(grants),
    },
    accepted_at,
  })
  .unwrap();
  let accepted = authority.accept_factory_build(materialized.clone()).await.unwrap();
  let access = retained_assignment(database, &materialized.jobs[0]).await;
  let mut artifacts = vec![];
  for document in &plan.documents {
    artifacts.push(publish_output(&store, &objects, access, accepted.job_ids[0], document, &plan.schema).await);
  }
  authority
    .complete_job(JobCompletion {
      completion_id: IdempotencyKey::new("native-candidate-producer-complete").unwrap(),
      lease: access,
      final_sequence: None,
      kind: JobCompletionKind::Succeeded,
      execution: Some(ExecutionEvidenceV2 {
        provider: ExecutionProviderId::new("retained-fixture-provider").unwrap(),
        target: managed::canonical_v3_spec().runtime.target,
      }),
      completed_at: now(),
    })
    .await
    .unwrap();
  let execution = FlowBuildExecution::new(&intent, accepted.build_id, accepted.attempt_id, accepted.job_ids).unwrap();
  let observed_at = now();
  let provenance = FlowBuildProvenance::new(
    intent.profile().clone(),
    artifacts
      .iter()
      .zip(&plan.documents)
      .map(|(artifact, (_, name, kind, _, _))| {
        let identity = artifact.identity();
        let source = *kind == EvidenceOutputKind::Artifact;
        FlowVerifiedBuildOutput {
          name: name.clone(),
          output_kind: *kind,
          schema: if source {
            plan.capture_schema.clone()
          } else {
            plan.schema.reference().clone()
          },
          artifact: FactoryArtifactReference::new(
            identity.artifact_id,
            FactoryDigest::from_bytes(identity.digest.as_bytes()),
            identity.size_bytes,
          )
          .unwrap(),
          producer: EvidenceProducer::new(
            execution.build_id(),
            execution.attempt_id(),
            identity.job_id,
            if source {
              capture_tool()
            } else {
              intent.profile().tool.clone()
            },
            intent.profile().plugin.clone(),
          ),
          verified_at: artifact.published_at().unwrap(),
          fresh_until: identity.retention.delete_after().unwrap(),
          data: None,
        }
      })
      .collect(),
  )
  .unwrap();
  let record = FlowNodeRecord::new(
    &input,
    &attempt,
    definition,
    FlowRecordObservation {
      outcome: key("observed"),
      payload: FlowPayload::new(&plan.schema, json!(true)).unwrap(),
      producer: attempt.execution().clone(),
      observed_at,
    },
  )
  .unwrap()
  .with_build_provenance(provenance, definition)
  .unwrap();
  let manifest_index = plan
    .documents
    .iter()
    .position(|(_, name, _, _, _)| name.as_str() == CHANGE_SET_MANIFEST_OUTPUT)
    .unwrap();
  let manifest = FactoryNodeBuildOutputDocument {
    record: artifacts[manifest_index].clone(),
    bytes: plan.documents[manifest_index].4.clone(),
    data: None,
    schema: plan.capture_schema.clone(),
    tool: capture_tool(),
    plugin: intent.profile().plugin.clone(),
  };
  let candidate = restore_factory_node_change_set(&record, &intent, &execution, &manifest, &capture_tool()).unwrap();
  let record = record
    .with_discovery(FlowDiscoveryFacts::changeset(&candidate))
    .unwrap();
  let usage = BudgetUsage {
    output_bytes: plan.documents.iter().map(|d| d.4.len() as u64).sum(),
    ..Default::default()
  };
  let completed_at = now();
  let completion = record
    .completion(&attempt, definition, ownership, usage, completed_at)
    .unwrap();
  let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  let mut finish =
    octacity_server_store::testing::factory_flow_contract_transition(&snapshot, claim, completed_at.unix_millis());
  finish.append.flow.data.build_executions.push(execution);
  finish.append.flow.data.records.push(record);
  finish.append.flow.completions.push(completion);
  finish.budget = FactoryBudgetRecord::new(
    admission.run.id(),
    finish.next_run.version(),
    BudgetUsage { attempts: 1, ..usage },
    completed_at,
  );
  finish.current.budget_id = finish.budget.id;
  store.commit_factory_run_transition(finish).await.unwrap();
  // Renew this same fixture owner with a short bounded claim. The default
  // runtime takes over after expiry without extending the producer deadline.
  let released = store
    .claim_factory_runs(ClaimFactoryRuns::new(claim.owner.clone(), now(), later(now(), 1), 1).unwrap())
    .await
    .unwrap();
  assert_eq!(released.len(), 1, "fixture owner claim renewal");
  artifacts
}

/// Freezes all artifact identities before admission, including capture provenance.
pub(super) fn candidate_plan(work: &WorkEnvelope, profile: &FactoryCodexBuildProfile) -> CandidatePlan {
  let producer_id = NodeAttemptId::generate();
  let capture_tool = capture_tool();
  let captured = CapturedChangeSetManifestV1 {
    format_version: 1,
    base_revision: work.subject().base_revision().as_str().to_owned(),
    candidate_revision: CANDIDATE.to_owned(),
    node_attempt_id: producer_id.to_string(),
    capture_tool: FactoryImmutableReferenceV3 {
      identity: capture_tool.identity().to_string(),
      version: capture_tool.version().to_string(),
      sha256: capture_tool.digest().to_string(),
    },
    changed_paths: vec![],
    cumulative_changes: None,
    bundle: CapturedChangeSetFileV1 {
      name: CHANGE_SET_BUNDLE_OUTPUT.to_owned(),
      size_bytes: BUNDLE.len() as u64,
      sha256: FactoryDigest::content_sha256(BUNDLE).to_string(),
    },
    patch: None,
  };
  let mut manifest = serde_json::to_vec(&captured).unwrap();
  manifest.push(b'\n');
  CapturedChangeSetManifestV1::decode_canonical(&manifest).unwrap();
  let documents = vec![
    (
      ArtifactId::generate(),
      key(CHANGE_SET_BUNDLE_OUTPUT),
      EvidenceOutputKind::Artifact,
      CHANGE_SET_BUNDLE_MEDIA_TYPE.to_owned(),
      BUNDLE.to_vec(),
    ),
    (
      ArtifactId::generate(),
      key(CHANGE_SET_MANIFEST_OUTPUT),
      EvidenceOutputKind::Artifact,
      CHANGE_SET_MANIFEST_MEDIA_TYPE.to_owned(),
      manifest,
    ),
    (
      ArtifactId::generate(),
      key("codex-run-result"),
      EvidenceOutputKind::Report,
      "application/json".to_owned(),
      b"true".to_vec(),
    ),
  ];
  let mut grants = serde_json::to_value(&profile.project_permissions).unwrap();
  grants["outputs"]["kinds"] = json!([
    CHANGE_SET_BUNDLE_OUTPUT,
    CHANGE_SET_MANIFEST_OUTPUT,
    "codex-run-provenance",
    "codex-run-result",
    "codex-run-trace"
  ]);
  let grants: FactoryPermissionSet = serde_json::from_value(grants).unwrap();
  let budget = BudgetLimit::new(4, 600_000, 100, 1000, 400_000).unwrap();
  let schema = FlowDataSchema::new(key("declared.measurement"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let capture_schema = ImmutableReference::new(
    key("declared.capture-artifacts"),
    key("v1"),
    FactoryDigest::from_bytes([68; 32]),
  );
  let binding = FlowBuildBinding {
    tool: profile.toolchain.executable.clone(),
    result_tool: None,
    capture_write_paths: None,
    capture_deliverables: None,
    plugin: profile.toolchain.plugin.clone(),
    model_or_tool: profile.toolchain.model.clone(),
    task_digest: profile.task_digest(),
    build_configuration: profile.build_configuration.clone(),
    result_output: key("codex-run-result"),
    result_output_kind: EvidenceOutputKind::Report,
    result_outcome: key("observed"),
    evidence: vec![],
  };
  let node = |name: &str, permissions: FactoryPermissionSet, binding: FlowBuildBinding| {
    FlowNodeDefinition::new(FlowNodeDefinitionInput {
      key: key(name),
      kind: FlowNodeKind::Reasoning,
      input_schema: Some(schema.reference().clone()),
      outcomes: vec![FlowOutcomeDefinition::new(
        key("observed"),
        FlowOutcomeKind::Success,
        schema.reference().clone(),
      )],
      budget: BudgetLimit::new(4, 600_000, 100, 1000, 200_000).unwrap(),
      permissions,
      required: true,
      subflow: None,
    })
    .unwrap()
    .with_build(binding)
    .unwrap()
    .with_input_binding(
      FlowInputBinding::new(
        Default::default(),
        FlowDataMapping::new(FlowValueMapping::Constant { value: json!(true) }).unwrap(),
      )
      .unwrap(),
    )
    .unwrap()
  };
  let mut producer_binding = binding.clone();
  producer_binding.task_digest = FactoryDigest::from_bytes([69; 32]);
  producer_binding.evidence = [CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT]
    .into_iter()
    .map(|name| {
      EvidenceRequirement::new(
        key(name),
        EvidenceOutputKind::Artifact,
        capture_schema.clone(),
        capture_tool.clone(),
        profile.toolchain.plugin.clone(),
      )
    })
    .collect();
  let producer = node("operator-supplied-producer-51", grants.clone(), producer_binding);
  let selections = documents
    .iter()
    .filter(|(_, _, kind, _, _)| *kind == EvidenceOutputKind::Artifact)
    .map(|(id, name, _, _, bytes)| {
      let artifact =
        FactoryArtifactReference::new(*id, FactoryDigest::content_sha256(bytes), bytes.len() as u64).unwrap();
      (
        name.clone(),
        FlowContextSelection {
          when_present: None,
          source_kind: ContextSourceKind::Evidence,
          source: FlowDataMapping::new(FlowValueMapping::Constant {
            value: serde_json::to_value(FactoryContextReference::Artifact(artifact)).unwrap(),
          })
          .unwrap(),
        },
      )
    })
    .collect();
  let consumer_key = key("operator-selected-inspector-97");
  let consumer = node(consumer_key.as_str(), profile.project_permissions.clone(), binding)
    .with_input_binding(
      FlowInputBinding::new(
        BTreeMap::from([(
          key("selected-producer"),
          FlowInputSource::Predecessor {
            view: FlowRecordView::Payload,
            path: String::new(),
          },
        )]),
        FlowDataMapping::new(FlowValueMapping::Pointer {
          path: "/selected-producer".to_owned(),
        })
        .unwrap(),
      )
      .unwrap(),
    )
    .unwrap()
    .with_context_binding(
      FlowContextBinding::new(
        Some(
          FlowDataMapping::new(FlowValueMapping::Constant {
            value: json!(CANDIDATE),
          })
          .unwrap(),
        ),
        selections,
      )
      .unwrap(),
    )
    .unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(schema.reference().clone()),
    entry: producer.key().clone(),
    nodes: vec![producer.clone(), consumer],
    transitions: vec![
      FlowTransition::new(
        producer.key().clone(),
        key("observed"),
        FlowTransitionTarget::Node(consumer_key.clone()),
      ),
      FlowTransition::new(
        consumer_key.clone(),
        key("observed"),
        FlowTransitionTarget::Terminal(key("done")),
      ),
    ],
    terminals: vec![FlowTerminalDefinition::new(key("done"), schema.reference().clone())],
    context_projections: vec![],
    data_projections: vec![FlowDataProjection::new(
      producer.key().clone(),
      key("observed"),
      consumer_key.clone(),
      schema.reference().clone(),
    )],
    execution: FlowExecutionPolicy::new(budget, grants.clone(), 1).unwrap(),
  })
  .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), work);
  let root = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&root).unwrap();
  let flow = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    FlowAdmissionLimits::product_defaults(1, budget, grants).unwrap(),
    root,
    cycle,
    FactoryWipLimits::new(4, 4).unwrap(),
  )
  .unwrap()
  .with_data_schemas(vec![schema.clone()])
  .unwrap();
  CandidatePlan {
    flow,
    producer_id,
    consumer: consumer_key,
    documents,
    schema,
    capture_schema,
  }
}
