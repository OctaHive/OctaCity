//! One-task job state machine plus an independent durable-delivery worker.
//!
//! `JobLifecycle` is the only owner of phase transitions for a leased attempt.
//! It coordinates an already verified `JobSpec`, the transport-independent job
//! executor, the lease monitor, and the local event pipeline. Execution
//! backends do not know about leases or HTTP, and the delivery worker cannot
//! mutate job state. This module deliberately uses private concrete helpers
//! instead of extension traits because there is one lifecycle implementation.

use std::{collections::BTreeMap, fs, path::PathBuf, sync::Arc, time::Duration};

use octacity_coordinator::{
  CoordinatorClient, CoordinatorError, LeaseMonitor, LeaseMonitorOutcome, LeaseMonitorPolicy, Registration,
  VerifiedLease,
};
use octacity_execution::ResourceUsage;
use octacity_job::{ExecuteJobRequest, JobError, JobExecutor};
use octacity_protocol::{
  ActiveJob, AgentLifecycleEvent, AttemptEventKind, CompleteLeaseRequest, HostCapacity, HostSnapshot,
  JobCompletionStatus, JobLifecycleState, LeaseFence, ResourceUsageSnapshot, RunnerEventPayload,
};
use octacity_runner::{RunStatus, RunnerStreamItem};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use tokio::sync::{Notify, mpsc, watch};
use tokio_util::sync::CancellationToken;

#[path = "attempt_events.rs"]
mod attempt_events;
#[path = "recovery.rs"]
mod recovery;
#[path = "terminal.rs"]
mod terminal;

pub use recovery::{RecoveredAttempt, cleanup_incomplete_attempts};

use crate::{
  delivery::DeliveryTask,
  spool::{SpoolError, SpoolLimits},
};
use attempt_events::AttemptEventState;
use terminal::TerminalAttempt;

/// Durable-delivery and heartbeat policy for one leased attempt.
#[derive(Clone, Debug)]
pub struct JobLifecycleConfig {
  /// Existing private root for attempt journals and event spools.
  pub state_root: PathBuf,
  /// Bounded runner-to-lifecycle event channel capacity.
  pub event_channel_capacity: usize,
  /// Delay before replaying an append after a coordinator failure.
  pub event_retry_delay: Duration,
  /// Persistent event spool and append-batch limits.
  pub spool: SpoolLimits,
  /// Independent heartbeat interval and lease safety margin.
  pub lease_monitor: LeaseMonitorPolicy,
}

impl JobLifecycleConfig {
  /// Validates durable-delivery and heartbeat policy before acquiring work.
  pub fn validate(&self) -> Result<(), JobLifecycleError> {
    if !self.state_root.is_absolute() || !self.state_root.is_dir() {
      return Err(JobLifecycleError::Invalid(
        "state_root must be an existing absolute directory".to_owned(),
      ));
    }
    if self.event_channel_capacity == 0 || self.event_retry_delay.is_zero() {
      return Err(JobLifecycleError::Invalid(
        "event channel capacity and retry delay must be greater than zero".to_owned(),
      ));
    }
    self.spool.validate()?;
    self.lease_monitor.validate()?;
    Ok(())
  }
}

/// Successfully acknowledged terminal result for one attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobLifecycleOutcome {
  /// Terminal status acknowledged by the coordinator.
  pub status: JobCompletionStatus,
  /// Sticky drain directive observed while this attempt was active.
  pub drain: bool,
  /// Last globally ordered event included in terminal completion.
  pub last_event_sequence: u64,
}

