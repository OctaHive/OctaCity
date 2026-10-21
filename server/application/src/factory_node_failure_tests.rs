//! Shared public technical-failure owner contract for memory and PostgreSQL.
use crate::*;
use async_trait::async_trait;
use octacity_server_factory::{BudgetUsage, FactoryDigest, FactoryKey, FlowBuildExecution, FlowNodeStopReason};
use octacity_server_store::{
  AuditActorKind, ClaimFactoryRun, FactoryAuditFact, FactoryRunClaimRecord, FactoryRunStore,
};
use std::sync::Arc;
fn at(value: i64) -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(value).unwrap()
}
type ExecutionIds = (
  octacity_server_domain::BuildId,
  octacity_server_domain::AttemptId,
  octacity_server_domain::JobId,
);
pub(crate) async fn verify_confirmed_node_retry<
  S: FactoryRunStore + octacity_server_store::FactoryRunControlStore + 'static,
>(
  store: Arc<S>,
  restarted: Arc<S>,
  fixture: &octacity_server_store::testing::FactoryNodeJournalContractFixture,
  ids: ExecutionIds,
) {
  verify_failed_node_owner(store.clone(), restarted.clone(), fixture, ids).await;
  let stopped = store.factory_run_snapshot(fixture.admission.run.id()).await.unwrap();
  let original = &stopped.flow.attempts[0];
  let claim = stopped.current_claim.as_ref().unwrap();
  let input = stopped.flow.data.inputs[0].retry().unwrap();
  let definition = stopped.admitted_flow.closure().definition(input.definition()).unwrap();
  let forged_attempt = octacity_server_factory::NodeAttempt::new(
    stopped.admitted_flow.root_run(),
    stopped.admitted_flow.initial_cycle(),
    definition,
    octacity_server_factory::NodeAttemptInput {
      id: octacity_server_factory::NodeAttemptId::generate(),
      node_key: original.node_key().clone(),
      node_kind: original.node_kind(),
      number: octacity_server_factory::NodeAttemptNumber::new(2).unwrap(),
      input_digest: input.digest().unwrap(),
      budget: original.budget(),
      started_at: at(14),
      deadline: at(1014),
      execution: original.execution().clone(),
      ownership: octacity_server_factory::FactoryClaimOwnership::new(claim.owner.clone(), claim.claim),
    },
  )
  .unwrap()
  .with_retry_of(original)
  .unwrap();
  let mut forged = octacity_server_store::testing::factory_flow_contract_transition(&stopped, claim, 14);
  let usage = forged.budget.usage;
  forged.append.flow.data.build_intents.push(
    octacity_server_factory::FlowBuildIntent::new(
      input.clone(),
      &stopped.admitted_flow,
      forged_attempt.clone(),
      usage,
      1,
    )
    .unwrap(),
  );
  forged.append.flow.data.inputs.push(input.clone());
  forged.append.flow.attempts.push(forged_attempt);
  forged.budget = octacity_server_store::FactoryBudgetRecord::new(
    stopped.run.id(),
    forged.next_run.version(),
    BudgetUsage {
      attempts: usage.attempts + 1,
      ..usage
    },
    at(14),
  );
  forged.current.budget_id = forged.budget.id;
  assert!(
    store.commit_factory_run_transition(forged).await.is_err(),
    "a worker cannot manufacture operator permission to retry"
  );
  assert_eq!(store.factory_run_snapshot(stopped.run.id()).await.unwrap(), stopped);
  let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
  let handlers = FactoryControlHandlers::new(store.clone());
  let grant = ManagementAuthorizationGrant::new(ManagementVisibility::all());
  let eligible = FactoryNodeAttemptProjection::from_snapshot(original, &stopped);
  assert_eq!(serde_json::to_value(eligible).unwrap()["retryable"], true);
  let command = RetryFactoryNodeCommand {
    run_id: stopped.run.id(),
    node_attempt_id: original.id(),
    expected_version: stopped.run.version(),
    idempotency_key: octacity_server_store::IdempotencyKey::new("retry-exact-node").unwrap(),
    requested_at: at(14),
  };
  let accepted = handlers
    .execute_management_command(&context, &grant, command.clone())
    .await
    .unwrap();
  assert_eq!(accepted.disposition, MutationDisposition::Applied);
  let requested = store.factory_run_snapshot(stopped.run.id()).await.unwrap();
  let requested_projection = FactoryNodeAttemptProjection::from_snapshot(&requested.flow.attempts[0], &requested);
  assert_eq!(serde_json::to_value(requested_projection).unwrap()["retryable"], false);
  let successor = FactoryRunClaimRecord::new(
    stopped.run.id(),
    claim.owner.clone(),
    octacity_server_factory::FactoryClaim::new(
      octacity_server_factory::FactoryClaimFence::new(FactoryDigest::from_bytes([3; 32])),
      at(15),
      at(200),
    )
    .unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: stopped.run.id(),
      expected_version: requested.run.version(),
      record: successor.clone(),
      audit: FactoryAuditFact::new(
        stopped.run.id(),
        AuditActorKind::Worker,
        None,
        FactoryKey::new("factory.claimed").unwrap(),
        FactoryDigest::from_bytes([3; 32]),
        FactoryKey::new("accepted").unwrap(),
        at(15),
      ),
    })
    .await
    .unwrap();
  let target = FactoryNodeTarget {
    flow_run_id: input.flow_run_id(),
    cycle_id: input.cycle_id(),
    node: input.node().clone(),
  };
  let resumed = FactoryNodeRunner::new(
    restarted.clone(),
    Arc::new(RecoveredExecution),
    crate::factory_node_test_support::clock(),
  );
  assert_eq!(
    resumed
      .run_once(stopped.run.id(), successor.id, target, None, at(16))
      .await
      .unwrap(),
    FactoryNodeStep::Advanced
  );
  let retried = restarted.factory_run_snapshot(stopped.run.id()).await.unwrap();
  let attempt = retried
    .flow
    .attempts
    .iter()
    .find(|attempt| attempt.retry_of() == Some(original.id()))
    .unwrap();
  let intent = retried
    .flow
    .data
    .build_intents
    .iter()
    .find(|intent| intent.node().id() == attempt.id())
    .unwrap();
  assert_eq!(intent.input(), &input);
  assert_eq!(attempt.started_at(), at(16));
  assert_eq!(attempt.deadline(), at(1016));
  assert_ne!(
    intent.operation_id().unwrap(),
    stopped.flow.data.build_intents[0].operation_id().unwrap()
  );
  assert_eq!(intent.usage_before().tokens, 23);
  assert_eq!(retried.flow.data.stops, stopped.flow.data.stops);
  assert_eq!(
    handlers
      .execute_management_command(&context, &grant, command)
      .await
      .unwrap()
      .disposition,
    MutationDisposition::Replayed
  );
  assert_eq!(store.factory_run_snapshot(stopped.run.id()).await.unwrap(), retried);
}

