use std::{
  future::{Future, poll_fn},
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
  },
  task::{Context, Poll, Waker},
};

use crate::FactoryExecutionClock;
use crate::{FactoryReconciler, FactoryReconciliationError, FactoryReconciliationShutdown};
use async_trait::async_trait;
use octacity_server_factory::{FactoryDigest, FactoryKey, PhasePoolPolicy};
use octacity_server_store::{
  ClaimFactoryOutbox, ClaimFactoryRun, ClaimFactoryRunOutcome, ClaimFactoryRuns, ClaimedFactoryOutbox,
  ClaimedFactoryRun, CommitFactoryRunTransition, CommitFactoryRunTransitionOutcome, FactoryOutboxRecord,
  FactoryPhasePoolStore, FactoryRunDiagnosticPage, FactoryRunSnapshot, FactoryRunStore, ListFactoryRunDiagnostics,
  PhasePoolEntry, PhasePoolInput, PhasePoolSelection, SelectPhasePool, SettleFactoryOutbox, StoreError,
  testing::InMemoryFactoryConfigurationStore,
};

pub(super) fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).expect("fixture key is valid")
}

fn digest(value: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([value; 32])
}

pub(super) fn time(value: i64) -> crate::Timestamp {
  crate::Timestamp::from_unix_millis(value).expect("fixture time is valid")
}

struct FixedClock(crate::Timestamp);

impl FactoryExecutionClock for FixedClock {
  fn now(&self) -> Result<crate::Timestamp, crate::ApplicationError> {
    Ok(self.0)
  }
}

pub(super) fn reconciler<S>(
  store: Arc<S>,
  owner: &str,
  batch_size: u16,
  concurrency: u16,
  now: i64,
) -> FactoryReconciler<S>
where
  S: FactoryRunStore + FactoryPhasePoolStore + 'static,
{
  FactoryReconciler::with_clock(
    store.clone(),
    key(owner),
    batch_size,
    concurrency,
    Arc::new(FixedClock(time(now))),
    Arc::new(crate::FactoryFlowRunner::new(
      store,
      Arc::new(NoExternalEffects),
      Arc::new(ConfiguredInputs),
      Arc::new(FixedClock(time(now))),
    )),
  )
  .expect("worker bounds are valid")
}

pub(super) fn seeded_store(run_count: usize) -> Arc<InMemoryFactoryConfigurationStore> {
  let store = Arc::new(InMemoryFactoryConfigurationStore::new());
  let audit = octacity_server_store::testing::management_mutation_with_request((), "configured-worker")
    .audit()
    .clone();
  for ordinal in 0..run_count {
    store
      .seed_factory_run(
        crate::factory_flow_control_tests::human_fixture(),
        digest(u8::try_from(ordinal + 30).unwrap()),
        &audit,
      )
      .unwrap();
  }
  store
}

