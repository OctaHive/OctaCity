use octacity_server_domain::{PipelineId, PipelineName, PipelineVersion, ProjectId, Timestamp};
use octacity_server_pipeline::{PipelineDag, PublishablePipelineDag};
use serde::{Deserialize, Serialize};

use crate::{IdempotencyKey, MutationDisposition};

/// One immutable published Pipeline version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublishedPipeline {
  /// Stable Pipeline identity shared by all versions.
  pub id: PipelineId,
  /// Project that owns the Pipeline.
  pub project_id: ProjectId,
  /// Stable operator-facing Pipeline name.
  pub name: PipelineName,
  /// Positive immutable version number.
  pub version: PipelineVersion,
  /// Canonical validated DAG snapshot.
  pub dag: PipelineDag,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

/// Complete atomic request to create a Pipeline and its initial version.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CreatePipeline {
  /// Stable identity selected once by the application.
  pub id: PipelineId,
  /// Existing owning Project.
  pub project_id: ProjectId,
  /// Name unique among Pipelines in the Project.
  pub name: PipelineName,
  /// Validated immutable initial DAG.
  pub dag: PublishablePipelineDag,
  /// Stable replay identity for this command.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

/// Complete atomic request to append the next Pipeline version.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PublishPipelineVersion {
  /// Existing Pipeline whose next version is published.
  pub id: PipelineId,
  /// Version that must still be the latest published version.
  pub expected_current_version: PipelineVersion,
  /// Validated immutable DAG for the next version.
  pub dag: PublishablePipelineDag,
  /// Stable replay identity for this command.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

/// Result of creating or appending one immutable Pipeline version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PipelineMutationOutcome {
  /// Whether the command was newly applied or exactly replayed.
  pub disposition: MutationDisposition,
  /// Immutable Pipeline version committed by the original command.
  pub pipeline: PublishedPipeline,
}