#[derive(Debug, Error)]
/// Configuration, execution, lease, durability, or cleanup failure.
pub enum JobLifecycleError {
  /// Local lifecycle policy is unusable.
  #[error("invalid job lifecycle configuration: {0}")]
  Invalid(String),
  /// Coordinator transport or protocol operation failed.
  #[error(transparent)]
  Coordinator(#[from] CoordinatorError),
  /// Durable event spool operation failed.
  #[error(transparent)]
  Spool(#[from] SpoolError),
  /// Attempt journal or completion-state filesystem operation failed.
  #[error("job state I/O failed: {0}")]
  StateIo(#[source] std::io::Error),
  /// Persistent attempt state could not be encoded.
  #[error("job state JSON failed: {0}")]
  StateJson(#[source] serde_json::Error),
  /// Internal lifecycle code attempted an illegal phase transition.
  #[error("invalid job lifecycle transition from {from:?} to {to:?}")]
  InvalidTransition {
    /// Last state durably recorded in the attempt journal.
    from: Option<JobLifecycleState>,
    /// State rejected by the journal transition table.
    to: JobLifecycleState,
  },
  /// An owned asynchronous task failed unexpectedly.
  #[error("job task failed: {0}")]
  Join(#[source] tokio::task::JoinError),
  /// Lease ownership ended before fenced terminal acknowledgement.
  #[error("lease ownership ended before durable completion: {0:?}")]
  LeaseLost(LeaseMonitorOutcome),
  /// Backend or workspace cleanup could not be confirmed.
  #[error("job cleanup was not confirmed: {0}")]
  Cleanup(String),
}

/// Dependencies that run one verified lease without exposing transport to the executor.
pub struct JobLifecycle {
  coordinator: Arc<dyn CoordinatorClient>,
  registration: Registration,
  executor: Arc<JobExecutor>,
  capacity: HostCapacity,
  config: JobLifecycleConfig,
}

impl JobLifecycle {
  /// Constructs a lifecycle owner from already validated concrete components.
  pub fn new(
    coordinator: Arc<dyn CoordinatorClient>,
    registration: Registration,
    executor: Arc<JobExecutor>,
    capacity: HostCapacity,
    config: JobLifecycleConfig,
  ) -> Result<Self, JobLifecycleError> {
    registration.validate()?;
    capacity.validate().map_err(CoordinatorError::from)?;
    config.validate()?;
    Ok(Self {
      coordinator,
      registration,
      executor,
      capacity,
      config,
    })
  }

  /// Executes, cleans, durably reports, and completes one verified lease.
  pub async fn run(
    &self,
    verified: VerifiedLease,
    mut snapshot: HostSnapshot,
    shutdown: CancellationToken,
  ) -> Result<JobLifecycleOutcome, JobLifecycleError> {
    let lease = verified.lease;
    let fence = LeaseFence::from(&lease);
    let attempt_root = create_attempt_root(&self.config.state_root, &fence)?;
    let (mut attempt_events, delivery_progress) =
      AttemptEventState::create(&attempt_root, &lease, self.config.spool.clone())?;
    let delivery_stop = CancellationToken::new();
    snapshot.active_job = Some(ActiveJob {
      job_id: lease.job_id.clone(),
      attempt: lease.attempt,
      lease_id: lease.lease_id.clone(),
      resource_usage: None,
    });
    snapshot.validate(&self.capacity).map_err(CoordinatorError::from)?;
    let (snapshot_sender, snapshots) = watch::channel(snapshot);
    let monitor = LeaseMonitor::start(
      self.coordinator.clone(),
      self.registration.clone(),
      lease.clone(),
      self.capacity.clone(),
      snapshots,
      self.config.lease_monitor.clone(),
      shutdown,
    )?;
    let cancellation = monitor.job_cancellation();
    let draining = monitor.draining();
    let mut monitor_task = tokio::spawn(monitor.wait());
    let delivery = DeliveryTask {
      coordinator: self.coordinator.clone(),
      registration: self.registration.clone(),
      lease: lease.clone(),
      spool: attempt_events.spool.clone(),
      work_available: attempt_events.work_available.clone(),
      progress: delivery_progress,
      retry_delay: self.config.event_retry_delay,
      stop: delivery_stop.clone(),
    }
    .spawn();
    let mut delivery = delivery;
    let mut delivery_finished = false;
    attempt_events.work_available.notify_one();
    let (event_sender, mut events) = mpsc::channel(self.config.event_channel_capacity);
    let executor = self.executor.clone();
    let spec = verified.spec;
    let job_cancellation = cancellation.clone();
    let mut job_task = tokio::spawn(async move {
      executor
        .execute(
          ExecuteJobRequest {
            spec,
            source_credentials: BTreeMap::new(),
          },
          job_cancellation,
          &event_sender,
        )
        .await
    });
    let mut lease_outcome = None;
    let mut lifecycle_error = None;
    let job_result = loop {
      tokio::select! {
        item = events.recv() => {
          if let Some(item) = item
            && lifecycle_error.is_none()
            && let Err(error) = attempt_events.record_runner_item(item, &snapshot_sender, &cancellation).await
          {
            cancellation.cancel();
            lifecycle_error = Some(error);
          }
        }
        result = &mut job_task => match result {
          Ok(result) => break Some(result),
          Err(error) => {
            lifecycle_error = Some(JobLifecycleError::Join(error));
            break None;
          }
        },
        outcome = &mut monitor_task, if lease_outcome.is_none() => {
          let outcome = outcome.map_err(JobLifecycleError::Join)??;
          lease_outcome = Some(outcome);
        }
        result = &mut delivery, if !delivery_finished => {
          delivery_finished = true;
          let error = delivery_failure(result);
          cancellation.cancel();
          lifecycle_error = Some(error);
        }
      }
    };
    // A runner result and a delivery failure can become ready in the same
    // scheduler turn. Observe the delivery handle once more before entering
    // terminalization so a permanent server rejection keeps its real cause.
    if !delivery_finished && delivery.is_finished() {
      delivery_finished = true;
      let error = delivery_failure((&mut delivery).await);
      lifecycle_error.get_or_insert(error);
    }
    while lifecycle_error.is_none()
      && let Ok(item) = events.try_recv()
    {
      if let Err(error) = attempt_events
        .record_runner_item(item, &snapshot_sender, &cancellation)
        .await
      {
        lifecycle_error = Some(error);
      }
    }

    let (status, final_usage, results, cleanup) = match job_result {
      Some(Ok(completion)) => {
        if !attempt_events.running {
          let result = attempt_events
            .transition(JobLifecycleState::Running, &delivery_stop)
            .await;
          if let Err(error) = result {
            lifecycle_error.get_or_insert(error);
          }
        }
        if lifecycle_error.is_none()
          && let Err(error) = attempt_events
            .transition(JobLifecycleState::Freezing, &delivery_stop)
            .await
        {
          lifecycle_error = Some(error);
        }
        let status = runner_status(completion.runner().status);
        let usage = Some(resource_snapshot(&completion.runner().final_usage));
        let results = completion.runner().results.clone();
        (
          status,
          usage,
          results,
          completion.cleanup().await.map_err(|error| error.to_string()),
        )
      }
      Some(Err(error)) => (
        job_error_status(&error),
        None,
        Vec::new(),
        if cleanup_confirmed(&error) {
          Ok(())
        } else {
          Err(error.to_string())
        },
      ),
      None => (
        JobCompletionStatus::InfrastructureFailed,
        None,
        Vec::new(),
        self.executor.cleanup_orphans().await.map_err(|error| error.to_string()),
      ),
    };
    if lifecycle_error.is_none()
      && let Err(error) = attempt_events
        .transition(JobLifecycleState::Cleaning, &delivery_stop)
        .await
    {
      lifecycle_error = Some(error);
    }
    if let Err(error) = cleanup {
      lifecycle_error = Some(JobLifecycleError::Cleanup(error));
    }

    if let Some(error) = lifecycle_error {
      let _ = stop_delivery(
        &delivery_stop,
        &attempt_events.work_available,
        &mut delivery,
        &mut delivery_finished,
      )
      .await;
      monitor_task.abort();
      return Err(error);
    }

    TerminalAttempt {
      lifecycle: self,
      lease,
      fence,
      attempt_root,
      events: attempt_events,
      delivery_stop,
      delivery,
      delivery_finished,
      monitor: monitor_task,
      lease_outcome,
      draining,
    }
    .complete(status, final_usage, results)
    .await
  }
}

/// Stops the event worker once and preserves a worker error when no earlier
/// lifecycle failure already owns the return path.
async fn stop_delivery(
  stop: &CancellationToken,
  work_available: &Notify,
  delivery: &mut tokio::task::JoinHandle<Result<(), JobLifecycleError>>,
  finished: &mut bool,
) -> Result<(), JobLifecycleError> {
  stop.cancel();
  work_available.notify_waiters();
  if *finished {
    return Ok(());
  }
  *finished = true;
  delivery.await.map_err(JobLifecycleError::Join)?
}

/// Converts delivery completion observed before an intentional stop into the
/// single failure value owned by the lifecycle state machine.
fn delivery_failure(result: Result<Result<(), JobLifecycleError>, tokio::task::JoinError>) -> JobLifecycleError {
  match result {
    Ok(Ok(())) => JobLifecycleError::Invalid("event delivery stopped before lifecycle shutdown".to_owned()),
    Ok(Err(error)) => error,
    Err(error) => JobLifecycleError::Join(error),
  }
}

fn stream_kind(item: RunnerStreamItem) -> AttemptEventKind {
  match item {
    RunnerStreamItem::Event(event) => AttemptEventKind::Runner {
      event: RunnerEventPayload {
        schema_version: event.schema_version,
        sequence: event.sequence,
        timestamp: event.timestamp,
        category: event.category,
        data: event.data,
      },
    },
    RunnerStreamItem::ResourceUsage(usage) => AttemptEventKind::Agent {
      event: AgentLifecycleEvent::ResourceUsage {
        usage: resource_snapshot(&usage),
      },
    },
    RunnerStreamItem::AccountingUnavailable { consecutive_failures } => AttemptEventKind::Agent {
      event: AgentLifecycleEvent::AccountingUnavailable { consecutive_failures },
    },
  }
}

fn resource_snapshot(usage: &ResourceUsage) -> ResourceUsageSnapshot {
  ResourceUsageSnapshot {
    observed_at_unix_ms: unix_now_millis(),
    elapsed_ms: usage.elapsed_ms,
    cpu_time_ms: usage.cpu_time_ms,
    memory_current_bytes: usage.memory_current_bytes,
    memory_peak_bytes: usage.memory_peak_bytes,
    disk_current_bytes: usage.disk_current_bytes,
    disk_peak_bytes: usage.disk_peak_bytes,
    io_read_bytes: usage.io_read_bytes,
    io_written_bytes: usage.io_written_bytes,
    network_received_bytes: usage.network_received_bytes,
    network_transmitted_bytes: usage.network_transmitted_bytes,
  }
}

fn runner_status(status: RunStatus) -> JobCompletionStatus {
  match status {
    RunStatus::Succeeded => JobCompletionStatus::Succeeded,
    RunStatus::Failed => JobCompletionStatus::Failed,
    RunStatus::Cancelled => JobCompletionStatus::Cancelled,
  }
}

fn job_error_status(error: &JobError) -> JobCompletionStatus {
  match error {
    JobError::Cancelled => JobCompletionStatus::Cancelled,
    JobError::TimedOut => JobCompletionStatus::TimedOut,
    _ => JobCompletionStatus::InfrastructureFailed,
  }
}

fn cleanup_confirmed(error: &JobError) -> bool {
  !matches!(
    error,
    JobError::OperationAndCleanup { .. } | JobError::Cleanup(_) | JobError::OrphanCleanup(_)
  )
}

fn fatal_lease(outcome: &LeaseMonitorOutcome) -> bool {
  matches!(
    outcome,
    LeaseMonitorOutcome::Fenced | LeaseMonitorOutcome::Expired | LeaseMonitorOutcome::Shutdown
  )
}

fn create_attempt_root(state_root: &std::path::Path, fence: &LeaseFence) -> Result<PathBuf, JobLifecycleError> {
  let jobs = state_root.join("jobs");
  create_private_directory(&jobs, false)?;
  let root = jobs.join(attempt_directory(fence));
  create_private_directory(&root, true)?;
  Ok(root)
}

fn create_private_directory(path: &std::path::Path, exclusive: bool) -> Result<(), JobLifecycleError> {
  let mut builder = fs::DirBuilder::new();
  builder.recursive(!exclusive);
  #[cfg(unix)]
  {
    use std::os::unix::fs::DirBuilderExt as _;
    builder.mode(0o700);
  }
  builder.create(path).map_err(JobLifecycleError::StateIo)
}

fn attempt_directory(fence: &LeaseFence) -> String {
  let mut digest = Sha256::new();
  digest.update(fence.lease_id.as_bytes());
  digest.update([0]);
  digest.update(fence.fencing_token.as_bytes());
  format!("attempt-{:x}", digest.finalize())
}

fn is_attempt_directory(name: &str) -> bool {
  name.len() == 72
    && name.starts_with("attempt-")
    && name[8..]
      .bytes()
      .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn completion_id(fence: &LeaseFence) -> String {
  format!("completion-{}", &attempt_directory(fence)[8..])
}

fn persist_completion(path: &std::path::Path, completion: &CompleteLeaseRequest) -> Result<(), JobLifecycleError> {
  completion.validate().map_err(CoordinatorError::from)?;
  let file = fs::OpenOptions::new()
    .create_new(true)
    .write(true)
    .open(path.join("completion.json"))
    .map_err(JobLifecycleError::StateIo)?;
  serde_json::to_writer(&file, completion).map_err(JobLifecycleError::StateJson)?;
  file.sync_data().map_err(JobLifecycleError::StateIo)
}

fn unix_now_millis() -> u64 {
  u64::try_from(
    std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .unwrap_or_default()
      .as_millis(),
  )
  .unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
