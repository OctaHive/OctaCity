use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationName, BuildConfigurationVersion, IntegrationId, PipelineId, PipelineVersion,
  ProjectId, RepositoryId, RepositoryName, RepositoryVersion, Timestamp,
};
use octacity_server_store::{
  BuildConfigurationDefinition, PublishedBuildConfiguration, PublishedRepository, RepositoryDefinition,
  RepositorySelectionPolicy, StoreError,
};
use serde_json::Value;
use sqlx::{FromRow, types::Json};
use uuid::Uuid;

#[derive(FromRow)]
pub(crate) struct RepositoryRow {
  pub(crate) id: Uuid,
  pub(crate) project_id: Uuid,
  pub(crate) name: String,
  pub(crate) version: i64,
  pub(crate) vcs_integration_id: Uuid,
  pub(crate) repository_locator: String,
  pub(crate) selection_policy: Json<Value>,
  pub(crate) published_at_millis: i64,
}

impl TryFrom<RepositoryRow> for PublishedRepository {
  type Error = StoreError;

  fn try_from(row: RepositoryRow) -> Result<Self, Self::Error> {
    let definition = RepositoryDefinition {
      vcs_integration_id: IntegrationId::from_uuid(row.vcs_integration_id).map_err(|_| StoreError::Unavailable)?,
      repository_locator: row.repository_locator,
      selection: serde_json::from_value::<RepositorySelectionPolicy>(row.selection_policy.0)
        .map_err(|_| StoreError::Unavailable)?,
    };
    definition.validate().map_err(|_| StoreError::Unavailable)?;
    Ok(Self {
      id: RepositoryId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?,
      project_id: ProjectId::from_uuid(row.project_id).map_err(|_| StoreError::Unavailable)?,
      name: RepositoryName::new(row.name).map_err(|_| StoreError::Unavailable)?,
      version: RepositoryVersion::new(u64::try_from(row.version).map_err(|_| StoreError::Unavailable)?)
        .map_err(|_| StoreError::Unavailable)?,
      definition,
      published_at: Timestamp::from_unix_millis(row.published_at_millis).map_err(|_| StoreError::Unavailable)?,
    })
  }
}

#[derive(FromRow)]
pub(crate) struct BuildConfigurationRow {
  pub(crate) id: Uuid,
  pub(crate) project_id: Uuid,
  pub(crate) name: String,
  pub(crate) version: i64,
  pub(crate) enabled: bool,
  pub(crate) repository_id: Uuid,
  pub(crate) repository_version: i64,
  pub(crate) pipeline_id: Uuid,
  pub(crate) pipeline_version: i64,
  pub(crate) configuration_snapshot: Json<Value>,
  pub(crate) published_at_millis: i64,
}

impl TryFrom<BuildConfigurationRow> for PublishedBuildConfiguration {
  type Error = StoreError;

  fn try_from(row: BuildConfigurationRow) -> Result<Self, Self::Error> {
    let definition: BuildConfigurationDefinition =
      serde_json::from_value(row.configuration_snapshot.0).map_err(|_| StoreError::Unavailable)?;
    definition.validate().map_err(|_| StoreError::Unavailable)?;
    let repository_id = RepositoryId::from_uuid(row.repository_id).map_err(|_| StoreError::Unavailable)?;
    let repository_version =
      RepositoryVersion::new(u64::try_from(row.repository_version).map_err(|_| StoreError::Unavailable)?)
        .map_err(|_| StoreError::Unavailable)?;
    let pipeline_id = PipelineId::from_uuid(row.pipeline_id).map_err(|_| StoreError::Unavailable)?;
    let pipeline_version =
      PipelineVersion::new(u64::try_from(row.pipeline_version).map_err(|_| StoreError::Unavailable)?)
        .map_err(|_| StoreError::Unavailable)?;
    if definition.enabled != row.enabled
      || definition.repository_id != repository_id
      || definition.repository_version != repository_version
      || definition.pipeline_id != pipeline_id
      || definition.pipeline_version != pipeline_version
    {
      return Err(StoreError::Unavailable);
    }
    Ok(Self {
      id: BuildConfigurationId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?,
      project_id: ProjectId::from_uuid(row.project_id).map_err(|_| StoreError::Unavailable)?,
      name: BuildConfigurationName::new(row.name).map_err(|_| StoreError::Unavailable)?,
      version: BuildConfigurationVersion::new(u64::try_from(row.version).map_err(|_| StoreError::Unavailable)?)
        .map_err(|_| StoreError::Unavailable)?,
      definition,
      published_at: Timestamp::from_unix_millis(row.published_at_millis).map_err(|_| StoreError::Unavailable)?,
    })
  }
}
