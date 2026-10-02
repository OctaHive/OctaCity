mod support;

use std::{fmt::Debug, str::FromStr};

use octacity_server_domain::{
  AttemptNumber, BuildConfigurationId, BuildConfigurationVersion, BuildId, ProjectId, Timestamp, TriggerId,
  TriggerOccurrenceId,
};
use octacity_server_orchestrator::{AttemptState, BuildState};
use octacity_server_store::testing::InMemoryBuildDiscoveryStore;
use octacity_server_store::{
  BuildDiscoveryStore as _, BuildListVisibility, ListProjectBuilds, ProjectBuildFilter, ProjectBuildPage,
  ProjectBuildSummary, StoreError, TriggerCausality, TriggerCause, TriggerMetadata,
};
use octacity_server_store_postgres::PostgresStore;
use serde_json::Value;
use sqlx::{Executor as _, types::Json};
use support::TestDatabase;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn postgres_build_discovery_matches_the_in_memory_contract() {
  let database = TestDatabase::migrated().await;
  let memory = InMemoryBuildDiscoveryStore::new();
  let postgres = PostgresStore::new(database.pool.clone());
  let project_id = id(1);
  let other_project_id = id(2);
  let configuration_id = id(10);
  let other_configuration_id = id(11);
  let foreign_configuration_id = id(12);

  seed_prerequisites(&database.pool).await;
  for project in [project_id, other_project_id] {
    memory.seed_project(project).unwrap();
  }
  for (project, configuration) in [
    (project_id, configuration_id),
    (project_id, other_configuration_id),
    (other_project_id, foreign_configuration_id),
  ] {
    memory.seed_configuration(project, configuration).unwrap();
  }

  let summaries = [
    build(107, project_id, configuration_id, BuildState::Running, 30),
    build(106, project_id, configuration_id, BuildState::Running, 30),
    build(105, project_id, other_configuration_id, BuildState::Failed, 25),
    build(104, project_id, configuration_id, BuildState::Succeeded, 20),
    build(103, project_id, configuration_id, BuildState::Running, 15),
    build(102, project_id, configuration_id, BuildState::Running, 10),
    build(201, other_project_id, foreign_configuration_id, BuildState::Running, 40),
  ];
  for summary in &summaries {
    memory.seed_build(summary.clone()).unwrap();
    seed_build(&database.pool, summary).await;
  }
  for hidden in [
    build(108, project_id, configuration_id, BuildState::Running, 35),
    build(109, project_id, configuration_id, BuildState::Running, 27),
    build(101, project_id, configuration_id, BuildState::Running, 5),
  ] {
    seed_build(&database.pool, &hidden).await;
    sqlx::query("UPDATE builds SET metadata_visible = false, metadata_deleted_at = updated_at WHERE id = $1")
      .bind(hidden.id.as_uuid())
      .execute(&database.pool)
      .await
      .unwrap();
  }

  let visibility = || {
    BuildListVisibility::restricted([
      id(108),
      id(109),
      id(107),
      id(106),
      id(105),
      id(104),
      id(102),
      id(101),
      id(201),
    ])
    .unwrap()
  };
  let first_request =
    || ListProjectBuilds::new(project_id, ProjectBuildFilter::default(), None, 2, visibility()).unwrap();
  let first = assert_parity(&memory, &postgres, first_request()).await;
  assert_eq!(ids(&first), [id(107), id(106)]);

  let second_request = || {
    ListProjectBuilds::new(
      project_id,
      ProjectBuildFilter::default(),
      first.next_cursor,
      2,
      visibility(),
    )
    .unwrap()
  };
  let second = assert_parity(&memory, &postgres, second_request()).await;
  assert_eq!(ids(&second), [id(105), id(104)]);

  let final_request = || {
    ListProjectBuilds::new(
      project_id,
      ProjectBuildFilter::default(),
      second.next_cursor,
      2,
      visibility(),
    )
    .unwrap()
  };
  let final_page = assert_parity(&memory, &postgres, final_request()).await;
  assert_eq!(ids(&final_page), [id(102)]);
  assert_eq!(final_page.next_cursor, None);

  let filter = ProjectBuildFilter {
    configuration_id: Some(configuration_id),
    state: Some(BuildState::Running),
  };
  let filtered_request = || ListProjectBuilds::new(project_id, filter, None, 20, BuildListVisibility::all()).unwrap();
  let filtered = assert_parity(&memory, &postgres, filtered_request()).await;
  assert_eq!(ids(&filtered), [id(107), id(106), id(103), id(102)]);

  for (selected_project, selected_configuration, entity) in [
    (id(999), None, octacity_server_domain::EntityKind::Project),
    (
      project_id,
      Some(foreign_configuration_id),
      octacity_server_domain::EntityKind::Configuration,
    ),
  ] {
    let request = || {
      ListProjectBuilds::new(
        selected_project,
        ProjectBuildFilter {
          configuration_id: selected_configuration,
          state: None,
        },
        None,
        1,
        BuildListVisibility::none(),
      )
      .unwrap()
    };
    let memory_error = memory.list_project_builds(request()).await.unwrap_err();
    let postgres_error = postgres.list_project_builds(request()).await.unwrap_err();
    assert_eq!(memory_error, StoreError::NotFound { entity });
    assert_eq!(postgres_error, memory_error);
  }

  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn representative_build_discovery_plans_use_the_migration_indexes() {
  let database = TestDatabase::migrated().await;
  seed_prerequisites(&database.pool).await;
  seed_build(&database.pool, &build(107, id(1), id(10), BuildState::Running, 30)).await;
  sqlx::query("ANALYZE builds, attempts, trigger_occurrences")
    .execute(&database.pool)
    .await
    .unwrap();
  let mut transaction = database.pool.begin().await.unwrap();
  transaction.execute("SET LOCAL enable_seqscan = off").await.unwrap();

  let base = explain_builds(&mut transaction, None, None).await;
  assert_plan_uses(&base, "builds_project_discovery_idx");
  assert_plan_uses(&base, "attempts_build_number_key");

  let configuration = explain_builds(&mut transaction, Some(id(10)), None).await;
  assert_plan_uses(&configuration, "builds_project_configuration_discovery_idx");

  let state = explain_builds(&mut transaction, None, Some("running")).await;
  assert_plan_uses(&state, "builds_project_state_discovery_idx");

  transaction.rollback().await.unwrap();
  database.cleanup().await;
}

async fn assert_parity(
  memory: &InMemoryBuildDiscoveryStore,
  postgres: &PostgresStore,
  request: ListProjectBuilds,
) -> ProjectBuildPage {
  let expected = memory.list_project_builds(request.clone()).await.unwrap();
  let actual = postgres.list_project_builds(request).await.unwrap();
  assert_eq!(actual, expected);
  actual
}

async fn seed_prerequisites(pool: &sqlx::PgPool) {
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
  sqlx::query(
    "INSERT INTO repositories (id, project_id, name, created_at) \
     VALUES ($1, $3, 'repository-one', to_timestamp(0)), ($2, $4, 'repository-two', to_timestamp(0))",
  )
  .bind(uuid::Uuid::from_u128(30))
  .bind(uuid::Uuid::from_u128(31))
  .bind(uuid::Uuid::from_u128(1))
  .bind(uuid::Uuid::from_u128(2))
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO repository_versions \
       (repository_id, version, vcs_integration_id, repository_locator, selection_policy, published_at) \
     VALUES ($1, 1, $3, 'one/repository', '{}', to_timestamp(0)), \
            ($2, 1, $3, 'two/repository', '{}', to_timestamp(0))",
  )
  .bind(uuid::Uuid::from_u128(30))
  .bind(uuid::Uuid::from_u128(31))
  .bind(uuid::Uuid::from_u128(32))
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO pipelines (id, project_id, name, created_at) \
     VALUES ($1, $3, 'pipeline-one', to_timestamp(0)), ($2, $4, 'pipeline-two', to_timestamp(0))",
  )
  .bind(uuid::Uuid::from_u128(40))
  .bind(uuid::Uuid::from_u128(41))
  .bind(uuid::Uuid::from_u128(1))
  .bind(uuid::Uuid::from_u128(2))
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO pipeline_versions (pipeline_id, version, dag_snapshot, published_at) \
     VALUES ($1, 1, '{}', to_timestamp(0)), ($2, 1, '{}', to_timestamp(0))",
  )
  .bind(uuid::Uuid::from_u128(40))
  .bind(uuid::Uuid::from_u128(41))
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO pools \
       (id, version, name, enabled, drain_state, admission_policy, concurrency_limit, created_at) \
     VALUES ($1, 1, 'pool', true, 'accepting', '{\"mode\":\"any\"}', 1, to_timestamp(0))",
  )
  .bind(uuid::Uuid::from_u128(50))
  .execute(pool)
  .await
  .unwrap();

  for (configuration, project, repository, pipeline) in
    [(10_u128, 1_u128, 30_u128, 40_u128), (11, 1, 30, 40), (12, 2, 31, 41)]
  {
    sqlx::query(
      "INSERT INTO build_configurations (id, project_id, name, created_at) \
       VALUES ($1, $2, $3, to_timestamp(0))",
    )
    .bind(uuid::Uuid::from_u128(configuration))
    .bind(uuid::Uuid::from_u128(project))
    .bind(format!("configuration-{configuration}"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
      "INSERT INTO build_configuration_versions \
         (build_configuration_id, version, enabled, repository_id, repository_version, pipeline_id, \
          pipeline_version, configuration_snapshot, job_concurrency_limit, allowed_pool_ids, retry_max_attempts, \
          retries_infrastructure, published_at) \
       VALUES ($1, 1, true, $2, 1, $3, 1, '{}', 1, ARRAY[$4::uuid], 1, false, to_timestamp(0))",
    )
    .bind(uuid::Uuid::from_u128(configuration))
    .bind(uuid::Uuid::from_u128(repository))
    .bind(uuid::Uuid::from_u128(pipeline))
    .bind(uuid::Uuid::from_u128(50))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
      "INSERT INTO triggers \
         (id, version, build_configuration_id, build_configuration_version, kind, enabled, definition, created_at) \
       VALUES ($1, 1, $2, 1, 'manual', true, '{}', to_timestamp(0))",
    )
    .bind(uuid::Uuid::from_u128(configuration + 100))
    .bind(uuid::Uuid::from_u128(configuration))
    .execute(pool)
    .await
    .unwrap();
  }
}

