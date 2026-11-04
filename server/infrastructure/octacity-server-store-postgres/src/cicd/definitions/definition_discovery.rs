use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationName, BuildConfigurationVersion, PipelineId, PipelineName, PipelineVersion,
  ProjectId, RepositoryId, RepositoryName, RepositoryVersion, TriggerId, TriggerVersion,
};
use octacity_server_store::{
  CurrentBuildConfigurationPage, CurrentBuildConfigurationSummary, CurrentPipelinePage, CurrentPipelineSummary,
  CurrentRepositoryPage, CurrentRepositorySummary, CurrentTriggerDefinitionKind, CurrentTriggerDefinitionPage,
  CurrentTriggerDefinitionSummary, ListProjectBuildConfigurations, ListProjectPipelines, ListProjectRepositories,
  ListProjectTriggerDefinitions, ReadVisibilityKind, StoreError,
};
use sqlx::{FromRow, PgPool};

use crate::{
  database::unavailable,
  discovery::{decode_page, positive, requested_row_limit, require_project, timestamp},
  read_visibility::sql_read_visibility,
};

#[derive(FromRow)]
struct NamedDefinitionRow {
  id: uuid::Uuid,
  project_id: uuid::Uuid,
  name: String,
  version: i64,
  published_at_millis: i64,
}

#[derive(FromRow)]
struct BuildConfigurationSummaryRow {
  id: uuid::Uuid,
  project_id: uuid::Uuid,
  name: String,
  version: i64,
  enabled: bool,
  published_at_millis: i64,
}

#[derive(FromRow)]
struct TriggerDefinitionSummaryRow {
  id: uuid::Uuid,
  project_id: uuid::Uuid,
  build_configuration_id: uuid::Uuid,
  build_configuration_version: i64,
  version: i64,
  kind: String,
  enabled: bool,
  published_at_millis: i64,
}

pub(crate) async fn pipelines(pool: &PgPool, request: ListProjectPipelines) -> Result<CurrentPipelinePage, StoreError> {
  require_project(pool, request.project_id()).await?;
  if request.visibility().kind() == ReadVisibilityKind::None {
    return Ok(CurrentPipelinePage {
      items: Vec::new(),
      next_cursor: None,
    });
  }
  let visibility = sql_read_visibility(request.visibility().view(), |id| id.as_uuid());
  let rows = sqlx::query_as::<_, NamedDefinitionRow>(
    "SELECT pipeline.id, pipeline.project_id, pipeline.name, current.version, \
       FLOOR(EXTRACT(EPOCH FROM current.published_at) * 1000)::BIGINT AS published_at_millis \
     FROM pipelines AS pipeline \
     JOIN LATERAL (\
       SELECT version, published_at FROM pipeline_versions \
       WHERE pipeline_id = pipeline.id ORDER BY version DESC LIMIT 1\
     ) AS current ON TRUE \
     WHERE pipeline.project_id = $1 \
       AND ($2 OR pipeline.id = ANY($3::uuid[])) \
       AND ($4::uuid IS NULL OR pipeline.id > $4) \
     ORDER BY pipeline.id LIMIT $5",
  )
  .bind(request.project_id().as_uuid())
  .bind(visibility.all)
  .bind(visibility.identities)
  .bind(request.after().map(PipelineId::as_uuid))
  .bind(requested_row_limit(request.limit().get()))
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;
  let (items, next_cursor) = decode_page(rows, request.limit().get(), decode_pipeline, |item| item.id)?;
  Ok(CurrentPipelinePage { items, next_cursor })
}

