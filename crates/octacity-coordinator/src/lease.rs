//! Cancellable long polling and independent active-lease heartbeats.

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use ed25519_dalek::VerifyingKey;
use octacity_protocol::{
  AcquireLeaseResponse, HeartbeatDirective, HostCapacity, HostSnapshot, JobSpecV1, LeaseAssignment,
};
use tokio::{sync::watch, task::JoinHandle, time::Instant};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::{CoordinatorError, Registration, SharedCoordinator, invalid, unix_now, verify_assignment};

/// Authenticated lease paired with its already decoded execution intent.
#[derive(Clone, Debug)]
pub struct VerifiedLease {
  /// Fencing and expiry values received from the coordinator.
  pub lease: LeaseAssignment,
  /// JobSpec whose signature and lease binding were verified.
  pub spec: JobSpecV1,
}

/// Terminal result of waiting for the next lease.
#[derive(Clone, Debug)]
pub enum LeasePollOutcome {
  /// One verified job attempt is ready for the job owner.
  Lease(Box<VerifiedLease>),
  /// The coordinator requested an idle agent to drain.
  Drain,
  /// No job was assigned; the caller should resample local admission state
  /// after waiting no longer than this server-provided delay.
  NoWork {
    /// Maximum delay before the agent asks the coordinator for work again.
    retry_after: Duration,
  },
}

/// Stateful long-poll loop that never exposes an unverified JobSpec.
pub struct LeasePoller {
  client: SharedCoordinator,
  registration: Registration,
  signing_keys: Arc<BTreeMap<String, VerifyingKey>>,
  poll_timeout: Duration,
  lease_safety_margin: Duration,
}

impl LeasePoller {
  /// Creates a poller with the keys and timing policy used for every assignment.
  pub fn new(
    client: SharedCoordinator,
    registration: Registration,
    signing_keys: Arc<BTreeMap<String, VerifyingKey>>,
    poll_timeout: Duration,
    lease_safety_margin: Duration,
  ) -> Result<Self, CoordinatorError> {
    registration.validate()?;
    if signing_keys.is_empty() || poll_timeout.as_secs() == 0 || lease_safety_margin.is_zero() {
      return Err(invalid("lease poller requires signing keys and non-zero timing policy"));
    }
    Ok(Self {
      client,
      registration,
      signing_keys,
      poll_timeout,
      lease_safety_margin,
    })
  }

  /// Performs one poll while declaring whether the agent can accept work.
  ///
  /// Returning no-work to the daemon is intentional: local disk admission is
  /// resampled between polls instead of becoming stale during an idle period.
  pub async fn next(
    &self,
    accept_jobs: bool,
    cancellation: CancellationToken,
  ) -> Result<LeasePollOutcome, CoordinatorError> {
    let response = self
      .client
      .acquire_lease(
        &self.registration,
        self.poll_timeout,
        self.lease_safety_margin,
        accept_jobs,
        cancellation,
      )
      .await?;
    match response {
      AcquireLeaseResponse::Lease { lease, .. } => {
        if !accept_jobs {
          return Err(invalid(
            "coordinator assigned a lease while the agent was not accepting jobs",
          ));
        }
        let lease = verify_assignment(lease, &self.signing_keys, unix_now()?, self.lease_safety_margin)?;
        info!(job_id = %lease.lease.job_id, attempt = lease.lease.attempt, lease_id = %lease.lease.lease_id, "acquired verified lease");
        Ok(LeasePollOutcome::Lease(Box::new(lease)))
      }
      AcquireLeaseResponse::Drain { .. } => Ok(LeasePollOutcome::Drain),
      AcquireLeaseResponse::NoWork { retry_after_ms, .. } => {
        if retry_after_ms == 0 {
          return Err(invalid("coordinator returned a zero no-work retry delay"));
        }
        let retry_after = Duration::from_millis(retry_after_ms).min(self.registration.max_retry_delay);
        debug!(?retry_after, accept_jobs, "coordinator long poll returned no work");
        Ok(LeasePollOutcome::NoWork { retry_after })
      }
    }
  }
}

