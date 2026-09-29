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
  assert!(column_exists(&previous.pool, "job_completions", "execution").await?);
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
  assert!(!column_exists(&rollback.pool, "job_completions", "execution").await?);
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
  Ok(())
}
