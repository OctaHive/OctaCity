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
fn context() -> ManagementRequestContext {
  ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap())
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
#[path = "../tests/support/factory_human_fixture.rs"]
mod human_fixture_support;
pub(crate) fn human_fixture() -> PublishedFactoryAdmission {
  human_fixture_with_response(
    FlowDataSchema::new(key("operator.response"), key("v1"), FlowValueSchema::Boolean).unwrap(),
  )
}
pub(crate) fn human_fixture_with_response(response: FlowDataSchema) -> PublishedFactoryAdmission {
  human_fixture_with_binding(response, None)
}
pub(crate) fn human_fixture_with_binding(
  response: FlowDataSchema,
  binding: Option<FlowHumanGateBinding>,
) -> PublishedFactoryAdmission {
  human_fixture_with_contract(response, binding, false)
}
use human_fixture_support::human_fixture_with_contract;

#[path = "factory_action_tests.rs"]
mod actions;

#[test]
fn superseded_cycles_conflict_for_new_intents_and_preserve_exact_replay() {
  crate::factory_flow_tests::run_ready(async {
    let response = FlowDataSchema::new(key("operator.response"), key("v1"), FlowValueSchema::Boolean).unwrap();
    let admission = human_fixture_with_contract(response, None, true);
    let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
    let audit = testing::management_mutation_with_request((), "cycle-fixture")
      .audit()
      .clone();
    store
      .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
      .unwrap();
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
    let context = context();
    let grant = ManagementAuthorizationGrant::new(ManagementVisibility::all());
    let command = crate::SubmitFactoryNodeResponseCommand {
      run_id: admission.run.id(),
      node_attempt_id: pending.flow.attempts[0].id(),
      outcome: key("approved"),
      value: serde_json::json!(true),
      expected_version: pending.run.version(),
      idempotency_key: IdempotencyKey::new("first-cycle-response").unwrap(),
      requested_at: at(12),
    };
    let controls = FactoryControlHandlers::new(store.clone());
    let accepted = controls
      .execute_management_command(&context, &grant, command.clone())
      .await
      .unwrap();
    let responded = store.factory_run_snapshot(admission.run.id()).await.unwrap();
    let ownership = claim(store.as_ref(), &responded, 13).await;
    for time in [14, 15, 16] {
      owner
        .reconcile_flow(admission.run.id(), ownership.id, at(time))
        .await
        .unwrap();
    }
    let current = store.factory_run_snapshot(admission.run.id()).await.unwrap();
    assert_eq!(current.flow.cycles.len(), 2);
    assert_eq!(current.flow.attempts.len(), 2);
    let mut stale = command.clone();
    stale.expected_version = current.run.version();
    stale.idempotency_key = IdempotencyKey::new("stale-cycle-response").unwrap();
    stale.requested_at = at(17);
    assert_eq!(
      controls
        .execute_management_command(&context, &grant, stale)
        .await
        .unwrap_err()
        .classification(),
      crate::ApplicationFailure::Conflict
    );
    let retry = crate::RetryFactoryNodeCommand {
      run_id: admission.run.id(),
      node_attempt_id: command.node_attempt_id,
      expected_version: current.run.version(),
      idempotency_key: IdempotencyKey::new("stale-cycle-retry").unwrap(),
      requested_at: at(17),
    };
    assert_eq!(
      controls
        .execute_management_command(&context, &grant, retry)
        .await
        .unwrap_err()
        .classification(),
      crate::ApplicationFailure::Conflict
    );
    let mut replay = command;
    replay.requested_at = at(18);
    let replay = controls
      .execute_management_command(&context, &grant, replay)
      .await
      .unwrap();
    assert_eq!(replay.disposition, MutationDisposition::Replayed);
    assert_eq!(replay.version, accepted.version);
    assert_eq!(store.factory_run_snapshot(admission.run.id()).await.unwrap(), current);
  });
}

#[test]
fn delivery_confirmation_uses_only_the_pinned_operator_contract() {
  crate::factory_flow_tests::run_ready(async {
    for (operation, disposition, purpose, choice) in [
      (
        FlowHumanOperation::Delivery,
        FlowHumanDisposition::Confirm,
        "delivery",
        "confirm",
      ),
      (
        FlowHumanOperation::Promotion,
        FlowHumanDisposition::Confirm,
        "promotion",
        "confirm",
      ),
      (
        FlowHumanOperation::Escalation,
        FlowHumanDisposition::Resume,
        "escalation",
        "resume",
      ),
    ] {
      let binding = FlowHumanGateBinding::new(
        operation,
        std::collections::BTreeMap::from([(disposition, key("approved"))]),
      )
      .unwrap();
      let schema = FlowDataSchema::new(
        key("operator.response"),
        key("v1"),
        binding.response_shape(disposition).unwrap(),
      )
      .unwrap();
      let admission = human_fixture_with_binding(schema, Some(binding.clone()));
      let store = Arc::new(testing::InMemoryFactoryConfigurationStore::new());
      let audit = testing::management_mutation_with_request((), "operator-fixture")
        .audit()
        .clone();
      store
        .seed_factory_run(admission.clone(), FactoryDigest::from_bytes([1; 32]), &audit)
        .unwrap();
      verify_operator_confirmation(
        store,
        &admission,
        &binding,
        disposition,
        serde_json::json!({"operation":purpose,"disposition":choice,"reason":"request review publication"}),
      )
      .await;
    }
  });
}
pub(crate) async fn verify_operator_confirmation<
  S: FactoryRunStore + FactoryPhasePoolStore + FactoryRunControlStore + 'static,
