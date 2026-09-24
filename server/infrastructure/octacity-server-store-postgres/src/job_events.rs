use std::collections::BTreeMap;

use octacity_server_domain::{EntityKind, JobId};
use octacity_server_store::{
  AppendJobEvents, AppendJobEventsOutcome, EventSequence, JobEventAppendPreparation, LeaseAccess, StoreError,
  StoreOperation, start_job_execution, validate_new_log_chunks,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder, types::Json};

use crate::{
  database::{classify, number, unavailable},
  lease,
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
  state::{job_state, parse_job_state},
};

pub(crate) async fn execute(pool: &PgPool, request: AppendJobEvents) -> Result<AppendJobEventsOutcome, StoreError> {
  request.validate()?;
  let first_sequence = request.events[0].sequence();
  let last_sequence = request.events[request.events.len() - 1].sequence();
  let events: Vec<_> = request
    .events
    .iter()
    .map(|event| {
      json!({
        "digest": event.digest().as_bytes(),
        "kind": event.kind().as_str(),
        "occurred_at": event.occurred_at().unix_millis(),
        "payload": event.payload(),
        "sequence": event.sequence().get(),
      })
    })
    .collect();
  let log_chunks: Vec<_> = request
    .log_chunks
    .iter()
    .map(|chunk| {
      json!({
        "byte_length": chunk.byte_length(),
        "chunk_id": chunk.chunk_id(),
        "digest": chunk.digest().as_bytes(),
        "first_sequence": chunk.first_sequence(),
        "last_sequence": chunk.last_sequence(),
        "stream": chunk.stream().as_str(),
        "work_id": chunk.indexing_work_id(),
      })
    })
    .collect();
  let digest_input = json!({
    "agent_id": request.lease.agent_id,
    "events": events,
    "fence": request.lease.fence.expose(),
    "lease_id": request.lease.lease_id,
    "log_chunks": log_chunks,
    "registration_epoch": request.lease.registration_epoch.get(),
  });
  let identity = MutationIdentity::new(
    MutationKind::AppendJobEvents,
    format!(
      "{}:{}-{}",
      request.lease.lease_id,
      first_sequence.get(),
      last_sequence.get()
    ),
    request.accepted_at,
    EntityKind::Job,
    &digest_input,
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay(outcome),
  };
  let lease = lease::load(&mut transaction, request.lease, StoreOperation::AppendJobEvents).await?;
  lease::require_current(&lease, request.lease, request.accepted_at)?;
  let durable_through: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(sequence), 0) FROM job_events WHERE job_id = $1")
    .bind(lease.job_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(unavailable)?;
  let durable_through = u64::try_from(durable_through).map_err(|_| StoreError::Unavailable)?;

  let first = number(request.events[0].sequence().get(), StoreOperation::AppendJobEvents)?;
  let replay_end = request
    .events
    .last()
    .map(|event| event.sequence().get().min(durable_through))
    .unwrap_or(0);
  let replay_end = number(replay_end, StoreOperation::AppendJobEvents)?;
  let durable_digests: BTreeMap<i64, Vec<u8>> = if first <= replay_end {
    sqlx::query_as::<_, EventDigestRow>(
      "SELECT sequence, event_digest FROM job_events WHERE job_id = $1 AND sequence BETWEEN $2 AND $3",
    )
    .bind(lease.job_id)
    .bind(first)
    .bind(replay_end)
    .fetch_all(&mut *transaction)
    .await
    .map_err(unavailable)?
    .into_iter()
    .map(|row| (row.sequence, row.event_digest))
    .collect()
  } else {
    BTreeMap::new()
  };

  let mut expected = durable_through.checked_add(1).ok_or(StoreError::Unavailable)?;
  let mut new_events = Vec::new();
  for event in &request.events {
    let sequence = event.sequence().get();
    if sequence <= durable_through {
      let sequence = number(sequence, StoreOperation::AppendJobEvents)?;
      if durable_digests.get(&sequence).map(Vec::as_slice) != Some(event.digest().as_bytes().as_slice()) {
        return Err(StoreError::Conflict {
          entity: EntityKind::Job,
        });
      }
      continue;
    }
    if sequence != expected {
      let job = JobId::from_uuid(lease.job_id).map_err(|_| StoreError::Unavailable)?;
      return Err(StoreError::EventGap {
        job,
        expected,
        actual: sequence,
      });
    }
    new_events.push((event, number(sequence, StoreOperation::AppendJobEvents)?));
    expected = expected.checked_add(1).ok_or(StoreError::Unavailable)?;
  }
  validate_new_log_chunks(&request.events, durable_through, &request.log_chunks)?;

  if !new_events.is_empty() {
    persist_execution_started(&mut transaction, lease.job_id, request.accepted_at).await?;
    let lease_id = request.lease.lease_id.as_uuid();
    let mut query = QueryBuilder::<Postgres>::new(
      "INSERT INTO job_events \
         (job_id, sequence, lease_id, event_kind, event_time, payload, event_digest, created_at) ",
    );
    query.push_values(new_events.iter().copied(), |mut values, (event, sequence)| {
      values
        .push_bind(lease.job_id)
        .push_bind(sequence)
        .push_bind(lease_id)
        .push_bind(event.kind().as_str())
        .push("to_timestamp(")
        .push_bind_unseparated(event.occurred_at().unix_millis())
        .push_unseparated("::double precision / 1000.0)")
        .push_bind(Json(event.payload().clone()))
        .push_bind(event.digest().as_bytes().to_vec())
        .push("now()");
    });
    query
      .build()
      .execute(&mut *transaction)
      .await
      .map_err(|error| classify(error, EntityKind::Job))?;
  }
  let inserted = new_events.len();

  persist_log_chunks(&mut transaction, &lease, &request).await?;

  let acknowledged = durable_through
    .checked_add(u64::try_from(inserted).map_err(|_| StoreError::Unavailable)?)
    .ok_or(StoreError::Unavailable)?;
  let acknowledged_through = EventSequence::new(acknowledged).map_err(|source| StoreError::InvalidInput {
    operation: StoreOperation::AppendJobEvents,
    source,
  })?;
  let outcome = AppendJobEventsOutcome {
    acknowledged_through,
    inserted,
  };
  let job_id = JobId::from_uuid(lease.job_id).map_err(|_| StoreError::Unavailable)?;
  crate::mutation::commit(
    transaction,
    &identity,
    facts(&request, job_id, &outcome),
    encode_outcome(&StoredOutcome::from(outcome))?,
  )
  .await?;
  Ok(outcome)
}