#[test]
fn a_confirmed_retry_addresses_the_failed_node_and_preserves_its_frozen_input() {
  let mut future = std::pin::pin!(async {
    let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture_with_retries(1);
    let store = Arc::new(octacity_server_store::testing::InMemoryFactoryConfigurationStore::new());
    let audit = octacity_server_store::testing::management_mutation_with_request((), "node-retry")
      .audit()
      .clone();
    store
      .seed_factory_run(fixture.admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
      .unwrap();
    verify_confirmed_node_retry(
      store.clone(),
      store,
      &fixture,
      (
        octacity_server_domain::BuildId::generate(),
        octacity_server_domain::AttemptId::generate(),
        octacity_server_domain::JobId::generate(),
      ),
    )
    .await;
  });
  let mut context = std::task::Context::from_waker(std::task::Waker::noop());
  assert!(std::future::Future::poll(future.as_mut(), &mut context).is_ready());
}

struct FailedExecution {
  ids: ExecutionIds,
}
#[async_trait]
impl FactoryNodeExecutor for FailedExecution {
  async fn cancel(
    &self,
    _: FactoryNodeExecutionRequest<'_>,
    _: octacity_server_domain::Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    panic!("stopped operations cannot be cancelled again")
  }
  async fn observe(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
  ) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    let execution = FlowBuildExecution::new(request.build.unwrap(), self.ids.0, self.ids.1, vec![self.ids.2]).unwrap();
    Ok(FactoryNodeExecutionStep::ExecutionStopped {
      execution: Some(Box::new(execution)),
      usage: BudgetUsage {
        tokens: 23,
        elapsed_millis: 2,
        ..Default::default()
      },
    })
  }
}
struct RecoveredExecution;
#[async_trait]
impl FactoryNodeExecutor for RecoveredExecution {
  async fn cancel(
    &self,
    _: FactoryNodeExecutionRequest<'_>,
    _: octacity_server_domain::Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    panic!("recovery reads a retained stop")
  }
  async fn observe(&self, _: FactoryNodeExecutionRequest<'_>) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    panic!("recovery reads a retained stop without dispatch")
  }
}
pub(crate) async fn verify_failed_node_owner<S: FactoryRunStore + 'static>(
  store: Arc<S>,
  restarted: Arc<S>,
  fixture: &octacity_server_store::testing::FactoryNodeJournalContractFixture,
  ids: ExecutionIds,
) {
  let run = &fixture.admission.run;
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
  let target = FactoryNodeTarget {
    flow_run_id: fixture.input.flow_run_id(),
    cycle_id: fixture.input.cycle_id(),
    node: fixture.input.node().clone(),
  };
  let runner = FactoryNodeRunner::new(
    store.clone(),
    Arc::new(FailedExecution { ids }),
    crate::factory_node_test_support::clock(),
  );
  assert_eq!(
    runner
      .run_once(run.id(), claim.id, target.clone(), Some(fixture.input.clone()), at(11))
      .await
      .unwrap(),
    FactoryNodeStep::Advanced
  );
  let running = store.factory_run_snapshot(run.id()).await.unwrap();
  assert_eq!(
    serde_json::to_value(FactoryNodeAttemptProjection::from_snapshot(
      &running.flow.attempts[0],
      &running
    ))
    .unwrap()["active"],
    true
  );
  assert_eq!(
    runner
      .run_once(run.id(), claim.id, target.clone(), None, at(12))
      .await
      .unwrap(),
    FactoryNodeStep::ExecutionStopped
  );
  let stopped = store.factory_run_snapshot(run.id()).await.unwrap();
  assert_eq!(
    serde_json::to_value(FactoryNodeAttemptProjection::from_snapshot(
      &stopped.flow.attempts[0],
      &stopped
    ))
    .unwrap()["active"],
    false
  );
  assert_eq!(stopped.flow.data.stops.len(), 1);
  assert_eq!(stopped.flow.data.stops[0].reason(), FlowNodeStopReason::ExecutionFailed);
  assert_eq!(stopped.flow.data.build_executions[0].build_id(), ids.0);
  assert_eq!(stopped.flow.data.build_executions[0].attempt_id(), ids.1);
  assert!(stopped.flow.completions.is_empty());
  assert!(stopped.flow.data.records.is_empty());
  assert_eq!(
    stopped
      .budgets
      .iter()
      .find(|row| row.id == stopped.current.budget_id)
      .unwrap()
      .usage
      .tokens,
    23
  );
  let resumed = FactoryNodeRunner::new(
    restarted.clone(),
    Arc::new(RecoveredExecution),
    crate::factory_node_test_support::clock(),
  );
  assert_eq!(
    resumed
      .run_once(run.id(), claim.id, target, None, at(13))
      .await
      .unwrap(),
    FactoryNodeStep::ExecutionStopped
  );
  assert_eq!(
    restarted.factory_run_snapshot(run.id()).await.unwrap(),
    stopped,
    "recovery must read the retained stop without dispatch or double charging"
  );
}

