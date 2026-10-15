#[allow(dead_code)]
#[path = "../../core/octacity-server-factory/src/flow_test_support.rs"]
pub(crate) mod fixtures;
use crate::{
  CreateFactoryBuild, FactoryBuildAcceptance, FactoryNodeBuildExecutionFacts, FactoryNodeBuildOutputDocument,
  FactoryNodeBuildOutputSource, FactoryNodeBuildOutputs, OrdinaryBuildApplication, OrdinaryBuildApplicationError,
};
use async_trait::async_trait;
use octacity_protocol::*;
use octacity_server_artifacts::*;
use octacity_server_domain::{
  AttemptId, AttemptNumber, AttemptVersion, BuildId, BuildVersion, PipelineId, PipelineVersion, RepositoryVersion,
};
use octacity_server_factory as factory;
use octacity_server_factory::*;
use octacity_server_orchestrator::{AttemptState, BuildState};
use octacity_server_store::{
  AttemptRecord, BuildQueryStore, BuildRecord, BuildRetentionDeadlines, ImmutableBuildInput, StoreError,
};
use std::sync::Mutex;

/// The caller advances deterministic fixture time through the public owner call.
/// Tests for time elapsing inside a capability supply their own moving clock.
pub(crate) fn clock() -> std::sync::Arc<dyn crate::FactoryExecutionClock> {
  struct FixtureClock;
  impl crate::FactoryExecutionClock for FixtureClock {
    fn now(&self) -> Result<octacity_server_domain::Timestamp, crate::ApplicationError> {
      Ok(time(0))
    }
  }
  std::sync::Arc::new(FixtureClock)
}

/// Drives the separate durable admission and observation turns in graph tests.
/// Human and external waits remain visible; this performs at most two passes.
pub(crate) async fn reconcile_admission_then_observe(
  owner: &dyn crate::FactoryFlowCoordinator,
  store: &dyn octacity_server_store::FactoryRunStore,
  run: FactoryRunId,
  claim: FactoryDigest,
  at: octacity_server_domain::Timestamp,
) -> Result<crate::FactoryFlowStep, crate::ApplicationError> {
  let before = store.factory_run_snapshot(run).await?;
  let first = owner.reconcile_flow(run, claim, at).await?;
  if first != crate::FactoryFlowStep::Waiting {
    return Ok(first);
  }
  let admitted = store.factory_run_snapshot(run).await?;
  if admitted.flow.data.build_executions == before.flow.data.build_executions {
    return Ok(first);
  }
  assert_eq!(
    admitted.flow.data.build_executions.len(),
    before.flow.data.build_executions.len() + 1
  );
  for previous in &before.flow.data.build_executions {
    assert!(admitted.flow.data.build_executions.contains(previous));
  }
  assert_eq!(admitted.flow.data.build_intents, before.flow.data.build_intents);
  let added = admitted
    .flow
    .data
    .build_executions
    .iter()
    .find(|execution| !before.flow.data.build_executions.contains(execution))
    .unwrap();
  let intent = admitted
    .flow
    .data
    .build_intents
    .iter()
    .find(|intent| intent.node().id() == added.node_attempt_id())
    .unwrap();
  assert_eq!(added.operation_id(), intent.operation_id().unwrap());
  assert!(!added.jobs().is_empty());
  let observed = owner.reconcile_flow(run, claim, at).await?;
  let after = store.factory_run_snapshot(run).await?;
  assert_eq!(after.flow.data.build_intents, admitted.flow.data.build_intents);
  assert_eq!(after.flow.data.build_executions, admitted.flow.data.build_executions);
  Ok(observed)
}

