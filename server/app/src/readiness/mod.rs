use std::sync::{
  Arc, Mutex,
  atomic::{AtomicBool, Ordering},
};

use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_store::{
  CriticalSystemConditionChange, CriticalSystemConditionSourceId, CriticalSystemConditionStore,
};
use sha2::{Digest as _, Sha256};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

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
type CriticalConditionSink = Arc<dyn CriticalSystemConditionStore>;

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
    critical_conditions: Option<CriticalConditionSink>,
    cancellation: CancellationToken,
  ) -> Self {
    let critical_condition_source = CriticalSystemConditionSourceId::from_uuid(uuid::Uuid::new_v4())
      .expect("a random UUID is a valid critical-condition source identity");
    let state = Arc::new(ReadinessState::default());
    let mut evaluation = checks.evaluate(timeout).await;
    state.set(evaluation.is_ready());
    report_transition(
      None,
      evaluation,
      critical_condition_source,
      critical_conditions.as_ref(),
      timeout,
    )
    .await;
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
            task_state.set(next.is_ready());
            if next != evaluation {
              report_transition(
                Some(evaluation),
                next,
                critical_condition_source,
                critical_conditions.as_ref(),
                timeout,
              ).await;
              evaluation = next;
            }
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

async fn report_transition(
  previous: Option<ReadinessEvaluation>,
  current: ReadinessEvaluation,
  source: CriticalSystemConditionSourceId,
  critical_conditions: Option<&CriticalConditionSink>,
  timeout: std::time::Duration,
) {
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

  let Some(critical_conditions) = critical_conditions else {
    return;
  };
  let Some(observed_at) = current_timestamp() else {
    tracing::warn!("could not timestamp server readiness attention transition");
    return;
  };
  let previous_failure = previous.and_then(|status| status.failure);
  if let Some(failure) = previous_failure.filter(|failure| Some(*failure) != current.failure) {
    record_condition(
      critical_conditions,
      CriticalSystemConditionChange::resolve(source, condition_code(failure), observed_at),
      timeout,
    )
    .await;
  }
  if let Some(failure) = current.failure.filter(|failure| Some(*failure) != previous_failure) {
    let reason = match failure.kind {
      ReadinessFailureKind::Unavailable => "unavailable",
      ReadinessFailureKind::TimedOut => "timed out",
    };
    record_condition(
      critical_conditions,
      CriticalSystemConditionChange::open(
        source,
        condition_code(failure),
        format!("Server readiness lost: {} is {reason}", failure.dependency),
        observed_at,
      ),
      timeout,
    )
    .await;
  }
}

async fn record_condition(
  sink: &CriticalConditionSink,
  change: Result<CriticalSystemConditionChange, octacity_server_store::OperatorAttentionValueError>,
  timeout: std::time::Duration,
) {
  let Ok(change) = change else {
    tracing::warn!("server readiness attention transition was not safe to persist");
    return;
  };
  match tokio::time::timeout(timeout, sink.record_critical_system_condition(change)).await {
    Ok(Ok(())) => {}
    Ok(Err(_)) => tracing::warn!("could not persist server readiness attention transition"),
    Err(_) => tracing::warn!("persisting server readiness attention transition timed out"),
  }
}

fn condition_code(failure: ReadinessFailure) -> String {
  let mut digest = Sha256::new();
  digest.update(b"octacity-server-readiness-v1\0");
  digest.update(failure.dependency.as_bytes());
  digest.update([match failure.kind {
    ReadinessFailureKind::Unavailable => 0,
    ReadinessFailureKind::TimedOut => 1,
  }]);
  let digest = digest.finalize();
  format!(
    "server_readiness_{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
    digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7]
  )
}

fn current_timestamp() -> Option<Timestamp> {
  let milliseconds = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .ok()?
    .as_millis();
  Timestamp::from_unix_millis(i64::try_from(milliseconds).ok()?).ok()
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

  #[derive(Default)]
  struct RecordingCriticalConditions {
    changes: Mutex<Vec<CriticalSystemConditionChange>>,
  }

  #[async_trait]
  impl CriticalSystemConditionStore for RecordingCriticalConditions {
    async fn record_critical_system_condition(
      &self,
      change: CriticalSystemConditionChange,
    ) -> Result<(), octacity_server_store::StoreError> {
      self.changes.lock().unwrap().push(change);
      Ok(())
    }
  }

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

  #[tokio::test(start_paused = true)]
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
      None,
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

  #[tokio::test]
  async fn readiness_transitions_publish_and_resolve_critical_conditions() {
    let database = controlled("postgres", false);
    let checks = ReadinessChecks::new(
      controlled("migrations", true),
      database.clone(),
      controlled("object-storage", true),
      controlled("signing-material", true),
      std::iter::empty(),
    );
    let recorded = Arc::new(RecordingCriticalConditions::default());
    let cancellation = CancellationToken::new();
    let monitor = ReadinessMonitor::start(
      checks,
      std::time::Duration::from_millis(2),
      std::time::Duration::from_millis(50),
      Some(recorded.clone()),
      cancellation.clone(),
    )
    .await;
    let state = monitor.state();
    assert!(!state.is_ready());

    database.healthy.store(true, Ordering::Release);
    wait_for(&state, true).await;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
      while recorded.changes.lock().unwrap().len() != 2 {
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
      }
    })
    .await
    .unwrap();

    {
      let changes = recorded.changes.lock().unwrap();
      let CriticalSystemConditionChange::Open { code, summary, .. } = &changes[0] else {
        panic!("initial readiness loss must open a condition");
      };
      assert!(code.starts_with("server_readiness_"));
      assert_eq!(summary, "Server readiness lost: postgres is unavailable");
      assert!(matches!(
        &changes[1],
        CriticalSystemConditionChange::Resolve { code: resolved, .. } if resolved == code
      ));
    }

    cancellation.cancel();
    monitor.into_task().await.unwrap();
  }
}
