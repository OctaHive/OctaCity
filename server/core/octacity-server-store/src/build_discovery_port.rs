use async_trait::async_trait;

use crate::{ListProjectBuilds, ProjectBuildPage, StoreError};

/// Read-only discovery port for Builds owned by one Project.
#[async_trait]
pub trait BuildDiscoveryStore: Send + Sync {
  /// Lists visible matching Builds in deterministic newest-first order.
  ///
  /// Returns [`StoreError::NotFound`] when the Project or selected Build
  /// Configuration does not exist in that Project.
  async fn list_project_builds(&self, request: ListProjectBuilds) -> Result<ProjectBuildPage, StoreError>;
}
