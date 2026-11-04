use std::num::NonZeroU16;

use octacity_server_domain::{ArtifactId, BuildId, JobId, LogChunkId, ProjectId, RetentionWorkId, Timestamp};

use crate::{
  ArtifactUploadRecord, DeleteLogSearchDocuments, LogChunkManifest, StoreError, StoreInputError, StoreOperation,
  WorkerOwner,
};

/// Maximum durable Build Result retention claims acquired by one transaction.
pub const MAX_RETENTION_WORK_BATCH_SIZE: u16 = 64;
/// Maximum physical objects returned by one retention claim pass.
pub const MAX_RETENTION_OBJECT_BATCH_SIZE: u16 = 64;
/// Maximum UTF-8 bytes retained for one safe retention failure code.
pub const MAX_RETENTION_FAILURE_CODE_BYTES: usize = 128;

/// Independently expiring component of one logical Build Result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildResultComponent {
  /// Build metadata and immutable execution snapshots.
  Metadata,
  /// Archived stdout and stderr chunks plus their derived search documents.
  Logs,
  /// Produced file outputs.
  Artifacts,
  /// Produced plugin-defined reports.
  Reports,
}

impl BuildResultComponent {
  /// Returns the stable persistence representation.
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Metadata => "metadata",
      Self::Logs => "logs",
      Self::Artifacts => "artifacts",
      Self::Reports => "reports",
    }
  }
}

/// Durable progress of one component through idempotent retention.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetentionPhase {
  /// The component remains logically visible until its deadline is claimed.
  Pending,
  /// Logical visibility was removed durably.
  Hidden,
  /// Derived Build-log search documents were tombstoned.
  SearchDeleted,
  /// Every unreferenced physical object was removed.
  BytesDeleted,
  /// Cleanup exhausted its bounded attempts and requires operator intervention.
  DeadLetter,
}

/// Bounded request to claim due Build Result retention work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimRetentionWork {
  /// Process instance acquiring work.
  pub owner: WorkerOwner,
  /// Authoritative time used for deadline and stale-claim decisions.
  pub observed_at: Timestamp,
  /// Exclusive deadline after which another replica may reclaim work.
  pub claim_expires_at: Timestamp,
  /// Positive bounded number of work items.
  pub limit: NonZeroU16,
}

impl ClaimRetentionWork {
  /// Creates a validated bounded claim request.
  pub fn new(
    owner: WorkerOwner,
    observed_at: Timestamp,
    claim_expires_at: Timestamp,
    limit: u16,
  ) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit)
      .filter(|limit| limit.get() <= MAX_RETENTION_WORK_BATCH_SIZE)
      .ok_or_else(|| invalid(StoreOperation::ClaimRetentionWork))?;
    if claim_expires_at <= observed_at {
      return Err(invalid(StoreOperation::ClaimRetentionWork));
    }
    Ok(Self {
      owner,
      observed_at,
      claim_expires_at,
      limit,
    })
  }

  /// Revalidates public fields at the persistence seam.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.limit.get() > MAX_RETENTION_WORK_BATCH_SIZE || self.claim_expires_at <= self.observed_at {
      return Err(invalid(StoreOperation::ClaimRetentionWork));
    }
    Ok(())
  }
}

/// Exclusively owned durable retention operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetentionWorkClaim {
  /// Stable work identity.
  pub work_id: RetentionWorkId,
  /// Project boundary owning the Build Result.
  pub project_id: ProjectId,
  /// Build Result being retained.
  pub build_id: BuildId,
  /// Independently expiring aggregate component.
  pub component: BuildResultComponent,
  /// Original immutable automatic-retention deadline.
  pub deadline: Timestamp,
  /// Durable phase observed when this claim was acquired.
  pub phase: RetentionPhase,
  /// One-based attempt count.
  pub attempt: u16,
  /// Process instance owning the claim.
  pub owner: WorkerOwner,
  /// Exclusive claim deadline.
  pub claim_expires_at: Timestamp,
}

/// Owned request to remove logical visibility and list the next object page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrepareRetentionWork {
  /// Claimed work identity.
  pub work_id: RetentionWorkId,
  /// Current claim owner.
  pub owner: WorkerOwner,
  /// Authoritative operation time before claim expiry.
  pub observed_at: Timestamp,
  /// Positive bounded physical-object page size.
  pub object_limit: NonZeroU16,
}

impl PrepareRetentionWork {
  /// Creates a validated preparation request.
  pub fn new(
    work_id: RetentionWorkId,
    owner: WorkerOwner,
    observed_at: Timestamp,
    object_limit: u16,
  ) -> Result<Self, StoreError> {
    let object_limit = NonZeroU16::new(object_limit)
      .filter(|limit| limit.get() <= MAX_RETENTION_OBJECT_BATCH_SIZE)
      .ok_or_else(|| invalid(StoreOperation::PrepareRetentionWork))?;
    Ok(Self {
      work_id,
      owner,
      observed_at,
      object_limit,
    })
  }

  /// Revalidates public fields at the persistence seam.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.object_limit.get() > MAX_RETENTION_OBJECT_BATCH_SIZE {
      return Err(invalid(StoreOperation::PrepareRetentionWork));
    }
    Ok(())
  }
}

/// Backend-neutral physical object selected after logical visibility is gone.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RetentionObject {
  /// One archived log chunk.
  Log(LogChunkManifest),
  /// One artifact or report upload generation.
  Output(Box<ArtifactUploadRecord>),
}

