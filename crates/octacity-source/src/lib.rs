//! Discovers trusted source plugins and supervises source materialization.
//!
//! Registry verification and process hosting live together because they form
//! one trust boundary: only an operator-installed and digest-verified plugin
//! may be started. Provider behavior remains outside this crate behind the
//! versioned `octacity-source-plugin` process protocol.

mod host;
mod registry;

use std::time::Duration;

use async_trait::async_trait;
use octacity_protocol::SourceSpec;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

pub use host::{MaterializedSource, SourceHostError, SourceMaterializationRequest};
pub use registry::{InstalledSourcePlugin, RegistryError, SourcePluginRegistry};

/// Source acquisition failure independent of a particular VCS provider.
#[derive(Debug, Error)]
pub enum SourceError {
  #[error(transparent)]
  /// No trusted installed plugin satisfies the signed source requirement.
  Registry(#[from] RegistryError),
  #[error(transparent)]
  /// The selected plugin could not complete the materialization protocol.
  Host(#[from] SourceHostError),
}

/// Materializes the exact source revision selected by a signed job.
#[async_trait]
pub trait SourceMaterializer: Send + Sync {
  /// Materializes one signed source requirement into its prepared destination.
  async fn materialize(
    &self,
    requirement: &SourceSpec,
    request: SourceMaterializationRequest,
    operation_timeout: Duration,
    cancellation_grace: Duration,
    cancellation: CancellationToken,
  ) -> Result<MaterializedSource, SourceError>;
}

#[async_trait]
impl SourceMaterializer for SourcePluginRegistry {
  async fn materialize(
    &self,
    requirement: &SourceSpec,
    request: SourceMaterializationRequest,
    operation_timeout: Duration,
    cancellation_grace: Duration,
    cancellation: CancellationToken,
  ) -> Result<MaterializedSource, SourceError> {
    Ok(
      self
        .resolve(requirement)?
        .materialize(request, operation_timeout, cancellation_grace, cancellation)
        .await?,
    )
  }
}
