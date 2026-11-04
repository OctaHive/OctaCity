use async_trait::async_trait;

use crate::{ClaimExpiredLeases, ExpiredLeaseClaim, RecoverExpiredLease, RecoverExpiredLeaseOutcome, StoreError};

/// Durable, replica-safe work queue for expired Lease recovery.
#[async_trait]
pub trait LeaseRecoveryStore: Send + Sync {
  /// Claims a bounded batch. Concurrent replicas must never own one item together.
  async fn claim_expired_leases(&self, request: ClaimExpiredLeases) -> Result<Vec<ExpiredLeaseClaim>, StoreError>;

  /// Fences one claimed Lease and either requeues or terminally fails its Job.
  async fn recover_expired_lease(&self, request: RecoverExpiredLease)
  -> Result<RecoverExpiredLeaseOutcome, StoreError>;
}