pub(crate) struct Builds {
  pub(crate) origin_override: Mutex<Option<octacity_server_store::BuildOrigin>>,
  pub(crate) created_at: octacity_server_domain::Timestamp,
  pub(crate) requests: Mutex<Vec<CreateFactoryBuild>>,
  pub(crate) acceptance: FactoryBuildAcceptance,
  pub(crate) state: Mutex<BuildState>,
  pub(crate) latest: Mutex<Option<AttemptRecord>>,
  pub(crate) previous: Mutex<Vec<AttemptRecord>>,
  pub(crate) lose_response: std::sync::atomic::AtomicBool,
}
impl Builds {
  pub(crate) fn new() -> Self {
    Self::new_at(time(30))
  }
  pub(crate) fn new_at(created_at: octacity_server_domain::Timestamp) -> Self {
    Self {
      origin_override: Mutex::new(None),
      created_at,
      requests: Mutex::new(vec![]),
      acceptance: FactoryBuildAcceptance {
        build_id: octacity_server_domain::BuildId::generate(),
        attempt_id: octacity_server_domain::AttemptId::generate(),
        job_ids: vec![octacity_server_domain::JobId::generate()],
        effective_policy_digest: FactoryPermissionSet::deny_all().digest(),
      },
      state: Mutex::new(BuildState::Queued),
      latest: Mutex::new(None),
      previous: Mutex::new(vec![]),
      lose_response: std::sync::atomic::AtomicBool::new(false),
    }
  }
}
#[async_trait]
impl OrdinaryBuildApplication for Builds {
  async fn factory_build_for_operation(
    &self,
    operation_id: FactoryDigest,
  ) -> Result<Option<FactoryBuildAcceptance>, OrdinaryBuildApplicationError> {
    Ok(
      self
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|row| row.operation_id == operation_id)
        .then(|| self.acceptance.clone()),
    )
  }
  async fn create_factory_build(
    &self,
    request: CreateFactoryBuild,
  ) -> Result<FactoryBuildAcceptance, OrdinaryBuildApplicationError> {
    self.requests.lock().unwrap().push(request);
    if self.lose_response.swap(false, std::sync::atomic::Ordering::SeqCst) {
      return Err(OrdinaryBuildApplicationError::Unavailable);
    }
    Ok(self.acceptance.clone())
  }
}

#[async_trait]
impl crate::OrdinaryBuildCancellationApplication for Builds {
  async fn cancel_factory_build(
    &self,
    request: crate::CancelFactoryBuild,
  ) -> Result<(), OrdinaryBuildApplicationError> {
    let accepted = self.factory_build_for_operation(request.build_operation_id).await?;
    if accepted.is_none_or(|accepted| accepted.build_id != request.build_id) {
      return Err(OrdinaryBuildApplicationError::Invalid);
    }
    *self.state.lock().unwrap() = BuildState::Cancelled;
    Ok(())
  }
}

#[async_trait]
impl BuildQueryStore for Builds {
  async fn build(&self, build_id: BuildId) -> Result<BuildRecord, StoreError> {
    let request = self.requests.lock().unwrap()[0].clone();
    let configuration = &request.build_configuration;
    if build_id != self.acceptance.build_id {
      return Err(StoreError::Unavailable);
    }
    let state = *self.state.lock().unwrap();
    let origin = self.origin_override.lock().unwrap().clone().unwrap_or_else(|| {
      octacity_server_store::BuildOrigin::Factory(octacity_server_store::FactoryBuildOrigin {
        operation_id: request.operation_id,
        run_id: request.causality.run_id,
        node_attempt_id: request.causality.node_attempt_id,
        input_digest: request.causality.input_digest,
      })
    });
    Ok(BuildRecord {
      build: ImmutableBuildInput {
        id: build_id,
        project_id: request.project_id,
        configuration_id: configuration.id(),
        configuration_version: configuration.version(),
        pipeline_id: PipelineId::generate(),
        pipeline_version: PipelineVersion::INITIAL,
        repository_id: request.repository_id,
        repository_version: RepositoryVersion::INITIAL,
        immutable_revision: request.immutable_revision,
        input_snapshot: serde_json::json!({}),
        effective_policy_snapshot: serde_json::json!({}),
        retention: BuildRetentionDeadlines {
          metadata: time(10_000),
          logs: time(10_000),
          artifacts: time(10_000),
          reports: time(10_000),
        },
        project_job_concurrency_limit: 1,
        priority: request.priority,
      },
      state,
      version: BuildVersion::INITIAL,
      origin,
      created_at: self.created_at,
      updated_at: self.created_at,
    })
  }

  async fn attempt(&self, attempt_id: AttemptId) -> Result<AttemptRecord, StoreError> {
    if let Some(attempt) = self.previous.lock().unwrap().iter().find(|row| row.id == attempt_id) {
      return Ok(attempt.clone());
    }
    let latest = self.latest_attempt(self.acceptance.build_id).await?;
    if latest.id != attempt_id {
      return Err(StoreError::Unavailable);
    }
    Ok(latest)
  }

