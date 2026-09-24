use octacity_server_domain::{EntityKind, Timestamp};
use octacity_server_store::{
  DurableJobEvent, EventSequence, JobEventKind, JobEventPage, ReadJobEvents, StoreError, StoreOperation,
};
use serde_json::Value;
use sqlx::{FromRow, PgPool, types::Json};

use crate::database::{number, unavailable};

pub(crate) async fn read(pool: &PgPool, request: ReadJobEvents) -> Result<JobEventPage, StoreError> {
  request.validate()?;
  let after_sequence = number(request.after_sequence, StoreOperation::ReadJobEvents)?;
  let rows = sqlx::query_as::<_, JobEventReadRow>(
    "WITH current_job AS (
       SELECT job.id, build.logs_visible FROM jobs AS job
       JOIN attempts AS attempt ON attempt.id = job.attempt_id
       JOIN builds AS build ON build.id = attempt.build_id
       WHERE job.id = $1
     ), current_cursor AS (
       SELECT current_job.id AS job_id, COALESCE(MAX(job_events.sequence), 0)::BIGINT AS current_sequence
       FROM current_job LEFT JOIN job_events ON job_events.job_id = current_job.id
       GROUP BY current_job.id
     ), page AS (
       SELECT sequence, event_kind,
              (EXTRACT(EPOCH FROM event_time) * 1000)::BIGINT AS event_time_unix_ms, payload
       FROM job_events
       WHERE job_id = $1 AND sequence > $2
         AND (event_kind NOT IN ('stdout', 'stderr') OR (SELECT logs_visible FROM current_job))
       ORDER BY sequence ASC
       LIMIT $3
     )
     SELECT current_cursor.current_sequence, page.sequence, page.event_kind,
            page.event_time_unix_ms, page.payload
     FROM current_cursor LEFT JOIN page ON TRUE
     ORDER BY page.sequence ASC NULLS LAST",
  )
  .bind(request.job_id.as_uuid())
  .bind(after_sequence)
  .bind(i64::from(request.limit))
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;

  let current_sequence = rows
    .first()
    .map(|row| row.current_sequence)
    .ok_or(StoreError::NotFound {
      entity: EntityKind::Job,
    })?;
  let current_cursor = u64::try_from(current_sequence).map_err(|_| StoreError::Unavailable)?;
  let mut events = Vec::with_capacity(rows.len());
  for row in rows {
    let fields = match (row.sequence, row.event_kind, row.event_time_unix_ms, row.payload) {
      (Some(sequence), Some(kind), Some(occurred_at), Some(payload)) => Some((sequence, kind, occurred_at, payload)),
      (None, None, None, None) => None,
      _ => return Err(StoreError::Unavailable),
    };
    let Some((sequence, kind, occurred_at, Json(payload))) = fields else {
      continue;
    };
    let sequence = u64::try_from(sequence)
      .ok()
      .and_then(|value| EventSequence::new(value).ok())
      .ok_or(StoreError::Unavailable)?;
    let kind = JobEventKind::new(kind).map_err(|_| StoreError::Unavailable)?;
    let occurred_at = Timestamp::from_unix_millis(occurred_at).map_err(|_| StoreError::Unavailable)?;
    events.push(DurableJobEvent::new(sequence, kind, occurred_at, payload).map_err(|_| StoreError::Unavailable)?);
  }
  let cursor = events
    .last()
    .map(DurableJobEvent::sequence)
    .map_or(current_cursor, EventSequence::get);
  Ok(JobEventPage { events, cursor })
}

#[derive(FromRow)]
struct JobEventReadRow {
  current_sequence: i64,
  sequence: Option<i64>,
  event_kind: Option<String>,
  event_time_unix_ms: Option<i64>,
  payload: Option<Json<Value>>,
}
