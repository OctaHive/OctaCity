#[path = "migrations/snapshot.rs"]
mod snapshot;
mod support;

use std::collections::BTreeSet;

use octacity_server_store_postgres::{
  MIGRATOR, MigrationStatus, PREVIOUS_BINARY_SCHEMA_VERSION, current_schema_version,
};
use snapshot::DatabaseSnapshot;
use sqlx::{
  SqlSafeStr as _,
  migrate::{Migration, MigrationType, Migrator},
};
use support::TestDatabase;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn completed_issue_publication_recovery_does_not_read_history_on_readiness_or_claim() {
  let database = TestDatabase::migrated().await;
  let reader = sqlx::postgres::PgPoolOptions::new()
    .max_connections(1)
    .connect_with((*database.pool.connect_options()).clone())
    .await
    .unwrap();
  sqlx::query("SET statement_timeout='500ms'")
    .execute(&reader)
    .await
    .unwrap();
  let mut history = database.pool.begin().await.unwrap();
  // An unavailable historical-publication dependency cannot affect the completed
  // migration or the ordinary claim port. No wall-clock performance assertion.
  sqlx::query("LOCK TABLE outbox_entries IN ACCESS EXCLUSIVE MODE")
    .execute(&mut *history)
    .await
    .unwrap();
  assert_eq!(
    octacity_server_store_postgres::migration_status(&reader).await.unwrap(),
    MigrationStatus::Current,
    "completed recovery must not rescan historical publications"
  );
  use octacity_server_store::{ClaimFactoryOutbox, FactoryRunStore};
  let claim = ClaimFactoryOutbox::new(
    octacity_server_factory::FactoryKey::new("issue-reporter").unwrap(),
    octacity_server_domain::Timestamp::from_unix_millis(1).unwrap(),
    octacity_server_domain::Timestamp::from_unix_millis(10).unwrap(),
    1,
  )
  .unwrap()
  .only_kind(octacity_server_factory::FactoryKey::new("issue.status.projection").unwrap());
  assert!(
    octacity_server_store_postgres::PostgresStore::new(reader.clone())
      .claim_factory_outbox(claim)
      .await
      .unwrap()
      .is_empty()
  );
  history.rollback().await.unwrap();
  reader.close().await;
  database.cleanup().await;
}

