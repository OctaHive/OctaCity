use octacity_server_artifacts::{ArtifactEvent, ArtifactIdentity, ArtifactRecord, ArtifactState, ArtifactType};
use octacity_server_domain::{
  ArtifactId, ArtifactName, ArtifactUploadId, ArtifactVersion, AttemptNumber, BuildId, Timestamp,
};

use crate::{ArtifactContentDigest, ArtifactMediaType, IdempotencyKey, LeaseAccess, MutationDisposition};

/// Maximum published Artifacts returned by one management query.
pub const MAX_ARTIFACT_PAGE_SIZE: u16 = 100;

/// Atomic request to reserve one logical output and its pending byte upload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BeginArtifactUpload {
  /// Fresh logical identity used only when the idempotency key is new.
  pub artifact_id: ArtifactId,
  /// Fresh upload identity used only when the idempotency key is new.
  pub upload_id: ArtifactUploadId,
  /// Agent-owned replay identity scoped to the current Lease.
  pub idempotency_key: IdempotencyKey,
  /// Current fenced Lease authority.
  pub lease: LeaseAccess,
  /// Job identity echoed by the Agent's Lease assignment.
  pub job_id: octacity_server_domain::JobId,
  /// Attempt number echoed by the Agent's Lease assignment.
  pub attempt: AttemptNumber,
  /// User-visible output name.
  pub logical_name: ArtifactName,
  /// Runner execution that declared the output.
  pub producer_run_id: u64,
  /// Runner task that declared the output.
  pub producer_task_id: u64,
  /// Artifact or open-format report semantics.
  pub artifact_type: ArtifactType,
  /// Logical media type preserved from the output declaration.
  pub media_type: ArtifactMediaType,
  /// Transport media type used by the direct byte upload.
  pub transport_media_type: ArtifactMediaType,
  /// Exact expected byte length.
  pub size_bytes: u64,
  /// Exact expected SHA-256.
  pub digest: ArtifactContentDigest,
  /// Server-observed reservation time.
  pub reserved_at: Timestamp,
  /// Expiry of the first short-lived upload capability.
  pub capability_expires_at: Timestamp,
}

/// Durable association between a logical Artifact and one physical upload identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactUploadRecord {
  /// Opaque upload-attempt identity.
  pub upload_id: ArtifactUploadId,
  /// Agent-owned replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Complete logical Artifact lifecycle record.
  pub artifact: ArtifactRecord,
  /// Runner execution that declared the output.
  pub producer_run_id: u64,
  /// Runner task that declared the output.
  pub producer_task_id: u64,
  /// Transport media type used to authorize and verify physical bytes.
  pub transport_media_type: ArtifactMediaType,
  /// Expiry of the capability issued for the original reservation.
  pub capability_expires_at: Timestamp,
}

/// Result of an idempotent upload reservation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BeginArtifactUploadOutcome {
  /// Reserved or replayed upload record.
  pub upload: ArtifactUploadRecord,
  /// Whether this call inserted state or replayed an identical reservation.
  pub disposition: MutationDisposition,
}

/// Fenced request to start or resume independent object verification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifyArtifactUpload {
  /// Pending upload selected by the Agent.
  pub upload_id: ArtifactUploadId,
  /// Current fenced Lease authority.
  pub lease: LeaseAccess,
  /// Job identity echoed by the Agent's Lease assignment.
  pub job_id: octacity_server_domain::JobId,
  /// Attempt number echoed by the Agent's Lease assignment.
  pub attempt: AttemptNumber,
  /// Server-observed transition time.
  pub observed_at: Timestamp,
}

/// Result of independent byte verification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactVerificationResult {
  /// Exact size and SHA-256 matched and immutable bytes were published.
  Verified,
  /// Bytes were absent or failed immutable identity verification.
  Rejected,
}

/// Bounded query for visible outputs of one Build.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListPublishedArtifacts {
  /// Build whose outputs are requested.
  pub build_id: BuildId,
  /// Positive result ceiling.
  pub limit: u16,
}

/// Request to reserve one immutable logical Artifact under a current fenced Lease.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReserveArtifact {
  /// Immutable logical and content identity.
  pub identity: ArtifactIdentity,
  /// Current Lease authority whose identity must match the Artifact.
  pub lease: LeaseAccess,
  /// Server-observed reservation time.
  pub reserved_at: Timestamp,
}

impl ReserveArtifact {
  /// Constructs and validates the initial pending record shape.
  pub fn pending_record(&self) -> Result<ArtifactRecord, octacity_server_artifacts::ArtifactRecordError> {
    ArtifactRecord::pending(self.identity.clone(), self.reserved_at)
  }
}

/// Authority used for one lifecycle transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactTransitionAuthority {
  /// A current Agent Lease performs an upload-related transition.
  Lease(LeaseAccess),
  /// A trusted coordinator retention worker performs a retention transition.
  Retention,
}

/// Request to apply one Rust-owned Artifact lifecycle decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitionArtifact {
  /// Complete immutable identity expected by the caller.
  pub expected_identity: ArtifactIdentity,
  /// Optimistic version expected by the caller.
  pub expected_version: ArtifactVersion,
  /// Lifecycle fact to apply.
  pub event: ArtifactEvent,
  /// Authority appropriate for the current state and event.
  pub authority: ArtifactTransitionAuthority,
  /// Server-observed transition time.
  pub transitioned_at: Timestamp,
}

/// Reports whether an authority can apply an event from a current state.
///
/// Byte-upload transitions are always fenced to the Lease that reserved the
/// record. Retention transitions are server-owned and remain possible after
/// the Job Lease has expired.
#[must_use]
pub fn artifact_authority_is_valid(
  authority: ArtifactTransitionAuthority,
  identity: &ArtifactIdentity,
  state: ArtifactState,
  event: ArtifactEvent,
) -> bool {
  match (authority, state, event) {
    (
      ArtifactTransitionAuthority::Lease(access),
      ArtifactState::Pending,
      ArtifactEvent::BeginVerification | ArtifactEvent::Delete,
    )
    | (
      ArtifactTransitionAuthority::Lease(access),
      ArtifactState::Verifying,
      ArtifactEvent::VerificationFailed | ArtifactEvent::Publish,
    ) => access.lease_id == identity.lease_id,
    (ArtifactTransitionAuthority::Retention, ArtifactState::Published, ArtifactEvent::Expire)
    | (ArtifactTransitionAuthority::Retention, ArtifactState::Expired, ArtifactEvent::Delete) => true,
    _ => false,
  }
}
