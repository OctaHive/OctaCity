use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::ProjectId;
use octacity_server_job::JobSpecSigner;
use octacity_server_store::{FactoryConfigurationAvailability, StoreError};
use sqlx::PgPool;

mod artifact_cache;
mod execution;
mod management;
mod trigger_webhook;

/// PostgreSQL adapter for ports that do not cross a JobSpec signing seam.
#[derive(Clone)]
pub struct PostgresStore {
  pool: PgPool,
  factory_capabilities: Arc<dyn FactoryCapabilitySource>,
}

pub use octacity_server_store::FactoryCapabilitySource;

struct DisabledFactoryCapabilities;

#[async_trait]
impl FactoryCapabilitySource for DisabledFactoryCapabilities {
  async fn availability(&self, _project_id: ProjectId) -> Result<FactoryConfigurationAvailability, StoreError> {
    Ok(FactoryConfigurationAvailability::Disabled)
  }
}

impl PostgresStore {
  /// Creates infrastructure ports backed by a migrated PostgreSQL pool.
  #[must_use]
  pub fn new(pool: PgPool) -> Self {
    Self {
      pool,
      factory_capabilities: Arc::new(DisabledFactoryCapabilities),
    }
  }

  /// Creates a store with an explicit deployment capability source.
  #[must_use]
  pub fn with_factory_capabilities(pool: PgPool, factory_capabilities: Arc<dyn FactoryCapabilitySource>) -> Self {
    Self {
      pool,
      factory_capabilities,
    }
  }

  /// Binds an existing runtime store to its installed authority, preserving the
  /// database pool and sharing subsequent catalog refreshes with management.
  #[must_use]
  pub fn with_factory_capability_source(mut self, source: Arc<dyn FactoryCapabilitySource>) -> Self {
    self.factory_capabilities = source;
    self
  }

  pub(crate) const fn pool(&self) -> &PgPool {
    &self.pool
  }

  pub(crate) fn factory_capabilities(&self) -> &dyn FactoryCapabilitySource {
    self.factory_capabilities.as_ref()
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
