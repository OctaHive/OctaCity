use std::collections::BTreeSet;

use octacity_server_domain::{AttemptId, BuildId, EntityKind, JobId};
use octacity_server_store::{
  AcceptTrigger, AcceptTriggerOutcome, ImmutableBuildInput, MaterializedJob, MutationDisposition, NormalizedTrigger,
  StoreError, StoreOperation,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction, types::Json};

use crate::{
  database::{classify, number, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};

pub(crate) async fn execute(pool: &PgPool, request: AcceptTrigger) -> Result<AcceptTriggerOutcome, StoreError> {
  request.validate()?;
  let fingerprint = RequestFingerprint::from(&request);
  let identity = MutationIdentity::new(
    MutationKind::AcceptTrigger,
    request.trigger.id.to_string(),
    request.accepted_at,
    EntityKind::Trigger,
    &fingerprint,
  )?;
  let trigger_version = number(request.trigger.trigger_version.get(), StoreOperation::AcceptTrigger)?;
  let configuration_version = number(request.build.configuration_version.get(), StoreOperation::AcceptTrigger)?;
  let pipeline_version = number(request.build.pipeline_version.get(), StoreOperation::AcceptTrigger)?;
  let repository_version = number(request.build.repository_version.get(), StoreOperation::AcceptTrigger)?;
  let attempt_number = number(request.attempt_number.get(), StoreOperation::AcceptTrigger)?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay(outcome),
  };

  if let Some(existing) = occurrence_digest(&mut transaction, &request, trigger_version).await? {
    if existing == identity.request_digest {
      let outcome = outcome(&request, MutationDisposition::Replayed);
      crate::mutation::commit(
        transaction,
        &identity,
        facts(&request, &outcome),
        encode_outcome(&StoredOutcome::from(&outcome))?,
      )
      .await?;
      return Ok(outcome);
    }
    return Err(StoreError::Conflict {
      entity: EntityKind::Trigger,
    });
  }

  require_references(
    &mut transaction,
    &request,
    trigger_version,
    configuration_version,
    pipeline_version,
    repository_version,
  )
  .await?;
  insert_occurrence(&mut transaction, &request, trigger_version, &identity.request_digest).await?;
  insert_build(
    &mut transaction,
    &request,
    configuration_version,
    pipeline_version,
    repository_version,
  )
  .await?;
  insert_attempt(&mut transaction, &request, attempt_number).await?;
  insert_jobs(&mut transaction, &request).await?;
  insert_dependencies(&mut transaction, &request).await?;
  enqueue_roots(&mut transaction, &request, configuration_version).await?;

  let outcome = outcome(&request, MutationDisposition::Applied);
  crate::mutation::commit(
    transaction,
    &identity,
    facts(&request, &outcome),
    encode_outcome(&StoredOutcome::from(&outcome))?,
  )
  .await?;
  Ok(outcome)
}

#[derive(Serialize)]
struct RequestFingerprint<'a> {
  trigger: &'a NormalizedTrigger,
  build: &'a ImmutableBuildInput,
  attempt_id: AttemptId,
  attempt_number: octacity_server_domain::AttemptNumber,
  jobs: &'a [MaterializedJob],
}

impl<'a> From<&'a AcceptTrigger> for RequestFingerprint<'a> {
  fn from(request: &'a AcceptTrigger) -> Self {
    Self {
      trigger: &request.trigger,
      build: &request.build,
      attempt_id: request.attempt_id,
      attempt_number: request.attempt_number,
      jobs: &request.jobs,
    }
  }
}

