use octacity_server_application::{RestoreReconciler, RestoreReconciliationSummary};
use octacity_server_domain::ProjectId;
use octacity_server_store::MAX_RESTORE_RECONCILIATION_BATCH_SIZE;
use octacity_server_store_postgres::{LogSearchRebuildSummary, MigrationStatus, PostgresLogSearchIndex, PostgresStore};
use thiserror::Error;

use super::assembly::{RuntimeResources, postgres_pool};
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

/// Reconciles one quiesced restored PostgreSQL/object-store consistency unit.
///
/// This command loads and validates all configured credential material but
/// binds no listener. Operators must keep the deployment offline until it
/// succeeds, then start the normal server process from the same configuration.
pub async fn reconcile_restored_state(
  config: &ServerConfig,
  rebuild_search: bool,
) -> Result<RestoredStateSummary, RestoreMaintenanceError> {
  let resources = RuntimeResources::from_config(config).await?;
  require_current_migrations(&resources.postgres).await?;
  let inventory = std::sync::Arc::new(PostgresStore::new(resources.postgres.clone()));
  let reconciler = RestoreReconciler::new(
    inventory,
    resources.object_storage,
    MAX_RESTORE_RECONCILIATION_BATCH_SIZE,
  )?;
  let objects = reconciler.reconcile().await?;
  let rebuilt_search_projects = if rebuild_search {
    PostgresLogSearchIndex::new(resources.postgres)
      .start_rebuild_all()
      .await?
  } else {
    0
  };
  Ok(RestoredStateSummary {
    objects,
    rebuilt_search_projects,
  })
}

async fn require_current_migrations(pool: &sqlx::PgPool) -> Result<(), RestoreMaintenanceError> {
  match octacity_server_store_postgres::migration_status(pool).await? {
    MigrationStatus::Current => Ok(()),
    MigrationStatus::Pending => Err(RestoreMaintenanceError::MigrationsPending),
    MigrationStatus::Incompatible => Err(RestoreMaintenanceError::MigrationsIncompatible),
  }
}

/// Result retained as evidence for one restored consistency unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RestoredStateSummary {
  /// Independently verified object counts by logical class.
  pub objects: RestoreReconciliationSummary,
  /// Number of Project search projections reset for durable replay.
  pub rebuilt_search_projects: u64,
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

/// Failure to validate a restored deployment before it binds listeners.
#[derive(Debug, Error)]
pub enum RestoreMaintenanceError {
  /// Configuration or protected credential loading failed.
  #[error("failed to assemble restored server dependencies: {0}")]
  Assembly(#[from] RuntimeAssemblyError),
  /// PostgreSQL could not be reached or inspected.
  #[error("failed to inspect restored PostgreSQL migration state: {0}")]
  Database(#[from] sqlx::Error),
  /// The restored schema must match the selected server before reconciliation.
  #[error("restored PostgreSQL migrations are pending")]
  MigrationsPending,
  /// The restored schema history is not compatible with the selected server.
  #[error("restored PostgreSQL migration history is incompatible")]
  MigrationsIncompatible,
  /// One authoritative object reference could not be independently verified.
  #[error("restored object inventory failed reconciliation: {0}")]
  Reconciliation(#[from] octacity_server_application::RestoreReconciliationError),
  /// The optional derived search projection rebuild could not be queued.
  #[error("failed to prepare restored Build-log search projection: {0}")]
  Projection(#[from] octacity_server_store::LogSearchError),
}
