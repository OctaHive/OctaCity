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
  pub(super) factory: Option<super::factory::InstalledFactoryWorker>,
  pub(super) actions: octacity_server_application::FactoryActionProjectionWorker<PostgresStore>,
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
  pub(super) input_retention: octacity_server_application::FactoryExecutionInputRetentionWorker<
    PostgresStore,
    octacity_artifact_s3::S3ArtifactStore,
  >,
  pub(super) input_retention_health: Arc<WorkerHealth>,
  pub(super) source_retention: octacity_server_application::FactorySourceInputRetentionWorker<
    PostgresStore,
    octacity_artifact_s3::S3ArtifactStore,
  >,
  pub(super) manual_trigger_retries: ManualTriggerRetryWorker,
  pub(super) manual_trigger_retry_owner: WorkerOwner,
  pub(super) manual_trigger_retry_health: Arc<WorkerHealth>,
  pub(super) webhook_deliveries: Option<(WebhookDeliveryWorker, WorkerOwner, Arc<WorkerHealth>)>,
  pub(super) managed_webhooks: Option<(ManagedWebhookRegistrationWorker, WorkerOwner, Arc<WorkerHealth>)>,
}

pub(super) const FACTORY_WORKER_NAME: &str = "factory-owner-worker";
pub(super) const EXPIRY_WORKER_NAME: &str = "lease-expiry-worker";
pub(super) const INTERNAL_TRIGGER_WORKER_NAME: &str = "internal-trigger-worker";
pub(super) const LOG_INDEX_WORKER_NAME: &str = "log-index-worker";
pub(super) const RETENTION_WORKER_NAME: &str = "build-retention-worker";
pub(super) const INPUT_RETENTION_WORKER_NAME: &str = "factory-input-retention-worker";
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
  let source_retention = spawn_source_input_retention_worker(
    workers.source_retention,
    retention_policy.retrying().worker().poll_interval(),
    retention_policy.retrying().worker().claim_lifetime(),
    retention_policy.object_batch_size().min(64),
    cancellation.child_token(),
  );
  let input_retention = spawn_input_retention_worker(
    workers.input_retention,
    workers.input_retention_health,
    retention_policy,
    config.factory(),
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
  let mut factory_outbox = vec![];
  let mut source_task = None;
  let factory = workers.factory.map(|installed| {
    let policy = config.factory().expect("validated enabled owner policy");
    source_task = installed
      .work_sources
      .map(|worker| spawn_factory_source_worker(worker, cancellation.child_token()));
    for outbox in installed.outbox {
      factory_outbox.push(spawn_factory_outbox_worker(
        outbox.dispatcher,
        outbox.health,
        policy,
        cancellation.child_token(),
      ));
    }
    spawn_factory_worker(installed.owner, installed.health, policy, cancellation.child_token())
  });
  let actions = spawn_action_projection_worker(workers.actions, retention_policy, cancellation.child_token());
  let mut tasks = vec![
    ("factory-action-projection", actions),
    (EXPIRY_WORKER_NAME, expiry),
    (SCHEDULE_WORKER_NAME, schedules),
    (INTERNAL_TRIGGER_WORKER_NAME, internal_triggers),
    (LOG_INDEX_WORKER_NAME, log_indexing),
    (RETENTION_WORKER_NAME, retention),
    (INPUT_RETENTION_WORKER_NAME, input_retention),
    ("factory-source-retention", source_retention),
    (MANUAL_TRIGGER_RETRY_WORKER_NAME, manual_trigger_retries),
  ];
  if let Some(task) = factory {
    tasks.push((FACTORY_WORKER_NAME, task));
  }
  if let Some(task) = source_task {
    tasks.push(("factory-source", task));
  }
  for task in factory_outbox {
    tasks.push(("factory-outbox", task));
  }
  if let Some(task) = webhook_deliveries {
    tasks.push((WEBHOOK_DELIVERY_WORKER_NAME, task));
  }
  if let Some(task) = managed_webhooks {
    tasks.push((MANAGED_WEBHOOK_WORKER_NAME, task));
  }
  spawn_worker_supervisor(tasks, cancellation)
}

