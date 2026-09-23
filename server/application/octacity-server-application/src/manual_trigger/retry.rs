use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use octacity_server_domain::{EntityKind, Timestamp};
use octacity_server_store::{
  ClaimTriggerEvaluations, CompleteTriggerEvaluation, FailTriggerEvaluation, RecordTriggerEvaluationRevision,
  ReserveTriggerEvaluation, StoreError, TriggerEvaluationClaim, TriggerEvaluationReservation,
  TriggerEvaluationWorkStore, WorkerOwner,
};
use thiserror::Error;

use super::{AcceptManualTriggerCommand, ManualTriggerError, ManualTriggerOutcome, ManualTriggerService};
use crate::{
  ApplicationFailure, CommandHandler, DurableRetryPolicy, diagnostic::bounded_diagnostic,
  manual_trigger::service::manual_trigger_identity,
};

/// Command handler that persists manual Trigger intent before mutable VCS resolution.
pub struct DurableManualTriggerService {
  evaluator: Arc<ManualTriggerService>,
  work: Arc<dyn TriggerEvaluationWorkStore>,
  retry_policy: DurableRetryPolicy,
  claim_lifetime: Duration,
}

impl DurableManualTriggerService {
  /// Creates the durable command handler over evaluation and leased-work modules.
  #[must_use]
  pub fn new(
    evaluator: Arc<ManualTriggerService>,
    work: Arc<dyn TriggerEvaluationWorkStore>,
    retry_policy: DurableRetryPolicy,
    claim_lifetime: Duration,
  ) -> Self {
    Self {
      evaluator,
      work,
      retry_policy,
      claim_lifetime,
    }
  }

  async fn evaluate_claim(
    &self,
    command: AcceptManualTriggerCommand,
    claim: TriggerEvaluationClaim,
    observed_at: Timestamp,
  ) -> Result<ManualTriggerOutcome, ManualTriggerError> {
    match evaluate_durable_claim(
      &self.evaluator,
      &self.work,
      self.retry_policy,
      command,
      claim,
      observed_at,
    )
    .await
    {
      Ok(outcome) => Ok(outcome),
      Err(DurableClaimError::Evaluation { error, .. }) => Err(error),
      Err(DurableClaimError::Store(error)) => Err(ManualTriggerError::Store(error)),
    }
  }
}

#[async_trait]
impl CommandHandler<AcceptManualTriggerCommand> for DurableManualTriggerService {
  type Error = ManualTriggerError;

  async fn handle_command(&self, command: AcceptManualTriggerCommand) -> Result<ManualTriggerOutcome, Self::Error> {
    let (occurrence_id, intent_digest) = manual_trigger_identity(&command.trigger)?;
    let payload = serde_json::to_value(&command).map_err(|_| ManualTriggerError::SnapshotEncoding)?;
    let owner = WorkerOwner::new(format!("manual-trigger:{}", uuid::Uuid::new_v4()))
      .map_err(|_| ManualTriggerError::SnapshotEncoding)?;
    let lifetime = i64::try_from(self.claim_lifetime.as_millis()).map_err(|_| ManualTriggerError::SnapshotEncoding)?;
    let claim_expires_at = Timestamp::from_unix_millis(
      command
        .accepted_at
        .unix_millis()
        .checked_add(lifetime)
        .ok_or(ManualTriggerError::SnapshotEncoding)?,
    )
    .map_err(|_| ManualTriggerError::SnapshotEncoding)?;
    match self
      .work
      .reserve_trigger_evaluation(ReserveTriggerEvaluation {
        occurrence_id,
        intent_digest: intent_digest.as_bytes(),
        payload,
        owner,
        requested_at: command.accepted_at,
        claim_expires_at,
      })
      .await
      .map_err(ManualTriggerError::Store)?
    {
      TriggerEvaluationReservation::Claimed(claim) => {
        let persisted =
          serde_json::from_value(claim.payload.clone()).map_err(|_| ManualTriggerError::SnapshotEncoding)?;
        self.evaluate_claim(persisted, claim, command.accepted_at).await
      }
      TriggerEvaluationReservation::Completed => self.evaluator.accept(command.trigger, command.accepted_at).await,
      TriggerEvaluationReservation::Pending => Err(ManualTriggerError::unavailable()),
      TriggerEvaluationReservation::DeadLetter => Err(ManualTriggerError::Store(StoreError::Conflict {
        entity: EntityKind::Trigger,
      })),
    }
  }
}

/// Counts produced by one bounded durable manual-Trigger retry pass.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ManualTriggerRetryBatchOutcome {
  /// Durable evaluations acquired by this replica.
  pub claimed: usize,
  /// Evaluations whose Build transaction and work completion both committed.
  pub completed: usize,
  /// Transient failures scheduled for another attempt.
  pub retries_scheduled: usize,
  /// Permanent or retry-exhausted evaluations retained as dead letters.
  pub dead_letters: usize,
}

/// Restart-safe worker for manual Trigger evaluations interrupted around VCS access.
pub struct ManualTriggerRetryWorker {
  evaluator: Arc<ManualTriggerService>,
  work: Arc<dyn TriggerEvaluationWorkStore>,
  retry_policy: DurableRetryPolicy,
}

impl ManualTriggerRetryWorker {
  /// Creates the retry worker over durable leased work.
  #[must_use]
  pub fn new(
    evaluator: Arc<ManualTriggerService>,
    work: Arc<dyn TriggerEvaluationWorkStore>,
    retry_policy: DurableRetryPolicy,
  ) -> Self {
    Self {
      evaluator,
      work,
      retry_policy,
    }
  }

