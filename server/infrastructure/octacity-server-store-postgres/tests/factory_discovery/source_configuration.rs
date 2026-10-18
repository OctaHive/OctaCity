//! The same management contract runs against memory and a single-connection PostgreSQL pool.
use super::*;
use octacity_server_domain::IntegrationVersion;
use octacity_server_factory::WorkSourceConnectionRef;
use octacity_server_store::{testing::*, *};

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn managed_source_configuration_publication_is_atomic_replayable_and_revocation_safe_in_both_stores() {
  let database = TestDatabase::migrated().await;
  let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(1)
    .connect_with((*database.pool.connect_options()).clone())
    .await
    .unwrap();
  let postgres = PostgresStore::new(pool.clone());
  let memory = InMemoryFactoryConfigurationStore::new();
  let project = ProjectId::generate();
  let foreign = ProjectId::generate();
  seed_projects(&database.pool, project, foreign).await;
  memory.seed_project(project).unwrap();
  let fixture = fixture(project);
  let configuration = FactoryConfiguration::publish(
    FactoryConfigurationId::generate(),
    FactoryConfigurationVersion::INITIAL,
    project,
    digest(3),
    fixture.draft.clone(),
    &fixture.choices,
  )
  .unwrap();
  let repository = repository(project, RepositoryId::generate());
  seed_repository(&database.pool, &repository).await;
  memory.seed_repository_version(repository.clone()).unwrap();
  memory
    .seed_factory_configuration_version(PublishedFactoryConfiguration {
      configuration: configuration.clone(),
      published_at: time(1),
    })
    .unwrap();
  postgres
    .create_factory_configuration(management_mutation(CreateFactoryConfiguration {
      configuration: configuration.clone(),
      idempotency_key: idempotency("managed-source-target"),
      published_at: time(1),
      intent: FactoryConfigurationMutationIntent::Create {
        id: configuration.reference().id(),
        project_id: project,
        definition_digest: digest(3),
        draft: fixture.draft,
      },
    }))
    .await
    .unwrap();
  for provider in ["github", "gitlab", "jira", "custom-tracker"] {
    managed_contract(&memory, &configuration, &repository, provider).await;
    managed_contract(&PostgresStore::new(pool.clone()), &configuration, &repository, provider).await;
  }
  pool.close().await;
  database.cleanup().await;
}

async fn managed_contract<
  S: FactorySourceConfigurationStore
    + IntegrationConnectionStore
    + IntegrationAdministrationStore
    + IntegrationProjectAccessStore,
