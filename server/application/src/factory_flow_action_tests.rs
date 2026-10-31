use crate::*;
use async_trait::async_trait;
use octacity_server_factory::*;
use octacity_server_store::testing as store_testing;
use octacity_server_store::*;
use std::sync::{Arc, Mutex};
#[path = "factory_work_status_contract_tests.rs"]
mod installed_contract;

#[path = "factory_issue_source_push_tests.rs"]
pub(crate) mod issue_push;
#[path = "factory_result_reporting_tests.rs"]
mod result_reporting;
#[path = "issue_code_review_delivery_tests.rs"]
mod review_delivery;
#[path = "factory_stage_reporting_tests.rs"]
mod stage_reporting;

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn reference(value: &str) -> ImmutableReference {
  ImmutableReference::new(key(value), key("v1"), FactoryDigest::from_bytes([1; 32]))
}
fn at(value: i64) -> octacity_server_domain::Timestamp {
  octacity_server_domain::Timestamp::from_unix_millis(value).unwrap()
}
#[test]
fn status_actions_accept_closed_stage_source_effects_and_require_matching_status_and_outbox() {
  let source = WorkSourceBinding {
    installation: reference("tracker"),
    capability: reference("source"),
    configuration: reference("reader"),
    resource: reference("tickets"),
  };
  let mut writer = source.clone();
  writer.capability = reference("reporter");
  writer.configuration = reference("writer");
  let parameters = serde_json::json!({
    "status":"ready / review", "outbox":{"max_attempts":2,"retry_delay_millis":5},
    "source_update": {
      "source":source, "reporter":writer,
      "connection": {"id":octacity_server_domain::IntegrationId::generate(), "version":1,"access_version":1,
        "source_configuration":reference("reader"),"reporter_configuration":reference("writer")},
      "effects":{"status":"ready / review","add":["review-next"],"remove":["new-item"],"state":null}
    }
  });
  let h = harness(
    binding("reporter", "set_status", parameters.clone()),
    permissions("reporter"),
  );
  let plugin = FactoryWorkStatusAction::new(reference("reporter"), reporter(), h.outcome.clone());
  assert!(
    plugin
      .validate(
        &binding("reporter", "set_status", parameters.clone()),
        std::slice::from_ref(&h.outcome)
      )
      .is_ok()
  );
  for invalid in 0..4 {
    let mut changed = parameters.clone();
    match invalid {
      0 => changed["source_update"]["effects"]["status"] = serde_json::json!("another-result"),
      1 => changed["source_update"]["effects"]["remove"] = serde_json::json!(["review-next"]),
      2 => {
        changed.as_object_mut().unwrap().remove("outbox");
      }
      _ => changed["source_update"]["token"] = serde_json::json!("forbidden"),
    }
    assert!(
      plugin
        .validate(
          &binding("reporter", "set_status", changed),
          std::slice::from_ref(&h.outcome)
        )
        .is_err()
    );
  }
}
struct Reporter {
  requests: Mutex<Vec<FactoryWorkStatusRequest>>,
  unavailable: bool,
  scoped: bool,
}
#[async_trait]
impl FactoryWorkStatusProjector for Reporter {
  async fn cancel_status(
    &self,
    _: FactoryWorkStatusRequest,
    _: octacity_server_domain::Timestamp,
  ) -> Result<Option<BudgetUsage>, ApplicationError> {
    panic!("this fixture exercises only completed synchronous projection")
  }

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
  harness_with_result(binding, permissions, schema)
}
fn harness_with_result(
  binding: FlowActionBinding,
  permissions: FactoryPermissionSet,
  schema: FlowDataSchema,
) -> Harness {
  harness_for_input(
    binding,
    permissions,
    schema,
    store_testing::factory_node_journal_contract_fixture().admission.work,
    FlowDataSchema::new(key("action.input"), key("v1"), FlowValueSchema::Boolean).unwrap(),
    serde_json::json!(true),
  )
}
fn harness_for_input(
  binding: FlowActionBinding,
  permissions: FactoryPermissionSet,
  schema: FlowDataSchema,
  work: WorkEnvelope,
  input_schema: FlowDataSchema,
  input_value: serde_json::Value,
) -> Harness {
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
    octacity_server_factory::FactoryWipLimits::new(8, 16).unwrap(),
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
    FlowPayload::new(&input_schema, input_value).unwrap(),
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
    .seed_factory_admission(
      PublishedFactoryAdmission {
        work,
        run: run.clone(),
        flow: admitted,
        admitted_at: at(1),
      },
      FactoryWorkSourceScope {
        source: "manual".parse().unwrap(),
        security_scope: audit.security_scope().clone(),
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

struct AcceptedReporter {
  store: Arc<store_testing::InMemoryFactoryConfigurationStore>,
  run: FactoryRunId,
  inner: Arc<Reporter>,
}
#[async_trait]
impl FactoryWorkStatusProjector for AcceptedReporter {
  async fn cancel_status(
    &self,
    _: FactoryWorkStatusRequest,
    _: octacity_server_domain::Timestamp,
  ) -> Result<Option<BudgetUsage>, ApplicationError> {
    unreachable!()
  }
  async fn set_status(
    &self,
    request: FactoryWorkStatusRequest,
  ) -> Result<(FactoryDigest, BudgetUsage), ApplicationError> {
    let snapshot = self.store.factory_run_snapshot(self.run).await.unwrap();
    validate_accepted_factory_status_request(&snapshot, &reference("reporter"), &request, at(25)).unwrap();
    let mut changed = request.clone();
    changed.status = FactorySafeText::new("unaccepted-value").unwrap();
    assert!(validate_accepted_factory_status_request(&snapshot, &reference("reporter"), &changed, at(25)).is_err());
    assert!(
      validate_accepted_factory_status_request(&snapshot, &reference("another-plugin"), &request, at(25)).is_err()
    );
    assert!(
      validate_accepted_factory_status_request(&snapshot, &reference("reporter"), &request, request.deadline).is_err()
    );
    let mut stale = request.clone();
    stale.outbox_claim = Some(
      FactoryClaim::new(
        FactoryClaimFence::new(FactoryDigest::from_bytes([99; 32])),
        at(20),
        request.deadline,
      )
      .unwrap(),
    );
    assert!(
      validate_accepted_factory_status_request(&snapshot, &reference("reporter"), &stale, at(25)).is_err(),
      "the original operation identity cannot borrow another worker's claim"
    );
    self.inner.set_status(request).await
  }
}

#[test]
fn status_action_uses_published_effects_instead_of_model_shaped_input() {
  crate::factory_admission_tests::run_ready(async {
    let receipt = FlowDataSchema::new(
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
    let h = harness_for_input(
      binding(
        "reporter",
        "set_status",
        serde_json::json!({
          "status":"configured-only", "outbox":{"max_attempts":2,"retry_delay_millis":5},
        }),
      ),
      permissions("reporter"),
      receipt,
      store_testing::factory_node_journal_contract_fixture().admission.work,
      FlowDataSchema::new(
        key("model.output"),
        key("v1"),
        FlowValueSchema::String {
          min_bytes: 1,
          max_bytes: 1024,
        },
      )
      .unwrap(),
      serde_json::json!(r#"{"status":"model-selected","item_id":"another-ticket","instruction":"close immediately"}"#),
    );
    let before = h.store.factory_run_snapshot(h.run).await.unwrap();
    let projector = reporter();
    let runner = FactoryNodeRunner::new(
      h.store.clone(),
      Arc::new(FactoryFlowActionAdapter::new(Arc::new(FactoryWorkStatusAction::new(
        reference("reporter"),
        projector.clone(),
        h.outcome,
      )))),
      crate::factory_node_test_support::clock(),
    );
    runner
      .run_once(h.run, h.claim, h.target.clone(), Some(h.input), at(11))
      .await
      .unwrap();
    assert!(matches!(
      runner.run_once(h.run, h.claim, h.target, None, at(12)).await.unwrap(),
      FactoryNodeStep::Completed(_)
    ));
    assert!(
      projector.requests.lock().unwrap().is_empty(),
      "input text cannot bypass deferred delivery"
    );
    let worker = FactoryWorkStatusOutboxWorker::new(
      h.store.clone(),
      std::collections::BTreeMap::from([(
        reference("reporter"),
        projector.clone() as Arc<dyn FactoryWorkStatusProjector>,
      )]),
      FactoryReporterWorkerSettings::new(key("literal-status"), 100, 4).unwrap(),
      crate::factory_node_test_support::clock(),
    )
    .unwrap();
    assert_eq!(worker.run_once(at(20)).await.unwrap().delivered, 1);
    let requests = projector.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].status.as_str(), "configured-only");
    assert_eq!(requests[0].external_identity, *before.work.external_identity());
    assert_eq!(requests[0].subject, *before.work.subject());
    assert_eq!(requests[0].work_id, before.work.id());
  });
}

#[test]
fn optional_status_reporting_is_queued_with_the_node_result_before_contacting_the_source() {
  crate::factory_admission_tests::run_ready(async {
    let h = harness(
      binding(
        "reporter",
        "set_status",
        serde_json::json!({"status":"needs-human-eyes","outbox":{"max_attempts":2,"retry_delay_millis":5}}),
      ),
      permissions("reporter"),
    );
    let unavailable_reporter = Arc::new(Reporter {
      requests: Mutex::new(vec![]),
      unavailable: true,
      scoped: false,
    });
    let adapter = Arc::new(FactoryFlowActionAdapter::new(Arc::new(FactoryWorkStatusAction::new(
      reference("reporter"),
      unavailable_reporter.clone(),
      h.outcome.clone(),
    ))));
    let runner = FactoryNodeRunner::new(
      h.store.clone(),
      adapter.clone(),
      crate::factory_node_test_support::clock(),
    );
    runner
      .run_once(h.run, h.claim, h.target.clone(), Some(h.input.clone()), at(11))
      .await
      .unwrap();
    let FactoryNodeStep::Completed(record) = runner
      .run_once(h.run, h.claim, h.target.clone(), None, at(12))
      .await
      .expect("reporter availability cannot block accepting the configured result")
    else {
      panic!("queued report must produce a retained result")
    };
    let snapshot = h.store.factory_run_snapshot(h.run).await.unwrap();
    assert_eq!(snapshot.flow.data.records, vec![*record.clone()]);
    let reports = snapshot
      .outbox
      .iter()
      .filter(|row| row.kind == key("work.status.projection"))
      .collect::<Vec<_>>();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].input_digest, record.digest().unwrap());
    assert_eq!(
      record.payload().value()["receipt"],
      serde_json::json!(reports[0].operation_id)
    );
    assert_eq!(reports[0].state, octacity_server_store::FactoryOutboxState::Pending);
    assert!(unavailable_reporter.requests.lock().unwrap().is_empty());
    assert_eq!(
      runner.run_once(h.run, h.claim, h.target, None, at(13)).await.unwrap(),
      FactoryNodeStep::Completed(record)
    );
    assert_eq!(
      h.store.factory_run_snapshot(h.run).await.unwrap().outbox,
      snapshot.outbox,
      "owner replay cannot queue another report"
    );
    let settings = FactoryReporterWorkerSettings::new(key("reporter.worker"), 10, 10).unwrap();
    let projectors = std::collections::BTreeMap::from([(
      reference("reporter"),
      unavailable_reporter.clone() as Arc<dyn FactoryWorkStatusProjector>,
    )]);
    let worker = FactoryWorkStatusOutboxWorker::new(
      h.store.clone(),
      projectors,
      settings.clone(),
      crate::factory_node_test_support::clock(),
    )
    .unwrap();
    let first = worker.run_once(at(20)).await.unwrap();
    assert_eq!(first.retrying, 1);
    let after = h.store.factory_run_snapshot(h.run).await.unwrap();
    assert_eq!(after.run, snapshot.run);
    assert_eq!(after.current, snapshot.current);
    assert_eq!(
      after.flow, snapshot.flow,
      "reporter failure cannot roll back or reconstruct authoritative flow data"
    );
    let success = reporter();
    let replacement = FactoryWorkStatusOutboxWorker::new(
      h.store.clone(),
      std::collections::BTreeMap::from([(
        reference("reporter"),
        Arc::new(AcceptedReporter {
          store: h.store.clone(),
          run: h.run,
          inner: success.clone(),
        }) as Arc<dyn FactoryWorkStatusProjector>,
      )]),
      settings,
      crate::factory_node_test_support::clock(),
    )
    .unwrap();
    assert_eq!(replacement.run_once(at(24)).await.unwrap().claimed, 0);
    assert_eq!(replacement.run_once(at(25)).await.unwrap().delivered, 1);
    {
      let requests = success.requests.lock().unwrap();
      assert_eq!(requests.len(), 1);
      assert_eq!(requests[0].operation_id, reports[0].operation_id);
      assert_eq!(requests[0].status.as_str(), "needs-human-eyes");
    }
    let final_snapshot = h.store.factory_run_snapshot(h.run).await.unwrap();
    assert!(
      validate_accepted_factory_status_request(
        &final_snapshot,
        &reference("reporter"),
        &success.requests.lock().unwrap()[0],
        at(25)
      )
      .is_err()
    );
    assert_eq!(final_snapshot.run, snapshot.run);
    assert_eq!(final_snapshot.flow, snapshot.flow);
  });
}

#[test]
fn a_reporter_exhausts_only_its_published_retry_budget_without_reopening_the_node() {
  crate::factory_admission_tests::run_ready(async {
    for (dispatch_at, unavailable) in [(20, true), (600, false)] {
      let h = harness(
        binding(
          "reporter",
          "set_status",
          serde_json::json!({"status":"any-status","outbox":{"max_attempts":1,"retry_delay_millis":5}}),
        ),
        permissions("reporter"),
      );
      let projector = Arc::new(Reporter {
        requests: Mutex::new(vec![]),
        unavailable,
        scoped: false,
      });
      let adapter = Arc::new(FactoryFlowActionAdapter::new(Arc::new(FactoryWorkStatusAction::new(
        reference("reporter"),
        projector.clone(),
        h.outcome.clone(),
      ))));
      let runner = FactoryNodeRunner::new(h.store.clone(), adapter, crate::factory_node_test_support::clock());
      runner
        .run_once(h.run, h.claim, h.target.clone(), Some(h.input), at(11))
        .await
        .unwrap();
      assert!(matches!(
        runner.run_once(h.run, h.claim, h.target, None, at(12)).await.unwrap(),
        FactoryNodeStep::Completed(_)
      ));
      let before = h.store.factory_run_snapshot(h.run).await.unwrap();
      let worker = FactoryWorkStatusOutboxWorker::new(
        h.store.clone(),
        std::collections::BTreeMap::from([(
          reference("reporter"),
          projector.clone() as Arc<dyn FactoryWorkStatusProjector>,
        )]),
        FactoryReporterWorkerSettings::new(key("reporter.worker"), 10, 10).unwrap(),
        crate::factory_node_test_support::clock(),
      )
      .unwrap();
      assert_eq!(worker.run_once(at(dispatch_at)).await.unwrap().failed, 1);
      assert_eq!(worker.run_once(at(dispatch_at + 10)).await.unwrap().claimed, 0);
      let after = h.store.factory_run_snapshot(h.run).await.unwrap();
      assert_eq!(after.run, before.run);
      assert_eq!(after.flow, before.flow);
      assert_eq!(after.current, before.current);
      assert!(
        after
          .outbox
          .iter()
          .any(|row| row.state == FactoryOutboxState::Failed && row.failure == Some(FactoryOutboxFailure::Exhausted)),
        "retry/deadline exhaustion must retain its classified reason at {dispatch_at}"
      );
      assert!(projector.requests.lock().unwrap().is_empty());
    }
  });
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
    let runner = FactoryNodeRunner::new(
      h.store.clone(),
      adapter.clone(),
      crate::factory_node_test_support::clock(),
    );
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
    let restarted = FactoryNodeRunner::new(h.store.clone(), adapter, crate::factory_node_test_support::clock());
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
        crate::factory_node_test_support::clock(),
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
fn an_unknown_action_node_selects_only_its_exact_installed_executor_binding() {
  crate::factory_admission_tests::run_ready(async {
    let h = harness(
      binding("reporter", "set_status", serde_json::json!({"status":"needs_context"})),
      permissions("reporter"),
    );
    let selected = reporter();
    let unrelated = reporter();
    let adapter = |identity, reporter| {
      Arc::new(FactoryFlowActionAdapter::new(Arc::new(FactoryWorkStatusAction::new(
        reference(identity),
        reporter,
        h.outcome.clone(),
      )))) as Arc<dyn FactoryNodeExecutor>
    };
    let registry = FactoryNodeExecutorRegistry::new(vec![
      (
        FlowNodeKind::TrustedAction,
        NodeExecutionIdentity::External(reference("unrelated")),
        adapter("unrelated", unrelated.clone()),
      ),
      (
        FlowNodeKind::TrustedAction,
        NodeExecutionIdentity::External(reference("reporter")),
        adapter("reporter", selected.clone()),
      ),
    ])
    .unwrap();
    let runner = FactoryNodeRunner::new(
      h.store.clone(),
      Arc::new(registry),
      crate::factory_node_test_support::clock(),
    );
    runner
      .run_once(h.run, h.claim, h.target.clone(), Some(h.input), at(11))
      .await
      .unwrap();
    assert!(matches!(
      runner.run_once(h.run, h.claim, h.target, None, at(12)).await.unwrap(),
      FactoryNodeStep::Completed(_)
    ));
    assert_eq!(selected.requests.lock().unwrap()[0].status.as_str(), "needs_context");
    assert!(unrelated.requests.lock().unwrap().is_empty());
  });
}
#[test]
fn installing_the_exact_executor_resumes_the_same_durable_action_without_a_fallback() {
  crate::factory_admission_tests::run_ready(async {
    let h = harness(
      binding("reporter", "set_status", serde_json::json!({"status":"investigated"})),
      permissions("reporter"),
    );
    let wrong_version = ImmutableReference::new(key("reporter"), key("v2"), FactoryDigest::from_bytes([2; 32]));
    let rejected_reporter = reporter();
    let wrong_adapter = Arc::new(FactoryFlowActionAdapter::new(Arc::new(FactoryWorkStatusAction::new(
      wrong_version.clone(),
      rejected_reporter.clone(),
      h.outcome.clone(),
    ))));
    let registry = FactoryNodeExecutorRegistry::new(vec![(
      FlowNodeKind::TrustedAction,
      NodeExecutionIdentity::External(wrong_version),
      wrong_adapter,
    )])
    .unwrap();
    let runner = FactoryNodeRunner::new(
      h.store.clone(),
      Arc::new(registry),
      crate::factory_node_test_support::clock(),
    );
    runner
      .run_once(h.run, h.claim, h.target.clone(), Some(h.input), at(11))
      .await
      .unwrap();
    let pending = h.store.factory_run_snapshot(h.run).await.unwrap();
    assert!(
      runner
        .run_once(h.run, h.claim, h.target.clone(), None, at(12))
        .await
        .is_err()
    );
    assert!(rejected_reporter.requests.lock().unwrap().is_empty());
    assert_eq!(h.store.factory_run_snapshot(h.run).await.unwrap(), pending);

    let accepted_reporter = reporter();
    let registry = FactoryNodeExecutorRegistry::new(vec![(
      FlowNodeKind::TrustedAction,
      NodeExecutionIdentity::External(reference("reporter")),
      Arc::new(FactoryFlowActionAdapter::new(Arc::new(FactoryWorkStatusAction::new(
        reference("reporter"),
        accepted_reporter.clone(),
        h.outcome,
      )))),
    )])
    .unwrap();
    let restarted = FactoryNodeRunner::new(
      h.store.clone(),
      Arc::new(registry),
      crate::factory_node_test_support::clock(),
    );
    assert!(matches!(
      restarted
        .run_once(h.run, h.claim, h.target, None, at(13))
        .await
        .unwrap(),
      FactoryNodeStep::Completed(_)
    ));
    let completed = h.store.factory_run_snapshot(h.run).await.unwrap();
    assert_eq!(completed.flow.attempts, pending.flow.attempts);
    assert_eq!(completed.flow.data.inputs, pending.flow.data.inputs);
    assert_eq!(completed.flow.data.records.len(), 1);
    assert_eq!(accepted_reporter.requests.lock().unwrap().len(), 1);
    assert_eq!(
      accepted_reporter.requests.lock().unwrap()[0].status.as_str(),
      "investigated"
    );
  });
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
        crate::factory_node_test_support::clock(),
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
  async fn cancel(
    &self,
    _: FactoryFlowActionRequest<'_>,
    _: octacity_server_domain::Timestamp,
  ) -> Result<Option<BudgetUsage>, ApplicationError> {
    panic!("this notification fixture has no pending operations")
  }

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
      discovery: None,
      outcome: key("done"),
      payload: FlowPayload::new(schema, serde_json::json!({"receipt":FactoryDigest::from_bytes([8;32])})).unwrap(),
      usage: BudgetUsage::default(),
      outbox: vec![],
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
    let runner = FactoryNodeRunner::new(
      h.store,
      Arc::new(FactoryFlowActionAdapter::new(Arc::new(Notification))),
      crate::factory_node_test_support::clock(),
    );
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

struct AmbiguousAction {
  operation: Mutex<Option<FactoryDigest>>,
}
#[async_trait]
impl FactoryFlowActionPlugin for AmbiguousAction {
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
    assert!(self.operation.lock().unwrap().replace(request.operation_id).is_none());
    Err(ApplicationError::unavailable()) // The remote effect may have started despite the lost response.
  }
  async fn cancel(
    &self,
    request: FactoryFlowActionRequest<'_>,
    requested_at: octacity_server_domain::Timestamp,
  ) -> Result<Option<BudgetUsage>, ApplicationError> {
    assert_eq!(Some(request.operation_id), *self.operation.lock().unwrap());
    assert_eq!(requested_at, at(13));
    assert_eq!(request.deadline, at(511));
    Ok(Some(BudgetUsage {
      tokens: 3,
      ..Default::default()
    }))
  }
}
#[test]
fn an_action_with_an_unknown_response_is_stopped_through_its_original_identity_after_the_deadline() {
  let h = harness(
    binding("notification", "notify", serde_json::json!({"channel":"audit"})),
    permissions("notification"),
  );
  crate::factory_admission_tests::run_ready(async {
    let plugin = Arc::new(AmbiguousAction {
      operation: Mutex::new(None),
    });
    let adapter = Arc::new(FactoryFlowActionAdapter::new(plugin.clone()));
    let runner = FactoryNodeRunner::new(
      h.store.clone(),
      adapter.clone(),
      crate::factory_node_test_support::clock(),
    );
    runner
      .run_once(h.run, h.claim, h.target.clone(), Some(h.input), at(11))
      .await
      .unwrap();
    assert!(runner.run_once(h.run, h.claim, h.target, None, at(12)).await.is_err());
    let snapshot = h.store.factory_run_snapshot(h.run).await.unwrap();
    assert!(snapshot.flow.data.records.is_empty());
    let successor = FactoryClaimOwnership::new(
      key("worker"),
      FactoryClaim::new(
        FactoryClaimFence::new(FactoryDigest::from_bytes([91; 32])),
        at(512),
        at(612),
      )
      .unwrap(),
    );
    let registry = FactoryNodeExecutorRegistry::new(vec![(
      FlowNodeKind::TrustedAction,
      NodeExecutionIdentity::External(reference("notification")),
      adapter,
    )])
    .unwrap();
    let result = registry
      .cancel(
        FactoryNodeExecutionRequest {
          run_id: h.run,
          work: &snapshot.work,
          admitted: &snapshot.admitted_flow,
          input: &snapshot.flow.data.inputs[0],
          attempt: &snapshot.flow.attempts[0],
          build: None,
          ownership: successor,
          observed_at: at(513),
        },
        at(13),
      )
      .await
      .unwrap();
    assert_eq!(
      result,
      FactoryNodeCancellationStep::Stopped {
        execution: None,
        usage: BudgetUsage {
          tokens: 3,
          ..Default::default()
        }
      }
    );
    assert_eq!(
      h.store.factory_run_snapshot(h.run).await.unwrap(),
      snapshot,
      "the adapter itself cannot select a graph transition"
    );
  });
}

struct Publisher {
  requests: Mutex<Vec<FactoryCandidatePublicationRequest>>,
  published: Mutex<Option<FactoryCandidatePublication>>,
  lose_response: std::sync::atomic::AtomicBool,
}
#[async_trait]
impl FactoryCandidatePublisher for Publisher {
  async fn observe(
    &self,
    request: &FactoryCandidatePublicationRequest,
  ) -> Result<Option<FactoryCandidatePublication>, ApplicationError> {
    let publication = self.published.lock().unwrap().clone();
    if let Some(ref publication) = publication {
      assert_eq!(publication.candidate, request.candidate);
    }
    Ok(publication)
  }
  async fn publish(
    &self,
    request: &FactoryCandidatePublicationRequest,
  ) -> Result<FactoryCandidatePublication, ApplicationError> {
    self.requests.lock().unwrap().push(request.clone());
    let publication = FactoryCandidatePublication {
      operation_id: request.operation_id,
      candidate: request.candidate.clone(),
      receipt: FactoryDigest::from_bytes([88; 32]),
      facts: FactoryPublicationFacts::new(
        "refs/heads/reviewed-candidate".into(),
        request.candidate.candidate_revision().clone(),
        "23".into(),
        FactoryReviewState::Open,
      )
      .unwrap(),
      usage: BudgetUsage::default(),
    };
    *self.published.lock().unwrap() = Some(publication.clone());
    if self.lose_response.swap(false, std::sync::atomic::Ordering::SeqCst) {
      return Err(ApplicationError::unavailable());
    }
    Ok(publication)
  }
  async fn cancel(
    &self,
    _: &FactoryCandidatePublicationRequest,
    _: octacity_server_domain::Timestamp,
  ) -> Result<Option<BudgetUsage>, ApplicationError> {
    Ok(Some(BudgetUsage::default()))
  }
}
fn publication_schema() -> FlowDataSchema {
  FlowDataSchema::new(
    key("candidate.receipt"),
    key("v1"),
    FlowValueSchema::Object {
      fields: [
        (
          key("external"),
          FlowFieldSchema::required(FlowValueSchema::Object {
            fields: [
              (
                key("branch_reference"),
                FlowFieldSchema::required(FlowValueSchema::String {
                  min_bytes: 1,
                  max_bytes: 1024,
                }),
              ),
              (
                key("remote_head"),
                FlowFieldSchema::required(FlowValueSchema::String {
                  min_bytes: 1,
                  max_bytes: 256,
                }),
              ),
              (
                key("review_identity"),
                FlowFieldSchema::required(FlowValueSchema::String {
                  min_bytes: 1,
                  max_bytes: 256,
                }),
              ),
              (
                key("review_state"),
                FlowFieldSchema::required(FlowValueSchema::Enum {
                  values: vec![
                    serde_json::json!("open"),
                    serde_json::json!("closed"),
                    serde_json::json!("merged_externally"),
                  ],
                }),
              ),
            ]
            .into_iter()
            .collect(),
          }),
        ),
        (
          key("receipt"),
          FlowFieldSchema::required(FlowValueSchema::String {
            min_bytes: 64,
            max_bytes: 64,
          }),
        ),
        (
          key("candidate_revision"),
          FlowFieldSchema::required(FlowValueSchema::String {
            min_bytes: 1,
            max_bytes: 128,
          }),
        ),
        (
          key("changeset_digest"),
          FlowFieldSchema::required(FlowValueSchema::String {
            min_bytes: 64,
            max_bytes: 64,
          }),
        ),
      ]
      .into_iter()
      .collect(),
    },
  )
  .unwrap()
}
fn attach_candidate(h: &mut Harness) -> CandidateSubject {
  let candidate = CandidateSubject::new(
    h.input.exact_resource().unwrap().clone(),
    octacity_server_domain::ImmutableRevision::new("reviewed-candidate").unwrap(),
    FactoryDigest::from_bytes([77; 32]),
  );
  let subject = FactoryTaskSubject::Candidate(candidate.clone());
  let entries = [("candidate_bytes", 41), ("candidate_manifest", 42)]
    .into_iter()
    .map(|(name, seed)| {
      let artifact = FactoryArtifactReference::new(
        octacity_server_domain::ArtifactId::generate(),
        FactoryDigest::from_bytes([seed; 32]),
        100,
      )
      .unwrap();
      ContextManifestEntry::new(
        ContextSourceKind::Specification,
        key(name),
        subject.clone(),
        FactoryContextReference::Artifact(artifact.clone()),
        artifact.content_digest(),
        100,
        FactorySafeText::new("Reviewed exact bytes").unwrap(),
        FactoryDigest::from_bytes([9; 32]),
      )
      .unwrap()
    })
    .collect();
  h.input = h
    .input
    .clone()
    .with_context(
      ContextManifest::new(
        ContextManifestId::generate(),
        subject,
        FactoryDigest::from_bytes([9; 32]),
        entries,
      )
      .unwrap(),
      vec![],
    )
    .unwrap();
  candidate
}
#[test]
fn exact_candidate_publication_recovers_a_lost_response_without_another_write() {
  crate::factory_admission_tests::run_ready(async {
    let schema = publication_schema();
    let mut h = harness_with_result(
      binding(
        "publisher",
        "publish_candidate",
        serde_json::json!({"bundle":"candidate_bytes", "manifest":"candidate_manifest", "target":reference("review_branch")}),
      ),
      permissions("publisher"),
      schema,
    );
    let candidate = attach_candidate(&mut h);
    let publisher = Arc::new(Publisher {
      requests: Mutex::new(vec![]),
      published: Mutex::new(None),
      lose_response: std::sync::atomic::AtomicBool::new(true),
    });
    let adapter = Arc::new(FactoryFlowActionAdapter::new(Arc::new(
      FactoryCandidatePublicationAction::new(reference("publisher"), publisher.clone(), h.outcome.clone()),
    )));
    let runner = FactoryNodeRunner::new(
      h.store.clone(),
      adapter.clone(),
      crate::factory_node_test_support::clock(),
    );
    runner
      .run_once(h.run, h.claim, h.target.clone(), Some(h.input.clone()), at(11))
      .await
      .unwrap();
    assert!(
      runner
        .run_once(h.run, h.claim, h.target.clone(), None, at(12))
        .await
        .is_err()
    );
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
    let restarted = FactoryNodeRunner::new(h.store.clone(), adapter, crate::factory_node_test_support::clock());
    let FactoryNodeStep::Completed(record) = restarted
      .run_once(h.run, h.claim, h.target, None, at(13))
      .await
      .unwrap()
    else {
      panic!("exact observed publication required")
    };
    assert_eq!(
      record.payload().value(),
      &serde_json::json!({"receipt":FactoryDigest::from_bytes([88;32]),"candidate_revision":"reviewed-candidate","changeset_digest":FactoryDigest::from_bytes([77;32]),
        "external":{"branch_reference":"refs/heads/reviewed-candidate","remote_head":"reviewed-candidate","review_identity":"23","review_state":"open"}})
    );
    assert_eq!(
      record.discovery().and_then(FlowDiscoveryFacts::candidate_subject),
      Some(&candidate)
    );
    let requests = publisher.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].candidate, candidate);
    assert_eq!(requests[0].cycle_id, h.input.cycle_id());
    assert_eq!(requests[0].accepted_input_digest, h.input.digest().unwrap());
    assert_eq!(requests[0].deadline, at(511));
    assert_eq!(requests[0].started_at, at(11));
    assert_eq!(requests[0].authority_expires_at, at(100));
  });
}

