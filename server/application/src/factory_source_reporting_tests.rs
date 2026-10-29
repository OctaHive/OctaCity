//! Input-owned effects are selectable only under an installed, currently granted Reporter.
use super::*;

fn mapping() -> WorkSourceStatusMapping {
  serde_json::from_value(serde_json::json!({"statuses":[{"status":"editorial-ready","add":["ready / review"],"remove":["needs context"],"state":null}]})).unwrap()
}
fn policy() -> WorkSourceReportingPolicy {
  WorkSourceReportingPolicy {
    ownership: None,
    labels: vec!["ready / review".into(), "needs context".into()],
    states: vec!["native-closed".into()],
  }
}

#[test]
fn stage_source_effects_require_current_separate_write_authority_and_installed_editing_ceiling() {
  run_ready(async {
    for provider in ["github", "custom-tracker"] {
      let fixture = Fixture::with_writer(provider, true).await;
      let writer = fixture.reporter.clone().unwrap();
      let handlers = fixture
        .handlers()
        .with_reporting_mapping(writer.clone(), mapping(), Some(policy()))
        .unwrap();
      let mut connection = fixture.intent.inputs[0].definition.connection.clone().unwrap();
      connection.reporter_configuration = Some(writer.binding.configuration.clone());
      let update = FactoryStageSourceUpdate {
        ownership_scopes: None,
        source: fixture.capability.binding.clone(),
        reporter: writer.binding,
        connection: Some(connection.clone()),
        effects: mapping().statuses[0].clone(),
      };
      handlers
        .validate_source_update(&context(), fixture.intent.project_id, &update)
        .await
        .unwrap();
      let mut substituted = update.clone();
      substituted.ownership_scopes = Some(["invented-independent-scope".parse().unwrap()].into());
      assert!(
        handlers
          .validate_source_update(&context(), fixture.intent.project_id, &substituted)
          .await
          .is_err(),
        "authoring cannot invent independent ownership absent from the installed Reporter"
      );
      let mut invalid = update.clone();
      invalid.effects.add = vec!["outside-authority".into()];
      assert!(
        handlers
          .validate_source_update(&context(), fixture.intent.project_id, &invalid)
          .await
          .is_err()
      );
      let mut invalid = update.clone();
      invalid.reporter.configuration = exact("another-profile", 87);
      assert!(
        handlers
          .validate_source_update(&context(), fixture.intent.project_id, &invalid)
          .await
          .is_err()
      );
      fixture
        .store
        .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
          intent: IntegrationProjectAccessIntent {
            connection_id: connection.id,
            connection_version: connection.version,
            project_id: fixture.intent.project_id,
            expected_current_version: Some(connection.access_version),
            resources: vec![],
          },
          idempotency_key: IdempotencyKey::new("revoke-stage-authority").unwrap(),
          published_at: time(20),
        }))
        .await
        .unwrap();
      assert!(
        handlers
          .validate_source_update(&context(), fixture.intent.project_id, &update)
          .await
          .is_err()
      );
    }
  });
}
#[test]
fn source_reporting_choices_and_publication_pin_arbitrary_effects_under_separate_write_authority() {
  run_ready(async {
    for provider in ["github", "custom-tracker"] {
      let mut fixture = Fixture::with_writer(provider, true).await;
      let reporter = fixture.reporter.clone().unwrap();
      let handlers = fixture
        .handlers()
        .with_reporting_mapping(reporter.clone(), mapping(), Some(policy()))
        .unwrap();
      let handlers = Arc::new(handlers);
      let queries = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers.clone());
      let connection = fixture.intent.inputs[0].definition.connection.as_ref().unwrap();
      let choices = queries
        .handle_query(
          &context(),
          ListWorkSourceInputChoicesQuery {
            project_id: fixture.intent.project_id,
            connection_id: connection.id,
            connection_version: connection.version,
          },
        )
        .await
        .unwrap();
      let writer = choices
        .items
        .iter()
        .find(|c| c.purpose == IntegrationPurpose::TaskUpdate)
        .unwrap();
      assert_eq!(writer.reporting.as_ref().unwrap().mapping, mapping());
      assert_eq!(writer.reporting.as_ref().unwrap().editable.as_ref(), Some(&policy()));
      fixture.intent.inputs[0].definition.reporter = Some(reporter.binding.clone());
      fixture.intent.inputs[0]
        .definition
        .connection
        .as_mut()
        .unwrap()
        .reporter_configuration = Some(reporter.binding.configuration.clone());
      fixture.intent.inputs[0].definition.reporting = Some(mapping());
      let commands = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), handlers);
      let command = PublishWorkSourceConfigurationCommand {
        intent: fixture.intent.clone(),
        idempotency_key: IdempotencyKey::new("arbitrary-status-mapping").unwrap(),
        published_at: time(10),
      };
      let original = commands.handle_command(&context(), command.clone()).await.unwrap();
      assert_eq!(
        original.publication.inputs[0].input.definition().reporting,
        Some(mapping())
      );
      assert_eq!(
        commands.handle_command(&context(), command).await.unwrap().publication,
        original.publication
      );
      let mut invalid = fixture.intent.clone();
      invalid.inputs[0].definition.reporting.as_mut().unwrap().statuses[0].add = vec!["outside-ceiling".into()];
      assert!(
        queries
          .handle_query(&context(), ValidateWorkSourceConfigurationQuery { intent: invalid })
          .await
          .is_err()
      );
      let legacy = AuthorizedQueryHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(fixture.handlers()));
      assert!(
        legacy
          .handle_query(
            &context(),
            ValidateWorkSourceConfigurationQuery { intent: fixture.intent }
          )
          .await
          .is_err(),
        "legacy installed Reporter cannot acquire editing support implicitly"
      );
    }
  });
}

#[path = "factory_source_compatibility_tests.rs"]
mod compatibility;
