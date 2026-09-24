use octacity_server_domain::ProjectId;
use octacity_server_store_postgres::{LogSearchRebuildSummary, MigrationStatus, PostgresLogSearchIndex};
use thiserror::Error;

use super::assembly::postgres_pool;
use crate::{RuntimeAssemblyError, ServerConfig};

/// Starts a durable, bounded Build-log search replay for one Project.
///
/// The command only resets derived PostgreSQL state and requeues existing
/// authoritative work. A running server performs object reads and projection
/// writes through its normal bounded log-index worker.
pub async fn rebuild_log_search(
  config: &ServerConfig,
  project_id: ProjectId,
) -> Result<LogSearchRebuildSummary, LogSearchMaintenanceError> {
  let (pool, _database_url) = postgres_pool(config).await?;
  match octacity_server_store_postgres::migration_status(&pool).await? {
    MigrationStatus::Current => {}
    MigrationStatus::Pending => return Err(LogSearchMaintenanceError::MigrationsPending),
    MigrationStatus::Incompatible => return Err(LogSearchMaintenanceError::MigrationsIncompatible),
  }
  PostgresLogSearchIndex::new(pool)
    .start_rebuild(project_id)
    .await
    .map_err(LogSearchMaintenanceError::Projection)
}

/// Failure to prepare a PostgreSQL Build-log projection rebuild.
#[derive(Debug, Error)]
pub enum LogSearchMaintenanceError {
  /// PostgreSQL configuration or credential loading failed.
  #[error("failed to assemble PostgreSQL access: {0}")]
  Assembly(#[from] RuntimeAssemblyError),
  /// PostgreSQL could not be reached or inspected.
  #[error("failed to inspect PostgreSQL migration state: {0}")]
  Database(#[from] sqlx::Error),
  /// The schema must be migrated by a normal server startup first.
  #[error("PostgreSQL migrations are pending; start the server before requesting a rebuild")]
  MigrationsPending,
  /// Applied migrations do not match this binary.
  #[error("PostgreSQL migration history is incompatible with this binary")]
  MigrationsIncompatible,
  /// The projection could not be reset and requeued atomically.
  #[error("failed to prepare the Build-log search rebuild: {0}")]
  Projection(octacity_server_store::LogSearchError),
}
