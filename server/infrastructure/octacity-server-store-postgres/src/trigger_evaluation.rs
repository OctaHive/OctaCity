use octacity_server_domain::{EntityKind, ImmutableRevision, Timestamp, TriggerOccurrenceId};
use octacity_server_store::{
  ClaimTriggerEvaluations, CompleteTriggerEvaluation, FailTriggerEvaluation, RecordTriggerEvaluationRevision,
  ReserveTriggerEvaluation, StoreError, TriggerEvaluationClaim, TriggerEvaluationReservation,
};
use serde_json::Value;
use sqlx::{FromRow, types::Json};

use crate::database::unavailable;

#[derive(FromRow)]
struct WorkRow {
  occurrence_id: uuid::Uuid,
  intent_digest: Vec<u8>,
  payload: Json<Value>,
  resolved_revision: Option<String>,
  state: String,
  attempts: i32,
  next_attempt_at_millis: Option<i64>,
  claim_owner: Option<String>,
  claim_expires_at_millis: Option<i64>,
}

pub(crate) async fn reserve(
  pool: &sqlx::PgPool,
  request: ReserveTriggerEvaluation,
) -> Result<TriggerEvaluationReservation, StoreError> {
  request.validate()?;
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let inserted = sqlx::query(
    "INSERT INTO trigger_evaluation_work \
       (occurrence_id, intent_digest, payload, state, attempts, next_attempt_at, claim_owner, claim_expires_at) \
     VALUES ($1, $2, $3, 'pending', 1, to_timestamp($4::double precision / 1000.0), $5, \
       to_timestamp($6::double precision / 1000.0)) ON CONFLICT DO NOTHING",
  )
  .bind(request.occurrence_id.as_uuid())
  .bind(request.intent_digest.as_slice())
  .bind(Json(&request.payload))
  .bind(request.requested_at.unix_millis())
  .bind(request.owner.as_str())
  .bind(request.claim_expires_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?
  .rows_affected()
    == 1;
  if inserted {
    transaction.commit().await.map_err(unavailable)?;
    return Ok(TriggerEvaluationReservation::Claimed(TriggerEvaluationClaim {
      occurrence_id: request.occurrence_id,
      payload: request.payload,
      resolved_revision: None,
      attempt: 1,
      owner: request.owner,
      claim_expires_at: request.claim_expires_at,
    }));
  }

  let row = sqlx::query_as::<_, WorkRow>(
    "SELECT occurrence_id, intent_digest, payload, resolved_revision, state, attempts, \
       FLOOR(EXTRACT(EPOCH FROM next_attempt_at) * 1000)::BIGINT AS next_attempt_at_millis, claim_owner, \
       FLOOR(EXTRACT(EPOCH FROM claim_expires_at) * 1000)::BIGINT AS claim_expires_at_millis \
     FROM trigger_evaluation_work WHERE occurrence_id = $1 FOR UPDATE",
  )
  .bind(request.occurrence_id.as_uuid())
  .fetch_one(&mut *transaction)
  .await
  .map_err(unavailable)?;
  if row.intent_digest.as_slice() != request.intent_digest {
    return Err(StoreError::Conflict {
      entity: EntityKind::Trigger,
    });
  }
  let outcome = match row.state.as_str() {
    "completed" => TriggerEvaluationReservation::Completed,
    "dead_letter" => TriggerEvaluationReservation::DeadLetter,
    "pending"
      if row
        .next_attempt_at_millis
        .is_some_and(|due_at| due_at <= request.requested_at.unix_millis())
        && (row.claim_owner.is_none()
          || row
            .claim_expires_at_millis
            .is_some_and(|deadline| deadline <= request.requested_at.unix_millis())) =>
    {
      let attempts: i32 = sqlx::query_scalar(
        "UPDATE trigger_evaluation_work SET attempts = attempts + 1, claim_owner = $1, \
           claim_expires_at = to_timestamp($2::double precision / 1000.0), diagnostic = NULL \
         WHERE occurrence_id = $3 RETURNING attempts",
      )
      .bind(request.owner.as_str())
      .bind(request.claim_expires_at.unix_millis())
      .bind(request.occurrence_id.as_uuid())
      .fetch_one(&mut *transaction)
      .await
      .map_err(unavailable)?;
      TriggerEvaluationReservation::Claimed(TriggerEvaluationClaim {
        occurrence_id: request.occurrence_id,
        payload: row.payload.0,
        resolved_revision: row
          .resolved_revision
          .map(ImmutableRevision::new)
          .transpose()
          .map_err(|_| StoreError::Unavailable)?,
        attempt: u16::try_from(attempts).map_err(|_| StoreError::Unavailable)?,
        owner: request.owner,
        claim_expires_at: request.claim_expires_at,
      })
    }
    "pending" => TriggerEvaluationReservation::Pending,
    _ => return Err(StoreError::Unavailable),
  };
  transaction.commit().await.map_err(unavailable)?;
  Ok(outcome)
}

pub(crate) async fn claim(
  pool: &sqlx::PgPool,
  request: ClaimTriggerEvaluations,
) -> Result<Vec<TriggerEvaluationClaim>, StoreError> {
  let rows = sqlx::query_as::<_, WorkRow>(
    "WITH available AS (\
       SELECT occurrence_id FROM trigger_evaluation_work \
       WHERE state = 'pending' AND next_attempt_at <= to_timestamp($1::double precision / 1000.0) \
         AND (claim_expires_at IS NULL OR claim_expires_at <= to_timestamp($1::double precision / 1000.0)) \
       ORDER BY next_attempt_at, occurrence_id FOR UPDATE SKIP LOCKED LIMIT $2\
     ), claimed AS (\
       UPDATE trigger_evaluation_work AS work SET attempts = attempts + 1, claim_owner = $3, \
         claim_expires_at = to_timestamp($4::double precision / 1000.0), diagnostic = NULL \
       FROM available WHERE work.occurrence_id = available.occurrence_id RETURNING work.*\
     ) SELECT occurrence_id, intent_digest, payload, resolved_revision, state, attempts, \
       FLOOR(EXTRACT(EPOCH FROM next_attempt_at) * 1000)::BIGINT AS next_attempt_at_millis, claim_owner, \
       FLOOR(EXTRACT(EPOCH FROM claim_expires_at) * 1000)::BIGINT AS claim_expires_at_millis FROM claimed \
       ORDER BY occurrence_id",
  )
  .bind(request.observed_at.unix_millis())
  .bind(i64::from(request.limit.get()))
  .bind(request.owner.as_str())
  .bind(request.claim_expires_at.unix_millis())
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;
  rows
    .into_iter()
    .map(|row| {
      Ok(TriggerEvaluationClaim {
        occurrence_id: TriggerOccurrenceId::from_uuid(row.occurrence_id).map_err(|_| StoreError::Unavailable)?,
        payload: row.payload.0,
        resolved_revision: row
          .resolved_revision
          .map(ImmutableRevision::new)
          .transpose()
          .map_err(|_| StoreError::Unavailable)?,
        attempt: u16::try_from(row.attempts).map_err(|_| StoreError::Unavailable)?,
        owner: request.owner.clone(),
        claim_expires_at: request.claim_expires_at,
      })
    })
    .collect()
}

pub(crate) async fn record_revision(
  pool: &sqlx::PgPool,
  request: RecordTriggerEvaluationRevision,
) -> Result<(), StoreError> {
  let result = sqlx::query(
    "UPDATE trigger_evaluation_work SET resolved_revision = $1 \
     WHERE occurrence_id = $2 AND state = 'pending' AND claim_owner = $3 \
       AND claim_expires_at > to_timestamp($4::double precision / 1000.0) \
       AND (resolved_revision IS NULL OR resolved_revision = $1)",
  )
  .bind(request.resolved_revision.as_str())
  .bind(request.occurrence_id.as_uuid())
  .bind(request.owner.as_str())
  .bind(request.recorded_at.unix_millis())
  .execute(pool)
  .await
  .map_err(unavailable)?;
  affected(result.rows_affected())
}

pub(crate) async fn complete(pool: &sqlx::PgPool, request: CompleteTriggerEvaluation) -> Result<(), StoreError> {
  let result = sqlx::query(
    "UPDATE trigger_evaluation_work SET state = 'completed', next_attempt_at = NULL, diagnostic = NULL, \
       completed_at = to_timestamp($1::double precision / 1000.0), claim_owner = NULL, claim_expires_at = NULL \
     WHERE occurrence_id = $2 AND state = 'pending' AND claim_owner = $3 \
       AND claim_expires_at > to_timestamp($1::double precision / 1000.0)",
  )
  .bind(request.completed_at.unix_millis())
  .bind(request.occurrence_id.as_uuid())
  .bind(request.owner.as_str())
  .execute(pool)
  .await
  .map_err(unavailable)?;
  affected(result.rows_affected())
}

pub(crate) async fn fail(pool: &sqlx::PgPool, request: FailTriggerEvaluation) -> Result<(), StoreError> {
  request.validate()?;
  let result = sqlx::query(
    "UPDATE trigger_evaluation_work SET state = CASE WHEN $1::bigint IS NULL THEN 'dead_letter' ELSE 'pending' END, \
       next_attempt_at = CASE WHEN $1::bigint IS NULL THEN NULL ELSE to_timestamp($1::double precision / 1000.0) END, \
       diagnostic = $2, completed_at = CASE WHEN $1::bigint IS NULL \
         THEN to_timestamp($3::double precision / 1000.0) ELSE NULL END, claim_owner = NULL, claim_expires_at = NULL \
     WHERE occurrence_id = $4 AND state = 'pending' AND claim_owner = $5 \
       AND claim_expires_at > to_timestamp($3::double precision / 1000.0)",
  )
  .bind(request.retry_at.map(Timestamp::unix_millis))
  .bind(request.diagnostic)
  .bind(request.failed_at.unix_millis())
  .bind(request.occurrence_id.as_uuid())
  .bind(request.owner.as_str())
  .execute(pool)
  .await
  .map_err(unavailable)?;
  affected(result.rows_affected())
}

fn affected(rows: u64) -> Result<(), StoreError> {
  if rows == 1 {
    Ok(())
  } else {
    Err(StoreError::Conflict {
      entity: EntityKind::Trigger,
    })
  }
}
