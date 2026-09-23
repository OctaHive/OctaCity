use std::sync::{
  Arc, Mutex,
  atomic::{AtomicBool, Ordering},
};

use async_trait::async_trait;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

mod dependencies;

pub use dependencies::ReadinessSetupError;
pub(crate) use dependencies::RuntimeDependencies;

/// One bounded check required before the server may accept mutations.
///
/// Implementations must not expose credentials in diagnostics. The runtime
/// applies one aggregate deadline to the complete set of checks.
#[async_trait]
pub(crate) trait ReadinessCheck: Send + Sync {
  /// Stable non-sensitive dependency name used in transition diagnostics.
  fn name(&self) -> &'static str;

  /// Returns whether the dependency can currently support safe mutations.
  async fn check(&self) -> bool;
}

type SharedCheck = Arc<dyn ReadinessCheck>;

/// Complete set of dependencies that gate server readiness.
///
/// Migrations, PostgreSQL, object storage, and signing material are always
/// required. Secret-provider and worker checks are required when their
/// adapters or workers are configured by the composition root.
pub(crate) struct ReadinessChecks {
  checks: Vec<SharedCheck>,
}

impl ReadinessChecks {
  /// Creates the readiness set with universal dependencies plus every
  /// composition-selected secret-provider and supervised-worker check.
  #[must_use]
  pub(crate) fn new(
    migrations: SharedCheck,
    postgres: SharedCheck,
    object_storage: SharedCheck,
    signing_material: SharedCheck,
    additional_required: impl IntoIterator<Item = SharedCheck>,
  ) -> Self {
    let mut checks = vec![migrations, postgres, object_storage, signing_material];
    checks.extend(additional_required);
    Self { checks }
  }

  /// Adds one composition-selected supervised worker to the readiness gate.
  pub(crate) fn push(&mut self, check: SharedCheck) {
    self.checks.push(check);
  }

  async fn evaluate(&self, timeout: std::time::Duration) -> ReadinessEvaluation {
    let deadline = tokio::time::Instant::now() + timeout;
    for check in &self.checks {
      let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
      if remaining.is_zero() {
        return ReadinessEvaluation::failed(check.name(), ReadinessFailureKind::TimedOut);
      }
      match tokio::time::timeout(remaining, check.check()).await {
        Ok(true) => {}
        Ok(false) => return ReadinessEvaluation::failed(check.name(), ReadinessFailureKind::Unavailable),
        Err(_) => return ReadinessEvaluation::failed(check.name(), ReadinessFailureKind::TimedOut),
      }
    }
    ReadinessEvaluation::READY
  }
}

/// Mutable health signal owned by one supervised background worker.
pub(crate) struct WorkerHealth {
  name: &'static str,
  healthy: AtomicBool,
  last_success: Mutex<Option<std::time::Instant>>,
  stale_after: std::time::Duration,
}

impl WorkerHealth {
  pub(crate) fn new(name: &'static str, stale_after: std::time::Duration) -> Self {
    Self {
      name,
      healthy: AtomicBool::new(false),
      last_success: Mutex::new(None),
      stale_after,
    }
  }

  pub(crate) fn mark_success(&self) {
    if let Ok(mut last_success) = self.last_success.lock() {
      *last_success = Some(std::time::Instant::now());
      self.healthy.store(true, Ordering::Release);
    } else {
      self.healthy.store(false, Ordering::Release);
    }
  }

  pub(crate) fn mark_failure(&self) {
    self.healthy.store(false, Ordering::Release);
  }
}

