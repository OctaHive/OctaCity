use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use octacity_server_domain::{AttemptId, BuildId, JobId, Timestamp};
use octacity_server_job::JobState;
use octacity_server_store::{
  AttemptRecord, BuildControlStore, BuildQueryStore, CancelBuild, IdempotencyKey, JobRecord,
  MutationDisposition as StoreMutationDisposition, RetryBuild, StoreError,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
  ApplicationError, AttemptProjection, BuildProjection, Command, CommandHandler, DagCausalityProjection,
  JobAssignmentProjection, JobProjection, JobProjectionFacts, JobQueueProjection, JobTerminalOutcomeProjection,
  MutationDisposition, Query, QueryHandler, TriggerHistoryProjection,
};

const RETRY_ATTEMPT_NAMESPACE: Uuid = Uuid::from_u128(0xe0de_164d_8f62_5f1e_9e0c_02bd_619a_3861);

/// Reads one Build together with its current Attempt summary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetBuildQuery {
  /// Stable Build identity.
  pub build_id: BuildId,
}

impl Query for GetBuildQuery {
  type Outcome = BuildDetailsProjection;
}

/// Reads one Attempt and its complete diagnostic DAG.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetAttemptQuery {
  /// Stable Attempt identity.
  pub attempt_id: AttemptId,
}

impl Query for GetAttemptQuery {
  type Outcome = AttemptDetailsProjection;
}

/// Reads one Job with execution diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetJobQuery {
  /// Stable Job identity.
  pub job_id: JobId,
}

impl Query for GetJobQuery {
  type Outcome = JobProjection;
}

/// Persists cancellation intent for one active Build.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelBuildCommand {
  /// Build to cancel.
  pub build_id: BuildId,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative request time.
  pub requested_at: Timestamp,
}

impl Command for CancelBuildCommand {
  type Outcome = CancelBuildCommandOutcome;
}

/// Creates the next Attempt from one failed Build's immutable snapshots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetryBuildCommand {
  /// Failed Build to retry.
  pub build_id: BuildId,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative request time.
  pub requested_at: Timestamp,
}

impl Command for RetryBuildCommand {
  type Outcome = RetryBuildCommandOutcome;
}

/// Safe Build read with a link to the latest Attempt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BuildDetailsProjection {
  /// Immutable Build and current aggregate state.
  pub build: BuildProjection,
  /// Latest Attempt in the Build.
  pub current_attempt: AttemptProjection,
}

/// Safe Attempt read with complete Job state and causal topology.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AttemptDetailsProjection {
  /// Attempt aggregate state.
  pub attempt: AttemptProjection,
  /// Materialized Jobs ordered by stable identity.
  pub jobs: Vec<JobProjection>,
  /// Validated causal DAG derived from the same Jobs.
  pub dag: DagCausalityProjection,
}

/// Result of applying or replaying Build cancellation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CancelBuildCommandOutcome {
  /// Whether the mutation was newly applied or replayed.
  pub disposition: MutationDisposition,
  /// Cancelled Build.
  pub build_id: BuildId,
  /// Attempt current when cancellation was accepted.
  pub attempt_id: AttemptId,
  /// Jobs made terminal without a current owner.
  pub cancelled_job_ids: Vec<JobId>,
  /// Jobs whose current owner receives a cancellation directive.
  pub cancelling_job_ids: Vec<JobId>,
}

/// Result of applying or replaying one Build retry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RetryBuildCommandOutcome {
  /// Whether the mutation was newly applied or replayed.
  pub disposition: MutationDisposition,
  /// Retried Build.
  pub build_id: BuildId,
  /// Prior failed Attempt retained as history.
  pub source_attempt_id: AttemptId,
  /// Newly materialized Attempt.
  pub attempt_id: AttemptId,
  /// Positive allocated Attempt number.
  pub attempt_number: u64,
  /// Root Jobs made ready.
  pub ready_job_ids: Vec<JobId>,
}