const SNAPSHOT_PROJECT_ID: &str = "0199a6f4-d56c-7440-9aa2-6a320f862795";
const LEGACY_IDEMPOTENCY_SCOPE: &str = "schema-rehearsal";
const LEGACY_IDEMPOTENCY_KEY: &str = "legacy-writer";
const FACTORY_TABLES: [&str; 43] = [
  "factory_action_projections",
  "factory_action_items",
  "factory_source_inputs",
  "factory_source_groups",
  "factory_source_scans",
  "factory_source_preparations",
  "factory_source_materializations",
  "factory_source_projection_intents",
  "factory_source_projection_journals",
  "factory_source_input_objects",
  "factory_audit_links",
  "factory_configuration_versions",
  "factory_configuration_flow_definitions",
  "factory_configurations",
  "factory_flow_definition_versions",
  "factory_flow_definitions",
  "factory_flow_publications",
  "factory_flow_runs",
  "factory_flow_incoming",
  "factory_flow_inputs",
  "factory_flow_records",
  "factory_flow_build_intents",
  "factory_flow_build_executions",
  "factory_lifecycle_checkpoints",
  "factory_node_attempts",
  "factory_node_attempt_completions",
  "factory_node_stops",
  "factory_outbox_records",
  "factory_phase_pool_policies",
  "factory_phase_pool_entries",
  "factory_phase_pool_passes",
  "factory_phase_pool_selections",
  "factory_run_budgets",
  "factory_run_claims",
  "factory_run_controls",
  "factory_run_current",
  "factory_runs",
  "factory_work_envelopes",
  "factory_workflow_cycles",
  "factory_execution_input_manifests",
  "factory_execution_inputs",
  "factory_decision_signal_requests",
  "factory_decision_signal_receipts",
];
const INTEGRATION_TABLES: [&str; 7] = [
  "integration_connections",
  "integration_connection_versions",
  "integration_administration",
  "integration_administration_operations",
  "integration_credential_owners",
  "integration_project_access",
  "integration_project_access_versions",
];
const COLLABORATION_TABLES: [&str; 21] = [
  "issue_code_review_associations",
  "issue_code_review_observations",
  "issue_comments",
  "issue_legacy_backfill",
  "issue_content_retention",
  "issue_numbers",
  "issues",
  "issue_revisions",
  "issue_status_history",
  "issue_property_versions",
  "issue_execution_requests",
  "issue_work_associations",
  "issue_source_links",
  "issue_source_observations",
  "issue_source_refresh",
  "issue_source_reads",
  "issue_source_mappings",
  "issue_reporter_recovery",
  "project_text_preparations",
  "project_text_input_objects",
  "project_text_documents",
];
const ISSUE_WORKFLOW_TABLES: [&str; 3] = ["issue_workflows", "issue_workflow_versions", "issue_workflow_defaults"];
const FACTORY_RETENTION_TABLES: [&str; 2] = ["factory_artifact_references", "factory_retention_work"];
const LEGACY_OPERATION_SCOPES: [(&str, &str, &str); 5] = [
  ("create-project", "legacy-management", "trusted-network"),
  ("claim-ready-job", "legacy-agent", "legacy-agent-data-plane"),
  (
    "complete-managed-webhook-create",
    "legacy-adapter",
    "legacy-adapter-data-plane",
  ),
  ("recover-expired-lease", "legacy-worker", "legacy-worker-data-plane"),
  ("accept-trigger", "legacy-trigger", "legacy-trigger-data-plane"),
];

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn migrates_an_empty_database_and_is_reentrant() {
  let database = TestDatabase::migrated().await;
  let result = verify_migration(&database.pool).await;
  database.cleanup().await;
  result.unwrap();
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn flow_publication_schema_rejects_oversized_and_non_object_documents() {
  let database = TestDatabase::migrated().await;
  let flow_id = uuid::Uuid::new_v4();
  sqlx::query(
    "WITH project AS (\
       INSERT INTO projects (id, name, version, created_at, updated_at) \
       VALUES ($1, 'publication bounds', 1, now(), now()) RETURNING id\
     ), owner AS (\
       INSERT INTO factory_flow_definitions (id, project_id, current_version, created_at, updated_at) \
       SELECT $2, id, 1, now(), now() FROM project RETURNING id\
     ), definition AS (\
       INSERT INTO factory_flow_definition_versions (id, version, definition_digest, definition) \
       SELECT id, 1, $3, '{}'::jsonb FROM owner RETURNING id\
     ) \
     INSERT INTO factory_flow_publications (id, version, publication) \
     SELECT id, 1, '{}'::jsonb FROM definition",
  )
  .bind(uuid::Uuid::new_v4())
  .bind(flow_id)
  .bind(vec![1_u8; 32])
  .execute(&database.pool)
  .await
  .unwrap();
  for publication in [
    serde_json::json!({"payload": "x".repeat(16 * 1024 * 1024 + 1)}),
    serde_json::json!([]),
  ] {
    let error = sqlx::query("UPDATE factory_flow_publications SET publication = $1 WHERE id = $2")
      .bind(sqlx::types::Json(publication))
      .bind(flow_id)
      .execute(&database.pool)
      .await
      .unwrap_err();
    assert_eq!(
      error.as_database_error().and_then(|error| error.code()).as_deref(),
      Some("23514")
    );
  }
  let publication: sqlx::types::Json<serde_json::Value> =
    sqlx::query_scalar("SELECT publication FROM factory_flow_publications WHERE id = $1")
      .bind(flow_id)
      .fetch_one(&database.pool)
      .await
      .unwrap();
  assert_eq!(publication.0, serde_json::json!({}));
  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn rehearses_forward_failure_and_previous_binary_snapshot_rollback() {
  let result = verify_schema_rehearsal().await;
  result.unwrap();
}

async fn verify_schema_rehearsal() -> Result<(), Box<dyn std::error::Error>> {
  assert_eq!(PREVIOUS_BINARY_SCHEMA_VERSION, 46);
  assert!(current_schema_version() > PREVIOUS_BINARY_SCHEMA_VERSION);

  let mut previous = TestDatabase::empty().await;
  MIGRATOR.run_to(PREVIOUS_BINARY_SCHEMA_VERSION, &previous.pool).await?;
  for table in FACTORY_TABLES
    .into_iter()
    .chain(INTEGRATION_TABLES)
    .chain(ISSUE_WORKFLOW_TABLES)
    .chain(COLLABORATION_TABLES)
  {
    assert!(!table_exists(&previous.pool, table).await?);
  }
  for table in FACTORY_RETENTION_TABLES {
    assert!(!table_exists(&previous.pool, table).await?);
  }
  seed_snapshot_marker(&previous.pool).await?;
  seed_legacy_idempotency_record(&previous.pool).await?;
  assert_eq!(
    octacity_server_store_postgres::migration_status(&previous.pool).await?,
    MigrationStatus::Pending
  );
  let snapshot = DatabaseSnapshot::capture(&mut previous).await;

  octacity_server_store_postgres::migrate(&previous.pool).await?;
  assert_eq!(
    octacity_server_store_postgres::migration_status(&previous.pool).await?,
    MigrationStatus::Current
  );
  assert!(authenticated_management_actor_is_allowed(&previous.pool).await?);
  verify_security_scoped_idempotency(&previous.pool).await?;
  verify_definition_discovery_indexes(&previous.pool).await?;
  verify_build_discovery_indexes(&previous.pool).await?;
  verify_resource_search_indexes(&previous.pool).await?;
  verify_operator_attention_schema(&previous.pool).await?;
  verify_factory_schema(&previous.pool).await?;
  assert_snapshot_marker(&previous.pool).await?;

  let failed = snapshot.restore().await;
  let error = octacity_server_store_postgres::run_migrator(&failed.pool, &deliberately_failing_migrator())
    .await
    .unwrap_err();
  assert!(
    matches!(error, sqlx::migrate::MigrateError::ExecuteMigration(_, version) if version == current_schema_version())
  );
  assert_eq!(
    octacity_server_store_postgres::migration_status(&failed.pool).await?,
    MigrationStatus::Pending,
    "a transactional migration failure must leave the exact prior history"
  );
  assert!(!table_exists(&failed.pool, "migration_failure_probe").await?);
  assert_snapshot_marker(&failed.pool).await?;
  octacity_server_store_postgres::migrate(&failed.pool).await?;
  assert_eq!(
    octacity_server_store_postgres::migration_status(&failed.pool).await?,
    MigrationStatus::Current
  );

  let rollback = snapshot.restore().await;
  previous_binary_migrator().run(&rollback.pool).await?;
  assert_eq!(
    maximum_applied_version(&rollback.pool).await?,
    PREVIOUS_BINARY_SCHEMA_VERSION
  );
  assert!(authenticated_management_actor_is_allowed(&rollback.pool).await?);
  verify_security_scoped_idempotency(&rollback.pool).await?;
  verify_definition_discovery_indexes(&rollback.pool).await?;
  verify_build_discovery_indexes(&rollback.pool).await?;
  verify_resource_search_indexes(&rollback.pool).await?;
  verify_operator_attention_schema(&rollback.pool).await?;
  for table in FACTORY_TABLES
    .into_iter()
    .chain(INTEGRATION_TABLES)
    .chain(ISSUE_WORKFLOW_TABLES)
    .chain(COLLABORATION_TABLES)
  {
    assert!(!table_exists(&rollback.pool, table).await?);
  }
  for table in FACTORY_RETENTION_TABLES {
    assert!(!table_exists(&rollback.pool, table).await?);
  }
  assert_legacy_idempotency_record(&rollback.pool).await?;
  assert_snapshot_marker(&rollback.pool).await?;

  rollback.cleanup().await;
  failed.cleanup().await;
  previous.cleanup().await;
  snapshot.cleanup().await;
  Ok(())
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn rejects_newer_changed_and_discontinuous_migration_histories() {
  let database = TestDatabase::migrated().await;
  let result = verify_incompatible_histories(&database.pool).await;
  database.cleanup().await;
  result.unwrap();
}

async fn verify_incompatible_histories(pool: &sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
  let newest = current_schema_version();
  sqlx::query(
    "INSERT INTO _sqlx_migrations (version, description, installed_on, success, checksum, execution_time) \
     VALUES ($1, 'unknown future migration', now(), true, $2, 0)",
  )
  .bind(newest + 1)
  .bind(vec![0_u8; 48])
  .execute(pool)
  .await?;
  assert_eq!(
    octacity_server_store_postgres::migration_status(pool).await?,
    MigrationStatus::Incompatible
  );
  sqlx::query("DELETE FROM _sqlx_migrations WHERE version = $1")
    .bind(newest + 1)
    .execute(pool)
    .await?;

  sqlx::query("UPDATE _sqlx_migrations SET checksum = $1 WHERE version = $2")
    .bind(vec![1_u8; 48])
    .bind(newest)
    .execute(pool)
    .await?;
  assert_eq!(
    octacity_server_store_postgres::migration_status(pool).await?,
    MigrationStatus::Incompatible
  );
  let newest_checksum = MIGRATOR
    .iter()
    .find(|migration| migration.version == newest)
    .expect("current schema migration exists")
    .checksum
    .to_vec();
  sqlx::query("UPDATE _sqlx_migrations SET checksum = $1 WHERE version = $2")
    .bind(newest_checksum)
    .bind(newest)
    .execute(pool)
    .await?;

  sqlx::query("UPDATE _sqlx_migrations SET success = false WHERE version = $1")
    .bind(newest)
    .execute(pool)
    .await?;
  assert_eq!(
    octacity_server_store_postgres::migration_status(pool).await?,
    MigrationStatus::Incompatible
  );
  sqlx::query("UPDATE _sqlx_migrations SET success = true WHERE version = $1")
    .bind(newest)
    .execute(pool)
    .await?;

  sqlx::query("DELETE FROM _sqlx_migrations WHERE version = $1")
    .bind(PREVIOUS_BINARY_SCHEMA_VERSION)
    .execute(pool)
    .await?;
  assert_eq!(
    octacity_server_store_postgres::migration_status(pool).await?,
    MigrationStatus::Incompatible
  );
  Ok(())
}

fn previous_binary_migrator() -> Migrator {
  Migrator::with_migrations(
    MIGRATOR
      .iter()
      .filter(|migration| migration.version <= PREVIOUS_BINARY_SCHEMA_VERSION)
      .cloned()
      .collect(),
  )
}

fn deliberately_failing_migrator() -> Migrator {
  let mut migrations: Vec<_> = previous_binary_migrator().iter().cloned().collect();
  migrations.push(Migration::new(
    current_schema_version(),
    "intentional rehearsal failure".into(),
    MigrationType::Simple,
    "CREATE TABLE migration_failure_probe (id BIGINT PRIMARY KEY); SELECT missing_column FROM migration_failure_probe;"
      .into_sql_str(),
    false,
  ));
  Migrator::with_migrations(migrations)
}

async fn seed_snapshot_marker(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
  sqlx::query(
    "INSERT INTO projects (id, parent_id, name, version, created_at, updated_at) \
     VALUES ($1::uuid, NULL, 'schema-rehearsal', 1, now(), now())",
  )
  .bind(SNAPSHOT_PROJECT_ID)
  .execute(pool)
  .await?;
  Ok(())
}

async fn seed_legacy_idempotency_record(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
  sqlx::query(
    "INSERT INTO idempotency_records (scope, idempotency_key, request_digest, outcome, created_at) \
     VALUES ($1, $2, $3, '{\"schema_version\": 1}', now())",
  )
  .bind(LEGACY_IDEMPOTENCY_SCOPE)
  .bind(LEGACY_IDEMPOTENCY_KEY)
  .bind(vec![7_u8; 32])
  .execute(pool)
  .await?;
  for (scope, key, security_scope) in LEGACY_OPERATION_SCOPES {
    sqlx::query(
      "INSERT INTO idempotency_records \
         (scope, security_scope, idempotency_key, request_digest, outcome, created_at) \
       VALUES ($1, $2, $3, $4, '{\"schema_version\": 1}', now())",
    )
    .bind(scope)
    .bind(security_scope)
    .bind(key)
    .bind(vec![7_u8; 32])
    .execute(pool)
    .await?;
  }
  Ok(())
}

async fn assert_legacy_idempotency_record(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
  let count: i64 =
    sqlx::query_scalar("SELECT COUNT(*) FROM idempotency_records WHERE scope = $1 AND idempotency_key = $2")
      .bind(LEGACY_IDEMPOTENCY_SCOPE)
      .bind(LEGACY_IDEMPOTENCY_KEY)
      .fetch_one(pool)
      .await?;
  assert_eq!(count, 1);
  for (scope, key, _) in LEGACY_OPERATION_SCOPES {
    let count: i64 =
      sqlx::query_scalar("SELECT COUNT(*) FROM idempotency_records WHERE scope = $1 AND idempotency_key = $2")
        .bind(scope)
        .bind(key)
        .fetch_one(pool)
        .await?;
    assert_eq!(count, 1);
  }
  Ok(())
}

async fn assert_snapshot_marker(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
  let name = sqlx::query_scalar::<_, String>("SELECT name FROM projects WHERE id = $1::uuid")
    .bind(SNAPSHOT_PROJECT_ID)
    .fetch_one(pool)
    .await?;
  assert_eq!(name, "schema-rehearsal");
  Ok(())
}

async fn maximum_applied_version(pool: &sqlx::PgPool) -> Result<i64, sqlx::Error> {
  sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations")
    .fetch_one(pool)
    .await
}

async fn authenticated_management_actor_is_allowed(pool: &sqlx::PgPool) -> Result<bool, sqlx::Error> {
  let definition: String = sqlx::query_scalar(
    "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
     WHERE conrelid = 'audit_facts'::regclass AND conname = 'audit_facts_actor_kind_known'",
  )
  .fetch_one(pool)
  .await?;
  Ok(definition.contains("'authenticated_management'::text"))
}

async fn table_exists(pool: &sqlx::PgPool, table: &str) -> Result<bool, sqlx::Error> {
  sqlx::query_scalar(
    "SELECT EXISTS (\
       SELECT 1 FROM information_schema.tables \
       WHERE table_schema = 'public' AND table_name = $1\
     )",
  )
  .bind(table)
  .fetch_one(pool)
  .await
}

async fn column_exists(pool: &sqlx::PgPool, table: &str, column: &str) -> Result<bool, sqlx::Error> {
  sqlx::query_scalar(
    "SELECT EXISTS (\
       SELECT 1 FROM information_schema.columns \
       WHERE table_schema = 'public' AND table_name = $1 AND column_name = $2\
     )",
  )
  .bind(table)
  .bind(column)
  .fetch_one(pool)
  .await
}

async fn verify_migration(pool: &sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
  octacity_server_store_postgres::migrate(pool).await?;
  assert!(octacity_server_store_postgres::health_check(pool).await);
  assert_eq!(
    octacity_server_store_postgres::migration_status(pool).await?,
    octacity_server_store_postgres::MigrationStatus::Current
  );

  let actual: BTreeSet<String> = sqlx::query_scalar(
    "SELECT tablename FROM pg_tables WHERE schemaname = 'public' AND tablename <> '_sqlx_migrations'",
  )
  .fetch_all(pool)
  .await?
  .into_iter()
  .collect();
  let expected: BTreeSet<String> = [
    "adapter_retries",
    "agent_enrollment_credentials",
    "agent_registrations",
    "agents",
    "artifact_uploads",
    "artifacts",
    "attempts",
    "audit_facts",
    "build_cancellations",
    "build_configurations",
    "build_configuration_versions",
    "builds",
    "build_result_retention_holds",
    "cache_sessions",
    "cache_actions",
    "cache_blobs",
    "idempotency_records",
    "job_dependencies",
    "job_events",
    "job_completions",
    "jobs",
    "leases",
    "log_chunk_manifests",
    "log_index_project_positions",
    "log_indexing_work",
    "log_search_applied_work",
    "log_search_build_tombstones",
    "log_search_documents",
    "log_search_project_positions",
    "managed_webhook_operations",
    "outbox_entries",
    "operator_attention_events",
    "orphan_log_chunk_work",
    "pipeline_versions",
    "pipelines",
    "pools",
    "project_policy_versions",
    "projects",
    "ready_queue_entries",
    "repositories",
    "repository_versions",
    "retention_work",
    "schedules",
    "trigger_evaluation_work",
    "trigger_occurrences",
    "triggers",
    "webhook_deliveries",
    "webhook_integrations",
    "webhook_normalized_events",
    "worker_claims",
  ]
  .into_iter()
  .chain(FACTORY_TABLES)
  .chain(FACTORY_RETENTION_TABLES)
  .chain(INTEGRATION_TABLES)
  .chain(ISSUE_WORKFLOW_TABLES)
  .chain(COLLABORATION_TABLES)
  .map(str::to_owned)
  .collect();
  if actual != expected {
    return Err(
      std::io::Error::other(format!(
        "unexpected authoritative-store tables: expected {expected:?}, got {actual:?}"
      ))
      .into(),
    );
  }

  let application_triggers: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) \
     FROM pg_trigger AS trigger \
     JOIN pg_class AS relation ON relation.oid = trigger.tgrelid \
     JOIN pg_namespace AS namespace ON namespace.oid = relation.relnamespace \
     WHERE namespace.nspname = 'public' AND NOT trigger.tgisinternal",
  )
  .fetch_one(pool)
  .await?;
  assert_eq!(
    application_triggers, 0,
    "business behavior must not be implemented by triggers"
  );

  let stored_business_routines: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) \
     FROM pg_proc AS routine \
     JOIN pg_namespace AS namespace ON namespace.oid = routine.pronamespace \
     JOIN pg_language AS language ON language.oid = routine.prolang \
     WHERE namespace.nspname = 'public' AND language.lanname = 'plpgsql'",
  )
  .fetch_one(pool)
  .await?;
  assert_eq!(
    stored_business_routines, 0,
    "business behavior must not be implemented by stored PL/pgSQL routines"
  );
  verify_security_scoped_idempotency(pool).await?;
  verify_definition_discovery_indexes(pool).await?;
  verify_build_discovery_indexes(pool).await?;
  verify_resource_search_indexes(pool).await?;
  verify_operator_attention_schema(pool).await?;
  verify_factory_schema(pool).await?;
  verify_factory_constraint_enforcement(pool).await?;
  Ok(())
}