async fn require_references(
  transaction: &mut Transaction<'_, Postgres>,
  request: &AcceptTrigger,
  trigger_version: i64,
  configuration_version: i64,
  pipeline_version: i64,
  repository_version: i64,
) -> Result<(), StoreError> {
  let references_exist: bool = sqlx::query_scalar(
    "SELECT EXISTS (\
       SELECT 1 \
       FROM triggers AS trigger \
       JOIN build_configurations AS configuration \
         ON configuration.id = trigger.build_configuration_id \
        AND configuration.version = trigger.build_configuration_version \
       WHERE trigger.id = $1 AND trigger.version = $2 \
         AND configuration.id = $3 AND configuration.version = $4 \
         AND configuration.project_id = $5 \
         AND configuration.pipeline_id = $6 AND configuration.pipeline_version = $7 \
         AND configuration.repository_id = $8 AND configuration.repository_version = $9 \
         AND trigger.enabled AND configuration.enabled\
     )",
  )
  .bind(request.trigger.trigger_id.as_uuid())
  .bind(trigger_version)
  .bind(request.build.configuration_id.as_uuid())
  .bind(configuration_version)
  .bind(request.build.project_id.as_uuid())
  .bind(request.build.pipeline_id.as_uuid())
  .bind(pipeline_version)
  .bind(request.build.repository_id.as_uuid())
  .bind(repository_version)
  .fetch_one(&mut **transaction)
  .await
  .map_err(unavailable)?;
  if !references_exist {
    return Err(StoreError::NotFound {
      entity: EntityKind::Trigger,
    });
  }

  let allowed_pools: BTreeSet<_> = request
    .jobs
    .iter()
    .flat_map(|job| job.allowed_pools.iter().map(|pool| pool.as_uuid()))
    .collect();
  let pool_ids: Vec<_> = allowed_pools.iter().copied().collect();
  let pool_count: i64 = sqlx::query_scalar("SELECT COUNT(DISTINCT id) FROM pools WHERE id = ANY($1)")
    .bind(&pool_ids)
    .fetch_one(&mut **transaction)
    .await
    .map_err(unavailable)?;
  if usize::try_from(pool_count).ok() != Some(allowed_pools.len()) {
    return Err(StoreError::NotFound {
      entity: EntityKind::Pool,
    });
  }
  Ok(())
}

async fn insert_occurrence(
  transaction: &mut Transaction<'_, Postgres>,
  request: &AcceptTrigger,
  trigger_version: i64,
  request_digest: &[u8; 32],
) -> Result<(), StoreError> {
  let inserted = sqlx::query(
    "INSERT INTO trigger_occurrences \
       (id, trigger_id, trigger_version, deduplication_identity, cause, source_time, state, build_id, \
        request_digest, created_at, updated_at) \
     VALUES ($1, $2, $3, $4, $5, to_timestamp($6::double precision / 1000.0), 'accepted', NULL, $7, \
             to_timestamp($8::double precision / 1000.0), to_timestamp($8::double precision / 1000.0)) \
     ON CONFLICT DO NOTHING",
  )
  .bind(request.trigger.id.as_uuid())
  .bind(request.trigger.trigger_id.as_uuid())
  .bind(trigger_version)
  .bind(request.trigger.identity.as_str())
  .bind(Json(request.trigger.cause.clone()))
  .bind(request.trigger.source_time.unix_millis())
  .bind(request_digest.as_slice())
  .bind(request.accepted_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Trigger))?;
  if inserted.rows_affected() == 1 {
    return Ok(());
  }
  Err(StoreError::Conflict {
    entity: EntityKind::Trigger,
  })
}

