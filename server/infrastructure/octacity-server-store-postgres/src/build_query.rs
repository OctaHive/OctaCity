use std::collections::BTreeMap;

use octacity_server_domain::{
  AgentId, AttemptId, AttemptNumber, AttemptVersion, BuildConfigurationId, BuildConfigurationVersion, BuildId,
  BuildVersion, EntityKind, ImmutableRevision, JobId, JobVersion, PipelineId, PipelineNodeId, PipelineVersion, PoolId,
  ProjectId, RepositoryId, RepositoryVersion, Timestamp, TriggerId, TriggerIdentity, TriggerOccurrenceId,
  TriggerVersion,
};
use octacity_server_job::{JobFailureClass, JobRequirements, JobSpecTemplate, JobState};
use octacity_server_pipeline::DependencyPolicy;
use octacity_server_store::{
  AttemptRecord, BuildRecord, BuildRetentionDeadlines, ImmutableBuildInput, JobAssignmentRecord, JobQueueRecord,
  JobRecord, JobTerminalRecord, MaterializedJob, NormalizedTriggerOccurrence, StoreError, TriggerCausality,
  TriggerCause, TriggerDefinitionRef, TriggerMetadata, TriggerOccurrenceState, TriggerTarget,
};
use serde_json::Value;
use sqlx::{FromRow, PgPool, types::Json};
use uuid::Uuid;

use crate::{
  database::unavailable,
  state::{parse_attempt_state, parse_build_state, parse_job_state},
};

macro_rules! id {
  ($type:ty, $value:expr) => {
    <$type>::from_uuid($value).map_err(|_| StoreError::Unavailable)
  };
}

macro_rules! positive {
  ($type:ty, $value:expr) => {
    u64::try_from($value)
      .ok()
      .and_then(|value| <$type>::new(value).ok())
      .ok_or(StoreError::Unavailable)
  };
}

pub(crate) async fn build(pool: &PgPool, build_id: BuildId) -> Result<BuildRecord, StoreError> {
  let row: BuildRow = sqlx::query_as(
    "SELECT build.id, build.project_id, build.build_configuration_id, build.build_configuration_version, \
            build.pipeline_id, build.pipeline_version, build.repository_id, build.repository_version, \
            build.immutable_revision, build.input_snapshot, build.effective_policy_snapshot, \
            FLOOR(EXTRACT(EPOCH FROM build.metadata_retention_until) * 1000)::BIGINT AS metadata_retention_millis, \
            FLOOR(EXTRACT(EPOCH FROM build.log_retention_until) * 1000)::BIGINT AS log_retention_millis, \
            FLOOR(EXTRACT(EPOCH FROM build.artifact_retention_until) * 1000)::BIGINT AS artifact_retention_millis, \
            FLOOR(EXTRACT(EPOCH FROM build.report_retention_until) * 1000)::BIGINT AS report_retention_millis, \
            build.project_job_concurrency_limit, build.priority, \
            build.state, build.version, \
            FLOOR(EXTRACT(EPOCH FROM build.created_at) * 1000)::BIGINT AS created_at_millis, \
            FLOOR(EXTRACT(EPOCH FROM build.updated_at) * 1000)::BIGINT AS updated_at_millis, \
            occurrence.id AS occurrence_id, occurrence.trigger_id, occurrence.trigger_version, \
            occurrence.build_configuration_id AS trigger_configuration_id, \
            occurrence.build_configuration_version AS trigger_configuration_version, \
            occurrence.deduplication_identity, occurrence.cause, occurrence.causality, \
            occurrence.provider_metadata, \
            FLOOR(EXTRACT(EPOCH FROM occurrence.source_time) * 1000)::BIGINT AS source_time_millis, \
            occurrence.state AS trigger_state, \
            FLOOR(EXTRACT(EPOCH FROM occurrence.created_at) * 1000)::BIGINT AS trigger_created_at_millis, \
            FLOOR(EXTRACT(EPOCH FROM occurrence.updated_at) * 1000)::BIGINT AS trigger_updated_at_millis \
     FROM builds AS build \
     JOIN trigger_occurrences AS occurrence ON occurrence.id = build.trigger_occurrence_id \
     WHERE build.id = $1 AND build.metadata_visible",
  )
  .bind(build_id.as_uuid())
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Build,
  })?;
  row.try_into()
}

