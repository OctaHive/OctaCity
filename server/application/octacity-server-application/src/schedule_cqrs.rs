use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationVersion, Timestamp, TriggerId, TriggerIdentity, TriggerVersion,
};
use octacity_server_store::{
  ClaimDueSchedules, CompleteScheduleClaim, CreateSchedule, CreateTriggerDefinition, IdempotencyKey,
  ScheduleDefinition, ScheduleStore, TriggerDefinitionRef, TriggerKind, TriggerTarget, WorkerOwner,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::{
  ApplicationError, Command, CommandHandler, ManualSourceSelection, ManualTriggerCommand, ManualTriggerError,
  ManualTriggerService, MutationDisposition, Query, QueryHandler,
};

/// Immutable Build input evaluated for every occurrence of one schedule.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduledBuildDefinition {
  /// Source selection resolved independently for every occurrence.
  pub source: ManualSourceSelection,
  /// Parameter values resolved against the immutable Build Configuration.
  pub parameters: BTreeMap<String, Value>,
  /// Durable ready-queue priority copied to root Jobs.
  pub priority: i64,
}

/// Typed command that atomically creates a scheduled Trigger and cursor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateScheduleCommand {
  /// Stable Trigger identity selected once by the transport.
  pub id: TriggerId,
  /// Initial immutable Trigger version.
  pub version: TriggerVersion,
  /// Build Configuration selected by this Trigger.
  pub configuration_id: BuildConfigurationId,
  /// Exact immutable Build Configuration version.
  pub configuration_version: BuildConfigurationVersion,
  /// Whether workers may evaluate occurrences.
  pub enabled: bool,
  /// Calendar and bounded missed-run behavior.
  pub schedule: ScheduleDefinition,
  /// Build input evaluated for each due time.
  pub build: ScheduledBuildDefinition,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative creation time.
  pub created_at: Timestamp,
}

/// Query for one exact durable schedule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetScheduleQuery {
  /// Stable Trigger identity.
  pub trigger_id: TriggerId,
  /// Exact immutable Trigger version.
  pub version: TriggerVersion,
}

/// Result of scheduled Trigger creation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleCommandOutcome {
  /// Whether the request applied or replayed an identical request.
  pub disposition: MutationDisposition,
  /// Stable Trigger identity.
  pub trigger_id: TriggerId,
  /// Exact immutable Trigger version.
  pub version: TriggerVersion,
}

/// Safe management projection of one durable schedule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleProjection {
  /// Exact immutable Trigger definition.
  pub trigger: TriggerDefinitionRef,
  /// Exact Build Configuration target.
  pub target: TriggerTarget,
  /// Whether workers may evaluate occurrences.
  pub enabled: bool,
  /// Calendar and bounded missed-run behavior.
  pub schedule: ScheduleDefinition,
  /// First durable occurrence not yet completed.
  pub next_occurrence_at: Timestamp,
  /// Build input evaluated for every occurrence.
  pub build: ScheduledBuildDefinition,
}

impl Command for CreateScheduleCommand {
  type Outcome = ScheduleCommandOutcome;
}

impl Query for GetScheduleQuery {
  type Outcome = ScheduleProjection;
}

/// Typed schedule management handlers backed by one narrow store port.
pub struct ScheduleHandlers<S> {
  store: Arc<S>,
}

impl<S> ScheduleHandlers<S> {
  /// Creates handlers from a durable schedule store.
  pub fn new(store: Arc<S>) -> Self {
    Self { store }
  }
}

#[async_trait]
impl<S> CommandHandler<CreateScheduleCommand> for ScheduleHandlers<S>
where
  S: ScheduleStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(&self, command: CreateScheduleCommand) -> Result<ScheduleCommandOutcome, Self::Error> {
    command.schedule.validate().map_err(|_| ApplicationError::invalid())?;
    let next_occurrence_at = command
      .schedule
      .next_after(command.created_at)
      .map_err(|_| ApplicationError::invalid())?;
    let definition = serde_json::to_value(&command.build).map_err(|_| ApplicationError::invalid())?;
    let outcome = self
      .store
      .create_schedule(CreateSchedule {
        trigger: CreateTriggerDefinition {
          id: command.id,
          version: command.version,
          configuration_id: command.configuration_id,
          configuration_version: command.configuration_version,
          kind: TriggerKind::Scheduled,
          enabled: command.enabled,
          definition,
          idempotency_key: command.idempotency_key,
          created_at: command.created_at,
        },
        schedule: command.schedule,
        next_occurrence_at,
      })
      .await?;
    Ok(ScheduleCommandOutcome {
      disposition: outcome.disposition.into(),
      trigger_id: outcome.trigger_id,
      version: outcome.version,
    })
  }
}