#[test]
fn publication_rejects_an_unversioned_target_policy_before_external_effects() {
  crate::factory_admission_tests::run_ready(async {
    let mut h = harness_with_result(
      binding(
        "publisher",
        "publish_candidate",
        serde_json::json!({"bundle":"candidate_bytes","manifest":"candidate_manifest","target":"mutable-alias"}),
      ),
      permissions("publisher"),
      publication_schema(),
    );
    attach_candidate(&mut h);
    let publisher = Arc::new(Publisher {
      requests: Mutex::new(vec![]),
      published: Mutex::new(None),
      lose_response: std::sync::atomic::AtomicBool::new(false),
    });
    let adapter = Arc::new(FactoryFlowActionAdapter::new(Arc::new(
      FactoryCandidatePublicationAction::new(reference("publisher"), publisher.clone(), h.outcome.clone()),
    )));
    let runner = FactoryNodeRunner::new(h.store.clone(), adapter, crate::factory_node_test_support::clock());
    runner
      .run_once(h.run, h.claim, h.target.clone(), Some(h.input.clone()), at(11))
      .await
      .unwrap();
    assert!(
      runner
        .run_once(h.run, h.claim, h.target.clone(), None, at(12))
        .await
        .is_err(),
      "mutable target aliases cannot redirect frozen publication requests"
    );
    assert!(publisher.requests.lock().unwrap().is_empty());
  });
}

