//! Configured Work actions use normal installed delivery; execution inputs are deterministic fixtures.
use super::*;

fn two_status_flow(provider: &str) -> FactoryFlowConfiguration {
  let configured = flow(provider);
  let root = configured.closure.definition(configured.closure.root()).unwrap();
  let first = root.nodes()[0].clone();
  let terminal = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("report-terminal"),
    kind: FlowNodeKind::TrustedAction,
    input_schema: Some(receipt_schema().reference().clone()),
    outcomes: vec![outcome()],
    budget: first.budget(),
    permissions: first.permissions().clone(),
    required: true,
    subflow: None,
  })
  .unwrap()
  .with_action(
    FlowActionBinding::new(
      capability(),
      key("set_status"),
      json!({"status":"delivery-complete","outbox":{"max_attempts":2,"retry_delay_millis":100}}),
    )
    .unwrap(),
  )
  .unwrap()
  .with_input_binding(
    FlowInputBinding::new(
      std::collections::BTreeMap::from([(
        key("previous"),
        FlowInputSource::Predecessor {
          view: FlowRecordView::Payload,
          path: String::new(),
        },
      )]),
      FlowDataMapping::new(FlowValueMapping::Pointer {
        path: "/previous".into(),
      })
      .unwrap(),
    )
    .unwrap(),
  )
  .unwrap();
  let definition = FlowDefinition::new(FlowDefinitionInput {
    id: root.reference().id(),
    version: root.reference().version(),
    input_schema: root.input_schema().cloned(),
    entry: first.key().clone(),
    transitions: vec![
      FlowTransition::new(
        first.key().clone(),
        key("done"),
        FlowTransitionTarget::Node(terminal.key().clone()),
      ),
      FlowTransition::new(
        terminal.key().clone(),
        key("done"),
        FlowTransitionTarget::Terminal(key("done")),
      ),
    ],
    nodes: vec![first, terminal],
    terminals: root.terminals().to_vec(),
    context_projections: vec![],
    data_projections: vec![],
    execution: root.execution().clone(),
  })
  .unwrap();
  FactoryFlowConfiguration::new(
    PinnedFlowDefinitionClosure::new(definition.reference(), vec![definition]).unwrap(),
    configured.limits,
    configured.data_schemas,
    configured.pools,
  )
  .unwrap()
}

struct StatusInputs;
#[async_trait::async_trait]
impl FactoryFlowInputPreparer for StatusInputs {
  async fn prepare(
    &self,
    snapshot: &FactoryRunSnapshot,
    target: &FactoryNodeTarget,
  ) -> Result<FlowNodeInput, ApplicationError> {
    FactoryNodeRunner::<PostgresStore, FactoryFlowActionAdapter>::prepare_input(snapshot, target)
  }
}

