use sqlx::{
  PgPool,
  migrate::{MigrateError, Migrator},
};

/// Last schema version supported by the immediately preceding server binary.
///
/// A rollback to that binary requires restoring a database snapshot captured
/// at this exact version. A previous binary is never allowed to open a database
/// containing migrations it does not know.
pub const PREVIOUS_BINARY_SCHEMA_VERSION: i64 = 46;

/// Ordered embedded forward migrations for the authoritative PostgreSQL store.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

/// Returns the newest embedded forward-migration version.
pub fn current_schema_version() -> i64 {
  MIGRATOR
    .iter()
    .filter(|migration| !migration.migration_type.is_down_migration())
    .map(|migration| migration.version)
    .max()
    .unwrap_or(0)
}

/// Applies every pending forward migration to one PostgreSQL database.
pub async fn migrate(pool: &PgPool) -> Result<(), MigrateError> {
  run_migrator(pool, &MIGRATOR).await?;
  crate::issue_reporter::recover_publications(pool).await.map_err(|_| {
    MigrateError::Execute(sqlx::Error::Protocol(
      "Issue Reporter publication recovery failed".into(),
    ))
  })
}

/// Runs an explicit migration set and discards a failed database session.
///
/// SQLx uses a session-level PostgreSQL advisory lock while migrating. A
/// failed run does not reach its normal unlock path, so returning that session
/// to a pool could block every later migration attempt. Closing the failed
/// session releases the lock. Normal server startup should call [`migrate`];
/// this entry point also supports release migration rehearsals.
pub async fn run_migrator(pool: &PgPool, migrator: &Migrator) -> Result<(), MigrateError> {
  let mut connection = pool.acquire().await?;
  let result = migrator.run_direct(None, &mut *connection, false).await;
  if result.is_err() {
    let _ = connection.close().await;
  }
  result
}

/// Read-only compatibility and completion state of database migration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MigrationStatus {
  /// Forward migrations and retained publication recovery are complete.
  Current,
  /// SQL history or its recoverable retained-publication phase is incomplete.
  Pending,
  /// Applied history is dirty, newer, discontinuous, or has a changed checksum.
  Incompatible,
}

/// Inspects migration compatibility without taking the migrator advisory lock.
///
/// Readiness can call this frequently. The comparatively expensive, locking
/// migration path is needed only while schema or publication recovery is pending. Applied
/// migrations must be an exact checksum-matching prefix: a newer, reordered,
/// or gapped history is incompatible with this binary.
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
  if applied.len() > expected.len() {
    return Ok(MigrationStatus::Incompatible);
  }

  for ((version, _, checksum), migration) in applied.iter().zip(&expected) {
    if *version != migration.version || migration.checksum.as_ref() != checksum {
      return Ok(MigrationStatus::Incompatible);
    }
  }

  Ok(
    if applied.len() == expected.len() && !crate::issue_reporter::has_pending_publications(pool).await? {
      MigrationStatus::Current
    } else {
      MigrationStatus::Pending
    },
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn rollback_version_matches_the_last_released_binary_schema() {
    assert_eq!(PREVIOUS_BINARY_SCHEMA_VERSION, 46);
    assert!(current_schema_version() > PREVIOUS_BINARY_SCHEMA_VERSION);
  }
}