async fn insert_build(
  transaction: &mut Transaction<'_, Postgres>,
  request: &AcceptTrigger,
  configuration_version: i64,
  pipeline_version: i64,
  repository_version: i64,
) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO builds \
       (id, project_id, build_configuration_id, build_configuration_version, pipeline_id, pipeline_version, \
        repository_id, repository_version, trigger_occurrence_id, immutable_revision, input_snapshot, \
        effective_policy_snapshot, priority, state, version, created_at, updated_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, 'running', 1, \
             to_timestamp($14::double precision / 1000.0), to_timestamp($14::double precision / 1000.0))",
  )
  .bind(request.build.id.as_uuid())
  .bind(request.build.project_id.as_uuid())
  .bind(request.build.configuration_id.as_uuid())
  .bind(configuration_version)
  .bind(request.build.pipeline_id.as_uuid())
  .bind(pipeline_version)
  .bind(request.build.repository_id.as_uuid())
  .bind(repository_version)
  .bind(request.trigger.id.as_uuid())
  .bind(&request.build.immutable_revision)
  .bind(Json(request.build.input_snapshot.clone()))
  .bind(Json(request.build.effective_policy_snapshot.clone()))
  .bind(request.build.priority)
  .bind(request.accepted_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Build))?;

  sqlx::query(
    "UPDATE trigger_occurrences SET build_id = $1, \
       updated_at = to_timestamp($2::double precision / 1000.0) WHERE id = $3",
  )
  .bind(request.build.id.as_uuid())
  .bind(request.accepted_at.unix_millis())
  .bind(request.trigger.id.as_uuid())
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Trigger))?;
  Ok(())
}

async fn insert_attempt(
  transaction: &mut Transaction<'_, Postgres>,
  request: &AcceptTrigger,
  attempt_number: i64,
) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO attempts (id, build_id, attempt_number, state, version, created_at, updated_at) \
     VALUES ($1, $2, $3, 'running', 1, to_timestamp($4::double precision / 1000.0), \
             to_timestamp($4::double precision / 1000.0))",
  )
  .bind(request.attempt_id.as_uuid())
  .bind(request.build.id.as_uuid())
  .bind(attempt_number)
  .bind(request.accepted_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Attempt))?;
  Ok(())
}

async fn insert_jobs(transaction: &mut Transaction<'_, Postgres>, request: &AcceptTrigger) -> Result<(), StoreError> {
  let attempt_id = request.attempt_id.as_uuid();
  let accepted_at = request.accepted_at.unix_millis();
  let mut query = QueryBuilder::<Postgres>::new(
    "INSERT INTO jobs \
       (id, attempt_id, pipeline_node_id, state, job_snapshot, allowed_pool_ids, requirements, version, \
        created_at, updated_at) ",
  );
  query.push_values(&request.jobs, |mut values, job| {
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
      .push_bind(Json(job.snapshot.clone()))
      .push_bind(pool_ids)
      .push_bind(Json(job.requirements.clone()))
      .push_bind(1_i64)
      .push("to_timestamp(")
      .push_bind_unseparated(accepted_at)
      .push_unseparated("::double precision / 1000.0)")
      .push("to_timestamp(")
      .push_bind_unseparated(accepted_at)
      .push_unseparated("::double precision / 1000.0)");
  });
  query
    .build()
    .execute(&mut **transaction)
    .await
    .map_err(|error| classify(error, EntityKind::Job))?;
  Ok(())
}

async fn insert_dependencies(
  transaction: &mut Transaction<'_, Postgres>,
  request: &AcceptTrigger,
) -> Result<(), StoreError> {
  let dependencies: Vec<_> = request
    .jobs
    .iter()
    .flat_map(|job| job.dependencies.iter().map(move |dependency| (job, dependency)))
    .collect();
  if dependencies.is_empty() {
    return Ok(());
  }
  let attempt_id = request.attempt_id.as_uuid();
  let mut query = QueryBuilder::<Postgres>::new(
    "INSERT INTO job_dependencies (attempt_id, job_id, dependency_job_id, dependency_policy) ",
  );
  query.push_values(dependencies, |mut values, (job, dependency)| {
    values
      .push_bind(attempt_id)
      .push_bind(job.id.as_uuid())
      .push_bind(dependency.as_uuid())
      .push_bind(Json(job.dependency_policy));
  });
  query
    .build()
    .execute(&mut **transaction)
    .await
    .map_err(|error| classify(error, EntityKind::Job))?;
  Ok(())
}