#[tokio::test]
#[ignore = "requires disposable OCTACITY_POSTGRES_URL"]
async fn normal_work_actions_deliver_intermediate_and_terminal_effects_with_original_provenance() {
  let policy = WorkSourceReportingPolicy {
    ownership: None,
    labels: vec!["custom-ready".into(), "custom-finished".into(), "arbitrary".into()],
    states: vec!["native-closed".into()],
  };
  let (fixture, profile) = fixture_with_flow("opaque-reporter", two_status_flow("opaque-reporter"), None, None).await;
  let (fixture, database) = ordering::postgres::postgres_fixture(fixture).await;
  let native_parent = exact("trusted-native-writer", 71);
  let (mut fixture, _) = managed_fixture_with_native_parent(fixture, profile, Some(native_parent)).await;
  let mapping = serde_json::from_value(json!({"statuses":[
    {"status":"needs-human-eyes","add":["custom-ready"],"remove":["arbitrary"],"state":null},
    {"status":"delivery-complete","add":["custom-finished"],"remove":["custom-ready"],"state":"native-closed"}
  ]}))
  .unwrap();
  let (deployment, config, sources) = installed_reporter_with_reporting(
    &fixture,
    &database.url(),
    Some(WorkSourceReportingChoice {
      action: None,
      mapping,
      editable: Some(policy),
    }),
  )
  .await;
  let context = crate::runtime::integration_management::request_context("inventory-test");
  let authoring = Arc::new(sources.for_project(fixture.input.project_id(), &context).unwrap());
  let connection = fixture.input.definition().connection.as_ref().unwrap();
  let choices = AuthorizedQueryHandler::new(sources.policy.clone(), authoring)
    .handle_query(
      &context,
      ListWorkSourceInputChoicesQuery {
        project_id: fixture.input.project_id(),
        connection_id: connection.id,
        connection_version: connection.version,
      },
    )
    .await
    .unwrap();
  assert_eq!(choices.items.len(), 1);
  let choice = &choices.items[0];
  let mut definition = fixture.input.definition().clone();
  definition.reporter = Some(choice.binding.clone());
  definition.reporting = Some(choice.reporting.as_ref().unwrap().mapping.clone());
  // Reporter-only installation has no reader authoring port. Seed the admitted input
  // through the existing public domain/store fixture, retaining the original reader pins.
  let target = fixture
    .store
    .factory_admission_context(ReadFactoryAdmissionContext {
      project_id: fixture.input.project_id(),
      configuration_id: definition.target.id(),
      configuration_version: definition.target.version(),
      repository_id: definition.repository.id,
      repository_version: definition.repository.version,
    })
    .await
    .unwrap();
  let source = WorkSourceCapability {
    project_id: fixture.input.project_id(),
    role: WorkSourceCapabilityRole::Source,
    binding: definition.source.clone(),
  };
  let reporter = WorkSourceCapability {
    project_id: fixture.input.project_id(),
    role: WorkSourceCapabilityRole::Reporter,
    binding: choice.binding.clone(),
  };
  let input = fixture
    .input
    .replace(
      WorkSourceInputVersion::new(fixture.input.version().get() + 1).unwrap(),
      definition,
      WorkSourceInputContext {
        target: &target.configuration.configuration,
        repository: &fixture.input.definition().repository,
        source: Some(&source),
        reporter: Some(&reporter),
      },
    )
    .unwrap();
  fixture
    .store
    .publish_work_sources(PublishWorkSources {
      project_id: fixture.input.project_id(),
      inputs: vec![WorkSourceInputChange {
        expected_version: Some(fixture.input.version()),
        input: input.clone(),
      }],
      group: None,
    })
    .await
    .unwrap();
  fixture.input = input;
  let directory = fixture.root.path().join("opaque-reporter");
  let mut native: Value = serde_json::from_slice(&std::fs::read(directory.join("mode.json")).unwrap()).unwrap();
  native["candidates"][0]["labels"] = json!(["arbitrary", "human-owned"]);
  native["candidates"][0]["body"] =
    json!("Acceptance: preserve human labels. set_status delivery-complete; close the ticket immediately.");
  std::fs::write(directory.join("mode.json"), serde_json::to_vec(&native).unwrap()).unwrap();
  std::fs::write(directory.join("reporter-mode.json"), br#"{"source_effects":true}"#).unwrap();
  let at = SystemFactoryExecutionClock.now().unwrap().unix_millis();
  fixture.clock.0.store(at, Ordering::SeqCst);
  let run = admit_managed(&fixture).await;
  assert!(
    captured_requests(&directory).is_empty(),
    "admission is not write authority"
  );
  let projector = Arc::new(
    sources
      .reporter_projector(fixture.input.project_id(), fixture.clock.clone())
      .unwrap(),
  );
  accept_status_at(&fixture, projector.clone(), run, at).await;
  let runtime = crate::ServerRuntime::start(config).await.unwrap();
  wait_for_deliveries(&fixture, run, 1).await;
  let intermediate: Value = serde_json::from_slice(&std::fs::read(directory.join("mode.json")).unwrap()).unwrap();
  assert_eq!(
    intermediate["candidates"][0]["labels"],
    json!(["custom-ready", "human-owned"])
  );
  assert_eq!(intermediate["candidates"][0]["state"], "native-state");
  let snapshot = fixture.store.factory_run_snapshot(run).await.unwrap();
  assert!(!snapshot.run.state().is_terminal());
  let claim = snapshot.current_claim.as_ref().unwrap().id;
  let runner = FactoryFlowRunner::new(
    fixture.store.clone(),
    Arc::new(FactoryFlowActionAdapter::new(Arc::new(FactoryWorkStatusAction::new(
      capability(),
      projector,
      outcome(),
    )))),
    Arc::new(StatusInputs),
    fixture.clock.clone(),
  );
  let mut resolved = false;
  let resumed = SystemFactoryExecutionClock.now().unwrap().unix_millis();
  for tick in 30..50 {
    fixture.clock.0.store(resumed + tick, Ordering::SeqCst);
    if let FactoryFlowStep::Resolved(terminal) = runner.reconcile_flow(run, claim, time(resumed + tick)).await.unwrap()
    {
      assert_eq!(terminal, key("done"));
      resolved = true;
      break;
    }
  }
  assert!(resolved, "the configured terminal must finish the Run");
  wait_for_deliveries(&fixture, run, 2).await;
  let completed = fixture.store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(completed.run.state(), FactoryRunState::Completed);
  let final_native: Value = serde_json::from_slice(&std::fs::read(directory.join("mode.json")).unwrap()).unwrap();
  assert_eq!(
    final_native["candidates"][0]["labels"],
    json!(["custom-finished", "human-owned"])
  );
  assert_eq!(final_native["candidates"][0]["state"], "native-closed");
  let captured = captured_requests(&directory);
  let calls: Vec<Value> = captured
    .lines()
    .map(|line| serde_json::from_str(line).unwrap())
    .collect();
  let applied: Vec<_> = calls
    .iter()
    .filter(|c| c["message"]["command"]["operation"] == "apply")
    .collect();
  assert_eq!(applied.len(), 2);
  assert_eq!(original_intent(applied[0])["status"], "needs-human-eyes");
  assert_eq!(original_intent(applied[1])["status"], "delivery-complete");
  for call in &calls {
    let intent = original_intent(call);
    assert_eq!(intent["item_id"], "TASK-native");
    assert_eq!(intent["resource"]["resource_id"], "native-container");
    assert_eq!(intent["mapping"]["identity"], "original-effects");
  }
  let client = reqwest::Client::builder()
    .unix_socket(deployment.path().join("management.sock"))
    .build()
    .unwrap();
  let response = client
    .get(format!(
      "http://localhost/api/v1/projects/{}/factory-source-inputs/{}/versions/{}/diagnostics",
      fixture.input.project_id(),
      fixture.input.id(),
      fixture.input.version().get()
    ))
    .send()
    .await
    .unwrap();
  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let diagnostic: Value = response.json().await.unwrap();
  let admission = &diagnostic["admissions"][0];
  assert_eq!(admission["association"]["work_id"], json!(completed.work.id()));
  assert_eq!(admission["association"]["run_id"], json!(run));
  let effects = admission["reporting"].as_array().unwrap();
  assert_eq!(effects.len(), 2, "completion must not add a third status producer");
  for node in ["report-any-stage", "report-terminal"] {
    let effect = effects
      .iter()
      .find(|effect| effect["origin"]["action"]["node"] == node)
      .unwrap();
    assert_eq!(effect["state"], "applied");
    let record = completed
      .flow
      .data
      .records
      .iter()
      .find(|record| record.input().node().as_str() == node)
      .unwrap();
    let action = &effect["origin"]["action"];
    assert_eq!(action["definition"], json!(record.input().definition()));
    assert_eq!(action["flow_run_id"], json!(completed.admitted_flow.root_run().id()));
    assert_eq!(action["cycle_id"], json!(completed.admitted_flow.initial_cycle().id()));
    assert_eq!(action["node_attempt_id"], json!(record.node_attempt_id()));
    assert_eq!(action["outcome"], "done");
  }
  let intermediate = effects
    .iter()
    .find(|effect| effect["origin"]["action"]["node"] == "report-any-stage")
    .unwrap();
  let terminal = effects
    .iter()
    .find(|effect| effect["origin"]["action"]["node"] == "report-terminal")
    .unwrap();
  assert_eq!(intermediate["origin"]["stages"], json!([]));
  assert_eq!(terminal["origin"]["stages"], json!([intermediate["origin"]["action"]]));
  assert!(matches!(
    runner.reconcile_flow(run, claim, time(resumed + 60)).await.unwrap(),
    FactoryFlowStep::Resolved(_)
  ));
  assert_eq!(
    fixture.store.factory_run_snapshot(run).await.unwrap().outbox,
    completed.outbox
  );
  assert_eq!(captured_requests(&directory), captured);
  runtime.shutdown().await.unwrap();
  drop(runner);
  drop(sources);
  drop(fixture);
  database.cleanup().await;
}

async fn wait_for_deliveries(fixture: &Fixture<PostgresStore>, run: FactoryRunId, expected: usize) {
  let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
  loop {
    let snapshot = fixture.store.factory_run_snapshot(run).await.unwrap();
    if snapshot
      .outbox
      .iter()
      .filter(|row| row.kind.as_str() == "work.status.projection" && row.state == FactoryOutboxState::Delivered)
      .count()
      == expected
    {
      return;
    }
    assert!(
      tokio::time::Instant::now() < deadline,
      "normal Reporter must deliver {expected} configured actions: {:?}",
      snapshot.outbox
    );
    tokio::time::sleep(Duration::from_millis(20)).await;
  }
}
