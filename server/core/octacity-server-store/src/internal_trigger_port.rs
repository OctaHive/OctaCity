use async_trait::async_trait;

use crate::{
  ClaimInternalTriggerEvents, CompleteInternalTriggerEvent, InternalTriggerEventClaim, MutationDisposition, StoreError,
};

/// Replica-safe delivery of terminal Build events from the transactional outbox.
#[async_trait]
pub trait InternalTriggerEventStore: Send + Sync {
  /// Claims a bounded batch and loads immutable matching Trigger definitions.
  async fn claim_internal_trigger_events(
    &self,
    request: ClaimInternalTriggerEvents,
  ) -> Result<Vec<InternalTriggerEventClaim>, StoreError>;

  /// Marks one source event delivered only while the caller owns its claim.
  async fn complete_internal_trigger_event(
    &self,
    request: CompleteInternalTriggerEvent,
  ) -> Result<MutationDisposition, StoreError>;
}
