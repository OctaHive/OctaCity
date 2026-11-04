use std::num::NonZeroU16;

use octacity_server_domain::{ImmutableRevision, Timestamp, TriggerOccurrenceId};
use serde_json::Value;

use crate::{StoreError, StoreInputError, StoreOperation, WorkerOwner};

/// Maximum manual Trigger evaluations acquired by one worker pass.
pub const MAX_TRIGGER_EVALUATION_BATCH_SIZE: u16 = 32;
/// Maximum encoded bytes retained for one transport-independent Trigger command.
pub const MAX_TRIGGER_EVALUATION_PAYLOAD_BYTES: usize = 1024 * 1024;
/// Maximum secret-free diagnostic bytes retained for one failed evaluation.
pub const MAX_TRIGGER_EVALUATION_DIAGNOSTIC_BYTES: usize = 4096;

/// Atomically persists and initially claims one manual Trigger evaluation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReserveTriggerEvaluation {
  /// Stable normalized occurrence identity.
  pub occurrence_id: TriggerOccurrenceId,
  /// Stable digest of caller intent, independent of resolved VCS state.
  pub intent_digest: [u8; 32],
  /// Bounded application command encoded as an opaque JSON object.
  pub payload: Value,
  /// Unique owner of the initial synchronous attempt.
  pub owner: WorkerOwner,
  /// Authoritative reservation time.
  pub requested_at: Timestamp,
  /// Exclusive deadline after which a worker may recover the attempt.
  pub claim_expires_at: Timestamp,
}

impl ReserveTriggerEvaluation {
  /// Revalidates payload and ownership bounds at the persistence seam.
  pub fn validate(&self) -> Result<(), StoreError> {
    if !self.payload.is_object()
      || serde_json::to_vec(&self.payload).map_or(true, |value| value.len() > MAX_TRIGGER_EVALUATION_PAYLOAD_BYTES)
      || self.claim_expires_at <= self.requested_at
    {
      return Err(StoreError::invalid(
        StoreOperation::ReserveTriggerEvaluation,
        StoreInputError::InvalidTriggerEvaluation,
      ));
    }
    Ok(())
  }
}

/// Result of replay-safely reserving one Trigger evaluation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TriggerEvaluationReservation {
  /// This caller owns the first or a recovered attempt.
  Claimed(TriggerEvaluationClaim),
  /// Equivalent work is currently owned by another caller or worker.
  Pending,
  /// Equivalent work and its Build transaction already completed.
  Completed,
  /// Equivalent work exhausted retries or failed permanently.
  DeadLetter,
}

/// Bounded request to acquire due Trigger evaluations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimTriggerEvaluations {
  /// Process instance acquiring work.
  pub owner: WorkerOwner,
  /// Authoritative due-work and stale-claim time.
  pub observed_at: Timestamp,
  /// Exclusive deadline after which another replica may reclaim work.
  pub claim_expires_at: Timestamp,
  /// Positive bounded number of evaluations.
  pub limit: NonZeroU16,
}

impl ClaimTriggerEvaluations {
  /// Validates the ownership window and batch bound.
  pub fn new(
    owner: WorkerOwner,
    observed_at: Timestamp,
    claim_expires_at: Timestamp,
    limit: u16,
  ) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit)
      .filter(|value| value.get() <= MAX_TRIGGER_EVALUATION_BATCH_SIZE)
      .ok_or_else(|| {
        StoreError::invalid(
          StoreOperation::ClaimTriggerEvaluations,
          StoreInputError::InvalidWorkerClaim,
        )
      })?;
    if claim_expires_at <= observed_at {
      return Err(StoreError::invalid(
        StoreOperation::ClaimTriggerEvaluations,
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

/// One durably owned manual Trigger evaluation attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TriggerEvaluationClaim {
  /// Stable normalized occurrence identity.
  pub occurrence_id: TriggerOccurrenceId,
  /// Bounded opaque application command.
  pub payload: Value,
  /// Immutable VCS result checkpointed before Build creation, when required.
  pub resolved_revision: Option<ImmutableRevision>,
  /// Attempt number including this claim.
  pub attempt: u16,
  /// Owner required by subsequent completion or failure.
  pub owner: WorkerOwner,
  /// Exclusive ownership deadline.
  pub claim_expires_at: Timestamp,
}

/// Fenced checkpoint that makes mutable reference resolution a one-time read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordTriggerEvaluationRevision {
  /// Stable normalized occurrence identity.
  pub occurrence_id: TriggerOccurrenceId,
  /// Current claim owner.
  pub owner: WorkerOwner,
  /// Exact immutable revision returned by the VCS adapter.
  pub resolved_revision: ImmutableRevision,
  /// Authoritative checkpoint time used to validate the claim deadline.
  pub recorded_at: Timestamp,
}

/// Completes one owned Trigger evaluation after its Build transaction commits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteTriggerEvaluation {
  /// Stable normalized occurrence identity.
  pub occurrence_id: TriggerOccurrenceId,
  /// Current claim owner.
  pub owner: WorkerOwner,
  /// Authoritative completion time.
  pub completed_at: Timestamp,
}

/// Schedules retry or records a terminal Trigger-evaluation dead letter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FailTriggerEvaluation {
  /// Stable normalized occurrence identity.
  pub occurrence_id: TriggerOccurrenceId,
  /// Current claim owner.
  pub owner: WorkerOwner,
  /// Bounded secret-free diagnostic.
  pub diagnostic: String,
  /// Authoritative failure time.
  pub failed_at: Timestamp,
  /// Next attempt time for a transient failure; `None` creates a dead letter.
  pub retry_at: Option<Timestamp>,
}

impl FailTriggerEvaluation {
  /// Revalidates diagnostics and retry ordering.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.diagnostic.is_empty()
      || self.diagnostic.len() > MAX_TRIGGER_EVALUATION_DIAGNOSTIC_BYTES
      || self.diagnostic.chars().any(char::is_control)
      || self.retry_at.is_some_and(|retry_at| retry_at <= self.failed_at)
    {
      return Err(StoreError::invalid(
        StoreOperation::FailTriggerEvaluation,
        StoreInputError::InvalidTriggerEvaluation,
      ));
    }
    Ok(())
  }
}