pub(crate) async fn attempt(pool: &PgPool, attempt_id: AttemptId) -> Result<AttemptRecord, StoreError> {
  let row: AttemptRow = sqlx::query_as(
    "SELECT attempt.id, attempt.build_id, attempt.attempt_number, attempt.retry_of_attempt_id, \
            attempt.state, attempt.version, \
            FLOOR(EXTRACT(EPOCH FROM attempt.created_at) * 1000)::BIGINT AS created_at_millis, \
            FLOOR(EXTRACT(EPOCH FROM attempt.updated_at) * 1000)::BIGINT AS updated_at_millis \
     FROM attempts AS attempt JOIN builds AS build ON build.id = attempt.build_id \
     WHERE attempt.id = $1 AND build.metadata_visible",
  )
  .bind(attempt_id.as_uuid())
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Attempt,
  })?;
  row.into_record(load_jobs(pool, attempt_id).await?)
}

pub(crate) async fn latest_attempt(pool: &PgPool, build_id: BuildId) -> Result<AttemptRecord, StoreError> {
  let row: AttemptRow = sqlx::query_as(
    "SELECT attempt.id, attempt.build_id, attempt.attempt_number, attempt.retry_of_attempt_id, \
            attempt.state, attempt.version, \
            FLOOR(EXTRACT(EPOCH FROM attempt.created_at) * 1000)::BIGINT AS created_at_millis, \
            FLOOR(EXTRACT(EPOCH FROM attempt.updated_at) * 1000)::BIGINT AS updated_at_millis \
     FROM attempts AS attempt JOIN builds AS build ON build.id = attempt.build_id \
     WHERE attempt.build_id = $1 AND build.metadata_visible ORDER BY attempt.attempt_number DESC LIMIT 1",
  )
  .bind(build_id.as_uuid())
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Attempt,
  })?;
  let attempt_id = AttemptId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?;
  row.into_record(load_jobs(pool, attempt_id).await?)
}

pub(crate) async fn job(pool: &PgPool, job_id: JobId) -> Result<JobRecord, StoreError> {
  let row = load_job_rows(pool, Some(job_id.as_uuid()), None)
    .await?
    .into_iter()
    .next()
    .ok_or(StoreError::NotFound {
      entity: EntityKind::Job,
    })?;
  let row_id = row.id;
  let dependencies = load_dependencies(pool, &[row_id]).await?;
  row.into_record(dependencies.get(&row_id).cloned().unwrap_or_default())
}

async fn load_jobs(pool: &PgPool, attempt_id: AttemptId) -> Result<Vec<JobRecord>, StoreError> {
  let rows = load_job_rows(pool, None, Some(attempt_id.as_uuid())).await?;
  let ids = rows.iter().map(|row| row.id).collect::<Vec<_>>();
  let dependencies = load_dependencies(pool, &ids).await?;
  rows
    .into_iter()
    .map(|row| {
      let job_dependencies = dependencies.get(&row.id).cloned().unwrap_or_default();
      row.into_record(job_dependencies)
    })
    .collect()
}