async fn enqueue_roots(
  transaction: &mut Transaction<'_, Postgres>,
  request: &AcceptTrigger,
  configuration_version: i64,
) -> Result<(), StoreError> {
  let roots: Vec<_> = request.jobs.iter().filter(|job| job.dependencies.is_empty()).collect();
  let mut query = QueryBuilder::<Postgres>::new(
    "INSERT INTO ready_queue_entries \
       (job_id, priority, enqueued_at, project_id, build_configuration_id, build_configuration_version, \
        allowed_pool_ids, requirements) ",
  );
  query.push_values(roots, |mut values, job| {
    let pool_ids: Vec<_> = job.allowed_pools.iter().map(|pool| pool.as_uuid()).collect();
    values
      .push_bind(job.id.as_uuid())
      .push_bind(request.build.priority)
      .push("to_timestamp(")
      .push_bind_unseparated(request.accepted_at.unix_millis())
      .push_unseparated("::double precision / 1000.0)")
      .push_bind(request.build.project_id.as_uuid())
      .push_bind(request.build.configuration_id.as_uuid())
      .push_bind(configuration_version)
      .push_bind(pool_ids)
      .push_bind(Json(job.requirements.clone()));
  });
  query
    .build()
    .execute(&mut **transaction)
    .await
    .map_err(|error| classify(error, EntityKind::Job))?;
  Ok(())
}

async fn occurrence_digest(
  transaction: &mut Transaction<'_, Postgres>,
  request: &AcceptTrigger,
  trigger_version: i64,
) -> Result<Option<Vec<u8>>, StoreError> {
  sqlx::query_scalar(
    "SELECT request_digest \
     FROM trigger_occurrences \
     WHERE id = $1 \
        OR (trigger_id = $2 AND trigger_version = $3 AND deduplication_identity = $4) \
     ORDER BY CASE WHEN id = $1 THEN 0 ELSE 1 END \
     LIMIT 1 \
     FOR UPDATE",
  )
  .bind(request.trigger.id.as_uuid())
  .bind(request.trigger.trigger_id.as_uuid())
  .bind(trigger_version)
  .bind(request.trigger.identity.as_str())
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)
}

#[derive(Deserialize, Serialize)]
struct StoredOutcome {
  build_id: BuildId,
  attempt_id: AttemptId,
  ready_jobs: Vec<JobId>,
}

impl From<&AcceptTriggerOutcome> for StoredOutcome {
  fn from(outcome: &AcceptTriggerOutcome) -> Self {
    Self {
      build_id: outcome.build_id,
      attempt_id: outcome.attempt_id,
      ready_jobs: outcome.ready_jobs.clone(),
    }
  }
}

fn replay(value: Value) -> Result<AcceptTriggerOutcome, StoreError> {
  let stored: StoredOutcome = decode_outcome(value)?;
  Ok(AcceptTriggerOutcome {
    disposition: MutationDisposition::Replayed,
    build_id: stored.build_id,
    attempt_id: stored.attempt_id,
    ready_jobs: stored.ready_jobs,
  })
}

fn outcome(request: &AcceptTrigger, disposition: MutationDisposition) -> AcceptTriggerOutcome {
  AcceptTriggerOutcome {
    disposition,
    build_id: request.build.id,
    attempt_id: request.attempt_id,
    ready_jobs: request
      .jobs
      .iter()
      .filter(|job| job.dependencies.is_empty())
      .map(|job| job.id)
      .collect(),
  }
}

fn facts(request: &AcceptTrigger, outcome: &AcceptTriggerOutcome) -> MutationFacts {
  MutationFacts {
    actor_kind: "trigger",
    actor_identity: Some(request.trigger.trigger_id.to_string()),
    target_identity: outcome.build_id.to_string(),
    safe_metadata: json!({
      "attempt_id": outcome.attempt_id,
      "ready_job_count": outcome.ready_jobs.len(),
      "trigger_occurrence_id": request.trigger.id,
    }),
    outbox_payload: json!({
      "attempt_id": outcome.attempt_id,
      "build_id": outcome.build_id,
      "schema_version": 1,
      "trigger_occurrence_id": request.trigger.id,
    }),
  }
}
