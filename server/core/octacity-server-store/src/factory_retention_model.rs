use std::num::NonZeroU16;

use octacity_server_domain::Timestamp;
use octacity_server_factory::FactoryRunId;

use crate::{StoreError, StoreInputError, StoreOperation, WorkerOwner};

/// Maximum Factory Runs claimed by one retention transaction.
pub const MAX_FACTORY_RETENTION_WORK_BATCH_SIZE: u16 = 64;
/// Maximum references or metadata rows cleaned by one retention transaction.
pub const MAX_FACTORY_RETENTION_CLEANUP_BATCH_SIZE: u16 = 64;

/// Closed semantic role of one Artifact retained by a Factory Run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FactoryArtifactRole {
  /// Admitted task body.
  Task,
  /// Admitted acceptance criteria.
  Acceptance,
  /// Admitted specification input.
  Specification,
  /// Published output of a linked ordinary Build.
  BuildOutput,
  /// Exact ChangeSet Git bundle.
  ChangeSetBundle,
  /// Canonical ChangeSet path and mode manifest.
  ChangeSetManifest,
  /// Exact deterministic evidence item.
  Evidence,
  /// Exact Artifact referenced by an immutable Stage Handoff.
  StageHandoff,
  /// Exact Artifact selected into a frozen Context Manifest.
  CallContext,
  /// Exact typed result, bounded summary, trace, or provenance of a completed macro call.
  CallOutput,
}

impl FactoryArtifactRole {
  /// Returns the stable persistence representation.
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Task => "task",
      Self::Acceptance => "acceptance",
      Self::Specification => "specification",
      Self::BuildOutput => "build_output",
      Self::ChangeSetBundle => "changeset_bundle",
      Self::ChangeSetManifest => "changeset_manifest",
      Self::Evidence => "evidence",
      Self::StageHandoff => "stage_handoff",
      Self::CallContext => "call_context",
      Self::CallOutput => "call_output",
    }
  }
}

/// Durable phase of Factory metadata retention.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FactoryRetentionPhase {
  /// The Run and its references remain visible.
  Pending,
  /// Run visibility is removed and references are being released incrementally.
  Hidden,
  /// Released references are gone and Factory projection/history metadata is being removed incrementally.
  Metadata,
  /// Every retained reference and eligible projection/history row was cleaned.
  Completed,
  /// Bounded retries were exhausted and operator attention is required.
  DeadLetter,
}

/// Bounded request to claim due Factory retention work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimFactoryRetentionWork {
  /// Process instance acquiring work.
  pub owner: WorkerOwner,
  /// Authoritative time for due-work, hold, and abandoned-claim checks.
  pub observed_at: Timestamp,
  /// Exclusive ownership deadline.
  pub claim_expires_at: Timestamp,
  /// Positive bounded claim count.
  pub limit: NonZeroU16,
}

impl ClaimFactoryRetentionWork {
  /// Constructs a validated claim request.
  pub fn new(
    owner: WorkerOwner,
    observed_at: Timestamp,
    claim_expires_at: Timestamp,
    limit: u16,
  ) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit)
      .filter(|value| value.get() <= MAX_FACTORY_RETENTION_WORK_BATCH_SIZE)
      .ok_or_else(invalid_claim)?;
    if claim_expires_at <= observed_at {
      return Err(invalid_claim());
    }
    Ok(Self {
      owner,
      observed_at,
      claim_expires_at,
      limit,
    })
  }

  /// Revalidates caller-mutable fields at the persistence seam.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.limit.get() > MAX_FACTORY_RETENTION_WORK_BATCH_SIZE || self.claim_expires_at <= self.observed_at {
      return Err(invalid_claim());
    }
    Ok(())
  }
}