>(
  store: Arc<S>,
  admission: &PublishedFactoryAdmission,
  binding: &FlowHumanGateBinding,
  disposition: FlowHumanDisposition,
  expected_value: serde_json::Value,
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
  let context = context();
  let grant = ManagementAuthorizationGrant::new(ManagementVisibility::all());
  let command = crate::ConfirmFactoryNodeOperationCommand {
    run_id: admission.run.id(),
    node_attempt_id: pending.flow.attempts[0].id(),
    operation: binding.operation(),
    disposition,
    reason: FactoryText::new("request review publication").unwrap(),
    expected_version: pending.run.version(),
    idempotency_key: IdempotencyKey::new("delivery-request").unwrap(),
    requested_at: at(12),
  };
  let controls = FactoryControlHandlers::new(store.clone());
  let mut wrong_purpose = command.clone();
  wrong_purpose.operation = if binding.operation() == FlowHumanOperation::Delivery {
    FlowHumanOperation::Promotion
  } else {
    FlowHumanOperation::Delivery
  };
  assert!(
    controls
      .execute_management_command(&context, &grant, wrong_purpose)
      .await
      .is_err()
  );
  assert_eq!(store.factory_run_snapshot(admission.run.id()).await.unwrap(), pending);
  assert!(
    controls
      .execute_management_command(
        &context,
        &grant,
        crate::SubmitFactoryNodeResponseCommand {
          run_id: command.run_id,
          node_attempt_id: command.node_attempt_id,
          outcome: key("approved"),
          value: binding.response(command.disposition, &command.reason).unwrap(),
          expected_version: command.expected_version,
          idempotency_key: IdempotencyKey::new("generic-bypass").unwrap(),
          requested_at: at(12),
        }
      )
      .await
      .is_err(),
    "generic observations must not bypass a bound operator purpose"
  );
  let audits = || {
    octacity_server_store::ListFactoryRunDiagnostics::new(
      admission.run.id(),
      octacity_server_store::FactoryRunDiagnosticKind::Audit,
      None,
      100,
    )
    .unwrap()
  };
  let before_audit = store.list_factory_run_diagnostics(audits()).await.unwrap();
  let accepted = controls
    .execute_management_command(&context, &grant, command.clone())
    .await
    .unwrap();
  let responded = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  assert!(
    responded.flow.completions.is_empty(),
    "operator confirmation is intent, not execution success"
  );
  assert_eq!(responded.controls.len(), 1);
  assert_eq!(
    store.list_factory_run_diagnostics(audits()).await.unwrap().items.len(),
    before_audit.items.len() + 1
  );
  let ownership = claim(store.as_ref(), &responded, 13).await;
  owner
    .reconcile_flow(admission.run.id(), ownership.id, at(14))
    .await
    .unwrap();
  owner
    .reconcile_flow(admission.run.id(), ownership.id, at(15))
    .await
    .unwrap();
  let completed = store.factory_run_snapshot(admission.run.id()).await.unwrap();
  assert_eq!(completed.flow.data.records[0].payload().value(), &expected_value);
  let mut replay = command.clone();
  replay.requested_at = at(200);
  let replay = controls
    .execute_management_command(&context, &grant, replay)
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.version, accepted.version);
  assert_eq!(store.factory_run_snapshot(admission.run.id()).await.unwrap(), completed);
  let mut stale = command;
  stale.expected_version = completed.run.version();
  stale.idempotency_key = IdempotencyKey::new("stale-delivery-request").unwrap();
  stale.requested_at = at(200);
  let error = controls
    .execute_management_command(&context, &grant, stale)
    .await
    .unwrap_err();
  assert_eq!(
    error.classification(),
    crate::ApplicationFailure::Conflict,
    "a fresh request against a completed or superseded attempt must conflict"
  );
  assert_eq!(store.factory_run_snapshot(admission.run.id()).await.unwrap(), completed);
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
  let context = context();
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
  assert!(crate::FactoryNodeActionDescription::from_snapshot(&done, attempt.id(), at(200)).is_none());
  assert_eq!(done.flow.attempts, pending.flow.attempts);
  assert_eq!(done.flow.completions.len(), 1);
  assert_eq!(done.flow.completions[0].outcome(), &key("approved"));
  assert_eq!(done.flow.data.records[0].payload().value(), &value);
  // A headless client sends only the declared outcome and bounded value. The
  // server resolves the retained schema, including a lost-response replay after
  // the owner has completed the node and its original deadline has passed.
  let replay = FactoryControlHandlers::new(restarted)
    .execute_management_command(
      &context,
      &grant,
      crate::SubmitFactoryNodeResponseCommand {
        run_id: admission.run.id(),
        node_attempt_id: attempt.id(),
        outcome: key("approved"),
        value,
        expected_version: pending.run.version(),
        idempotency_key: IdempotencyKey::new("human-response").unwrap(),
        requested_at: at(200),
      },
    )
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.version, accepted.version);
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
  let context = context();
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
  let stop_page = restarted
    .list_factory_run_diagnostics(
      octacity_server_store::ListFactoryRunDiagnostics::new(
        admission.run.id(),
        octacity_server_store::FactoryRunDiagnosticKind::NodeStop,
        None,
        1,
      )
      .unwrap(),
    )
    .await
    .unwrap();
  assert!(
    stop_page.items.is_empty(),
    "a cancelled built-in HumanGate has no external process receipt"
  );
  assert!(stop_page.next.is_none());

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