#[test]
fn an_execution_failure_is_retained_once_without_a_semantic_result_or_automatic_retry() {
  let mut future = std::pin::pin!(async {
    let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture();
    let store = Arc::new(octacity_server_store::testing::InMemoryFactoryConfigurationStore::new());
    let audit = octacity_server_store::testing::management_mutation_with_request((), "failed-node")
      .audit()
      .clone();
    store
      .seed_factory_run(fixture.admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
      .unwrap();
    verify_failed_node_owner(
      store.clone(),
      store,
      &fixture,
      (
        octacity_server_domain::BuildId::generate(),
        octacity_server_domain::AttemptId::generate(),
        octacity_server_domain::JobId::generate(),
      ),
    )
    .await;
  });
  let mut context = std::task::Context::from_waker(std::task::Waker::noop());
  assert!(std::future::Future::poll(future.as_mut(), &mut context).is_ready());
}

pub(crate) fn pooled_retry_fixture() -> (
  octacity_server_store::PublishedFactoryAdmission,
  octacity_server_factory::FlowNodeInput,
) {
  use octacity_server_factory::*;
  let pool = FactoryKey::new("inspection.queue").unwrap();
  let (work, flow, schema, input, _) =
    crate::factory_node_test_support::fixtures::execution_fixture_with_retries_and_pool(
      FlowNodeKind::Reasoning,
      1,
      Some(pool.clone()),
    );
  let budget = flow.limits().budget();
  let flow = flow
    .with_data_schemas(vec![schema])
    .unwrap()
    .with_pool_settings(std::collections::BTreeMap::from([(
      pool.clone(),
      FlowPoolSettings {
        policy: PhasePoolPolicy {
          phase: pool,
          order: vec![
            PhasePoolOrder::Severity,
            PhasePoolOrder::ProjectPriority,
            PhasePoolOrder::Age,
          ],
          max_wip: 1,
          max_project_wip: 1,
          budget,
        },
        project_priority: 0,
        capabilities: Default::default(),
        selection: FlowDataMapping::new(FlowValueMapping::Constant {
          value: serde_json::json!({"severity":"low","dependencies":[]}),
        })
        .unwrap(),
      },
    )]))
    .unwrap();
  let run = FactoryRun::admitted(flow.root_run().factory_run_id(), &work);
  (
    octacity_server_store::PublishedFactoryAdmission {
      work,
      run,
      flow,
      admitted_at: at(1),
    },
    input,
  )
}
struct FrozenOnce {
  input: octacity_server_factory::FlowNodeInput,
  calls: std::sync::atomic::AtomicUsize,
}
#[async_trait]
impl FactoryFlowInputPreparer for FrozenOnce {
  async fn prepare(
    &self,
    _: &octacity_server_store::FactoryRunSnapshot,
    _: &FactoryNodeTarget,
  ) -> Result<octacity_server_factory::FlowNodeInput, ApplicationError> {
    assert_eq!(
      self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
      0,
      "technical retry must not collect Context again"
    );
    Ok(self.input.clone())
  }
}
pub(crate) async fn verify_pooled_node_retry<
  S: FactoryRunStore
    + octacity_server_store::FactoryPhasePoolStore
    + octacity_server_store::FactoryRunControlStore
    + 'static,
>(
  store: Arc<S>,
  restarted: Arc<S>,
  admission: &octacity_server_store::PublishedFactoryAdmission,
  input: octacity_server_factory::FlowNodeInput,
  ids: ExecutionIds,
) {
  use octacity_server_factory::*;
  use octacity_server_store::*;
  let inputs = Arc::new(FrozenOnce {
    input: input.clone(),
    calls: Default::default(),
  });
  let owner = |store: Arc<S>| {
    FactoryFlowRunner::new(
      store,
      Arc::new(FailedExecution { ids }),
      inputs.clone(),
      crate::factory_node_test_support::clock(),
    )
  };
  let claim = FactoryRunClaimRecord::new(
    admission.run.id(),
    FactoryKey::new("owner").unwrap(),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([1; 32])),
      at(10),
      at(100),
    )
    .unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: admission.run.id(),
      expected_version: admission.run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        admission.run.id(),
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
  assert_eq!(
    owner(store.clone())
      .reconcile_flow(admission.run.id(), claim.id, at(11))
      .await
      .unwrap(),
    FactoryFlowStep::Advanced
  );
  let FactoryFlowStep::Ready(first) = owner(restarted.clone())
    .reconcile_flow(admission.run.id(), claim.id, at(12))
    .await
    .unwrap()
  else {
    panic!("initial execution requires queue selection")
  };
  let select = |time: i64| SelectPhasePool {
    policy_digest: first.policy_digest,
    request_id: FactoryDigest::content_sha256(&time.to_be_bytes()),
    owner: FactoryKey::new("queue.owner").unwrap(),
    observed_at: at(time),
    expires_at: at(time + 100),
    capabilities: Default::default(),
    limit: 1,
  };
  let first_selection = store.select_phase_ready(select(101i64)).await.unwrap().remove(0);
  let first_claim = first_selection.run_claim();
  assert_eq!(
    owner(restarted.clone())
      .reconcile_flow(admission.run.id(), first_claim.id, at(102))
      .await
      .unwrap(),
    FactoryFlowStep::Advanced
  );
  assert_eq!(
    owner(store.clone())
      .reconcile_flow(admission.run.id(), first_claim.id, at(103))
      .await
      .unwrap(),
    FactoryFlowStep::Waiting
  );
  let failed = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  assert_eq!(failed.flow.data.stops.len(), 1);
  assert!(!phase_pool_selection_active(&first_selection, &failed, at(103)));
  let original = &failed.flow.attempts[0];
  FactoryControlHandlers::new(store.clone())
    .execute_management_command(
      &ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap()),
      &ManagementAuthorizationGrant::new(ManagementVisibility::all()),
      RetryFactoryNodeCommand {
        run_id: admission.run.id(),
        node_attempt_id: original.id(),
        expected_version: failed.run.version(),
        idempotency_key: IdempotencyKey::new("queued-node-retry").unwrap(),
        requested_at: at(104),
      },
    )
    .await
    .unwrap();
  let retry_claim = store
    .claim_factory_runs(ClaimFactoryRuns::new(FactoryKey::new("retry.owner").unwrap(), at(105), at(200), 1).unwrap())
    .await
    .unwrap()
    .remove(0)
    .record;
  assert_eq!(
    owner(restarted.clone())
      .reconcile_flow(admission.run.id(), retry_claim.id, at(106))
      .await
      .unwrap(),
    FactoryFlowStep::Advanced
  );
  let frozen = restarted.factory_run_snapshot(admission.run.id()).await.unwrap();
  assert_eq!(frozen.flow.data.inputs.len(), 2);
  assert_eq!(
    frozen
      .flow
      .data
      .inputs
      .iter()
      .find(|row| row.generation() == 1)
      .unwrap(),
    &input.retry().unwrap()
  );
  assert_eq!(
    frozen.flow.attempts.len(),
    1,
    "retry waits for its own capacity reservation"
  );
  let FactoryFlowStep::Ready(second) = owner(store.clone())
    .reconcile_flow(admission.run.id(), retry_claim.id, at(107))
    .await
    .unwrap()
  else {
    panic!("retry must enter its configured queue again")
  };
  assert_eq!(second.input.generation, 1);
  assert_ne!(first.id, second.id);
  let second_selection = restarted.select_phase_ready(select(201i64)).await.unwrap().remove(0);
  assert_eq!(second_selection.entry, *second);
  assert_eq!(
    owner(store.clone())
      .reconcile_flow(admission.run.id(), second_selection.run_claim().id, at(202))
      .await
      .unwrap(),
    FactoryFlowStep::Advanced
  );
  let retried = restarted.factory_run_snapshot(admission.run.id()).await.unwrap();
  assert_eq!(retried.flow.attempts.len(), 2);
  assert_eq!(
    retried
      .flow
      .attempts
      .iter()
      .filter(|row| row.retry_of() == Some(original.id()))
      .count(),
    1
  );
  assert!(phase_pool_selection_active(&second_selection, &retried, at(202)));
  assert!(!phase_pool_selection_active(&first_selection, &retried, at(202)));
  assert_eq!(inputs.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}
