use async_trait::async_trait;
use octacity_server_factory::FactoryRunId;

use crate::{
  CurrentFactoryConfigurationPage, FactoryRunListVisibility, FactoryRunPage, FactoryRunSummary, ListFactoryRuns,
  ListProjectFactoryConfigurations, StoreError,
};

/// Backend-neutral authorization-safe Factory management discovery.
#[async_trait]
pub trait FactoryDiscoveryStore: Send + Sync {
  /// Lists current immutable Factory Configurations owned by one Project.
  async fn list_project_factory_configurations(
    &self,
    request: ListProjectFactoryConfigurations,
  ) -> Result<CurrentFactoryConfigurationPage, StoreError>;

  /// Lists Project-scoped or global current Factory Runs.
  async fn list_factory_runs(&self, request: ListFactoryRuns) -> Result<FactoryRunPage, StoreError>;

  /// Reads one visible current Factory Run summary.
  async fn factory_run_summary(
    &self,
    run_id: FactoryRunId,
    visibility: FactoryRunListVisibility,
  ) -> Result<FactoryRunSummary, StoreError>;
}