async fn seed_build(pool: &sqlx::PgPool, summary: &ProjectBuildSummary) {
  let occurrence_id: TriggerOccurrenceId = id(summary.id.as_uuid().as_u128() + 1_000);
  let trigger_id: TriggerId = id(summary.configuration_id.as_uuid().as_u128() + 100);
  let created_at = summary.created_at.unix_millis();
  let updated_at = summary.terminal_at.unwrap_or(summary.created_at).unix_millis();
  let mut transaction = pool.begin().await.unwrap();
  sqlx::query(
    "INSERT INTO trigger_occurrences \
       (id, trigger_id, trigger_version, build_configuration_id, build_configuration_version, kind, security_scope, \
        deduplication_identity, cause, causality, provider_metadata, source_time, state, build_id, request_digest, \
        created_at, updated_at) \
     VALUES ($1, $2, 1, $3, $4, 'manual', 'trusted-network', $5, $6, $7, $8, \
             to_timestamp($9::double precision / 1000.0), 'accepted', NULL, $10, \
             to_timestamp($9::double precision / 1000.0), to_timestamp($9::double precision / 1000.0))",
  )
  .bind(occurrence_id.as_uuid())
  .bind(trigger_id.as_uuid())
  .bind(summary.configuration_id.as_uuid())
  .bind(i64::try_from(summary.configuration_version.get()).unwrap())
  .bind(format!("build:{}", summary.id))
  .bind(Json(summary.cause.clone()))
  .bind(Json(TriggerCausality::root(occurrence_id)))
  .bind(Json(TriggerMetadata::default()))
  .bind(created_at)
  .bind(vec![7_u8; 32])
  .execute(&mut *transaction)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO builds \
       (id, project_id, build_configuration_id, build_configuration_version, pipeline_id, pipeline_version, \
        repository_id, repository_version, trigger_occurrence_id, immutable_revision, input_snapshot, \
        effective_policy_snapshot, metadata_retention_until, log_retention_until, artifact_retention_until, \
        report_retention_until, project_job_concurrency_limit, priority, state, version, created_at, updated_at) \
     SELECT $1, $2, $3, $4, version.pipeline_id, version.pipeline_version, version.repository_id, \
            version.repository_version, $5, 'revision', '{}', '{}', \
            to_timestamp(($6 + 10000)::double precision / 1000.0), \
            to_timestamp(($6 + 10000)::double precision / 1000.0), \
            to_timestamp(($6 + 10000)::double precision / 1000.0), \
            to_timestamp(($6 + 10000)::double precision / 1000.0), 1, 0, $7, 1, \
            to_timestamp($6::double precision / 1000.0), to_timestamp($8::double precision / 1000.0) \
       FROM build_configuration_versions AS version \
       WHERE version.build_configuration_id = $3 AND version.version = $4",
  )
  .bind(summary.id.as_uuid())
  .bind(summary.project_id.as_uuid())
  .bind(summary.configuration_id.as_uuid())
  .bind(i64::try_from(summary.configuration_version.get()).unwrap())
  .bind(occurrence_id.as_uuid())
  .bind(created_at)
  .bind(build_state(summary.state))
  .bind(updated_at)
  .execute(&mut *transaction)
  .await
  .unwrap();
  if summary.current_attempt_number.get() > 1 {
    sqlx::query(
      "INSERT INTO attempts \
         (id, build_id, attempt_number, retry_of_attempt_id, state, version, created_at, updated_at) \
       VALUES ($1, $2, 1, NULL, 'failed', 1, to_timestamp($3::double precision / 1000.0), \
               to_timestamp($3::double precision / 1000.0))",
    )
    .bind(uuid::Uuid::from_u128(
      summary.current_attempt_id.as_uuid().as_u128() + 100_000,
    ))
    .bind(summary.id.as_uuid())
    .bind(created_at)
    .execute(&mut *transaction)
    .await
    .unwrap();
  }
  sqlx::query("UPDATE trigger_occurrences SET build_id = $1 WHERE id = $2")
    .bind(summary.id.as_uuid())
    .bind(occurrence_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .unwrap();
  sqlx::query(
    "INSERT INTO attempts \
       (id, build_id, attempt_number, retry_of_attempt_id, state, version, created_at, updated_at) \
     VALUES ($1, $2, $3, NULL, $4, 1, to_timestamp($5::double precision / 1000.0), \
             to_timestamp($6::double precision / 1000.0))",
  )
  .bind(summary.current_attempt_id.as_uuid())
  .bind(summary.id.as_uuid())
  .bind(i64::try_from(summary.current_attempt_number.get()).unwrap())
  .bind(attempt_state(summary.current_attempt_state))
  .bind(created_at)
  .bind(updated_at)
  .execute(&mut *transaction)
  .await
  .unwrap();
  transaction.commit().await.unwrap();
}