async fn load_job_rows(
  pool: &PgPool,
  job_id: Option<Uuid>,
  attempt_id: Option<Uuid>,
) -> Result<Vec<JobRow>, StoreError> {
  sqlx::query_as(
    "SELECT job.id, job.attempt_id, job.pipeline_node_id, job.state, job.allowed_pool_ids, \
            job.requirements, job.version, job.job_spec_template, job.dependency_policy, \
            FLOOR(EXTRACT(EPOCH FROM job.created_at) * 1000)::BIGINT AS created_at_millis, \
            FLOOR(EXTRACT(EPOCH FROM job.updated_at) * 1000)::BIGINT AS updated_at_millis, \
            queue.priority AS queue_priority, \
            FLOOR(EXTRACT(EPOCH FROM queue.enqueued_at) * 1000)::BIGINT AS enqueued_at_millis, \
            assignment.pool_id AS assigned_pool_id, assignment.agent_id AS assigned_agent_id, \
            completion.kind AS completion_kind, completion.failure_class, \
            FLOOR(EXTRACT(EPOCH FROM completion.completed_at) * 1000)::BIGINT AS completed_at_millis, \
            COALESCE(event_cursor.sequence, 0)::BIGINT AS event_cursor \
     FROM jobs AS job \
     JOIN attempts AS attempt ON attempt.id = job.attempt_id \
     JOIN builds AS build ON build.id = attempt.build_id \
     LEFT JOIN ready_queue_entries AS queue ON queue.job_id = job.id \
     LEFT JOIN LATERAL (\
       SELECT lease.pool_id, registration.agent_id \
       FROM leases AS lease \
       JOIN agent_registrations AS registration ON registration.id = lease.registration_id \
       WHERE lease.job_id = job.id ORDER BY lease.leased_at DESC, lease.id DESC LIMIT 1\
     ) AS assignment ON TRUE \
     LEFT JOIN job_completions AS completion ON completion.job_id = job.id \
     LEFT JOIN LATERAL (SELECT MAX(sequence) AS sequence FROM job_events WHERE job_id = job.id) AS event_cursor ON TRUE \
     WHERE build.metadata_visible AND ($1::uuid IS NULL OR job.id = $1) \
       AND ($2::uuid IS NULL OR job.attempt_id = $2) \
     ORDER BY job.id",
  )
  .bind(job_id)
  .bind(attempt_id)
  .fetch_all(pool)
  .await
  .map_err(unavailable)
}

async fn load_dependencies(pool: &PgPool, job_ids: &[Uuid]) -> Result<BTreeMap<Uuid, Vec<JobId>>, StoreError> {
  if job_ids.is_empty() {
    return Ok(BTreeMap::new());
  }
  let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
    "SELECT job_id, dependency_job_id FROM job_dependencies \
     WHERE job_id = ANY($1::uuid[]) ORDER BY job_id, dependency_job_id",
  )
  .bind(job_ids)
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;
  let mut result = BTreeMap::new();
  for (job_id, dependency_id) in rows {
    result
      .entry(job_id)
      .or_insert_with(Vec::new)
      .push(JobId::from_uuid(dependency_id).map_err(|_| StoreError::Unavailable)?);
  }
  Ok(result)
}

#[derive(FromRow)]
struct BuildRow {
  id: Uuid,
  project_id: Uuid,
  build_configuration_id: Uuid,
  build_configuration_version: i64,
  pipeline_id: Uuid,
  pipeline_version: i64,
  repository_id: Uuid,
  repository_version: i64,
  immutable_revision: String,
  input_snapshot: Json<Value>,
  effective_policy_snapshot: Json<Value>,
  metadata_retention_millis: i64,
  log_retention_millis: i64,
  artifact_retention_millis: i64,
  report_retention_millis: i64,
  project_job_concurrency_limit: i64,
  priority: i64,
  state: String,
  version: i64,
  created_at_millis: i64,
  updated_at_millis: i64,
  occurrence_id: Uuid,
  trigger_id: Uuid,
  trigger_version: i64,
  trigger_configuration_id: Uuid,
  trigger_configuration_version: i64,
  deduplication_identity: String,
  cause: Json<TriggerCause>,
  causality: Json<TriggerCausality>,
  provider_metadata: Json<TriggerMetadata>,
  source_time_millis: i64,
  trigger_state: String,
  trigger_created_at_millis: i64,
  trigger_updated_at_millis: i64,
}

impl TryFrom<BuildRow> for BuildRecord {
  type Error = StoreError;

