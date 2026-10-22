//! A declared clarification followed by one recoverable controlled local operation.
use octacity_server_application::*;
use octacity_server_factory::*;
use octacity_server_store::{testing::InMemoryFactoryConfigurationStore, *};
use std::sync::{
  Arc,
  atomic::{AtomicI64, Ordering},
};
fn key(s: &str) -> FactoryKey {
  FactoryKey::new(s).unwrap()
}
fn time(n: i64) -> Timestamp {
  Timestamp::from_unix_millis(n).unwrap()
}
fn digest(n: u8) -> FactoryDigest {
  FactoryDigest::from_bytes([n; 32])
}
struct Clock(AtomicI64);
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(time(self.0.load(Ordering::SeqCst)))
  }
}
#[allow(dead_code)]
#[path = "../../../application/tests/support/factory_human_fixture.rs"]
mod human;
pub(super) fn flow() -> FactoryFlowConfiguration {
  let schema = FlowDataSchema::new(key("pilot.response"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let human = human::human_fixture_with_contract(schema.clone(), None, false);
  let previous = human.flow.closure().definitions()[0].clone();
  let capability = ImmutableReference::new(key("controlled-check"), key("v1"), digest(88));
  let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: vec![capability.clone()],
    ..Default::default()
  })
  .unwrap();
  let budget = BudgetLimit::new(10, 10000, 1000, 10000, 30000).unwrap();
  let action = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("controlled-check"),
    kind: FlowNodeKind::TrustedAction,
    input_schema: Some(schema.reference().clone()),
    outcomes: vec![FlowOutcomeDefinition::new(
      key("observed"),
      FlowOutcomeKind::Success,
      schema.reference().clone(),
    )],
    budget: BudgetLimit::new(1, 1000, 0, 0, 10000).unwrap(),
    permissions: permissions.clone(),
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
  .unwrap()
  .with_execution_retries(1)
  .unwrap()
  .with_action(FlowActionBinding::new(capability, key("check"), serde_json::json!({})).unwrap())
  .unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: previous.reference().id(),
    version: previous.reference().version(),
    input_schema: previous.input_schema().cloned(),
    entry: previous.entry().clone(),
    nodes: vec![previous.nodes()[0].clone(), action],
    transitions: vec![
      FlowTransition::new(
        key("legal_review"),
        key("approved"),
        FlowTransitionTarget::Node(key("controlled-check")),
      ),
      FlowTransition::new(
        key("controlled-check"),
        key("observed"),
        FlowTransitionTarget::Terminal(key("reviewed")),
      ),
    ],
    terminals: previous.terminals().to_vec(),
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(budget, permissions.clone(), 1).unwrap(),
  })
  .unwrap();
  let mut schemas = human.flow.data_schemas().to_vec();
  schemas.sort_by_key(|s| s.reference().clone());
  schemas.dedup();
  FactoryFlowConfiguration::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    FlowAdmissionLimits::product_defaults(1, budget, permissions).unwrap(),
    schemas,
    Default::default(),
  )
  .unwrap()
}

