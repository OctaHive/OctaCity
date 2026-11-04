use std::num::NonZeroU16;

use octacity_server_domain::{JobId, LeaseId, Timestamp};
use serde::{Deserialize, Serialize};

use crate::{MutationDisposition, StoreError, StoreInputError, StoreOperation};

/// Maximum number of expired Leases claimed by one worker transaction.
pub const MAX_LEASE_EXPIRY_BATCH_SIZE: u16 = 100;
/// Maximum UTF-8 bytes in a durable worker owner identity.
pub const MAX_WORKER_OWNER_BYTES: usize = 128;

/// Validated process-instance identity used by durable worker claims.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct WorkerOwner(String);

impl WorkerOwner {
  /// Validates a non-empty bounded owner identity.
  pub fn new(value: impl Into<String>) -> Result<Self, StoreError> {
    let value = value.into();
    if value.is_empty() || value.len() > MAX_WORKER_OWNER_BYTES {
      return Err(StoreError::invalid(
        StoreOperation::ClaimExpiredLeases,
        StoreInputError::InvalidWorkerClaim,
      ));
    }
    Ok(Self(value))
  }

  /// Borrows the opaque identity.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

/// Bounded request to acquire durable ownership of expired-Lease work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimExpiredLeases {
  /// Process instance acquiring the work.
  pub owner: WorkerOwner,
  /// Authoritative time used to find expired Leases and stale claims.
  pub observed_at: Timestamp,
  /// Exclusive deadline after which another replica may reclaim the work.
  pub claim_expires_at: Timestamp,
  /// Positive bounded number of claims.
  pub limit: NonZeroU16,
}

impl ClaimExpiredLeases {
  /// Validates the claim window and batch bound.
  pub fn new(
    owner: WorkerOwner,
    observed_at: Timestamp,
    claim_expires_at: Timestamp,
    limit: u16,
  ) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit)
      .filter(|value| value.get() <= MAX_LEASE_EXPIRY_BATCH_SIZE)
      .ok_or_else(|| StoreError::invalid(StoreOperation::ClaimExpiredLeases, StoreInputError::InvalidWorkerClaim))?;
    if claim_expires_at <= observed_at {
      return Err(StoreError::invalid(
        StoreOperation::ClaimExpiredLeases,
        StoreInputError::InvalidWorkerClaim,
      ));
    }
    Ok(Self {
      owner,
      observed_at,
      claim_expires_at,
      limit,
    })
  }
}

/// One expired Lease exclusively claimed by a durable worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpiredLeaseClaim {
  /// Expired Lease to fence and recover.
  pub lease_id: LeaseId,
  /// Job whose ownership expired.
  pub job_id: JobId,
  /// Owner that must present the claim when applying recovery.
  pub owner: WorkerOwner,
  /// Exclusive claim deadline.
  pub claim_expires_at: Timestamp,
}

/// Atomic request to fence and recover one claimed expired Lease.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoverExpiredLease {
  /// Durable work claim.
  pub claim: ExpiredLeaseClaim,
  /// Authoritative recovery time.
  pub recovered_at: Timestamp,
}

/// Durable result of expired-Lease recovery.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LeaseRecoveryAction {
  /// The Job was returned to the ready queue within its retry policy.
  Requeued,
  /// Infrastructure retries were disabled or exhausted and the Job failed.
  Failed,
  /// A durable Build cancellation caused the Job to become cancelled.
  Cancelled,
  /// Another committed recovery already made the Lease terminal.
  AlreadyRecovered,
}

/// Result of one idempotent expired-Lease recovery.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecoverExpiredLeaseOutcome {
  /// Whether this call changed authoritative state or observed prior recovery.
  pub disposition: MutationDisposition,
  /// Expired Lease identity.
  pub lease_id: LeaseId,
  /// Recovered Job identity.
  pub job_id: JobId,
  /// Recovery action.
  pub action: LeaseRecoveryAction,
  /// Number of infrastructure requeues consumed by this Job.
  pub infrastructure_requeues: u16,
}