  fn try_from(row: BuildRow) -> Result<Self, Self::Error> {
    let occurrence_id = id!(TriggerOccurrenceId, row.occurrence_id)?;
    let trigger = NormalizedTriggerOccurrence {
      id: occurrence_id,
      trigger: TriggerDefinitionRef {
        id: id!(TriggerId, row.trigger_id)?,
        version: positive!(TriggerVersion, row.trigger_version)?,
      },
      target: TriggerTarget {
        configuration_id: id!(BuildConfigurationId, row.trigger_configuration_id)?,
        configuration_version: positive!(BuildConfigurationVersion, row.trigger_configuration_version)?,
      },
      deduplication_identity: TriggerIdentity::new(row.deduplication_identity).map_err(|_| StoreError::Unavailable)?,
      cause: row.cause.0,
      causality: row.causality.0,
      provider_metadata: row.provider_metadata.0,
      source_time: timestamp(row.source_time_millis)?,
    };
    trigger.validate().map_err(|_| StoreError::Unavailable)?;
    Ok(Self {
      build: ImmutableBuildInput {
        id: id!(BuildId, row.id)?,
        project_id: id!(ProjectId, row.project_id)?,
        configuration_id: id!(BuildConfigurationId, row.build_configuration_id)?,
        configuration_version: positive!(BuildConfigurationVersion, row.build_configuration_version)?,
        pipeline_id: id!(PipelineId, row.pipeline_id)?,
        pipeline_version: positive!(PipelineVersion, row.pipeline_version)?,
        repository_id: id!(RepositoryId, row.repository_id)?,
        repository_version: positive!(RepositoryVersion, row.repository_version)?,
        immutable_revision: ImmutableRevision::new(row.immutable_revision).map_err(|_| StoreError::Unavailable)?,
        input_snapshot: row.input_snapshot.0,
        effective_policy_snapshot: row.effective_policy_snapshot.0,
        retention: BuildRetentionDeadlines {
          metadata: timestamp(row.metadata_retention_millis)?,
          logs: timestamp(row.log_retention_millis)?,
          artifacts: timestamp(row.artifact_retention_millis)?,
          reports: timestamp(row.report_retention_millis)?,
        },
        project_job_concurrency_limit: u32::try_from(row.project_job_concurrency_limit)
          .map_err(|_| StoreError::Unavailable)?,
        priority: row.priority,
      },
      state: parse_build_state(&row.state)?,
      version: positive!(BuildVersion, row.version)?,
      trigger,
      trigger_state: trigger_state(&row.trigger_state)?,
      trigger_created_at: timestamp(row.trigger_created_at_millis)?,
      trigger_updated_at: timestamp(row.trigger_updated_at_millis)?,
      created_at: timestamp(row.created_at_millis)?,
      updated_at: timestamp(row.updated_at_millis)?,
    })
  }
}

#[derive(FromRow)]
struct AttemptRow {
  id: Uuid,
  build_id: Uuid,
  attempt_number: i64,
  retry_of_attempt_id: Option<Uuid>,
  state: String,
  version: i64,
  created_at_millis: i64,
  updated_at_millis: i64,
}

impl AttemptRow {
  fn into_record(self, jobs: Vec<JobRecord>) -> Result<AttemptRecord, StoreError> {
    Ok(AttemptRecord {
      id: id!(AttemptId, self.id)?,
      build_id: id!(BuildId, self.build_id)?,
      number: positive!(AttemptNumber, self.attempt_number)?,
      retry_of_attempt_id: self
        .retry_of_attempt_id
        .map(|value| id!(AttemptId, value))
        .transpose()?,
      state: parse_attempt_state(&self.state)?,
      version: positive!(AttemptVersion, self.version)?,
      created_at: timestamp(self.created_at_millis)?,
      updated_at: timestamp(self.updated_at_millis)?,
      jobs,
    })
  }
}

#[derive(FromRow)]
struct JobRow {
  id: Uuid,
  attempt_id: Uuid,
  pipeline_node_id: String,
  state: String,
  allowed_pool_ids: Vec<Uuid>,
  requirements: Json<JobRequirements>,
  version: i64,
  job_spec_template: Json<JobSpecTemplate>,
  dependency_policy: Json<DependencyPolicy>,
  created_at_millis: i64,
  updated_at_millis: i64,
  queue_priority: Option<i64>,
  enqueued_at_millis: Option<i64>,
  assigned_pool_id: Option<Uuid>,
  assigned_agent_id: Option<Uuid>,
  completion_kind: Option<String>,
  failure_class: Option<String>,
  completed_at_millis: Option<i64>,
  event_cursor: i64,
}