#[test]
fn a_queued_technical_retry_reserves_fresh_capacity_without_collecting_its_input_again() {
  let mut future = std::pin::pin!(async {
    let (admission, input) = pooled_retry_fixture();
    let store = Arc::new(octacity_server_store::testing::InMemoryFactoryConfigurationStore::new());
    let audit = octacity_server_store::testing::management_mutation_with_request((), "queued-node-retry")
      .audit()
      .clone();
    store
      .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
      .unwrap();
    verify_pooled_node_retry(
      store.clone(),
      store,
      &admission,
      input,
      (
        octacity_server_domain::BuildId::generate(),
        octacity_server_domain::AttemptId::generate(),
        octacity_server_domain::JobId::generate(),
      ),
    )
    .await;
  });
  let mut context = std::task::Context::from_waker(std::task::Waker::noop());
  assert!(std::future::Future::poll(future.as_mut(), &mut context).is_ready());
}

pub(crate) fn single_slot_admissions() -> Vec<octacity_server_store::PublishedFactoryAdmission> {
  use octacity_server_factory::*;
  let fixture = octacity_server_store::testing::factory_node_journal_contract_fixture();
  let template = &fixture.admission.flow;
  (0..2)
    .map(|index| {
      let work = WorkEnvelope::new(
        WorkEnvelopeId::generate(),
        fixture.admission.work.configuration().clone(),
        ExternalWorkIdentity::new(format!("manual/concurrent-node-{index}")).unwrap(),
        fixture.admission.work.subject().clone(),
        fixture.admission.work.artifacts().clone(),
        WorkClassification::new(
          fixture.admission.work.priority(),
          fixture.admission.work.risk(),
          Default::default(),
        ),
      )
      .unwrap();
      let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
      let root = FlowRun::root(&run, template.closure().root()).unwrap();
      let cycle = WorkflowCycle::initial(&root).unwrap();
      let flow = AdmittedFlow::new(
        template.closure().clone(),
        FlowAdmissionLimits::product_defaults(1, template.limits().budget(), template.limits().permissions().clone())
          .unwrap(),
        root,
        cycle,
        FactoryWipLimits::new(2, 1).unwrap(),
      )
      .unwrap()
      .with_data_schemas(template.data_schemas().to_vec())
      .unwrap();
      octacity_server_store::PublishedFactoryAdmission {
        work,
        run,
        flow,
        admitted_at: at(1),
      }
    })
    .collect()
}

