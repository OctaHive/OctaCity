mod support;

use std::collections::BTreeSet;

use support::TestDatabase;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn migrates_an_empty_database_and_is_reentrant() {
  let database = TestDatabase::migrated().await;
  let result = verify_migration(&database.pool).await;
  database.cleanup().await;
  result.unwrap();
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
