mod support;

use std::collections::BTreeSet;

use octacity_server_domain::{AgentId, BuildId, PoolId, ProjectId};
use octacity_server_store::testing::InMemoryResourceSearchStore;
use octacity_server_store::{
  NormalizedResourceSearchQuery, ResourceSearchKind, ResourceSearchPage, ResourceSearchResource,
  ResourceSearchStore as _, ResourceSearchSummary, ResourceSearchVisibility, SearchResources,
};
use octacity_server_store_postgres::PostgresStore;
use serde_json::Value;
use sqlx::{Executor as _, types::Json};
use support::TestDatabase;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn postgres_resource_search_matches_the_in_memory_contract() {
  let database = TestDatabase::migrated().await;
  let memory = InMemoryResourceSearchStore::new();
  let postgres = PostgresStore::new(database.pool.clone());
  let fixture = seed_fixture(&database.pool, &memory).await;

  let first = assert_parity(
    &memory,
    &postgres,
    request(
      "  ALPHA\t",
      ResourceSearchKind::ALL,
      None,
      2,
      ResourceSearchVisibility::all(),
    ),
  )
  .await;
  assert_eq!(
    resources(&first),
    [
      ResourceSearchResource::Project(fixture.project_one),
      ResourceSearchResource::Project(fixture.project_two),
    ]
  );
  let second = assert_parity(
    &memory,
    &postgres,
    request(
      "alpha",
      ResourceSearchKind::ALL,
      first.next_cursor,
      2,
      ResourceSearchVisibility::all(),
    ),
  )
  .await;
  assert_eq!(
    resources(&second),
    [
      ResourceSearchResource::Build(fixture.build_one),
      ResourceSearchResource::Build(fixture.build_two),
    ]
  );

  let filtered = assert_parity(
    &memory,
    &postgres,
    request(
      "alpha",
      [ResourceSearchKind::Build, ResourceSearchKind::Agent],
      None,
      20,
      ResourceSearchVisibility::all(),
    ),
  )
  .await;
  assert!(filtered.items.iter().all(|item| matches!(
    item.resource(),
    ResourceSearchResource::Build(_) | ResourceSearchResource::Agent(_)
  )));

  let restricted = ResourceSearchVisibility::restricted([
    ResourceSearchResource::Project(fixture.project_two),
    ResourceSearchResource::Build(fixture.build_two),
    ResourceSearchResource::Build(fixture.hidden_build),
    ResourceSearchResource::Agent(fixture.agent),
    ResourceSearchResource::AgentPool(fixture.pool),
  ])
  .unwrap();
  let visible = assert_parity(
    &memory,
    &postgres,
    request("alpha", ResourceSearchKind::ALL, None, 2, restricted),
  )
  .await;
  assert_eq!(
    resources(&visible),
    [
      ResourceSearchResource::Project(fixture.project_two),
      ResourceSearchResource::Build(fixture.build_two),
    ]
  );
  let visible_tail = assert_parity(
    &memory,
    &postgres,
    request(
      "alpha",
      ResourceSearchKind::ALL,
      visible.next_cursor,
      2,
      ResourceSearchVisibility::restricted([
        ResourceSearchResource::Project(fixture.project_two),
        ResourceSearchResource::Build(fixture.build_two),
        ResourceSearchResource::Build(fixture.hidden_build),
        ResourceSearchResource::Agent(fixture.agent),
        ResourceSearchResource::AgentPool(fixture.pool),
      ])
      .unwrap(),
    ),
  )
  .await;
  assert_eq!(
    resources(&visible_tail),
    [
      ResourceSearchResource::Agent(fixture.agent),
      ResourceSearchResource::AgentPool(fixture.pool),
    ]
  );
  assert_eq!(visible_tail.next_cursor, None);

  let exact = assert_parity(
    &memory,
    &postgres,
    request(
      &fixture.build_one.to_string(),
      ResourceSearchKind::ALL,
      None,
      20,
      ResourceSearchVisibility::all(),
    ),
  )
  .await;
  assert_eq!(
    resources(&exact),
    [
      ResourceSearchResource::Build(fixture.build_one),
      ResourceSearchResource::Project(fixture.identifier_named_project),
    ]
  );

  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn representative_resource_search_plans_use_only_the_migration_indexes() {
  let database = TestDatabase::migrated().await;
  let memory = InMemoryResourceSearchStore::new();
  seed_fixture(&database.pool, &memory).await;
  sqlx::query("ANALYZE projects, build_configurations, builds, agents, pools")
    .execute(&database.pool)
    .await
    .unwrap();
  let mut transaction = database.pool.begin().await.unwrap();
  transaction.execute("SET LOCAL enable_seqscan = off").await.unwrap();

  let plan = explain_production_search(&mut transaction).await;
  assert!(!plan.to_string().contains("\"Node Type\":\"Seq Scan\""), "{plan}");
  assert_plan_uses(&plan, "projects_resource_search_name_idx");
  assert_plan_uses(&plan, "pools_current_version_idx");

  for (table, index) in [
    ("agents", "agents_resource_search_name_idx"),
    ("build_configurations", "build_configurations_resource_search_name_idx"),
    ("pools", "pools_resource_search_name_idx"),
  ] {
    assert_plan_uses(&explain_named_index(&mut transaction, table).await, index);
  }
  assert_plan_uses(
    &explain_build_name_join(&mut transaction).await,
    "builds_configuration_resource_search_idx",
  );

  transaction.rollback().await.unwrap();
  database.cleanup().await;
}

struct Fixture {
  project_one: ProjectId,
  project_two: ProjectId,
  identifier_named_project: ProjectId,
  build_one: BuildId,
  build_two: BuildId,
  hidden_build: BuildId,
  agent: AgentId,
  pool: PoolId,
}

async fn seed_fixture(pool: &sqlx::PgPool, memory: &InMemoryResourceSearchStore) -> Fixture {
  let fixture = Fixture {
    project_one: project(1),
    project_two: project(2),
    identifier_named_project: project(4),
    build_one: build(101),
    build_two: build(102),
    hidden_build: build(104),
    agent: agent(201),
    pool: agent_pool(301),
  };
  let project_contains = project(3);
  let build_contains = build(103);
  let agent_contains = agent(202);
  let pool_contains = agent_pool(302);

  sqlx::query(
    "INSERT INTO projects (id, parent_id, name, version, created_at, updated_at) \
     VALUES ($1, NULL, 'Alpha', 1, now(), now()), \
            ($2, $1, 'Alpha', 1, now(), now()), \
            ($3, NULL, 'Gamma Alpha', 1, now(), now()), \
            ($4, NULL, $5, 1, now(), now())",
  )
  .bind(fixture.project_one.as_uuid())
  .bind(fixture.project_two.as_uuid())
  .bind(project_contains.as_uuid())
  .bind(fixture.identifier_named_project.as_uuid())
  .bind(fixture.build_one.to_string())
  .execute(pool)
  .await
  .unwrap();

  seed_summary(
    memory,
    ResourceSearchResource::Project(fixture.project_one),
    "Alpha",
    None,
  );
  seed_summary(
    memory,
    ResourceSearchResource::Project(fixture.project_two),
    "Alpha",
    None,
  );
  seed_summary(
    memory,
    ResourceSearchResource::Project(project_contains),
    "Gamma Alpha",
    None,
  );
  seed_summary(
    memory,
    ResourceSearchResource::Project(fixture.identifier_named_project),
    &fixture.build_one.to_string(),
    None,
  );

  for (pool_id, version, name) in [
    (fixture.pool, 1_i64, "Alpha"),
    (fixture.pool, 2, "Alpha"),
    (pool_contains, 1, "Gamma Alpha"),
  ] {
    sqlx::query(
      "INSERT INTO pools \
         (id, version, name, enabled, drain_state, admission_policy, concurrency_limit, created_at) \
       VALUES ($1, $2, $3, true, 'accepting', '{\"mode\":\"any\"}', 1, now())",
    )
    .bind(pool_id.as_uuid())
    .bind(version)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();
  }
  seed_summary(memory, ResourceSearchResource::AgentPool(fixture.pool), "Alpha", None);
  seed_summary(
    memory,
    ResourceSearchResource::AgentPool(pool_contains),
    "Gamma Alpha",
    None,
  );

  for (agent_id, name, pool_id, pool_version, pool_name) in [
    (fixture.agent, "Alpha", fixture.pool, 2_i64, "Alpha"),
    (agent_contains, "Gamma Alpha", pool_contains, 1, "Gamma Alpha"),
  ] {
    sqlx::query(
      "INSERT INTO agents \
         (id, name, pool_id, pool_version, state, inventory, version, created_at, updated_at) \
       VALUES ($1, $2, $3, $4, 'online', '{}', 1, now(), now())",
    )
    .bind(agent_id.as_uuid())
    .bind(name)
    .bind(pool_id.as_uuid())
    .bind(pool_version)
    .execute(pool)
    .await
    .unwrap();
    seed_summary(memory, ResourceSearchResource::Agent(agent_id), name, Some(pool_name));
  }

  seed_build_prerequisites(pool, fixture.project_one).await;
  for (configuration, name) in [(10_u128, "Alpha"), (11, "Gamma Alpha")] {
    seed_configuration(pool, fixture.project_one, configuration, name).await;
  }
  for (build_id, configuration, label, visible) in [
    (fixture.build_one, 10_u128, "Alpha", true),
    (fixture.build_two, 10, "Alpha", true),
    (build_contains, 11, "Gamma Alpha", true),
    (fixture.hidden_build, 10, "Alpha", false),
  ] {
    seed_build(pool, fixture.project_one, build_id, configuration, visible).await;
    if visible {
      seed_summary(memory, ResourceSearchResource::Build(build_id), label, Some("Alpha"));
    }
  }

  fixture
}

async fn seed_build_prerequisites(pool: &sqlx::PgPool, project_id: ProjectId) {
  sqlx::query("INSERT INTO repositories (id, project_id, name, created_at) VALUES ($1, $2, 'source', now())")
    .bind(uuid::Uuid::from_u128(20))
    .bind(project_id.as_uuid())
    .execute(pool)
    .await
    .unwrap();
  sqlx::query(
    "INSERT INTO repository_versions \
       (repository_id, version, vcs_integration_id, repository_locator, selection_policy, published_at) \
     VALUES ($1, 1, $2, 'octacity/source', '{}', now())",
  )
  .bind(uuid::Uuid::from_u128(20))
  .bind(uuid::Uuid::from_u128(21))
  .execute(pool)
  .await
  .unwrap();
  sqlx::query("INSERT INTO pipelines (id, project_id, name, created_at) VALUES ($1, $2, 'pipeline', now())")
    .bind(uuid::Uuid::from_u128(30))
    .bind(project_id.as_uuid())
    .execute(pool)
    .await
    .unwrap();
  sqlx::query(
    "INSERT INTO pipeline_versions (pipeline_id, version, dag_snapshot, published_at) VALUES ($1, 1, '{}', now())",
  )
  .bind(uuid::Uuid::from_u128(30))
  .execute(pool)
  .await
  .unwrap();
}

async fn seed_configuration(pool: &sqlx::PgPool, project_id: ProjectId, value: u128, name: &str) {
  sqlx::query("INSERT INTO build_configurations (id, project_id, name, created_at) VALUES ($1, $2, $3, now())")
    .bind(uuid::Uuid::from_u128(value))
    .bind(project_id.as_uuid())
    .bind(name)
    .execute(pool)
    .await
    .unwrap();
  sqlx::query(
    "INSERT INTO build_configuration_versions \
       (build_configuration_id, version, enabled, repository_id, repository_version, pipeline_id, \
        pipeline_version, configuration_snapshot, job_concurrency_limit, allowed_pool_ids, retry_max_attempts, \
        retries_infrastructure, published_at) \
     VALUES ($1, 1, true, $2, 1, $3, 1, '{}', 1, ARRAY[$4::uuid], 1, false, now())",
  )
  .bind(uuid::Uuid::from_u128(value))
  .bind(uuid::Uuid::from_u128(20))
  .bind(uuid::Uuid::from_u128(30))
  .bind(uuid::Uuid::from_u128(301))
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO triggers \
       (id, version, build_configuration_id, build_configuration_version, kind, enabled, definition, created_at) \
     VALUES ($1, 1, $2, 1, 'manual', true, '{}', now())",
  )
  .bind(uuid::Uuid::from_u128(value + 1_000))
  .bind(uuid::Uuid::from_u128(value))
  .execute(pool)
  .await
  .unwrap();
}

