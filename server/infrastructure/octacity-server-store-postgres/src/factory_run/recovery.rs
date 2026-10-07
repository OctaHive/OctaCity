use octacity_server_domain::Timestamp;
use octacity_server_store::{MAX_RESTORE_RECONCILIATION_BATCH_SIZE, RestoreFactoryRecovery, StoreError};
use sqlx::PgPool;

use super::{OutboxRow, decode_outbox, insert_outbox};
use crate::database::unavailable;

pub(crate) async fn recover_restored_ownership(
  pool: &PgPool,
  observed_at: Timestamp,
  limit: u16,
) -> Result<RestoreFactoryRecovery, StoreError> {
  if limit == 0 || limit > MAX_RESTORE_RECONCILIATION_BATCH_SIZE {
    return Err(StoreError::Unavailable);
  }
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let expired_claims = sqlx::query(
    "WITH candidates AS (\
       SELECT current.run_id FROM factory_run_current AS current \
       JOIN factory_runs AS run ON run.id = current.run_id \
       WHERE run.visible AND current.claim_id IS NOT NULL \
       ORDER BY current.run_id FOR UPDATE OF current SKIP LOCKED LIMIT $1\
     ) UPDATE factory_run_current AS current SET claim_id = NULL \
       FROM candidates WHERE current.run_id = candidates.run_id",
  )
  .bind(i64::from(limit))
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?
  .rows_affected();

  let claimed = sqlx::query_as::<_, OutboxRow>(
    "WITH latest AS (\
       SELECT DISTINCT ON (operation_id) id, operation_id, run_id, kind, input_digest, state, attempt, \
         available_at, recorded_at, owner, fence, claimed_at, claim_expires_at \
       FROM factory_outbox_records \
       ORDER BY operation_id, attempt DESC, \
         CASE state WHEN 'pending' THEN 0 WHEN 'claimed' THEN 1 ELSE 2 END DESC, recorded_at DESC, id DESC\
     ) SELECT id, operation_id, run_id, kind, input_digest, state, attempt, \
       FLOOR(EXTRACT(EPOCH FROM available_at) * 1000)::BIGINT AS available_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM recorded_at) * 1000)::BIGINT AS recorded_at_millis, \
       owner, fence, FLOOR(EXTRACT(EPOCH FROM claimed_at) * 1000)::BIGINT AS claimed_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM claim_expires_at) * 1000)::BIGINT AS claim_expires_at_millis \
     FROM latest WHERE state = 'claimed' ORDER BY operation_id LIMIT $1",
  )
  .bind(i64::from(limit))
  .fetch_all(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let requeued_outbox = u16::try_from(claimed.len()).map_err(|_| StoreError::Unavailable)?;
  for row in claimed {
    let current = decode_outbox(row)?;
    let retry = octacity_server_store::FactoryOutboxRecord::retry(&current, observed_at, observed_at)
      .ok_or(StoreError::Unavailable)?;
    insert_outbox(&mut transaction, &retry).await?;
  }

  let expired_retention_claims = sqlx::query(
    "WITH candidates AS (\
       SELECT run_id FROM factory_retention_work WHERE claim_owner IS NOT NULL \
       ORDER BY run_id FOR UPDATE SKIP LOCKED LIMIT $1\
     ) UPDATE factory_retention_work AS work SET claim_owner = NULL, claim_expires_at = NULL \
       FROM candidates WHERE work.run_id = candidates.run_id",
  )
  .bind(i64::from(limit))
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?
  .rows_affected();

  let has_claims: bool = sqlx::query_scalar(
    "SELECT EXISTS(\
       SELECT 1 FROM factory_run_current AS current \
       JOIN factory_runs AS run ON run.id = current.run_id \
       WHERE run.visible AND current.claim_id IS NOT NULL\
     )",
  )
  .fetch_one(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let has_claimed_outbox: bool = sqlx::query_scalar(
    "SELECT EXISTS(\
       SELECT 1 FROM (\
         SELECT DISTINCT ON (operation_id) state FROM factory_outbox_records \
         ORDER BY operation_id, attempt DESC, \
           CASE state WHEN 'pending' THEN 0 WHEN 'claimed' THEN 1 ELSE 2 END DESC, recorded_at DESC, id DESC\
       ) AS latest WHERE state = 'claimed'\
     )",
  )
  .fetch_one(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let has_retention_claims: bool =
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM factory_retention_work WHERE claim_owner IS NOT NULL)")
      .fetch_one(&mut *transaction)
      .await
      .map_err(unavailable)?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(RestoreFactoryRecovery {
    expired_claims: u16::try_from(expired_claims).map_err(|_| StoreError::Unavailable)?,
    expired_retention_claims: u16::try_from(expired_retention_claims).map_err(|_| StoreError::Unavailable)?,
    requeued_outbox,
    has_more: has_claims || has_claimed_outbox || has_retention_claims,
  })
}