impl JobRow {
  fn into_record(self, dependencies: Vec<JobId>) -> Result<JobRecord, StoreError> {
    let state = parse_job_state(&self.state)?;
    let updated_at = timestamp(self.updated_at_millis)?;
    let queue = self
      .queue_priority
      .zip(self.enqueued_at_millis)
      .map(|(priority, enqueued)| {
        Ok(JobQueueRecord {
          priority,
          enqueued_at: timestamp(enqueued)?,
        })
      })
      .transpose()?;
    if self.queue_priority.is_some() != self.enqueued_at_millis.is_some() {
      return Err(StoreError::Unavailable);
    }
    let assignment = self
      .assigned_pool_id
      .zip(self.assigned_agent_id)
      .map(|(pool_id, agent_id)| {
        Ok(JobAssignmentRecord {
          pool_id: id!(PoolId, pool_id)?,
          agent_id: id!(AgentId, agent_id)?,
        })
      })
      .transpose()?;
    if self.assigned_pool_id.is_some() != self.assigned_agent_id.is_some() {
      return Err(StoreError::Unavailable);
    }
    let completed_at = self
      .completed_at_millis
      .map(timestamp)
      .transpose()?
      .or_else(|| state.is_terminal().then_some(updated_at));
    let terminal = terminal(
      state,
      self.completion_kind.as_deref(),
      self.failure_class.as_deref(),
      completed_at,
    )?;
    Ok(JobRecord {
      attempt_id: id!(AttemptId, self.attempt_id)?,
      job: MaterializedJob {
        id: id!(JobId, self.id)?,
        pipeline_node_id: PipelineNodeId::new(self.pipeline_node_id).map_err(|_| StoreError::Unavailable)?,
        dependencies,
        dependency_policy: self.dependency_policy.0,
        allowed_pools: self
          .allowed_pool_ids
          .into_iter()
          .map(|value| id!(PoolId, value))
          .collect::<Result<_, _>>()?,
        requirements: self.requirements.0,
        job_spec_template: self.job_spec_template.0,
      },
      state,
      version: positive!(JobVersion, self.version)?,
      created_at: timestamp(self.created_at_millis)?,
      updated_at,
      queue,
      assignment,
      terminal,
      event_cursor: u64::try_from(self.event_cursor).map_err(|_| StoreError::Unavailable)?,
    })
  }
}

fn terminal(
  state: JobState,
  kind: Option<&str>,
  failure: Option<&str>,
  completed_at: Option<Timestamp>,
) -> Result<Option<JobTerminalRecord>, StoreError> {
  if !state.is_terminal() {
    return (kind.is_none() && failure.is_none() && completed_at.is_none())
      .then_some(None)
      .ok_or(StoreError::Unavailable);
  }
  let completed_at = completed_at.ok_or(StoreError::Unavailable)?;
  let failure_class = match failure {
    None => None,
    Some("execution") => Some(JobFailureClass::Execution),
    Some("infrastructure") => Some(JobFailureClass::Infrastructure),
    Some(_) => return Err(StoreError::Unavailable),
  };
  let valid = match state {
    JobState::Succeeded => kind == Some("succeeded") && failure_class.is_none(),
    JobState::Failed => kind == Some("failed") && failure_class.is_some(),
    JobState::Cancelled => kind.is_none() || kind == Some("cancelled"),
    JobState::Skipped => kind.is_none(),
    _ => false,
  };
  valid
    .then_some(Some(JobTerminalRecord {
      state,
      failure_class,
      completed_at,
    }))
    .ok_or(StoreError::Unavailable)
}

fn trigger_state(value: &str) -> Result<TriggerOccurrenceState, StoreError> {
  match value {
    "pending" => Ok(TriggerOccurrenceState::Pending),
    "evaluating" => Ok(TriggerOccurrenceState::Evaluating),
    "deferred" => Ok(TriggerOccurrenceState::Deferred),
    "accepted" => Ok(TriggerOccurrenceState::Accepted),
    "suppressed" => Ok(TriggerOccurrenceState::Suppressed),
    "rejected" => Ok(TriggerOccurrenceState::Rejected),
    _ => Err(StoreError::Unavailable),
  }
}

fn timestamp(value: i64) -> Result<Timestamp, StoreError> {
  Timestamp::from_unix_millis(value).map_err(|_| StoreError::Unavailable)
}
