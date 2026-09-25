use std::sync::Arc;

use octacity_observability::record_ready_jobs;
use octacity_server_api_agent::ReadyJobNotificationHub;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

pub(super) fn spawn_ready_job_listener(
  pool: sqlx::PgPool,
  hub: Arc<ReadyJobNotificationHub>,
  reconnect_delay: std::time::Duration,
  sample_interval: std::time::Duration,
  cancellation: CancellationToken,
) -> JoinHandle<()> {
  tokio::spawn(async move {
    loop {
      let result = ready_job_listener_pass(&pool, &hub, sample_interval, cancellation.child_token()).await;
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
  sample_interval: std::time::Duration,
  cancellation: CancellationToken,
) -> Result<(), sqlx::Error> {
  let mut listener = sqlx::postgres::PgListener::connect_with(pool).await?;
  listener
    .listen(octacity_server_store_postgres::READY_JOB_NOTIFICATION_CHANNEL)
    .await?;
  sample_ready_jobs(pool).await;
  let mut sample_tick = tokio::time::interval(sample_interval);
  sample_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
  sample_tick.tick().await;
  loop {
    tokio::select! {
      biased;
      () = cancellation.cancelled() => return Ok(()),
      notification = listener.recv() => {
        notification?;
        hub.notify();
        sample_ready_jobs(pool).await;
      }
      _ = sample_tick.tick() => sample_ready_jobs(pool).await,
    }
  }
}

async fn sample_ready_jobs(pool: &sqlx::PgPool) {
  match octacity_server_store_postgres::ready_job_count(pool).await {
    Ok(count) => record_ready_jobs(count),
    Err(_) => debug!("ready-Job metric sample unavailable"),
  }
}
