//! Operator commands built on the validated component graph.
//!
//! This module owns the daemon-level lifecycle: startup recovery, registration,
//! idle lease polling, one-at-a-time job execution, and process shutdown. It
//! deliberately does not construct backends or parse configuration; those are
//! composition-root responsibilities. A failed job attempt is handled here
//! without automatically turning it into a failed agent process.

use std::{
  path::{Path, PathBuf},
  sync::Arc,
  time::Duration,
};

use octacity_config::AgentConfig;
use octacity_coordinator::{LeaseMonitorOutcome, LeasePollOutcome, LeasePoller};
use octacity_lifecycle::{
  JobLifecycle, JobLifecycleConfig, JobLifecycleError, JobLifecycleOutcome, RecoveredAttempt, SpoolLimits,
  cleanup_incomplete_attempts,
};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::composition::Components;

pub(crate) async fn validate(config: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
  info!(config = %config.display(), "validating agent configuration");
  let components = Components::load(&config).await?;
  println!(
    "agent '{}' configuration is valid (Octa {}, {} signing key(s), {} runtime mode(s), {} runtime route(s), {} source plugin(s))",
    components.validated.config.agent_id,
    components.runner.capabilities.octa_version,
    components.validated.signing_keys.len(),
    components.validated.config.enabled_runtime_modes.len(),
    components.inventory.runtimes.len(),
    components.source_plugin_count,
  );
  Ok(())
}

pub(crate) async fn run(config: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
  let mut components = Components::load(&config).await?;
  let shutdown = CancellationToken::new();
  let signal = shutdown.clone();
  tokio::spawn(async move {
    match shutdown_signal().await {
      Ok(()) => {
        info!("shutdown signal received; stopping lease acquisition");
        signal.cancel();
      }
      Err(error) => warn!(%error, "failed to install or receive a shutdown signal"),
    }
  });
  run_loaded(&mut components, shutdown).await
}

/// Runs the worker from an already constructed component graph. Keeping signal
/// installation outside makes the daemon policy independently testable while
/// concrete adapter selection remains confined to the composition root.
async fn run_loaded(
  components: &mut Components,
  shutdown: CancellationToken,
) -> Result<(), Box<dyn std::error::Error>> {
  components.executor.cleanup_orphans().await?;
  let recovered = cleanup_incomplete_attempts(&components.validated.config.state_root)?;
  log_recovered_attempts(&recovered);

  let coordinator = components.coordinator.clone();
  let registration = coordinator.register(&components.inventory, shutdown.clone()).await?;
  info!(registration_id = %registration.registration_id, "registered agent");
  let poller = LeasePoller::new(
    coordinator.clone(),
    registration.clone(),
    Arc::new(components.validated.signing_keys.clone()),
    Duration::from_secs(components.validated.config.poll_timeout_seconds),
    Duration::from_secs(components.validated.config.lease_safety_margin_seconds),
  )?;
  let lifecycle = JobLifecycle::new(
    coordinator,
    registration,
    components.executor.clone(),
    components.host.capacity().clone(),
    lifecycle_config(&components.validated.config),
  )?;

  loop {
    let lease = match poller.next(shutdown.clone()).await {
      Ok(LeasePollOutcome::Lease(lease)) => *lease,
      Ok(LeasePollOutcome::Drain) => {
        info!("coordinator drained idle agent");
        return Ok(());
      }
      Err(octacity_coordinator::CoordinatorError::Cancelled) if shutdown.is_cancelled() => return Ok(()),
      Err(error) => return Err(error.into()),
    };
    let snapshot = components.host.snapshot(None, components.backend_health.clone())?;
    match handle_lifecycle_result(
      lifecycle.run(lease, snapshot, shutdown.clone()).await,
      &components.validated.config.state_root,
    )? {
      WorkerDirective::Continue => {}
      WorkerDirective::Stop => return Ok(()),
    }
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkerDirective {
  Continue,
  Stop,
}

/// Applies daemon policy to one completed lifecycle without mixing it into the
/// attempt state machine. Fencing and expiry are scoped to an attempt; shutdown
/// and a sticky drain directive are scoped to the worker process.
fn handle_lifecycle_result(
  result: Result<JobLifecycleOutcome, JobLifecycleError>,
  state_root: &Path,
) -> Result<WorkerDirective, JobLifecycleError> {
  match result {
    Ok(outcome) if outcome.drain => {
      info!("completed active job and entered drain mode");
      Ok(WorkerDirective::Stop)
    }
    Ok(_) => Ok(WorkerDirective::Continue),
    Err(error @ JobLifecycleError::LeaseLost(LeaseMonitorOutcome::Shutdown)) => {
      warn!(%error, "active job stopped during agent shutdown");
      Ok(WorkerDirective::Stop)
    }
    Err(error @ JobLifecycleError::LeaseLost(LeaseMonitorOutcome::Fenced | LeaseMonitorOutcome::Expired)) => {
      // The lifecycle has already cancelled and cleaned execution resources.
      // Remove its retained diagnostic state before accepting another lease.
      warn!(%error, "active lease was lost; continuing to poll for work");
      let recovered = cleanup_incomplete_attempts(state_root)?;
      log_recovered_attempts(&recovered);
      Ok(WorkerDirective::Continue)
    }
    Err(error) => Err(error),
  }
}

/// Emits one bounded structured record per removed attempt. Attempt identifiers
/// are local hashes, so diagnostics never copy untrusted job text or event data.
fn log_recovered_attempts(attempts: &[RecoveredAttempt]) {
  for attempt in attempts {
    warn!(
      attempt_id = %attempt.attempt_id,
      last_state = ?attempt.last_state,
      unacknowledged_events = attempt.unacknowledged_events,
      completion_persisted = attempt.completion_persisted,
      "removed state for an interrupted attempt after resource cleanup"
    );
  }
}

/// Waits for the service-stop signals native to the current host platform.
async fn shutdown_signal() -> std::io::Result<()> {
  #[cfg(unix)]
  {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
      result = tokio::signal::ctrl_c() => result,
      signal = terminate.recv() => signal.map(|_| ()).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::BrokenPipe, "SIGTERM stream closed")
      }),
    }
  }
  #[cfg(not(unix))]
  {
    tokio::signal::ctrl_c().await
  }
}

