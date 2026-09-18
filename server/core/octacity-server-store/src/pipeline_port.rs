use async_trait::async_trait;
use octacity_server_domain::{PipelineId, PipelineVersion};

use crate::{CreatePipeline, PipelineMutationOutcome, PublishPipelineVersion, PublishedPipeline, StoreError};

/// Backend-neutral append-only operations for immutable Pipeline versions.
///
/// Mutation methods are transaction boundaries: the version, replay outcome,
/// audit fact, and outbox record commit together. The interface intentionally
/// exposes no update or delete operation for a published version.
#[async_trait]
pub trait PipelineStore: Send + Sync {
  /// Creates a Pipeline identity together with immutable version one.
  async fn create_pipeline(&self, request: CreatePipeline) -> Result<PipelineMutationOutcome, StoreError>;

  /// Appends exactly the next version when the optimistic precondition is current.
  async fn publish_pipeline_version(
    &self,
    request: PublishPipelineVersion,
  ) -> Result<PipelineMutationOutcome, StoreError>;

  /// Reads one exact immutable Pipeline version.
  async fn pipeline_version(
    &self,
    pipeline_id: PipelineId,
    version: PipelineVersion,
  ) -> Result<PublishedPipeline, StoreError>;
}
