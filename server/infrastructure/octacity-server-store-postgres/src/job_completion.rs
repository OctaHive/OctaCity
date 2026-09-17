use octacity_server_domain::{EntityKind, JobId, Timestamp};
use octacity_server_orchestrator::{DependencyObservation, newly_ready_jobs};
use octacity_server_pipeline::{DependencyOutcome, DependencyPolicy};
use octacity_server_store::{
  CompletionDisposition, EventSequence, JobCompletion, JobCompletionKind, MutationDisposition, StoreError,
  StoreOperation,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool, Postgres, Transaction, types::Json};
use uuid::Uuid;

use crate::{
  database::{classify, number, unavailable},
  lease,
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};

pub(crate) async fn execute(pool: &PgPool, request: JobCompletion) -> Result<CompletionDisposition, StoreError> {
  let digest_input = json!({
    "agent_id": request.lease.agent_id,
    "final_sequence": request.final_sequence.map(EventSequence::get),
    "fence": request.lease.fence.expose(),
    "kind": completion_kind(request.kind),
    "lease_id": request.lease.lease_id,
    "registration_epoch": request.lease.registration_epoch.get(),
  });
  let identity = MutationIdentity::new(
    MutationKind::CompleteJob,
    request.lease.lease_id.to_string(),
    request.completed_at,
    EntityKind::Job,
    &digest_input,
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay(outcome),
  };
  let lease = lease::load(&mut transaction, request.lease, StoreOperation::CompleteJob).await?;
  let required = request.final_sequence.map_or(0, EventSequence::get);
  let required_db = number(required, StoreOperation::CompleteJob)?;
  let kind = completion_kind(request.kind);

  let existing: Option<CompletionRow> = sqlx::query_as(
    "SELECT lease_id, final_sequence, kind, ready_job_ids \
     FROM job_completions WHERE job_id = $1 FOR UPDATE",
  )
  .bind(lease.job_id)
  .fetch_optional(&mut *transaction)
  .await
  .map_err(unavailable)?;
  if let Some(existing) = existing {
    if existing.matches(request, required_db, kind) {
      let ready_jobs = existing
        .ready_job_ids
        .into_iter()
        .map(|id| JobId::from_uuid(id).map_err(|_| StoreError::Unavailable))
        .collect::<Result<_, _>>()?;
      let outcome = CompletionDisposition {
        disposition: MutationDisposition::Replayed,
        job_id: JobId::from_uuid(lease.job_id).map_err(|_| StoreError::Unavailable)?,
        ready_jobs,
      };
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
      entity: EntityKind::Job,
    });
  }

  lease::require_current(&lease, request.lease, request.completed_at)?;
  require_durable_cursor(&mut transaction, lease.job_id, required).await?;
  lock_attempt(&mut transaction, lease.attempt_id).await?;
  persist_completion(&mut transaction, &request, lease.job_id, required_db, kind).await?;
  let ready = ready_dependents(&mut transaction, lease.attempt_id, request.completed_at).await?;
  sqlx::query("UPDATE job_completions SET ready_job_ids = $1 WHERE job_id = $2")
    .bind(&ready)
    .bind(lease.job_id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| classify(error, EntityKind::Job))?;

  let outcome = CompletionDisposition {
    disposition: MutationDisposition::Applied,
    job_id: JobId::from_uuid(lease.job_id).map_err(|_| StoreError::Unavailable)?,
    ready_jobs: ready
      .into_iter()
      .map(|id| JobId::from_uuid(id).map_err(|_| StoreError::Unavailable))
      .collect::<Result<_, _>>()?,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    facts(&request, &outcome),
    encode_outcome(&StoredOutcome::from(&outcome))?,
  )
  .await?;
  Ok(outcome)
}

async fn require_durable_cursor(
  transaction: &mut Transaction<'_, Postgres>,
  job_id: Uuid,
  required: u64,
) -> Result<(), StoreError> {
  let durable_through: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(sequence), 0) FROM job_events WHERE job_id = $1")
    .bind(job_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(unavailable)?;
  let durable_through = u64::try_from(durable_through).map_err(|_| StoreError::Unavailable)?;
  if required > durable_through {
    return Err(StoreError::EventsMissing {
      job: JobId::from_uuid(job_id).map_err(|_| StoreError::Unavailable)?,
      durable_through,
      required,
    });
  }
  if required < durable_through {
    return Err(StoreError::Conflict {
      entity: EntityKind::Job,
    });
  }
  Ok(())
}

async fn persist_completion(
  transaction: &mut Transaction<'_, Postgres>,
  request: &JobCompletion,
  job_id: Uuid,
  required: i64,
  kind: &str,
) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO job_completions (job_id, lease_id, final_sequence, kind, ready_job_ids, completed_at) \
     VALUES ($1, $2, $3, $4, '{}', to_timestamp($5::double precision / 1000.0))",
  )
  .bind(job_id)
  .bind(request.lease.lease_id.as_uuid())
  .bind(required)
  .bind(kind)
  .bind(request.completed_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Job))?;
  sqlx::query(
    "UPDATE leases SET state = 'completed', version = version + 1, \
       completed_at = to_timestamp($1::double precision / 1000.0) WHERE id = $2",
  )
  .bind(request.completed_at.unix_millis())
  .bind(request.lease.lease_id.as_uuid())
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Lease))?;
  sqlx::query(
    "UPDATE jobs SET state = $1, version = version + 1, \
       updated_at = to_timestamp($2::double precision / 1000.0) WHERE id = $3",
  )
  .bind(kind)
  .bind(request.completed_at.unix_millis())
  .bind(job_id)
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Job))?;
  Ok(())
}

