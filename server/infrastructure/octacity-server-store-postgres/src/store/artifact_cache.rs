use async_trait::async_trait;
use octacity_observability::Operation;
use octacity_server_store::*;

use super::PostgresStore;

#[async_trait]
impl AuditFactStore for PostgresStore {
  async fn list_audit_facts(&self, query: AuditFactQuery) -> Result<AuditFactPage, StoreError> {
    crate::audit_query::list(&self.pool, query).await
  }
}

#[async_trait]
impl ArtifactRecordStore for PostgresStore {
  async fn reserve_artifact(&self, request: ReserveArtifact) -> Result<ArtifactRecord, StoreError> {
    crate::artifact::reserve(&self.pool, request).await
  }

  async fn artifact(&self, artifact_id: octacity_server_domain::ArtifactId) -> Result<ArtifactRecord, StoreError> {
    crate::artifact::read(&self.pool, artifact_id).await
  }

  async fn transition_artifact(&self, request: TransitionArtifact) -> Result<ArtifactRecord, StoreError> {
    crate::artifact::transition(&self.pool, request).await
  }

  async fn begin_artifact_upload(
    &self,
    request: BeginArtifactUpload,
  ) -> Result<BeginArtifactUploadOutcome, StoreError> {
    crate::telemetry::observe(Operation::Upload, crate::artifact::begin_upload(&self.pool, request)).await
  }

  async fn artifact_upload(
    &self,
    upload_id: octacity_server_domain::ArtifactUploadId,
  ) -> Result<ArtifactUploadRecord, StoreError> {
    crate::artifact::read_upload(&self.pool, upload_id).await
  }

  async fn begin_artifact_verification(
    &self,
    request: VerifyArtifactUpload,
  ) -> Result<ArtifactUploadRecord, StoreError> {
    crate::telemetry::observe(
      Operation::Verify,
      crate::artifact::begin_verification(&self.pool, request),
    )
    .await
  }

  async fn finish_artifact_verification(
    &self,
    request: VerifyArtifactUpload,
    result: ArtifactVerificationResult,
  ) -> Result<ArtifactUploadRecord, StoreError> {
    crate::telemetry::observe(
      Operation::Complete,
      crate::artifact::finish_verification(&self.pool, request, result),
    )
    .await
  }

  async fn published_artifact(
    &self,
    artifact_id: octacity_server_domain::ArtifactId,
  ) -> Result<ArtifactUploadRecord, StoreError> {
    crate::artifact::published(&self.pool, artifact_id).await
  }

  async fn list_published_artifacts(
    &self,
    query: ListPublishedArtifacts,
  ) -> Result<Vec<ArtifactUploadRecord>, StoreError> {
    crate::artifact::list_published(&self.pool, query).await
  }
}

#[async_trait]
impl CacheSessionStore for PostgresStore {
  async fn begin_cache_session(&self, request: BeginCacheSession) -> Result<BeginCacheSessionOutcome, StoreError> {
    crate::telemetry::observe(Operation::Accept, crate::cache::begin(&self.pool, request)).await
  }

  async fn revoke_cache_session(&self, request: RevokeCacheSession) -> Result<MutationDisposition, StoreError> {
    crate::telemetry::observe(Operation::Cancel, crate::cache::revoke(&self.pool, request)).await
  }

  async fn authorize_cache_session(
    &self,
    request: AuthorizeCacheSession,
  ) -> Result<CacheAuthorizationOutcome, StoreError> {
    crate::telemetry::observe(Operation::Authenticate, crate::cache::authorize(&self.pool, request)).await
  }

  async fn cache_session(
    &self,
    session_id: octacity_server_domain::CacheSessionId,
  ) -> Result<CacheSessionRecord, StoreError> {
    crate::cache::read(&self.pool, session_id).await
  }

  async fn list_build_cache_sessions(
    &self,
    request: ListBuildCacheSessions,
  ) -> Result<Vec<CacheSessionRecord>, StoreError> {
    crate::cache::list(&self.pool, request).await
  }
}

#[async_trait]
impl CacheDataStore for PostgresStore {
  async fn resolve_cache_namespace(
    &self,
    credential_digest: octacity_server_cache::CacheCredentialDigest,
    observed_at: octacity_server_domain::Timestamp,
  ) -> Result<octacity_server_cache::CacheNamespace, StoreError> {
    crate::cache::resolve_namespace(&self.pool, credential_digest, observed_at).await
  }

