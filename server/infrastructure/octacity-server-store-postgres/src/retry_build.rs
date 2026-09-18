use octacity_server_domain::{AttemptId, AttemptNumber, BuildId, EntityKind, JobId, PipelineNodeId, PoolId};
use octacity_server_job::JobSpecSigner;
use octacity_server_orchestrator::{RetryDecisionError, decide_retry};
use octacity_server_pipeline::DependencyPolicy;
use octacity_server_store::{
  MaterializedJob, MutationDisposition, RetryBuild, RetryDisposition, StoreError, retry_graph_is_equivalent,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool, Postgres, Transaction, types::Json};
use uuid::Uuid;

use crate::{
  database::{number, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
  state::{build_state, parse_attempt_state, parse_build_state},
};

pub(crate) async fn execute(
  pool: &PgPool,
  signer: &JobSpecSigner,
  request: RetryBuild,
) -> Result<RetryDisposition, StoreError> {
  request.validate()?;
  let identity = MutationIdentity::new(
    MutationKind::RetryBuild,
    request.idempotency_key.to_string(),
    request.requested_at,
    EntityKind::Build,
    &RequestFingerprint::from(&request),
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay(outcome),
  };

  let current_build_state: String = sqlx::query_scalar("SELECT state FROM builds WHERE id = $1 FOR UPDATE")
    .bind(request.build_id.as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(unavailable)?
    .ok_or(StoreError::NotFound {
      entity: EntityKind::Build,
    })?;
  let source: SourceAttempt = sqlx::query_as(
    "SELECT id, attempt_number, state FROM attempts \
     WHERE build_id = $1 ORDER BY attempt_number DESC LIMIT 1 FOR UPDATE",
  )
  .bind(request.build_id.as_uuid())
  .fetch_optional(&mut *transaction)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Attempt,
  })?;
  let latest_number = u64::try_from(source.attempt_number)
    .ok()
    .and_then(|number| AttemptNumber::new(number).ok())
    .ok_or(StoreError::Unavailable)?;
  let retry_decision = decide_retry(
    parse_build_state(&current_build_state)?,
    parse_attempt_state(&source.state)?,
    latest_number,
  )
  .map_err(|error| match error {
    RetryDecisionError::BuildNotFailed => StoreError::Conflict {
      entity: EntityKind::Build,
    },
    RetryDecisionError::AttemptNotFailed => StoreError::Conflict {
      entity: EntityKind::Attempt,
    },
    RetryDecisionError::AttemptNumberOverflow => StoreError::Unavailable,
  })?;
  let allocated = retry_decision.attempt_number();
  if request.attempt_number != allocated {
    return Err(StoreError::Conflict {
      entity: EntityKind::Attempt,
    });
  }
  verify_immutable_graph(&mut transaction, source.id, &request.jobs).await?;

  crate::attempt_materialization::insert_attempt(
    &mut transaction,
    request.attempt_id,
    request.build_id,
    number(allocated.get(), octacity_server_store::StoreOperation::RetryBuild)?,
    Some(source.id),
    retry_decision.attempt_state(),
    request.requested_at,
  )
  .await?;
  crate::attempt_materialization::insert_jobs(
    &mut transaction,
    request.attempt_id,
    &request.jobs,
    request.requested_at,
  )
  .await?;
  crate::attempt_materialization::insert_dependencies(&mut transaction, request.attempt_id, &request.jobs).await?;
  let ready_jobs = crate::attempt_materialization::enqueue_roots(
    &mut transaction,
    signer,
    request.attempt_id,
    &request.jobs,
    request.requested_at,
  )
  .await?;
  sqlx::query(
    "UPDATE builds SET state = $1, version = version + 1, \
       updated_at = to_timestamp($2::double precision / 1000.0) WHERE id = $3",
  )
  .bind(build_state(retry_decision.build_state()))
  .bind(request.requested_at.unix_millis())
  .bind(request.build_id.as_uuid())
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?;

  let outcome = RetryDisposition {
    disposition: MutationDisposition::Applied,
    build_id: request.build_id,
    source_attempt_id: AttemptId::from_uuid(source.id).map_err(|_| StoreError::Unavailable)?,
    attempt_id: request.attempt_id,
    attempt_number: allocated,
    ready_jobs,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    facts(&outcome),
    encode_outcome(&StoredOutcome::from(&outcome))?,
  )
  .await?;
  Ok(outcome)
}

#[derive(Serialize)]
struct RequestFingerprint<'a> {
  build_id: BuildId,
  attempt_id: AttemptId,
  attempt_number: AttemptNumber,
  jobs: &'a [MaterializedJob],
}

impl<'a> From<&'a RetryBuild> for RequestFingerprint<'a> {
  fn from(request: &'a RetryBuild) -> Self {
    Self {
      build_id: request.build_id,
      attempt_id: request.attempt_id,
      attempt_number: request.attempt_number,
      jobs: &request.jobs,
    }
  }
}