async fn verify_definition_discovery_indexes(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
  let definitions = definition_discovery_index_definitions(pool).await?;
  assert_eq!(definitions.len(), 3);
  assert!(definitions[0].1.contains("(project_id, id)"));
  assert!(definitions[1].1.contains("(project_id, id)"));
  assert!(definitions[2].1.contains("(project_id, id)"));
  Ok(())
}

async fn definition_discovery_index_definitions(pool: &sqlx::PgPool) -> Result<Vec<(String, String)>, sqlx::Error> {
  sqlx::query_as(
    "SELECT indexname, indexdef FROM pg_indexes \
     WHERE schemaname = 'public' AND indexname = ANY($1::text[]) ORDER BY indexname",
  )
  .bind(
    [
      "build_configurations_project_discovery_idx",
      "pipelines_project_discovery_idx",
      "repositories_project_discovery_idx",
    ]
    .as_slice(),
  )
  .fetch_all(pool)
  .await
}

async fn verify_build_discovery_indexes(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
  let definitions = build_discovery_index_definitions(pool).await?;
  assert_eq!(definitions.len(), 3);
  assert!(
    definitions[0]
      .1
      .contains("(project_id, build_configuration_id, created_at DESC, id DESC)")
  );
  assert!(definitions[1].1.contains("(project_id, created_at DESC, id DESC)"));
  assert!(
    definitions[2]
      .1
      .contains("(project_id, state, created_at DESC, id DESC)")
  );
  assert!(
    definitions
      .iter()
      .all(|(_, definition)| definition.contains("WHERE metadata_visible"))
  );
  Ok(())
}

