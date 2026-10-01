mod support;

use octacity_server_domain::{ProjectId, TriggerId};
use octacity_server_store::testing::{
  verify_seeded_configuration_definition_discovery_contract, verify_seeded_pipeline_definition_discovery_contract,
  verify_seeded_trigger_definition_discovery_contract,
};
use octacity_server_store::{
  ListProjectTriggerDefinitions, TriggerDefinitionDiscoveryStore, TriggerDefinitionListVisibility,
};
use octacity_server_store_postgres::PostgresStore;
use serde_json::Value;
use sqlx::{Executor as _, types::Json};
use support::TestDatabase;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn postgres_definition_discovery_matches_the_in_memory_contract() {
  let database = TestDatabase::migrated().await;
  seed_definition_discovery_fixture(&database.pool).await;
  let store = PostgresStore::new(database.pool.clone());
  let project_id = id(1);

  verify_seeded_pipeline_definition_discovery_contract(&store, project_id).await;
  verify_seeded_configuration_definition_discovery_contract(&store, project_id).await;
  verify_seeded_trigger_definition_discovery_contract(&store, project_id).await;
  let triggers = store
    .list_project_trigger_definitions(
      ListProjectTriggerDefinitions::new(project_id, None, 20, TriggerDefinitionListVisibility::all()).unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(
    triggers.items.iter().map(|item| item.id).collect::<Vec<_>>(),
    [300_u128, 302, 303, 304]
      .into_iter()
      .map(trigger_id)
      .collect::<Vec<_>>(),
    "external definitions and definitions owned by another Project must stay hidden"
  );

  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn representative_definition_discovery_plans_use_the_migration_indexes() {
  let database = TestDatabase::migrated().await;
  seed_definition_discovery_fixture(&database.pool).await;
  sqlx::query("ANALYZE pipelines, repositories, build_configurations, triggers")
    .execute(&database.pool)
    .await
    .unwrap();
  let mut transaction = database.pool.begin().await.unwrap();
  transaction.execute("SET LOCAL enable_seqscan = off").await.unwrap();

  let pipeline_plan =
    explain_named_definitions(&mut transaction, "pipelines", "pipeline_versions", "pipeline_id", id(1)).await;
  assert_plan_uses(&pipeline_plan, "pipelines_project_discovery_idx");

  let repository_plan = explain_named_definitions(
    &mut transaction,
    "repositories",
    "repository_versions",
    "repository_id",
    id(1),
  )
  .await;
  assert_plan_uses(&repository_plan, "repositories_project_discovery_idx");

  let configuration_plan = explain_build_configurations(&mut transaction, id(1)).await;
  assert_plan_uses(&configuration_plan, "build_configurations_project_discovery_idx");

  let trigger_plan = explain_triggers(&mut transaction, id(2)).await;
  assert_plan_uses(&trigger_plan, "build_configurations_project_discovery_idx");
  assert_plan_uses(&trigger_plan, "triggers_pkey");

  transaction.rollback().await.unwrap();
  database.cleanup().await;
}

async fn seed_definition_discovery_fixture(pool: &sqlx::PgPool) {
  sqlx::query(
    "INSERT INTO projects (id, parent_id, name, version, created_at, updated_at) \
     VALUES ($1, NULL, 'project-one', 1, to_timestamp(0), to_timestamp(0)), \
            ($2, NULL, 'project-two', 1, to_timestamp(0), to_timestamp(0))",
  )
  .bind(uuid::Uuid::from_u128(1))
  .bind(uuid::Uuid::from_u128(2))
  .execute(pool)
  .await
  .unwrap();

  for value in [3_u128, 4, 10, 11, 12, 13, 14, 15] {
    let owner = if matches!(value, 4 | 15) { 2 } else { 1 };
    sqlx::query(
      "INSERT INTO pipelines (id, project_id, name, created_at) \
       VALUES ($1, $2, $3, to_timestamp($4::double precision / 1000.0))",
    )
    .bind(uuid::Uuid::from_u128(value))
    .bind(uuid::Uuid::from_u128(owner))
    .bind(format!("pipeline-{value}"))
    .bind(i64::try_from(value).unwrap())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
      "INSERT INTO pipeline_versions (pipeline_id, version, dag_snapshot, published_at) \
       VALUES ($1, 1, '{}', to_timestamp($2::double precision / 1000.0))",
    )
    .bind(uuid::Uuid::from_u128(value))
    .bind(i64::try_from(value).unwrap())
    .execute(pool)
    .await
    .unwrap();
  }
  sqlx::query(
    "INSERT INTO pipeline_versions (pipeline_id, version, dag_snapshot, published_at) \
     VALUES ($1, 2, '{}', to_timestamp(20::double precision / 1000.0))",
  )
  .bind(uuid::Uuid::from_u128(10))
  .execute(pool)
  .await
  .unwrap();

  for value in 100_u128..=105 {
    let owner = if value == 105 { 2 } else { 1 };
    sqlx::query(
      "INSERT INTO repositories (id, project_id, name, created_at) \
       VALUES ($1, $2, $3, to_timestamp($4::double precision / 1000.0))",
    )
    .bind(uuid::Uuid::from_u128(value))
    .bind(uuid::Uuid::from_u128(owner))
    .bind(format!("repository-{value}"))
    .bind(i64::try_from(value).unwrap())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
      "INSERT INTO repository_versions \
         (repository_id, version, vcs_integration_id, repository_locator, selection_policy, published_at) \
       VALUES ($1, 1, $2, $3, '{}', to_timestamp($4::double precision / 1000.0))",
    )
    .bind(uuid::Uuid::from_u128(value))
    .bind(uuid::Uuid::from_u128(value + 1_000))
    .bind(format!("octacity/repository-{value}"))
    .bind(i64::try_from(value).unwrap())
    .execute(pool)
    .await
    .unwrap();
  }
  sqlx::query(
    "INSERT INTO repository_versions \
       (repository_id, version, vcs_integration_id, repository_locator, selection_policy, published_at) \
     VALUES ($1, 2, $2, 'octacity/repository-current', '{}', \
             to_timestamp(200::double precision / 1000.0))",
  )
  .bind(uuid::Uuid::from_u128(100))
  .bind(uuid::Uuid::from_u128(1_100))
  .execute(pool)
  .await
  .unwrap();

  for value in 200_u128..=205 {
    let other_project = value == 205;
    let owner = if other_project { 2 } else { 1 };
    let repository = if other_project { 105 } else { 100 };
    let repository_version = if other_project { 1 } else { 2 };
    let pipeline = if other_project { 4 } else { 3 };
    sqlx::query(
      "INSERT INTO build_configurations (id, project_id, name, created_at) \
       VALUES ($1, $2, $3, to_timestamp($4::double precision / 1000.0))",
    )
    .bind(uuid::Uuid::from_u128(value))
    .bind(uuid::Uuid::from_u128(owner))
    .bind(format!("configuration-{value}"))
    .bind(i64::try_from(value).unwrap())
    .execute(pool)
    .await
    .unwrap();
    insert_configuration_version(
      pool,
      ConfigurationVersionFixture {
        id: value,
        version: 1,
        enabled: true,
        repository_id: repository,
        repository_version,
        pipeline_id: pipeline,
        published_at_millis: value,
      },
    )
    .await;
  }
  insert_configuration_version(
    pool,
    ConfigurationVersionFixture {
      id: 200,
      version: 2,
      enabled: false,
      repository_id: 100,
      repository_version: 2,
      pipeline_id: 3,
      published_at_millis: 300,
    },
  )
  .await;

  for value in 300_u128..=305 {
    let configuration = if value == 305 { 205 } else { 200 };
    let kind = match value {
      302 => "scheduled",
      304 => "internal",
      _ => "manual",
    };
    insert_trigger_version(pool, value, 1, configuration, kind, true, value + 1).await;
  }
  insert_trigger_version(pool, 300, 2, 200, "internal", false, 302).await;
  insert_trigger_version(pool, 301, 2, 205, "manual", true, 303).await;
  insert_trigger_version(pool, 306, 1, 200, "external", true, 307).await;
}

