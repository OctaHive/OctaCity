use octacity_server_domain::{AttemptId, BuildId, EntityKind, Timestamp};
use octacity_server_domain::{AttemptNumber, JobId};
use octacity_server_job::{JobSpecSigner, JobSpecTemplate, sign_ready_job_spec};
use octacity_server_orchestrator::AttemptState;
use octacity_server_store::{MaterializedJob, StoreError};
use sqlx::{Postgres, QueryBuilder, Transaction, types::Json};
use uuid::Uuid;

use crate::{database::classify, state::attempt_state};

pub(crate) async fn insert_attempt(
  transaction: &mut Transaction<'_, Postgres>,
  attempt_id: AttemptId,
  build_id: BuildId,
  attempt_number: i64,
  retry_of: Option<Uuid>,
  state: AttemptState,
  created_at: Timestamp,
) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO attempts \
       (id, build_id, attempt_number, retry_of_attempt_id, state, version, created_at, updated_at) \
     VALUES ($1, $2, $3, $4, $5, 1, to_timestamp($6::double precision / 1000.0), \
             to_timestamp($6::double precision / 1000.0))",
  )
  .bind(attempt_id.as_uuid())
  .bind(build_id.as_uuid())
  .bind(attempt_number)
  .bind(retry_of)
  .bind(attempt_state(state))
  .bind(created_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Attempt))?;
  Ok(())
}

pub(crate) async fn insert_jobs(
  transaction: &mut Transaction<'_, Postgres>,
  attempt_id: AttemptId,
  jobs: &[MaterializedJob],
  created_at: Timestamp,
) -> Result<(), StoreError> {
  let attempt_id = attempt_id.as_uuid();
  let created_at = created_at.unix_millis();
  let mut query = QueryBuilder::<Postgres>::new(
    "INSERT INTO jobs \
       (id, attempt_id, pipeline_node_id, state, allowed_pool_ids, requirements, job_spec_template, \
        dependency_policy, version, \
        created_at, updated_at) ",
  );
  query.push_values(jobs, |mut values, job| {
    let state = if job.dependencies.is_empty() {
      "ready"
    } else {
      "blocked"
    };
    let pool_ids: Vec<_> = job.allowed_pools.iter().map(|pool| pool.as_uuid()).collect();
    values
      .push_bind(job.id.as_uuid())
      .push_bind(attempt_id)
      .push_bind(job.pipeline_node_id.as_str())
      .push_bind(state)
      .push_bind(pool_ids)
      .push_bind(Json(job.requirements.clone()))
      .push_bind(Json(&job.job_spec_template))
      .push_bind(Json(job.dependency_policy))
      .push_bind(1_i64)
      .push("to_timestamp(")
      .push_bind_unseparated(created_at)
      .push_unseparated("::double precision / 1000.0)")
      .push("to_timestamp(")
      .push_bind_unseparated(created_at)
      .push_unseparated("::double precision / 1000.0)");
  });
  query
    .build()
    .execute(&mut **transaction)
    .await
    .map_err(|error| classify(error, EntityKind::Job))?;
  Ok(())
}

pub(crate) async fn insert_dependencies(
  transaction: &mut Transaction<'_, Postgres>,
  attempt_id: AttemptId,
  jobs: &[MaterializedJob],
) -> Result<(), StoreError> {
  let dependencies: Vec<_> = jobs
    .iter()
    .flat_map(|job| job.dependencies.iter().map(move |dependency| (job, dependency)))
    .collect();
  if dependencies.is_empty() {
    return Ok(());
  }
  let attempt_id = attempt_id.as_uuid();
  let mut query =
    QueryBuilder::<Postgres>::new("INSERT INTO job_dependencies (attempt_id, job_id, dependency_job_id) ");
  query.push_values(dependencies, |mut values, (job, dependency)| {
    values
      .push_bind(attempt_id)
      .push_bind(job.id.as_uuid())
      .push_bind(dependency.as_uuid());
  });
  query
    .build()
    .execute(&mut **transaction)
    .await
    .map_err(|error| classify(error, EntityKind::Job))?;
  Ok(())
}

