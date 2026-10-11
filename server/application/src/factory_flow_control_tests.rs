//! Operator responses address exact configured HumanGate attempts, never business stages.
use crate::{
  ApplicationError, CancelFactoryRunCommand, FactoryControlHandlers, FactoryFlowCoordinator, FactoryFlowInputPreparer,
  FactoryFlowRunner, FactoryFlowStep, FactoryNodeExecutionRequest, FactoryNodeExecutionStep, FactoryNodeExecutor,
  FactoryNodeRunner, FactoryNodeTarget, ManagementAuthorizationGrant, ManagementCommandUseCase,
  ManagementRequestContext, ManagementRequestId, ManagementVisibility, MutationDisposition, ResolveFactoryNodeCommand,
};
use async_trait::async_trait;
use octacity_server_factory::*;
use octacity_server_store::testing;
use octacity_server_store::*;
use std::sync::Arc;
fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn at(value: i64) -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(value).unwrap()
}
struct Inputs;
#[async_trait]
impl FactoryFlowInputPreparer for Inputs {
  async fn prepare(
    &self,
    snapshot: &FactoryRunSnapshot,
    target: &FactoryNodeTarget,
  ) -> Result<FlowNodeInput, ApplicationError> {
    FactoryNodeRunner::<testing::InMemoryFactoryConfigurationStore, NoExternalEffects>::prepare_input(snapshot, target)
  }
}
struct NoExternalEffects;
#[async_trait]
impl FactoryNodeExecutor for NoExternalEffects {
  async fn cancel(
    &self,
    _: FactoryNodeExecutionRequest<'_>,
    _: octacity_server_domain::Timestamp,
  ) -> Result<crate::FactoryNodeCancellationStep, ApplicationError> {
    panic!("this fixture has no cancellable external operation")
  }

  async fn observe(&self, _: FactoryNodeExecutionRequest<'_>) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    panic!("a HumanGate response must be consumed by the owner, without external execution")
  }
}
pub(crate) fn human_fixture() -> PublishedFactoryAdmission {
  human_fixture_with_response(
    FlowDataSchema::new(key("operator.response"), key("v1"), FlowValueSchema::Boolean).unwrap(),
  )
}
pub(crate) fn human_fixture_with_response(response: FlowDataSchema) -> PublishedFactoryAdmission {
  let fixture = testing::factory_node_journal_contract_fixture();
  let work = fixture.admission.work;
  let schema = FlowDataSchema::new(key("operator.confirmation"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let node = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("legal_review"),
    kind: FlowNodeKind::HumanGate,
    input_schema: Some(schema.reference().clone()),
    outcomes: vec![FlowOutcomeDefinition::new(
      key("approved"),
      FlowOutcomeKind::Success,
      response.reference().clone(),
    )],
    budget: fixture.attempt.budget(),
    permissions: FactoryPermissionSet::deny_all(),
    required: true,
    subflow: None,
  })
  .unwrap()
  .with_input_binding(
    FlowInputBinding::new(
      Default::default(),
      FlowDataMapping::new(FlowValueMapping::Constant {
        value: serde_json::json!(true),
      })
      .unwrap(),
    )
    .unwrap(),
  )
  .unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(schema.reference().clone()),
    entry: node.key().clone(),
    nodes: vec![node],
    transitions: vec![FlowTransition::new(
      key("legal_review"),
      key("approved"),
      FlowTransitionTarget::Terminal(key("reviewed")),
    )],
    terminals: vec![FlowTerminalDefinition::new(
      key("reviewed"),
      response.reference().clone(),
    )],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(
      fixture.admission.flow.limits().budget(),
      FactoryPermissionSet::deny_all(),
      1,
    )
    .unwrap(),
  })
  .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let root = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&root).unwrap();
  let flow = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    fixture.admission.flow.limits().clone(),
    root,
    cycle,
    octacity_server_factory::FactoryWipLimits::new(8, 16).unwrap(),
  )
  .unwrap()
  .with_data_schemas(vec![schema, response])
  .unwrap();
  PublishedFactoryAdmission {
    work,
    run,
    flow,
    admitted_at: at(1),
  }
}
async fn claim<S: FactoryRunStore>(store: &S, snapshot: &FactoryRunSnapshot, time: i64) -> FactoryRunClaimRecord {
  let claim = FactoryRunClaimRecord::new(
    snapshot.run.id(),
    key("human.owner"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([time as u8; 32])),
      at(time),
      at(100),
    )
    .unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: snapshot.run.id(),
      expected_version: snapshot.run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        snapshot.run.id(),
        AuditActorKind::Worker,
        None,
        key("factory.claimed"),
        FactoryDigest::from_bytes([time as u8; 32]),
        key("accepted"),
        at(time),
      ),
    })
    .await
    .unwrap();
  claim
}
pub(crate) async fn verify_human_owner<
  S: FactoryRunStore + FactoryPhasePoolStore + FactoryRunControlStore + 'static,
