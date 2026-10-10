use crate::*;
use async_trait::async_trait;
use octacity_server_factory::*;
use octacity_server_store::testing as store_testing;
use octacity_server_store::*;
use std::sync::{Arc, Mutex};

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn reference(value: &str) -> ImmutableReference {
  ImmutableReference::new(key(value), key("v1"), FactoryDigest::from_bytes([1; 32]))
}
fn at(value: i64) -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(value).unwrap()
}
struct Reporter {
  requests: Mutex<Vec<FactoryWorkStatusRequest>>,
  unavailable: bool,
  scoped: bool,
}
#[async_trait]
impl FactoryWorkStatusProjector for Reporter {
  async fn set_status(
    &self,
    request: FactoryWorkStatusRequest,
  ) -> Result<(FactoryDigest, BudgetUsage), ApplicationError> {
    if self.unavailable {
      return Err(ApplicationError::unavailable());
    }
    if self.scoped
      && (!request
        .permissions
        .network_hosts()
        .any(|host| host.as_str() == "tickets.example")
        || !request
          .permissions
          .secret_profiles()
          .any(|profile| profile == &key("ticket-writer")))
    {
      return Err(ApplicationError::invalid());
    }
    self.requests.lock().unwrap().push(request);
    Ok((FactoryDigest::from_bytes([7; 32]), BudgetUsage::default()))
  }
}
struct Harness {
  store: Arc<store_testing::InMemoryFactoryConfigurationStore>,
  run: FactoryRunId,
  claim: FactoryDigest,
  target: FactoryNodeTarget,
  input: FlowNodeInput,
  outcome: FlowOutcomeDefinition,
}
fn harness(binding: FlowActionBinding, permissions: FactoryPermissionSet) -> Harness {
  let fixture = store_testing::factory_node_journal_contract_fixture();
  let work = fixture.admission.work;
  let schema = FlowDataSchema::new(
    key("action.receipt"),
    key("v1"),
    FlowValueSchema::Object {
      fields: std::collections::BTreeMap::from([(
        key("receipt"),
        FlowFieldSchema::required(FlowValueSchema::String {
          min_bytes: 64,
          max_bytes: 64,
        }),
      )]),
    },
  )
  .unwrap();
  let input_schema = FlowDataSchema::new(key("action.input"), key("v1"), FlowValueSchema::Boolean).unwrap();
  let outcome = FlowOutcomeDefinition::new(key("done"), FlowOutcomeKind::Success, schema.reference().clone());
  let budget = BudgetLimit::new(4, 1000, 100, 100, 10000).unwrap();
  let node = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("any_action_name"),
    kind: FlowNodeKind::TrustedAction,
    input_schema: Some(input_schema.reference().clone()),
    outcomes: vec![outcome.clone()],
    budget: BudgetLimit::new(1, 500, 10, 10, 1000).unwrap(),
    permissions: permissions.clone(),
    required: true,
    subflow: None,
  })
  .unwrap()
  .with_action(binding)
  .unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: Some(input_schema.reference().clone()),
    entry: node.key().clone(),
    nodes: vec![node],
    transitions: vec![FlowTransition::new(
      key("any_action_name"),
      key("done"),
      FlowTransitionTarget::Terminal(key("done")),
    )],
    terminals: vec![FlowTerminalDefinition::new(key("done"), schema.reference().clone())],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(budget, permissions.clone(), 1).unwrap(),
  })
  .unwrap();
  let run = FactoryRun::admitted(FactoryRunId::generate(), &work);
  let flow = FlowRun::root(&run, definition.reference()).unwrap();
  let cycle = WorkflowCycle::initial(&flow).unwrap();
  let admitted = AdmittedFlow::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    FlowAdmissionLimits::product_defaults(1, budget, permissions).unwrap(),
    flow,
    cycle,
  )
  .unwrap()
  .with_data_schemas(vec![input_schema.clone(), schema])
  .unwrap();
  let input = FlowNodeInput::new(
    &work,
    &admitted,
    admitted.root_run(),
    admitted.initial_cycle(),
    key("any_action_name"),
    FlowPayload::new(&input_schema, serde_json::json!(true)).unwrap(),
  )
  .unwrap();
  let target = FactoryNodeTarget {
    flow_run_id: input.flow_run_id(),
    cycle_id: input.cycle_id(),
    node: input.node().clone(),
  };
  let store = Arc::new(store_testing::InMemoryFactoryConfigurationStore::new());
  let audit = store_testing::management_mutation_with_request((), "action-admission")
    .audit()
    .clone();
  store
    .seed_factory_run(
      PublishedFactoryAdmission {
        work,
        run: run.clone(),
        flow: admitted,
        admitted_at: at(1),
      },
      FactoryDigest::from_bytes([1; 32]),
      &audit,
    )
    .unwrap();
  let claim = FactoryRunClaimRecord::new(
    run.id(),
    key("worker"),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([2; 32])),
      at(10),
      at(100),
    )
    .unwrap(),
  );
  crate::factory_admission_tests::run_ready(store.claim_factory_run(ClaimFactoryRun {
    run_id: run.id(),
    expected_version: run.version(),
    record: claim.clone(),
    audit: FactoryAuditFact::new(
      run.id(),
      AuditActorKind::Worker,
      None,
      key("factory.claimed"),
      FactoryDigest::from_bytes([1; 32]),
      key("accepted"),
      at(10),
    ),
  }))
  .unwrap();
  Harness {
    store,
    run: run.id(),
    claim: claim.id,
    target,
    input,
    outcome,
  }
}
fn binding(plugin: &str, action: &str, parameters: serde_json::Value) -> FlowActionBinding {
  FlowActionBinding::new(reference(plugin), key(action), parameters).unwrap()
}
fn permissions(plugin: &str) -> FactoryPermissionSet {
  FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: vec![reference(plugin)],
    ..Default::default()
  })
  .unwrap()
}
fn reporter() -> Arc<Reporter> {
  Arc::new(Reporter {
    requests: Mutex::new(vec![]),
    unavailable: false,
    scoped: false,
  })
}