  async fn job(&self, job_id: octacity_server_domain::JobId) -> Result<octacity_server_store::JobRecord, StoreError> {
    if let Some(attempt) = self.latest.lock().unwrap().as_ref()
      && let Some(job) = attempt.jobs.iter().find(|job| job.id() == job_id)
    {
      return Ok(job.clone());
    }
    if let Some(job) = self
      .previous
      .lock()
      .unwrap()
      .iter()
      .flat_map(|attempt| &attempt.jobs)
      .find(|job| job.id() == job_id)
    {
      return Ok(job.clone());
    }
    if !self.acceptance.job_ids.contains(&job_id) {
      return Err(StoreError::Unavailable);
    }
    let state = match *self.state.lock().unwrap() {
      BuildState::Queued => octacity_server_job::JobState::Ready,
      BuildState::Running => octacity_server_job::JobState::Running,
      BuildState::Succeeded => octacity_server_job::JobState::Succeeded,
      BuildState::Failed => octacity_server_job::JobState::Failed,
      BuildState::Cancelled => octacity_server_job::JobState::Cancelled,
    };
    Ok(job_record(self, self.acceptance.attempt_id, job_id, state))
  }

  async fn latest_attempt(&self, build_id: BuildId) -> Result<AttemptRecord, StoreError> {
    if build_id != self.acceptance.build_id {
      return Err(StoreError::Unavailable);
    }
    if let Some(attempt) = self.latest.lock().unwrap().as_ref() {
      return Ok(attempt.clone());
    }
    let state = match *self.state.lock().unwrap() {
      BuildState::Queued => AttemptState::Created,
      BuildState::Running => AttemptState::Running,
      BuildState::Succeeded => AttemptState::Succeeded,
      BuildState::Failed => AttemptState::Failed,
      BuildState::Cancelled => AttemptState::Cancelled,
    };
    Ok(AttemptRecord {
      id: self.acceptance.attempt_id,
      build_id,
      number: AttemptNumber::FIRST,
      retry_of_attempt_id: None,
      state,
      version: AttemptVersion::INITIAL,
      created_at: self.created_at,
      updated_at: self.created_at,
      jobs: self
        .acceptance
        .job_ids
        .iter()
        .map(|id| {
          job_record(
            self,
            self.acceptance.attempt_id,
            *id,
            match state {
              AttemptState::Created => octacity_server_job::JobState::Ready,
              AttemptState::Running => octacity_server_job::JobState::Running,
              AttemptState::Succeeded => octacity_server_job::JobState::Succeeded,
              AttemptState::Failed => octacity_server_job::JobState::Failed,
              AttemptState::Cancelled => octacity_server_job::JobState::Cancelled,
            },
          )
        })
        .collect(),
    })
  }
}

pub(crate) struct Outputs(pub(crate) Option<FactoryNodeBuildOutputs>);
#[async_trait]
impl FactoryNodeBuildOutputSource for Outputs {
  async fn terminal_usage(
    &self,
    _: octacity_server_domain::BuildId,
    _: octacity_server_domain::AttemptId,
  ) -> Result<BudgetUsage, OrdinaryBuildApplicationError> {
    panic!("terminal consumption must be supplied explicitly by this fixture")
  }

  async fn published_outputs(
    &self,
    build: octacity_server_domain::BuildId,
    attempt: octacity_server_domain::AttemptId,
    _: u64,
  ) -> Result<crate::FactoryNodeBuildOutputObservation, OrdinaryBuildApplicationError> {
    let outputs = self.0.clone().expect("pending Build cannot publish results");
    assert!(
      outputs
        .documents
        .iter()
        .all(|doc| doc.record.identity().build_id == build && doc.record.identity().attempt_id == attempt)
    );
    Ok(crate::FactoryNodeBuildOutputObservation::Verified(Box::new(outputs)))
  }
}

pub(crate) fn document(
  builds: &Builds,
  name: &FactoryKey,
  schema: &ImmutableReference,
  bytes: Vec<u8>,
  tool: ImmutableReference,
) -> FactoryNodeBuildOutputDocument {
  document_kind(builds, name, schema, bytes, tool, EvidenceOutputKind::Report)
}

pub(crate) fn document_kind(
  builds: &Builds,
  name: &FactoryKey,
  schema: &ImmutableReference,
  bytes: Vec<u8>,
  tool: ImmutableReference,
  kind: EvidenceOutputKind,
) -> FactoryNodeBuildOutputDocument {
  document_kind_at(builds, name, schema, bytes, tool, kind, builds.created_at, None)
}

