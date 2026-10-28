//! Native record verification over existing immutable Build Artifacts.
//!
//! Captured events are audit evidence, including after failure; this boundary
//! never creates a model result, accepts an Issue effect or interprets a provider's
//! search response. Callers obtain the expected invocation from the frozen Job
//! and read these exact Artifact bytes through `FactoryArtifactReader` first.

use octacity_server_artifacts::{ArtifactIdentity, ArtifactType};
use octacity_server_domain::{AttemptId, BuildId, JobId};

use super::{
  FactoryCodexOutputDocument, FactoryCodexProvenanceContract, FactoryNativeCaptureStatus,
  evidence::{document_map, output_map, valid_provenance_identity, valid_provenance_window, valid_trace},
  records::CodexProvenanceRecord,
};
use crate::{
  ApplicationError,
  factory_codex_contract::{CODEX_RUN_PROVENANCE, CODEX_RUN_TRACE, JSON_MEDIA_TYPE, TRACE_MEDIA_TYPE},
};

/// Caller-owned expectations from the exact original ordinary execution.
pub struct FactoryCodexNativeCaptureContract<'a> {
  /// Build selected by the persisted invocation.
  pub build_id: BuildId,
  /// Exact observed Attempt, not the Build's latest Attempt.
  pub attempt_id: AttemptId,
  /// Original frozen Job that produced these records.
  pub job_id: JobId,
  /// Immutable executable/plugin/model and protected input identity.
  pub provenance: FactoryCodexProvenanceContract<'a>,
  /// Output ceiling established by the original Job, checked before parsing.
  pub maximum_bytes: u64,
  /// Original Job creation through completion, capped by its signed deadline.
  pub execution_window: std::ops::RangeInclusive<octacity_server_domain::Timestamp>,
}

/// Verified references to native observations; no semantic result or remote coverage is implied.
pub struct FactoryCodexNativeCapture {
  trace: ArtifactIdentity,
  provenance: ArtifactIdentity,
  status: FactoryNativeCaptureStatus,
  record_count: u64,
}

impl FactoryCodexNativeCapture {
  /// Exact ordinary Artifact, including an empty trace when no valid event arrived.
  #[must_use]
  pub const fn trace(&self) -> &ArtifactIdentity {
    &self.trace
  }
  /// Exact context and capture-provenance Artifact for the same original Job.
  #[must_use]
  pub const fn provenance(&self) -> &ArtifactIdentity {
    &self.provenance
  }
  /// Captured stream lifecycle, independently of any claimed search result.
  #[must_use]
  pub const fn status(&self) -> FactoryNativeCaptureStatus {
    self.status
  }
  /// Actual retained native events. Zero in a failed stream is not successful empty search.
  #[must_use]
  pub const fn record_count(&self) -> u64 {
    self.record_count
  }
}

/// Verifies bounded native evidence without projecting it into authoritative Flow data.
/// Legacy/unsupported capture returns an explicit capability blocker. MCP connection,
/// resource and tool selection still come from the original Role execution package;
/// model text and arbitrary event arguments cannot establish those selections.
pub fn verify_codex_native_capture(
  expected: FactoryCodexNativeCaptureContract<'_>,
  documents: &[FactoryCodexOutputDocument],
) -> Result<FactoryCodexNativeCapture, ApplicationError> {
  let invalid = ApplicationError::invalid;
  let total = documents
    .iter()
    .try_fold(0_u64, |total, document| total.checked_add(document.identity.size_bytes))
    .ok_or_else(invalid)?;
  if total > expected.maximum_bytes
    || documents.iter().any(|document| {
      document.identity.build_id != expected.build_id
        || document.identity.attempt_id != expected.attempt_id
        || document.identity.job_id != expected.job_id
    })
  {
    return Err(invalid());
  }
  let identities = documents
    .iter()
    .map(|document| document.identity.clone())
    .collect::<Vec<_>>();
  let outputs = output_map(&identities).ok_or_else(invalid)?;
  let documents = document_map(documents, &outputs).ok_or_else(invalid)?;
  let trace = documents
    .get(CODEX_RUN_TRACE)
    .ok_or_else(ApplicationError::capability_unavailable)?;
  let provenance = documents
    .get(CODEX_RUN_PROVENANCE)
    .ok_or_else(ApplicationError::capability_unavailable)?;
  if trace.identity.artifact_type != ArtifactType::Artifact
    || trace.identity.media_type.as_str() != TRACE_MEDIA_TYPE
    || provenance.identity.artifact_type != ArtifactType::Artifact
    || provenance.identity.media_type.as_str() != JSON_MEDIA_TYPE
  {
    return Err(invalid());
  }
  let record: CodexProvenanceRecord = serde_json::from_slice(&provenance.bytes).map_err(|_| invalid())?;
  let capture = record
    .capture
    .as_ref()
    .filter(|_| record.format_version == 2 && record.trace_format_version == 2)
    .ok_or_else(ApplicationError::capability_unavailable)?;
  if !valid_provenance_identity(expected.provenance, &record)
    || !valid_provenance_window(&record, &expected.execution_window)
    || !valid_trace(&trace.bytes, &record)
  {
    return Err(invalid());
  }
  Ok(FactoryCodexNativeCapture {
    trace: trace.identity.clone(),
    provenance: provenance.identity.clone(),
    status: capture.status,
    record_count: capture.record_count,
  })
}
