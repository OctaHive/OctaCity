use octacity_server_domain::{AttemptId, BuildId, EntityKind, JobId};
use octacity_server_job::JobState;
use octacity_server_orchestrator::{AttemptState, BuildState, CancellationError, cancel_job_states};
use octacity_server_store::{CancelBuild, CancellationDisposition, MutationDisposition, StoreError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
  database::{classify, job_ids, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
  state::{attempt_state, build_state, job_state, parse_attempt_state, parse_build_state, parse_job_state},
};

pub(crate) async fn execute(pool: &PgPool, request: CancelBuild) -> Result<CancellationDisposition, StoreError> {
  let identity = MutationIdentity::new(
    MutationKind::CancelBuild,
    request.idempotency_key.to_string(),
    request.requested_at,
    EntityKind::Build,
    &json!({"build_id": request.build_id}),
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay(outcome),
  };

  let build_state_value: String = sqlx::query_scalar("SELECT state FROM builds WHERE id = $1 FOR UPDATE")
    .bind(request.build_id.as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(unavailable)?
    .ok_or(StoreError::NotFound {
      entity: EntityKind::Build,
    })?;
  if let Some(recorded) = load_recorded(&mut transaction, request.build_id).await? {
    let outcome = recorded.outcome(MutationDisposition::Replayed)?;
    crate::mutation::commit(
      transaction,
      &identity,
      facts(&outcome),
      encode_outcome(&StoredOutcome::from(&outcome))?,
    )
    .await?;
    return Ok(outcome);
  }
  let attempt: AttemptRow = sqlx::query_as(
    "SELECT id, state FROM attempts WHERE build_id = $1 ORDER BY attempt_number DESC LIMIT 1 FOR UPDATE",
  )
  .bind(request.build_id.as_uuid())
  .fetch_optional(&mut *transaction)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Attempt,
  })?;
  // Placement locks a queue row before changing its Job. Cancellation uses the
  // same order so a concurrent claim either commits first and is cancelled as
  // leased work, or observes the queue entry already withdrawn.
  sqlx::query_scalar::<_, Uuid>(
    "SELECT queue.job_id FROM ready_queue_entries AS queue \
     JOIN jobs AS job ON job.id = queue.job_id \
     WHERE job.attempt_id = $1 ORDER BY queue.job_id FOR UPDATE OF queue",
  )
  .bind(attempt.id)
  .fetch_all(&mut *transaction)
  .await
  .map_err(unavailable)?;
  sqlx::query_scalar::<_, Uuid>(
    "SELECT lease.id FROM leases AS lease \
     JOIN jobs AS job ON job.id = lease.job_id \
     WHERE job.attempt_id = $1 AND lease.state IN ('active', 'cancellation_requested', 'drain_requested') \
     ORDER BY lease.id FOR UPDATE OF lease",
  )
  .bind(attempt.id)
  .fetch_all(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let rows: Vec<JobRow> = sqlx::query_as("SELECT id, state FROM jobs WHERE attempt_id = $1 ORDER BY id FOR UPDATE")
    .bind(attempt.id)
    .fetch_all(&mut *transaction)
    .await
    .map_err(unavailable)?;
  let decision = cancel_job_states(
    parse_build_state(&build_state_value)?,
    parse_attempt_state(&attempt.state)?,
    rows
      .iter()
      .map(|row| {
        Ok((
          JobId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?,
          parse_job_state(&row.state)?,
        ))
      })
      .collect::<Result<Vec<_>, StoreError>>()?,
  )
  .map_err(|error| match error {
    CancellationError::BuildNotActive => StoreError::Conflict {
      entity: EntityKind::Build,
    },
    CancellationError::AttemptNotActive => StoreError::Conflict {
      entity: EntityKind::Attempt,
    },
    CancellationError::EmptyAttempt
    | CancellationError::DuplicateJob { .. }
    | CancellationError::InvalidTransition { .. } => StoreError::Unavailable,
  })?;
  let cancelled: Vec<_> = decision
    .transitions()
    .iter()
    .filter_map(|transition| (transition.state() == JobState::Cancelled).then_some(transition.job_id().as_uuid()))
    .collect();
  let cancelling: Vec<_> = decision
    .transitions()
    .iter()
    .filter_map(|transition| (transition.state() == JobState::Cancelling).then_some(transition.job_id().as_uuid()))
    .collect();

  update_jobs(&mut transaction, &cancelled, JobState::Cancelled, request.requested_at).await?;
  update_jobs(
    &mut transaction,
    &cancelling,
    JobState::Cancelling,
    request.requested_at,
  )
  .await?;
  sqlx::query("DELETE FROM ready_queue_entries WHERE job_id = ANY($1::uuid[])")
    .bind(&cancelled)
    .execute(&mut *transaction)
    .await
    .map_err(|error| classify(error, EntityKind::Job))?;
  request_lease_cancellation(&mut transaction, &cancelling).await?;
  persist_aggregate_states(&mut transaction, &request, attempt.id, &decision).await?;
  persist_record(
    &mut transaction,
    &request,
    attempt.id,
    &cancelled,
    &cancelling,
    &decision,
  )
  .await?;

  let outcome = CancellationDisposition {
    disposition: MutationDisposition::Applied,
    build_id: request.build_id,
    attempt_id: AttemptId::from_uuid(attempt.id).map_err(|_| StoreError::Unavailable)?,
    cancelled_jobs: job_ids(cancelled)?,
    cancelling_jobs: job_ids(cancelling)?,
    attempt_state: decision.attempt_state(),
    build_state: decision.build_state(),
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

#[derive(FromRow)]
struct AttemptRow {
  id: Uuid,
  state: String,
}

#[derive(FromRow)]
struct JobRow {
  id: Uuid,
  state: String,
}

async fn update_jobs(
  transaction: &mut Transaction<'_, Postgres>,
  ids: &[Uuid],
  state: JobState,
  requested_at: octacity_server_domain::Timestamp,
) -> Result<(), StoreError> {
  if ids.is_empty() {
    return Ok(());
  }
  let updated = sqlx::query(
    "UPDATE jobs SET state = $1, version = version + 1, \
       updated_at = to_timestamp($2::double precision / 1000.0) WHERE id = ANY($3::uuid[])",
  )
  .bind(job_state(state))
  .bind(requested_at.unix_millis())
  .bind(ids)
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Job))?;
  if updated.rows_affected() != ids.len() as u64 {
    return Err(StoreError::Unavailable);
  }
  Ok(())
}

async fn request_lease_cancellation(
  transaction: &mut Transaction<'_, Postgres>,
  job_ids: &[Uuid],
) -> Result<(), StoreError> {
  if job_ids.is_empty() {
    return Ok(());
  }
  let updated = sqlx::query(
    "UPDATE leases SET state = 'cancellation_requested', version = version + 1 \
     WHERE job_id = ANY($1::uuid[]) AND state IN ('active', 'drain_requested')",
  )
  .bind(job_ids)
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Lease))?;
  if updated.rows_affected() != job_ids.len() as u64 {
    return Err(StoreError::Unavailable);
  }
  Ok(())
}

