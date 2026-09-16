//! Server-only ubiquitous domain value objects.
//!
//! This crate owns semantic primitives shared by server core modules. It does
//! not own aggregates, transport DTOs, persistence rows, or provider payloads.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod error;
mod id;
mod identity;
mod name;
mod time;
mod transition;
mod version;

pub use error::{
  DomainError, DomainValueError, EntityKind, IdentifierErrorKind, TextErrorKind, TimestampErrorKind, VersionErrorKind,
};
pub use id::{
  AgentId, ArtifactId, ArtifactUploadId, AttemptId, BuildConfigurationId, BuildId, IntegrationId, JobId, LeaseId,
  PipelineId, PoolId, ProjectId, TriggerId, TriggerOccurrenceId,
};
pub use identity::{MAX_PIPELINE_NODE_ID_BYTES, MAX_TRIGGER_IDENTITY_BYTES, PipelineNodeId, TriggerIdentity};
pub use name::{
  AgentName, ArtifactName, BuildConfigurationName, IntegrationName, JobName, MAX_ARTIFACT_NAME_BYTES,
  MAX_RESOURCE_NAME_BYTES, PipelineName, PoolName, ProjectName,
};
pub use time::{MAX_TIMESTAMP_MILLIS, MIN_TIMESTAMP_MILLIS, Timestamp};
pub use transition::TransitionError;
pub use version::{
  AgentVersion, ArtifactVersion, AttemptNumber, AttemptVersion, BuildConfigurationVersion, BuildVersion,
  IntegrationVersion, JobVersion, LeaseVersion, PipelineVersion, PoolVersion, ProjectVersion,
};
