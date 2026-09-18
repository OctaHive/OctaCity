mod build;
mod configuration;
mod dag;
mod job;
mod pipeline;
mod project;
mod trigger;

use thiserror::Error;

pub use build::{AttemptProjection, BuildProjection, ParameterValueProjection};
pub use configuration::{BuildConfigurationProjection, ConfigurationCacheProjection, ConfigurationRuntimeProjection};
pub use dag::{DagCausalityProjection, DagEdgeProjection, DagNodeProjection};
pub use job::{
  JobAssignmentProjection, JobFailureClassification, JobOutputKind, JobOutputReference, JobPlacementProjection,
  JobProjection, JobProjectionFacts, JobQueueProjection, JobTerminalOutcomeProjection, Sha256DigestProjection,
};
pub use pipeline::{PipelineEdgeProjection, PipelineNodeProjection, PipelineProjection};
pub use project::{ProjectProjection, ProjectSummaryProjection};
pub use trigger::{TriggerCauseProjection, TriggerHistoryProjection};

/// A persisted authoritative value cannot be represented by a safe application projection.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ProjectionError {
  /// A Pipeline template contains fields outside the repository-controlled execution contract.
  #[error("pipeline template is not safe to project")]
  InvalidPipelineTemplate,
  /// A configuration contains an invalid logical cache or workload-identity reference.
  #[error("build configuration contains an invalid logical reference")]
  InvalidConfigurationReference,
  /// A Build Configuration snapshot violates its authoritative core invariants.
  #[error("build configuration snapshot is invalid")]
  InvalidConfigurationSnapshot,
  /// A Build snapshot does not match the strict server-owned snapshot schema.
  #[error("build snapshot is invalid")]
  InvalidBuildSnapshot,
  /// Job lifecycle fields disagree with the projected durable state.
  #[error("job lifecycle projection is inconsistent")]
  InvalidJobLifecycle,
  /// A logical output reference contains a malformed SHA-256 identity.
  #[error("output content digest is invalid")]
  InvalidContentDigest,
  /// Attempt Jobs do not form one self-contained acyclic causality graph.
  #[error("job projections do not form a valid DAG")]
  InvalidDagCausality,
}