async fn build_discovery_index_definitions(pool: &sqlx::PgPool) -> Result<Vec<(String, String)>, sqlx::Error> {
  sqlx::query_as(
    "SELECT indexname, indexdef FROM pg_indexes \
     WHERE schemaname = 'public' AND indexname = ANY($1::text[]) ORDER BY indexname",
  )
  .bind(
    [
      "builds_project_configuration_discovery_idx",
      "builds_project_discovery_idx",
      "builds_project_state_discovery_idx",
    ]
    .as_slice(),
  )
  .fetch_all(pool)
  .await
}

async fn verify_resource_search_indexes(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
  let extension_exists: bool =
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_extension WHERE extname = 'pg_trgm')")
      .fetch_one(pool)
      .await?;
  assert!(extension_exists);

  let definitions = resource_search_index_definitions(pool).await?;
  assert_eq!(definitions.len(), 5);
  for (name, definition) in &definitions {
    if name == "builds_configuration_resource_search_idx" {
      assert!(definition.contains("(build_configuration_id, id)"));
      assert!(definition.contains("WHERE metadata_visible"));
    } else {
      assert!(definition.contains("USING gin"));
      assert!(definition.contains("gin_trgm_ops"));
      assert!(definition.contains("octacity_normalize_resource_search_text"));
    }
  }
  let normalization: String = sqlx::query_scalar("SELECT octacity_normalize_resource_search_text(E'  ALPHA\\tBeta  ')")
    .fetch_one(pool)
    .await?;
  assert_eq!(normalization, "alpha beta");
  Ok(())
}

