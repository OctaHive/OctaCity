use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use octacity_server_domain::{EntityKind, Timestamp};
use octacity_server_store::{
  AuditActor, AuditActorKind, ClaimTriggerEvaluations, CompleteTriggerEvaluation, FailTriggerEvaluation,
  ManagementMutation, ManagementSecurityScope, MutationAuditContext, RecordTriggerEvaluationRevision,
  ReserveTriggerEvaluation, StoreError, TriggerEvaluationClaim, TriggerEvaluationReservation,
  TriggerEvaluationWorkStore, WorkerOwner,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{AcceptManualTriggerCommand, ManualTriggerError, ManualTriggerOutcome, ManualTriggerService};
use crate::{
  ApplicationFailure, DurableRetryPolicy, diagnostic::bounded_diagnostic,
  manual_trigger::service::manual_trigger_identity,
};

const PERSISTED_MANUAL_TRIGGER_SCHEMA_VERSION: u16 = 2;
const LEGACY_AUDITED_MANUAL_TRIGGER_SCHEMA_VERSION: u16 = 1;

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
    audit: MutationAuditContext,
    claim: TriggerEvaluationClaim,
    observed_at: Timestamp,
  ) -> Result<ManualTriggerOutcome, ManualTriggerError> {
    match evaluate_durable_claim(
      &self.evaluator,
      &self.work,
      self.retry_policy,
      command,
      audit,
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
impl crate::ManagementCommandUseCase<AcceptManualTriggerCommand> for DurableManualTriggerService {
  type Error = ManualTriggerError;

  async fn execute_management_command(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    command: AcceptManualTriggerCommand,
  ) -> Result<ManualTriggerOutcome, Self::Error> {
    let audit = MutationAuditContext::try_from(context).map_err(|_| ManualTriggerError::SnapshotEncoding)?;
    let (occurrence_id, intent_digest) = manual_trigger_identity(&command.trigger, audit.security_scope())?;
    let payload = serde_json::to_value(PersistedManualTriggerRequest::from_parts(command.clone(), &audit))
      .map_err(|_| ManualTriggerError::SnapshotEncoding)?;
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
      .reserve_trigger_evaluation(ManagementMutation::new(
        ReserveTriggerEvaluation {
          occurrence_id,
          intent_digest: intent_digest.as_bytes(),
          payload,
          owner,
          requested_at: command.accepted_at,
          claim_expires_at,
        },
        audit.clone(),
      ))
      .await
      .map_err(ManualTriggerError::Store)?
    {
      TriggerEvaluationReservation::Claimed(claim) => {
        let (persisted, persisted_audit) = persisted_request(&claim)?;
        self
          .evaluate_claim(persisted, persisted_audit, claim, command.accepted_at)
          .await
      }
      TriggerEvaluationReservation::Completed => {
        self
          .evaluator
          .accept_management(command.trigger, command.accepted_at, audit)
          .await
      }
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
      let (command, audit) = match persisted_request(&claim) {
        Ok(request) => request,
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
        audit,
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
  audit: MutationAuditContext,
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
    .accept_with_resolved_revision(command.trigger, command.accepted_at, resolved_revision, audit)
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

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedManualTriggerRequest {
  schema_version: u16,
  command: AcceptManualTriggerCommand,
  actor_kind: PersistedManagementActorKind,
  actor_identity: Option<String>,
  security_scope: String,
  request_identity: String,
}

impl PersistedManualTriggerRequest {
  fn from_parts(command: AcceptManualTriggerCommand, audit: &MutationAuditContext) -> Self {
    Self {
      schema_version: PERSISTED_MANUAL_TRIGGER_SCHEMA_VERSION,
      command,
      actor_kind: PersistedManagementActorKind::from(audit.actor().kind),
      actor_identity: audit.actor().identity.clone(),
      security_scope: audit.security_scope().as_str().to_owned(),
      request_identity: audit.request_identity().to_owned(),
    }
  }

  fn into_parts(self) -> Result<(AcceptManualTriggerCommand, MutationAuditContext), ManualTriggerError> {
    if self.schema_version != PERSISTED_MANUAL_TRIGGER_SCHEMA_VERSION {
      return Err(ManualTriggerError::SnapshotEncoding);
    }
    let audit = MutationAuditContext::try_new(
      AuditActor {
        kind: self.actor_kind.into(),
        identity: self.actor_identity,
      },
      ManagementSecurityScope::new(self.security_scope).map_err(|_| ManualTriggerError::SnapshotEncoding)?,
      self.request_identity,
    )
    .map_err(|_| ManualTriggerError::SnapshotEncoding)?;
    Ok((self.command, audit))
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyAuditedManualTriggerRequest {
  schema_version: u16,
  command: AcceptManualTriggerCommand,
  actor_kind: PersistedManagementActorKind,
  actor_identity: Option<String>,
  request_identity: String,
}

impl LegacyAuditedManualTriggerRequest {
  fn into_parts(self) -> Result<(AcceptManualTriggerCommand, MutationAuditContext), ManualTriggerError> {
    if self.schema_version != LEGACY_AUDITED_MANUAL_TRIGGER_SCHEMA_VERSION {
      return Err(ManualTriggerError::SnapshotEncoding);
    }
    let audit = MutationAuditContext::try_new(
      AuditActor {
        kind: self.actor_kind.into(),
        identity: self.actor_identity,
      },
      ManagementSecurityScope::trusted_network(),
      self.request_identity,
    )
    .map_err(|_| ManualTriggerError::SnapshotEncoding)?;
    Ok((self.command, audit))
  }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PersistedManualTriggerPayload {
  Scoped(PersistedManualTriggerRequest),
  LegacyAudited(LegacyAuditedManualTriggerRequest),
  Legacy(AcceptManualTriggerCommand),
}

impl PersistedManualTriggerPayload {
  fn into_parts(
    self,
    occurrence_id: octacity_server_domain::TriggerOccurrenceId,
  ) -> Result<(AcceptManualTriggerCommand, MutationAuditContext), ManualTriggerError> {
    match self {
      Self::Scoped(request) => request.into_parts(),
      Self::LegacyAudited(request) => request.into_parts(),
      Self::Legacy(command) => Ok((command, legacy_management_audit(occurrence_id)?)),
    }
  }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PersistedManagementActorKind {
  UnauthenticatedManagement,
  AuthenticatedManagement,
}

impl From<AuditActorKind> for PersistedManagementActorKind {
  fn from(value: AuditActorKind) -> Self {
    match value {
      AuditActorKind::UnauthenticatedManagement => Self::UnauthenticatedManagement,
      AuditActorKind::AuthenticatedManagement => Self::AuthenticatedManagement,
      AuditActorKind::Agent
      | AuditActorKind::Trigger
      | AuditActorKind::Orchestrator
      | AuditActorKind::Adapter
      | AuditActorKind::Worker => unreachable!("validated management audit context contains a non-management actor"),
    }
  }
}

impl From<PersistedManagementActorKind> for AuditActorKind {
  fn from(value: PersistedManagementActorKind) -> Self {
    match value {
      PersistedManagementActorKind::UnauthenticatedManagement => Self::UnauthenticatedManagement,
      PersistedManagementActorKind::AuthenticatedManagement => Self::AuthenticatedManagement,
    }
  }
}

fn persisted_request(
  claim: &TriggerEvaluationClaim,
) -> Result<(AcceptManualTriggerCommand, MutationAuditContext), ManualTriggerError> {
  serde_json::from_value::<PersistedManualTriggerPayload>(claim.payload.clone())
    .map_err(|_| ManualTriggerError::SnapshotEncoding)?
    .into_parts(claim.occurrence_id)
}

fn legacy_management_audit(
  occurrence_id: octacity_server_domain::TriggerOccurrenceId,
) -> Result<MutationAuditContext, ManualTriggerError> {
  MutationAuditContext::try_new(
    AuditActor {
      kind: AuditActorKind::UnauthenticatedManagement,
      identity: None,
    },
    ManagementSecurityScope::trusted_network(),
    format!("legacy-trigger-evaluation:{occurrence_id}"),
  )
  .map_err(|_| ManualTriggerError::SnapshotEncoding)
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
