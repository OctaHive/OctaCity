use async_trait::async_trait;

use crate::{
  ClaimTriggerEvaluations, CompleteTriggerEvaluation, FailTriggerEvaluation, RecordTriggerEvaluationRevision,
  ReserveTriggerEvaluation, StoreError, TriggerEvaluationClaim, TriggerEvaluationReservation,
};

/// Durable leased work used to recover manual Trigger evaluation across process failure.
#[async_trait]
pub trait TriggerEvaluationWorkStore: Send + Sync {
  /// Persists caller intent before VCS access and grants at most one initial claim.
  async fn reserve_trigger_evaluation(
    &self,
    request: ReserveTriggerEvaluation,
  ) -> Result<TriggerEvaluationReservation, StoreError>;

  /// Claims a bounded batch of due or abandoned evaluations.
  async fn claim_trigger_evaluations(
    &self,
    request: ClaimTriggerEvaluations,
  ) -> Result<Vec<TriggerEvaluationClaim>, StoreError>;

  /// Persists the immutable VCS result while retaining the current claim.
  async fn record_trigger_evaluation_revision(
    &self,
    request: RecordTriggerEvaluationRevision,
  ) -> Result<(), StoreError>;

  /// Marks work complete after Trigger acceptance or suppression commits.
  async fn complete_trigger_evaluation(&self, request: CompleteTriggerEvaluation) -> Result<(), StoreError>;

  /// Releases transient work for retry or records a terminal dead letter.
  async fn fail_trigger_evaluation(&self, request: FailTriggerEvaluation) -> Result<(), StoreError>;
}