pub(crate) async fn prepare(
  pool: &PgPool,
  access: LeaseAccess,
  accepted_at: octacity_server_domain::Timestamp,
) -> Result<JobEventAppendPreparation, StoreError> {
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let lease = lease::load(&mut transaction, access, StoreOperation::PrepareJobEventAppend).await?;
  lease::require_current(&lease, access, accepted_at)?;
  let durable_through: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(sequence), 0) FROM job_events WHERE job_id = $1")
    .bind(lease.job_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(unavailable)?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(JobEventAppendPreparation {
    durable_through: u64::try_from(durable_through).map_err(|_| StoreError::Unavailable)?,
  })
}

pub(crate) async fn is_chunk_committed(
  pool: &PgPool,
  chunk_id: octacity_server_domain::LogChunkId,
) -> Result<bool, StoreError> {
  sqlx::query_scalar(
    "SELECT EXISTS(SELECT 1 FROM log_chunk_manifests WHERE id = $1 AND visible AND deleted_at IS NULL)",
  )
  .bind(chunk_id.as_uuid())
  .fetch_one(pool)
  .await
  .map_err(unavailable)
}

async fn persist_log_chunks(
  transaction: &mut sqlx::Transaction<'_, Postgres>,
  lease: &lease::LeaseRow,
  request: &AppendJobEvents,
) -> Result<(), StoreError> {
  if request.log_chunks.is_empty() {
    return Ok(());
  }
  sqlx::query(
    "INSERT INTO log_index_project_positions (project_id, committed_through) VALUES ($1, 0) \
     ON CONFLICT (project_id) DO NOTHING",
  )
  .bind(lease.project_id)
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  let mut position: i64 =
    sqlx::query_scalar("SELECT committed_through FROM log_index_project_positions WHERE project_id = $1 FOR UPDATE")
      .bind(lease.project_id)
      .fetch_one(&mut **transaction)
      .await
      .map_err(unavailable)?;

  for chunk in &request.log_chunks {
    position = position.checked_add(1).ok_or(StoreError::Unavailable)?;
    sqlx::query(
      "INSERT INTO log_chunk_manifests \
         (id, build_id, attempt_id, job_id, stream, first_sequence, last_sequence, object_identity, \
          byte_length, sha256, visible, created_at) \
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, true, \
               to_timestamp($11::double precision / 1000.0))",
    )
    .bind(chunk.chunk_id().as_uuid())
    .bind(lease.build_id)
    .bind(lease.attempt_id)
    .bind(lease.job_id)
    .bind(chunk.stream().as_str())
    .bind(number(chunk.first_sequence(), StoreOperation::AppendJobEvents)?)
    .bind(number(chunk.last_sequence(), StoreOperation::AppendJobEvents)?)
    .bind(chunk.object_identity())
    .bind(number(chunk.byte_length(), StoreOperation::AppendJobEvents)?)
    .bind(chunk.digest().as_bytes().to_vec())
    .bind(request.accepted_at.unix_millis())
    .execute(&mut **transaction)
    .await
    .map_err(|error| classify(error, EntityKind::LogChunk))?;

    sqlx::query(
      "INSERT INTO log_indexing_work \
         (id, project_id, position, build_id, chunk_id, operation, state, attempt_count, available_at, created_at) \
       VALUES ($1, $2, $3, $4, $5, 'index', 'pending', 0, \
               to_timestamp($6::double precision / 1000.0), \
               to_timestamp($6::double precision / 1000.0))",
    )
    .bind(chunk.indexing_work_id().as_uuid())
    .bind(lease.project_id)
    .bind(position)
    .bind(lease.build_id)
    .bind(chunk.chunk_id().as_uuid())
    .bind(request.accepted_at.unix_millis())
    .execute(&mut **transaction)
    .await
    .map_err(|error| classify(error, EntityKind::LogIndexingWork))?;
  }
  let updated = sqlx::query("UPDATE log_index_project_positions SET committed_through = $1 WHERE project_id = $2")
    .bind(position)
    .bind(lease.project_id)
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
  if updated.rows_affected() != 1 {
    return Err(StoreError::Unavailable);
  }
  Ok(())
}

