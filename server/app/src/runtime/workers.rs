use std::{
  future::Future,
  sync::Arc,
  time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use octacity_observability::{ErrorClass, Outcome, TraceSpan, WorkerKind, record_worker_retries, record_worker_run};
use octacity_server_application::{
  BuildRetentionWorker, InternalTriggerWorker, LeaseExpiryWorker, LogIndexingWorker, ManagedWebhookRegistrationWorker,
  ManualTriggerRetryWorker, ScheduleWorker, WebhookDeliveryWorker,
};
use octacity_server_domain::Timestamp;
use octacity_server_store::WorkerOwner;
use octacity_server_store_postgres::{PostgresAuthoritativeStore, PostgresLogSearchIndex, PostgresStore};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;
use tracing::{info, warn};

use crate::{DurableWorkerError, ServerConfig, config::RetentionWorkerPolicy, readiness::WorkerHealth};

pub(super) struct DurableWorkers {
  pub(super) expiry: LeaseExpiryWorker<PostgresAuthoritativeStore>,
  pub(super) expiry_health: Arc<WorkerHealth>,
  pub(super) schedules: ScheduleWorker<PostgresStore>,
  pub(super) schedule_owner: WorkerOwner,
  pub(super) schedule_health: Arc<WorkerHealth>,
  pub(super) internal_triggers: InternalTriggerWorker<PostgresStore>,
  pub(super) internal_trigger_owner: WorkerOwner,
  pub(super) internal_trigger_health: Arc<WorkerHealth>,
  pub(super) log_indexing: LogIndexingWorker<PostgresStore, PostgresLogSearchIndex>,
  pub(super) log_index_owner: WorkerOwner,
  pub(super) log_index_health: Arc<WorkerHealth>,
  pub(super) retention: BuildRetentionWorker<PostgresStore, PostgresLogSearchIndex>,
  pub(super) retention_owner: WorkerOwner,
  pub(super) retention_health: Arc<WorkerHealth>,
  pub(super) manual_trigger_retries: ManualTriggerRetryWorker,
  pub(super) manual_trigger_retry_owner: WorkerOwner,
  pub(super) manual_trigger_retry_health: Arc<WorkerHealth>,
  pub(super) webhook_deliveries: Option<(WebhookDeliveryWorker, WorkerOwner, Arc<WorkerHealth>)>,
  pub(super) managed_webhooks: Option<(ManagedWebhookRegistrationWorker, WorkerOwner, Arc<WorkerHealth>)>,
}

pub(super) const EXPIRY_WORKER_NAME: &str = "lease-expiry-worker";
pub(super) const INTERNAL_TRIGGER_WORKER_NAME: &str = "internal-trigger-worker";
pub(super) const LOG_INDEX_WORKER_NAME: &str = "log-index-worker";
pub(super) const RETENTION_WORKER_NAME: &str = "build-retention-worker";
pub(super) const MANAGED_WEBHOOK_WORKER_NAME: &str = "managed-webhook-worker";
pub(super) const MANUAL_TRIGGER_RETRY_WORKER_NAME: &str = "manual-trigger-retry-worker";
pub(super) const SCHEDULE_WORKER_NAME: &str = "schedule-worker";
pub(super) const WEBHOOK_DELIVERY_WORKER_NAME: &str = "webhook-delivery-worker";

pub(super) fn spawn_durable_workers(
  workers: DurableWorkers,
  config: &ServerConfig,
  cancellation: CancellationToken,
) -> JoinHandle<Result<(), DurableWorkerError>> {
  let lease_expiry_policy = config.lease_expiry_worker();
  let schedule_policy = config.schedule_worker();
  let internal_trigger_policy = config.internal_trigger_worker();
  let log_index_policy = config.log_index_worker().worker();
  let retention_policy = config.retention_worker();
  let trigger_evaluation_policy = config.trigger_evaluation_worker().worker();
  let webhook_policy = config.webhook_worker();
  let webhook_delivery_policy = webhook_policy.delivery().worker();
  let expiry = spawn_expiry_worker(
    workers.expiry,
    workers.expiry_health,
    lease_expiry_policy.poll_interval(),
    lease_expiry_policy.claim_lifetime(),
    cancellation.child_token(),
  );
  let schedules = spawn_schedule_worker(
    workers.schedules,
    workers.schedule_owner,
    workers.schedule_health,
    schedule_policy.poll_interval(),
    schedule_policy.claim_lifetime(),
    schedule_policy.batch_size(),
    cancellation.child_token(),
  );
  let internal_triggers = spawn_internal_trigger_worker(
    workers.internal_triggers,
    workers.internal_trigger_owner,
    workers.internal_trigger_health,
    internal_trigger_policy.poll_interval(),
    internal_trigger_policy.claim_lifetime(),
    internal_trigger_policy.batch_size(),
    cancellation.child_token(),
  );
  let log_indexing = spawn_log_index_worker(
    workers.log_indexing,
    workers.log_index_owner,
    workers.log_index_health,
    log_index_policy.poll_interval(),
    log_index_policy.claim_lifetime(),
    log_index_policy.batch_size(),
    cancellation.child_token(),
  );
  let retention = spawn_retention_worker(
    workers.retention,
    workers.retention_owner,
    workers.retention_health,
    retention_policy,
    cancellation.child_token(),
  );
  let manual_trigger_retries = spawn_manual_trigger_retry_worker(
    workers.manual_trigger_retries,
    workers.manual_trigger_retry_owner,
    workers.manual_trigger_retry_health,
    trigger_evaluation_policy.poll_interval(),
    trigger_evaluation_policy.claim_lifetime(),
    trigger_evaluation_policy.batch_size(),
    cancellation.child_token(),
  );
  let webhook_deliveries = workers.webhook_deliveries.map(|(worker, owner, health)| {
    spawn_webhook_delivery_worker(
      worker,
      owner,
      health,
      webhook_delivery_policy.poll_interval(),
      webhook_delivery_policy.claim_lifetime(),
      webhook_delivery_policy.batch_size(),
      cancellation.child_token(),
    )
  });
  let managed_webhooks = workers.managed_webhooks.map(|(worker, owner, health)| {
    spawn_managed_webhook_worker(
      worker,
      owner,
      health,
      webhook_delivery_policy.poll_interval(),
      webhook_delivery_policy.claim_lifetime(),
      webhook_policy.managed_batch_size(),
      cancellation.child_token(),
    )
  });
  let mut tasks = vec![
    (EXPIRY_WORKER_NAME, expiry),
    (SCHEDULE_WORKER_NAME, schedules),
    (INTERNAL_TRIGGER_WORKER_NAME, internal_triggers),
    (LOG_INDEX_WORKER_NAME, log_indexing),
    (RETENTION_WORKER_NAME, retention),
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

fn spawn_retention_worker(
  worker: BuildRetentionWorker<PostgresStore, PostgresLogSearchIndex>,
  owner: WorkerOwner,
  health: Arc<WorkerHealth>,
  policy: RetentionWorkerPolicy,
  cancellation: CancellationToken,
) -> JoinHandle<()> {
  let worker_policy = policy.retrying().worker();
  let work_batch_size = worker_policy.batch_size();
  let object_batch_size = policy.object_batch_size();
  let worker = Arc::new(worker);
  spawn_worker_loop(
    WorkerIdentity::new(RETENTION_WORKER_NAME, WorkerKind::Retention),
    health,
    worker_policy.poll_interval(),
    worker_policy.claim_lifetime(),
    cancellation,
    move |observed_at, claim_expires_at| {
      let worker = worker.clone();
      let owner = owner.clone();
      async move {
        worker
          .run_once(owner, observed_at, claim_expires_at, work_batch_size, object_batch_size)
          .await
      }
    },
    |outcome| {
      if outcome.retries_scheduled > 0 {
        record_worker_retries(WorkerKind::Retention, outcome.retries_scheduled);
      }
      if outcome.claimed > 0 {
        info!(
          claimed = outcome.claimed,
          completed = outcome.completed,
          pending = outcome.pending,
          retries = outcome.retries_scheduled,
          "Build Result retention work advanced"
        );
      }
    },
  )
}

fn spawn_log_index_worker(
  worker: LogIndexingWorker<PostgresStore, PostgresLogSearchIndex>,
  owner: WorkerOwner,
  health: Arc<WorkerHealth>,
  poll_interval: Duration,
  claim_lifetime: Duration,
  batch_size: u16,
  cancellation: CancellationToken,
) -> JoinHandle<()> {
  let worker = Arc::new(worker);
  spawn_worker_loop(
    WorkerIdentity::new(LOG_INDEX_WORKER_NAME, WorkerKind::LogIndex),
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
      if outcome.retries_scheduled > 0 {
        record_worker_retries(WorkerKind::LogIndex, outcome.retries_scheduled);
      }
      if outcome.claimed > 0 {
        info!(
          claimed = outcome.claimed,
          completed = outcome.completed,
          retries = outcome.retries_scheduled,
          dead_letters = outcome.dead_letters,
          lost_claims = outcome.lost_claims,
          "Build-log indexing work advanced"
        );
      }
    },
  )
}

fn spawn_worker_supervisor(
  workers: Vec<(&'static str, JoinHandle<()>)>,
  cancellation: CancellationToken,
) -> JoinHandle<Result<(), DurableWorkerError>> {
  tokio::spawn(async move {
    let mut tasks = tokio::task::JoinSet::new();
    for (name, task) in workers {
      track_worker(&mut tasks, name, task);
    }

    let first_exit = tasks.join_next().await;
    if cancellation.is_cancelled() {
      while tasks.join_next().await.is_some() {}
      return Ok(());
    }

    cancellation.cancel();
    while tasks.join_next().await.is_some() {}
    Err(match first_exit {
      Some(Ok((worker, Ok(())))) => DurableWorkerError::UnexpectedExit { worker },
      Some(Ok((worker, Err(source)))) => DurableWorkerError::WorkerTask { worker, source },
      Some(Err(source)) => DurableWorkerError::SupervisorTask(source),
      None => DurableWorkerError::NoWorkers,
    })
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
    WorkerIdentity::new(MANUAL_TRIGGER_RETRY_WORKER_NAME, WorkerKind::TriggerEvaluation),
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
      if outcome.retries_scheduled > 0 {
        record_worker_retries(WorkerKind::TriggerEvaluation, outcome.retries_scheduled);
      }
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
    WorkerIdentity::new(MANAGED_WEBHOOK_WORKER_NAME, WorkerKind::Webhook),
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
      if outcome.retries_scheduled > 0 {
        record_worker_retries(WorkerKind::Webhook, outcome.retries_scheduled);
      }
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
    WorkerIdentity::new(EXPIRY_WORKER_NAME, WorkerKind::LeaseExpiry),
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
    WorkerIdentity::new(SCHEDULE_WORKER_NAME, WorkerKind::Schedule),
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
    WorkerIdentity::new(INTERNAL_TRIGGER_WORKER_NAME, WorkerKind::InternalTrigger),
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
    WorkerIdentity::new(WEBHOOK_DELIVERY_WORKER_NAME, WorkerKind::Webhook),
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
      if outcome.retries_scheduled > 0 {
        record_worker_retries(WorkerKind::Webhook, outcome.retries_scheduled);
      }
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

#[derive(Clone, Copy)]
struct WorkerIdentity {
  name: &'static str,
  kind: WorkerKind,
}

impl WorkerIdentity {
  const fn new(name: &'static str, kind: WorkerKind) -> Self {
    Self { name, kind }
  }
}

fn spawn_worker_loop<Pass, PassFuture, WorkerOutcome, Error, Observe>(
  worker: WorkerIdentity,
  health: Arc<WorkerHealth>,
  poll_interval: Duration,
  claim_lifetime: Duration,
  cancellation: CancellationToken,
  pass: Pass,
  observe: Observe,
) -> JoinHandle<()>
where
  Pass: Fn(Timestamp, Timestamp) -> PassFuture + Send + Sync + 'static,
  PassFuture: Future<Output = Result<WorkerOutcome, Error>> + Send + 'static,
  WorkerOutcome: Send + 'static,
  Error: Send + 'static,
  Observe: Fn(&WorkerOutcome) + Send + Sync + 'static,
{
  let WorkerIdentity {
    name: worker_name,
    kind: worker_kind,
  } = worker;
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
        record_worker_run(
          worker_kind,
          Outcome::Failure,
          Some(ErrorClass::Internal),
          Duration::ZERO,
        );
        warn!(
          worker = worker_name,
          error.class = ErrorClass::Internal.as_str(),
          "durable worker could not represent the system clock"
        );
        continue;
      };
      let span = tracing::info_span!(TraceSpan::ServerWorkerRun.as_str(), worker.kind = worker_kind.as_str(),);
      let started = Instant::now();
      let result = pass(observed_at, claim_expires_at).instrument(span.clone()).await;
      let elapsed = started.elapsed();
      let _entered = span.enter();
      match result {
        Ok(outcome) => {
          health.mark_success();
          record_worker_run(worker_kind, Outcome::Success, None, elapsed);
          observe(&outcome);
        }
        Err(_) => {
          health.mark_failure();
          record_worker_run(worker_kind, Outcome::Failure, Some(ErrorClass::Unavailable), elapsed);
          warn!(
            worker = worker_name,
            error.class = ErrorClass::Unavailable.as_str(),
            "durable worker pass failed"
          );
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
    assert!(matches!(
      result.expect("supervisor task must not panic"),
      Err(DurableWorkerError::WorkerTask { worker: "failed", .. })
    ));
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
    assert!(matches!(
      supervisor.await.expect("supervisor task must not panic"),
      Ok(())
    ));
  }
}
