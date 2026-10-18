//! An active automatic input must match the source effects in its exact target closure.
use super::*;

async fn stage_target(fixture: &mut Fixture, update: &FactoryStageSourceUpdate, nested: bool) {
  let schema = FlowDataSchema::new(
    key("reporting.receipt"),
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
  let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: vec![update.reporter.capability.clone()],
    ..Default::default()
  })
  .unwrap();
  let budget = BudgetLimit::new(4, 1000, 100, 100, 10000).unwrap();
  let node = FlowNodeDefinition::new(FlowNodeDefinitionInput {
    key: key("update-original-task"),
    kind: FlowNodeKind::TrustedAction,
    input_schema: None,
    outcomes: vec![FlowOutcomeDefinition::new(
      key("done"),
      FlowOutcomeKind::Success,
      schema.reference().clone(),
    )],
    budget,
    permissions: permissions.clone(),
    required: true,
    subflow: None,
  })
  .unwrap()
  .with_action(
    FlowActionBinding::new(
      update.reporter.capability.clone(),
      key("set_status"),
      serde_json::json!({"status":update.effects.status,"source_update":update,
      "outbox":{"max_attempts":2,"retry_delay_millis":5}}),
    )
    .unwrap(),
  )
  .unwrap();
  let root = FlowDefinition::new(FlowDefinitionInput {
    id: FlowDefinitionId::generate(),
    version: FlowDefinitionVersion::INITIAL,
    input_schema: None,
    entry: node.key().clone(),
    nodes: vec![node],
    transitions: vec![FlowTransition::new(
      key("update-original-task"),
      key("done"),
      FlowTransitionTarget::Terminal(key("done")),
    )],
    terminals: vec![FlowTerminalDefinition::new(key("done"), schema.reference().clone())],
    context_projections: vec![],
    data_projections: vec![],
    execution: FlowExecutionPolicy::new(budget, permissions.clone(), 1).unwrap(),
  })
  .unwrap();
  let closure = if nested {
    let call = FlowNodeDefinition::new(FlowNodeDefinitionInput {
      key: key("nested-stage"),
      kind: FlowNodeKind::SubflowCall,
      input_schema: None,
      outcomes: vec![FlowOutcomeDefinition::new(
        key("done"),
        FlowOutcomeKind::Success,
        schema.reference().clone(),
      )],
      budget,
      permissions: permissions.clone(),
      required: true,
      subflow: Some(root.reference()),
    })
    .unwrap();
    let parent = FlowDefinition::new(FlowDefinitionInput {
      id: FlowDefinitionId::generate(),
      version: FlowDefinitionVersion::INITIAL,
      input_schema: None,
      entry: call.key().clone(),
      nodes: vec![call],
      transitions: vec![FlowTransition::new(
        key("nested-stage"),
        key("done"),
        FlowTransitionTarget::Terminal(key("done")),
      )],
      terminals: vec![FlowTerminalDefinition::new(key("done"), schema.reference().clone())],
      context_projections: vec![],
      data_projections: vec![],
      execution: FlowExecutionPolicy::new(budget, permissions.clone(), 1).unwrap(),
    })
    .unwrap();
    PinnedFlowDefinitionClosure::new(parent.reference(), vec![parent, root]).unwrap()
  } else {
    PinnedFlowDefinitionClosure::new(root.reference(), vec![root]).unwrap()
  };
  let flow = FactoryFlowConfiguration::new(
    closure,
    FlowAdmissionLimits::product_defaults(1, budget, permissions).unwrap(),
    vec![schema],
    Default::default(),
  )
  .unwrap();
  let target = configuration_with_flow(
    fixture.intent.project_id,
    FactoryConfigurationId::generate(),
    Some(flow),
  );
  fixture
    .store
    .seed_factory_configuration_version(PublishedFactoryConfiguration {
      configuration: target.clone(),
      published_at: time(4),
    })
    .unwrap();
  fixture.intent.inputs[0].definition.target = target.reference().clone();
}

