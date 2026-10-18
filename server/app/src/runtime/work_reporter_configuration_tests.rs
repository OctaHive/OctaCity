//! Retained input connection selection controls Reporter use as well as discovery.
use super::*;
use octacity_server_store::testing::management_mutation;

async fn managed_fixture(provider: &str) -> (Fixture, FactoryReporterProfile) {
  let (mut fixture, profile) = fixture(provider).await;
  let (_, id) = managed_access(&fixture, &fixture.profile.read.binding, IntegrationPurpose::TaskRead).await;
  let metadata = fixture.store.integration_connection(id, None).await.unwrap();
  let mut resources = metadata.definition.resources.clone();
  let writer = IntegrationCapability {
    purpose: IntegrationPurpose::TaskUpdate,
    capability: profile.binding().capability.clone(),
    configuration: profile.binding().configuration.clone(),
    credential: serde_json::from_value(serde_json::json!({"provider":"installed","reference":"independent-writer"}))
      .unwrap(),
  };
  resources[0].capabilities.push(writer.clone());
  let selection = vec![IntegrationResourceSelection {
    reference: resources[0].reference.clone(),
    purposes: vec![IntegrationPurpose::TaskRead, IntegrationPurpose::TaskUpdate],
  }];
  let installed = InstalledIntegration {
    installation: metadata.definition.installation.clone(),
    provider: metadata.definition.provider.clone(),
    adapter: metadata.definition.adapter.clone(),
    endpoints: vec![metadata.definition.endpoint.clone()],
    authentication: vec![metadata.definition.authentication.clone()],
    resources,
  };
  let updated = fixture
    .store
    .publish_integration_connection(management_mutation(PublishIntegrationConnection {
      intent: IntegrationConnectionIntent {
        id,
        expected_current_version: Some(metadata.version),
        draft: IntegrationConnectionDraft {
          name: metadata.definition.name.clone(),
          installation: metadata.definition.installation.clone(),
          endpoint: metadata.definition.endpoint.clone(),
          authentication: metadata.definition.authentication.clone(),
          resources: selection.clone(),
        },
      },
      installed,
      idempotency_key: IdempotencyKey::new("enable-input-writer").unwrap(),
      published_at: time(4),
    }))
    .await
    .unwrap()
    .connection;
  let state = fixture.store.integration_administration(id).await.unwrap();
  let receipt = fixture
    .store
    .begin_integration_administration(management_mutation(BeginIntegrationAdministration {
      intent: IntegrationAdministrationIntent {
        id,
        expected_metadata_version: updated.version,
        expected_revision: state.revision,
        action: IntegrationAdministrationAction::Credential {
          reference: writer.credential,
          kind: serde_json::from_value(serde_json::json!("adopt")).unwrap(),
          activate_connection: true,
        },
      },
      idempotency_key: IdempotencyKey::new("activate-input-writer").unwrap(),
      requested_at: time(5),
    }))
    .await
    .unwrap();
  fixture
    .store
    .complete_integration_administration(management_mutation(receipt))
    .await
    .unwrap();
  let access = fixture
    .store
    .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
      intent: IntegrationProjectAccessIntent {
        connection_id: id,
        connection_version: updated.version,
        project_id: fixture.input.project_id(),
        expected_current_version: Some(IntegrationVersion::INITIAL),
        resources: selection,
      },
      idempotency_key: IdempotencyKey::new("grant-input-writer").unwrap(),
      published_at: time(6),
    }))
    .await
    .unwrap()
    .access;
  let source = WorkSourceCapability {
    project_id: fixture.input.project_id(),
    role: WorkSourceCapabilityRole::Source,
    binding: fixture.profile.read.binding.clone(),
  };
  let reporter = WorkSourceCapability {
    project_id: fixture.input.project_id(),
    role: WorkSourceCapabilityRole::Reporter,
    binding: profile.binding().clone(),
  };
  let mut definition = fixture.input.definition().clone();
  definition.connection = Some(WorkSourceConnectionRef {
    id,
    version: updated.version,
    access_version: access.version,
    source_configuration: source.binding.configuration.clone(),
    reporter_configuration: Some(reporter.binding.configuration.clone()),
  });
  let handlers = FactorySourceConfigurationHandlers::new(
    fixture.store.clone(),
    Arc::new(TrustedNetworkManagementPolicy),
    vec![source, reporter],
  )
  .unwrap();
  let published = AuthorizedCommandHandler::new(Arc::new(TrustedNetworkManagementPolicy), Arc::new(handlers))
    .handle_command(
      &fixture.profile.read.context,
      PublishWorkSourceConfigurationCommand {
        intent: WorkSourceConfigurationIntent {
          project_id: fixture.input.project_id(),
          inputs: vec![WorkSourceInputIntent {
            id: fixture.input.id(),
            expected_version: Some(fixture.input.version()),
            definition,
          }],
          group: None,
        },
        idempotency_key: IdempotencyKey::new("managed-reporter-input").unwrap(),
        published_at: time(7),
      },
    )
    .await
    .unwrap();
  fixture.input = published.publication.inputs[0].input.clone();
  (fixture, profile)
}

