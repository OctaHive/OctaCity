use async_trait::async_trait;

use crate::{
  CurrentBuildConfigurationPage, CurrentPipelinePage, CurrentRepositoryPage, CurrentTriggerDefinitionPage,
  ListProjectBuildConfigurations, ListProjectPipelines, ListProjectRepositories, ListProjectTriggerDefinitions,
  StoreError,
};

/// Read-only discovery port for current Pipeline definitions.
#[async_trait]
pub trait PipelineDiscoveryStore: Send + Sync {
  /// Lists visible current Pipeline versions owned by one Project.
  ///
  /// Returns [`StoreError::NotFound`] when the Project does not exist.
  async fn list_project_pipelines(&self, request: ListProjectPipelines) -> Result<CurrentPipelinePage, StoreError>;
}

/// Read-only discovery port for current Repository and Build Configuration definitions.
#[async_trait]
pub trait ConfigurationDiscoveryStore: Send + Sync {
  /// Lists visible current Repository versions owned by one Project.
  ///
  /// Returns [`StoreError::NotFound`] when the Project does not exist.
  async fn list_project_repositories(
    &self,
    request: ListProjectRepositories,
  ) -> Result<CurrentRepositoryPage, StoreError>;

  /// Lists visible current Build Configuration versions owned by one Project.
  ///
  /// Returns [`StoreError::NotFound`] when the Project does not exist.
  async fn list_project_build_configurations(
    &self,
    request: ListProjectBuildConfigurations,
  ) -> Result<CurrentBuildConfigurationPage, StoreError>;
}

/// Read-only discovery port for current manual, scheduled, and internal Trigger definitions.
#[async_trait]
pub trait TriggerDefinitionDiscoveryStore: Send + Sync {
  /// Lists visible credential-free current Trigger definitions owned by one Project.
  ///
  /// Returns [`StoreError::NotFound`] when the Project does not exist.
  async fn list_project_trigger_definitions(
    &self,
    request: ListProjectTriggerDefinitions,
  ) -> Result<CurrentTriggerDefinitionPage, StoreError>;
}