#[test]
fn automatic_input_rejects_a_stage_connection_revision_that_cannot_report_for_its_work() {
  run_ready(async {
    for provider in ["github", "custom-tracker"] {
      let mut fixture = Fixture::with_writer(provider, true).await;
      let writer = fixture.reporter.clone().unwrap();
      fixture.intent.inputs[0].definition.reporter = Some(writer.binding.clone());
      fixture.intent.inputs[0]
        .definition
        .connection
        .as_mut()
        .unwrap()
        .reporter_configuration = Some(writer.binding.configuration.clone());
      let original = fixture.intent.inputs[0].definition.clone();
      let update = FactoryStageSourceUpdate {
        source: original.source.clone(),
        reporter: writer.binding.clone(),
        connection: original.connection.clone(),
        effects: mapping().statuses[0].clone(),
      };
      let handlers = Arc::new(
        fixture
          .handlers()
          .with_reporting_mapping(writer, mapping(), Some(policy()))
          .unwrap(),
      );
      handlers
        .validate_source_update(&context(), fixture.intent.project_id, &update)
        .await
        .unwrap();
      stage_target(&mut fixture, &update, false).await;
      let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
      queries
        .handle_query(
          &context(),
          ValidateWorkSourceConfigurationQuery {
            intent: fixture.intent.clone(),
          },
        )
        .await
        .unwrap();
      let connection = original.connection.unwrap();
      let grant = fixture
        .store
        .integration_project_access(connection.id, fixture.intent.project_id)
        .await
        .unwrap()
        .unwrap();
      let next = fixture
        .store
        .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
          intent: IntegrationProjectAccessIntent {
            connection_id: connection.id,
            connection_version: connection.version,
            project_id: fixture.intent.project_id,
            expected_current_version: Some(connection.access_version),
            resources: grant.resources,
          },
          idempotency_key: IdempotencyKey::new("new-read-write-revision").unwrap(),
          published_at: time(5),
        }))
        .await
        .unwrap();
      fixture.intent.inputs[0]
        .definition
        .connection
        .as_mut()
        .unwrap()
        .access_version = next.access.version;
      let error = queries
        .handle_query(
          &context(),
          ValidateWorkSourceConfigurationQuery {
            intent: fixture.intent.clone(),
          },
        )
        .await
        .expect_err(
          "an enabled input cannot execute the old pinned stage effect under a different connection revision",
        );
      let ManagementHandlerError::Application(ApplicationError::FactoryValidation(report)) = error else {
        panic!("expected safe target compatibility diagnostics, got {error:?}")
      };
      assert_eq!(report.errors[0].field, "/inputs/0/definition/target");
      let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
      assert!(
        commands
          .handle_command(
            &context(),
            PublishWorkSourceConfigurationCommand {
              intent: fixture.intent.clone(),
              idempotency_key: IdempotencyKey::new("incompatible-stage-input").unwrap(),
              published_at: time(6),
            }
          )
          .await
          .is_err()
      );
      assert!(
        fixture
          .store
          .source_scan_targets(fixture.intent.project_id, None, 64)
          .await
          .unwrap()
          .is_empty()
      );
      let mut repaired = update;
      repaired.connection = fixture.intent.inputs[0].definition.connection.clone();
      stage_target(&mut fixture, &repaired, false).await;
      let published = commands
        .handle_command(
          &context(),
          PublishWorkSourceConfigurationCommand {
            intent: fixture.intent.clone(),
            idempotency_key: IdempotencyKey::new("compatible-current-stage-input").unwrap(),
            published_at: time(7),
          },
        )
        .await
        .expect("matching reviewed pins must still activate an input");
      assert_eq!(
        published.publication.inputs[0].input.definition(),
        &fixture.intent.inputs[0].definition
      );
      assert_eq!(
        fixture
          .store
          .source_scan_targets(fixture.intent.project_id, None, 64)
          .await
          .unwrap()
          .len(),
        1
      );
    }
  });
}

#[test]
fn automatic_input_checks_exact_source_and_reporter_pins_in_nested_stage_effects_before_activation() {
  run_ready(async {
    for provider in ["github", "custom-tracker"] {
      for mismatch in ["source", "reporter", "no-reporter"] {
        let mut fixture = Fixture::with_writer(provider, true).await;
        let writer = fixture.reporter.clone().unwrap();
        fixture.intent.inputs[0].definition.reporter = Some(writer.binding.clone());
        fixture.intent.inputs[0]
          .definition
          .connection
          .as_mut()
          .unwrap()
          .reporter_configuration = Some(writer.binding.configuration.clone());
        let original = fixture.intent.inputs[0].definition.clone();
        let update = FactoryStageSourceUpdate {
          source: original.source.clone(),
          reporter: writer.binding.clone(),
          connection: original.connection.clone(),
          effects: mapping().statuses[0].clone(),
        };
        let mut other = if mismatch == "source" {
          fixture.capability.clone()
        } else {
          writer.clone()
        };
        let parent_configuration = other.binding.configuration.clone();
        other.binding.configuration = exact("separately-installed-profile", 90);
        let handlers = FactorySourceConfigurationHandlers::new(
          fixture.store.clone(),
          Arc::new(TrustedNetworkManagementPolicy),
          vec![fixture.capability.clone(), writer.clone(), other.clone()],
        )
        .unwrap()
        .with_connection_configuration(other.clone(), parent_configuration)
        .unwrap()
        .with_reporting_mapping(writer, mapping(), Some(policy()))
        .unwrap();
        handlers
          .validate_source_update(&context(), fixture.intent.project_id, &update)
          .await
          .unwrap();
        stage_target(&mut fixture, &update, true).await;
        let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(handlers));
        queries
          .handle_query(
            &context(),
            ValidateWorkSourceConfigurationQuery {
              intent: fixture.intent.clone(),
            },
          )
          .await
          .unwrap();
        match mismatch {
          "source" => fixture.intent.inputs[0].definition.source = other.binding,
          "reporter" => fixture.intent.inputs[0].definition.reporter = Some(other.binding),
          "no-reporter" => {
            fixture.intent.inputs[0].definition.reporter = None;
            fixture.intent.inputs[0]
              .definition
              .connection
              .as_mut()
              .unwrap()
              .reporter_configuration = None;
          }
          _ => unreachable!(),
        }
        let error = queries
          .handle_query(
            &context(),
            ValidateWorkSourceConfigurationQuery {
              intent: fixture.intent.clone(),
            },
          )
          .await
          .unwrap_err();
        let ManagementHandlerError::Application(ApplicationError::FactoryValidation(report)) = error else {
          panic!("expected target mismatch for {provider}/{mismatch}, got {error:?}")
        };
        assert_eq!(report.errors[0].field, "/inputs/0/definition/target");
        fixture.intent.inputs[0].definition.enabled = false;
        queries
          .handle_query(
            &context(),
            ValidateWorkSourceConfigurationQuery {
              intent: fixture.intent.clone(),
            },
          )
          .await
          .expect("a disabled input remains retainable for repair without executing its incompatible effects");
      }
    }
  });
}