pub(crate) async fn claim_configuration_nodes<S: FactoryRunStore>(
  store: &S,
  admissions: &[octacity_server_store::PublishedFactoryAdmission],
) -> (
  Vec<octacity_server_store::FactoryRunClaimRecord>,
  Vec<FactoryNodeTarget>,
  Vec<octacity_server_factory::FlowNodeInput>,
) {
  use octacity_server_factory::*;
  use octacity_server_store::*;
  let mut claims = Vec::new();
  let mut targets = Vec::new();
  let mut inputs = Vec::new();
  for admission in admissions {
    let record = FactoryRunClaimRecord::new(
      admission.run.id(),
      FactoryKey::new("concurrent.owner").unwrap(),
      FactoryClaim::new(
        FactoryClaimFence::new(FactoryDigest::from_bytes([2; 32])),
        at(10),
        at(100),
      )
      .unwrap(),
    );
    store
      .claim_factory_run(ClaimFactoryRun {
        run_id: admission.run.id(),
        expected_version: admission.run.version(),
        record: record.clone(),
        audit: FactoryAuditFact::new(
          admission.run.id(),
          AuditActorKind::Worker,
          None,
          FactoryKey::new("factory.claimed").unwrap(),
          FactoryDigest::from_bytes([2; 32]),
          FactoryKey::new("accepted").unwrap(),
          at(10),
        ),
      })
      .await
      .unwrap();
    let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
    let target = FactoryNodeTarget {
      flow_run_id: admission.flow.root_run().id(),
      cycle_id: admission.flow.initial_cycle().id(),
      node: admission
        .flow
        .closure()
        .definition(admission.flow.closure().root())
        .unwrap()
        .entry()
        .clone(),
    };
    let subject = FactoryTaskSubject::Exact(snapshot.work.subject().clone());
    let task = snapshot.work.artifacts().task().clone();
    let context = ContextManifest::new(
      ContextManifestId::generate(),
      subject.clone(),
      FactoryDigest::from_bytes([4; 32]),
      vec![
        ContextManifestEntry::new(
          ContextSourceKind::Task,
          FactoryKey::new("task").unwrap(),
          subject,
          FactoryContextReference::Artifact(task.clone()),
          task.content_digest(),
          task.encoded_size(),
          FactorySafeText::new("Configured task input").unwrap(),
          FactoryDigest::from_bytes([4; 32]),
        )
        .unwrap(),
      ],
    )
    .unwrap();
    let schema = &admission.flow.data_schemas()[0];
    inputs.push(
      FlowNodeInput::new(
        &snapshot.work,
        &snapshot.admitted_flow,
        snapshot.admitted_flow.root_run(),
        snapshot.admitted_flow.initial_cycle(),
        target.node.clone(),
        FlowPayload::new(schema, serde_json::json!({"count":3})).unwrap(),
      )
      .unwrap()
      .with_context(context, vec![])
      .unwrap(),
    );
    targets.push(target);
    claims.push(record);
  }
  (claims, targets, inputs)
}