>(
  store: Arc<S>,
  restarted: Arc<S>,
  admission: &PublishedFactoryAdmission,
) {
  verify_human_response(store, restarted, admission, serde_json::json!(true)).await;
}
pub(crate) async fn verify_human_response<
  S: FactoryRunStore + FactoryPhasePoolStore + FactoryRunControlStore + 'static,
>(
  store: Arc<S>,
  restarted: Arc<S>,
  admission: &PublishedFactoryAdmission,
  value: serde_json::Value,
) {
  let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  let owner = FactoryFlowRunner::new(
    store.clone(),
    Arc::new(NoExternalEffects),
    Arc::new(Inputs),
    crate::factory_node_test_support::clock(),
  );
  let original_claim = claim(store.as_ref(), &snapshot, 10).await;
  assert_eq!(
    owner
      .reconcile_flow(admission.run.id(), original_claim.id, at(11))
      .await
      .unwrap(),
    FactoryFlowStep::Advanced
  );
  let pending = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  let attempt = &pending.flow.attempts[0];
  assert_eq!(attempt.node_kind(), FlowNodeKind::HumanGate);
  let claims = store
    .claim_factory_runs(ClaimFactoryRuns::new(key("human.owner"), at(10), at(100), 1).unwrap())
    .await
    .unwrap();
  assert_eq!(claims.len(), 1);
  assert_eq!(claims[0].wip_usage.active_runs(), 1);
  assert_eq!(
    claims[0].wip_usage.active_nodes(),
    1,
    "WIP is derived from the common node journal rather than a capability lifecycle projection"
  );
  let outcome_schema = admission
    .flow
    .closure()
    .definition(admission.flow.root_run().definition())
    .unwrap()
    .node(attempt.node_key())
    .unwrap()
    .outcome(&key("approved"))
    .unwrap()
    .schema();
  let payload = FlowPayload::new(admission.flow.data_schema(outcome_schema).unwrap(), value.clone()).unwrap();
  let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
  let grant = ManagementAuthorizationGrant::new(ManagementVisibility::all());
  let command = ResolveFactoryNodeCommand {
    run_id: admission.run.id(),
    node_attempt_id: attempt.id(),
    outcome: key("approved"),
    payload,
    expected_version: pending.run.version(),
    idempotency_key: IdempotencyKey::new("human-response").unwrap(),
    requested_at: at(12),
  };
  let mut undeclared = command.clone();
  undeclared.outcome = key("skip_everything");
  assert!(
    FactoryControlHandlers::new(store.clone())
      .execute_management_command(&context, &grant, undeclared)
      .await
      .is_err()
  );
  assert_eq!(store.factory_run_snapshot(admission.run.id()).await.unwrap(), pending);
  let handlers = FactoryControlHandlers::new(store.clone());
  let accepted = handlers
    .execute_management_command(&context, &grant, command.clone())
    .await
    .unwrap();
  assert_eq!(accepted.disposition, MutationDisposition::Applied);
  let replay = FactoryControlHandlers::new(restarted.clone())
    .execute_management_command(&context, &grant, command)
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.version, accepted.version);
  let responded = restarted.factory_run_snapshot(admission.run.id()).await.unwrap();
  assert_eq!(responded.flow.attempts, pending.flow.attempts);
  assert!(
    responded.flow.completions.is_empty(),
    "operator intent alone cannot complete a node"
  );
  let resumed_claim = claim(restarted.as_ref(), &responded, 13).await;
  let resumed = FactoryFlowRunner::new(
    restarted.clone(),
    Arc::new(NoExternalEffects),
    Arc::new(Inputs),
    crate::factory_node_test_support::clock(),
  );
  assert_eq!(
    resumed
      .reconcile_flow(admission.run.id(), resumed_claim.id, at(14))
      .await
      .unwrap(),
    FactoryFlowStep::Advanced
  );
  assert_eq!(
    resumed
      .reconcile_flow(admission.run.id(), resumed_claim.id, at(15))
      .await
      .unwrap(),
    FactoryFlowStep::Resolved(key("reviewed"))
  );
  let done = restarted.factory_run_snapshot(admission.run.id()).await.unwrap();
  assert_eq!(done.run.state(), FactoryRunState::Completed);
  assert_eq!(done.flow.attempts, pending.flow.attempts);
  assert_eq!(done.flow.completions.len(), 1);
  assert_eq!(done.flow.completions[0].outcome(), &key("approved"));
  assert_eq!(done.flow.data.records[0].payload().value(), &value);
}
#[test]
fn human_response_resumes_the_exact_configured_attempt_and_graph_across_restart() {
  let mut future = std::pin::pin!(async {
    let admission = human_fixture();
    let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
    let audit = testing::management_mutation_with_request((), "human-fixture")
      .audit()
      .clone();
    store
      .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
      .unwrap();
    verify_human_owner(store.clone(), store, &admission).await;
  });
  let mut context = std::task::Context::from_waker(std::task::Waker::noop());
  assert!(
    std::future::Future::poll(future.as_mut(), &mut context).is_ready(),
    "the memory contract must not block"
  );
}

