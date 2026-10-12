//! Shared public owner cancellation contract for memory and PostgreSQL.
use crate::*;
use async_trait::async_trait;
use octacity_server_factory::{
  BudgetUsage, FactoryClaim, FactoryClaimFence, FactoryDigest, FactoryKey, FactoryRunState, FlowBuildExecution,
  FlowNodeInput,
};
use octacity_server_store::{
  AuditActorKind, ClaimFactoryRun, FactoryAuditFact, FactoryPhasePoolStore, FactoryRunClaimRecord,
  FactoryRunControlStore, FactoryRunSnapshot, FactoryRunStore, IdempotencyKey,
};
use std::sync::Arc;
fn at(value: i64) -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(value).unwrap()
}
struct Inputs;
#[async_trait]
impl FactoryFlowInputPreparer for Inputs {
  async fn prepare(&self, _: &FactoryRunSnapshot, _: &FactoryNodeTarget) -> Result<FlowNodeInput, ApplicationError> {
    panic!("cancellation cannot prepare or start a new operation")
  }
}

struct StoppingExecution {
  stopped: std::sync::atomic::AtomicBool,
  ids: (
    octacity_server_domain::BuildId,
    octacity_server_domain::AttemptId,
    octacity_server_domain::JobId,
  ),
}
#[async_trait]
impl FactoryNodeExecutor for StoppingExecution {
  async fn observe(&self, _: FactoryNodeExecutionRequest<'_>) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    panic!("accepted cancellation must never dispatch or observe semantic results")
  }
  async fn cancel(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
    requested_at: octacity_server_domain::Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    assert_eq!(requested_at, at(12), "restart preserves the original cancellation time");
    let execution = FlowBuildExecution::new(request.build.unwrap(), self.ids.0, self.ids.1, vec![self.ids.2]).unwrap();
    if !self.stopped.load(std::sync::atomic::Ordering::SeqCst) {
      return Ok(FactoryNodeCancellationStep::Waiting {
        execution: Some(Box::new(execution)),
      });
    }
    Ok(FactoryNodeCancellationStep::Stopped {
      execution: Some(Box::new(execution)),
      usage: BudgetUsage {
        tokens: 23,
        elapsed_millis: 2,
        ..Default::default()
      },
    })
  }
}
pub(crate) async fn verify_effectful_cancellation<
  S: FactoryRunStore + FactoryPhasePoolStore + FactoryRunControlStore + 'static,