pub(crate) async fn enqueue_roots(
  transaction: &mut Transaction<'_, Postgres>,
  signer: &JobSpecSigner,
  attempt_id: AttemptId,
  jobs: &[MaterializedJob],
  enqueued_at: Timestamp,
) -> Result<Vec<octacity_server_domain::JobId>, StoreError> {
  let roots: Vec<_> = jobs
    .iter()
    .filter(|job| job.dependencies.is_empty())
    .map(|job| job.id)
    .collect();
  let root_ids: Vec<_> = roots.iter().map(|job| job.as_uuid()).collect();
  sign_ready_jobs(transaction, signer, &root_ids, enqueued_at).await?;
  let inserted = sqlx::query(
    "INSERT INTO ready_queue_entries \
       (job_id, priority, enqueued_at, project_id, build_configuration_id, build_configuration_version, \
        allowed_pool_ids, requirements) \
     SELECT job.id, build.priority, to_timestamp($1::double precision / 1000.0), build.project_id, \
            build.build_configuration_id, build.build_configuration_version, job.allowed_pool_ids, job.requirements \
     FROM jobs AS job \
     JOIN attempts AS attempt ON attempt.id = job.attempt_id \
     JOIN builds AS build ON build.id = attempt.build_id \
     WHERE attempt.id = $2 AND job.id = ANY($3::uuid[]) \
     ORDER BY job.id",
  )
  .bind(enqueued_at.unix_millis())
  .bind(attempt_id.as_uuid())
  .bind(&root_ids)
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Job))?;
  if inserted.rows_affected() != roots.len() as u64 {
    return Err(StoreError::Unavailable);
  }
  Ok(roots)
}

#[derive(sqlx::FromRow)]
struct ReadyJobSpecRow {
  job_id: Uuid,
  attempt_number: i64,
  job_spec_template: Json<JobSpecTemplate>,
}

/// Signs ready Jobs inside the same transaction that makes them queue-visible.
pub(crate) async fn sign_ready_jobs(
  transaction: &mut Transaction<'_, Postgres>,
  signer: &JobSpecSigner,
  job_ids: &[Uuid],
  issued_at: Timestamp,
) -> Result<(), StoreError> {
  if job_ids.is_empty() {
    return Ok(());
  }
  let rows: Vec<ReadyJobSpecRow> = sqlx::query_as(
    "SELECT job.id AS job_id, attempt.attempt_number, job.job_spec_template \
     FROM jobs AS job JOIN attempts AS attempt ON attempt.id = job.attempt_id \
     WHERE job.id = ANY($1::uuid[]) ORDER BY job.id FOR UPDATE OF job",
  )
  .bind(job_ids)
  .fetch_all(&mut **transaction)
  .await
  .map_err(crate::database::unavailable)?;
  if rows.len() != job_ids.len() {
    return Err(StoreError::Unavailable);
  }
  let signed = rows
    .into_iter()
    .map(|row| {
      let attempt_number = u64::try_from(row.attempt_number)
        .ok()
        .and_then(|number| AttemptNumber::new(number).ok())
        .ok_or(StoreError::Unavailable)?;
      let job_id = JobId::from_uuid(row.job_id).map_err(|_| StoreError::Unavailable)?;
      let signed = sign_ready_job_spec(&row.job_spec_template.0, attempt_number, job_id, issued_at, signer)
        .map_err(|_| StoreError::Unavailable)?;
      let encoded = serde_json::to_value(signed).map_err(|_| StoreError::Unavailable)?;
      Ok((row.job_id, encoded))
    })
    .collect::<Result<Vec<_>, StoreError>>()?;
  let mut query =
    QueryBuilder::<Postgres>::new("UPDATE jobs AS job SET signed_job_spec = signed.signed_job_spec FROM (");
  query.push_values(signed.iter(), |mut values, (job_id, spec)| {
    values.push_bind(job_id).push_bind(Json(spec));
  });
  query.push(") AS signed(job_id, signed_job_spec) WHERE job.id = signed.job_id");
  let updated = query
    .build()
    .execute(&mut **transaction)
    .await
    .map_err(|error| classify(error, EntityKind::Job))?;
  if updated.rows_affected() != signed.len() as u64 {
    return Err(StoreError::Unavailable);
  }
  Ok(())
}