/// Exclusively owned Factory retention operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryRetentionWorkClaim {
  /// Factory Run whose visible history owns the retained references.
  pub run_id: FactoryRunId,
  /// Durable phase observed at claim time.
  pub phase: FactoryRetentionPhase,
  /// One-based attempt number.
  pub attempt: u16,
  /// Process instance owning the claim.
  pub owner: WorkerOwner,
  /// Exclusive ownership deadline.
  pub claim_expires_at: Timestamp,
}

/// Owned bounded request to hide a Run and advance its incremental cleanup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdvanceFactoryRetentionWork {
  /// Claimed Factory Run.
  pub run_id: FactoryRunId,
  /// Current claim owner.
  pub owner: WorkerOwner,
  /// Authoritative operation time before claim expiry.
  pub observed_at: Timestamp,
  /// Positive bounded cleanup page size.
  pub page_limit: NonZeroU16,
}

impl AdvanceFactoryRetentionWork {
  /// Constructs one bounded cleanup pass.
  pub fn new(
    run_id: FactoryRunId,
    owner: WorkerOwner,
    observed_at: Timestamp,
    page_limit: u16,
  ) -> Result<Self, StoreError> {
    let page_limit = NonZeroU16::new(page_limit)
      .filter(|value| value.get() <= MAX_FACTORY_RETENTION_CLEANUP_BATCH_SIZE)
      .ok_or_else(invalid_advance)?;
    Ok(Self {
      run_id,
      owner,
      observed_at,
      page_limit,
    })
  }

  /// Revalidates caller-mutable fields at the persistence seam.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.page_limit.get() > MAX_FACTORY_RETENTION_CLEANUP_BATCH_SIZE {
      return Err(invalid_advance());
    }
    Ok(())
  }
}

/// Result of one atomic bounded Factory cleanup pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FactoryRetentionPassOutcome {
  /// Number of logical Artifact references released in this pass.
  pub released_references: u16,
  /// Number of Factory projection/history rows deleted in this pass.
  pub deleted_records: u16,
  /// Whether all references and eligible Factory metadata were cleaned durably.
  pub completed: bool,
}

/// Owned request to retry or dead-letter Factory retention.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FailFactoryRetentionWork {
  /// Claimed Factory Run.
  pub run_id: FactoryRunId,
  /// Current claim owner.
  pub owner: WorkerOwner,
  /// Authoritative failure time.
  pub failed_at: Timestamp,
  /// Stable bounded non-sensitive classification.
  pub error_code: String,
  /// Strictly later retry time, or `None` after retry exhaustion.
  pub retry_at: Option<Timestamp>,
}

fn invalid_claim() -> StoreError {
  StoreError::InvalidInput {
    operation: StoreOperation::ClaimFactoryRetentionWork,
    source: StoreInputError::InvalidWorkerClaim,
  }
}

fn invalid_advance() -> StoreError {
  StoreError::InvalidInput {
    operation: StoreOperation::AdvanceFactoryRetentionWork,
    source: StoreInputError::InvalidWorkerClaim,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn time(value: i64) -> Timestamp {
    Timestamp::from_unix_millis(value).unwrap()
  }

  #[test]
  fn factory_retention_claim_and_cleanup_pages_are_bounded() {
    let owner = WorkerOwner::new("factory-retention").unwrap();
    assert!(ClaimFactoryRetentionWork::new(owner.clone(), time(1), time(2), 1).is_ok());
    assert!(ClaimFactoryRetentionWork::new(owner.clone(), time(1), time(1), 1).is_err());
    assert!(ClaimFactoryRetentionWork::new(owner.clone(), time(1), time(2), 0).is_err());
    assert!(
      ClaimFactoryRetentionWork::new(
        owner.clone(),
        time(1),
        time(2),
        MAX_FACTORY_RETENTION_WORK_BATCH_SIZE + 1,
      )
      .is_err()
    );
    assert!(
      AdvanceFactoryRetentionWork::new(
        FactoryRunId::generate(),
        owner,
        time(1),
        MAX_FACTORY_RETENTION_CLEANUP_BATCH_SIZE + 1,
      )
      .is_err()
    );
  }
}
