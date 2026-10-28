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
fn a_run_failure_does_not_skip_other_claimed_runs() {
  run_to_completion(async {
    for concurrency in [1, 2] {
      let measured = Arc::new(MeasuredStore::with_snapshot_failure(seeded_store(3)));
      let worker = reconciler(measured.clone(), "worker.failure", 3, concurrency, 11);

      assert!(matches!(
        worker
          .run_once(time(10), time(20), &FactoryReconciliationShutdown::new())
          .await,
        Err(FactoryReconciliationError::Store(StoreError::Unavailable))
      ));
      assert_eq!(measured.committed.load(Ordering::SeqCst), 2);
      assert_eq!(measured.active.load(Ordering::SeqCst), 0);
    }
  });
}

#[async_trait]
impl FactoryPhasePoolStore for MeasuredStore {
  async fn ready_phase_policies(
    &self,
    after: Option<FactoryDigest>,
    limit: u16,
  ) -> Result<Vec<FactoryDigest>, StoreError> {
    self.inner.ready_phase_policies(after, limit).await
  }

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

#[test]
fn native_owner_selects_ready_phase_nodes_and_finishes_after_worker_restart() {
  run_to_completion(async {
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    let admission = crate::factory_flow_tests::bounded_fixture(false, true);
    let run_id = admission.run.id();
    let audit = octacity_server_store::testing::management_mutation_with_request((), "native-phase-worker")
      .audit()
      .clone();
    store.seed_factory_run(admission, digest(83), &audit).unwrap();
    let shutdown = FactoryReconciliationShutdown::new();
    for pass in 0..50 {
      let at = 10 + pass;
      // Recreate the owner on every pass; selection must not depend on a
      // process-private queue or a hand-written phase selection call.
      let flow = Arc::new(crate::FactoryFlowRunner::new(
        store.clone(),
        Arc::new(NoExternalEffects),
        Arc::new(ConfiguredInputs),
        Arc::new(FixedClock(time(at + 1))),
      ));
      let selector = Arc::new(crate::FactoryPhasePoolSelector::new(store.clone(), Default::default(), 1).unwrap());
      let worker = FactoryReconciler::with_clock(
        store.clone(),
        key("worker.phase"),
        1,
        1,
        Arc::new(FixedClock(time(at + 1))),
        flow,
      )
      .unwrap()
      .with_phase_pool_selector(selector);
      worker.run_once(time(at), time(at + 8), &shutdown).await.unwrap();
      let snapshot = store.factory_run_snapshot(run_id).await.unwrap();
      if snapshot.run.state().is_terminal() {
        break;
      }
    }
    let snapshot = store.factory_run_snapshot(run_id).await.unwrap();
    assert_eq!(
      snapshot.run.state(),
      octacity_server_factory::FactoryRunState::Completed
    );
    let pooled = snapshot
      .flow
      .attempts
      .iter()
      .filter(|attempt| {
        snapshot.admitted_flow.closure().definitions().iter().any(|definition| {
          definition
            .node(attempt.node_key())
            .is_some_and(|node| node.phase_pool().is_some())
        })
      })
      .count();
    assert_eq!(pooled, 2, "each bounded repeat receives a fresh admitted selection");
  });
}

#[test]
fn a_selected_phase_survives_owner_expiry_before_the_attempt_starts() {
  run_to_completion(async {
    use crate::FactoryPhasePoolCoordination as _;
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    let admission = crate::factory_flow_tests::bounded_fixture(false, true);
    let run_id = admission.run.id();
    let audit = octacity_server_store::testing::management_mutation_with_request((), "phase-recovery")
      .audit()
      .clone();
    store.seed_factory_run(admission, digest(84), &audit).unwrap();
    let shutdown = FactoryReconciliationShutdown::new();
    for pass in 0..10 {
      let at = 10 + pass;
      reconciler(store.clone(), "worker.prepare", 1, 1, at + 1)
        .run_once(time(at), time(at + 8), &shutdown)
        .await
        .unwrap();
      if !store.ready_phase_policies(None, 1).await.unwrap().is_empty() {
        break;
      }
    }
    crate::FactoryPhasePoolSelector::new(store.clone(), Default::default(), 1)
      .unwrap()
      .select_once(&key("worker.selected"), time(100), time(110))
      .await
      .unwrap();
    let selected = store.factory_run_snapshot(run_id).await.unwrap();
    let original = store
      .phase_pool_selection_for_claim(run_id, selected.current_claim.as_ref().unwrap().id)
      .await
      .unwrap()
      .expect("capacity is durably selected before dispatch");
    assert!(
      !selected
        .flow
        .attempts
        .iter()
        .any(|attempt| attempt.node_key() == &original.entry.input.node)
    );
    let replacement = store
      .claim_factory_runs(ClaimFactoryRuns::new(key("worker.replacement"), time(110), time(130), 1).unwrap())
      .await
      .unwrap()
      .remove(0);
    let recovered = store
      .phase_pool_selection_for_claim(run_id, replacement.record.id)
      .await
      .unwrap()
      .expect("a new current owner resumes the retained reservation without a second selection");
    assert_eq!(recovered, original);
    assert!(
      store
        .phase_pool_selection_for_claim(run_id, digest(85))
        .await
        .unwrap()
        .is_none()
    );
    for pass in 0..4 {
      let at = 110 + pass;
      let worker = reconciler(store.clone(), "worker.replacement", 1, 1, at + 1);
      worker.run_once(time(at), time(130), &shutdown).await.unwrap();
    }
    let resumed = store.factory_run_snapshot(run_id).await.unwrap();
    let completed = resumed
      .flow
      .attempts
      .iter()
      .filter(|attempt| {
        attempt.node_key() == &original.entry.input.node
          && resumed
            .flow
            .completions
            .iter()
            .any(|completion| completion.node_attempt_id() == attempt.id())
      })
      .count();
    assert_eq!(
      completed, 1,
      "the recovered owner dispatches and completes the reserved target"
    );
    let audits = store
      .list_factory_run_diagnostics(
        ListFactoryRunDiagnostics::new(
          run_id,
          octacity_server_store::FactoryRunDiagnosticKind::Audit,
          None,
          100,
        )
        .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(audits.items.iter().filter(|record| matches!(record,
      octacity_server_store::FactoryRunDiagnosticRecord::Audit(fact) if fact.operation.as_str() == "factory.phase_selected"
    )).count(), 1);
  });
}

#[test]
fn phase_admission_accepts_the_full_installed_executor_catalog_bound() {
  let store = Arc::new(InMemoryFactoryConfigurationStore::new());
  let capabilities = (0..crate::MAX_FACTORY_EXECUTOR_BINDINGS)
    .map(|index| {
      octacity_server_factory::ImmutableReference::new(key(&format!("installed-{index}")), key("v1"), digest(60))
    })
    .collect::<std::collections::BTreeSet<_>>();
  assert!(crate::FactoryPhasePoolSelector::new(store, capabilities.clone(), 1).is_ok());
  assert!(
    SelectPhasePool {
      policy_digest: digest(90),
      request_id: digest(91),
      owner: key("worker.full-catalog"),
      observed_at: time(1),
      expires_at: time(2),
      capabilities,
      limit: 1,
    }
    .validate()
    .is_ok()
  );
}

#[test]
fn native_phase_pass_does_not_skip_policies_when_its_batch_fills_early() {
  run_to_completion(async {
    use crate::FactoryPhasePoolCoordination as _;
    use octacity_server_factory::{AdmittedFlow, FactoryPermissionSet, FlowAdmissionLimits};
    let store = Arc::new(InMemoryFactoryConfigurationStore::new());
    let mut runs = Vec::new();
    for policy_index in 0..3 {
      for _ in 0..2 {
        let mut admission = crate::factory_flow_tests::bounded_fixture(false, true);
        let mut pools = admission.flow.pool_settings().clone();
        let settings = pools.values_mut().next().unwrap();
        settings.policy.max_wip = 2;
        settings.policy.max_project_wip = 2;
        settings.policy.budget =
          octacity_server_factory::BudgetLimit::new(16, 10000, 1000 - policy_index, 10000, 10000).unwrap();
        let policy = settings.policy.digest();
        admission.flow = AdmittedFlow::new(
          admission.flow.closure().clone(),
          FlowAdmissionLimits::product_defaults(2, admission.flow.limits().budget(), FactoryPermissionSet::deny_all())
            .unwrap(),
          admission.flow.root_run().clone(),
          admission.flow.initial_cycle().clone(),
          admission.flow.wip_limits(),
        )
        .unwrap()
        .with_data_schemas(admission.flow.data_schemas().to_vec())
        .unwrap()
        .with_pool_settings(pools)
        .unwrap();
        let run = admission.run.id();
        let audit = octacity_server_store::testing::management_mutation_with_request((), "phase-pagination")
          .audit()
          .clone();
        store.seed_factory_run(admission, digest(93), &audit).unwrap();
        runs.push((policy, run));
      }
    }
    let shutdown = FactoryReconciliationShutdown::new();
    for pass in 0..20 {
      let at = 10 + pass;
      reconciler(store.clone(), "worker.prepare", 6, 6, at + 1)
        .run_once(time(at), time(at + 8), &shutdown)
        .await
        .unwrap();
      if store.ready_phase_policies(None, 10).await.unwrap().len() == 3 {
        break;
      }
    }
    let policies = store.ready_phase_policies(None, 10).await.unwrap();
    assert_eq!(
      policies.len(),
      3,
      "three independently admitted policy queues are ready"
    );
    let selector = crate::FactoryPhasePoolSelector::new(store.clone(), Default::default(), 2).unwrap();
    selector
      .select_once(&key("worker.selector"), time(100), time(200))
      .await
      .unwrap();
    selector
      .select_once(&key("worker.selector"), time(101), time(201))
      .await
      .unwrap();
    for (policy, run) in runs {
      let snapshot = store.factory_run_snapshot(run).await.unwrap();
      let selection = match snapshot.current_claim {
        Some(claim) => store.phase_pool_selection_for_claim(run, claim.id).await.unwrap(),
        None => None,
      };
      assert_eq!(
        selection.is_some(),
        policies[..2].contains(&policy),
        "the next pass resumes at the next unprocessed policy"
      );
    }
  });
}
