use async_trait::async_trait;

use crate::{
  AdmitFactoryWork, FactoryAdmissionContext, FactoryAdmissionMutationOutcome, FactoryAdmissionProbe,
  ManagementMutation, ReadFactoryAdmissionContext, StoreError,
};

/// Backend-neutral atomic operations for manual Factory Work admission.
///
/// The replay probe deliberately precedes immutable definition reads and VCS
/// resolution, so an exact retry cannot observe a newer mutable source state.
#[async_trait]
pub trait FactoryAdmissionStore: Send + Sync {
  /// Returns the original admission for an exact replay, rejects a mismatch,
  /// or reports that the identity has not been admitted.
  async fn replay_factory_admission(
    &self,
    probe: &FactoryAdmissionProbe,
  ) -> Result<Option<FactoryAdmissionMutationOutcome>, StoreError>;

  /// Loads the exact Project-owned definitions needed for a new admission.
  async fn factory_admission_context(
    &self,
    request: ReadFactoryAdmissionContext,
  ) -> Result<FactoryAdmissionContext, StoreError>;

  /// Atomically persists the immutable Work Envelope, initial Factory Run,
  /// idempotency outcome, audit fact, and outbox entry.
  async fn admit_factory_work(
    &self,
    request: ManagementMutation<AdmitFactoryWork>,
  ) -> Result<FactoryAdmissionMutationOutcome, StoreError>;
}
