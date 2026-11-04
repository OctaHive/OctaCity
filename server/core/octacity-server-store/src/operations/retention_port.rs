use crate::{
  ClaimOrphanLogChunks, ClaimRetentionWork, CompleteOrphanLogChunk, CompleteRetentionObject, CompleteRetentionSearch,
  FailOrphanLogChunk, FailRetentionWork, FinishRetentionPass, OrphanLogChunkClaim, PrepareRetentionWork,
  RetentionPassOutcome, RetentionPreparation, RetentionWorkClaim, StageOrphanLogChunk, StoreError,
};
use async_trait::async_trait;

/// Authoritative persistence for bounded, resumable Build Result retention.
#[async_trait]
pub trait BuildRetentionStore: Send + Sync {
  /// Claims a bounded set of due or abandoned component operations.
  async fn claim_retention_work(&self, request: ClaimRetentionWork) -> Result<Vec<RetentionWorkClaim>, StoreError>;

  /// Removes logical visibility and returns the next bounded physical cleanup page.
  async fn prepare_retention_work(&self, request: PrepareRetentionWork) -> Result<RetentionPreparation, StoreError>;

  /// Records that the derived log-search tombstone was applied.
  async fn complete_retention_search(&self, request: CompleteRetentionSearch) -> Result<(), StoreError>;

  /// Records one idempotently deleted or reference-retained physical object.
  async fn complete_retention_object(&self, request: CompleteRetentionObject) -> Result<(), StoreError>;

  /// Completes the work or releases it when another bounded page remains.
  async fn finish_retention_pass(&self, request: FinishRetentionPass) -> Result<RetentionPassOutcome, StoreError>;

  /// Releases failed work for a durable later retry.
  async fn fail_retention_work(&self, request: FailRetentionWork) -> Result<(), StoreError>;
}

/// Durable staging and bounded cleanup of log objects not yet referenced by a committed manifest.
#[async_trait]
pub trait OrphanLogChunkStore: Send + Sync {
  /// Records cleanup intent before the corresponding object-store write begins.
  async fn stage_orphan_log_chunk(&self, request: StageOrphanLogChunk) -> Result<(), StoreError>;

  /// Claims a bounded due batch, including candidates abandoned by a crashed owner.
  async fn claim_orphan_log_chunks(
    &self,
    request: ClaimOrphanLogChunks,
  ) -> Result<Vec<OrphanLogChunkClaim>, StoreError>;

  /// Completes one candidate after bytes were deleted or a committed manifest retained them.
  async fn complete_orphan_log_chunk(&self, request: CompleteOrphanLogChunk) -> Result<(), StoreError>;

  /// Releases one candidate for retry or moves it to a terminal dead letter.
  async fn fail_orphan_log_chunk(&self, request: FailOrphanLogChunk) -> Result<(), StoreError>;
}
