//! Shared PostgreSQL installation of the controlled Source fixture.
use super::*;
use octacity_server_store_postgres::PostgresStore;
use sqlx::types::Json;
impl Fixture {
  pub(in super::super) async fn into_postgres(self, pool: sqlx::PgPool) -> Fixture<PostgresStore> {
    let fixture = self;
    let project = fixture.input.project_id();
    sqlx::query("INSERT INTO projects (id,parent_id,name,version,created_at,updated_at) VALUES ($1,NULL,'reporter-ordering',1,to_timestamp(0),to_timestamp(0))").bind(project.as_uuid()).execute(&pool).await.unwrap();
    let context = fixture
      .store
      .factory_admission_context(ReadFactoryAdmissionContext {
        project_id: project,
        configuration_id: fixture.input.definition().target.id(),
        configuration_version: fixture.input.definition().target.version(),
        repository_id: fixture.input.definition().repository.id,
        repository_version: fixture.input.definition().repository.version,
      })
      .await
      .unwrap();
    let repository = &context.repository;
    sqlx::query("INSERT INTO repositories (id,project_id,name,created_at) VALUES ($1,$2,$3,to_timestamp(0))")
      .bind(repository.id.as_uuid())
      .bind(project.as_uuid())
      .bind(repository.name.as_str())
      .execute(&pool)
      .await
      .unwrap();
    sqlx::query("INSERT INTO repository_versions (repository_id,version,vcs_integration_id,repository_locator,selection_policy,published_at) VALUES ($1,1,$2,$3,$4,to_timestamp(0))")
    .bind(repository.id.as_uuid()).bind(repository.definition.vcs_integration_id.as_uuid()).bind(repository.definition.repository_locator.as_str()).bind(Json(&repository.definition.selection)).execute(&pool).await.unwrap();
    let configuration = context.configuration.configuration;
    let draft = FactoryConfigurationDraft {
      flow: Some(configuration.flow().clone()),
      admission_policy: key("admission"),
      wip_limits: configuration.wip_limits(),
      hard_budget: configuration.hard_budget(),
      permission_ceiling: key("permissions"),
      enabled: true,
    };
    let store = Arc::new(PostgresStore::new(pool.clone()));
    store
      .create_factory_configuration(octacity_server_store::testing::management_mutation_with_request(
        CreateFactoryConfiguration {
          intent: FactoryConfigurationMutationIntent::Create {
            id: configuration.reference().id(),
            project_id: project,
            definition_digest: configuration.reference().definition_digest(),
            draft,
          },
          configuration,
          idempotency_key: IdempotencyKey::new("reporter-target").unwrap(),
          published_at: time(1),
        },
        "reporter-target",
      ))
      .await
      .unwrap();
    for version in 1..=fixture.input.version().get() {
      let input = fixture
        .store
        .source_input(
          project,
          fixture.input.id(),
          WorkSourceInputVersion::new(version).unwrap(),
        )
        .await
        .unwrap();
      store
        .publish_work_sources(PublishWorkSources {
          project_id: project,
          inputs: vec![WorkSourceInputChange {
            expected_version: if version == 1 {
              None
            } else {
              Some(WorkSourceInputVersion::new(version - 1).unwrap())
            },
            input,
          }],
          group: None,
        })
        .await
        .unwrap();
    }
    Fixture {
      root: fixture.root,
      store,
      bytes: fixture.bytes,
      clock: fixture.clock,
      input: fixture.input,
      profile: fixture.profile,
    }
  }
}