struct Operation(std::sync::atomic::AtomicBool);
#[async_trait::async_trait]
impl FactoryNodeExecutor for Operation {
  async fn observe(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
  ) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    if self.0.swap(false, Ordering::SeqCst) {
      return Ok(FactoryNodeExecutionStep::ExecutionStopped {
        execution: None,
        usage: BudgetUsage::default(),
      });
    }
    let definition = request
      .admitted
      .closure()
      .definition(request.input.definition())
      .unwrap();
    let schema = request
      .admitted
      .data_schema(definition.node(request.input.node()).unwrap().outcomes()[0].schema())
      .unwrap();
    let record = FlowNodeRecord::new(
      request.input,
      request.attempt,
      definition,
      FlowRecordObservation {
        outcome: key("observed"),
        payload: FlowPayload::new(schema, serde_json::json!(true)).unwrap(),
        producer: request.attempt.execution().clone(),
        observed_at: request.observed_at,
      },
    )
    .unwrap();
    Ok(FactoryNodeExecutionStep::Completed {
      execution: None,
      record: Box::new(record),
      usage: BudgetUsage::default(),
      outbox: vec![],
    })
  }
  async fn cancel(
    &self,
    _: FactoryNodeExecutionRequest<'_>,
    _: Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    panic!("this pilot does not cancel an external operation")
  }
}
struct Inputs;
#[async_trait::async_trait]
impl FactoryFlowInputPreparer for Inputs {
  async fn prepare(
    &self,
    snapshot: &FactoryRunSnapshot,
    target: &FactoryNodeTarget,
  ) -> Result<FlowNodeInput, ApplicationError> {
    FactoryNodeRunner::<InMemoryFactoryConfigurationStore, Operation>::prepare_input(snapshot, target)
  }
}
pub(super) async fn qualify(
  store: Arc<octacity_server_store_postgres::PostgresStore>,
  context: &ManagementRequestContext,
  run: FactoryRunId,
) {
  let base = SystemFactoryExecutionClock.now().unwrap().unix_millis();
  let clock = Arc::new(Clock(AtomicI64::new(base + 1)));
  let snapshot = store.factory_run_snapshot(run).await.unwrap();
  let claim = FactoryRunClaimRecord::new(
    run,
    key("pilot-owner"),
    FactoryClaim::new(FactoryClaimFence::new(digest(89)), time(base), time(base + 10000)).unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: run,
      expected_version: snapshot.run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        run,
        AuditActorKind::Worker,
        None,
        key("factory.claimed"),
        digest(89),
        key("accepted"),
        time(base),
      ),
    })
    .await
    .unwrap();
  clock.0.store(base + 1, Ordering::SeqCst);
  let operation = Arc::new(Operation(std::sync::atomic::AtomicBool::new(true)));
  let owner = FactoryFlowRunner::new(store.clone(), operation.clone(), Arc::new(Inputs), clock.clone());
  owner.reconcile_flow(run, claim.id, time(base + 1)).await.unwrap();
  let pending = store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(pending.flow.attempts[0].node_kind(), FlowNodeKind::HumanGate);
  // Operator discussion was already appended to this card; it did not answer this gate.
  assert!(pending.flow.completions.is_empty());
  let response = SubmitFactoryNodeResponseCommand {
    run_id: run,
    node_attempt_id: pending.flow.attempts[0].id(),
    outcome: key("approved"),
    value: serde_json::json!(true),
    expected_version: pending.run.version(),
    idempotency_key: "pilot-clarification".parse().unwrap(),
    requested_at: time(base + 2),
  };
  let handler = FactoryControlHandlers::new(store.clone());
  let grant = ManagementAuthorizationGrant::new(ManagementVisibility::all());
  let accepted = handler
    .execute_management_command(context, &grant, response.clone())
    .await
    .unwrap();
  assert_eq!(
    handler
      .execute_management_command(context, &grant, response)
      .await
      .unwrap()
      .version,
    accepted.version
  );
  let claim = reclaim(&store, run, base + 3).await;
  for step in 3..8 {
    clock.0.store(base + step, Ordering::SeqCst);
    owner.reconcile_flow(run, claim.id, time(base + step)).await.unwrap();
    if !store
      .factory_run_snapshot(run)
      .await
      .unwrap()
      .flow
      .data
      .stops
      .is_empty()
    {
      break;
    }
  }
  let stopped = store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(stopped.flow.data.stops.len(), 1);
  assert_eq!(
    stopped.flow.data.records.len(),
    1,
    "technical failure is not a semantic result"
  );
  let failed = stopped
    .flow
    .attempts
    .iter()
    .find(|a| a.node_key() == &key("controlled-check"))
    .unwrap();
  let frozen = stopped
    .flow
    .data
    .inputs
    .iter()
    .find(|i| i.node() == &key("controlled-check"))
    .unwrap()
    .clone();
  let retry = RetryFactoryNodeCommand {
    run_id: run,
    node_attempt_id: failed.id(),
    expected_version: stopped.run.version(),
    idempotency_key: "pilot-retry".parse().unwrap(),
    requested_at: time(base + 10),
  };
  handler
    .execute_management_command(context, &grant, retry)
    .await
    .unwrap();
  let claim = reclaim(&store, run, base + 11).await;
  for step in 11..17 {
    clock.0.store(base + step, Ordering::SeqCst);
    if matches!(
      owner.reconcile_flow(run, claim.id, time(base + step)).await.unwrap(),
      FactoryFlowStep::Resolved(_)
    ) {
      break;
    }
  }
  let done = store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(done.run.state(), FactoryRunState::Completed);
  assert_eq!(
    done
      .flow
      .attempts
      .iter()
      .filter(|a| a.retry_of() == Some(failed.id()))
      .count(),
    1
  );
  assert_eq!(
    done
      .flow
      .data
      .inputs
      .iter()
      .find(|i| i.digest() == frozen.digest())
      .unwrap(),
    &frozen
  );
  assert_eq!(done.flow.data.records.len(), 2);
}

pub(super) async fn reclaim(
  store: &octacity_server_store_postgres::PostgresStore,
  run: FactoryRunId,
  at: i64,
) -> FactoryRunClaimRecord {
  let snapshot = store.factory_run_snapshot(run).await.unwrap();
  let fence = FactoryDigest::content_sha256(&at.to_le_bytes());
  let claim = FactoryRunClaimRecord::new(
    run,
    key("pilot-owner"),
    FactoryClaim::new(FactoryClaimFence::new(fence), time(at), time(at + 10000)).unwrap(),
  );
  store
    .claim_factory_run(ClaimFactoryRun {
      run_id: run,
      expected_version: snapshot.run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        run,
        AuditActorKind::Worker,
        None,
        key("factory.claimed"),
        fence,
        key("accepted"),
        time(at),
      ),
    })
    .await
    .unwrap();
  claim
}

/// Starts the declared gate so the HTTP pilot edits requirements during an actual Run.
pub(super) async fn begin(store: Arc<octacity_server_store_postgres::PostgresStore>, run: FactoryRunId) {
  let now = SystemFactoryExecutionClock.now().unwrap().unix_millis();
  let clock = Arc::new(Clock(AtomicI64::new(now)));
  let claim = reclaim(&store, run, now).await;
  FactoryFlowRunner::new(
    store.clone(),
    Arc::new(Operation(std::sync::atomic::AtomicBool::new(true))),
    Arc::new(Inputs),
    clock,
  )
  .reconcile_flow(run, claim.id, time(now))
  .await
  .unwrap();
  assert_eq!(
    store.factory_run_snapshot(run).await.unwrap().run.state(),
    FactoryRunState::Running
  );
}