#[test]
fn actions_commit_common_typed_results_and_arbitrary_status_once_across_owner_restart() {
  let h = harness(
    binding(
      "reporter",
      "set_status",
      serde_json::json!({"status":"Ожидает уточнения"}),
    ),
    permissions("reporter"),
  );
  crate::factory_admission_tests::run_ready(async {
    let reporter = reporter();
    let adapter = Arc::new(FactoryFlowActionAdapter::new(Arc::new(FactoryWorkStatusAction::new(
      reference("reporter"),
      reporter.clone(),
      h.outcome.clone(),
    ))));
    let runner = FactoryNodeRunner::new(h.store.clone(), adapter.clone());
    assert_eq!(
      runner
        .run_once(h.run, h.claim, h.target.clone(), Some(h.input.clone()), at(11))
        .await
        .unwrap(),
      FactoryNodeStep::Advanced
    );
    assert!(reporter.requests.lock().unwrap().is_empty());
    let FactoryNodeStep::Completed(record) = runner
      .run_once(h.run, h.claim, h.target.clone(), None, at(12))
      .await
      .unwrap()
    else {
      panic!("retained action result required");
    };
    assert_eq!(
      record.payload().value(),
      &serde_json::json!({"receipt":FactoryDigest::from_bytes([7;32])})
    );
    assert_eq!(
      h.store.factory_run_snapshot(h.run).await.unwrap().flow.data.records,
      vec![*record.clone()]
    );
    let restarted = FactoryNodeRunner::new(h.store.clone(), adapter);
    assert_eq!(
      restarted
        .run_once(h.run, h.claim, h.target, None, at(13))
        .await
        .unwrap(),
      FactoryNodeStep::Completed(record)
    );
    let requests = reporter.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].status.as_str(), "Ожидает уточнения");
  });
}
#[test]
fn invalid_plugin_parameters_claim_deadline_or_authority_cannot_reach_a_connector() {
  for invalid in 0..6 {
    let plugin = if invalid == 1 { "replacement" } else { "reporter" };
    let action = if invalid == 2 { "unknown" } else { "set_status" };
    let parameters = if invalid == 0 {
      serde_json::json!({"status":"Ready","route":"implementation"})
    } else {
      serde_json::json!({"status":"Ready"})
    };
    let h = harness(
      binding(plugin, action, parameters),
      if invalid == 5 {
        FactoryPermissionSet::deny_all()
      } else {
        permissions(plugin)
      },
    );
    crate::factory_admission_tests::run_ready(async {
      let reporter = reporter();
      let runner = FactoryNodeRunner::new(
        h.store.clone(),
        Arc::new(FactoryFlowActionAdapter::new(Arc::new(FactoryWorkStatusAction::new(
          reference("reporter"),
          reporter.clone(),
          h.outcome,
        )))),
      );
      runner
        .run_once(h.run, h.claim, h.target.clone(), Some(h.input), at(11))
        .await
        .unwrap();
      let claim = if invalid == 3 {
        FactoryDigest::from_bytes([99; 32])
      } else {
        h.claim
      };
      assert!(
        runner
          .run_once(h.run, claim, h.target, None, at(if invalid == 4 { 100 } else { 12 }))
          .await
          .is_err()
      );
      assert!(reporter.requests.lock().unwrap().is_empty());
      assert!(
        h.store
          .factory_run_snapshot(h.run)
          .await
          .unwrap()
          .flow
          .data
          .records
          .is_empty()
      );
    });
  }
}
#[test]
fn connector_receives_frozen_authority_and_failure_never_commits_a_result() {
  for missing in [Some("host"), Some("secret"), Some("service"), None] {
    let permission = FactoryPermissionSet::try_new(FactoryPermissionDraft {
      plugins: vec![reference("reporter")],
      network_hosts: if missing == Some("host") {
        vec![]
      } else {
        vec![octacity_server_factory::NetworkHost::new("tickets.example").unwrap()]
      },
      secret_profiles: if missing == Some("secret") {
        vec![]
      } else {
        vec![key("ticket-writer")]
      },
      ..Default::default()
    })
    .unwrap();
    let h = harness(
      binding(
        "reporter",
        "set_status",
        serde_json::json!({"status":"cannot_reproduce"}),
      ),
      permission.clone(),
    );
    crate::factory_admission_tests::run_ready(async {
      let reporter = Arc::new(Reporter {
        requests: Mutex::new(vec![]),
        unavailable: missing == Some("service"),
        scoped: true,
      });
      let runner = FactoryNodeRunner::new(
        h.store.clone(),
        Arc::new(FactoryFlowActionAdapter::new(Arc::new(FactoryWorkStatusAction::new(
          reference("reporter"),
          reporter.clone(),
          h.outcome,
        )))),
      );
      runner
        .run_once(h.run, h.claim, h.target.clone(), Some(h.input), at(11))
        .await
        .unwrap();
      let result = runner.run_once(h.run, h.claim, h.target, None, at(12)).await;
      if missing.is_some() {
        assert!(result.is_err());
        assert!(
          h.store
            .factory_run_snapshot(h.run)
            .await
            .unwrap()
            .flow
            .data
            .records
            .is_empty()
        );
      } else {
        result.unwrap();
        assert_eq!(reporter.requests.lock().unwrap()[0].permissions, permission);
      }
    });
  }
}

