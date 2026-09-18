use std::collections::BTreeMap;

use octacity_server_domain::{EntityKind, JobId, Timestamp};
use octacity_server_job::JobSpecSigner;
use octacity_server_job::JobState;
use octacity_server_orchestrator::{
  AttemptState, BuildState, JobGraphNode, OrchestrationDecision, reconcile_cancelled_job_graph, reconcile_job_graph,
};
use octacity_server_pipeline::DependencyPolicy;
use octacity_server_store::StoreError;
use sqlx::{FromRow, Postgres, Transaction, types::Json};
use uuid::Uuid;

use crate::{
  database::{classify, unavailable},
  state::{attempt_state, build_state, parse_job_state},
};

pub(super) struct AppliedOrchestration {
  pub(super) ready_jobs: Vec<Uuid>,
  pub(super) skipped_jobs: Vec<Uuid>,
  pub(super) attempt_state: AttemptState,
  pub(super) build_state: BuildState,
}

pub(super) async fn reconcile_and_apply(
  transaction: &mut Transaction<'_, Postgres>,
  signer: &JobSpecSigner,
  attempt_id: Uuid,
  build_id: Uuid,
  transitioned_at: Timestamp,
) -> Result<AppliedOrchestration, StoreError> {
  let cancellation_requested: bool =
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM build_cancellations WHERE build_id = $1)")
      .bind(build_id)
      .fetch_one(&mut **transaction)
      .await
      .map_err(unavailable)?;
  let decision = reconcile_attempt(transaction, attempt_id, cancellation_requested).await?;
  let (ready_jobs, skipped_jobs) = apply_job_transitions(transaction, &decision, transitioned_at).await?;
  crate::attempt_materialization::sign_ready_jobs(transaction, signer, &ready_jobs, transitioned_at).await?;
  enqueue_ready(transaction, &ready_jobs, transitioned_at).await?;
  persist_aggregate_states(transaction, attempt_id, build_id, &decision, transitioned_at).await?;
  Ok(AppliedOrchestration {
    ready_jobs,
    skipped_jobs,
    attempt_state: decision.attempt_state(),
    build_state: decision.build_state(),
  })
}

async fn reconcile_attempt(
  transaction: &mut Transaction<'_, Postgres>,
  attempt_id: Uuid,
  cancellation_requested: bool,
) -> Result<OrchestrationDecision, StoreError> {
  let rows: Vec<GraphRow> = sqlx::query_as(
    "SELECT job.id AS job_id, job.state AS job_state, dependency.dependency_job_id, job.dependency_policy \
     FROM jobs AS job \
     LEFT JOIN job_dependencies AS dependency ON dependency.job_id = job.id \
     WHERE job.attempt_id = $1 \
     ORDER BY job.id, dependency.dependency_job_id NULLS FIRST \
     FOR UPDATE OF job",
  )
  .bind(attempt_id)
  .fetch_all(&mut **transaction)
  .await
  .map_err(unavailable)?;

  let mut builders = BTreeMap::<Uuid, GraphNodeBuilder>::new();
  for row in rows {
    let state = parse_job_state(&row.job_state)?;
    let builder = builders.entry(row.job_id).or_insert_with(|| GraphNodeBuilder {
      state,
      dependencies: Vec::new(),
      dependency_policy: row.dependency_policy.0,
    });
    if builder.state != state || builder.dependency_policy != row.dependency_policy.0 {
      return Err(StoreError::Unavailable);
    }
    if let Some(dependency_id) = row.dependency_job_id {
      builder.dependencies.push(dependency_id);
    }
  }

  let nodes = builders
    .into_iter()
    .map(|(job_id, builder)| {
      JobGraphNode::new(
        JobId::from_uuid(job_id).map_err(|_| StoreError::Unavailable)?,
        builder.state,
        builder
          .dependencies
          .into_iter()
          .map(|dependency| JobId::from_uuid(dependency).map_err(|_| StoreError::Unavailable))
          .collect::<Result<Vec<_>, _>>()?,
        builder.dependency_policy,
      )
      .map_err(|_| StoreError::Unavailable)
    })
    .collect::<Result<Vec<_>, _>>()?;
  if cancellation_requested {
    reconcile_cancelled_job_graph(nodes)
  } else {
    reconcile_job_graph(nodes)
  }
  .map_err(|_| StoreError::Unavailable)
}