#[test]
fn publication_rejects_an_incompatible_receipt_contract_before_any_external_write() {
  crate::factory_admission_tests::run_ready(async {
    let mut h = harness(
      binding(
        "publisher",
        "publish_candidate",
        serde_json::json!({"bundle":"candidate_bytes","manifest":"candidate_manifest","target":reference("review_branch")}),
      ),
      permissions("publisher"),
    );
    attach_candidate(&mut h);
    let publisher = Arc::new(Publisher {
      requests: Mutex::new(vec![]),
      published: Mutex::new(None),
      lose_response: std::sync::atomic::AtomicBool::new(false),
    });
    let runner = FactoryNodeRunner::new(
      h.store.clone(),
      Arc::new(FactoryFlowActionAdapter::new(Arc::new(
        FactoryCandidatePublicationAction::new(reference("publisher"), publisher.clone(), h.outcome),
      ))),
      crate::factory_node_test_support::clock(),
    );
    runner
      .run_once(h.run, h.claim, h.target.clone(), Some(h.input), at(11))
      .await
      .unwrap();
    assert!(runner.run_once(h.run, h.claim, h.target, None, at(12)).await.is_err());
    assert!(
      publisher.requests.lock().unwrap().is_empty(),
      "invalid output contracts must fail before credentials or publication"
    );
  });
}