struct Notification;
#[async_trait]
impl FactoryFlowActionPlugin for Notification {
  fn identity(&self) -> ImmutableReference {
    reference("notification")
  }
  fn validate(&self, binding: &FlowActionBinding, _: &[FlowOutcomeDefinition]) -> Result<(), ApplicationError> {
    if binding.action() != &key("notify") {
      return Err(ApplicationError::invalid());
    }
    Ok(())
  }
  async fn execute(
    &self,
    request: FactoryFlowActionRequest<'_>,
  ) -> Result<FactoryFlowActionObservation, ApplicationError> {
    assert_eq!(request.input.payload().value(), &serde_json::json!(true));
    let schema = request
      .schemas
      .iter()
      .find(|schema| schema.reference().identity() == &key("action.receipt"))
      .unwrap();
    Ok(FactoryFlowActionObservation {
      outcome: key("done"),
      payload: FlowPayload::new(schema, serde_json::json!({"receipt":FactoryDigest::from_bytes([8;32])})).unwrap(),
      usage: BudgetUsage::default(),
    })
  }
}
#[test]
fn another_action_plugin_receives_the_common_projected_input_without_a_runtime_change() {
  let h = harness(
    binding("notification", "notify", serde_json::json!({"channel":"audit"})),
    permissions("notification"),
  );
  crate::factory_admission_tests::run_ready(async {
    let runner = FactoryNodeRunner::new(h.store, Arc::new(FactoryFlowActionAdapter::new(Arc::new(Notification))));
    runner
      .run_once(h.run, h.claim, h.target.clone(), Some(h.input), at(11))
      .await
      .unwrap();
    let FactoryNodeStep::Completed(record) = runner.run_once(h.run, h.claim, h.target, None, at(12)).await.unwrap()
    else {
      panic!("completion required");
    };
    assert_eq!(
      record.payload().value()["receipt"],
      serde_json::json!(FactoryDigest::from_bytes([8; 32]))
    );
  });
}