#[derive(FromRow)]
struct GraphRow {
  job_id: Uuid,
  job_state: String,
  dependency_job_id: Option<Uuid>,
  dependency_policy: Json<DependencyPolicy>,
}

struct GraphNodeBuilder {
  state: JobState,
  dependencies: Vec<Uuid>,
  dependency_policy: DependencyPolicy,
}

async fn apply_job_transitions(
  transaction: &mut Transaction<'_, Postgres>,
  decision: &OrchestrationDecision,
  transitioned_at: Timestamp,
) -> Result<(Vec<Uuid>, Vec<Uuid>), StoreError> {
  let ready: Vec<_> = decision
    .job_transitions()
    .iter()
    .filter_map(|transition| (transition.state() == JobState::Ready).then_some(transition.job_id().as_uuid()))
    .collect();
  let skipped: Vec<_> = decision
    .job_transitions()
    .iter()
    .filter_map(|transition| (transition.state() == JobState::Skipped).then_some(transition.job_id().as_uuid()))
    .collect();
  update_blocked_jobs(transaction, &ready, "ready", transitioned_at).await?;
  update_blocked_jobs(transaction, &skipped, "skipped", transitioned_at).await?;
  Ok((ready, skipped))
}

async fn update_blocked_jobs(
  transaction: &mut Transaction<'_, Postgres>,
  job_ids: &[Uuid],
  state: &str,
  transitioned_at: Timestamp,
) -> Result<(), StoreError> {
  if job_ids.is_empty() {
    return Ok(());
  }
  let updated = sqlx::query(
    "UPDATE jobs SET state = $1, version = version + 1, \
     updated_at = to_timestamp($2::double precision / 1000.0) \
     WHERE id = ANY($3::uuid[]) AND state = 'blocked'",
  )
  .bind(state)
  .bind(transitioned_at.unix_millis())
  .bind(job_ids)
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Job))?;
  if updated.rows_affected() != job_ids.len() as u64 {
    return Err(StoreError::Unavailable);
  }
  Ok(())
}

async fn enqueue_ready(
  transaction: &mut Transaction<'_, Postgres>,
  ready: &[Uuid],
  ready_at: Timestamp,
) -> Result<(), StoreError> {
  if ready.is_empty() {
    return Ok(());
  }
  let enqueued = sqlx::query(
    "INSERT INTO ready_queue_entries \
       (job_id, priority, enqueued_at, project_id, build_configuration_id, build_configuration_version, \
        allowed_pool_ids, requirements) \
     SELECT job.id, build.priority, to_timestamp($1::double precision / 1000.0), build.project_id, \
            build.build_configuration_id, build.build_configuration_version, job.allowed_pool_ids, job.requirements \
     FROM jobs AS job \
     JOIN attempts AS attempt ON attempt.id = job.attempt_id \
     JOIN builds AS build ON build.id = attempt.build_id \
     WHERE job.id = ANY($2::uuid[]) \
     ORDER BY job.id",
  )
  .bind(ready_at.unix_millis())
  .bind(ready)
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Job))?;
  if enqueued.rows_affected() != ready.len() as u64 {
    return Err(StoreError::Unavailable);
  }
  Ok(())
}

async fn persist_aggregate_states(
  transaction: &mut Transaction<'_, Postgres>,
  attempt_id: Uuid,
  build_id: Uuid,
  decision: &OrchestrationDecision,
  transitioned_at: Timestamp,
) -> Result<(), StoreError> {
  sqlx::query(
    "UPDATE attempts SET state = $1, version = version + 1, \
       updated_at = to_timestamp($2::double precision / 1000.0) \
     WHERE id = $3 AND state <> $1",
  )
  .bind(attempt_state(decision.attempt_state()))
  .bind(transitioned_at.unix_millis())
  .bind(attempt_id)
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Attempt))?;
  sqlx::query(
    "UPDATE builds SET state = $1, version = version + 1, \
       updated_at = to_timestamp($2::double precision / 1000.0) \
     WHERE id = $3 AND state <> $1",
  )
  .bind(build_state(decision.build_state()))
  .bind(transitioned_at.unix_millis())
  .bind(build_id)
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Build))?;
  Ok(())
}