  async fn find_missing_cache_blobs(
    &self,
    access: CacheDataAccess,
    blobs: Vec<octacity_server_cache::BlobDescriptor>,
  ) -> Result<Vec<octacity_server_cache::BlobDescriptor>, StoreError> {
    crate::cache::find_missing_blobs(&self.pool, access, blobs).await
  }

  async fn cache_blob(
    &self,
    access: CacheDataAccess,
    blob: octacity_server_cache::BlobDescriptor,
  ) -> Result<Option<octacity_server_cache::CacheBlobObject>, StoreError> {
    crate::cache::read_blob(&self.pool, access, blob).await
  }

  async fn publish_cache_blob(&self, request: PublishCacheBlob) -> Result<CachePublicationOutcome, StoreError> {
    crate::telemetry::observe(Operation::Upload, crate::cache::publish_blob(&self.pool, request)).await
  }

  async fn prepare_cache_blob(
    &self,
    access: CacheDataAccess,
    blob: octacity_server_cache::BlobDescriptor,
  ) -> Result<CacheBlobPreparationOutcome, StoreError> {
    crate::cache::prepare_blob(&self.pool, access, blob).await
  }

  async fn cache_action(
    &self,
    access: CacheDataAccess,
    action: octacity_server_cache::Digest,
  ) -> Result<Option<octacity_server_cache::ActionResultV1>, StoreError> {
    crate::cache::read_action(&self.pool, access, action).await
  }

  async fn publish_cache_action(&self, request: PublishCacheAction) -> Result<CachePublicationOutcome, StoreError> {
    crate::telemetry::observe(Operation::Upload, crate::cache::publish_action(&self.pool, request)).await
  }

  async fn prune_cache(&self, access: CacheDataAccess) -> Result<CacheRetentionOutcome, StoreError> {
    crate::cache::prune(&self.pool, access).await
  }
}

#[async_trait]
impl BuildRetentionStore for PostgresStore {
  async fn claim_retention_work(&self, request: ClaimRetentionWork) -> Result<Vec<RetentionWorkClaim>, StoreError> {
    crate::retention::claim(&self.pool, request).await
  }

  async fn prepare_retention_work(&self, request: PrepareRetentionWork) -> Result<RetentionPreparation, StoreError> {
    crate::retention::prepare(&self.pool, request).await
  }

  async fn complete_retention_search(&self, request: CompleteRetentionSearch) -> Result<(), StoreError> {
    crate::retention::complete_search(&self.pool, request).await
  }

  async fn complete_retention_object(&self, request: CompleteRetentionObject) -> Result<(), StoreError> {
    crate::retention::complete_object(&self.pool, request).await
  }

  async fn finish_retention_pass(&self, request: FinishRetentionPass) -> Result<RetentionPassOutcome, StoreError> {
    crate::retention::finish_pass(&self.pool, request).await
  }

  async fn fail_retention_work(&self, request: FailRetentionWork) -> Result<(), StoreError> {
    crate::retention::fail(&self.pool, request).await
  }
}

#[async_trait]
impl BuildResultRetentionHoldStore for PostgresStore {
  async fn build_result_retention(
    &self,
    query: GetBuildResultRetention,
  ) -> Result<BuildResultRetentionState, StoreError> {
    crate::retention_hold::read(&self.pool, query).await
  }

  async fn place_build_result_hold(
    &self,
    request: PlaceBuildResultHold,
  ) -> Result<RetentionHoldMutationOutcome, StoreError> {
    crate::retention_hold::place(&self.pool, request).await
  }

  async fn release_build_result_hold(
    &self,
    request: ReleaseBuildResultHold,
  ) -> Result<RetentionHoldMutationOutcome, ReleaseBuildResultHoldError> {
    crate::retention_hold::release(&self.pool, request).await
  }
}

#[async_trait]
impl OrphanLogChunkStore for PostgresStore {
  async fn stage_orphan_log_chunk(&self, request: StageOrphanLogChunk) -> Result<(), StoreError> {
    crate::retention::stage_orphan(&self.pool, request).await
  }

  async fn claim_orphan_log_chunks(
    &self,
    request: ClaimOrphanLogChunks,
  ) -> Result<Vec<OrphanLogChunkClaim>, StoreError> {
    crate::retention::claim_orphans(&self.pool, request).await
  }

  async fn complete_orphan_log_chunk(&self, request: CompleteOrphanLogChunk) -> Result<(), StoreError> {
    crate::retention::complete_orphan(&self.pool, request).await
  }

  async fn fail_orphan_log_chunk(&self, request: FailOrphanLogChunk) -> Result<(), StoreError> {
    crate::retention::fail_orphan(&self.pool, request).await
  }
}