#[test]
fn lost_publication_cannot_be_accepted_after_the_declared_review_expiry() {
  crate::factory_admission_tests::run_ready(async {
    let mut h = harness_with_result(
      binding(
        "publisher",
        "publish_candidate",
        serde_json::json!({"bundle":"candidate_bytes","manifest":"candidate_manifest","target":reference("review_branch"),"valid_until":{"value":{"mapping":"constant","value":14}}}),
      ),
      permissions("publisher"),
      publication_schema(),
    );
    attach_candidate(&mut h);
    let publisher = Arc::new(Publisher {
      requests: Mutex::new(vec![]),
      published: Mutex::new(None),
      lose_response: std::sync::atomic::AtomicBool::new(true),
    });
    let adapter = Arc::new(FactoryFlowActionAdapter::new(Arc::new(
      FactoryCandidatePublicationAction::new(reference("publisher"), publisher.clone(), h.outcome),
    )));
    let runner = FactoryNodeRunner::new(
      h.store.clone(),
      adapter.clone(),
      crate::factory_node_test_support::clock(),
    );
    runner
      .run_once(h.run, h.claim, h.target.clone(), Some(h.input), at(11))
      .await
      .unwrap();
    assert!(
      runner
        .run_once(h.run, h.claim, h.target.clone(), None, at(12))
        .await
        .is_err()
    );
    assert_eq!(
      publisher.requests.lock().unwrap().len(),
      1,
      "publication was valid before expiry"
    );
    let restarted = FactoryNodeRunner::new(h.store.clone(), adapter, crate::factory_node_test_support::clock());
    assert!(
      restarted
        .run_once(h.run, h.claim, h.target, None, at(14))
        .await
        .is_err()
    );
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
    assert_eq!(publisher.requests.lock().unwrap().len(), 1);
  });
}