>(
  store: Arc<S>,
  restarted: Arc<S>,
  fixture: &octacity_server_store::testing::FactoryNodeJournalContractFixture,
  ids: (
    octacity_server_domain::BuildId,
    octacity_server_domain::AttemptId,
    octacity_server_domain::JobId,
  ),
) {
  let run = fixture.admission.run.clone();
  let claim = FactoryRunClaimRecord::new(run.id(), fixture.attempt.owner().clone(), fixture.attempt.claim());
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: run.id(),
      expected_version: run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        run.id(),
        AuditActorKind::Worker,
        None,
        FactoryKey::new("factory.claimed").unwrap(),
        FactoryDigest::from_bytes([1; 32]),
        FactoryKey::new("accepted").unwrap(),
        at(10),
      ),
    })
    .await
    .unwrap();
  let executor = Arc::new(StoppingExecution {
    stopped: false.into(),
    ids,
  });
  let runner = FactoryNodeRunner::new(
    store.clone(),
    executor.clone(),
    crate::factory_node_test_support::clock(),
  );
  runner
    .run_once(
      run.id(),
      claim.id,
      FactoryNodeTarget {
        flow_run_id: fixture.input.flow_run_id(),
        cycle_id: fixture.input.cycle_id(),
        node: fixture.input.node().clone(),
      },
      Some(fixture.input.clone()),
      at(11),
    )
    .await
    .unwrap();
  let pending = store.factory_run_snapshot(run.id()).await.unwrap();
  let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
  FactoryControlHandlers::new(store.clone())
    .execute_management_command(
      &context,
      &ManagementAuthorizationGrant::new(ManagementVisibility::all()),
      CancelFactoryRunCommand {
        run_id: run.id(),
        expected_version: pending.run.version(),
        idempotency_key: IdempotencyKey::new("stop-external").unwrap(),
        requested_at: at(12),
      },
    )
    .await
    .unwrap();
  let requested = store.factory_run_snapshot(run.id()).await.unwrap();
  let successor = FactoryRunClaimRecord::new(
    run.id(),
    fixture.attempt.owner().clone(),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([2; 32])),
      at(14),
      at(200),
    )
    .unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: run.id(),
      expected_version: requested.run.version(),
      record: successor.clone(),
      audit: FactoryAuditFact::new(
        run.id(),
        AuditActorKind::Worker,
        None,
        FactoryKey::new("factory.claimed").unwrap(),
        FactoryDigest::from_bytes([2; 32]),
        FactoryKey::new("accepted").unwrap(),
        at(14),
      ),
    })
    .await
    .unwrap();
  let requested = store.factory_run_snapshot(run.id()).await.unwrap();
  let forged = octacity_server_store::testing::factory_flow_contract_transition(&requested, &successor, 15);
  assert!(
    store.commit_factory_run_transition(forged).await.is_err(),
    "a current worker cannot clear an accepted operator cancellation"
  );
  assert_eq!(store.factory_run_snapshot(run.id()).await.unwrap(), requested);
  let mut effect = octacity_server_store::testing::factory_flow_contract_transition(&requested, &successor, 15);
  effect.lifecycle_checkpoint = octacity_server_store::FactoryLifecycleCheckpoint::new(
    run.id(),
    effect.next_run.version(),
    effect.lifecycle_checkpoint.progress.clone(),
    true,
    at(15),
  );
  effect.current.lifecycle_checkpoint_id = effect.lifecycle_checkpoint.id;
  effect.outbox.push(octacity_server_store::FactoryOutboxRecord::pending(
    FactoryDigest::from_bytes([89; 32]),
    run.id(),
    FactoryKey::new("arbitrary.external.effect").unwrap(),
    FactoryDigest::from_bytes([90; 32]),
    at(15),
    at(15),
  ));
  assert!(
    store.commit_factory_run_transition(effect).await.is_err(),
    "accepted cancellation cannot authorize a new external effect"
  );
  assert_eq!(store.factory_run_snapshot(run.id()).await.unwrap(), requested);

  let owner = FactoryFlowRunner::new(
    store.clone(),
    executor.clone(),
    Arc::new(Inputs),
    crate::factory_node_test_support::clock(),
  );
  assert_eq!(
    owner.reconcile_flow(run.id(), successor.id, at(15)).await.unwrap(),
    FactoryFlowStep::Waiting
  );
  let waiting = restarted.factory_run_snapshot(run.id()).await.unwrap();
  assert_eq!(waiting.run.state(), FactoryRunState::Running);
  assert!(waiting.flow.data.stops.is_empty());
  executor.stopped.store(true, std::sync::atomic::Ordering::SeqCst);
  let resumed = FactoryFlowRunner::new(
    restarted.clone(),
    executor,
    Arc::new(Inputs),
    crate::factory_node_test_support::clock(),
  );
  assert!(
    resumed.reconcile_flow(run.id(), successor.id, at(16)).await.unwrap() == FactoryFlowStep::Waiting,
    "retain the receipt before root finalization"
  );
  let stopped = restarted.factory_run_snapshot(run.id()).await.unwrap();
  assert_eq!(stopped.flow.data.stops.len(), 1);
  let page = restarted
    .list_factory_run_diagnostics(
      octacity_server_store::ListFactoryRunDiagnostics::new(
        run.id(),
        octacity_server_store::FactoryRunDiagnosticKind::NodeStop,
        None,
        1,
      )
      .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(
    page.items,
    vec![octacity_server_store::FactoryRunDiagnosticRecord::NodeStop(Box::new(
      stopped.flow.data.stops[0].clone()
    ))]
  );
  assert!(page.next.is_none());

  assert_eq!(
    octacity_server_factory::FlowRuntimeHistory {
      stops: &stopped.flow.data.stops,
      flow_runs: &stopped.flow.runs,
      cycles: &stopped.flow.cycles,
      attempts: &stopped.flow.attempts,
      completions: &stopped.flow.completions,
    }
    .active_node_count(),
    0,
    "a confirmed stopped operation releases its execution slot"
  );
  let receipt = &stopped.flow.data.stops[0];
  assert_eq!(receipt.node_attempt_id(), pending.flow.attempts[0].id());
  assert_eq!(
    receipt.reason(),
    octacity_server_factory::FlowNodeStopReason::Cancelled { requested_at: at(12) }
  );
  assert_eq!(receipt.usage().tokens, 23);
  assert_eq!(
    resumed.reconcile_flow(run.id(), successor.id, at(17)).await.unwrap(),
    FactoryFlowStep::Cancelled
  );
  let cancelled = restarted.factory_run_snapshot(run.id()).await.unwrap();
  assert_eq!(cancelled.run.state(), FactoryRunState::Cancelled);
  assert!(cancelled.flow.completions.is_empty());
  assert!(cancelled.flow.data.records.is_empty());
  assert_eq!(cancelled.flow.data.stops, stopped.flow.data.stops);
  assert_eq!(
    cancelled
      .budgets
      .iter()
      .find(|row| row.id == cancelled.current.budget_id)
      .unwrap()
      .usage
      .tokens,
    23
  );
  assert_eq!(
    resumed.reconcile_flow(run.id(), successor.id, at(18)).await.unwrap(),
    FactoryFlowStep::Cancelled
  );
  assert_eq!(restarted.factory_run_snapshot(run.id()).await.unwrap(), cancelled);
}
#[test]
fn effectful_node_cancellation_waits_for_a_retained_stop_receipt_and_charges_measured_usage_once() {
  let mut future = std::pin::pin!(async {
    let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture();
    let store = Arc::new(octacity_server_store::testing::InMemoryFactoryConfigurationStore::new());
    let audit = octacity_server_store::testing::management_mutation_with_request((), "stop-receipt")
      .audit()
      .clone();
    store
      .seed_factory_run(fixture.admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
      .unwrap();
    let ids = (
      octacity_server_domain::BuildId::generate(),
      octacity_server_domain::AttemptId::generate(),
      octacity_server_domain::JobId::generate(),
    );
    verify_effectful_cancellation(store.clone(), store, &fixture, ids).await;
  });
  let mut context = std::task::Context::from_waker(std::task::Waker::noop());
  assert!(std::future::Future::poll(future.as_mut(), &mut context).is_ready());
}
