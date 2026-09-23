use std::num::NonZeroU16;

use octacity_server_domain::{BuildId, Timestamp, TriggerIdentity};

use crate::{
  NormalizedTriggerOccurrence, StoreError, StoreInputError, StoreOperation, TriggerDefinitionRef, TriggerEventKind,
  TriggerTarget, WorkerOwner,
};

/// Maximum terminal Build events acquired by one internal-Trigger worker pass.
pub const MAX_INTERNAL_TRIGGER_EVENT_BATCH_SIZE: u16 = 100;
/// Maximum matching Trigger definitions expanded from one terminal Build event.
pub const MAX_INTERNAL_TRIGGER_MATCHES_PER_EVENT: usize = 256;

/// Bounded request to claim terminal Build events from the transactional outbox.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimInternalTriggerEvents {
  /// Process instance acquiring work.
  pub owner: WorkerOwner,
  /// Authoritative time used to select available work and stale claims.
  pub observed_at: Timestamp,
  /// Exclusive deadline after which another replica may reclaim work.
  pub claim_expires_at: Timestamp,
  /// Positive bounded number of source events.
  pub limit: NonZeroU16,
}

impl ClaimInternalTriggerEvents {
  /// Validates ownership duration and batch size.
  pub fn new(
    owner: WorkerOwner,
    observed_at: Timestamp,
    claim_expires_at: Timestamp,
    limit: u16,
  ) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit)
      .filter(|limit| limit.get() <= MAX_INTERNAL_TRIGGER_EVENT_BATCH_SIZE)
      .ok_or_else(|| {
        StoreError::invalid(
          StoreOperation::ClaimInternalTriggerEvents,
          StoreInputError::InvalidWorkerClaim,
        )
      })?;
    if claim_expires_at <= observed_at {
      return Err(StoreError::invalid(
        StoreOperation::ClaimInternalTriggerEvents,
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

/// One immutable internal Trigger definition matched to a source event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternalTriggerMatch {
  /// Exact immutable Trigger definition.
  pub trigger: TriggerDefinitionRef,
  /// Exact immutable Build Configuration selected by the Trigger.
  pub target: TriggerTarget,
  /// Strict kind-specific definition decoded by the application worker.
  pub definition: serde_json::Value,
}

/// One terminal Build event exclusively owned until its claim deadline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternalTriggerEventClaim {
  /// Stable transactional-outbox identity used for retry deduplication.
  pub event_identity: TriggerIdentity,
  /// Build whose terminal transition produced the event.
  pub source_build_id: BuildId,
  /// Persisted occurrence that created the source Build.
  pub source_occurrence: NormalizedTriggerOccurrence,
  /// Root-to-parent immutable Trigger lineage used for cycle detection.
  pub trigger_ancestry: Vec<TriggerDefinitionRef>,
  /// Documented server-owned terminal event classification.
  pub event_kind: TriggerEventKind,
  /// Time at which the source transition committed.
  pub occurred_at: Timestamp,
  /// Enabled definitions that matched at source-event time.
  pub matches: Vec<InternalTriggerMatch>,
  /// Owner that must complete this outbox delivery.
  pub owner: WorkerOwner,
  /// Exclusive claim deadline.
  pub claim_expires_at: Timestamp,
}

/// Marks one claimed source event delivered after every candidate was handled.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteInternalTriggerEvent {
  /// Stable transactional-outbox identity returned by the claim.
  pub event_identity: TriggerIdentity,
  /// Owner returned by the claim operation.
  pub owner: WorkerOwner,
  /// Authoritative completion time, which must precede claim expiry.
  pub completed_at: Timestamp,
}