#[test]
fn a_receipt_for_another_publication_cannot_authorize_the_same_candidate() {
  crate::factory_admission_tests::run_ready(async {
    let mut h = harness_with_result(
      binding(
        "publisher",
        "publish_candidate",
        serde_json::json!({"bundle":"candidate_bytes","manifest":"candidate_manifest","target":reference("review_branch")}),
      ),
      permissions("publisher"),
      publication_schema(),
    );
    attach_candidate(&mut h);
    let publisher = Arc::new(Publisher {
      requests: Mutex::new(vec![]),
      published: Mutex::new(None),
      lose_response: std::sync::atomic::AtomicBool::new(true),
    });
    let adapter = Arc::new(FactoryFlowActionAdapter::new(Arc::new(
      FactoryCandidatePublicationAction::new(reference("publisher"), publisher.clone(), h.outcome),
    )));
    let runner = FactoryNodeRunner::new(h.store.clone(), adapter, crate::factory_node_test_support::clock());
    runner
      .run_once(h.run, h.claim, h.target.clone(), Some(h.input), at(11))
      .await
      .unwrap();
    assert!(
      runner
        .run_once(h.run, h.claim, h.target.clone(), None, at(12))
        .await
        .is_err()
    );
    publisher.published.lock().unwrap().as_mut().unwrap().operation_id = FactoryDigest::from_bytes([99; 32]);
    assert!(runner.run_once(h.run, h.claim, h.target, None, at(13)).await.is_err());
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
    assert_eq!(publisher.requests.lock().unwrap().len(), 1);
  });
}

