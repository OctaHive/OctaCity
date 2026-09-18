use async_trait::async_trait;
use octacity_server_domain::{BuildConfigurationId, BuildConfigurationVersion, RepositoryId, RepositoryVersion};

use crate::{
  BuildConfigurationMutationOutcome, CreateBuildConfiguration, CreateRepository, PublishBuildConfigurationVersion,
  PublishRepositoryVersion, PublishedBuildConfiguration, PublishedRepository, RepositoryMutationOutcome, StoreError,
};

/// Backend-neutral append-only Repository and Build Configuration operations.
///
/// Every mutation is one atomic use case that commits its immutable version,
/// idempotency result, audit fact, and outbox record together. Published
/// versions have no update or delete operation.
#[async_trait]
pub trait ConfigurationStore: Send + Sync {
  /// Creates one Repository identity together with version one.
  async fn create_repository(&self, request: CreateRepository) -> Result<RepositoryMutationOutcome, StoreError>;

  /// Appends exactly the next Repository version.
  async fn publish_repository_version(
    &self,
    request: PublishRepositoryVersion,
  ) -> Result<RepositoryMutationOutcome, StoreError>;

  /// Reads one exact immutable Repository version.
  async fn repository_version(
    &self,
    repository_id: RepositoryId,
    version: RepositoryVersion,
  ) -> Result<PublishedRepository, StoreError>;

  /// Creates one Build Configuration identity together with version one.
  async fn create_build_configuration(
    &self,
    request: CreateBuildConfiguration,
  ) -> Result<BuildConfigurationMutationOutcome, StoreError>;

  /// Appends exactly the next Build Configuration version.
  async fn publish_build_configuration_version(
    &self,
    request: PublishBuildConfigurationVersion,
  ) -> Result<BuildConfigurationMutationOutcome, StoreError>;

  /// Reads one exact immutable Build Configuration version.
  async fn build_configuration_version(
    &self,
    configuration_id: BuildConfigurationId,
    version: BuildConfigurationVersion,
  ) -> Result<PublishedBuildConfiguration, StoreError>;
}
