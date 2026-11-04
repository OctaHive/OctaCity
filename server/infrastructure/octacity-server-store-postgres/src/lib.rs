//! PostgreSQL persistence adapter.
//!
//! Migrations, SQL rows, transaction mechanics, locks, and conversion to the
//! backend-neutral store contract are isolated here.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod artifacts;
mod cicd;
mod database;
mod discovery;
mod factory;
mod integrations;
mod issues;
mod migration;
mod mutation;
mod operations;
mod projects;
mod read_visibility;
mod store;
mod telemetry;

use sqlx::PgPool;

pub use cicd::builds::ready_queue_notification::ready_job_count;
pub use migration::{
  MIGRATOR, MigrationStatus, PREVIOUS_BINARY_SCHEMA_VERSION, current_schema_version, migrate, migration_status,
  run_migrator,
};
pub use operations::log_search::{LogSearchRebuildSummary, PostgresLogSearchIndex};
pub use store::{FactoryCapabilitySource, PostgresAuthoritativeStore, PostgresStore};

/// PostgreSQL notification channel emitted after a ready-queue transaction commits.
pub const READY_JOB_NOTIFICATION_CHANNEL: &str = "octacity_ready_jobs";

/// Checks that PostgreSQL can execute a trivial query through the pool.
pub async fn health_check(pool: &PgPool) -> bool {
  sqlx::query_scalar::<_, i32>("SELECT 1")
    .fetch_one(pool)
    .await
    .is_ok_and(|value| value == 1)
}