#[test]
fn publication_rejects_a_probe_only_receipt_schema_before_any_external_write() {
  crate::factory_admission_tests::run_ready(async {
    let mut wire = serde_json::to_value(publication_schema()).unwrap();
    wire["shape"]["fields"]["external"]["value"]["fields"]["branch_reference"]["value"] =
      serde_json::json!({"type":"enum","values":["x","x".repeat(1024)]});
    let schema = FlowDataSchema::new(
      key("probe.only.receipt"),
      key("v1"),
      serde_json::from_value(wire["shape"].clone()).unwrap(),
    )
    .unwrap();
    let mut h = harness_with_result(
      binding(
        "publisher",
        "publish_candidate",
        serde_json::json!({"bundle":"candidate_bytes","manifest":"candidate_manifest","target":reference("review_branch")}),
      ),
      permissions("publisher"),
      schema,
    );
    attach_candidate(&mut h);
    let publisher = Arc::new(Publisher {
      requests: Mutex::new(vec![]),
      published: Mutex::new(None),
      lose_response: std::sync::atomic::AtomicBool::new(false),
    });
    let runner = FactoryNodeRunner::new(
      h.store.clone(),
      Arc::new(FactoryFlowActionAdapter::new(Arc::new(
        FactoryCandidatePublicationAction::new(reference("publisher"), publisher.clone(), h.outcome.clone()),
      ))),
      crate::factory_node_test_support::clock(),
    );
    runner
      .run_once(h.run, h.claim, h.target.clone(), Some(h.input.clone()), at(11))
      .await
      .unwrap();
    assert!(runner.run_once(h.run, h.claim, h.target, None, at(12)).await.is_err());
    assert!(
      publisher.requests.lock().unwrap().is_empty(),
      "receipt compatibility must be established before exposing write authority"
    );
    assert!(publisher.published.lock().unwrap().is_none());
  });
}

