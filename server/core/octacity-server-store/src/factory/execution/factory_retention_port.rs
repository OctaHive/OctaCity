use async_trait::async_trait;

use crate::{
  AdvanceFactoryRetentionWork, ClaimFactoryRetentionWork, FactoryRetentionPassOutcome, FactoryRetentionWorkClaim,
  FailFactoryRetentionWork, StoreError,
};

/// Authoritative bounded, resumable cleanup of Factory metadata references.
#[async_trait]
pub trait FactoryRetentionStore: Send + Sync {
  /// Claims due terminal Runs while active, escalated, and held Runs remain visible.
  async fn claim_factory_retention_work(
    &self,
    request: ClaimFactoryRetentionWork,
  ) -> Result<Vec<FactoryRetentionWorkClaim>, StoreError>;

  /// Atomically hides one Run and advances one bounded reference or metadata page.
  async fn advance_factory_retention_work(
    &self,
    request: AdvanceFactoryRetentionWork,
  ) -> Result<FactoryRetentionPassOutcome, StoreError>;

  /// Releases failed work for retry or moves it to a durable dead letter.
  async fn fail_factory_retention_work(&self, request: FailFactoryRetentionWork) -> Result<(), StoreError>;
}