async fn admit_managed(fixture: &Fixture) -> FactoryRunId {
  let worker = fixture
    .worker()
    .with_managed_connections(ManagedIntegrationConnections::new(fixture.store.clone()));
  assert_eq!(worker.run_once(CancellationToken::new()).await.unwrap().completed, 1);
  fixture.runs().await[0].id
}

#[tokio::test]
async fn managed_reporter_input_cannot_borrow_legacy_profile_without_connection_resolution() {
  let (fixture, profile) = managed_fixture("custom-tracker").await;
  let run = admit_managed(&fixture).await;
  let projector = hosted(&fixture, vec![profile]);
  accept_status_at(&fixture, projector.clone(), run, 100).await;
  let before = fixture.store.factory_run_snapshot(run).await.unwrap();
  let directory = fixture.root.path().join("custom-tracker");
  let captured = captured_requests(&directory);
  let result = recovery::worker(&fixture, projector, "uncomposed-managed-reporter", 10000)
    .run_once(time(120))
    .await
    .unwrap();
  assert_eq!(
    (result.retrying, result.delivered),
    (1, 0),
    "a managed input requires its selected connection, not installed legacy authority"
  );
  assert_eq!(captured_requests(&directory), captured);
  assert!(!directory.join("reporter-applied").exists());
  let after = fixture.store.factory_run_snapshot(run).await.unwrap();
  assert_eq!(after.work, before.work);
  assert_eq!(after.flow, before.flow);
}

#[tokio::test]
async fn managed_reporter_input_resolves_its_writer_and_rechecks_the_independent_current_grant() {
  for provider in ["github", "gitlab", "jira", "custom-tracker"] {
    for allow_write in [true, false] {
      let (fixture, profile) = managed_fixture(provider).await;
      let run = admit_managed(&fixture).await;
      let projector = Arc::new(
        Arc::try_unwrap(hosted(&fixture, vec![profile]))
          .ok()
          .unwrap()
          .with_managed_connections(ManagedIntegrationConnections::new(fixture.store.clone())),
      );
      accept_status_at(&fixture, projector.clone(), run, 100).await;
      let before = fixture.store.factory_run_snapshot(run).await.unwrap();
      if !allow_write {
        let connection = fixture.input.definition().connection.as_ref().unwrap();
        fixture
          .store
          .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
            intent: IntegrationProjectAccessIntent {
              connection_id: connection.id,
              connection_version: connection.version,
              project_id: fixture.input.project_id(),
              expected_current_version: Some(connection.access_version),
              resources: vec![IntegrationResourceSelection {
                reference: fixture.input.definition().source.resource.clone(),
                purposes: vec![IntegrationPurpose::TaskRead],
              }],
            },
            idempotency_key: IdempotencyKey::new("revoke-only-writer").unwrap(),
            published_at: time(115),
          }))
          .await
          .unwrap();
      }
      let result = recovery::worker(&fixture, projector, "managed-input-writer", 10000)
        .run_once(time(120))
        .await
        .unwrap();
      assert_eq!(result.delivered, u16::from(allow_write));
      let directory = fixture.root.path().join(provider);
      assert_eq!(directory.join("reporter-applied").exists(), allow_write);
      if !allow_write {
        assert!(recovery::calls(&directory).is_empty());
      }
      let after = fixture.store.factory_run_snapshot(run).await.unwrap();
      assert_eq!(after.work, before.work);
      assert_eq!(after.run, before.run);
      assert_eq!(after.flow, before.flow);
    }
  }
}