/// Typed execution management handlers backed by narrow read and mutation ports.
pub struct BuildHandlers<S> {
  store: Arc<S>,
}

impl<S> BuildHandlers<S> {
  /// Creates handlers from one backend-neutral store implementation.
  pub fn new(store: Arc<S>) -> Self {
    Self { store }
  }
}

#[async_trait]
impl<S> QueryHandler<GetBuildQuery> for BuildHandlers<S>
where
  S: BuildQueryStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_query(&self, query: GetBuildQuery) -> Result<BuildDetailsProjection, Self::Error> {
    let build = self.store.build(query.build_id).await?;
    let current_attempt = self.store.latest_attempt(query.build_id).await?;
    Ok(BuildDetailsProjection {
      build: BuildProjection::from_authoritative(
        &build.build,
        build.state,
        build.version,
        TriggerHistoryProjection::from_occurrence(
          &build.trigger,
          build.trigger_state,
          Some(build.build.id),
          build.trigger_created_at,
          build.trigger_updated_at,
        ),
        build.created_at,
        build.updated_at,
      )?,
      current_attempt: attempt_projection(&current_attempt),
    })
  }
}

#[async_trait]
impl<S> QueryHandler<GetAttemptQuery> for BuildHandlers<S>
where
  S: BuildQueryStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_query(&self, query: GetAttemptQuery) -> Result<AttemptDetailsProjection, Self::Error> {
    project_attempt(self.store.attempt(query.attempt_id).await?)
  }
}

#[async_trait]
impl<S> QueryHandler<GetJobQuery> for BuildHandlers<S>
where
  S: BuildQueryStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_query(&self, query: GetJobQuery) -> Result<JobProjection, Self::Error> {
    project_job(self.store.job(query.job_id).await?)
  }
}

#[async_trait]
impl<S> CommandHandler<CancelBuildCommand> for BuildHandlers<S>
where
  S: BuildControlStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(&self, command: CancelBuildCommand) -> Result<CancelBuildCommandOutcome, Self::Error> {
    let outcome = self
      .store
      .cancel_build(CancelBuild {
        build_id: command.build_id,
        idempotency_key: command.idempotency_key,
        requested_at: command.requested_at,
      })
      .await?;
    Ok(CancelBuildCommandOutcome {
      disposition: mutation_disposition(outcome.disposition),
      build_id: outcome.build_id,
      attempt_id: outcome.attempt_id,
      cancelled_job_ids: outcome.cancelled_jobs,
      cancelling_job_ids: outcome.cancelling_jobs,
    })
  }
}

#[async_trait]
impl<S> CommandHandler<RetryBuildCommand> for BuildHandlers<S>
where
  S: BuildQueryStore + BuildControlStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(&self, command: RetryBuildCommand) -> Result<RetryBuildCommandOutcome, Self::Error> {
    let attempt_id = retry_attempt_id(command.build_id, &command.idempotency_key)?;
    let (attempt_number, jobs) = match self.store.attempt(attempt_id).await {
      Ok(replay) if replay.build_id == command.build_id => (
        replay.number,
        replay.jobs.into_iter().map(|record| record.job).collect(),
      ),
      Ok(_) => return Err(StoreError::Unavailable.into()),
      Err(StoreError::NotFound { .. }) => {
        let source = self.store.latest_attempt(command.build_id).await?;
        (
          source.number.next().map_err(|_| StoreError::Unavailable)?,
          retry_jobs(attempt_id, &source.jobs)?,
        )
      }
      Err(error) => return Err(error.into()),
    };
    let outcome = self
      .store
      .retry_build(RetryBuild::new(
        command.build_id,
        attempt_id,
        attempt_number,
        jobs,
        command.idempotency_key,
        command.requested_at,
      )?)
      .await?;
    Ok(RetryBuildCommandOutcome {
      disposition: mutation_disposition(outcome.disposition),
      build_id: outcome.build_id,
      source_attempt_id: outcome.source_attempt_id,
      attempt_id: outcome.attempt_id,
      attempt_number: outcome.attempt_number.get(),
      ready_job_ids: outcome.ready_jobs,
    })
  }
}