struct ConfigurationVersionFixture {
  id: u128,
  version: i64,
  enabled: bool,
  repository_id: u128,
  repository_version: i64,
  pipeline_id: u128,
  published_at_millis: u128,
}

async fn insert_configuration_version(pool: &sqlx::PgPool, fixture: ConfigurationVersionFixture) {
  sqlx::query(
    "INSERT INTO build_configuration_versions \
       (build_configuration_id, version, enabled, repository_id, repository_version, pipeline_id, pipeline_version, \
        configuration_snapshot, job_concurrency_limit, allowed_pool_ids, retry_max_attempts, \
        retries_infrastructure, published_at) \
     VALUES ($1, $2, $3, $4, $5, $6, 1, '{}', 1, ARRAY[$7::uuid], 1, false, \
             to_timestamp($8::double precision / 1000.0))",
  )
  .bind(uuid::Uuid::from_u128(fixture.id))
  .bind(fixture.version)
  .bind(fixture.enabled)
  .bind(uuid::Uuid::from_u128(fixture.repository_id))
  .bind(fixture.repository_version)
  .bind(uuid::Uuid::from_u128(fixture.pipeline_id))
  .bind(uuid::Uuid::from_u128(5))
  .bind(i64::try_from(fixture.published_at_millis).unwrap())
  .execute(pool)
  .await
  .unwrap();
}