impl RetentionObject {
  /// Returns the stable logical object identity used to complete deletion.
  #[must_use]
  pub const fn identity(&self) -> RetentionObjectIdentity {
    match self {
      Self::Log(manifest) => RetentionObjectIdentity::Log(manifest.chunk_id()),
      Self::Output(upload) => RetentionObjectIdentity::Output(upload.artifact.identity().artifact_id),
    }
  }
}

/// Stable identity of an object completed by a retention worker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetentionObjectIdentity {
  /// Archived log chunk.
  Log(LogChunkId),
  /// Artifact or report.
  Output(ArtifactId),
}

/// Durable preparation returned only after visibility changes commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetentionPreparation {
  /// Search tombstone required before archived log bytes can be removed.
  pub search_deletion: Option<DeleteLogSearchDocuments>,
  /// Bounded next page of logically hidden physical objects.
  pub objects: Vec<RetentionObject>,
}

/// Owned acknowledgement that the search tombstone was applied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteRetentionSearch {
  /// Claimed work identity.
  pub work_id: RetentionWorkId,
  /// Current claim owner.
  pub owner: WorkerOwner,
  /// Authoritative completion time.
  pub completed_at: Timestamp,
}

/// Owned acknowledgement that one logical object was safely released.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteRetentionObject {
  /// Claimed work identity.
  pub work_id: RetentionWorkId,
  /// Current claim owner.
  pub owner: WorkerOwner,
  /// Logical object that was deleted or retained through another reference.
  pub object: RetentionObjectIdentity,
  /// Authoritative completion time.
  pub completed_at: Timestamp,
}

/// Outcome of settling one bounded retention pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetentionPassOutcome {
  /// All required phases completed durably.
  Completed,
  /// More bounded object work remains for a later claim.
  Pending,
}

/// Owned request to settle the current bounded pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinishRetentionPass {
  /// Claimed work identity.
  pub work_id: RetentionWorkId,
  /// Current claim owner.
  pub owner: WorkerOwner,
  /// Authoritative settlement time.
  pub finished_at: Timestamp,
}

/// Owned request to durably retry a failed retention pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FailRetentionWork {
  /// Claimed work identity.
  pub work_id: RetentionWorkId,
  /// Current claim owner.
  pub owner: WorkerOwner,
  /// Authoritative failure time.
  pub failed_at: Timestamp,
  /// Stable bounded non-sensitive failure classification.
  pub error_code: String,
  /// Strictly later retry time, or `None` when attempts are exhausted.
  pub retry_at: Option<Timestamp>,
}

/// Durable candidate recorded before one archived log object is written.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StageOrphanLogChunk {
  /// Job whose output produced the chunk.
  pub job_id: JobId,
  /// Immutable object identity and integrity metadata.
  pub manifest: LogChunkManifest,
  /// Earliest instant at which an uncommitted object may be deleted.
  pub cleanup_after: Timestamp,
}

/// Bounded request to claim due orphan-log cleanup candidates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimOrphanLogChunks {
  /// Process instance acquiring work.
  pub owner: WorkerOwner,
  /// Authoritative claim time.
  pub observed_at: Timestamp,
  /// Exclusive claim deadline.
  pub claim_expires_at: Timestamp,
  /// Positive bounded claim size.
  pub limit: NonZeroU16,
}

impl ClaimOrphanLogChunks {
  /// Creates a validated orphan-cleanup claim.
  pub fn new(
    owner: WorkerOwner,
    observed_at: Timestamp,
    claim_expires_at: Timestamp,
    limit: u16,
  ) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit)
      .filter(|value| value.get() <= MAX_RETENTION_OBJECT_BATCH_SIZE)
      .ok_or_else(|| invalid(StoreOperation::ClaimOrphanLogChunks))?;
    if claim_expires_at <= observed_at {
      return Err(invalid(StoreOperation::ClaimOrphanLogChunks));
    }
    Ok(Self {
      owner,
      observed_at,
      claim_expires_at,
      limit,
    })
  }

  /// Revalidates public fields at the persistence seam.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.limit.get() > MAX_RETENTION_OBJECT_BATCH_SIZE || self.claim_expires_at <= self.observed_at {
      return Err(invalid(StoreOperation::ClaimOrphanLogChunks));
    }
    Ok(())
  }
}

/// One exclusively owned orphan-log cleanup candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrphanLogChunkClaim {
  /// Immutable object identity and integrity metadata.
  pub manifest: LogChunkManifest,
  /// One-based cleanup attempt.
  pub attempt: u16,
  /// Process instance owning the claim.
  pub owner: WorkerOwner,
}

/// Owned completion of one orphan-log cleanup candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteOrphanLogChunk {
  /// Immutable chunk identity.
  pub chunk_id: LogChunkId,
  /// Current claim owner.
  pub owner: WorkerOwner,
  /// Authoritative completion time.
  pub completed_at: Timestamp,
}

/// Owned retry or terminal failure of orphan-log cleanup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FailOrphanLogChunk {
  /// Immutable chunk identity.
  pub chunk_id: LogChunkId,
  /// Current claim owner.
  pub owner: WorkerOwner,
  /// Authoritative failure time.
  pub failed_at: Timestamp,
  /// Stable bounded non-sensitive failure classification.
  pub error_code: String,
  /// Strictly later retry time, or `None` when attempts are exhausted.
  pub retry_at: Option<Timestamp>,
}

const fn invalid(operation: StoreOperation) -> StoreError {
  StoreError::invalid(operation, StoreInputError::InvalidWorkerClaim)
}
