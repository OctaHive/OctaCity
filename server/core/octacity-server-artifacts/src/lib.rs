//! Logical artifact lifecycle and policy.
//!
//! This module owns upload, publication, download, fencing, and retention
//! decisions independently of any concrete byte-store implementation.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod log;
mod model;
mod state;

pub use log::{BuildLogStream, LogChunkDigest, LogChunkManifest, LogChunkManifestError, MAX_LOG_CHUNK_BYTES};
pub use model::{
  ArtifactContentDigest, ArtifactIdentity, ArtifactMediaType, ArtifactRecord, ArtifactRecordError,
  ArtifactReportFormat, ArtifactRetentionPolicy, ArtifactType, MAX_ARTIFACT_MEDIA_TYPE_BYTES,
  MAX_ARTIFACT_REPORT_FORMAT_BYTES,
};
pub use state::{ArtifactEvent, ArtifactState};
