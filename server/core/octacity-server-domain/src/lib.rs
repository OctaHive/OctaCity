//! Server-only ubiquitous domain value objects.
//!
//! This crate owns semantic primitives shared by server core modules. It does
//! not own aggregates, transport DTOs, persistence rows, or provider payloads.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod error;
mod execution;
mod id;
mod identity;
mod json;
mod name;
mod source;
mod time;
mod transition;
mod version;

pub use error::{
  DomainError, DomainValueError, EntityKind, IdentifierErrorKind, TextErrorKind, TimestampErrorKind, VersionErrorKind,
};
pub use execution::{ArtifactPolicy, ArtifactPolicyError, RuntimeClass};
pub use id::{
  AgentId, ArtifactId, ArtifactUploadId, AttemptId, BuildConfigurationId, BuildId, CacheSessionId,
  EnrollmentCredentialId, IntegrationId, JobId, LeaseId, LogChunkId, LogIndexingWorkId, PipelineId, PoolId, ProjectId,
  RegistrationCredentialId, RepositoryId, RetentionWorkId, TriggerId, TriggerOccurrenceId,
};
pub use identity::{MAX_PIPELINE_NODE_ID_BYTES, MAX_TRIGGER_IDENTITY_BYTES, PipelineNodeId, TriggerIdentity};
pub use json::{JsonCanonicalizationError, MAX_CANONICAL_JSON_DEPTH, canonicalize_json};
pub use name::{
  AgentName, ArtifactName, BuildConfigurationName, IntegrationName, JobName, MAX_ARTIFACT_NAME_BYTES,
  MAX_RESOURCE_NAME_BYTES, PipelineName, PoolName, ProjectName, RepositoryName,
};
pub use source::{
  ImmutableRevision, ImmutableRevisionError, MAX_IMMUTABLE_REVISION_BYTES, MAX_NETWORK_HOST_BYTES,
  MAX_REPOSITORY_LOCATOR_BYTES, MAX_SOURCE_REFERENCE_BYTES, NetworkHost, NetworkHostError, RepositoryLocator,
  RepositoryLocatorError, SourceReference, SourceReferenceError,
};
pub use time::{MAX_TIMESTAMP_MILLIS, MIN_TIMESTAMP_MILLIS, Timestamp};
pub use transition::TransitionError;
pub use version::{
  AgentVersion, ArtifactVersion, AttemptNumber, AttemptVersion, BuildConfigurationVersion, BuildVersion,
  IntegrationVersion, JobVersion, LeaseVersion, PipelineVersion, PoolVersion, ProjectPolicyVersion, ProjectVersion,
  RepositoryVersion, RetentionHoldVersion, TriggerVersion,
};
