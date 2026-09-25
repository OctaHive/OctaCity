use octacity_server_store::StoreError;
use sqlx::{PgPool, Postgres, Transaction};

use crate::{READY_JOB_NOTIFICATION_CHANNEL, database::unavailable};

/// Adds a commit-coupled wake-up hint to a transaction that made Jobs ready.
pub(crate) async fn notify_after_commit(transaction: &mut Transaction<'_, Postgres>) -> Result<(), StoreError> {
  sqlx::query("SELECT pg_notify($1, '')")
    .bind(READY_JOB_NOTIFICATION_CHANNEL)
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
  Ok(())
}

/// Returns an authoritative snapshot of Jobs currently eligible for placement.
pub async fn ready_job_count(pool: &PgPool) -> Result<u64, sqlx::Error> {
  let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM ready_queue_entries")
    .fetch_one(pool)
    .await?;
  u64::try_from(count).map_err(|error| sqlx::Error::Decode(Box::new(error)))
}
