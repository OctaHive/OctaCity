use std::{
  fmt::Display,
  future::Future,
  sync::Arc,
  time::{Duration, SystemTime, UNIX_EPOCH},
};

use octacity_server_application::{
  InternalTriggerWorker, LeaseExpiryWorker, ManagedWebhookRegistrationWorker, ManualTriggerRetryWorker, ScheduleWorker,
  WebhookDeliveryWorker,
};
use octacity_server_domain::Timestamp;
use octacity_server_store::WorkerOwner;
use octacity_server_store_postgres::{PostgresAuthoritativeStore, PostgresStore};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::{ServerConfig, readiness::WorkerHealth};

pub(super) struct DurableWorkers {
  pub(super) expiry: LeaseExpiryWorker<PostgresAuthoritativeStore>,
  pub(super) expiry_health: Arc<WorkerHealth>,
  pub(super) schedules: ScheduleWorker<PostgresStore>,
  pub(super) schedule_owner: WorkerOwner,
  pub(super) schedule_health: Arc<WorkerHealth>,
  pub(super) internal_triggers: InternalTriggerWorker<PostgresStore>,
  pub(super) internal_trigger_owner: WorkerOwner,
  pub(super) internal_trigger_health: Arc<WorkerHealth>,
  pub(super) manual_trigger_retries: ManualTriggerRetryWorker,
  pub(super) manual_trigger_retry_owner: WorkerOwner,
  pub(super) manual_trigger_retry_health: Arc<WorkerHealth>,
  pub(super) webhook_deliveries: Option<(WebhookDeliveryWorker, WorkerOwner, Arc<WorkerHealth>)>,
  pub(super) managed_webhooks: Option<(ManagedWebhookRegistrationWorker, WorkerOwner, Arc<WorkerHealth>)>,
}

pub(super) const EXPIRY_WORKER_NAME: &str = "lease-expiry-worker";
pub(super) const INTERNAL_TRIGGER_WORKER_NAME: &str = "internal-trigger-worker";
pub(super) const MANAGED_WEBHOOK_WORKER_NAME: &str = "managed-webhook-worker";
pub(super) const MANUAL_TRIGGER_RETRY_WORKER_NAME: &str = "manual-trigger-retry-worker";
pub(super) const SCHEDULE_WORKER_NAME: &str = "schedule-worker";
pub(super) const WEBHOOK_DELIVERY_WORKER_NAME: &str = "webhook-delivery-worker";

pub(super) fn spawn_durable_workers(
  workers: DurableWorkers,
  config: &ServerConfig,
  cancellation: CancellationToken,
) -> JoinHandle<()> {
  let expiry = spawn_expiry_worker(
    workers.expiry,
    workers.expiry_health,
    config.lease_expiry_poll_interval(),
    config.lease_expiry_claim_lifetime(),
    cancellation.child_token(),
  );
  let schedules = spawn_schedule_worker(
    workers.schedules,
    workers.schedule_owner,
    workers.schedule_health,
    config.schedule_poll_interval(),
    config.schedule_claim_lifetime(),
    config.schedule_batch_size(),
    cancellation.child_token(),
  );
  let internal_triggers = spawn_internal_trigger_worker(
    workers.internal_triggers,
    workers.internal_trigger_owner,
    workers.internal_trigger_health,
    config.internal_trigger_poll_interval(),
    config.internal_trigger_claim_lifetime(),
    config.internal_trigger_batch_size(),
    cancellation.child_token(),
  );
  let manual_trigger_retries = spawn_manual_trigger_retry_worker(
    workers.manual_trigger_retries,
    workers.manual_trigger_retry_owner,
    workers.manual_trigger_retry_health,
    config.trigger_evaluation_poll_interval(),
    config.trigger_evaluation_claim_lifetime(),
    config.trigger_evaluation_batch_size(),
    cancellation.child_token(),
  );
  let webhook_deliveries = workers.webhook_deliveries.map(|(worker, owner, health)| {
    spawn_webhook_delivery_worker(
      worker,
      owner,
      health,
      config.webhook_worker_poll_interval(),
      config.webhook_worker_claim_lifetime(),
      config.webhook_delivery_batch_size(),
      cancellation.child_token(),
    )
  });
  let managed_webhooks = workers.managed_webhooks.map(|(worker, owner, health)| {
    spawn_managed_webhook_worker(
      worker,
      owner,
      health,
      config.webhook_worker_poll_interval(),
      config.webhook_worker_claim_lifetime(),
      config.managed_webhook_batch_size(),
      cancellation.child_token(),
    )
  });
  let mut tasks = vec![
    (EXPIRY_WORKER_NAME, expiry),
    (SCHEDULE_WORKER_NAME, schedules),
    (INTERNAL_TRIGGER_WORKER_NAME, internal_triggers),
    (MANUAL_TRIGGER_RETRY_WORKER_NAME, manual_trigger_retries),
  ];
  if let Some(task) = webhook_deliveries {
    tasks.push((WEBHOOK_DELIVERY_WORKER_NAME, task));
  }
  if let Some(task) = managed_webhooks {
    tasks.push((MANAGED_WEBHOOK_WORKER_NAME, task));
  }
  spawn_worker_supervisor(tasks, cancellation)
}