pub(crate) async fn verify_configuration_node_capacity<S: FactoryRunStore + 'static>(
  store: Arc<S>,
  restarted: Arc<S>,
  admissions: &[octacity_server_store::PublishedFactoryAdmission],
  ids: ExecutionIds,
) {
  let (claims, targets, inputs) = claim_configuration_nodes(store.as_ref(), admissions).await;
  let owner = FactoryNodeRunner::new(
    store.clone(),
    Arc::new(FailedExecution { ids }),
    crate::factory_node_test_support::clock(),
  );
  assert_eq!(
    owner
      .run_once(
        admissions[0].run.id(),
        claims[0].id,
        targets[0].clone(),
        Some(inputs[0].clone()),
        at(11)
      )
      .await
      .unwrap(),
    FactoryNodeStep::Advanced
  );
  let untouched = restarted.factory_run_snapshot(admissions[1].run.id()).await.unwrap();
  assert!(
    owner
      .run_once(
        admissions[1].run.id(),
        claims[1].id,
        targets[1].clone(),
        Some(inputs[1].clone()),
        at(12)
      )
      .await
      .is_err(),
    "two independently claimed Runs must not overbook the same configured execution slot"
  );
  assert_eq!(
    restarted.factory_run_snapshot(admissions[1].run.id()).await.unwrap(),
    untouched,
    "capacity rejection must be atomic"
  );
  assert_eq!(
    owner
      .run_once(admissions[0].run.id(), claims[0].id, targets[0].clone(), None, at(13))
      .await
      .unwrap(),
    FactoryNodeStep::ExecutionStopped
  );
  let restarted_owner = FactoryNodeRunner::new(
    restarted.clone(),
    Arc::new(RecoveredExecution),
    crate::factory_node_test_support::clock(),
  );
  assert_eq!(
    restarted_owner
      .run_once(
        admissions[1].run.id(),
        claims[1].id,
        targets[1].clone(),
        Some(inputs[1].clone()),
        at(14)
      )
      .await
      .unwrap(),
    FactoryNodeStep::Advanced,
    "a confirmed stop releases capacity for another Run"
  );
}
#[test]
fn independently_claimed_runs_cannot_overbook_configured_node_capacity() {
  let mut future = std::pin::pin!(async {
    let admissions = single_slot_admissions();
    let store = Arc::new(octacity_server_store::testing::InMemoryFactoryConfigurationStore::new());
    for admission in &admissions {
      let audit = octacity_server_store::testing::management_mutation_with_request(
        (),
        admission.work.external_identity().as_str(),
      )
      .audit()
      .clone();
      store
        .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
        .unwrap();
    }
    verify_configuration_node_capacity(
      store.clone(),
      store,
      &admissions,
      (
        octacity_server_domain::BuildId::generate(),
        octacity_server_domain::AttemptId::generate(),
        octacity_server_domain::JobId::generate(),
      ),
    )
    .await;
  });
  let mut context = std::task::Context::from_waker(std::task::Waker::noop());
  assert!(std::future::Future::poll(future.as_mut(), &mut context).is_ready());
}

