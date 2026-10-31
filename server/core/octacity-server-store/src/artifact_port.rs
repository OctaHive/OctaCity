use async_trait::async_trait;
use octacity_server_domain::{ArtifactId, ArtifactUploadId, EntityKind, JobId};

use crate::{
  ArtifactRecord, ArtifactUploadRecord, ArtifactVerificationResult, BeginArtifactUpload, BeginArtifactUploadOutcome,
  ListPublishedArtifacts, ReserveArtifact, StoreError, TransitionArtifact, VerifyArtifactUpload,
};

/// Persistence port for complete logical Artifact lifecycle operations.
#[async_trait]
pub trait ArtifactRecordStore: Send + Sync {
  /// Reserves one pending record after validating current fenced Lease authority.
  async fn reserve_artifact(&self, request: ReserveArtifact) -> Result<ArtifactRecord, StoreError>;

  /// Reads one logical record, including non-visible lifecycle states.
  async fn artifact(&self, artifact_id: ArtifactId) -> Result<ArtifactRecord, StoreError>;

  /// Applies one state transition while preserving immutable identity.
  async fn transition_artifact(&self, request: TransitionArtifact) -> Result<ArtifactRecord, StoreError>;

  /// Atomically reserves logical metadata and a pending upload before any capability is returned.
  async fn begin_artifact_upload(&self, request: BeginArtifactUpload)
  -> Result<BeginArtifactUploadOutcome, StoreError>;

  /// Reads one upload and its complete logical Artifact record.
  async fn artifact_upload(&self, upload_id: ArtifactUploadId) -> Result<ArtifactUploadRecord, StoreError>;

  /// Starts verification or replays an already verifying/published upload.
  async fn begin_artifact_verification(
    &self,
    request: VerifyArtifactUpload,
  ) -> Result<ArtifactUploadRecord, StoreError>;

  /// Atomically publishes verified bytes or returns a rejected upload to pending state.
  async fn finish_artifact_verification(
    &self,
    request: VerifyArtifactUpload,
    result: ArtifactVerificationResult,
  ) -> Result<ArtifactUploadRecord, StoreError>;

  /// Reads a published logical Artifact and its backend-neutral upload identity.
  async fn published_artifact(&self, artifact_id: ArtifactId) -> Result<ArtifactUploadRecord, StoreError>;

  /// Reads verified metadata for trusted owners without fabricating Agent provenance.
  /// Private compiler inputs remain excluded from ordinary output discovery.
  /// Metadata alone grants no download authority; consumers use the scoped Job lookup.
  async fn published_execution_input(
    &self,
    artifact_id: ArtifactId,
  ) -> Result<crate::PublishedExecutionInput, StoreError> {
    let upload = self.published_artifact(artifact_id).await?;
    crate::PublishedExecutionInput::from_artifact(&upload)
  }

  /// Resolves only inputs selected by the authoritative consuming Job and permitted by their owner.
  /// Project publications remain Project-scoped; private compiler inputs additionally require
  /// their exact retained Factory operation. Metadata knowledge grants no read authority.
  /// Adapters without this owner projection fail closed.
  async fn published_execution_input_for_job(
    &self,
    _job_id: JobId,
    _artifact_id: ArtifactId,
  ) -> Result<crate::PublishedExecutionInput, StoreError> {
    Err(StoreError::NotFound {
      entity: EntityKind::Artifact,
    })
  }

  /// Lists a bounded set of published logical outputs for one Build.
  async fn list_published_artifacts(
    &self,
    query: ListPublishedArtifacts,
  ) -> Result<Vec<ArtifactUploadRecord>, StoreError>;
}
