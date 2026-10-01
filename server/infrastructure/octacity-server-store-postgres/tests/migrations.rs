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

const SNAPSHOT_PROJECT_ID: &str = "0199a6f4-d56c-7440-9aa2-6a320f862795";
const LEGACY_IDEMPOTENCY_SCOPE: &str = "schema-rehearsal";
const LEGACY_IDEMPOTENCY_KEY: &str = "legacy-writer";
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
async fn rehearses_forward_failure_and_previous_binary_snapshot_rollback() {
  let result = verify_schema_rehearsal().await;
  result.unwrap();
}

async fn verify_schema_rehearsal() -> Result<(), Box<dyn std::error::Error>> {
  assert_eq!(PREVIOUS_BINARY_SCHEMA_VERSION + 1, current_schema_version());

  let mut previous = TestDatabase::empty().await;
  MIGRATOR.run_to(PREVIOUS_BINARY_SCHEMA_VERSION, &previous.pool).await?;
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
  assert!(!column_exists(&rollback.pool, "idempotency_records", "security_scope").await?);
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
  for (scope, key, _) in LEGACY_OPERATION_SCOPES {
    sqlx::query(
      "INSERT INTO idempotency_records (scope, idempotency_key, request_digest, outcome, created_at) \
       VALUES ($1, $2, $3, '{\"schema_version\": 1}', now())",
    )
    .bind(scope)
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
