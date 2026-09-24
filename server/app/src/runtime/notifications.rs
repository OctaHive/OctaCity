use std::sync::Arc;

use octacity_server_api_agent::ReadyJobNotificationHub;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::warn;

pub(super) fn spawn_ready_job_listener(
  pool: sqlx::PgPool,
  hub: Arc<ReadyJobNotificationHub>,
  reconnect_delay: std::time::Duration,
  cancellation: CancellationToken,
) -> JoinHandle<()> {
  tokio::spawn(async move {
    loop {
      let result = ready_job_listener_pass(&pool, &hub, cancellation.child_token()).await;
      if cancellation.is_cancelled() {
        return;
      }
      if let Err(error) = result {
        warn!(%error, "ready-Job notification listener disconnected");
      }
      tokio::select! {
        biased;
        () = cancellation.cancelled() => return,
        () = tokio::time::sleep(reconnect_delay) => {}
      }
    }
  })
}

async fn ready_job_listener_pass(
  pool: &sqlx::PgPool,
  hub: &ReadyJobNotificationHub,
  cancellation: CancellationToken,
) -> Result<(), sqlx::Error> {
  let mut listener = sqlx::postgres::PgListener::connect_with(pool).await?;
  listener
    .listen(octacity_server_store_postgres::READY_JOB_NOTIFICATION_CHANNEL)
    .await?;
  loop {
    tokio::select! {
      biased;
      () = cancellation.cancelled() => return Ok(()),
      notification = listener.recv() => {
        notification?;
        hub.notify();
      }
    }
  }
}