async fn persist_aggregate_states(
  transaction: &mut Transaction<'_, Postgres>,
  request: &CancelBuild,
  attempt_id: Uuid,
  decision: &octacity_server_orchestrator::CancellationDecision,
) -> Result<(), StoreError> {
  sqlx::query(
    "UPDATE attempts SET state = $1, version = version + 1, \
       updated_at = to_timestamp($2::double precision / 1000.0) WHERE id = $3",
  )
  .bind(attempt_state(decision.attempt_state()))
  .bind(request.requested_at.unix_millis())
  .bind(attempt_id)
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Attempt))?;
  sqlx::query(
    "UPDATE builds SET state = $1, version = version + 1, \
       updated_at = to_timestamp($2::double precision / 1000.0) WHERE id = $3",
  )
  .bind(build_state(decision.build_state()))
  .bind(request.requested_at.unix_millis())
  .bind(request.build_id.as_uuid())
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Build))?;
  Ok(())
}

async fn persist_record(
  transaction: &mut Transaction<'_, Postgres>,
  request: &CancelBuild,
  attempt_id: Uuid,
  cancelled: &[Uuid],
  cancelling: &[Uuid],
  decision: &octacity_server_orchestrator::CancellationDecision,
) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO build_cancellations \
       (build_id, attempt_id, cancelled_job_ids, cancelling_job_ids, attempt_state, build_state, requested_at) \
     VALUES ($1, $2, $3, $4, $5, $6, to_timestamp($7::double precision / 1000.0))",
  )
  .bind(request.build_id.as_uuid())
  .bind(attempt_id)
  .bind(cancelled)
  .bind(cancelling)
  .bind(attempt_state(decision.attempt_state()))
  .bind(build_state(decision.build_state()))
  .bind(request.requested_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Build))?;
  Ok(())
}