async fn resource_search_index_definitions(pool: &sqlx::PgPool) -> Result<Vec<(String, String)>, sqlx::Error> {
  sqlx::query_as(
    "SELECT indexname, indexdef FROM pg_indexes \
     WHERE schemaname = 'public' AND indexname = ANY($1::text[]) ORDER BY indexname",
  )
  .bind(
    [
      "agents_resource_search_name_idx",
      "build_configurations_resource_search_name_idx",
      "builds_configuration_resource_search_idx",
      "pools_resource_search_name_idx",
      "projects_resource_search_name_idx",
    ]
    .as_slice(),
  )
  .fetch_all(pool)
  .await
}

async fn verify_operator_attention_schema(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
  assert!(table_exists(pool, "operator_attention_events").await?);
  let definitions = operator_attention_index_definitions(pool).await?;
  assert_eq!(definitions.len(), 3);
  assert!(
    definitions
      .iter()
      .find(|(name, _)| name == "operator_attention_critical_order_idx")
      .unwrap()
      .1
      .contains("(occurred_at DESC, id DESC) WHERE (source_kind = 'critical_system_condition'::text)")
  );
  assert!(
    definitions
      .iter()
      .find(|(name, _)| name == "operator_attention_active_critical_code_idx")
      .unwrap()
      .1
      .contains("UNIQUE INDEX")
  );
  let target = definitions
    .iter()
    .find(|(name, _)| name == "operator_attention_target_order_idx")
    .unwrap();
  assert!(target.1.contains("(target_kind, target_id, occurred_at DESC, id DESC)"));
  assert!(target.1.contains("build_failed"));
  assert!(target.1.contains("agent_unavailable"));
  assert!(target.1.contains("agent_pool_unavailable"));
  Ok(())
}

async fn operator_attention_index_definitions(pool: &sqlx::PgPool) -> Result<Vec<(String, String)>, sqlx::Error> {
  sqlx::query_as(
    "SELECT indexname, indexdef FROM pg_indexes \
     WHERE schemaname = 'public' AND indexname = ANY($1::text[]) ORDER BY indexname",
  )
  .bind(
    [
      "operator_attention_critical_order_idx",
      "operator_attention_active_critical_code_idx",
      "operator_attention_target_order_idx",
    ]
    .as_slice(),
  )
  .fetch_all(pool)
  .await
}