// Index maintenance is always scheduled, even when the execution owner is disabled.
fn spawn_action_projection_worker(
  worker: octacity_server_application::FactoryActionProjectionWorker<PostgresStore>,
  policy: RetentionWorkerPolicy,
  cancellation: CancellationToken,
) -> JoinHandle<()> {
  let ordinary = policy.retrying().worker();
  let name = "factory-action-projection";
  let worker = Arc::new(worker);
  let limit = ordinary
    .batch_size()
    .min(octacity_server_store::MAX_FACTORY_RUN_DIAGNOSTIC_PAGE_SIZE);
  let task = spawn_worker_loop(
    WorkerIdentity::new(name, WorkerKind::Factory),
    Arc::new(WorkerHealth::new(
      name,
      ordinary.poll_interval().saturating_add(ordinary.claim_lifetime()),
    )),
    ordinary.poll_interval(),
    ordinary.claim_lifetime(),
    cancellation.child_token(),
    move |at, _| {
      let worker = worker.clone();
      async move {
        let progress = worker.run_once(at, limit).await?;
        if progress.failed > 0 {
          return Err(octacity_server_application::ApplicationError::unavailable());
        }
        Ok(progress)
      }
    },
    |progress| {
      if progress.completed > 0 {
        info!(completed = progress.completed, "Factory action projection advanced");
      }
    },
  );
  isolate_read_model_worker(task, cancellation)
}

// Keep optional read-side failure outside the fail-fast execution supervisor.
// Shutdown still owns/drains the inner task; a stopped index never repairs on read.
fn isolate_read_model_worker(mut task: JoinHandle<()>, cancellation: CancellationToken) -> JoinHandle<()> {
  tokio::spawn(async move {
    tokio::select! {
      biased;
      () = cancellation.cancelled() => { task.abort(); let _ = task.await; }
      _ = &mut task => {
        if !cancellation.is_cancelled() {
          warn!(worker="factory-action-projection", "read-model worker stopped; coverage may be incomplete");
          cancellation.cancelled().await;
        }
      }
    }
  })
}

fn spawn_input_retention_worker(
  worker: octacity_server_application::FactoryExecutionInputRetentionWorker<
    PostgresStore,
    octacity_artifact_s3::S3ArtifactStore,
  >,
  health: Arc<WorkerHealth>,
  retention: RetentionWorkerPolicy,
  factory: Option<crate::FactoryRuntimeConfig>,
  cancellation: CancellationToken,
) -> JoinHandle<()> {
  let ordinary = retention.retrying().worker();
  let poll = factory.map_or(ordinary.poll_interval(), |policy| policy.poll_interval());
  let claim = factory.map_or(ordinary.claim_lifetime(), |policy| policy.claim_lifetime());
  let limit = factory
    .map_or(retention.object_batch_size(), |policy| policy.batch_size())
    .min(octacity_server_store::MAX_ARTIFACT_PAGE_SIZE);
  let mut identity = WorkerIdentity::new(INPUT_RETENTION_WORKER_NAME, WorkerKind::Factory);
  if let Some(policy) = factory {
    identity = identity.with_retry(policy.retry_policy());
  }
  let worker = Arc::new(worker);
  spawn_worker_loop(
    identity,
    health,
    poll,
    claim,
    cancellation,
    move |at, _| {
      let worker = worker.clone();
      async move { worker.run_once(at, limit).await }
    },
    |completed| {
      if *completed > 0 {
        info!(completed, "expired Factory execution inputs deleted");
      }
    },
  )
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
  retry: Option<crate::config::FactoryRetryPolicy>,
}

