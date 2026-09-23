//! PostgreSQL persistence adapter.
//!
//! Migrations, SQL rows, transaction mechanics, locks, and conversion to the
//! backend-neutral store contract are isolated here.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod accept_trigger;
mod agent_credential_auth;
mod agent_credential_revocation;
mod agent_enrollment;
mod agent_mutation;
mod agent_query;
mod agent_registration;
mod agent_row;
mod attempt_materialization;
mod build_configuration_mutation;
mod build_query;
mod cancel_build;
mod configuration_query;
mod configuration_row;
mod database;
mod definition_mutation;
mod external_trigger;
mod internal_trigger;
mod job_claim;
mod job_completion;
mod job_event_query;
mod job_events;
mod lease;
mod lease_heartbeat;
mod lease_recovery;
mod mutation;
mod pipeline_mutation;
mod pipeline_query;
mod pipeline_row;
mod pool_mutation;
mod pool_query;
mod pool_row;
mod project_mutation;
mod project_policy_query;
mod project_query;
mod project_row;
mod ready_queue_notification;
mod repository_mutation;
mod retry_build;
mod schedule;
mod state;
mod store;
mod trigger_evaluation;
mod trigger_query;

use sqlx::{PgPool, migrate::MigrateError};

pub use store::{PostgresAuthoritativeStore, PostgresStore};

/// PostgreSQL notification channel emitted after a ready-queue transaction commits.
pub const READY_JOB_NOTIFICATION_CHANNEL: &str = "octacity_ready_jobs";

/// Ordered embedded forward migrations for the authoritative PostgreSQL store.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

/// Applies every pending forward migration to one PostgreSQL database.
pub async fn migrate(pool: &PgPool) -> Result<(), MigrateError> {
  MIGRATOR.run(pool).await
}

/// Read-only compatibility state of the database migration history.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MigrationStatus {
  /// Every embedded forward migration is applied with its original checksum.
  Current,
  /// The migration table is absent or at least one embedded migration is pending.
  Pending,
  /// Applied history is dirty, unknown to this binary, or has a changed checksum.
  Incompatible,
}

/// Inspects migration compatibility without taking the migrator advisory lock.
///
/// Readiness can call this frequently. The comparatively expensive, locking
/// migration path is needed only while the schema is actually behind.
pub async fn migration_status(pool: &PgPool) -> Result<MigrationStatus, sqlx::Error> {
  let table_exists = sqlx::query_scalar::<_, bool>("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
    .fetch_one(pool)
    .await?;
  if !table_exists {
    return Ok(MigrationStatus::Pending);
  }

  let applied = sqlx::query_as::<_, (i64, bool, Vec<u8>)>(
    "SELECT version, success, checksum FROM _sqlx_migrations ORDER BY version",
  )
  .fetch_all(pool)
  .await?;
  if applied.iter().any(|(_, success, _)| !success) {
    return Ok(MigrationStatus::Incompatible);
  }

  let expected: Vec<_> = MIGRATOR
    .iter()
    .filter(|migration| !migration.migration_type.is_down_migration())
    .collect();
  for (version, _, checksum) in &applied {
    let Some(migration) = expected.iter().find(|migration| migration.version == *version) else {
      return Ok(MigrationStatus::Incompatible);
    };
    if migration.checksum.as_ref() != checksum {
      return Ok(MigrationStatus::Incompatible);
    }
  }

  Ok(if applied.len() == expected.len() {
    MigrationStatus::Current
  } else {
    MigrationStatus::Pending
  })
}

/// Checks that PostgreSQL can execute a trivial query through the pool.
pub async fn health_check(pool: &PgPool) -> bool {
  sqlx::query_scalar::<_, i32>("SELECT 1")
    .fetch_one(pool)
    .await
    .is_ok_and(|value| value == 1)
}
