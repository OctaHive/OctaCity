#[allow(dead_code)]
#[path = "../../core/octacity-server-factory/src/flow_test_support.rs"]
pub(crate) mod fixtures;
use crate::{
  CreateFactoryBuild, FactoryBuildAcceptance, FactoryNodeBuildOutputDocument, FactoryNodeBuildOutputSource,
  FactoryNodeBuildOutputs, OrdinaryBuildApplication, OrdinaryBuildApplicationError,
};
use async_trait::async_trait;
use octacity_server_domain::{
  AttemptId, AttemptNumber, AttemptVersion, BuildId, BuildVersion, PipelineId, PipelineVersion, RepositoryVersion,
  TriggerId, TriggerIdentity, TriggerOccurrenceId, TriggerVersion,
};
use octacity_server_factory as factory;
use octacity_server_factory::*;
use octacity_server_orchestrator::{AttemptState, BuildState};
use octacity_server_store::{
  AttemptRecord, BuildQueryStore, BuildRecord, BuildRetentionDeadlines, ImmutableBuildInput,
  NormalizedTriggerOccurrence, StoreError, TriggerCause, TriggerDefinitionRef, TriggerMetadata, TriggerOccurrenceState,
  TriggerTarget,
};
use std::sync::Mutex;

pub(crate) struct Builds {
  pub(crate) created_at: octacity_server_domain::Timestamp,
  pub(crate) requests: Mutex<Vec<CreateFactoryBuild>>,
  pub(crate) acceptance: FactoryBuildAcceptance,
  pub(crate) state: Mutex<BuildState>,
  pub(crate) latest: Mutex<Option<AttemptRecord>>,
  pub(crate) lose_response: std::sync::atomic::AtomicBool,
}
impl Builds {
  pub(crate) fn new() -> Self {
    Self::new_at(time(30))
  }
  pub(crate) fn new_at(created_at: octacity_server_domain::Timestamp) -> Self {
    Self {
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
impl BuildQueryStore for Builds {
  async fn build(&self, build_id: BuildId) -> Result<BuildRecord, StoreError> {
    let request = self.requests.lock().unwrap()[0].clone();
    let configuration = &request.build_configuration;
    if build_id != self.acceptance.build_id {
      return Err(StoreError::Unavailable);
    }
    let state = *self.state.lock().unwrap();
    let trigger = NormalizedTriggerOccurrence::root(
      TriggerOccurrenceId::generate(),
      TriggerDefinitionRef {
        id: TriggerId::generate(),
        version: TriggerVersion::INITIAL,
      },
      TriggerTarget {
        configuration_id: configuration.id(),
        configuration_version: configuration.version(),
      },
      TriggerIdentity::new("factory-build-fixture").unwrap(),
      TriggerCause::Manual {},
      TriggerMetadata::default(),
      self.created_at,
    )
    .unwrap();
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
      trigger,
      trigger_state: TriggerOccurrenceState::Accepted,
      trigger_created_at: self.created_at,
      trigger_updated_at: self.created_at,
      created_at: self.created_at,
      updated_at: self.created_at,
    })
  }

  async fn attempt(&self, _attempt_id: AttemptId) -> Result<AttemptRecord, StoreError> {
    self.latest_attempt(self.acceptance.build_id).await
  }

  async fn job(&self, _job_id: octacity_server_domain::JobId) -> Result<octacity_server_store::JobRecord, StoreError> {
    Err(StoreError::Unavailable)
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
      jobs: Vec::new(),
    })
  }
}

pub(crate) struct Outputs(pub(crate) Option<FactoryNodeBuildOutputs>);
#[async_trait]
impl FactoryNodeBuildOutputSource for Outputs {
  async fn published_outputs(
    &self,
    build: octacity_server_domain::BuildId,
    attempt: octacity_server_domain::AttemptId,
    _: u64,
  ) -> Result<FactoryNodeBuildOutputs, OrdinaryBuildApplicationError> {
    let outputs = self.0.clone().expect("pending Build cannot publish results");
    assert!(
      outputs
        .documents
        .iter()
        .all(|doc| doc.record.identity().build_id == build && doc.record.identity().attempt_id == attempt)
    );
    Ok(outputs)
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
        ArtifactType::Report(ArtifactReportFormat::new(schema.identity().as_str()).unwrap())
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
      builds.created_at,
    )
    .unwrap();
  let record = record
    .transition(&identity, record.version(), ArtifactEvent::Publish, builds.created_at)
    .unwrap();
  FactoryNodeBuildOutputDocument {
    record,
    bytes,
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
  *builds.latest.lock().unwrap() = Some(AttemptRecord {
    id: attempt_id,
    build_id: builds.acceptance.build_id,
    number: AttemptNumber::new(2).unwrap(),
    retry_of_attempt_id: Some(builds.acceptance.attempt_id),
    state: AttemptState::Succeeded,
    version: AttemptVersion::INITIAL,
    created_at: builds.created_at,
    updated_at: builds.created_at,
    jobs: vec![octacity_server_store::JobRecord {
      attempt_id,
      job,
      state: octacity_server_job::JobState::Succeeded,
      version: octacity_server_domain::JobVersion::INITIAL,
      created_at: builds.created_at,
      updated_at: builds.created_at,
      queue: None,
      assignment: None,
      terminal: None,
      event_cursor: 0,
    }],
  });
  (attempt_id, job_id)
}