#[derive(FromRow)]
struct SourceAttempt {
  id: Uuid,
  attempt_number: i64,
  state: String,
}

#[derive(FromRow)]
struct SourceJobRow {
  job_id: Uuid,
  pipeline_node_id: String,
  allowed_pool_ids: Vec<Uuid>,
  requirements: Json<octacity_server_job::JobRequirements>,
  job_spec_template: Json<octacity_server_job::JobSpecTemplate>,
  dependency_job_id: Option<Uuid>,
  dependency_policy: Json<DependencyPolicy>,
}

async fn verify_immutable_graph(
  transaction: &mut Transaction<'_, Postgres>,
  source_attempt_id: Uuid,
  candidates: &[MaterializedJob],
) -> Result<(), StoreError> {
  let rows: Vec<SourceJobRow> = sqlx::query_as(
    "SELECT job.id AS job_id, job.pipeline_node_id, job.allowed_pool_ids, job.requirements, \
            job.job_spec_template, dependency.dependency_job_id, job.dependency_policy \
     FROM jobs AS job \
     LEFT JOIN job_dependencies AS dependency ON dependency.job_id = job.id \
     WHERE job.attempt_id = $1 \
     ORDER BY job.id, dependency.dependency_job_id NULLS FIRST \
     FOR UPDATE OF job",
  )
  .bind(source_attempt_id)
  .fetch_all(&mut **transaction)
  .await
  .map_err(unavailable)?;
  let mut source = Vec::<MaterializedJob>::new();
  for row in rows {
    let job_id = JobId::from_uuid(row.job_id).map_err(|_| StoreError::Unavailable)?;
    if let Some(existing) = source.iter_mut().find(|job| job.id == job_id) {
      if existing.pipeline_node_id.as_str() != row.pipeline_node_id
        || existing.requirements != row.requirements.0
        || existing.dependency_policy != row.dependency_policy.0
        || existing.job_spec_template != row.job_spec_template.0
      {
        return Err(StoreError::Unavailable);
      }
      if let Some(dependency) = row.dependency_job_id {
        existing
          .dependencies
          .push(JobId::from_uuid(dependency).map_err(|_| StoreError::Unavailable)?);
      }
      continue;
    }
    source.push(MaterializedJob {
      id: job_id,
      pipeline_node_id: PipelineNodeId::new(row.pipeline_node_id).map_err(|_| StoreError::Unavailable)?,
      dependencies: row
        .dependency_job_id
        .map(|id| JobId::from_uuid(id).map_err(|_| StoreError::Unavailable))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?,
      dependency_policy: row.dependency_policy.0,
      allowed_pools: row
        .allowed_pool_ids
        .into_iter()
        .map(|id| PoolId::from_uuid(id).map_err(|_| StoreError::Unavailable))
        .collect::<Result<Vec<_>, _>>()?,
      requirements: row.requirements.0,
      job_spec_template: row.job_spec_template.0,
    });
  }
  retry_graph_is_equivalent(&source, candidates)
    .then_some(())
    .ok_or(StoreError::Conflict {
      entity: EntityKind::Attempt,
    })
}

#[derive(Deserialize, Serialize)]
struct StoredOutcome {
  build_id: BuildId,
  source_attempt_id: AttemptId,
  attempt_id: AttemptId,
  attempt_number: AttemptNumber,
  ready_jobs: Vec<JobId>,
}

impl From<&RetryDisposition> for StoredOutcome {
  fn from(outcome: &RetryDisposition) -> Self {
    Self {
      build_id: outcome.build_id,
      source_attempt_id: outcome.source_attempt_id,
      attempt_id: outcome.attempt_id,
      attempt_number: outcome.attempt_number,
      ready_jobs: outcome.ready_jobs.clone(),
    }
  }
}

fn replay(value: Value) -> Result<RetryDisposition, StoreError> {
  let stored: StoredOutcome = decode_outcome(value)?;
  Ok(RetryDisposition {
    disposition: MutationDisposition::Replayed,
    build_id: stored.build_id,
    source_attempt_id: stored.source_attempt_id,
    attempt_id: stored.attempt_id,
    attempt_number: stored.attempt_number,
    ready_jobs: stored.ready_jobs,
  })
}

fn facts(outcome: &RetryDisposition) -> MutationFacts {
  MutationFacts {
    actor_kind: "unauthenticated_management",
    actor_identity: None,
    target_identity: outcome.build_id.to_string(),
    safe_metadata: json!({
      "attempt_id": outcome.attempt_id,
      "attempt_number": outcome.attempt_number,
      "ready_job_count": outcome.ready_jobs.len(),
      "source_attempt_id": outcome.source_attempt_id,
    }),
    outbox_payload: json!({
      "attempt_id": outcome.attempt_id,
      "attempt_number": outcome.attempt_number,
      "build_id": outcome.build_id,
      "schema_version": 1,
      "source_attempt_id": outcome.source_attempt_id,
    }),
  }
}