async fn seed_build(pool: &sqlx::PgPool, project_id: ProjectId, build_id: BuildId, configuration: u128, visible: bool) {
  let occurrence = uuid::Uuid::from_u128(build_id.as_uuid().as_u128() + 10_000);
  sqlx::query(
    "INSERT INTO trigger_occurrences \
       (id, trigger_id, trigger_version, build_configuration_id, build_configuration_version, kind, \
        deduplication_identity, cause, causality, provider_metadata, source_time, state, request_digest, \
        created_at, updated_at) \
     VALUES ($1, $2, 1, $3, 1, 'manual', $4, '{\"kind\":\"manual\"}', \
             jsonb_build_object('root_occurrence_id', $1::text, 'parent_occurrence_id', NULL, 'depth', 0), '{}', \
             now(), 'accepted', decode(repeat('02', 32), 'hex'), now(), now())",
  )
  .bind(occurrence)
  .bind(uuid::Uuid::from_u128(configuration + 1_000))
  .bind(uuid::Uuid::from_u128(configuration))
  .bind(format!("search-{build_id}"))
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO builds \
       (id, project_id, build_configuration_id, build_configuration_version, pipeline_id, pipeline_version, \
        repository_id, repository_version, trigger_occurrence_id, immutable_revision, input_snapshot, \
        effective_policy_snapshot, project_job_concurrency_limit, priority, state, version, created_at, updated_at, \
        metadata_retention_until, log_retention_until, artifact_retention_until, report_retention_until, \
        metadata_visible, metadata_deleted_at) \
     VALUES ($1, $2, $3, 1, $4, 1, $5, 1, $6, 'revision', '{}', '{}', 1, 0, 'running', 1, now(), now(), \
             now() + interval '100 years', now() + interval '100 years', now() + interval '100 years', \
             now() + interval '100 years', $7, CASE WHEN $7 THEN NULL ELSE now() END)",
  )
  .bind(build_id.as_uuid())
  .bind(project_id.as_uuid())
  .bind(uuid::Uuid::from_u128(configuration))
  .bind(uuid::Uuid::from_u128(30))
  .bind(uuid::Uuid::from_u128(20))
  .bind(occurrence)
  .bind(visible)
  .execute(pool)
  .await
  .unwrap();
}