async fn explain_builds(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  configuration_id: Option<BuildConfigurationId>,
  state: Option<&str>,
) -> Value {
  sqlx::query_scalar::<_, Json<Value>>(
    "EXPLAIN (FORMAT JSON, COSTS OFF) \
     SELECT build.id \
     FROM builds AS build \
     JOIN trigger_occurrences AS occurrence ON occurrence.id = build.trigger_occurrence_id \
     JOIN LATERAL (\
       SELECT attempt.id FROM attempts AS attempt \
       WHERE attempt.build_id = build.id ORDER BY attempt.attempt_number DESC LIMIT 1\
     ) AS current_attempt ON TRUE \
     WHERE build.project_id = $1 AND build.metadata_visible \
       AND ($2 OR build.id = ANY($3::uuid[])) \
       AND ($4::uuid IS NULL OR build.build_configuration_id = $4) \
       AND ($5::text IS NULL OR build.state = $5) \
       AND ($6::bigint IS NULL OR (build.created_at, build.id) < \
         (to_timestamp($6::double precision / 1000.0), $7::uuid)) \
     ORDER BY build.created_at DESC, build.id DESC LIMIT $8",
  )
  .bind(uuid::Uuid::from_u128(1))
  .bind(true)
  .bind(Vec::<uuid::Uuid>::new())
  .bind(configuration_id.map(BuildConfigurationId::as_uuid))
  .bind(state)
  .bind(Option::<i64>::None)
  .bind(Option::<uuid::Uuid>::None)
  .bind(3_i64)
  .fetch_one(&mut **transaction)
  .await
  .unwrap()
  .0
}