#[test]
fn configured_owner_restores_the_same_frozen_attempt_after_restart() {
  run_to_completion(async {
    let store = seeded_store(1);
    let first = reconciler(store.clone(), "worker.one", 1, 1, 11);
    let running = FactoryReconciliationShutdown::new();
    let committed = first.run_once(time(10), time(20), &running).await.unwrap();
    assert_eq!(committed.claimed, 1);
    assert_eq!(committed.actions_committed, 1);

    let external_action = first.run_once(time(10), time(20), &running).await.unwrap();
    assert_eq!(external_action.waiting, 1);
    let replayed = first.run_once(time(10), time(20), &running).await.unwrap();
    assert_eq!(replayed.waiting, 1);

    let replacement = reconciler(store.clone(), "worker.two", 1, 1, 21);
    let recovered = replacement.run_once(time(20), time(30), &running).await.unwrap();
    assert_eq!(recovered.waiting, 1);
    let stale = reconciler(store.clone(), "worker.one", 1, 1, 22);
    let no_stale_reentry = stale.run_once(time(21), time(31), &running).await.unwrap();
    assert_eq!(no_stale_reentry.claimed, 0);

    let claims = store
      .claim_factory_runs(ClaimFactoryRuns::new(key("observer"), time(30), time(40), 1).unwrap())
      .await
      .unwrap();
    let snapshot = store.factory_run_snapshot(claims[0].run_id).await.unwrap();
    let history = store
      .list_factory_run_diagnostics(
        octacity_server_store::ListFactoryRunDiagnostics::new(
          snapshot.run.id(),
          octacity_server_store::FactoryRunDiagnosticKind::Claim,
          None,
          100,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(history.items.len(), 3);
    assert_eq!(snapshot.outbox.len(), 1, "only the admission event is emitted");
    assert_eq!(snapshot.flow.attempts.len(), 1);
    assert_eq!(snapshot.flow.data.inputs.len(), 1);
  });
}

#[test]
fn batch_concurrency_and_graceful_shutdown_are_bounded() {
  run_to_completion(async {
    let measured = Arc::new(MeasuredStore::new(seeded_store(4)));
    let worker = reconciler(measured.clone(), "worker.bounded", 3, 2, 11);
    let outcome = worker
      .run_once(time(10), time(20), &FactoryReconciliationShutdown::new())
      .await
      .unwrap();
    assert_eq!(outcome.claimed, 3);
    assert_eq!(outcome.actions_committed, 3);
    assert_eq!(measured.max_active.load(Ordering::SeqCst), 2);

    let shutdown = Arc::new(FactoryReconciliationShutdown::new());
    let draining = Arc::new(MeasuredStore::with_shutdown(seeded_store(3), Arc::clone(&shutdown)));
    let worker = reconciler(draining, "worker.draining", 3, 1, 11);
    let outcome = worker.run_once(time(10), time(20), shutdown.as_ref()).await.unwrap();
    assert_eq!(outcome.actions_committed, 1);
    assert_eq!(outcome.shutdown_skipped, 2);
  });
}

struct WaitingFlow;
#[async_trait]
impl crate::FactoryFlowCoordinator for WaitingFlow {
  async fn reconcile_flow(
    &self,
    _: octacity_server_factory::FactoryRunId,
    _: FactoryDigest,
    _: crate::Timestamp,
  ) -> Result<crate::FactoryFlowStep, crate::ApplicationError> {
    Ok(crate::FactoryFlowStep::Waiting)
  }
}

#[test]
fn invalid_worker_bounds_fail_before_claiming() {
  let store = seeded_store(0);
  assert!(matches!(
    FactoryReconciler::new(store.clone(), key("worker"), 0, 1, Arc::new(WaitingFlow)),
    Err(FactoryReconciliationError::InvalidBounds)
  ));
  assert!(matches!(
    FactoryReconciler::new(store, key("worker"), 2, 3, Arc::new(WaitingFlow)),
    Err(FactoryReconciliationError::InvalidBounds)
  ));
}

struct MeasuredStore {
  inner: Arc<InMemoryFactoryConfigurationStore>,
  active: AtomicUsize,
  max_active: AtomicUsize,
  cancel_once: AtomicBool,
  fail_snapshot_once: AtomicBool,
  committed: AtomicUsize,
  shutdown: Option<Arc<FactoryReconciliationShutdown>>,
}

impl MeasuredStore {
  fn new(inner: Arc<InMemoryFactoryConfigurationStore>) -> Self {
    Self {
      inner,
      active: AtomicUsize::new(0),
      max_active: AtomicUsize::new(0),
      cancel_once: AtomicBool::new(false),
      fail_snapshot_once: AtomicBool::new(false),
      committed: AtomicUsize::new(0),
      shutdown: None,
    }
  }

  fn with_snapshot_failure(inner: Arc<InMemoryFactoryConfigurationStore>) -> Self {
    Self {
      fail_snapshot_once: AtomicBool::new(true),
      ..Self::new(inner)
    }
  }

  fn with_shutdown(
    inner: Arc<InMemoryFactoryConfigurationStore>,
    shutdown: Arc<FactoryReconciliationShutdown>,
  ) -> Self {
    Self {
      inner,
      active: AtomicUsize::new(0),
      max_active: AtomicUsize::new(0),
      cancel_once: AtomicBool::new(true),
      fail_snapshot_once: AtomicBool::new(false),
      committed: AtomicUsize::new(0),
      shutdown: Some(shutdown),
    }
  }
}

#[async_trait]
impl FactoryRunStore for MeasuredStore {
  async fn factory_run_snapshot(
    &self,
    run_id: octacity_server_factory::FactoryRunId,
  ) -> Result<FactoryRunSnapshot, StoreError> {
    let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
    self.max_active.fetch_max(active, Ordering::SeqCst);
    if self.cancel_once.swap(false, Ordering::SeqCst) {
      self.shutdown.as_ref().expect("shutdown token is configured").request();
    }
    let mut yielded = false;
    poll_fn(|context| {
      if yielded {
        Poll::Ready(())
      } else {
        yielded = true;
        context.waker().wake_by_ref();
        Poll::Pending
      }
    })
    .await;
    let result = if self.fail_snapshot_once.swap(false, Ordering::SeqCst) {
      Err(StoreError::Unavailable)
    } else {
      self.inner.factory_run_snapshot(run_id).await
    };
    self.active.fetch_sub(1, Ordering::SeqCst);
    result
  }

  async fn list_factory_run_diagnostics(
    &self,
    request: ListFactoryRunDiagnostics,
  ) -> Result<FactoryRunDiagnosticPage, StoreError> {
    self.inner.list_factory_run_diagnostics(request).await
  }

  async fn claim_factory_run(&self, request: ClaimFactoryRun) -> Result<ClaimFactoryRunOutcome, StoreError> {
    self.inner.claim_factory_run(request).await
  }

  async fn claim_factory_runs(&self, request: ClaimFactoryRuns) -> Result<Vec<ClaimedFactoryRun>, StoreError> {
    self.inner.claim_factory_runs(request).await
  }

  async fn commit_factory_run_transition(
    &self,
    request: CommitFactoryRunTransition,
  ) -> Result<CommitFactoryRunTransitionOutcome, StoreError> {
    let outcome = self.inner.commit_factory_run_transition(request).await?;
    self.committed.fetch_add(1, Ordering::SeqCst);
    Ok(outcome)
  }

  async fn claim_factory_outbox(&self, request: ClaimFactoryOutbox) -> Result<Vec<ClaimedFactoryOutbox>, StoreError> {
    self.inner.claim_factory_outbox(request).await
  }

  async fn settle_factory_outbox(&self, request: SettleFactoryOutbox) -> Result<FactoryOutboxRecord, StoreError> {
    self.inner.settle_factory_outbox(request).await
  }
}

#[test]
fn an_in_flight_transition_finishes_after_a_peer_snapshot_fails() {
  run_to_completion(async {
    let measured = Arc::new(MeasuredStore::with_snapshot_failure(seeded_store(2)));
    let worker = reconciler(measured.clone(), "worker.failure", 2, 2, 11);

    assert!(matches!(
      worker
        .run_once(time(10), time(20), &FactoryReconciliationShutdown::new())
        .await,
      Err(FactoryReconciliationError::Store(StoreError::Unavailable))
    ));
    assert_eq!(measured.committed.load(Ordering::SeqCst), 1);
    assert_eq!(measured.active.load(Ordering::SeqCst), 0);
  });
}

#[async_trait]
impl FactoryPhasePoolStore for MeasuredStore {
  async fn phase_pool_selection_for_claim(
    &self,
    run: octacity_server_factory::FactoryRunId,
    claim: FactoryDigest,
  ) -> Result<Option<PhasePoolSelection>, StoreError> {
    self.inner.phase_pool_selection_for_claim(run, claim).await
  }
  async fn publish_phase_ready(
    &self,
    policy: PhasePoolPolicy,
    input: PhasePoolInput,
  ) -> Result<PhasePoolEntry, StoreError> {
    self.inner.publish_phase_ready(policy, input).await
  }
  async fn phase_ready_entries(
    &self,
    policy: FactoryDigest,
    after: Option<FactoryDigest>,
    limit: u16,
  ) -> Result<Vec<PhasePoolEntry>, StoreError> {
    self.inner.phase_ready_entries(policy, after, limit).await
  }
  async fn select_phase_ready(&self, request: SelectPhasePool) -> Result<Vec<PhasePoolSelection>, StoreError> {
    self.inner.select_phase_ready(request).await
  }
}

pub(super) fn run_to_completion<F: Future>(future: F) -> F::Output {
  let mut future = std::pin::pin!(future);
  let mut context = Context::from_waker(Waker::noop());
  for _ in 0..10_000 {
    if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
      return value;
    }
  }
  panic!("in-memory Factory reconciliation did not settle")
}

struct ConfiguredInputs;
#[async_trait]
impl crate::FactoryFlowInputPreparer for ConfiguredInputs {
  async fn prepare(
    &self,
    snapshot: &FactoryRunSnapshot,
    target: &crate::FactoryNodeTarget,
  ) -> Result<octacity_server_factory::FlowNodeInput, crate::ApplicationError> {
    crate::FactoryNodeRunner::<InMemoryFactoryConfigurationStore, NoExternalEffects>::prepare_input(snapshot, target)
  }
}
struct NoExternalEffects;
#[async_trait]
impl crate::FactoryNodeExecutor for NoExternalEffects {
  async fn observe(
    &self,
    _: crate::FactoryNodeExecutionRequest<'_>,
  ) -> Result<crate::FactoryNodeExecutionStep, crate::ApplicationError> {
    panic!("a configured HumanGate requires no external executor")
  }
  async fn cancel(
    &self,
    _: crate::FactoryNodeExecutionRequest<'_>,
    _: crate::Timestamp,
  ) -> Result<crate::FactoryNodeCancellationStep, crate::ApplicationError> {
    panic!("a configured HumanGate requires no external cancellation")
  }
}
#[test]
fn an_admitted_flow_continues_without_reloading_its_configuration() {
  run_to_completion(async {
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    let fixture = crate::factory_flow_control_tests::human_fixture();
    let run_id = fixture.run.id();
    let audit = octacity_server_store::testing::management_mutation_with_request((), "pinned-owner")
      .audit()
      .clone();
    // Admission already retains the complete validated closure and policies.
    // A configuration catalogue outage must not stop an admitted execution.
    store.seed_factory_run(fixture, digest(41), &audit).unwrap();
    let flow = Arc::new(crate::FactoryFlowRunner::new(
      store.clone(),
      Arc::new(NoExternalEffects),
      Arc::new(ConfiguredInputs),
      Arc::new(FixedClock(time(11))),
    ));
    let worker = FactoryReconciler::with_clock(
      store.clone(),
      key("configured.owner"),
      1,
      1,
      Arc::new(FixedClock(time(11))),
      flow,
    )
    .unwrap();
    let result = worker
      .run_once(time(10), time(100), &FactoryReconciliationShutdown::new())
      .await
      .unwrap();
    assert_eq!(result.actions_committed, 1);
    let snapshot = store.factory_run_snapshot(run_id).await.unwrap();
    assert_eq!(snapshot.flow.attempts.len(), 1);
    assert_eq!(snapshot.flow.attempts[0].node_key().as_str(), "legal_review");
  });
}