#[async_trait]
impl<S> QueryHandler<GetScheduleQuery> for ScheduleHandlers<S>
where
  S: ScheduleStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_query(&self, query: GetScheduleQuery) -> Result<ScheduleProjection, Self::Error> {
    let record = self.store.schedule(query.trigger_id, query.version).await?;
    let build =
      serde_json::from_value(record.definition).map_err(|_| crate::ProjectionError::InvalidScheduleSnapshot)?;
    Ok(ScheduleProjection {
      trigger: record.trigger,
      target: record.target,
      enabled: record.enabled,
      schedule: record.schedule,
      next_occurrence_at: record.next_occurrence_at,
      build,
    })
  }
}

/// Counts produced by one bounded schedule-worker pass.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScheduleBatchOutcome {
  /// Schedules durably claimed by this replica.
  pub claimed_schedules: usize,
  /// Occurrences accepted, replayed, or intentionally suppressed.
  pub evaluated_occurrences: usize,
}

/// Restart-safe worker that evaluates only durably claimed schedules.
pub struct ScheduleWorker<S> {
  store: Arc<S>,
  triggers: Arc<ManualTriggerService>,
}

impl<S> ScheduleWorker<S>
where
  S: ScheduleStore,
{
  /// Creates a worker from the durable work queue and shared Build evaluator.
  pub fn new(store: Arc<S>, triggers: Arc<ManualTriggerService>) -> Self {
    Self { store, triggers }
  }

  /// Claims and evaluates one bounded batch at an explicit authoritative time.
  pub async fn run_once(
    &self,
    owner: WorkerOwner,
    observed_at: Timestamp,
    claim_expires_at: Timestamp,
    limit: u16,
  ) -> Result<ScheduleBatchOutcome, ScheduleWorkerError> {
    let claims = self
      .store
      .claim_due_schedules(ClaimDueSchedules::new(owner, observed_at, claim_expires_at, limit)?)
      .await?;
    let mut outcome = ScheduleBatchOutcome {
      claimed_schedules: claims.len(),
      evaluated_occurrences: 0,
    };
    for claim in claims {
      let due = claim
        .schedule
        .schedule
        .due_occurrences(claim.schedule.next_occurrence_at, observed_at)
        .map_err(|_| ScheduleWorkerError::InvalidPersistedSchedule)?;
      let build: ScheduledBuildDefinition = serde_json::from_value(claim.schedule.definition.clone())
        .map_err(|_| ScheduleWorkerError::InvalidPersistedSchedule)?;
      for occurrence_at in due.occurrences {
        let identity = TriggerIdentity::new(format!("schedule:{}", occurrence_at.unix_millis()))
          .map_err(|_| ScheduleWorkerError::InvalidPersistedSchedule)?;
        self
          .triggers
          .accept_scheduled(
            ManualTriggerCommand {
              trigger: claim.schedule.trigger,
              target: claim.schedule.target,
              deduplication_identity: identity,
              source: build.source.clone(),
              parameters: build.parameters.clone(),
              priority: build.priority,
              observed_at: occurrence_at,
            },
            observed_at,
          )
          .await?;
        outcome.evaluated_occurrences += 1;
      }
      self
        .store
        .complete_schedule_claim(CompleteScheduleClaim {
          trigger: claim.schedule.trigger,
          owner: claim.owner,
          expected_next_occurrence_at: claim.schedule.next_occurrence_at,
          next_occurrence_at: due.next_occurrence_at,
          completed_at: observed_at,
        })
        .await?;
    }
    Ok(outcome)
  }
}

/// Failure from one bounded schedule-worker pass.
#[derive(Debug, Error)]
pub enum ScheduleWorkerError {
  /// Durable schedule work could not be claimed or completed.
  #[error("durable schedule store failed")]
  Store(#[from] octacity_server_store::StoreError),
  /// Persisted schedule or Build input violates the typed contract.
  #[error("persisted schedule definition is invalid")]
  InvalidPersistedSchedule,
  /// One occurrence could not be evaluated into a Build.
  #[error("scheduled trigger evaluation failed")]
  Trigger(#[from] ManualTriggerError),
}