async fn lock_attempt(transaction: &mut Transaction<'_, Postgres>, attempt_id: Uuid) -> Result<(), StoreError> {
  sqlx::query("SELECT id FROM attempts WHERE id = $1 FOR UPDATE")
    .bind(attempt_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(unavailable)?;
  Ok(())
}

async fn ready_dependents(
  transaction: &mut Transaction<'_, Postgres>,
  attempt_id: Uuid,
  ready_at: Timestamp,
) -> Result<Vec<Uuid>, StoreError> {
  let rows: Vec<DependencyRow> = sqlx::query_as(
    "SELECT job.id AS job_id, dependency.dependency_policy, completion.kind AS completion_kind \
     FROM jobs AS job \
     JOIN job_dependencies AS dependency ON dependency.job_id = job.id \
     LEFT JOIN job_completions AS completion ON completion.job_id = dependency.dependency_job_id \
     WHERE job.attempt_id = $1 AND job.state = 'blocked' \
     ORDER BY job.id, dependency.dependency_job_id \
     FOR UPDATE OF job",
  )
  .bind(attempt_id)
  .fetch_all(&mut **transaction)
  .await
  .map_err(unavailable)?;

  let observations = rows
    .into_iter()
    .map(|row| {
      Ok(DependencyObservation {
        job_id: JobId::from_uuid(row.job_id).map_err(|_| StoreError::Unavailable)?,
        policy: row.dependency_policy.0,
        outcome: dependency_outcome(row.completion_kind.as_deref())?,
      })
    })
    .collect::<Result<Vec<_>, StoreError>>()?;
  let candidates: Vec<Uuid> = newly_ready_jobs(observations)
    .map_err(|_| StoreError::Unavailable)?
    .into_iter()
    .map(JobId::as_uuid)
    .collect();

  if !candidates.is_empty() {
    let updated = sqlx::query(
      "UPDATE jobs SET state = 'ready', version = version + 1, \
       updated_at = to_timestamp($1::double precision / 1000.0) WHERE id = ANY($2::uuid[])",
    )
    .bind(ready_at.unix_millis())
    .bind(&candidates)
    .execute(&mut **transaction)
    .await
    .map_err(|error| classify(error, EntityKind::Job))?;
    if updated.rows_affected() != candidates.len() as u64 {
      return Err(StoreError::Unavailable);
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
    .bind(&candidates)
    .execute(&mut **transaction)
    .await
    .map_err(|error| classify(error, EntityKind::Job))?;
    if enqueued.rows_affected() != candidates.len() as u64 {
      return Err(StoreError::Unavailable);
    }
  }
  Ok(candidates)
}

#[derive(FromRow)]
struct DependencyRow {
  job_id: Uuid,
  dependency_policy: Json<DependencyPolicy>,
  completion_kind: Option<String>,
}

fn dependency_outcome(kind: Option<&str>) -> Result<DependencyOutcome, StoreError> {
  match kind {
    None => Ok(DependencyOutcome::Pending),
    Some("succeeded") => Ok(DependencyOutcome::Succeeded),
    Some("failed") => Ok(DependencyOutcome::Failed),
    Some("cancelled") => Ok(DependencyOutcome::Cancelled),
    Some(_) => Err(StoreError::Unavailable),
  }
}

#[derive(FromRow)]
struct CompletionRow {
  lease_id: Uuid,
  final_sequence: i64,
  kind: String,
  ready_job_ids: Vec<Uuid>,
}

impl CompletionRow {
  fn matches(&self, request: JobCompletion, final_sequence: i64, kind: &str) -> bool {
    self.lease_id == request.lease.lease_id.as_uuid() && self.final_sequence == final_sequence && self.kind == kind
  }
}

#[derive(Deserialize, Serialize)]
struct StoredOutcome {
  job_id: JobId,
  ready_jobs: Vec<JobId>,
}

impl From<&CompletionDisposition> for StoredOutcome {
  fn from(outcome: &CompletionDisposition) -> Self {
    Self {
      job_id: outcome.job_id,
      ready_jobs: outcome.ready_jobs.clone(),
    }
  }
}

fn replay(value: Value) -> Result<CompletionDisposition, StoreError> {
  let stored: StoredOutcome = decode_outcome(value)?;
  Ok(CompletionDisposition {
    disposition: MutationDisposition::Replayed,
    job_id: stored.job_id,
    ready_jobs: stored.ready_jobs,
  })
}

fn facts(request: &JobCompletion, outcome: &CompletionDisposition) -> MutationFacts {
  MutationFacts {
    actor_kind: "agent",
    actor_identity: Some(request.lease.agent_id.to_string()),
    target_identity: outcome.job_id.to_string(),
    safe_metadata: json!({
      "final_sequence": request.final_sequence.map(EventSequence::get),
      "kind": completion_kind(request.kind),
      "lease_id": request.lease.lease_id,
      "ready_job_count": outcome.ready_jobs.len(),
    }),
    outbox_payload: json!({
      "job_id": outcome.job_id,
      "kind": completion_kind(request.kind),
      "ready_jobs": outcome.ready_jobs,
      "schema_version": 1,
    }),
  }
}

const fn completion_kind(kind: JobCompletionKind) -> &'static str {
  match kind {
    JobCompletionKind::Succeeded => "succeeded",
    JobCompletionKind::Failed => "failed",
    JobCompletionKind::Cancelled => "cancelled",
  }
}