pub(crate) async fn verify_human_cancellation<
  S: FactoryRunStore + FactoryPhasePoolStore + FactoryRunControlStore + 'static,
>(
  store: Arc<S>,
  restarted: Arc<S>,
  admission: &PublishedFactoryAdmission,
) {
  let snapshot = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  let ownership = claim(store.as_ref(), &snapshot, 10).await;
  let owner = FactoryFlowRunner::new(
    store.clone(),
    Arc::new(NoExternalEffects),
    Arc::new(Inputs),
    crate::factory_node_test_support::clock(),
  );
  owner
    .reconcile_flow(admission.run.id(), ownership.id, at(11))
    .await
    .unwrap();
  let pending = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  let command = CancelFactoryRunCommand {
    run_id: admission.run.id(),
    expected_version: pending.run.version(),
    idempotency_key: IdempotencyKey::new("cancel-human-gate").unwrap(),
    requested_at: at(12),
  };
  let context = ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap());
  let grant = ManagementAuthorizationGrant::new(ManagementVisibility::all());
  let accepted = FactoryControlHandlers::new(store.clone())
    .execute_management_command(&context, &grant, command.clone())
    .await
    .unwrap();
  assert!(accepted.cancellation_requested);
  let replay = FactoryControlHandlers::new(restarted.clone())
    .execute_management_command(&context, &grant, command)
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.version, accepted.version);
  assert!(
    owner
      .reconcile_flow(admission.run.id(), ownership.id, at(13))
      .await
      .is_err(),
    "the pre-control worker must lose write authority"
  );
  let requested = restarted.factory_run_snapshot(admission.run.id()).await.unwrap();
  let resumed_claim = claim(restarted.as_ref(), &requested, 14).await;
  let resumed = FactoryFlowRunner::new(
    restarted.clone(),
    Arc::new(NoExternalEffects),
    Arc::new(Inputs),
    crate::factory_node_test_support::clock(),
  );
  resumed
    .reconcile_flow(admission.run.id(), resumed_claim.id, at(15))
    .await
    .unwrap();
  let cancelled = restarted.factory_run_snapshot(admission.run.id()).await.unwrap();
  assert_eq!(cancelled.run.state(), FactoryRunState::Cancelled);
  assert_eq!(cancelled.flow.attempts, pending.flow.attempts);
  assert!(
    cancelled.flow.completions.is_empty(),
    "cancellation cannot invent a configured success or failure outcome"
  );
  assert!(cancelled.flow.data.records.is_empty());
  assert_eq!(
    cancelled.outbox, pending.outbox,
    "cancellation cannot implicitly select reporting or delivery actions"
  );
  assert!(
    resumed
      .reconcile_flow(admission.run.id(), resumed_claim.id, at(16))
      .await
      .is_ok(),
    "recovery must observe the same cancellation without appending it again"
  );
  assert_eq!(
    restarted.factory_run_snapshot(admission.run.id()).await.unwrap(),
    cancelled
  );
}

#[test]
fn cancellation_stops_a_human_node_without_inventing_a_business_outcome() {
  let mut future = std::pin::pin!(async {
    let admission = human_fixture();
    let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
    let audit = testing::management_mutation_with_request((), "human-cancellation")
      .audit()
      .clone();
    store
      .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([2; 32]), &audit)
      .unwrap();
    verify_human_cancellation(store.clone(), store, &admission).await;
  });
  let mut context = std::task::Context::from_waker(std::task::Waker::noop());
  assert!(std::future::Future::poll(future.as_mut(), &mut context).is_ready());
}