pub(crate) async fn repositories(
  pool: &PgPool,
  request: ListProjectRepositories,
) -> Result<CurrentRepositoryPage, StoreError> {
  require_project(pool, request.project_id()).await?;
  if request.visibility().kind() == ReadVisibilityKind::None {
    return Ok(CurrentRepositoryPage {
      items: Vec::new(),
      next_cursor: None,
    });
  }
  let visibility = sql_read_visibility(request.visibility().view(), |id| id.as_uuid());
  let rows = sqlx::query_as::<_, NamedDefinitionRow>(
    "SELECT repository.id, repository.project_id, repository.name, current.version, \
       FLOOR(EXTRACT(EPOCH FROM current.published_at) * 1000)::BIGINT AS published_at_millis \
     FROM repositories AS repository \
     JOIN LATERAL (\
       SELECT version, published_at FROM repository_versions \
       WHERE repository_id = repository.id ORDER BY version DESC LIMIT 1\
     ) AS current ON TRUE \
     WHERE repository.project_id = $1 \
       AND ($2 OR repository.id = ANY($3::uuid[])) \
       AND ($4::uuid IS NULL OR repository.id > $4) \
     ORDER BY repository.id LIMIT $5",
  )
  .bind(request.project_id().as_uuid())
  .bind(visibility.all)
  .bind(visibility.identities)
  .bind(request.after().map(RepositoryId::as_uuid))
  .bind(requested_row_limit(request.limit().get()))
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;
  let (items, next_cursor) = decode_page(rows, request.limit().get(), decode_repository, |item| item.id)?;
  Ok(CurrentRepositoryPage { items, next_cursor })
}

pub(crate) async fn build_configurations(
  pool: &PgPool,
  request: ListProjectBuildConfigurations,
) -> Result<CurrentBuildConfigurationPage, StoreError> {
  require_project(pool, request.project_id()).await?;
  if request.visibility().kind() == ReadVisibilityKind::None {
    return Ok(CurrentBuildConfigurationPage {
      items: Vec::new(),
      next_cursor: None,
    });
  }
  let visibility = sql_read_visibility(request.visibility().view(), |id| id.as_uuid());
  let rows = sqlx::query_as::<_, BuildConfigurationSummaryRow>(
    "SELECT configuration.id, configuration.project_id, configuration.name, current.version, current.enabled, \
       FLOOR(EXTRACT(EPOCH FROM current.published_at) * 1000)::BIGINT AS published_at_millis \
     FROM build_configurations AS configuration \
     JOIN LATERAL (\
       SELECT version, enabled, published_at FROM build_configuration_versions \
       WHERE build_configuration_id = configuration.id ORDER BY version DESC LIMIT 1\
     ) AS current ON TRUE \
     WHERE configuration.project_id = $1 \
       AND ($2 OR configuration.id = ANY($3::uuid[])) \
       AND ($4::uuid IS NULL OR configuration.id > $4) \
     ORDER BY configuration.id LIMIT $5",
  )
  .bind(request.project_id().as_uuid())
  .bind(visibility.all)
  .bind(visibility.identities)
  .bind(request.after().map(BuildConfigurationId::as_uuid))
  .bind(requested_row_limit(request.limit().get()))
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;
  let (items, next_cursor) = decode_page(rows, request.limit().get(), decode_build_configuration, |item| item.id)?;
  Ok(CurrentBuildConfigurationPage { items, next_cursor })
}