// Observation receipts become immutable when their measurement window finishes,
// which may be later than Build creation. Other fixtures keep the existing time.
pub(crate) fn document_at(
  builds: &Builds,
  name: &FactoryKey,
  schema: &ImmutableReference,
  bytes: Vec<u8>,
  tool: ImmutableReference,
  published_at: octacity_server_domain::Timestamp,
) -> FactoryNodeBuildOutputDocument {
  document_kind_at(
    builds,
    name,
    schema,
    bytes,
    tool,
    EvidenceOutputKind::Report,
    published_at,
    None,
  )
}

pub(crate) fn protocol_document(
  builds: &Builds,
  name: &FactoryKey,
  schema: &ImmutableReference,
  bytes: Vec<u8>,
  tool: ImmutableReference,
  report_format: &str,
) -> FactoryNodeBuildOutputDocument {
  document_kind_at(
    builds,
    name,
    schema,
    bytes,
    tool,
    EvidenceOutputKind::Report,
    builds.created_at,
    Some(report_format),
  )
}

#[expect(
  clippy::too_many_arguments,
  reason = "published output fixture binds metadata and independent schema attestation"
)]
fn document_kind_at(
  builds: &Builds,
  name: &FactoryKey,
  schema: &ImmutableReference,
  bytes: Vec<u8>,
  tool: ImmutableReference,
  kind: EvidenceOutputKind,
  published_at: octacity_server_domain::Timestamp,
  report_format: Option<&str>,
) -> FactoryNodeBuildOutputDocument {
  use octacity_server_artifacts::*;
  let (attempt_id, job_id) = {
    let latest = builds.latest.lock().unwrap();
    latest.as_ref().map_or(
      (builds.acceptance.attempt_id, builds.acceptance.job_ids[0]),
      |attempt| (attempt.id, attempt.jobs[0].id()),
    )
  };
  let identity = ArtifactIdentity {
    artifact_id: octacity_server_domain::ArtifactId::generate(),
    build_id: builds.acceptance.build_id,
    attempt_id,
    job_id,
    lease_id: octacity_server_domain::LeaseId::generate(),
    logical_name: octacity_server_domain::ArtifactName::new(name.as_str()).unwrap(),
    artifact_type: match kind {
      EvidenceOutputKind::Report => {
        ArtifactType::Report(ArtifactReportFormat::new(report_format.unwrap_or(schema.identity().as_str())).unwrap())
      }
      EvidenceOutputKind::Artifact => ArtifactType::Artifact,
    },
    media_type: ArtifactMediaType::new("application/json").unwrap(),
    size_bytes: bytes.len() as u64,
    digest: ArtifactContentDigest::from_bytes(FactoryDigest::content_sha256(&bytes).as_bytes()),
    retention: ArtifactRetentionPolicy::DeleteAfter(time(10000)),
  };
  let record = ArtifactRecord::pending(identity.clone(), builds.created_at).unwrap();
  let record = record
    .transition(
      &identity,
      record.version(),
      ArtifactEvent::BeginVerification,
      published_at,
    )
    .unwrap();
  let record = record
    .transition(&identity, record.version(), ArtifactEvent::Publish, published_at)
    .unwrap();
  FactoryNodeBuildOutputDocument {
    record,
    bytes,
    data: None,
    schema: schema.clone(),
    tool,
    plugin: reference("runner"),
  }
}

fn time(value: i64) -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(value).unwrap()
}
fn reference(value: &str) -> ImmutableReference {
  ImmutableReference::new(
    FactoryKey::new(value).unwrap(),
    FactoryKey::new("v1").unwrap(),
    FactoryDigest::from_bytes([60; 32]),
  )
}