fn seed_summary(
  memory: &InMemoryResourceSearchStore,
  resource: ResourceSearchResource,
  label: &str,
  context: Option<&str>,
) {
  memory
    .seed(ResourceSearchSummary::new(resource, label, context.map(str::to_owned)).unwrap())
    .unwrap();
}

fn request(
  query: &str,
  kinds: impl IntoIterator<Item = ResourceSearchKind>,
  after: Option<octacity_server_store::ResourceSearchPagePosition>,
  limit: u16,
  visibility: ResourceSearchVisibility,
) -> SearchResources {
  SearchResources::new(
    NormalizedResourceSearchQuery::new(query).unwrap(),
    kinds.into_iter().collect::<BTreeSet<_>>(),
    after,
    limit,
    visibility,
  )
  .unwrap()
}

async fn assert_parity(
  memory: &InMemoryResourceSearchStore,
  postgres: &PostgresStore,
  request: SearchResources,
) -> ResourceSearchPage {
  let expected = memory.search_resources(request.clone()).await.unwrap();
  let actual = postgres.search_resources(request).await.unwrap();
  assert_eq!(actual, expected);
  actual
}

fn resources(page: &ResourceSearchPage) -> Vec<ResourceSearchResource> {
  page.items.iter().map(ResourceSearchSummary::resource).collect()
}