async fn verify_factory_schema(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
  let actual_tables: BTreeSet<String> = sqlx::query_scalar(
    "SELECT tablename FROM pg_tables \
     WHERE schemaname = 'public' AND tablename LIKE 'factory_%'",
  )
  .fetch_all(pool)
  .await?
  .into_iter()
  .collect();
  let expected_tables = FACTORY_TABLES
    .into_iter()
    .chain(FACTORY_RETENTION_TABLES)
    .map(str::to_owned)
    .collect();
  assert_eq!(actual_tables, expected_tables);

  let (is_deferrable, initially_deferred): (bool, bool) = sqlx::query_as(
    "SELECT condeferrable, condeferred FROM pg_constraint \
     WHERE conrelid = 'factory_configurations'::regclass \
       AND conname = 'factory_configurations_current_version_fkey'",
  )
  .fetch_one(pool)
  .await?;
  assert!(is_deferrable && initially_deferred);

  let missing_primary_keys: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM pg_tables AS tables \
     WHERE tables.schemaname = 'public' AND tables.tablename LIKE 'factory_%' \
       AND NOT EXISTS (\
         SELECT 1 FROM pg_constraint AS table_constraint \
         WHERE table_constraint.conrelid = format('public.%I', tables.tablename)::regclass \
           AND table_constraint.contype = 'p'\
       )",
  )
  .fetch_one(pool)
  .await?;
  assert_eq!(missing_primary_keys, 0, "every Factory record needs a stable identity");

  let cascading_foreign_keys: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM pg_constraint AS table_constraint \
     JOIN pg_class AS relation ON relation.oid = table_constraint.conrelid \
     JOIN pg_namespace AS namespace ON namespace.oid = relation.relnamespace \
     WHERE namespace.nspname = 'public' AND relation.relname LIKE 'factory_%' \
       AND table_constraint.contype = 'f' \
       AND (table_constraint.confupdtype <> 'a' OR table_constraint.confdeltype <> 'a')",
  )
  .fetch_one(pool)
  .await?;
  assert_eq!(
    cascading_foreign_keys, 0,
    "immutable Factory history must never cascade updates or deletes"
  );

  // Views derive projections and have no column constraints. Retained documents
  // may bound serialized text bytes or PostgreSQL's JSONB representation. The
  // discovery index inherits its bound from the canonical document only when
  // PostgreSQL enforces both exact projection equality and that document cap.
  let unbounded_documents: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM information_schema.columns AS column_definition \
     JOIN information_schema.tables AS table_definition USING (table_schema, table_name) \
     WHERE table_definition.table_type = 'BASE TABLE' \
       AND column_definition.table_schema = 'public' \
       AND column_definition.table_name LIKE 'factory_%' \
       AND column_definition.data_type = 'jsonb' \
       AND NOT EXISTS (\
         SELECT 1 FROM pg_constraint AS table_constraint \
         WHERE table_constraint.conrelid = format('public.%I', column_definition.table_name)::regclass \
           AND table_constraint.contype = 'c' \
           AND pg_get_constraintdef(table_constraint.oid) LIKE '%' || column_definition.column_name || '%' \
           AND pg_get_constraintdef(table_constraint.oid) LIKE '%jsonb_typeof%' \
           AND (pg_get_constraintdef(table_constraint.oid) LIKE '%octet_length%' \
             OR pg_get_constraintdef(table_constraint.oid) LIKE '%pg_column_size%' \
             OR (column_definition.table_name = 'factory_flow_records' \
               AND column_definition.column_name = 'discovery_facts' \
               AND table_constraint.conname = 'factory_flow_records_discovery_matches_document' \
               AND pg_get_constraintdef(table_constraint.oid) \
                 LIKE '%NOT (discovery_facts IS DISTINCT FROM%convert_from(document,%-> ''discovery''%' \
               AND EXISTS (\
                 SELECT 1 FROM pg_constraint AS document_constraint \
                 WHERE document_constraint.conrelid = table_constraint.conrelid \
                   AND document_constraint.contype = 'c' \
                   AND pg_get_constraintdef(document_constraint.oid) LIKE $1\
               )))\
       )",
  )
  .bind(format!(
    "%octet_length(document) <= {}%",
    octacity_server_factory::MAX_FLOW_DATA_BYTES
  ))
  .fetch_one(pool)
  .await?;
  assert_eq!(
    unbounded_documents, 0,
    "Factory JSON metadata must be typed and bounded"
  );

  let indexes: BTreeSet<String> = sqlx::query_scalar(
    "SELECT indexname FROM pg_indexes WHERE schemaname = 'public' \
     AND indexname = ANY($1::text[])",
  )
  .bind(
    [
      "factory_configurations_discovery_idx",
      "factory_node_attempts_diagnostics_idx",
      "factory_outbox_due_idx",
      "factory_outbox_operation_history_idx",
      "factory_run_claims_history_idx",
      "factory_runs_configuration_discovery_idx",
      "factory_runs_project_discovery_idx",
      "factory_runs_reconciliation_idx",
      "factory_runs_state_discovery_idx",
      "factory_node_attempt_completions_diagnostics_idx",
      "factory_work_envelopes_source_discovery_idx",
    ]
    .as_slice(),
  )
  .fetch_all(pool)
  .await?
  .into_iter()
  .collect();
  assert_eq!(indexes.len(), 11);
  Ok(())
}