/// Timing policy for an active lease heartbeat task.
#[derive(Clone, Debug)]
pub struct LeaseMonitorPolicy {
  /// Interval between independent heartbeat calls.
  pub heartbeat_interval: Duration,
  /// Time reserved to cancel before the current lease expiry.
  pub lease_safety_margin: Duration,
}

impl LeaseMonitorPolicy {
  /// Ensures heartbeats can run before the safety deadline.
  pub fn validate(&self) -> Result<(), CoordinatorError> {
    if self.heartbeat_interval.is_zero() || self.lease_safety_margin.is_zero() {
      return Err(invalid("heartbeat interval and lease safety margin must be non-zero"));
    }
    if self.heartbeat_interval >= self.lease_safety_margin {
      return Err(invalid("heartbeat interval must be shorter than lease safety margin"));
    }
    Ok(())
  }
}

/// Reason the heartbeat task stopped owning an active lease.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseMonitorOutcome {
  /// The coordinator explicitly cancelled the attempt.
  Cancelled,
  /// A newer fencing token owns the attempt.
  Fenced,
  /// Renewal did not complete before the safety deadline.
  Expired,
  /// Agent shutdown cancelled the active lease locally.
  Shutdown,
  /// The job owner stopped monitoring after job termination.
  Stopped,
}

/// Handle to an independent heartbeat task and its cancellation/drain signals.
pub struct LeaseMonitor {
  job_cancellation: CancellationToken,
  draining: watch::Receiver<bool>,
  stop: CancellationToken,
  join: Option<JoinHandle<LeaseMonitorOutcome>>,
}

impl LeaseMonitor {
  /// Starts heartbeats immediately and then at the configured interval.
  pub fn start(
    client: SharedCoordinator,
    registration: Registration,
    lease: LeaseAssignment,
    capacity: HostCapacity,
    snapshots: watch::Receiver<HostSnapshot>,
    policy: LeaseMonitorPolicy,
    shutdown: CancellationToken,
  ) -> Result<Self, CoordinatorError> {
    policy.validate()?;
    lease.validate(unix_now()?, policy.lease_safety_margin.as_secs())?;
    snapshots.borrow().validate(&capacity)?;
    let job_cancellation = CancellationToken::new();
    let stop = CancellationToken::new();
    let (drain_sender, draining) = watch::channel(false);
    let join = tokio::spawn(
      MonitorTask {
        client,
        registration,
        lease,
        capacity,
        snapshots,
        policy,
        shutdown,
        stop: stop.clone(),
        job_cancellation: job_cancellation.clone(),
        drain_sender,
      }
      .run(),
    );
    Ok(Self {
      job_cancellation,
      draining,
      stop,
      join: Some(join),
    })
  }

  /// Cancellation token passed to source acquisition and execution.
  pub fn job_cancellation(&self) -> CancellationToken {
    self.job_cancellation.clone()
  }

  /// Sticky drain flag; once set it never returns to false for this session.
  pub fn draining(&self) -> watch::Receiver<bool> {
    self.draining.clone()
  }

  /// Stops heartbeats after the job owner no longer needs the lease.
  pub fn stop(&self) {
    self.stop.cancel();
  }

  /// Waits for the terminal lease outcome.
  pub async fn wait(mut self) -> Result<LeaseMonitorOutcome, CoordinatorError> {
    let join = self.join.take().expect("lease monitor join handle is present");
    join
      .await
      .map_err(|error| invalid(format!("lease heartbeat task failed: {error}")))
  }
}

impl Drop for LeaseMonitor {
  fn drop(&mut self) {
    self.stop.cancel();
    if let Some(join) = &self.join {
      join.abort();
    }
  }
}