pub(crate) async fn triggers(
  pool: &PgPool,
  request: ListProjectTriggerDefinitions,
) -> Result<CurrentTriggerDefinitionPage, StoreError> {
  require_project(pool, request.project_id()).await?;
  if request.visibility().kind() == ReadVisibilityKind::None {
    return Ok(CurrentTriggerDefinitionPage {
      items: Vec::new(),
      next_cursor: None,
    });
  }
  let visibility = sql_read_visibility(request.visibility().view(), |id| id.as_uuid());
  let rows = sqlx::query_as::<_, TriggerDefinitionSummaryRow>(
    "SELECT trigger.id, configuration.project_id, trigger.build_configuration_id, \
       trigger.build_configuration_version, trigger.version, trigger.kind, trigger.enabled, \
       FLOOR(EXTRACT(EPOCH FROM trigger.created_at) * 1000)::BIGINT AS published_at_millis \
     FROM triggers AS trigger \
     JOIN build_configurations AS configuration ON configuration.id = trigger.build_configuration_id \
     WHERE configuration.project_id = $1 \
       AND trigger.kind IN ('manual', 'scheduled', 'internal') \
       AND ($2 OR trigger.id = ANY($3::uuid[])) \
       AND ($4::uuid IS NULL OR trigger.id > $4) \
       AND NOT EXISTS (\
         SELECT 1 FROM triggers AS newer \
         WHERE newer.id = trigger.id AND newer.version > trigger.version\
       ) \
     ORDER BY trigger.id LIMIT $5",
  )
  .bind(request.project_id().as_uuid())
  .bind(visibility.all)
  .bind(visibility.identities)
  .bind(request.after().map(TriggerId::as_uuid))
  .bind(requested_row_limit(request.limit().get()))
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;
  let (items, next_cursor) = decode_page(rows, request.limit().get(), decode_trigger, |item| item.id)?;
  Ok(CurrentTriggerDefinitionPage { items, next_cursor })
}

fn decode_pipeline(row: NamedDefinitionRow) -> Result<CurrentPipelineSummary, StoreError> {
  Ok(CurrentPipelineSummary {
    id: PipelineId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?,
    project_id: ProjectId::from_uuid(row.project_id).map_err(|_| StoreError::Unavailable)?,
    name: PipelineName::new(row.name).map_err(|_| StoreError::Unavailable)?,
    version: PipelineVersion::new(positive(row.version)?).map_err(|_| StoreError::Unavailable)?,
    published_at: timestamp(row.published_at_millis)?,
  })
}

fn decode_repository(row: NamedDefinitionRow) -> Result<CurrentRepositorySummary, StoreError> {
  Ok(CurrentRepositorySummary {
    id: RepositoryId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?,
    project_id: ProjectId::from_uuid(row.project_id).map_err(|_| StoreError::Unavailable)?,
    name: RepositoryName::new(row.name).map_err(|_| StoreError::Unavailable)?,
    version: RepositoryVersion::new(positive(row.version)?).map_err(|_| StoreError::Unavailable)?,
    published_at: timestamp(row.published_at_millis)?,
  })
}

fn decode_build_configuration(
  row: BuildConfigurationSummaryRow,
) -> Result<CurrentBuildConfigurationSummary, StoreError> {
  Ok(CurrentBuildConfigurationSummary {
    id: BuildConfigurationId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?,
    project_id: ProjectId::from_uuid(row.project_id).map_err(|_| StoreError::Unavailable)?,
    name: BuildConfigurationName::new(row.name).map_err(|_| StoreError::Unavailable)?,
    version: BuildConfigurationVersion::new(positive(row.version)?).map_err(|_| StoreError::Unavailable)?,
    enabled: row.enabled,
    published_at: timestamp(row.published_at_millis)?,
  })
}

fn decode_trigger(row: TriggerDefinitionSummaryRow) -> Result<CurrentTriggerDefinitionSummary, StoreError> {
  let kind = match row.kind.as_str() {
    "manual" => CurrentTriggerDefinitionKind::Manual,
    "scheduled" => CurrentTriggerDefinitionKind::Scheduled,
    "internal" => CurrentTriggerDefinitionKind::Internal,
    _ => return Err(StoreError::Unavailable),
  };
  Ok(CurrentTriggerDefinitionSummary {
    id: TriggerId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?,
    project_id: ProjectId::from_uuid(row.project_id).map_err(|_| StoreError::Unavailable)?,
    configuration_id: BuildConfigurationId::from_uuid(row.build_configuration_id)
      .map_err(|_| StoreError::Unavailable)?,
    configuration_version: BuildConfigurationVersion::new(positive(row.build_configuration_version)?)
      .map_err(|_| StoreError::Unavailable)?,
    version: TriggerVersion::new(positive(row.version)?).map_err(|_| StoreError::Unavailable)?,
    kind,
    enabled: row.enabled,
    published_at: timestamp(row.published_at_millis)?,
  })
}