#[test]
fn a_failed_retry_cannot_exceed_the_pinned_node_retry_allowance() {
  let mut future = std::pin::pin!(async {
    use octacity_server_store::*;
    let fixture = testing::factory_node_journal_contract_fixture_with_retries(1);
    let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
    let audit = testing::management_mutation_with_request((), "bounded-node-retry")
      .audit()
      .clone();
    store
      .seed_factory_run(fixture.admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
      .unwrap();
    verify_confirmed_node_retry(
      store.clone(),
      store.clone(),
      &fixture,
      (
        octacity_server_domain::BuildId::generate(),
        octacity_server_domain::AttemptId::generate(),
        octacity_server_domain::JobId::generate(),
      ),
    )
    .await;
    let retry = store.factory_run_snapshot(fixture.admission.run.id()).await.unwrap();
    let target = FactoryNodeTarget {
      flow_run_id: fixture.input.flow_run_id(),
      cycle_id: fixture.input.cycle_id(),
      node: fixture.input.node().clone(),
    };
    let owner = FactoryNodeRunner::new(
      store.clone(),
      Arc::new(FailedExecution {
        ids: (
          octacity_server_domain::BuildId::generate(),
          octacity_server_domain::AttemptId::generate(),
          octacity_server_domain::JobId::generate(),
        ),
      }),
      crate::factory_node_test_support::clock(),
    );
    assert_eq!(
      owner
        .run_once(retry.run.id(), retry.current_claim.unwrap().id, target, None, at(17))
        .await
        .unwrap(),
      FactoryNodeStep::ExecutionStopped
    );
    let exhausted = store.factory_run_snapshot(retry.run.id()).await.unwrap();
    assert_eq!(exhausted.flow.attempts.len(), 2);
    assert_eq!(exhausted.flow.data.stops.len(), 2);
    assert!(exhausted.flow.data.records.is_empty());
    let failed_retry = exhausted
      .flow
      .attempts
      .iter()
      .find(|row| row.retry_of().is_some())
      .unwrap();
    assert!(FactoryNodeActionDescription::from_snapshot(&exhausted, failed_retry.id(), at(18)).is_none());
    let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
    let grant = ManagementAuthorizationGrant::new(ManagementVisibility::all());
    assert!(
      FactoryControlHandlers::new(store.clone())
        .execute_management_command(
          &context,
          &grant,
          RetryFactoryNodeCommand {
            run_id: exhausted.run.id(),
            node_attempt_id: failed_retry.id(),
            expected_version: exhausted.run.version(),
            idempotency_key: IdempotencyKey::new("retry-over-pinned-bound").unwrap(),
            requested_at: at(18),
          }
        )
        .await
        .is_err()
    );
    assert_eq!(store.factory_run_snapshot(exhausted.run.id()).await.unwrap(), exhausted);
  });
  let mut context = std::task::Context::from_waker(std::task::Waker::noop());
  assert!(std::future::Future::poll(future.as_mut(), &mut context).is_ready());
}
