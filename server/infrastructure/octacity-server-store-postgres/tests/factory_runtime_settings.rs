mod support;

use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::*;
use octacity_server_store::{
  FactoryRuntimeSettingsIntent, FactoryRuntimeSettingsStore, PublishFactoryRuntimeSettings,
  testing::{role_profile_contract_definition, verify_factory_runtime_settings_publication},
};
use octacity_server_store_postgres::PostgresStore;
use support::{TestDatabase, evidence::PostgresEvidenceProbe};

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn publication_matches_the_shared_memory_contract() {
  let database = TestDatabase::migrated().await;
  let intent = fixture(seed_project(&database.pool).await);
  verify_factory_runtime_settings_publication(
    &PostgresStore::new(database.pool.clone()),
    &PostgresEvidenceProbe(database.pool.clone()),
    intent,
  )
  .await;
  database.cleanup().await;
}

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn fixture(project: ProjectId) -> FactoryRuntimeSettingsIntent {
  FactoryRuntimeSettingsIntent {
    project_id: project,
    runtime: key("qualified-runtime"),
    expected_version: None,
    definition: FactoryRuntimeSettingsDefinition {
      toolchain: role_profile_contract_definition(project).toolchain,
      mcp_servers: [(
        key("reader"),
        FactoryRuntimeMcpEntry {
          server: FactoryMcpServer::Http {
            url: "https://mcp.example/tools".into(),
            bearer_token_env_var: None,
            enabled_tools: Some(vec!["read".into()]),
          },
          integration: None,
        },
      )]
      .into(),
      integrations: Default::default(),
      permissions: FactoryPermissionSet::try_new(FactoryPermissionDraft {
        network_hosts: vec![NetworkHost::new("mcp.example").unwrap()],
        ..Default::default()
      })
      .unwrap(),
    },
  }
}
async fn seed_project(pool: &sqlx::PgPool) -> ProjectId {
  let project = ProjectId::generate();
  sqlx::query(
    "INSERT INTO projects (id,name,version,created_at,updated_at) VALUES ($1,$2,1,to_timestamp(0),to_timestamp(0))",
  )
  .bind(project.as_uuid())
  .bind(format!("runtime owner {project}"))
  .execute(pool)
  .await
  .unwrap();
  project
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn interrupted_publication_and_competing_replacements_recover_exact_history() {
  use octacity_server_store::{
    FactoryRuntimeSettingsPublicationError, IdempotencyKey, MutationDisposition, ReplayFactoryRuntimeSettings,
    testing::{MutationEvidenceProbe, management_mutation_with_request},
  };
  let database = TestDatabase::migrated().await;
  let intent = fixture(seed_project(&database.pool).await);
  let original = PublishFactoryRuntimeSettings {
    intent: intent.clone(),
    idempotency_key: IdempotencyKey::new("original-runtime").unwrap(),
    published_at: Timestamp::from_unix_millis(1).unwrap(),
  };
  let store = PostgresStore::new(database.pool.clone());
  let evidence = PostgresEvidenceProbe(database.pool.clone());
  let before = evidence.mutation_evidence_counts().await;
  // One failure at the shared outbox boundary must roll back settings, audit and replay together.
  sqlx::raw_sql("CREATE FUNCTION reject_runtime_event() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'test outbox interruption'; END $$; CREATE TRIGGER reject_runtime_event BEFORE INSERT ON outbox_entries FOR EACH ROW WHEN (NEW.topic = 'factory.runtime-settings-published') EXECUTE FUNCTION reject_runtime_event()")
    .execute(&database.pool).await.unwrap();
  assert!(
    store
      .publish_factory_runtime_settings(management_mutation_with_request(original.clone(), "interrupted"))
      .await
      .is_err()
  );
  assert!(
    store
      .factory_runtime_settings(intent.project_id, &intent.runtime, None)
      .await
      .unwrap()
      .is_none()
  );
  assert!(
    store
      .replay_factory_runtime_settings(management_mutation_with_request(
        ReplayFactoryRuntimeSettings {
          intent: intent.clone(),
          idempotency_key: original.idempotency_key.clone()
        },
        "uncommitted-replay"
      ))
      .await
      .unwrap()
      .is_none()
  );
  assert_eq!(evidence.mutation_evidence_counts().await, before);
  sqlx::raw_sql("DROP TRIGGER reject_runtime_event ON outbox_entries; DROP FUNCTION reject_runtime_event()")
    .execute(&database.pool)
    .await
    .unwrap();
  let first = store
    .publish_factory_runtime_settings(management_mutation_with_request(original.clone(), "initial"))
    .await
    .unwrap();
  let first_pin = first
    .settings
    .mcp_configuration(&key("reader"))
    .unwrap()
    .unwrap()
    .reference()
    .clone();
  let replacement = |label: &str| {
    let mut request = original.clone();
    request.intent.expected_version = Some(first.settings.version());
    if let FactoryMcpServer::Http { url, .. } = &mut request
      .intent
      .definition
      .mcp_servers
      .get_mut(&key("reader"))
      .unwrap()
      .server
    {
      *url = format!("https://mcp.example/{label}");
    }
    request.idempotency_key = IdempotencyKey::new(label).unwrap();
    request
  };
  let left = replacement("left");
  let right = replacement("right");
  let left_pool = independent_pool(&database.pool).await;
  let right_pool = independent_pool(&database.pool).await;
  let left_store = PostgresStore::new(left_pool.clone());
  let right_store = PostgresStore::new(right_pool.clone());
  let before_race = evidence.mutation_evidence_counts().await;
  let (l, r) = tokio::join!(
    left_store.publish_factory_runtime_settings(management_mutation_with_request(left.clone(), "left")),
    right_store.publish_factory_runtime_settings(management_mutation_with_request(right.clone(), "right")),
  );
  let (winner, accepted, loser) = match (l, r) {
    (Ok(outcome), Err(FactoryRuntimeSettingsPublicationError::PreconditionFailed)) => (left, outcome, right),
    (Err(FactoryRuntimeSettingsPublicationError::PreconditionFailed), Ok(outcome)) => (right, outcome, left),
    other => panic!("one reviewed replacement must win: {other:?}"),
  };
  assert_eq!(accepted.settings.version().get(), 2);
  let after_race = evidence.mutation_evidence_counts().await;
  assert_eq!(
    (
      after_race.idempotency - before_race.idempotency,
      after_race.audit - before_race.audit,
      after_race.outbox - before_race.outbox
    ),
    (1, 1, 1)
  );
  // Advance the current head so replay cannot accidentally return today's settings.
  let mut latest = replacement("latest");
  latest.intent.expected_version = Some(accepted.settings.version());
  let third = store
    .publish_factory_runtime_settings(management_mutation_with_request(latest, "later-update"))
    .await
    .unwrap();
  assert_eq!(third.settings.version().get(), 3);
  left_pool.close().await;
  right_pool.close().await;
  let restarted_pool = independent_pool(&database.pool).await;
  let restarted = PostgresStore::new(restarted_pool.clone());
  let before_recovery = evidence.mutation_evidence_counts().await;
  let mut retry = winner;
  retry.published_at = Timestamp::from_unix_millis(999).unwrap();
  let replay = restarted
    .publish_factory_runtime_settings(management_mutation_with_request(retry, "lost-response-after-restart"))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.settings, accepted.settings);
  assert_eq!(replay.published_at, accepted.published_at);
  assert!(
    restarted
      .replay_factory_runtime_settings(management_mutation_with_request(
        ReplayFactoryRuntimeSettings {
          intent: loser.intent,
          idempotency_key: loser.idempotency_key
        },
        "loser-replay"
      ))
      .await
      .unwrap()
      .is_none()
  );
  let retained = restarted
    .factory_runtime_settings(intent.project_id, &intent.runtime, Some(first.settings.version()))
    .await
    .unwrap()
    .unwrap();
  assert_eq!(retained, first.settings);
  assert_eq!(
    retained.mcp_configuration(&key("reader")).unwrap().unwrap().reference(),
    &first_pin
  );
  assert_eq!(
    restarted
      .factory_runtime_settings(intent.project_id, &intent.runtime, None)
      .await
      .unwrap(),
    Some(third.settings)
  );
  assert_eq!(evidence.mutation_evidence_counts().await, before_recovery);
  restarted_pool.close().await;
  database.cleanup().await;
}
async fn independent_pool(pool: &sqlx::PgPool) -> sqlx::PgPool {
  sqlx::postgres::PgPoolOptions::new()
    .max_connections(1)
    .connect_with((*pool.connect_options()).clone())
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn configured_catalog_keeps_installation_separate_and_serializes_aggregate_capacity() {
  use octacity_server_store::{
    FactoryConfigurationAvailability, FactoryConfigurationStore, FactoryRuntimeSettingsPublicationError, StoreError,
    testing::{MutationEvidenceProbe, management_mutation_with_request},
  };
  use octacity_server_store_postgres::FactoryCapabilitySource;
  use std::sync::Arc;
  struct Installed(FactoryConfigurationChoices);
  #[async_trait::async_trait]
  impl FactoryCapabilitySource for Installed {
    async fn availability(&self, _: ProjectId) -> Result<FactoryConfigurationAvailability, StoreError> {
      Ok(FactoryConfigurationAvailability::Available(self.0.clone()))
    }
  }
  let database = TestDatabase::migrated().await;
  let project = seed_project(&database.pool).await;
  let mut left = fixture(project);
  left.runtime = key("left-runtime");
  let mut right = left.clone();
  right.runtime = key("right-runtime");
  let mut definition = role_profile_contract_definition(project);
  definition.execution = Default::default();
  definition.skills.clear();
  definition.permissions = left.definition.permissions.clone();
  let mut entries = FactoryConfigurationChoiceEntries {
    references: vec![
      FactoryReferenceChoice {
        kind: FactoryChoiceKind::NodeCapability,
        alias: left.runtime.clone(),
        reference: definition.toolchain.executable.clone(),
      },
      FactoryReferenceChoice {
        kind: FactoryChoiceKind::NodeCapability,
        alias: right.runtime.clone(),
        reference: definition.toolchain.executable.clone(),
      },
      FactoryReferenceChoice {
        kind: FactoryChoiceKind::NodeCapability,
        alias: key("plugin"),
        reference: definition.toolchain.plugin.clone(),
      },
      FactoryReferenceChoice {
        kind: FactoryChoiceKind::NodeCapability,
        alias: key("model"),
        reference: definition.toolchain.model.clone(),
      },
    ],
    build_configurations: vec![(key("build"), definition.build_configuration.clone())],
  };
  while entries.references.len() < MAX_CONFIGURATION_CHOICES_PER_KIND - 1 {
    let name = key(&format!("installed-{}", entries.references.len()));
    entries.references.push(FactoryReferenceChoice {
      kind: FactoryChoiceKind::NodeCapability,
      alias: name.clone(),
      reference: ImmutableReference::new(name, key("v1"), FactoryDigest::from_bytes([44; 32])),
    });
  }
  let cap = FactoryRoleProfileCapability {
    mcp_required_environment: Default::default(),
    model_catalog: None,
    public_environment: Default::default(),
    parameters: FlowDataSchema::new(
      key("parameters"),
      key("v1"),
      FlowValueSchema::Object {
        fields: Default::default(),
      },
    )
    .unwrap(),
    local_permissions: LocalPermissionCeiling::fully_enforced(definition.permissions.clone()),
    ceiling: definition,
  };
  let mut choices = FactoryConfigurationChoices::try_new(entries).unwrap();
  for alias in [&left.runtime, &right.runtime] {
    choices = choices
      .with_role_profile_capability(alias.clone(), cap.clone())
      .unwrap()
      .with_runtime_settings_support(
        alias.clone(),
        FactoryRuntimeSettingsSupport {
          mcp_transports: [FactoryMcpTransport::Http].into(),
          mcp_executables: Default::default(),
        },
      )
      .unwrap();
  }
  let source = Arc::new(Installed(choices.clone()));
  let first_pool = independent_pool(&database.pool).await;
  let second_pool = independent_pool(&database.pool).await;
  let first = PostgresStore::with_factory_capabilities(first_pool.clone(), source.clone());
  let second = PostgresStore::with_factory_capabilities(second_pool.clone(), source.clone());
  let publication = |intent, name: &str| {
    management_mutation_with_request(
      PublishFactoryRuntimeSettings {
        intent,
        idempotency_key: name.parse().unwrap(),
        published_at: Timestamp::from_unix_millis(1).unwrap(),
      },
      name,
    )
  };
  let evidence = PostgresEvidenceProbe(database.pool.clone());
  let before = evidence.mutation_evidence_counts().await;
  let (a, b) = tokio::join!(
    first.publish_factory_runtime_settings(publication(left, "left-capacity")),
    second.publish_factory_runtime_settings(publication(right, "right-capacity"))
  );
  let winner = match (a, b) {
    (Ok(value), Err(FactoryRuntimeSettingsPublicationError::Store(StoreError::InvalidInput { .. })))
    | (Err(FactoryRuntimeSettingsPublicationError::Store(StoreError::InvalidInput { .. })), Ok(value)) => value,
    other => panic!("one remaining catalog slot permits one publication: {other:?}"),
  };
  first_pool.close().await;
  second_pool.close().await;
  let reopened = PostgresStore::with_factory_capabilities(database.pool.clone(), source);
  assert_eq!(
    reopened.factory_configuration_installation(project).await.unwrap(),
    FactoryConfigurationAvailability::Available(choices)
  );
  let FactoryConfigurationAvailability::Available(configured) =
    reopened.factory_configuration_availability(project).await.unwrap()
  else {
    panic!("configured catalog");
  };
  assert_eq!(
    configured.entries().references.len(),
    MAX_CONFIGURATION_CHOICES_PER_KIND
  );
  assert_eq!(
    configured.role_profile_capabilities()[winner.settings.runtime()]
      .ceiling
      .execution
      .mcp_servers,
    winner.settings.execution().unwrap().mcp_servers
  );
  let foreign = seed_project(&database.pool).await;
  let pin = winner.settings.mcp_configuration(&key("reader")).unwrap().unwrap();
  assert!(
    reopened
      .factory_runtime_mcp_configuration(foreign, pin.reference())
      .await
      .unwrap()
      .is_none()
  );
  let after = evidence.mutation_evidence_counts().await;
  assert_eq!(
    (
      after.audit - before.audit,
      after.idempotency - before.idempotency,
      after.outbox - before.outbox
    ),
    (1, 1, 1)
  );
  database.cleanup().await;
}