async fn explain_production_search(transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> Value {
  let statement = sqlx::AssertSqlSafe(format!(
    "EXPLAIN (FORMAT JSON, COSTS OFF) {}",
    include_str!("../src/resource_search.sql")
  ));
  sqlx::query_scalar::<_, Json<Value>>(statement)
    .bind("alpha")
    .bind(true)
    .bind(true)
    .bind(true)
    .bind(true)
    .bind(true)
    .bind(None::<uuid::Uuid>)
    .bind(Vec::<uuid::Uuid>::new())
    .bind(Vec::<uuid::Uuid>::new())
    .bind(Vec::<uuid::Uuid>::new())
    .bind(Vec::<uuid::Uuid>::new())
    .bind(None::<i32>)
    .bind(None::<i32>)
    .bind(None::<String>)
    .bind(None::<uuid::Uuid>)
    .bind(21_i64)
    .fetch_one(&mut **transaction)
    .await
    .unwrap()
    .0
}

async fn explain_named_index(transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>, table: &str) -> Value {
  let statement = sqlx::AssertSqlSafe(format!(
    "EXPLAIN (FORMAT JSON, COSTS OFF) SELECT id FROM {table} \
     WHERE octacity_normalize_resource_search_text(name) LIKE '%' || $1 || '%'"
  ));
  sqlx::query_scalar::<_, Json<Value>>(statement)
    .bind("alpha")
    .fetch_one(&mut **transaction)
    .await
    .unwrap()
    .0
}

async fn explain_build_name_join(transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> Value {
  sqlx::query_scalar::<_, Json<Value>>(
    "EXPLAIN (FORMAT JSON, COSTS OFF) SELECT build.id \
     FROM build_configurations AS configuration \
     JOIN builds AS build ON build.build_configuration_id = configuration.id \
     WHERE build.metadata_visible \
       AND octacity_normalize_resource_search_text(configuration.name) LIKE '%' || $1 || '%'",
  )
  .bind("alpha")
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

fn project(value: u128) -> ProjectId {
  ProjectId::from_uuid(uuid::Uuid::from_u128(value)).unwrap()
}

fn build(value: u128) -> BuildId {
  BuildId::from_uuid(uuid::Uuid::from_u128(value)).unwrap()
}

fn agent(value: u128) -> AgentId {
  AgentId::from_uuid(uuid::Uuid::from_u128(value)).unwrap()
}

fn agent_pool(value: u128) -> PoolId {
  PoolId::from_uuid(uuid::Uuid::from_u128(value)).unwrap()
}
