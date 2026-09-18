use octacity_server_domain::{EntityKind, JobId};
use octacity_server_job::{JobFailureClass, JobSpecSigner};
use octacity_server_orchestrator::{AttemptState, BuildState};
use octacity_server_store::{
  CompletionDisposition, EventSequence, JobCompletion, JobCompletionKind, MutationDisposition, StoreError,
  StoreOperation, complete_job_state,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
  database::{classify, job_ids, number, unavailable},
  lease,
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
  state::{attempt_state, build_state, job_state, parse_attempt_state, parse_build_state, parse_job_state},
};

mod orchestration;

pub(crate) async fn execute(
  pool: &PgPool,
  signer: &JobSpecSigner,
  request: JobCompletion,
) -> Result<CompletionDisposition, StoreError> {
  let digest_input = json!({
    "agent_id": request.lease.agent_id,
    "final_sequence": request.final_sequence.map(EventSequence::get),
    "fence": request.lease.fence.expose(),
    "failure_class": failure_class(request.kind),
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
  let (attempt_id, build_id) = locate_graph(&mut transaction, request.lease.lease_id.as_uuid())
    .await?
    .ok_or(StoreError::Fenced {
      lease: request.lease.lease_id,
    })?;
  lock_build_and_attempt(&mut transaction, build_id, attempt_id).await?;
  let lease = lease::load(&mut transaction, request.lease, StoreOperation::CompleteJob).await?;
  if lease.attempt_id != attempt_id {
    return Err(StoreError::Unavailable);
  }
  let required = request.final_sequence.map_or(0, EventSequence::get);
  let required_db = number(required, StoreOperation::CompleteJob)?;
  let kind = completion_kind(request.kind);

  let existing: Option<CompletionRow> = sqlx::query_as(
    "SELECT lease_id, final_sequence, kind, failure_class, ready_job_ids, skipped_job_ids, attempt_state, build_state \
     FROM job_completions WHERE job_id = $1 FOR UPDATE",
  )
  .bind(lease.job_id)
  .fetch_optional(&mut *transaction)
  .await
  .map_err(unavailable)?;
  if let Some(existing) = existing {
    if existing.matches(request, required_db, kind) {
      let ready_jobs = job_ids(existing.ready_job_ids)?;
      let outcome = CompletionDisposition {
        disposition: MutationDisposition::Replayed,
        job_id: JobId::from_uuid(lease.job_id).map_err(|_| StoreError::Unavailable)?,
        failure_class: parse_failure_class(existing.failure_class.as_deref())?,
        ready_jobs,
        skipped_jobs: job_ids(existing.skipped_job_ids)?,
        attempt_state: parse_attempt_state(&existing.attempt_state)?,
        build_state: parse_build_state(&existing.build_state)?,
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
  persist_terminal_state(&mut transaction, &request, lease.job_id, kind).await?;
  let applied = orchestration::reconcile_and_apply(
    &mut transaction,
    signer,
    lease.attempt_id,
    build_id,
    request.completed_at,
  )
  .await?;
  persist_completion(
    &mut transaction,
    CompletionWrite {
      request: &request,
      job_id: lease.job_id,
      final_sequence: required_db,
      kind,
      ready: &applied.ready_jobs,
      skipped: &applied.skipped_jobs,
      attempt_state: applied.attempt_state,
      build_state: applied.build_state,
    },
  )
  .await?;

  let outcome = CompletionDisposition {
    disposition: MutationDisposition::Applied,
    job_id: JobId::from_uuid(lease.job_id).map_err(|_| StoreError::Unavailable)?,
    failure_class: request.kind.failure_class(),
    ready_jobs: job_ids(applied.ready_jobs)?,
    skipped_jobs: job_ids(applied.skipped_jobs)?,
    attempt_state: applied.attempt_state,
    build_state: applied.build_state,
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

async fn persist_terminal_state(
  transaction: &mut Transaction<'_, Postgres>,
  request: &JobCompletion,
  job_id: Uuid,
  kind: &str,
) -> Result<(), StoreError> {
  let current: String = sqlx::query_scalar("SELECT state FROM jobs WHERE id = $1 FOR UPDATE")
    .bind(job_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(unavailable)?;
  let terminal = complete_job_state(parse_job_state(&current)?, request.kind)?;
  if job_state(terminal) != kind {
    return Err(StoreError::Unavailable);
  }
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
  .bind(job_state(terminal))
  .bind(request.completed_at.unix_millis())
  .bind(job_id)
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Job))?;
  Ok(())
}

async fn locate_graph(
  transaction: &mut Transaction<'_, Postgres>,
  lease_id: Uuid,
) -> Result<Option<(Uuid, Uuid)>, StoreError> {
  sqlx::query_as(
    "SELECT job.attempt_id, attempt.build_id FROM leases AS lease \
     JOIN jobs AS job ON job.id = lease.job_id \
     JOIN attempts AS attempt ON attempt.id = job.attempt_id \
     WHERE lease.id = $1",
  )
  .bind(lease_id)
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)
}

async fn lock_build_and_attempt(
  transaction: &mut Transaction<'_, Postgres>,
  build_id: Uuid,
  attempt_id: Uuid,
) -> Result<(), StoreError> {
  sqlx::query_scalar::<_, Uuid>("SELECT id FROM builds WHERE id = $1 FOR UPDATE")
    .bind(build_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(unavailable)?;
  let locked_build: Uuid = sqlx::query_scalar("SELECT build_id FROM attempts WHERE id = $1 FOR UPDATE")
    .bind(attempt_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(unavailable)?;
  if locked_build != build_id {
    return Err(StoreError::Unavailable);
  }
  Ok(())
}

struct CompletionWrite<'a> {
  request: &'a JobCompletion,
  job_id: Uuid,
  final_sequence: i64,
  kind: &'a str,
  ready: &'a [Uuid],
  skipped: &'a [Uuid],
  attempt_state: AttemptState,
  build_state: BuildState,
}

async fn persist_completion(
  transaction: &mut Transaction<'_, Postgres>,
  write: CompletionWrite<'_>,
) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO job_completions \
       (job_id, lease_id, final_sequence, kind, failure_class, ready_job_ids, skipped_job_ids, attempt_state, \
        build_state, completed_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, to_timestamp($10::double precision / 1000.0))",
  )
  .bind(write.job_id)
  .bind(write.request.lease.lease_id.as_uuid())
  .bind(write.final_sequence)
  .bind(write.kind)
  .bind(failure_class(write.request.kind))
  .bind(write.ready)
  .bind(write.skipped)
  .bind(attempt_state(write.attempt_state))
  .bind(build_state(write.build_state))
  .bind(write.request.completed_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Job))?;
  Ok(())
}

#[derive(FromRow)]
struct CompletionRow {
  lease_id: Uuid,
  final_sequence: i64,
  kind: String,
  failure_class: Option<String>,
  ready_job_ids: Vec<Uuid>,
  skipped_job_ids: Vec<Uuid>,
  attempt_state: String,
  build_state: String,
}

impl CompletionRow {
  fn matches(&self, request: JobCompletion, final_sequence: i64, kind: &str) -> bool {
    self.lease_id == request.lease.lease_id.as_uuid()
      && self.final_sequence == final_sequence
      && self.kind == kind
      && self.failure_class.as_deref() == failure_class(request.kind)
  }
}

#[derive(Deserialize, Serialize)]
struct StoredOutcome {
  job_id: JobId,
  failure_class: Option<JobFailureClass>,
  ready_jobs: Vec<JobId>,
  skipped_jobs: Vec<JobId>,
  attempt_state: AttemptState,
  build_state: BuildState,
}

impl From<&CompletionDisposition> for StoredOutcome {
  fn from(outcome: &CompletionDisposition) -> Self {
    Self {
      job_id: outcome.job_id,
      failure_class: outcome.failure_class,
      ready_jobs: outcome.ready_jobs.clone(),
      skipped_jobs: outcome.skipped_jobs.clone(),
      attempt_state: outcome.attempt_state,
      build_state: outcome.build_state,
    }
  }
}

fn replay(value: Value) -> Result<CompletionDisposition, StoreError> {
  let stored: StoredOutcome = decode_outcome(value)?;
  Ok(CompletionDisposition {
    disposition: MutationDisposition::Replayed,
    job_id: stored.job_id,
    failure_class: stored.failure_class,
    ready_jobs: stored.ready_jobs,
    skipped_jobs: stored.skipped_jobs,
    attempt_state: stored.attempt_state,
    build_state: stored.build_state,
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
      "failure_class": failure_class(request.kind),
      "lease_id": request.lease.lease_id,
      "ready_job_count": outcome.ready_jobs.len(),
      "skipped_job_count": outcome.skipped_jobs.len(),
      "attempt_state": attempt_state(outcome.attempt_state),
      "build_state": build_state(outcome.build_state),
    }),
    outbox_payload: json!({
      "job_id": outcome.job_id,
      "kind": completion_kind(request.kind),
      "failure_class": failure_class(request.kind),
      "ready_jobs": outcome.ready_jobs,
      "skipped_jobs": outcome.skipped_jobs,
      "attempt_state": attempt_state(outcome.attempt_state),
      "build_state": build_state(outcome.build_state),
      "schema_version": 2,
    }),
  }
}

const fn completion_kind(kind: JobCompletionKind) -> &'static str {
  match kind {
    JobCompletionKind::Succeeded => "succeeded",
    JobCompletionKind::Failed(_) => "failed",
    JobCompletionKind::Cancelled => "cancelled",
  }
}

const fn failure_class(kind: JobCompletionKind) -> Option<&'static str> {
  match kind {
    JobCompletionKind::Failed(JobFailureClass::Execution) => Some("execution"),
    JobCompletionKind::Failed(JobFailureClass::Infrastructure) => Some("infrastructure"),
    JobCompletionKind::Succeeded | JobCompletionKind::Cancelled => None,
  }
}

fn parse_failure_class(value: Option<&str>) -> Result<Option<JobFailureClass>, StoreError> {
  match value {
    None => Ok(None),
    Some("execution") => Ok(Some(JobFailureClass::Execution)),
    Some("infrastructure") => Ok(Some(JobFailureClass::Infrastructure)),
    Some(_) => Err(StoreError::Unavailable),
  }
}