#[test]
fn published_status_action_fixture_matches_the_installed_contract_and_rejects_unbounded_parameters() {
  let fixtures: serde_json::Value = serde_json::from_str(include_str!(
    "../../api/octacity-server-api-rest/fixtures/v1/integration-contracts.json"
  ))
  .unwrap();
  let parameters = &fixtures["FactoryWorkStatusActionParameters"][0];
  let outcome = FlowOutcomeDefinition::new(key("reported"), FlowOutcomeKind::Success, reference("action.result"));
  let reporter = Arc::new(Reporter {
    requests: Mutex::new(vec![]),
    unavailable: false,
    scoped: false,
  });
  let action = FactoryWorkStatusAction::new(reference("reporter"), reporter.clone(), outcome.clone());
  let validate = |parameters| {
    action.validate(
      &binding("reporter", "set_status", parameters),
      std::slice::from_ref(&outcome),
    )
  };
  assert!(validate(parameters.clone()).is_ok());
  for invalid in [
    serde_json::json!({"status":"arbitrary-outcome","outbox":{"max_attempts":0,"retry_delay_millis":1}}),
    serde_json::json!({"status":"arbitrary-outcome","outbox":{"max_attempts":1025,"retry_delay_millis":1}}),
    serde_json::json!({"status":"arbitrary-outcome","outbox":{"max_attempts":1,"retry_delay_millis":3600001}}),
    serde_json::json!({"status":"я".repeat(1025)}),
    serde_json::json!({"status":"arbitrary-outcome","issue_id":"retarget-forbidden"}),
  ] {
    assert!(validate(invalid).is_err());
  }
  assert!(reporter.requests.lock().unwrap().is_empty());
}