async fn verify_factory_constraint_enforcement(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
  let project_id = uuid::Uuid::new_v4();
  let configuration_id = uuid::Uuid::new_v4();
  let mut transaction = pool.begin().await?;
  sqlx::query(
    "INSERT INTO projects (id, parent_id, name, version, created_at, updated_at) \
     VALUES ($1, NULL, $2, 1, now(), now())",
  )
  .bind(project_id)
  .bind(format!("factory-schema-{}", project_id.simple()))
  .execute(&mut *transaction)
  .await?;
  sqlx::query(
    "INSERT INTO factory_configurations (id, project_id, current_version, created_at, updated_at) \
     VALUES ($1, $2, 1, now(), now())",
  )
  .bind(configuration_id)
  .bind(project_id)
  .execute(&mut *transaction)
  .await?;
  sqlx::query(
    "INSERT INTO factory_configuration_versions \
       (factory_configuration_id, version, definition_digest, definition, enabled, published_at) \
     VALUES ($1, 1, $2, '{}', true, now())",
  )
  .bind(configuration_id)
  .bind(vec![1_u8; 32])
  .execute(&mut *transaction)
  .await?;
  transaction.commit().await?;

  let duplicate = sqlx::query(
    "INSERT INTO factory_configuration_versions \
       (factory_configuration_id, version, definition_digest, definition, enabled, published_at) \
     VALUES ($1, 1, $2, '{}', true, now())",
  )
  .bind(configuration_id)
  .bind(vec![2_u8; 32])
  .execute(pool)
  .await
  .unwrap_err();
  assert_eq!(
    duplicate.as_database_error().and_then(|error| error.code()).as_deref(),
    Some("23505")
  );

  let unbounded = sqlx::query(
    "INSERT INTO factory_configuration_versions \
       (factory_configuration_id, version, definition_digest, definition, enabled, published_at) \
     VALUES ($1, 2, $2, jsonb_build_object('payload', repeat('x', 1048577)), true, now())",
  )
  .bind(configuration_id)
  .bind(vec![3_u8; 32])
  .execute(pool)
  .await
  .unwrap_err();
  assert_eq!(
    unbounded.as_database_error().and_then(|error| error.code()).as_deref(),
    Some("23514")
  );

  let foreign_key = sqlx::query(
    "INSERT INTO factory_runs \
       (id, project_id, work_envelope_id, factory_configuration_id, factory_configuration_version, \
        state, version, subject_digest, flow_admission_limits, admitted_at, updated_at) \
     VALUES ($1, $2, $3, $4, 1, 'admitted', 1, $5, '{}'::jsonb, now(), now())",
  )
  .bind(uuid::Uuid::new_v4())
  .bind(project_id)
  .bind(uuid::Uuid::new_v4())
  .bind(configuration_id)
  .bind(vec![4_u8; 32])
  .execute(pool)
  .await
  .unwrap_err();
  assert_eq!(
    foreign_key
      .as_database_error()
      .and_then(|error| error.code())
      .as_deref(),
    Some("23503")
  );
  Ok(())
}

async fn verify_security_scoped_idempotency(pool: &sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
  let primary_key_columns: Vec<String> = sqlx::query_scalar(
    "SELECT key_column.column_name \
     FROM information_schema.table_constraints AS table_constraint \
     JOIN information_schema.key_column_usage AS key_column \
       ON key_column.constraint_schema = table_constraint.constraint_schema \
      AND key_column.constraint_name = table_constraint.constraint_name \
     WHERE table_constraint.table_schema = 'public' \
       AND table_constraint.table_name = 'idempotency_records' \
       AND table_constraint.constraint_type = 'PRIMARY KEY' \
     ORDER BY key_column.ordinal_position",
  )
  .fetch_all(pool)
  .await?;
  assert_eq!(primary_key_columns, ["scope", "security_scope", "idempotency_key"]);

  let trigger_deduplication_columns: Vec<String> = sqlx::query_scalar(
    "SELECT key_column.column_name \
     FROM information_schema.table_constraints AS table_constraint \
     JOIN information_schema.key_column_usage AS key_column \
       ON key_column.constraint_schema = table_constraint.constraint_schema \
      AND key_column.constraint_name = table_constraint.constraint_name \
     WHERE table_constraint.table_schema = 'public' \
       AND table_constraint.table_name = 'trigger_occurrences' \
       AND table_constraint.constraint_name = 'trigger_occurrences_deduplication_key' \
     ORDER BY key_column.ordinal_position",
  )
  .fetch_all(pool)
  .await?;
  assert_eq!(
    trigger_deduplication_columns,
    [
      "security_scope",
      "trigger_id",
      "trigger_version",
      "deduplication_identity"
    ]
  );

  let (nullable, default): (String, Option<String>) = sqlx::query_as(
    "SELECT is_nullable, column_default FROM information_schema.columns \
     WHERE table_schema = 'public' AND table_name = 'idempotency_records' AND column_name = 'security_scope'",
  )
  .fetch_one(pool)
  .await?;
  assert_eq!(nullable, "NO");
  assert!(default.is_some_and(|value| value.contains("trusted-network")));
  assert!(table_exists(pool, "idempotency_records").await?);
  assert!(
    sqlx::query_scalar::<_, bool>("SELECT to_regclass('idempotency_records_legacy_lookup_idx') IS NOT NULL")
      .fetch_one(pool)
      .await?
  );

  if column_exists(pool, "idempotency_records", "security_scope").await? {
    let legacy_scope: Option<String> =
      sqlx::query_scalar("SELECT security_scope FROM idempotency_records WHERE scope = $1 AND idempotency_key = $2")
        .bind(LEGACY_IDEMPOTENCY_SCOPE)
        .bind(LEGACY_IDEMPOTENCY_KEY)
        .fetch_optional(pool)
        .await?;
    if legacy_scope.is_some() {
      assert_eq!(legacy_scope.as_deref(), Some("trusted-network"));
    }
  }
  for (scope, key, expected_security_scope) in LEGACY_OPERATION_SCOPES {
    let actual: Option<String> =
      sqlx::query_scalar("SELECT security_scope FROM idempotency_records WHERE scope = $1 AND idempotency_key = $2")
        .bind(scope)
        .bind(key)
        .fetch_optional(pool)
        .await?;
    if actual.is_some() {
      assert_eq!(actual.as_deref(), Some(expected_security_scope));
    }
  }

  let old_writer_key = format!("old-writer-{}", uuid::Uuid::new_v4().simple());
  sqlx::query(
    "INSERT INTO idempotency_records (scope, idempotency_key, request_digest, outcome, created_at) \
     VALUES ('schema-contract', $1, $2, '{}', now())",
  )
  .bind(&old_writer_key)
  .bind(vec![8_u8; 32])
  .execute(pool)
  .await?;
  let default_scope: String = sqlx::query_scalar(
    "SELECT security_scope FROM idempotency_records WHERE scope = 'schema-contract' AND idempotency_key = $1",
  )
  .bind(&old_writer_key)
  .fetch_one(pool)
  .await?;
  assert_eq!(default_scope, "trusted-network");
  for invalid_scope in [String::new(), "UPPER".to_owned(), "x".repeat(129)] {
    let error = sqlx::query(
      "INSERT INTO idempotency_records \
         (scope, security_scope, idempotency_key, request_digest, outcome, created_at) \
       VALUES ('schema-contract', $1, $2, $3, '{}', now())",
    )
    .bind(invalid_scope)
    .bind(format!("invalid-scope-{}", uuid::Uuid::new_v4().simple()))
    .bind(vec![0_u8; 32])
    .execute(pool)
    .await
    .unwrap_err();
    assert_eq!(
      error.as_database_error().and_then(|error| error.code()).as_deref(),
      Some("23514")
    );
  }
  assert!(idempotency_shape_can_be_restored_without_merging(pool).await?);

  sqlx::query(
    "INSERT INTO idempotency_records \
       (scope, security_scope, idempotency_key, request_digest, outcome, created_at) \
     VALUES ('schema-contract', 'operator:second', $1, $2, '{}', now())",
  )
  .bind(&old_writer_key)
  .bind(vec![9_u8; 32])
  .execute(pool)
  .await?;
  assert!(!idempotency_shape_can_be_restored_without_merging(pool).await?);
  sqlx::query(
    "DELETE FROM idempotency_records \
     WHERE scope = 'schema-contract' AND idempotency_key = $1 AND security_scope = 'operator:second'",
  )
  .bind(&old_writer_key)
  .execute(pool)
  .await?;
  Ok(())
}

