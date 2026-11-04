use async_trait::async_trait;
use octacity_server_domain::ProjectId;
use octacity_server_factory::{FactoryConfigurationId, FactoryConfigurationVersion};

use crate::{
  CreateFactoryConfiguration, FactoryConfigurationAvailability, FactoryConfigurationMutationOutcome,
  ManagementMutation, PublishedFactoryConfiguration, ReplaceFactoryConfiguration, ReplaceFactoryConfigurationError,
  ReplayFactoryConfigurationMutation, StoreError,
};

/// Deployment-owned source of exact Factory Configuration choices.
///
/// PostgreSQL owns immutable published definitions, while selectable adapter,
/// model, policy, and enforcement capabilities are runtime inventory. Keeping
/// this source explicit prevents stale capability snapshots from becoming
/// durable authority.
#[async_trait]
pub trait FactoryCapabilitySource: Send + Sync {
  /// Reads the current trusted availability for one existing Project.
  async fn availability(&self, project_id: ProjectId) -> Result<FactoryConfigurationAvailability, StoreError>;
  /// Explicit inventory refresh; ordinary availability reads never perform discovery.
  async fn refresh_role_models(
    &self,
    _project_id: ProjectId,
    _alias: &octacity_server_factory::FactoryKey,
  ) -> Result<(), StoreError> {
    Err(StoreError::Unavailable)
  }
}

/// Backend-neutral append-only Factory Configuration operations.
///
/// Mutations atomically commit the immutable version, idempotency result,
/// audit fact, and outbox record. Capability choices originate from this
/// trusted port rather than from management clients.
#[async_trait]
pub trait FactoryConfigurationStore: Send + Sync {
  /// Returns an exact prior mutation result before mutable capability lookup.
  async fn replay_factory_configuration_mutation(
    &self,
    request: ManagementMutation<ReplayFactoryConfigurationMutation>,
  ) -> Result<Option<FactoryConfigurationMutationOutcome>, StoreError>;

  /// Reads current trusted capability choices for one existing Project.
  async fn factory_configuration_availability(
    &self,
    project_id: ProjectId,
  ) -> Result<FactoryConfigurationAvailability, StoreError>;

  /// Independent installed authority for Runtime administration, before Project settings narrow choices.
  /// Adapters without configurable Runtime settings preserve their existing inventory contract.
  async fn factory_configuration_installation(
    &self,
    project_id: ProjectId,
  ) -> Result<FactoryConfigurationAvailability, StoreError> {
    self.factory_configuration_availability(project_id).await
  }

  /// Refreshes one installed plugin's advisory catalog, without publishing a definition.
  async fn refresh_factory_role_models(
    &self,
    _project: ProjectId,
    _alias: &octacity_server_factory::FactoryKey,
  ) -> Result<(), StoreError> {
    Err(StoreError::Unavailable)
  }

  /// Creates one Factory Configuration identity together with version one.
  async fn create_factory_configuration(
    &self,
    request: ManagementMutation<CreateFactoryConfiguration>,
  ) -> Result<FactoryConfigurationMutationOutcome, StoreError>;

  /// Appends exactly the next immutable version using a strong precondition.
  async fn replace_factory_configuration(
    &self,
    request: ManagementMutation<ReplaceFactoryConfiguration>,
  ) -> Result<FactoryConfigurationMutationOutcome, ReplaceFactoryConfigurationError>;

  /// Reads one exact immutable Factory Configuration version.
  async fn factory_configuration_version(
    &self,
    id: FactoryConfigurationId,
    version: FactoryConfigurationVersion,
  ) -> Result<PublishedFactoryConfiguration, StoreError>;

  /// Reads the current immutable Factory Configuration version.
  async fn current_factory_configuration(
    &self,
    id: FactoryConfigurationId,
  ) -> Result<PublishedFactoryConfiguration, StoreError>;
}