struct MonitorTask {
  client: SharedCoordinator,
  registration: Registration,
  lease: LeaseAssignment,
  capacity: HostCapacity,
  snapshots: watch::Receiver<HostSnapshot>,
  policy: LeaseMonitorPolicy,
  shutdown: CancellationToken,
  stop: CancellationToken,
  job_cancellation: CancellationToken,
  drain_sender: watch::Sender<bool>,
}

impl MonitorTask {
  async fn run(self) -> LeaseMonitorOutcome {
    let mut safety_deadline = match compute_safety_deadline(self.lease.expires_at, self.policy.lease_safety_margin) {
      Ok(deadline) => deadline,
      Err(()) => {
        self.job_cancellation.cancel();
        return LeaseMonitorOutcome::Expired;
      }
    };
    let mut interval = tokio::time::interval(self.policy.heartbeat_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut cancelled = false;
    loop {
      tokio::select! {
        () = self.shutdown.cancelled() => {
          self.job_cancellation.cancel();
          return LeaseMonitorOutcome::Shutdown;
        }
        () = self.stop.cancelled() => return if cancelled {
          LeaseMonitorOutcome::Cancelled
        } else {
          LeaseMonitorOutcome::Stopped
        },
        () = tokio::time::sleep_until(safety_deadline) => {
          warn!(lease_id = %self.lease.lease_id, "lease reached its safety deadline");
          self.job_cancellation.cancel();
          return LeaseMonitorOutcome::Expired;
        }
        _ = interval.tick() => {
          let snapshot = self.snapshots.borrow().clone();
          let heartbeat = self.client.heartbeat(
            &self.registration,
            &self.lease,
            &snapshot,
            &self.capacity,
            self.policy.lease_safety_margin,
            self.stop.clone(),
          );
          let directive = tokio::select! {
            () = self.shutdown.cancelled() => {
              self.job_cancellation.cancel();
              return LeaseMonitorOutcome::Shutdown;
            }
            () = self.stop.cancelled() => return if cancelled {
              LeaseMonitorOutcome::Cancelled
            } else {
              LeaseMonitorOutcome::Stopped
            },
            () = tokio::time::sleep_until(safety_deadline) => {
              self.job_cancellation.cancel();
              return LeaseMonitorOutcome::Expired;
            }
            result = heartbeat => result,
          };
          match directive {
            Ok(HeartbeatDirective::Continue { expires_at: renewed }) => {
              let Ok(renewed_deadline) = compute_safety_deadline(renewed, self.policy.lease_safety_margin) else {
                self.job_cancellation.cancel();
                return LeaseMonitorOutcome::Expired;
              };
              safety_deadline = renewed_deadline;
            }
            Ok(HeartbeatDirective::Drain { expires_at: renewed }) => {
              let Ok(renewed_deadline) = compute_safety_deadline(renewed, self.policy.lease_safety_margin) else {
                self.job_cancellation.cancel();
                return LeaseMonitorOutcome::Expired;
              };
              safety_deadline = renewed_deadline;
              self.drain_sender.send_replace(true);
            }
            Ok(HeartbeatDirective::Cancel) => {
              self.job_cancellation.cancel();
              cancelled = true;
            }
            Ok(HeartbeatDirective::Fenced) => {
              self.job_cancellation.cancel();
              return LeaseMonitorOutcome::Fenced;
            }
            Err(error) => {
              // A transient heartbeat failure is not itself proof of lease loss.
              // Keep trying until the monotonic safety deadline forces cancel.
              warn!(lease_id = %self.lease.lease_id, %error, "lease heartbeat failed");
            }
          }
        }
      }
    }
  }
}

fn compute_safety_deadline(expires_at: u64, margin: Duration) -> Result<Instant, ()> {
  let now = unix_now().map_err(|_| ())?;
  let remaining = expires_at.checked_sub(now).ok_or(())?;
  let remaining = Duration::from_secs(remaining).checked_sub(margin).ok_or(())?;
  if remaining.is_zero() {
    return Err(());
  }
  Instant::now().checked_add(remaining).ok_or(())
}