fn project_attempt(record: AttemptRecord) -> Result<AttemptDetailsProjection, ApplicationError> {
  let attempt = attempt_projection(&record);
  let jobs = record
    .jobs
    .into_iter()
    .map(project_job)
    .collect::<Result<Vec<_>, _>>()?;
  let dag = DagCausalityProjection::new(attempt.id, &jobs)?;
  Ok(AttemptDetailsProjection { attempt, jobs, dag })
}

fn attempt_projection(record: &AttemptRecord) -> AttemptProjection {
  AttemptProjection {
    id: record.id,
    build_id: record.build_id,
    number: record.number,
    retry_of_attempt_id: record.retry_of_attempt_id,
    state: record.state,
    version: record.version,
    created_at: record.created_at,
    updated_at: record.updated_at,
  }
}

fn project_job(record: JobRecord) -> Result<JobProjection, ApplicationError> {
  let terminal = match record.terminal {
    None => None,
    Some(terminal) => Some(match terminal.state {
      JobState::Succeeded => JobTerminalOutcomeProjection::succeeded(terminal.completed_at),
      JobState::Failed => JobTerminalOutcomeProjection::failed(
        terminal.failure_class.ok_or(StoreError::Unavailable)?,
        terminal.completed_at,
      ),
      JobState::Cancelled => JobTerminalOutcomeProjection::cancelled(terminal.completed_at),
      JobState::Skipped => JobTerminalOutcomeProjection::skipped(terminal.completed_at),
      _ => return Err(StoreError::Unavailable.into()),
    }),
  };
  JobProjection::from_authoritative(
    record.attempt_id,
    &record.job,
    JobProjectionFacts {
      state: record.state,
      version: record.version,
      created_at: record.created_at,
      updated_at: record.updated_at,
      queue: record.queue.map(|queue| JobQueueProjection {
        priority: queue.priority,
        enqueued_at: queue.enqueued_at,
      }),
      assignment: record.assignment.map(|assignment| JobAssignmentProjection {
        selected_pool_id: assignment.pool_id,
        assigned_agent_id: assignment.agent_id,
      }),
      terminal,
      event_cursor: record.event_cursor,
      outputs: Vec::new(),
    },
  )
  .map_err(Into::into)
}

fn retry_attempt_id(build_id: BuildId, key: &IdempotencyKey) -> Result<AttemptId, StoreError> {
  let identity = format!("{build_id}\0{key}");
  AttemptId::from_uuid(Uuid::new_v5(&RETRY_ATTEMPT_NAMESPACE, identity.as_bytes())).map_err(|_| StoreError::Unavailable)
}

fn retry_jobs(
  attempt_id: AttemptId,
  source: &[JobRecord],
) -> Result<Vec<octacity_server_store::MaterializedJob>, StoreError> {
  let ids = source
    .iter()
    .map(|record| {
      let id = JobId::from_uuid(Uuid::new_v5(
        &attempt_id.as_uuid(),
        record.job.pipeline_node_id.as_str().as_bytes(),
      ))
      .map_err(|_| StoreError::Unavailable)?;
      Ok((record.job.id, id))
    })
    .collect::<Result<BTreeMap<_, _>, StoreError>>()?;
  source
    .iter()
    .map(|record| {
      let mut job = record.job.clone();
      job.id = *ids.get(&job.id).ok_or(StoreError::Unavailable)?;
      job.dependencies = job
        .dependencies
        .iter()
        .map(|id| ids.get(id).copied().ok_or(StoreError::Unavailable))
        .collect::<Result<_, _>>()?;
      Ok(job)
    })
    .collect()
}

const fn mutation_disposition(value: StoreMutationDisposition) -> MutationDisposition {
  match value {
    StoreMutationDisposition::Applied => MutationDisposition::Applied,
    StoreMutationDisposition::Replayed => MutationDisposition::Replayed,
  }
}