  /// Claims and advances one bounded batch at explicit authoritative times.
  pub async fn run_once(
    &self,
    owner: WorkerOwner,
    observed_at: Timestamp,
    claim_expires_at: Timestamp,
    limit: u16,
  ) -> Result<ManualTriggerRetryBatchOutcome, ManualTriggerRetryWorkerError> {
    let claims = self
      .work
      .claim_trigger_evaluations(ClaimTriggerEvaluations::new(
        owner,
        observed_at,
        claim_expires_at,
        limit,
      )?)
      .await?;
    let mut outcome = ManualTriggerRetryBatchOutcome {
      claimed: claims.len(),
      ..ManualTriggerRetryBatchOutcome::default()
    };
    for claim in claims {
      let command: AcceptManualTriggerCommand = match serde_json::from_value(claim.payload.clone()) {
        Ok(command) => command,
        Err(_) => {
          self
            .work
            .fail_trigger_evaluation(FailTriggerEvaluation {
              occurrence_id: claim.occurrence_id,
              owner: claim.owner,
              diagnostic: "persisted manual Trigger command is invalid".to_owned(),
              failed_at: observed_at,
              retry_at: None,
            })
            .await?;
          outcome.dead_letters += 1;
          continue;
        }
      };
      match evaluate_durable_claim(
        &self.evaluator,
        &self.work,
        self.retry_policy,
        command,
        claim,
        observed_at,
      )
      .await
      {
        Ok(_) => outcome.completed += 1,
        Err(DurableClaimError::Evaluation { error, .. }) if cancelled(&error) => {
          return Err(ManualTriggerRetryWorkerError::AdapterCancelled);
        }
        Err(DurableClaimError::Evaluation { retry_at, .. }) => {
          if retry_at {
            outcome.retries_scheduled += 1;
          } else {
            outcome.dead_letters += 1;
          }
        }
        Err(DurableClaimError::Store(error)) => return Err(error.into()),
      }
    }
    Ok(outcome)
  }
}

async fn evaluate_durable_claim(
  evaluator: &ManualTriggerService,
  work: &Arc<dyn TriggerEvaluationWorkStore>,
  retry_policy: DurableRetryPolicy,
  command: AcceptManualTriggerCommand,
  claim: TriggerEvaluationClaim,
  observed_at: Timestamp,
) -> Result<ManualTriggerOutcome, DurableClaimError> {
  let resolved_revision = match claim.resolved_revision.clone() {
    Some(revision) => Some(revision),
    None => match evaluator.resolve_manual_revision(&command.trigger).await {
      Ok(Some(revision)) => {
        work
          .record_trigger_evaluation_revision(RecordTriggerEvaluationRevision {
            occurrence_id: claim.occurrence_id,
            owner: claim.owner.clone(),
            resolved_revision: revision.clone(),
            recorded_at: observed_at,
          })
          .await?;
        Some(revision)
      }
      Ok(None) => None,
      Err(error) => return fail_claim(work, retry_policy, claim, observed_at, error).await,
    },
  };
  match evaluator
    .accept_with_resolved_revision(command.trigger, command.accepted_at, resolved_revision)
    .await
  {
    Ok(outcome) => {
      work
        .complete_trigger_evaluation(CompleteTriggerEvaluation {
          occurrence_id: claim.occurrence_id,
          owner: claim.owner,
          completed_at: observed_at,
        })
        .await?;
      Ok(outcome)
    }
    Err(error) => fail_claim(work, retry_policy, claim, observed_at, error).await,
  }
}

async fn fail_claim(
  work: &Arc<dyn TriggerEvaluationWorkStore>,
  retry_policy: DurableRetryPolicy,
  claim: TriggerEvaluationClaim,
  observed_at: Timestamp,
  error: ManualTriggerError,
) -> Result<ManualTriggerOutcome, DurableClaimError> {
  if cancelled(&error) {
    return Err(DurableClaimError::Evaluation { error, retry_at: false });
  }
  let retry_at = (error.classification() == ApplicationFailure::Unavailable)
    .then(|| retry_policy.retry_at(claim.attempt, observed_at))
    .flatten();
  work
    .fail_trigger_evaluation(FailTriggerEvaluation {
      occurrence_id: claim.occurrence_id,
      owner: claim.owner,
      diagnostic: trigger_diagnostic(&error),
      failed_at: observed_at,
      retry_at,
    })
    .await?;
  Err(DurableClaimError::Evaluation {
    error,
    retry_at: retry_at.is_some(),
  })
}

enum DurableClaimError {
  Evaluation { error: ManualTriggerError, retry_at: bool },
  Store(StoreError),
}

impl From<StoreError> for DurableClaimError {
  fn from(value: StoreError) -> Self {
    Self::Store(value)
  }
}

fn cancelled(error: &ManualTriggerError) -> bool {
  matches!(
    error,
    ManualTriggerError::Revision(super::RevisionResolutionError::Cancelled)
  )
}

fn trigger_diagnostic(error: &ManualTriggerError) -> String {
  bounded_diagnostic(
    &error.to_string(),
    octacity_server_store::MAX_TRIGGER_EVALUATION_DIAGNOSTIC_BYTES,
    "manual Trigger evaluation failed",
  )
}

/// Failure from one bounded durable manual-Trigger retry pass.
#[derive(Debug, Error)]
pub enum ManualTriggerRetryWorkerError {
  /// Durable work could not be claimed or advanced.
  #[error("manual trigger retry store failed")]
  Store(#[from] StoreError),
  /// VCS operation stopped cooperatively with the process cancellation tree.
  #[error("manual trigger VCS operation was cancelled")]
  AdapterCancelled,
}