impl WorkerIdentity {
  const fn new(name: &'static str, kind: WorkerKind) -> Self {
    Self {
      name,
      kind,
      retry: None,
    }
  }
  const fn with_retry(mut self, retry: crate::config::FactoryRetryPolicy) -> Self {
    self.retry = Some(retry);
    self
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
  Error: std::fmt::Display + Send + 'static,
  Observe: Fn(&WorkerOutcome) + Send + Sync + 'static,
{
  let WorkerIdentity {
    name: worker_name,
    kind: worker_kind,
    retry,
  } = worker;
  tokio::spawn(async move {
    let _unhealthy_on_drop = UnhealthyOnDrop(health.clone());
    let mut failures = 0_u16;
    let mut delay = poll_interval;
    loop {
      tokio::select! {
        biased;
        () = cancellation.cancelled() => return,
        () = tokio::time::sleep(delay) => {}
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
          failures = 0;
          delay = poll_interval;
          health.mark_success();
          record_worker_run(worker_kind, Outcome::Success, None, elapsed);
          observe(&outcome);
        }
        Err(_error) => {
          if let Some(retry) = retry {
            failures = failures.saturating_add(1);
            delay = if failures >= retry.consecutive_failure_limit() {
              retry.circuit_open()
            } else {
              delay.saturating_mul(2).min(retry.max_backoff())
            };
            record_worker_retries(worker_kind, 1);
          }
          health.mark_failure();
          record_worker_run(worker_kind, Outcome::Failure, Some(ErrorClass::Unavailable), elapsed);
          warn!(
            worker = worker_name,
            error.class = ErrorClass::Unavailable.as_str(),
            retry.delay_ms = delay.as_millis() as u64,
            circuit.open = retry.is_some_and(|policy| failures >= policy.consecutive_failure_limit()),
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

fn spawn_factory_worker(
  worker: octacity_server_application::FactoryReconciler<PostgresStore>,
  health: Arc<WorkerHealth>,
  policy: crate::FactoryRuntimeConfig,
  cancellation: CancellationToken,
) -> JoinHandle<()> {
  let worker = Arc::new(worker);
  let shutdown = Arc::new(octacity_server_application::FactoryReconciliationShutdown::new());
  let pass_cancellation = cancellation.clone();
  spawn_worker_loop(
    WorkerIdentity::new(FACTORY_WORKER_NAME, WorkerKind::Factory).with_retry(policy.retry_policy()),
    health,
    policy.poll_interval(),
    policy.claim_lifetime(),
    cancellation,
    move |observed_at, expires_at| {
      let worker = worker.clone();
      let shutdown = shutdown.clone();
      let cancellation = pass_cancellation.clone();
      async move {
        let pass = worker.run_once(observed_at, expires_at, &shutdown);
        tokio::pin!(pass);
        tokio::select! {
          biased;
          () = cancellation.cancelled() => { shutdown.request(); pass.await },
          result = &mut pass => result,
        }
      }
    },
    |_| {},
  )
}

pub(super) fn spawn_factory_outbox_worker(
  dispatcher: Arc<dyn octacity_server_application::FactoryOutboxDispatcher>,
  health: Arc<WorkerHealth>,
  policy: crate::FactoryRuntimeConfig,
  cancellation: CancellationToken,
) -> JoinHandle<()> {
  spawn_worker_loop(
    WorkerIdentity::new("factory-outbox", WorkerKind::Factory).with_retry(policy.retry_policy()),
    health,
    policy.poll_interval(),
    policy.claim_lifetime(),
    cancellation,
    move |at, _| {
      let dispatcher = dispatcher.clone();
      async move { dispatcher.dispatch_once(at).await }
    },
    |_| {},
  )
}

// Source health is deliberately excluded from Factory admission readiness.
pub(super) fn spawn_factory_source_worker(
  worker: Arc<dyn super::FactorySourceRuntime>,
  cancellation: CancellationToken,
) -> JoinHandle<()> {
  let settings = worker.settings();
  let policy = settings.worker;
  let pass_cancel = cancellation.clone();
  spawn_worker_loop(
    WorkerIdentity::new("factory-source", WorkerKind::Factory).with_retry(policy.retry_policy()),
    Arc::new(WorkerHealth::new(
      "factory-source",
      policy.poll_interval().saturating_add(policy.claim_lifetime()),
    )),
    policy.poll_interval(),
    policy.claim_lifetime(),
    cancellation,
    move |_, _| {
      let worker = worker.clone();
      let cancellation = pass_cancel.clone();
      async move { worker.poll(cancellation).await }
    },
    |outcome| {
      if outcome.claimed > 0 {
        info!(
          claimed = outcome.claimed,
          completed = outcome.completed,
          deferred = outcome.deferred,
          "source polling advanced"
        );
      }
    },
  )
}

// Always scheduled, including when Source polling or Factory itself is disabled.
pub(super) fn spawn_source_input_retention_worker<S, B>(
  worker: octacity_server_application::FactorySourceInputRetentionWorker<S, B>,
  poll: Duration,
  claim: Duration,
  limit: u16,
  cancellation: CancellationToken,
) -> JoinHandle<()>
where
  S: octacity_server_store::WorkSourceArtifactStore + 'static,
  B: octacity_artifact_store::ArtifactStore + 'static,
{
  let worker = Arc::new(worker);
  let cursor = Arc::new(tokio::sync::Mutex::new(None));
  spawn_worker_loop(
    WorkerIdentity::new("factory-source-retention", WorkerKind::Factory),
    Arc::new(WorkerHealth::new(
      "factory-source-retention",
      poll.saturating_add(claim),
    )),
    poll,
    claim,
    cancellation,
    move |at, _| {
      let worker = worker.clone();
      let cursor = cursor.clone();
      async move {
        let mut cursor = cursor.lock().await;
        let result = worker.run_once(at, *cursor, limit).await?;
        *cursor = result.next_cursor;
        Ok::<_, octacity_server_application::ApplicationError>(result)
      }
    },
    |outcome| {
      if outcome.completed > 0 || outcome.poll_pages_expired > 0 || outcome.failed > 0 {
        info!(
          completed = outcome.completed,
          pages = outcome.poll_pages_expired,
          failed = outcome.failed,
          "expired source inputs reconciled"
        );
      }
    },
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test(start_paused = true)]
  async fn stopped_action_read_model_does_not_cancel_execution_workers() {
    for panic in [false, true] {
      let cancellation = CancellationToken::new();
      let stopped = tokio::spawn(async move {
        assert!(!panic, "injected index worker crash");
      });
      let isolated = isolate_read_model_worker(stopped, cancellation.child_token());
      let sibling_cancel = cancellation.child_token();
      let sibling = tokio::spawn(async move {
        sibling_cancel.cancelled().await;
      });
      let supervisor = spawn_worker_supervisor(
        vec![("actions", isolated), ("execution", sibling)],
        cancellation.clone(),
      );
      tokio::time::sleep(Duration::from_secs(1)).await;
      assert!(
        !cancellation.is_cancelled(),
        "index failure must not stop Factory execution"
      );
      assert!(!supervisor.is_finished());
      cancellation.cancel();
      assert!(supervisor.await.unwrap().is_ok());
    }
  }

  #[tokio::test(start_paused = true)]
  async fn factory_outbox_failures_have_independent_cooldown_and_shutdown_is_immediate() {
    struct Unavailable(tokio::sync::mpsc::UnboundedSender<tokio::time::Instant>);
    #[async_trait::async_trait]
    impl octacity_server_application::FactoryOutboxDispatcher for Unavailable {
      async fn dispatch_once(
        &self,
        _: Timestamp,
      ) -> Result<
        octacity_server_application::FactoryOutboxDispatchSummary,
        octacity_server_application::ApplicationError,
      > {
        self.0.send(tokio::time::Instant::now()).unwrap();
        Err(octacity_server_application::ApplicationError::Store(
          octacity_server_store::StoreError::Unavailable,
        ))
      }
    }
    let policy: crate::FactoryRuntimeConfig = toml::from_str("poll_interval_milliseconds=10\nclaim_lifetime_milliseconds=1000\nbatch_size=4\nconcurrency=2\n[retry]\nmax_backoff_milliseconds=40\nconsecutive_failure_limit=2\ncircuit_open_milliseconds=200").unwrap();
    let (events, mut observed) = tokio::sync::mpsc::unbounded_channel();
    let cancel = CancellationToken::new();
    let started = tokio::time::Instant::now();
    let task = spawn_factory_outbox_worker(
      Arc::new(Unavailable(events)),
      Arc::new(WorkerHealth::new("factory-outbox", Duration::from_secs(1))),
      policy,
      cancel.clone(),
    );
    assert_eq!(
      observed.recv().await.unwrap().duration_since(started),
      Duration::from_millis(10)
    );
    assert_eq!(
      observed.recv().await.unwrap().duration_since(started),
      Duration::from_millis(30)
    );
    tokio::time::advance(Duration::from_millis(100)).await;
    assert!(
      observed.try_recv().is_err(),
      "the open circuit must suppress dependency calls"
    );
    let stopped_at = tokio::time::Instant::now();
    cancel.cancel();
    task.await.unwrap();
    assert_eq!(
      tokio::time::Instant::now(),
      stopped_at,
      "shutdown must not wait for the cooldown"
    );
  }

  #[tokio::test(start_paused = true)]
  async fn factory_failures_back_off_open_a_circuit_and_reset_after_recovery() {
    let policy: crate::FactoryRuntimeConfig = toml::from_str("poll_interval_milliseconds=10\nclaim_lifetime_milliseconds=1000\nbatch_size=4\nconcurrency=2\n[retry]\nmax_backoff_milliseconds=40\nconsecutive_failure_limit=3\ncircuit_open_milliseconds=200").unwrap();
    let started = tokio::time::Instant::now();
    let (events, mut observed) = tokio::sync::mpsc::unbounded_channel();
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let cancel = CancellationToken::new();
    let health = Arc::new(WorkerHealth::new(FACTORY_WORKER_NAME, Duration::from_secs(1)));
    let task = spawn_worker_loop(
      WorkerIdentity::new(FACTORY_WORKER_NAME, WorkerKind::Factory).with_retry(policy.retry_policy()),
      health,
      policy.poll_interval(),
      policy.claim_lifetime(),
      cancel.clone(),
      move |_, _| {
        events.send(started.elapsed().as_millis()).unwrap();
        let index = count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        async move { if index < 3 { Err("unavailable") } else { Ok(()) } }
      },
      |_| {},
    );
    for expected in [10, 30, 70, 270, 280] {
      assert_eq!(observed.recv().await.unwrap(), expected);
    }
    cancel.cancel();
    task.await.unwrap();
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
  async fn factory_worker_errors_emit_only_safe_classification_and_never_provider_text() {
    const CHILD: &str = "OCTACITY_TEST_WORKER_LOG_CAPTURE_CHILD";
    if std::env::var_os(CHILD).is_none() {
      // Tracing dispatch/callsite state is process-wide. Isolate the production-style
      // global subscriber rather than serializing every unrelated worker test.
      let output = tokio::time::timeout(
        Duration::from_secs(30),
        tokio::process::Command::new(std::env::current_exe().unwrap())
          .args([
            "--exact",
            "runtime::workers::tests::factory_worker_errors_emit_only_safe_classification_and_never_provider_text",
            "--nocapture",
          ])
          .env(CHILD, "1")
          .kill_on_drop(true)
          .output(),
      )
      .await
      .expect("isolated worker logging check must finish")
      .unwrap();
      assert!(
        output.status.success(),
        "isolated worker logging check failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
      );
      return;
    }
    struct Capture(Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for Capture {
      fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
      }
      fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
      }
    }
    let output = Arc::new(std::sync::Mutex::new(Vec::new()));
    let writer = output.clone();
    let subscriber = tracing_subscriber::fmt()
      .without_time()
      .with_ansi(false)
      .with_writer(move || Capture(writer.clone()))
      .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let passes = calls.clone();
    let cancel = CancellationToken::new();
    let health = Arc::new(WorkerHealth::new(FACTORY_WORKER_NAME, Duration::from_secs(1)));
    let task = spawn_worker_loop(
      WorkerIdentity::new(FACTORY_WORKER_NAME, WorkerKind::Factory),
      health,
      Duration::from_millis(1),
      Duration::from_secs(1),
      cancel.clone(),
      move |_, _| {
        passes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        async {
          Err::<(), _>(
            "private-provider-token https://private.invalid/key private-prompt hidden-test-source /private/repository/file raw-command hidden-reasoning private-task-body private-provider-payload private-finding",
          )
        }
      },
      |_| {},
    );
    tokio::time::timeout(Duration::from_secs(1), async {
      while calls.load(std::sync::atomic::Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
      }
    })
    .await
    .unwrap();
    cancel.cancel();
    task.await.unwrap();
    let text = String::from_utf8(output.lock().unwrap().clone()).unwrap();
    assert!(
      text.contains("durable worker pass failed"),
      "captured {} log bytes",
      text.len()
    );
    assert!(text.contains("unavailable"));
    for private in [
      "private-provider-token",
      "private.invalid",
      "private-prompt",
      "hidden-test-source",
      "/private/repository",
      "raw-command",
      "hidden-reasoning",
      "private-task-body",
      "private-provider-payload",
      "private-finding",
    ] {
      assert!(!text.contains(private), "worker diagnostics must omit {private}");
    }
  }

  #[tokio::test]
  async fn configured_factory_outbox_dispatchers_run_without_invoking_the_flow_owner() {
    struct Dispatcher(std::sync::atomic::AtomicUsize);
    #[async_trait::async_trait]
    impl octacity_server_application::FactoryOutboxDispatcher for Dispatcher {
      async fn dispatch_once(
        &self,
        _: Timestamp,
      ) -> Result<
        octacity_server_application::FactoryOutboxDispatchSummary,
        octacity_server_application::ApplicationError,
      > {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(octacity_server_application::FactoryOutboxDispatchSummary {
          failed: 1,
          ..Default::default()
        })
      }
    }
    let dispatcher = Arc::new(Dispatcher(std::sync::atomic::AtomicUsize::new(0)));
    let cancel = CancellationToken::new();
    let health = Arc::new(WorkerHealth::new("factory-outbox", Duration::from_secs(1)));
    let worker = spawn_factory_outbox_worker(
      dispatcher.clone(),
      health,
      toml::from_str("poll_interval_milliseconds=10\nclaim_lifetime_milliseconds=1000\nbatch_size=4\nconcurrency=2")
        .unwrap(),
      cancel.clone(),
    );
    tokio::time::timeout(Duration::from_secs(1), async {
      while dispatcher.0.load(std::sync::atomic::Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
      }
    })
    .await
    .expect("configured outbox dispatchers must be supervised independently");
    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(1), worker)
      .await
      .unwrap()
      .unwrap();
  }

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
