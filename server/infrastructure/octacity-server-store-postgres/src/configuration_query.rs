use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationVersion, EntityKind, RepositoryId, RepositoryVersion,
};
use octacity_server_store::{PublishedBuildConfiguration, PublishedRepository, StoreError, StoreOperation};

use crate::{
  configuration_row::{BuildConfigurationRow, RepositoryRow},
  database::{number, unavailable},
};

pub(crate) async fn repository(
  pool: &sqlx::PgPool,
  repository_id: RepositoryId,
  version: RepositoryVersion,
) -> Result<PublishedRepository, StoreError> {
  let version = number(version.get(), StoreOperation::ReadRepositoryVersion)?;
  sqlx::query_as::<_, RepositoryRow>(
    "SELECT repository.id, repository.project_id, repository.name, version.version, \
       version.vcs_integration_id, version.repository_locator, version.selection_policy, \
       FLOOR(EXTRACT(EPOCH FROM version.published_at) * 1000)::BIGINT AS published_at_millis \
     FROM repositories AS repository \
     JOIN repository_versions AS version ON version.repository_id = repository.id \
     WHERE repository.id = $1 AND version.version = $2",
  )
  .bind(repository_id.as_uuid())
  .bind(version)
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Repository,
  })?
  .try_into()
}

pub(crate) async fn build_configuration(
  pool: &sqlx::PgPool,
  configuration_id: BuildConfigurationId,
  version: BuildConfigurationVersion,
) -> Result<PublishedBuildConfiguration, StoreError> {
  let version = number(version.get(), StoreOperation::ReadBuildConfigurationVersion)?;
  sqlx::query_as::<_, BuildConfigurationRow>(
    "SELECT configuration.id, configuration.project_id, configuration.name, version.version, version.enabled, \
       version.repository_id, version.repository_version, version.pipeline_id, version.pipeline_version, \
       version.configuration_snapshot, \
       FLOOR(EXTRACT(EPOCH FROM version.published_at) * 1000)::BIGINT AS published_at_millis \
     FROM build_configurations AS configuration \
     JOIN build_configuration_versions AS version ON version.build_configuration_id = configuration.id \
     WHERE configuration.id = $1 AND version.version = $2",
  )
  .bind(configuration_id.as_uuid())
  .bind(version)
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Configuration,
  })?
  .try_into()
}