fn build(
  value: u128,
  project_id: ProjectId,
  configuration_id: BuildConfigurationId,
  state: BuildState,
  created_at: i64,
) -> ProjectBuildSummary {
  ProjectBuildSummary {
    id: id(value),
    project_id,
    configuration_id,
    configuration_version: BuildConfigurationVersion::INITIAL,
    cause: TriggerCause::Manual {},
    state,
    created_at: time(created_at),
    current_attempt_id: id(value + 10_000),
    current_attempt_number: AttemptNumber::new(if value == 105 { 2 } else { 1 }).unwrap(),
    current_attempt_state: match state {
      BuildState::Queued => AttemptState::Created,
      BuildState::Running => AttemptState::Running,
      BuildState::Succeeded => AttemptState::Succeeded,
      BuildState::Failed => AttemptState::Failed,
      BuildState::Cancelled => AttemptState::Cancelled,
    },
    terminal_at: state.is_terminal().then(|| time(created_at + 1)),
  }
}

fn ids(page: &ProjectBuildPage) -> Vec<BuildId> {
  page.items.iter().map(|item| item.id).collect()
}

fn id<I>(value: u128) -> I
where
  I: FromStr,
  I::Err: Debug,
{
  uuid::Uuid::from_u128(value).to_string().parse().unwrap()
}

fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}

const fn build_state(state: BuildState) -> &'static str {
  match state {
    BuildState::Queued => "queued",
    BuildState::Running => "running",
    BuildState::Succeeded => "succeeded",
    BuildState::Failed => "failed",
    BuildState::Cancelled => "cancelled",
  }
}

const fn attempt_state(state: AttemptState) -> &'static str {
  match state {
    AttemptState::Created => "created",
    AttemptState::Running => "running",
    AttemptState::Succeeded => "succeeded",
    AttemptState::Failed => "failed",
    AttemptState::Cancelled => "cancelled",
  }
}

fn assert_plan_uses(plan: &Value, index: &str) {
  assert!(
    plan.to_string().contains(index),
    "expected query plan to use {index}: {plan}"
  );
}