#[derive(FromRow)]
struct RecordedCancellation {
  build_id: Uuid,
  attempt_id: Uuid,
  cancelled_job_ids: Vec<Uuid>,
  cancelling_job_ids: Vec<Uuid>,
  attempt_state: String,
  build_state: String,
}

impl RecordedCancellation {
  fn outcome(self, disposition: MutationDisposition) -> Result<CancellationDisposition, StoreError> {
    Ok(CancellationDisposition {
      disposition,
      build_id: BuildId::from_uuid(self.build_id).map_err(|_| StoreError::Unavailable)?,
      attempt_id: AttemptId::from_uuid(self.attempt_id).map_err(|_| StoreError::Unavailable)?,
      cancelled_jobs: job_ids(self.cancelled_job_ids)?,
      cancelling_jobs: job_ids(self.cancelling_job_ids)?,
      attempt_state: parse_attempt_state(&self.attempt_state)?,
      build_state: parse_build_state(&self.build_state)?,
    })
  }
}

async fn load_recorded(
  transaction: &mut Transaction<'_, Postgres>,
  build_id: BuildId,
) -> Result<Option<RecordedCancellation>, StoreError> {
  sqlx::query_as(
    "SELECT build_id, attempt_id, cancelled_job_ids, cancelling_job_ids, attempt_state, build_state \
     FROM build_cancellations WHERE build_id = $1 FOR UPDATE",
  )
  .bind(build_id.as_uuid())
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)
}

#[derive(Deserialize, Serialize)]
struct StoredOutcome {
  build_id: BuildId,
  attempt_id: AttemptId,
  cancelled_jobs: Vec<JobId>,
  cancelling_jobs: Vec<JobId>,
  attempt_state: AttemptState,
  build_state: BuildState,
}

impl From<&CancellationDisposition> for StoredOutcome {
  fn from(outcome: &CancellationDisposition) -> Self {
    Self {
      build_id: outcome.build_id,
      attempt_id: outcome.attempt_id,
      cancelled_jobs: outcome.cancelled_jobs.clone(),
      cancelling_jobs: outcome.cancelling_jobs.clone(),
      attempt_state: outcome.attempt_state,
      build_state: outcome.build_state,
    }
  }
}

fn replay(value: Value) -> Result<CancellationDisposition, StoreError> {
  let stored: StoredOutcome = decode_outcome(value)?;
  Ok(CancellationDisposition {
    disposition: MutationDisposition::Replayed,
    build_id: stored.build_id,
    attempt_id: stored.attempt_id,
    cancelled_jobs: stored.cancelled_jobs,
    cancelling_jobs: stored.cancelling_jobs,
    attempt_state: stored.attempt_state,
    build_state: stored.build_state,
  })
}

fn facts(outcome: &CancellationDisposition) -> MutationFacts {
  MutationFacts {
    actor_kind: "unauthenticated_management",
    actor_identity: None,
    target_identity: outcome.build_id.to_string(),
    safe_metadata: json!({
      "attempt_id": outcome.attempt_id,
      "cancelled_job_count": outcome.cancelled_jobs.len(),
      "cancelling_job_count": outcome.cancelling_jobs.len(),
    }),
    outbox_payload: json!({
      "attempt_id": outcome.attempt_id,
      "build_id": outcome.build_id,
      "cancelled_jobs": outcome.cancelled_jobs,
      "cancelling_jobs": outcome.cancelling_jobs,
      "build_state": crate::state::build_state(outcome.build_state),
      "schema_version": 1,
    }),
  }
}