async fn idempotency_shape_can_be_restored_without_merging(pool: &sqlx::PgPool) -> Result<bool, sqlx::Error> {
  sqlx::query_scalar(
    "SELECT NOT EXISTS (\
       SELECT 1 FROM idempotency_records \
       GROUP BY scope, idempotency_key HAVING COUNT(*) > 1\
     )",
  )
  .fetch_one(pool)
  .await
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn unreleased_factory_schema_has_no_parallel_stage_execution_journal() {
  let database = TestDatabase::migrated().await;
  let obsolete: Vec<String> = sqlx::query_scalar(
    "SELECT table_name::text FROM information_schema.tables WHERE table_schema='public' AND table_name = ANY($1) ORDER BY table_name",
  )
  .bind(vec![
    "factory_stage_attempts", "factory_stage_attempt_completions", "factory_stage_handoffs",
    "factory_context_manifests", "factory_call_nodes", "factory_call_dependencies",
    "factory_call_stage_dependencies", "factory_call_completions", "factory_build_links",
    "factory_build_link_jobs", "factory_build_observations", "factory_changesets",
    "factory_evidence_manifests", "factory_evaluation_plans", "factory_assessments",
    "factory_decision_assessments", "factory_decisions", "factory_escalations",
    "factory_delivery_attempts", "factory_reporting_attempts",
  ])
  .fetch_all(&database.pool).await.unwrap();
  database.cleanup().await;
  assert!(
    obsolete.is_empty(),
    "stage-specific execution must use the common node journal: {obsolete:?}"
  );
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn poll_page_upgrade_preserves_legacy_progress_and_keeps_database_documents_bounded() {
  let database = TestDatabase::empty().await;
  MIGRATOR.run_to(61, &database.pool).await.unwrap();
  let project = uuid::Uuid::new_v4();
  let source = uuid::Uuid::new_v4();
  sqlx::query(
    "INSERT INTO projects(id,name,version,created_at,updated_at) VALUES ($1,'legacy poll fixture',1,now(),now())",
  )
  .bind(project)
  .execute(&database.pool)
  .await
  .unwrap();
  let legacy = serde_json::json!({"progress":{"window_start":0,"window_end":100,"continuation":"private-legacy-position","reconciliation":false}});
  sqlx::query("INSERT INTO factory_source_scans(kind,id,project_id,document) VALUES ('input',$1,$2,$3)")
    .bind(source)
    .bind(project)
    .bind(sqlx::types::Json(&legacy))
    .execute(&database.pool)
    .await
    .unwrap();
  octacity_server_store_postgres::migrate(&database.pool).await.unwrap();
  let restored: sqlx::types::Json<serde_json::Value> =
    sqlx::query_scalar("SELECT document FROM factory_source_scans WHERE id=$1")
      .bind(source)
      .fetch_one(&database.pool)
      .await
      .unwrap();
  assert_eq!(restored.0, legacy);
  for invalid in [
    serde_json::json!([]),
    serde_json::json!({"payload":"x".repeat(8421377)}),
  ] {
    let error = sqlx::query("UPDATE factory_source_scans SET document=$1 WHERE id=$2")
      .bind(sqlx::types::Json(invalid))
      .bind(source)
      .execute(&database.pool)
      .await
      .unwrap_err();
    assert_eq!(
      error.as_database_error().and_then(|e| e.code()).as_deref(),
      Some("23514")
    );
  }
  database.cleanup().await;
}
