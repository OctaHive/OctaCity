use async_trait::async_trait;
use octacity_server_domain::ProjectId;
use octacity_server_factory::{FactoryConfigurationId, FactoryConfigurationVersion};

use crate::{
  CreateFactoryConfiguration, FactoryConfigurationAvailability, FactoryConfigurationMutationOutcome,
  ManagementMutation, PublishedFactoryConfiguration, ReplaceFactoryConfiguration, ReplaceFactoryConfigurationError,
  ReplayFactoryConfigurationMutation, StoreError,
};

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
