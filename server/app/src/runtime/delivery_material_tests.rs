//! Public retained-input handoff to a private installed delivery connector.
use super::delivery::*;
use super::*;
use octacity_artifact_store::{ArtifactByteReader, ArtifactObject, ArtifactStoreError};
use octacity_server_application::{ApplicationError, FactoryArtifactReader, FactoryExecutionClock};
use octacity_server_domain::*;
use octacity_server_factory::*;
use octacity_server_store::{testing::*, *};
use std::{collections::BTreeMap, sync::Arc};
fn at(n: i64) -> Timestamp {
  Timestamp::from_unix_millis(n).unwrap()
}
struct Clock;
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(10))
  }
}
struct Bytes(BTreeMap<ArtifactId, Vec<u8>>);
#[async_trait::async_trait]
impl ArtifactByteReader for Bytes {
  async fn read_published(&self, object: &ArtifactObject, _: u64) -> Result<Vec<u8>, ArtifactStoreError> {
    Ok(self.0[&object.artifact_id()].clone())
  }
}
struct Builds {
  project: ProjectId,
  id: BuildId,
}
#[async_trait::async_trait]
impl BuildQueryStore for Builds {
  async fn build(&self, id: BuildId) -> Result<BuildRecord, StoreError> {
    assert_eq!(id, self.id);
    Ok(BuildRecord {
      build: ImmutableBuildInput {
        id,
        project_id: self.project,
        configuration_id: BuildConfigurationId::generate(),
        configuration_version: BuildConfigurationVersion::INITIAL,
        pipeline_id: PipelineId::generate(),
        pipeline_version: PipelineVersion::INITIAL,
        repository_id: Some(RepositoryId::generate()),
        repository_version: Some(RepositoryVersion::INITIAL),
        immutable_revision: Some(ImmutableRevision::new("1".repeat(40)).unwrap()),
        source: None,
        input_snapshot: serde_json::json!({}),
        effective_policy_snapshot: serde_json::json!({}),
        retention: BuildRetentionDeadlines {
          metadata: at(1000),
          logs: at(1000),
          artifacts: at(1000),
          reports: at(1000),
        },
        project_job_concurrency_limit: 1,
        priority: 0,
      },
      state: octacity_server_orchestrator::BuildState::Succeeded,
      version: BuildVersion::INITIAL,
      origin: BuildOrigin::Factory(FactoryBuildOrigin {
        operation_id: FactoryDigest::from_bytes([1; 32]),
        run_id: FactoryRunId::generate(),
        node_attempt_id: NodeAttemptId::generate(),
        input_digest: FactoryDigest::from_bytes([2; 32]),
      }),
      created_at: at(1),
      updated_at: at(2),
    })
  }
  async fn attempt(&self, _: AttemptId) -> Result<AttemptRecord, StoreError> {
    panic!("delivery only reads owning Build")
  }
  async fn job(&self, _: JobId) -> Result<JobRecord, StoreError> {
    panic!("delivery never reads signed Job")
  }
  async fn latest_attempt(&self, _: BuildId) -> Result<AttemptRecord, StoreError> {
    panic!("delivery uses exact retained artifacts")
  }
}
async fn published(
  store: &InMemoryArtifactRecordStore,
  lease: &ArtifactLeaseFixture,
  name: &str,
  body: &[u8],
) -> FactoryArtifactReference {
  use sha2::{Digest as _, Sha256};
  let digest: [u8; 32] = Sha256::digest(body).into();
  let id = ArtifactId::generate();
  let upload = store
    .begin_artifact_upload(BeginArtifactUpload {
      artifact_id: id,
      upload_id: ArtifactUploadId::generate(),
      idempotency_key: IdempotencyKey::new(name).unwrap(),
      lease: lease.access,
      job_id: lease.job_id,
      attempt: AttemptNumber::FIRST,
      logical_name: ArtifactName::new(name).unwrap(),
      producer_run_id: 1,
      producer_task_id: 1,
      artifact_type: ArtifactType::Artifact,
      media_type: ArtifactMediaType::new("application/octet-stream").unwrap(),
      transport_media_type: ArtifactMediaType::new("application/octet-stream").unwrap(),
      size_bytes: body.len() as u64,
      digest: ArtifactContentDigest::from_bytes(digest),
      reserved_at: at(1),
      capability_expires_at: at(1000),
    })
    .await
    .unwrap();
  let verify = VerifyArtifactUpload {
    upload_id: upload.upload.upload_id,
    lease: lease.access,
    job_id: lease.job_id,
    attempt: AttemptNumber::FIRST,
    observed_at: at(2),
  };
  store.begin_artifact_verification(verify).await.unwrap();
  store
    .finish_artifact_verification(verify, ArtifactVerificationResult::Verified)
    .await
    .unwrap();
  FactoryArtifactReference::new(id, FactoryDigest::from_bytes(digest), body.len() as u64).unwrap()
}
#[tokio::test]
async fn retained_delivery_materialization_stages_exact_private_bytes_and_replays_without_overwrite() {
  use sha2::{Digest as _, Sha256};
  let project = ProjectId::generate();
  let lease = ArtifactLeaseFixture {
    access: LeaseAccess {
      lease_id: LeaseId::generate(),
      fence: LeaseFence::from_bytes([1; 32]),
      agent_id: AgentId::generate(),
      registration_epoch: RegistrationEpoch::new(1).unwrap(),
    },
    build_id: BuildId::generate(),
    attempt_id: AttemptId::generate(),
    attempt: AttemptNumber::FIRST,
    job_id: JobId::generate(),
    expires_at: at(1000),
  };
  let store = Arc::new(InMemoryArtifactRecordStore::new(lease.clone()));
  let bundle = published(&store, &lease, "candidate.bundle", b"private-candidate-canary").await;
  let alternate_bundle = published(&store, &lease, "other.bundle", b"other-candidate-bytes").await;
  let captured: octacity_protocol::CapturedChangeSetManifestV1 = serde_json::from_value(serde_json::json!({
    "format_version":1,"base_revision":"5".repeat(40),"candidate_revision":"2".repeat(40),
    "node_attempt_id":"accepted-attempt","capture_tool":{"identity":"capture","version":"v1","sha256":"f".repeat(64)},
    "changed_paths":[{"status":"M","path":"public/facts.txt","old_mode":"100644","new_mode":"100644","content_sha256":bundle.content_digest()}],
    "cumulative_changes":{"base_revision":"1".repeat(40),"changed_paths":[{"status":"M","path":"public/facts.txt","old_mode":"100644","new_mode":"100644","content_sha256":bundle.content_digest()}]},
    "bundle":{"name":"change-set.bundle","size_bytes":bundle.encoded_size(),"sha256":bundle.content_digest()}
  })).unwrap();
  let mut manifest_bytes = serde_json::to_vec(&captured).unwrap();
  manifest_bytes.push(b'\n');
  octacity_protocol::CapturedChangeSetManifestV1::decode_canonical(&manifest_bytes).unwrap();
  let manifest = published(&store, &lease, "capture.manifest", &manifest_bytes).await;
  let candidate_digest = captured
    .candidate_digest(&manifest.content_digest().to_string())
    .unwrap()
    .parse()
    .unwrap();
  assert_ne!(candidate_digest, manifest.content_digest());
  let reader = Arc::new(FactoryArtifactReader::new(
    store,
    Arc::new(Builds {
      project,
      id: lease.build_id,
    }),
    Arc::new(Bytes(BTreeMap::from([
      (bundle.artifact_id(), b"private-candidate-canary".to_vec()),
      (manifest.artifact_id(), manifest_bytes.clone()),
      (alternate_bundle.artifact_id(), b"other-candidate-bytes".to_vec()),
    ]))),
  ));
  let root = tempfile::tempdir().unwrap();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
  }
  let target = ImmutableReference::new(
    FactoryKey::new("installed-delivery").unwrap(),
    FactoryKey::new("v1").unwrap(),
    FactoryDigest::from_bytes([4; 32]),
  );
  let request = octacity_server_application::FactoryCandidatePublicationRequest {
    operation_id: FactoryDigest::from_bytes([1; 32]),
    work_id: WorkEnvelopeId::generate(),
    cycle_id: WorkflowCycleId::generate(),
    accepted_input_digest: FactoryDigest::from_bytes([3; 32]),
    candidate: CandidateSubject::new(
      ExactSubject::new(
        project,
        RepositoryId::generate(),
        ImmutableRevision::new("1".repeat(40)).unwrap(),
      ),
      ImmutableRevision::new("2".repeat(40)).unwrap(),
      candidate_digest,
    ),
    bundle: bundle.clone(),
    manifest: manifest.clone(),
    target: target.clone(),
    permissions: FactoryPermissionSet::deny_all(),
    budget: BudgetLimit::new(1, 1000, 1, 1, 10000).unwrap(),
    deadline: at(1000),
    started_at: at(1),
    authority_expires_at: at(1000),
  };
  let inputs = FactoryDeliveryInputs {
    bundle: octacity_delivery_protocol::ArtifactInput {
      handle: format!("factory.delivery:{}:bundle", request.operation_id),
      sha256: bundle.content_digest().to_string(),
      size_bytes: bundle.encoded_size(),
    },
    manifest: octacity_delivery_protocol::ArtifactInput {
      handle: format!("factory.delivery:{}:manifest", request.operation_id),
      sha256: manifest.content_digest().to_string(),
      size_bytes: manifest.encoded_size(),
    },
  };
  let material = RetainedFactoryDeliveryMaterializer::new(
    reader,
    BTreeMap::from([(target, root.path().to_owned())]),
    Arc::new(Clock),
  )
  .unwrap();
  for field in ["digest", "revision", "base", "bundle"] {
    let mut substituted = request.clone();
    let subject = request.candidate.exact();
    substituted.candidate = CandidateSubject::new(
      ExactSubject::new(
        subject.project_id(),
        subject.repository_id(),
        ImmutableRevision::new(if field == "base" {
          "4".repeat(40)
        } else {
          subject.base_revision().to_string()
        })
        .unwrap(),
      ),
      ImmutableRevision::new(if field == "revision" {
        "3".repeat(40)
      } else {
        request.candidate.candidate_revision().to_string()
      })
      .unwrap(),
      if field == "digest" {
        FactoryDigest::from_bytes([12; 32])
      } else {
        candidate_digest
      },
    );
    if field == "bundle" {
      substituted.bundle = alternate_bundle.clone();
    }
    let selected = FactoryDeliveryInputs {
      bundle: delivery_artifact_input(substituted.operation_id, "bundle", &substituted.bundle),
      manifest: delivery_artifact_input(substituted.operation_id, "manifest", &substituted.manifest),
    };
    assert!(
      material.stage(&substituted, &selected).await.is_err(),
      "substituted {field} must fail before staging"
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
  }
  material.stage(&request, &inputs).await.unwrap();
  material.stage(&request, &inputs).await.unwrap();
  let path = |handle: &str| root.path().join(format!("{}.bin", hex::encode(Sha256::digest(handle))));
  let bundle_path = path(&inputs.bundle.handle);
  assert_eq!(std::fs::read(&bundle_path).unwrap(), b"private-candidate-canary");
  assert_eq!(std::fs::read(path(&inputs.manifest.handle)).unwrap(), manifest_bytes);
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    assert_eq!(
      std::fs::metadata(&bundle_path).unwrap().permissions().mode() & 0o777,
      0o600
    );
  }
  std::fs::write(&bundle_path, b"unrelated-connector-input").unwrap();
  assert!(material.stage(&request, &inputs).await.is_err());
  assert_eq!(std::fs::read(&bundle_path).unwrap(), b"unrelated-connector-input");
}