fn spawn_worker_supervisor(
  workers: Vec<(&'static str, JoinHandle<()>)>,
  cancellation: CancellationToken,
) -> JoinHandle<()> {
  tokio::spawn(async move {
    let mut tasks = tokio::task::JoinSet::new();
    for (name, task) in workers {
      track_worker(&mut tasks, name, task);
    }

    let first_exit = tasks.join_next().await;
    if cancellation.is_cancelled() {
      while tasks.join_next().await.is_some() {}
      return;
    }

    cancellation.cancel();
    while tasks.join_next().await.is_some() {}
    match first_exit {
      Some(Ok((worker, Ok(())))) => panic!("durable worker {worker} terminated unexpectedly"),
      Some(Ok((worker, Err(source)))) => panic!("durable worker {worker} failed: {source}"),
      Some(Err(source)) => panic!("durable worker supervisor failed: {source}"),
      None => panic!("durable worker supervisor started without workers"),
    }
  })
}

fn track_worker(
  tasks: &mut tokio::task::JoinSet<(&'static str, Result<(), tokio::task::JoinError>)>,
  name: &'static str,
  task: JoinHandle<()>,
) {
  tasks.spawn(async move { (name, task.await) });
}

fn spawn_manual_trigger_retry_worker(
  worker: ManualTriggerRetryWorker,
  owner: WorkerOwner,
  health: Arc<WorkerHealth>,
  poll_interval: Duration,
  claim_lifetime: Duration,
  batch_size: u16,
  cancellation: CancellationToken,
) -> JoinHandle<()> {
  let worker = Arc::new(worker);
  spawn_worker_loop(
    MANUAL_TRIGGER_RETRY_WORKER_NAME,
    health,
    poll_interval,
    claim_lifetime,
    cancellation,
    move |observed_at, claim_expires_at| {
      let worker = worker.clone();
      let owner = owner.clone();
      async move { worker.run_once(owner, observed_at, claim_expires_at, batch_size).await }
    },
    |outcome| {
      if outcome.claimed > 0 {
        info!(
          claimed = outcome.claimed,
          completed = outcome.completed,
          retries = outcome.retries_scheduled,
          dead_letters = outcome.dead_letters,
          "manual Trigger evaluations advanced"
        );
      }
    },
  )
}

fn spawn_managed_webhook_worker(
  worker: ManagedWebhookRegistrationWorker,
  owner: WorkerOwner,
  health: Arc<WorkerHealth>,
  poll_interval: Duration,
  claim_lifetime: Duration,
  batch_size: u16,
  cancellation: CancellationToken,
) -> JoinHandle<()> {
  let worker = Arc::new(worker);
  spawn_worker_loop(
    MANAGED_WEBHOOK_WORKER_NAME,
    health,
    poll_interval,
    claim_lifetime,
    cancellation,
    move |observed_at, claim_expires_at| {
      let worker = worker.clone();
      let owner = owner.clone();
      async move { worker.run_once(owner, observed_at, claim_expires_at, batch_size).await }
    },
    |outcome| {
      if outcome.claimed > 0 {
        info!(
          claimed = outcome.claimed,
          completed = outcome.completed,
          retries = outcome.retries_scheduled,
          dead_letters = outcome.dead_letters,
          "managed webhook operations advanced"
        );
      }
    },
  )
}

fn spawn_expiry_worker(
  worker: LeaseExpiryWorker<PostgresAuthoritativeStore>,
  health: Arc<WorkerHealth>,
  poll_interval: Duration,
  claim_lifetime: Duration,
  cancellation: CancellationToken,
) -> JoinHandle<()> {
  let worker = Arc::new(worker);
  spawn_worker_loop(
    EXPIRY_WORKER_NAME,
    health,
    poll_interval,
    claim_lifetime,
    cancellation,
    move |observed_at, claim_expires_at| {
      let worker = worker.clone();
      async move { worker.run_once(observed_at, claim_expires_at).await }
    },
    |outcome| {
      if outcome.claimed > 0 {
        info!(
          claimed = outcome.claimed,
          requeued = outcome.requeued,
          failed = outcome.failed,
          cancelled = outcome.cancelled,
          "expired leases recovered"
        );
      }
    },
  )
}

fn spawn_schedule_worker(
  worker: ScheduleWorker<PostgresStore>,
  owner: WorkerOwner,
  health: Arc<WorkerHealth>,
  poll_interval: Duration,
  claim_lifetime: Duration,
  batch_size: u16,
  cancellation: CancellationToken,
) -> JoinHandle<()> {
  let worker = Arc::new(worker);
  spawn_worker_loop(
    SCHEDULE_WORKER_NAME,
    health,
    poll_interval,
    claim_lifetime,
    cancellation,
    move |observed_at, claim_expires_at| {
      let worker = worker.clone();
      let owner = owner.clone();
      async move { worker.run_once(owner, observed_at, claim_expires_at, batch_size).await }
    },
    |outcome| {
      if outcome.claimed_schedules > 0 {
        info!(
          claimed = outcome.claimed_schedules,
          evaluated = outcome.evaluated_occurrences,
          "due schedules evaluated"
        );
      }
    },
  )
}

fn spawn_internal_trigger_worker(
  worker: InternalTriggerWorker<PostgresStore>,
  owner: WorkerOwner,
  health: Arc<WorkerHealth>,
  poll_interval: Duration,
  claim_lifetime: Duration,
  batch_size: u16,
  cancellation: CancellationToken,
) -> JoinHandle<()> {
  let worker = Arc::new(worker);
  spawn_worker_loop(
    INTERNAL_TRIGGER_WORKER_NAME,
    health,
    poll_interval,
    claim_lifetime,
    cancellation,
    move |observed_at, claim_expires_at| {
      let worker = worker.clone();
      let owner = owner.clone();
      async move { worker.run_once(owner, observed_at, claim_expires_at, batch_size).await }
    },
    |outcome| {
      if outcome.claimed_events > 0 {
        info!(
          claimed = outcome.claimed_events,
          evaluated = outcome.evaluated_occurrences,
          protected = outcome.protected_occurrences,
          "internal Trigger events delivered"
        );
      }
    },
  )
}

fn spawn_webhook_delivery_worker(
  worker: WebhookDeliveryWorker,
  owner: WorkerOwner,
  health: Arc<WorkerHealth>,
  poll_interval: Duration,
  claim_lifetime: Duration,
  batch_size: u16,
  cancellation: CancellationToken,
) -> JoinHandle<()> {
  let worker = Arc::new(worker);
  spawn_worker_loop(
    WEBHOOK_DELIVERY_WORKER_NAME,
    health,
    poll_interval,
    claim_lifetime,
    cancellation,
    move |observed_at, claim_expires_at| {
      let worker = worker.clone();
      let owner = owner.clone();
      async move { worker.run_once(owner, observed_at, claim_expires_at, batch_size).await }
    },
    |outcome| {
      if outcome.claimed > 0 {
        info!(
          claimed = outcome.claimed,
          normalized = outcome.normalized,
          duplicates = outcome.duplicates,
          evaluated = outcome.evaluated,
          suppressed = outcome.suppressed,
          retries = outcome.retries_scheduled,
          dead_letters = outcome.dead_letters,
          "webhook deliveries advanced"
        );
      }
    },
  )
}

fn spawn_worker_loop<Pass, PassFuture, Outcome, Error, Observe>(
  worker_name: &'static str,
  health: Arc<WorkerHealth>,
  poll_interval: Duration,
  claim_lifetime: Duration,
  cancellation: CancellationToken,
  pass: Pass,
  observe: Observe,
) -> JoinHandle<()>
where
  Pass: Fn(Timestamp, Timestamp) -> PassFuture + Send + Sync + 'static,
  PassFuture: Future<Output = Result<Outcome, Error>> + Send + 'static,
  Outcome: Send + 'static,
  Error: Display + Send + 'static,
  Observe: Fn(&Outcome) + Send + Sync + 'static,
{
  tokio::spawn(async move {
    let _unhealthy_on_drop = UnhealthyOnDrop(health.clone());
    loop {
      tokio::select! {
        biased;
        () = cancellation.cancelled() => return,
        () = tokio::time::sleep(poll_interval) => {}
      }
      let Some((observed_at, claim_expires_at)) = claim_window(claim_lifetime) else {
        health.mark_failure();
        warn!(
          worker = worker_name,
          "durable worker could not represent the system clock"
        );
        continue;
      };
      match pass(observed_at, claim_expires_at).await {
        Ok(outcome) => {
          health.mark_success();
          observe(&outcome);
        }
        Err(error) => {
          health.mark_failure();
          warn!(worker = worker_name, %error, "durable worker pass failed");
        }
      }
    }
  })
}

struct UnhealthyOnDrop(Arc<WorkerHealth>);

impl Drop for UnhealthyOnDrop {
  fn drop(&mut self) {
    self.0.mark_failure();
  }
}

fn claim_window(claim_lifetime: Duration) -> Option<(Timestamp, Timestamp)> {
  let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .ok()
    .and_then(|duration| i64::try_from(duration.as_millis()).ok())?;
  let lifetime = i64::try_from(claim_lifetime.as_millis()).ok()?;
  let until = now.checked_add(lifetime)?;
  Some((
    Timestamp::from_unix_millis(now).ok()?,
    Timestamp::from_unix_millis(until).ok()?,
  ))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn supervisor_propagates_the_first_worker_failure_and_stops_siblings() {
    let cancellation = CancellationToken::new();
    let sibling_cancellation = cancellation.child_token();
    let sibling = tokio::spawn(async move {
      sibling_cancellation.cancelled().await;
    });
    let failed = tokio::spawn(async {
      panic!("worker failure");
    });
    let supervisor = spawn_worker_supervisor(vec![("sibling", sibling), ("failed", failed)], cancellation);

    let result = tokio::time::timeout(Duration::from_secs(1), supervisor)
      .await
      .expect("worker failure must stop the worker group");
    assert!(result.expect_err("worker failure must fail the supervisor").is_panic());
  }

  #[tokio::test]
  async fn supervisor_drains_workers_after_requested_cancellation() {
    let cancellation = CancellationToken::new();
    let worker_cancellation = cancellation.child_token();
    let worker = tokio::spawn(async move {
      worker_cancellation.cancelled().await;
    });
    let supervisor = spawn_worker_supervisor(vec![("worker", worker)], cancellation.clone());

    cancellation.cancel();
    supervisor.await.expect("requested cancellation must drain cleanly");
  }
}