async fn persist_execution_started(
  transaction: &mut sqlx::Transaction<'_, Postgres>,
  job_id: uuid::Uuid,
  started_at: octacity_server_domain::Timestamp,
) -> Result<(), StoreError> {
  let current: String = sqlx::query_scalar("SELECT state FROM jobs WHERE id = $1 FOR UPDATE")
    .bind(job_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(unavailable)?;
  let current_state = parse_job_state(&current)?;
  let next = start_job_execution(current_state)?;
  if next != current_state {
    let updated = sqlx::query(
      "UPDATE jobs SET state = $1, version = version + 1, \
       updated_at = to_timestamp($2::double precision / 1000.0) WHERE id = $3 AND state = $4",
    )
    .bind(job_state(next))
    .bind(started_at.unix_millis())
    .bind(job_id)
    .bind(current)
    .execute(&mut **transaction)
    .await
    .map_err(|error| classify(error, EntityKind::Job))?;
    if updated.rows_affected() != 1 {
      return Err(StoreError::Unavailable);
    }
  }
  Ok(())
}

#[derive(FromRow)]
struct EventDigestRow {
  sequence: i64,
  event_digest: Vec<u8>,
}

#[derive(Deserialize, Serialize)]
struct StoredOutcome {
  acknowledged_through: u64,
  inserted: usize,
}

impl From<AppendJobEventsOutcome> for StoredOutcome {
  fn from(outcome: AppendJobEventsOutcome) -> Self {
    Self {
      acknowledged_through: outcome.acknowledged_through.get(),
      inserted: outcome.inserted,
    }
  }
}

fn replay(value: Value) -> Result<AppendJobEventsOutcome, StoreError> {
  let stored: StoredOutcome = decode_outcome(value)?;
  let acknowledged_through =
    EventSequence::new(stored.acknowledged_through).map_err(|source| StoreError::InvalidInput {
      operation: StoreOperation::AppendJobEvents,
      source,
    })?;
  Ok(AppendJobEventsOutcome {
    acknowledged_through,
    inserted: stored.inserted,
  })
}

fn facts(request: &AppendJobEvents, job_id: JobId, outcome: &AppendJobEventsOutcome) -> MutationFacts {
  MutationFacts {
    actor_kind: "agent",
    actor_identity: Some(request.lease.agent_id.to_string()),
    target_identity: job_id.to_string(),
    safe_metadata: json!({
      "acknowledged_through": outcome.acknowledged_through.get(),
      "batch_size": request.events.len(),
      "inserted": outcome.inserted,
      "lease_id": request.lease.lease_id,
    }),
    outbox_payload: json!({
      "acknowledged_through": outcome.acknowledged_through.get(),
      "job_id": job_id,
      "lease_id": request.lease.lease_id,
      "schema_version": 1,
    }),
  }
}