fn lifecycle_config(config: &AgentConfig) -> JobLifecycleConfig {
  JobLifecycleConfig {
    state_root: config.state_root.clone(),
    event_channel_capacity: config.event_channel_capacity,
    event_retry_delay: Duration::from_millis(config.retry_initial_delay_milliseconds),
    spool: SpoolLimits {
      max_bytes: config.max_spool_bytes,
      max_records: config.max_spool_records,
      batch_bytes: config.event_batch_max_bytes,
      batch_records: config.event_batch_max_records,
    },
    lease_monitor: octacity_coordinator::LeaseMonitorPolicy {
      heartbeat_interval: Duration::from_secs(config.heartbeat_interval_seconds),
      lease_safety_margin: Duration::from_secs(config.lease_safety_margin_seconds),
    },
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use async_trait::async_trait;
  use octacity_coordinator::{CoordinatorClient, CoordinatorError, Registration};
  use octacity_protocol::{
    AcquireLeaseResponse, AgentInventory, AppendEventsResponse, AttemptEventEnvelope, CompleteLeaseRequest,
    HeartbeatDirective, HostCapacity, HostSnapshot, LeaseAssignment,
  };

  struct DrainCoordinator;

  #[async_trait]
  impl CoordinatorClient for DrainCoordinator {
    async fn register(
      &self,
      inventory: &AgentInventory,
      _cancellation: CancellationToken,
    ) -> Result<Registration, CoordinatorError> {
      Ok(Registration {
        agent_id: inventory.agent_id.clone(),
        registration_id: "registration-coverage".to_owned(),
        max_retry_delay: Duration::from_secs(1),
      })
    }

    async fn acquire_lease(
      &self,
      _registration: &Registration,
      _wait: Duration,
      _lease_safety_margin: Duration,
      _cancellation: CancellationToken,
    ) -> Result<AcquireLeaseResponse, CoordinatorError> {
      Ok(AcquireLeaseResponse::Drain {
        protocol_version: octacity_protocol::COORDINATOR_PROTOCOL_VERSION,
        request_id: "request-coverage".to_owned(),
      })
    }

    async fn heartbeat(
      &self,
      _registration: &Registration,
      _lease: &LeaseAssignment,
      _snapshot: &HostSnapshot,
      _capacity: &HostCapacity,
      _lease_safety_margin: Duration,
      _cancellation: CancellationToken,
    ) -> Result<HeartbeatDirective, CoordinatorError> {
      unreachable!("a drained worker has no active lease")
    }

    async fn append_events(
      &self,
      _registration: &Registration,
      _lease: &LeaseAssignment,
      _events: &[AttemptEventEnvelope],
      _cancellation: CancellationToken,
    ) -> Result<AppendEventsResponse, CoordinatorError> {
      unreachable!("a drained worker has no event stream")
    }

    async fn complete_lease(
      &self,
      _registration: &Registration,
      _lease: &LeaseAssignment,
      _completion: &CompleteLeaseRequest,
      _cancellation: CancellationToken,
    ) -> Result<(), CoordinatorError> {
      unreachable!("a drained worker cannot complete a lease")
    }
  }

  #[test]
  fn lifecycle_policy_keeps_the_daemon_alive_after_attempt_lease_loss() {
    let state = tempfile::tempdir().unwrap();
    assert_eq!(
      handle_lifecycle_result(
        Err(JobLifecycleError::LeaseLost(LeaseMonitorOutcome::Fenced)),
        state.path(),
      )
      .unwrap(),
      WorkerDirective::Continue
    );
    assert_eq!(
      handle_lifecycle_result(
        Err(JobLifecycleError::LeaseLost(LeaseMonitorOutcome::Expired)),
        state.path(),
      )
      .unwrap(),
      WorkerDirective::Continue
    );
    assert_eq!(
      handle_lifecycle_result(
        Err(JobLifecycleError::LeaseLost(LeaseMonitorOutcome::Shutdown)),
        state.path(),
      )
      .unwrap(),
      WorkerDirective::Stop
    );
  }

  #[test]
  fn lifecycle_policy_honors_drain_and_propagates_real_failures() {
    let state = tempfile::tempdir().unwrap();
    assert_eq!(
      handle_lifecycle_result(
        Ok(JobLifecycleOutcome {
          status: octacity_protocol::JobCompletionStatus::Succeeded,
          drain: true,
          last_event_sequence: 3,
        }),
        state.path(),
      )
      .unwrap(),
      WorkerDirective::Stop
    );
    assert_eq!(
      handle_lifecycle_result(
        Ok(JobLifecycleOutcome {
          status: octacity_protocol::JobCompletionStatus::Failed,
          drain: false,
          last_event_sequence: 4,
        }),
        state.path(),
      )
      .unwrap(),
      WorkerDirective::Continue
    );
    assert!(handle_lifecycle_result(Err(JobLifecycleError::Invalid("broken".to_owned())), state.path(),).is_err());
  }

  #[test]
  fn maps_all_lifecycle_limits_from_validated_agent_configuration() {
    let fixture = crate::composition::tests::installed_agent_fixture("https://coordinator.example");
    let validated = AgentConfig::load(&fixture.config).unwrap().validate().unwrap();
    let lifecycle = lifecycle_config(&validated.config);

    assert_eq!(lifecycle.state_root, validated.config.state_root);
    assert_eq!(lifecycle.event_channel_capacity, 8);
    assert_eq!(lifecycle.event_retry_delay, Duration::from_millis(1));
    assert_eq!(lifecycle.spool.max_bytes, 1_048_576);
    assert_eq!(lifecycle.spool.max_records, 32);
    assert_eq!(lifecycle.spool.batch_bytes, 16_384);
    assert_eq!(lifecycle.spool.batch_records, 8);
    assert_eq!(lifecycle.lease_monitor.heartbeat_interval, Duration::from_secs(1));
    assert_eq!(lifecycle.lease_monitor.lease_safety_margin, Duration::from_secs(2));
  }

  #[test]
  fn accepts_structured_recovery_diagnostics() {
    log_recovered_attempts(&[RecoveredAttempt {
      attempt_id: format!("attempt-{}", "a".repeat(64)),
      last_state: octacity_protocol::JobLifecycleState::Cleaning,
      unacknowledged_events: 2,
      completion_persisted: true,
    }]);
  }

  #[cfg(any(
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")),
    all(target_os = "macos", target_arch = "aarch64")
  ))]
  #[tokio::test]
  async fn validates_a_complete_installed_agent() {
    let fixture = crate::composition::tests::installed_agent_fixture("https://coordinator.example");
    validate(fixture.config).await.unwrap();
  }

  #[cfg(any(
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")),
    all(target_os = "macos", target_arch = "aarch64")
  ))]
  #[tokio::test]
  async fn cleans_registers_and_honors_an_idle_drain() {
    let fixture = crate::composition::tests::installed_agent_fixture("https://coordinator.example");
    let mut components = Components::load(&fixture.config).await.unwrap();
    components.coordinator = Arc::new(DrainCoordinator);

    run_loaded(&mut components, CancellationToken::new()).await.unwrap();
    assert!(!components.validated.config.state_root.join("jobs").exists());
  }
}