>(
  store: &S,
  configuration: &FactoryConfiguration,
  repository: &PublishedRepository,
  provider: &str,
) {
  let mut original = input(configuration, repository, provider);
  let binding = &original.definition().source;
  let connection = IntegrationId::generate();
  let (mut installed, mut draft) = integration_connection_fixture(provider);
  installed.installation = binding.installation.clone();
  installed.resources[0].reference = binding.resource.clone();
  installed.resources[0].capabilities[0].capability = binding.capability.clone();
  installed.resources[0].capabilities[0].configuration = binding.configuration.clone();
  installed.resources[0].capabilities[0].credential =
    serde_json::from_value(serde_json::json!({"provider":"installed", "reference":format!("reader-{connection}")}))
      .unwrap();
  let credential = installed.resources[0].capabilities[0].credential.clone();
  draft.installation = installed.installation.clone();
  draft.resources[0].reference = binding.resource.clone();
  let resources = draft.resources.clone();
  store
    .publish_integration_connection(management_mutation(PublishIntegrationConnection {
      intent: IntegrationConnectionIntent {
        id: connection,
        expected_current_version: None,
        draft,
      },
      installed,
      idempotency_key: idempotency(&format!("connection-{connection}")),
      published_at: time(1),
    }))
    .await
    .unwrap();
  let receipt = store
    .begin_integration_administration(management_mutation(BeginIntegrationAdministration {
      intent: IntegrationAdministrationIntent {
        id: connection,
        expected_metadata_version: IntegrationVersion::INITIAL,
        expected_revision: 0,
        action: IntegrationAdministrationAction::Credential {
          reference: credential,
          kind: serde_json::from_value(serde_json::json!("adopt")).unwrap(),
          activate_connection: true,
        },
      },
      idempotency_key: idempotency(&format!("activate-{connection}")),
      requested_at: time(2),
    }))
    .await
    .unwrap();
  store
    .complete_integration_administration(management_mutation(receipt))
    .await
    .unwrap();
  store
    .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
      intent: IntegrationProjectAccessIntent {
        connection_id: connection,
        connection_version: IntegrationVersion::INITIAL,
        project_id: repository.project_id,
        expected_current_version: None,
        resources,
      },
      idempotency_key: idempotency(&format!("grant-{connection}")),
      published_at: time(3),
    }))
    .await
    .unwrap();
  let mut value = serde_json::to_value(&original).unwrap();
  value["definition"]["connection"] = serde_json::to_value(WorkSourceConnectionRef {
    id: connection,
    version: IntegrationVersion::INITIAL,
    access_version: IntegrationVersion::INITIAL,
    source_configuration: original.definition().source.configuration.clone(),
    reporter_configuration: None,
  })
  .unwrap();
  original = serde_json::from_value(value).unwrap();
  let group = WorkSourceGroup::publish(
    WorkSourceGroupId::generate(),
    repository.project_id,
    vec![original.clone()],
  )
  .unwrap();
  let packet = PublishWorkSourceConfiguration {
    intent: WorkSourceConfigurationIntent {
      project_id: repository.project_id,
      inputs: vec![WorkSourceInputIntent {
        id: original.id(),
        expected_version: None,
        definition: original.definition().clone(),
      }],
      group: Some(WorkSourceGroupIntent {
        id: group.id(),
        expected_version: None,
        members: vec![WorkSourceMemberRef {
          id: original.id(),
          version: original.version(),
        }],
      }),
    },
    publication: PublishWorkSources {
      project_id: repository.project_id,
      inputs: vec![WorkSourceInputChange {
        expected_version: None,
        input: original.clone(),
      }],
      group: Some(WorkSourceGroupChange {
        expected_version: None,
        group: group.clone(),
      }),
    },
    idempotency_key: idempotency(&format!("publish-{connection}")),
    published_at: time(4),
  };
  let (left, right) = tokio::join!(
    store.publish_work_source_configuration(management_mutation(packet.clone())),
    store.publish_work_source_configuration(management_mutation(packet.clone()))
  );
  let (left, right) = (left.unwrap(), right.unwrap());
  assert_eq!(left.publication, right.publication);
  assert_eq!(
    [left.disposition, right.disposition]
      .into_iter()
      .filter(|d| *d == octacity_server_store::MutationDisposition::Applied)
      .count(),
    1
  );
  assert_eq!(
    store
      .source_input(repository.project_id, original.id(), original.version())
      .await
      .unwrap(),
    original
  );
  let request = ListWorkSourceConfigurations {
    project_id: repository.project_id,
    after: None,
    limit: 1,
    visibility: WorkSourceConfigurationVisibility {
      configurations: FactoryConfigurationListVisibility::all(),
      repositories: RepositoryListVisibility::all(),
      connections: IntegrationConnectionListVisibility::restricted([connection]).unwrap(),
    },
  };
  let page = store.list_work_source_configurations(request.clone()).await.unwrap();
  assert_eq!(page.items.len(), 1);
  assert_eq!(
    page.items[0].target(),
    WorkSourceScanTarget::Group {
      id: group.id(),
      version: group.version()
    }
  );
  let mut next_page = request.clone();
  next_page.after = page.next_cursor;
  assert!(next_page.after.is_some());
  let page = store.list_work_source_configurations(next_page).await.unwrap();
  assert_eq!(page.items.len(), 1);
  assert_eq!(
    page.items[0].target(),
    WorkSourceScanTarget::Input {
      id: original.id(),
      version: original.version()
    }
  );
  assert_eq!(page.next_cursor, None);
  for hidden in ["configuration", "repository", "connection"] {
    let mut filtered = request.clone();
    match hidden {
      "configuration" => filtered.visibility.configurations = FactoryConfigurationListVisibility::none(),
      "repository" => filtered.visibility.repositories = RepositoryListVisibility::none(),
      "connection" => filtered.visibility.connections = IntegrationConnectionListVisibility::none(),
      _ => unreachable!(),
    }
    let page = store.list_work_source_configurations(filtered).await.unwrap();
    assert!(page.items.is_empty());
    assert_eq!(page.next_cursor, None);
  }
  let mut next = serde_json::to_value(&original).unwrap();
  next["version"] = serde_json::json!(2);
  next["definition"]["enabled"] = serde_json::json!(false);
  let next: WorkSourceInput = serde_json::from_value(next).unwrap();
  let mut replacement = packet.clone();
  replacement.intent.group = None;
  replacement.publication.group = None;
  replacement.intent.inputs[0].expected_version = Some(original.version());
  replacement.intent.inputs[0].definition = next.definition().clone();
  replacement.publication.inputs[0] = WorkSourceInputChange {
    expected_version: Some(original.version()),
    input: next,
  };
  replacement.idempotency_key = idempotency(&format!("replace-{connection}"));
  // This was valid when constructed. Revoke before its commit, not before validation.
  store
    .publish_integration_project_access(management_mutation(PublishIntegrationProjectAccess {
      intent: IntegrationProjectAccessIntent {
        connection_id: connection,
        connection_version: IntegrationVersion::INITIAL,
        project_id: repository.project_id,
        expected_current_version: Some(IntegrationVersion::INITIAL),
        resources: vec![],
      },
      idempotency_key: idempotency(&format!("revoke-{connection}")),
      published_at: time(5),
    }))
    .await
    .unwrap();
  assert!(
    store
      .publish_work_source_configuration(management_mutation(replacement))
      .await
      .is_err()
  );
  assert!(
    store
      .source_input(
        repository.project_id,
        original.id(),
        WorkSourceInputVersion::new(2).unwrap()
      )
      .await
      .is_err()
  );
  let replay = store
    .replay_work_source_configuration(management_mutation(ReplayWorkSourceConfiguration {
      intent: packet.intent.clone(),
      idempotency_key: packet.idempotency_key.clone(),
    }))
    .await
    .unwrap()
    .unwrap();
  assert_eq!(replay.published_at, time(4));
  assert_eq!(replay.publication, packet.publication);
  assert_eq!(replay.disposition, octacity_server_store::MutationDisposition::Replayed);
}
