use octacity_server_store::StoreError;
use sqlx::{Postgres, Transaction};

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
