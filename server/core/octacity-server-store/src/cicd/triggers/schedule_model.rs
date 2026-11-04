use std::num::NonZeroU16;

use octacity_server_domain::Timestamp;
use octacity_server_trigger::ScheduleDefinition;

use crate::{
  CreateTriggerDefinition, StoreError, StoreInputError, StoreOperation, TriggerDefinitionRef, TriggerTarget,
  WorkerOwner,
};

/// Maximum schedules acquired by one durable worker claim.
pub const MAX_SCHEDULE_CLAIM_BATCH_SIZE: u16 = 100;

/// Atomic creation of a scheduled Trigger and its first durable cursor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateSchedule {
  /// Common immutable Trigger definition.
  pub trigger: CreateTriggerDefinition,
  /// Validated calendar and missed-run rules.
  pub schedule: ScheduleDefinition,
  /// First occurrence strictly after Trigger creation.
  pub next_occurrence_at: Timestamp,
}

impl CreateSchedule {
  /// Revalidates cross-field invariants at the persistence seam.
  pub fn validate(&self) -> Result<(), StoreError> {
    self.trigger.validate()?;
    if self.trigger.kind != crate::TriggerKind::Scheduled
      || self.schedule.validate().is_err()
      || self.schedule.next_after(self.trigger.created_at).ok() != Some(self.next_occurrence_at)
    {
      return Err(StoreError::invalid(
        StoreOperation::CreateSchedule,
        StoreInputError::InvalidNormalizedTrigger,
      ));
    }
    Ok(())
  }
}

/// Durable management projection of one schedule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleRecord {
  /// Exact immutable Trigger version that owns the schedule.
  pub trigger: TriggerDefinitionRef,
  /// Build Configuration selected by the Trigger.
  pub target: TriggerTarget,
  /// Whether new occurrences may be evaluated.
  pub enabled: bool,
  /// Trigger-specific Build input used for every occurrence.
  pub definition: serde_json::Value,
  /// Calendar and missed-run rules.
  pub schedule: ScheduleDefinition,
  /// Durable cursor for the next unprocessed occurrence.
  pub next_occurrence_at: Timestamp,
}

/// Bounded request to claim schedules whose durable cursor is due.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimDueSchedules {
  /// Process instance acquiring work.
  pub owner: WorkerOwner,
  /// Authoritative time used to select due work and stale claims.
  pub observed_at: Timestamp,
  /// Exclusive deadline after which another replica may reclaim work.
  pub claim_expires_at: Timestamp,
  /// Positive bounded number of claims.
  pub limit: NonZeroU16,
}

impl ClaimDueSchedules {
  /// Validates ownership duration and batch size.
  pub fn new(
    owner: WorkerOwner,
    observed_at: Timestamp,
    claim_expires_at: Timestamp,
    limit: u16,
  ) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit)
      .filter(|limit| limit.get() <= MAX_SCHEDULE_CLAIM_BATCH_SIZE)
      .ok_or_else(|| StoreError::invalid(StoreOperation::ClaimDueSchedules, StoreInputError::InvalidWorkerClaim))?;
    if claim_expires_at <= observed_at {
      return Err(StoreError::invalid(
        StoreOperation::ClaimDueSchedules,
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

/// One due schedule exclusively owned until its claim deadline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DueScheduleClaim {
  /// Management-visible schedule data and the claimed cursor.
  pub schedule: ScheduleRecord,
  /// Owner that must complete the claim.
  pub owner: WorkerOwner,
  /// Exclusive claim deadline.
  pub claim_expires_at: Timestamp,
}

/// Atomic cursor advancement after every selected occurrence was evaluated.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteScheduleClaim {
  /// Exact Trigger version whose schedule was claimed.
  pub trigger: TriggerDefinitionRef,
  /// Owner returned by the claim operation.
  pub owner: WorkerOwner,
  /// Claimed cursor used as an optimistic concurrency fence.
  pub expected_next_occurrence_at: Timestamp,
  /// First occurrence not processed by this claim.
  pub next_occurrence_at: Timestamp,
  /// Authoritative completion time, which must precede claim expiry.
  pub completed_at: Timestamp,
}