async fn insert_trigger_version(
  pool: &sqlx::PgPool,
  id: u128,
  version: i64,
  configuration_id: u128,
  kind: &str,
  enabled: bool,
  published_at_millis: u128,
) {
  sqlx::query(
    "INSERT INTO triggers \
       (id, version, build_configuration_id, build_configuration_version, kind, enabled, definition, created_at) \
     VALUES ($1, $2, $3, 1, $4, $5, '{}', to_timestamp($6::double precision / 1000.0))",
  )
  .bind(uuid::Uuid::from_u128(id))
  .bind(version)
  .bind(uuid::Uuid::from_u128(configuration_id))
  .bind(kind)
  .bind(enabled)
  .bind(i64::try_from(published_at_millis).unwrap())
  .execute(pool)
  .await
  .unwrap();
}

async fn explain_named_definitions(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  identities: &str,
  versions: &str,
  foreign_key: &str,
  project_id: ProjectId,
) -> Value {
  let statement = sqlx::AssertSqlSafe(format!(
    "EXPLAIN (FORMAT JSON, COSTS OFF) \
     SELECT identity.id FROM {identities} AS identity \
     JOIN LATERAL (SELECT version FROM {versions} \
       WHERE {foreign_key} = identity.id ORDER BY version DESC LIMIT 1) AS current ON TRUE \
     WHERE identity.project_id = $1 \
       AND ($2 OR identity.id = ANY($3::uuid[])) \
       AND ($4::uuid IS NULL OR identity.id > $4) \
     ORDER BY identity.id LIMIT $5"
  ));
  sqlx::query_scalar::<_, Json<Value>>(statement)
    .bind(project_id.as_uuid())
    .bind(true)
    .bind(Vec::<uuid::Uuid>::new())
    .bind(Option::<uuid::Uuid>::None)
    .bind(3_i64)
    .fetch_one(&mut **transaction)
    .await
    .unwrap()
    .0
}

async fn explain_build_configurations(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  project_id: ProjectId,
) -> Value {
  sqlx::query_scalar::<_, Json<Value>>(
    "EXPLAIN (FORMAT JSON, COSTS OFF) \
     SELECT configuration.id FROM build_configurations AS configuration \
     JOIN LATERAL (SELECT version FROM build_configuration_versions \
       WHERE build_configuration_id = configuration.id ORDER BY version DESC LIMIT 1) AS current ON TRUE \
     WHERE configuration.project_id = $1 \
       AND ($2 OR configuration.id = ANY($3::uuid[])) \
       AND ($4::uuid IS NULL OR configuration.id > $4) \
     ORDER BY configuration.id LIMIT $5",
  )
  .bind(project_id.as_uuid())
  .bind(true)
  .bind(Vec::<uuid::Uuid>::new())
  .bind(Option::<uuid::Uuid>::None)
  .bind(3_i64)
  .fetch_one(&mut **transaction)
  .await
  .unwrap()
  .0
}

async fn explain_triggers(transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>, project_id: ProjectId) -> Value {
  sqlx::query_scalar::<_, Json<Value>>(
    "EXPLAIN (FORMAT JSON, COSTS OFF) \
     SELECT trigger.id FROM triggers AS trigger \
     JOIN build_configurations AS configuration ON configuration.id = trigger.build_configuration_id \
     WHERE configuration.project_id = $1 \
       AND trigger.kind IN ('manual', 'scheduled', 'internal') \
       AND ($2 OR trigger.id = ANY($3::uuid[])) \
       AND ($4::uuid IS NULL OR trigger.id > $4) \
       AND NOT EXISTS (SELECT 1 FROM triggers AS newer \
         WHERE newer.id = trigger.id AND newer.version > trigger.version) \
     ORDER BY trigger.id LIMIT $5",
  )
  .bind(project_id.as_uuid())
  .bind(true)
  .bind(Vec::<uuid::Uuid>::new())
  .bind(Option::<uuid::Uuid>::None)
  .bind(3_i64)
  .fetch_one(&mut **transaction)
  .await
  .unwrap()
  .0
}

fn assert_plan_uses(plan: &Value, index: &str) {
  assert!(
    plan.to_string().contains(index),
    "expected query plan to use {index}: {plan}"
  );
}

fn id(value: u128) -> ProjectId {
  ProjectId::from_uuid(uuid::Uuid::from_u128(value)).unwrap()
}

fn trigger_id(value: u128) -> TriggerId {
  TriggerId::from_uuid(uuid::Uuid::from_u128(value)).unwrap()
}