#[async_trait]
impl ReadinessCheck for WorkerHealth {
  fn name(&self) -> &'static str {
    self.name
  }

  async fn check(&self) -> bool {
    self.healthy.load(Ordering::Acquire)
      && self
        .last_success
        .lock()
        .ok()
        .and_then(|last_success| *last_success)
        .is_some_and(|last_success| last_success.elapsed() <= self.stale_after)
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReadinessEvaluation {
  failure: Option<ReadinessFailure>,
}

impl ReadinessEvaluation {
  const READY: Self = Self { failure: None };

  const fn failed(dependency: &'static str, kind: ReadinessFailureKind) -> Self {
    Self {
      failure: Some(ReadinessFailure { dependency, kind }),
    }
  }

  const fn is_ready(self) -> bool {
    self.failure.is_none()
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReadinessFailure {
  dependency: &'static str,
  kind: ReadinessFailureKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReadinessFailureKind {
  Unavailable,
  TimedOut,
}

pub(crate) struct ReadinessMonitor {
  state: Arc<ReadinessState>,
  task: JoinHandle<()>,
}

impl ReadinessMonitor {
  pub(crate) async fn start(
    checks: ReadinessChecks,
    interval: std::time::Duration,
    timeout: std::time::Duration,
    cancellation: CancellationToken,
  ) -> Self {
    let state = Arc::new(ReadinessState::default());
    let mut evaluation = checks.evaluate(timeout).await;
    state.set(evaluation.is_ready());
    report_transition(None, evaluation);
    let task_state = state.clone();
    let task = tokio::spawn(async move {
      let _unready_on_exit = UnreadyOnDrop(task_state.clone());
      loop {
        tokio::select! {
          biased;
          () = cancellation.cancelled() => {
            task_state.set(false);
            return;
          }
          () = tokio::time::sleep(interval) => {
            let next = tokio::select! {
              biased;
              () = cancellation.cancelled() => {
                task_state.set(false);
                return;
              }
              evaluation = checks.evaluate(timeout) => evaluation,
            };
            if next != evaluation {
              report_transition(Some(evaluation), next);
              evaluation = next;
            }
            task_state.set(next.is_ready());
          }
        }
      }
    });
    Self { state, task }
  }

  pub(crate) fn state(&self) -> Arc<ReadinessState> {
    self.state.clone()
  }

  pub(crate) fn into_task(self) -> JoinHandle<()> {
    self.task
  }
}

fn report_transition(previous: Option<ReadinessEvaluation>, current: ReadinessEvaluation) {
  match current.failure {
    None => tracing::info!(
      recovered = previous.is_some_and(|status| !status.is_ready()),
      "server readiness established"
    ),
    Some(failure) => tracing::warn!(
      dependency = failure.dependency,
      reason = ?failure.kind,
      "server readiness lost"
    ),
  }
}

struct UnreadyOnDrop(Arc<ReadinessState>);

impl Drop for UnreadyOnDrop {
  fn drop(&mut self) {
    self.0.set(false);
  }
}

#[derive(Default)]
pub(crate) struct ReadinessState {
  ready: AtomicBool,
}

impl ReadinessState {
  pub(crate) fn is_ready(&self) -> bool {
    self.ready.load(Ordering::Acquire)
  }

  pub(crate) fn set(&self, ready: bool) {
    self.ready.store(ready, Ordering::Release);
  }
}

#[cfg(test)]
mod tests {
  use std::sync::atomic::AtomicBool;

  use super::*;

  struct ControlledCheck {
    name: &'static str,
    healthy: AtomicBool,
  }

  struct PendingCheck;

  #[async_trait]
  impl ReadinessCheck for PendingCheck {
    fn name(&self) -> &'static str {
      "pending-worker"
    }

    async fn check(&self) -> bool {
      std::future::pending().await
    }
  }

  #[async_trait]
  impl ReadinessCheck for ControlledCheck {
    fn name(&self) -> &'static str {
      self.name
    }

    async fn check(&self) -> bool {
      self.healthy.load(Ordering::Acquire)
    }
  }

  fn controlled(name: &'static str, initial: bool) -> Arc<ControlledCheck> {
    Arc::new(ControlledCheck {
      name,
      healthy: AtomicBool::new(initial),
    })
  }

  async fn wait_for(state: &ReadinessState, expected: bool) {
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
      while state.is_ready() != expected {
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
      }
    })
    .await
    .expect("readiness state did not converge");
  }

  #[tokio::test]
  async fn evaluation_identifies_the_failed_dependency_and_reason() {
    let unavailable = controlled("object-storage", false);
    let checks = ReadinessChecks::new(
      controlled("migrations", true),
      controlled("postgres", true),
      unavailable,
      controlled("signing-material", true),
      std::iter::empty(),
    );

    assert_eq!(
      checks.evaluate(std::time::Duration::from_secs(1)).await,
      ReadinessEvaluation::failed("object-storage", ReadinessFailureKind::Unavailable)
    );
  }

  #[tokio::test]
  async fn evaluation_attributes_the_aggregate_deadline_to_the_active_check() {
    let checks = ReadinessChecks::new(
      controlled("migrations", true),
      controlled("postgres", true),
      Arc::new(PendingCheck),
      controlled("signing-material", true),
      std::iter::empty(),
    );

    assert_eq!(
      checks.evaluate(std::time::Duration::from_millis(5)).await,
      ReadinessEvaluation::failed("pending-worker", ReadinessFailureKind::TimedOut)
    );
  }

  #[tokio::test]
  async fn worker_health_requires_a_recent_successful_pass() {
    let health = WorkerHealth::new("test-worker", std::time::Duration::from_millis(5));
    assert_eq!(health.name(), "test-worker");
    assert!(
      !health.check().await,
      "a worker must not be healthy before its first pass"
    );

    health.mark_success();
    assert!(health.check().await);
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    assert!(!health.check().await, "a stalled worker must eventually lose readiness");

    health.mark_success();
    assert!(health.check().await);
    health.mark_failure();
    assert!(!health.check().await);
  }

  #[tokio::test]
  async fn configured_dependencies_and_workers_remove_and_restore_readiness() {
    let healthy = |name| controlled(name, true) as SharedCheck;
    let secret_provider = controlled("secret-provider", true);
    let worker = controlled("worker", true);
    let checks = ReadinessChecks::new(
      healthy("migrations"),
      healthy("postgres"),
      healthy("object-storage"),
      healthy("signing-material"),
      [secret_provider.clone() as SharedCheck, worker.clone() as SharedCheck],
    );
    let cancellation = CancellationToken::new();
    let monitor = ReadinessMonitor::start(
      checks,
      std::time::Duration::from_millis(2),
      std::time::Duration::from_millis(50),
      cancellation.clone(),
    )
    .await;
    let state = monitor.state();
    assert!(state.is_ready());

    secret_provider.healthy.store(false, Ordering::Release);
    wait_for(&state, false).await;
    secret_provider.healthy.store(true, Ordering::Release);
    wait_for(&state, true).await;
    worker.healthy.store(false, Ordering::Release);
    wait_for(&state, false).await;
    worker.healthy.store(true, Ordering::Release);
    wait_for(&state, true).await;

    cancellation.cancel();
    monitor.into_task().await.unwrap();
    assert!(!state.is_ready());
  }
}
