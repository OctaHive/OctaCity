use std::sync::Arc;

use octacity_server_job::JobSpecSigner;
use sqlx::PgPool;

mod artifact_cache;
mod execution;
mod management;
mod trigger_webhook;

/// PostgreSQL adapter for ports that do not cross a JobSpec signing seam.
#[derive(Clone)]
pub struct PostgresStore {
  pool: PgPool,
}

impl PostgresStore {
  /// Creates infrastructure ports backed by a migrated PostgreSQL pool.
  #[must_use]
  pub const fn new(pool: PgPool) -> Self {
    Self { pool }
  }

  pub(crate) const fn pool(&self) -> &PgPool {
    &self.pool
  }
}

/// PostgreSQL adapter for mutations that cross a signed JobSpec seam.
#[derive(Clone)]
pub struct PostgresAuthoritativeStore {
  store: PostgresStore,
  job_spec_signer: Arc<JobSpecSigner>,
}

impl PostgresAuthoritativeStore {
  /// Creates an authoritative adapter with an explicit active signing key.
  #[must_use]
  pub fn new(pool: PgPool, job_spec_signer: Arc<JobSpecSigner>) -> Self {
    Self {
      store: PostgresStore::new(pool),
      job_spec_signer,
    }
  }
}