pub(crate) fn complete_ordinary_retry(builds: &Builds) -> (AttemptId, octacity_server_domain::JobId) {
  let attempt_id = AttemptId::generate();
  let job_id = octacity_server_domain::JobId::generate();
  builds.previous.lock().unwrap().push(AttemptRecord {
    id: builds.acceptance.attempt_id,
    build_id: builds.acceptance.build_id,
    number: AttemptNumber::FIRST,
    retry_of_attempt_id: None,
    state: AttemptState::Failed,
    version: AttemptVersion::INITIAL,
    created_at: builds.created_at,
    updated_at: builds.created_at,
    jobs: builds
      .acceptance
      .job_ids
      .iter()
      .map(|id| {
        job_record(
          builds,
          builds.acceptance.attempt_id,
          *id,
          octacity_server_job::JobState::Failed,
        )
      })
      .collect(),
  });
  *builds.latest.lock().unwrap() = Some(AttemptRecord {
    id: attempt_id,
    build_id: builds.acceptance.build_id,
    number: AttemptNumber::new(2).unwrap(),
    retry_of_attempt_id: Some(builds.acceptance.attempt_id),
    state: AttemptState::Succeeded,
    version: AttemptVersion::INITIAL,
    created_at: builds.created_at,
    updated_at: builds.created_at,
    jobs: vec![job_record(
      builds,
      attempt_id,
      job_id,
      octacity_server_job::JobState::Succeeded,
    )],
  });
  (attempt_id, job_id)
}

fn job_record(
  builds: &Builds,
  attempt_id: AttemptId,
  job_id: octacity_server_domain::JobId,
  state: octacity_server_job::JobState,
) -> octacity_server_store::JobRecord {
  let job = octacity_server_store::MaterializedJob::new(
    job_id,
    octacity_server_domain::PipelineNodeId::new("configured_node").unwrap(),
    vec![],
    octacity_server_pipeline::DependencyPolicy::AllSucceeded,
    vec![octacity_server_domain::PoolId::generate()],
    octacity_server_store::MaterializedJobPayload::new(
      octacity_server_job::JobRequirements {
        capabilities: Default::default(),
        labels: Default::default(),
        minimum_cpu_millis: 0,
        minimum_memory_bytes: 0,
        minimum_disk_bytes: 0,
        runtime_class: octacity_server_domain::RuntimeClass::Native,
        operating_system: octacity_protocol::PlatformOs::Linux,
        architecture: octacity_protocol::PlatformArchitecture::Amd64,
        host_platform: None,
        required_guarantees: Default::default(),
      },
      octacity_server_store::testing::job_spec_template(builds.acceptance.build_id, "configured_node"),
    )
    .unwrap(),
  )
  .unwrap();
  octacity_server_store::JobRecord {
    attempt_id,
    job,
    state,
    version: octacity_server_domain::JobVersion::INITIAL,
    created_at: builds.created_at,
    updated_at: builds.created_at,
    queue: None,
    assignment: None,
    terminal: None,
    event_cursor: 0,
  }
}

/// Prepares explicitly selected candidate context without any logical stage role.
pub(crate) fn input_with_candidate_context(
  admitted: &AdmittedFlow,
  input: FlowNodeInput,
  node: &NodeAttempt,
  candidate: CandidateSubject,
) -> (FlowNodeInput, NodeAttempt) {
  let previous = input.context().unwrap();
  let subject = FactoryTaskSubject::Candidate(candidate);
  let entries = previous
    .entries()
    .iter()
    .map(|entry| {
      ContextManifestEntry::new(
        entry.source_kind(),
        entry.logical_identity().clone(),
        subject.clone(),
        entry.source().clone(),
        entry.content_digest(),
        entry.encoded_size(),
        entry.inclusion_reason().clone(),
        entry.provenance_digest(),
      )
      .unwrap()
    })
    .collect();
  let context = ContextManifest::new(previous.id(), subject, previous.construction_policy_digest(), entries).unwrap();
  let input = input.with_context(context, vec![]).unwrap();
  let definition = admitted.closure().definition(input.definition()).unwrap();
  let updated = NodeAttempt::new(
    admitted.root_run(),
    admitted.initial_cycle(),
    definition,
    NodeAttemptInput {
      started_at: node.started_at(),
      id: node.id(),
      node_key: node.node_key().clone(),
      node_kind: node.node_kind(),
      number: node.number(),
      input_digest: input.digest().unwrap(),
      budget: node.budget(),
      deadline: node.deadline(),
      execution: node.execution().clone(),
      ownership: FactoryClaimOwnership::new(node.owner().clone(), node.claim()),
    },
  )
  .unwrap();
  (input, updated)
}

pub(crate) fn capture_outputs(intent: &FlowBuildIntent, builds: &Builds, base: &str) -> FactoryNodeBuildOutputs {
  let tool = reference("git.capture");
  let bundle = b"trusted fixture bundle\n".to_vec();
  let manifest = CapturedChangeSetManifestV1 {
    cumulative_changes: None,
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
      &fixtures::key(name),
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
