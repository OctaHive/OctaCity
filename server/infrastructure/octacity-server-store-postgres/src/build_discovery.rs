use octacity_server_domain::{
  AttemptId, AttemptNumber, BuildConfigurationId, BuildConfigurationVersion, BuildId, EntityKind, ProjectId,
};
use octacity_server_store::{
  ListProjectBuilds, ProjectBuildPage, ProjectBuildSummary, ReadVisibilityKind, StoreError, TriggerCause,
};
use sqlx::{FromRow, PgPool, types::Json};

use crate::{
  database::unavailable,
  discovery::{decode_page, positive, requested_row_limit, require_project, timestamp},
  read_visibility::sql_read_visibility,
  state::{build_state, parse_attempt_state, parse_build_state},
};

#[derive(FromRow)]
struct ProjectBuildSummaryRow {
  id: uuid::Uuid,
  project_id: uuid::Uuid,
  configuration_id: uuid::Uuid,
  configuration_version: i64,
  cause: Json<TriggerCause>,
  state: String,
  created_at_millis: i64,
  updated_at_millis: i64,
  current_attempt_id: uuid::Uuid,
  current_attempt_number: i64,
  current_attempt_state: String,
}

pub(crate) async fn list(pool: &PgPool, request: ListProjectBuilds) -> Result<ProjectBuildPage, StoreError> {
  require_project(pool, request.project_id()).await?;
  if let Some(configuration_id) = request.filter().configuration_id {
    require_configuration(pool, request.project_id(), configuration_id).await?;
  }
  if request.visibility().kind() == ReadVisibilityKind::None {
    return Ok(ProjectBuildPage {
      items: Vec::new(),
      next_cursor: None,
    });
  }

  let visibility = sql_read_visibility(request.visibility().view(), |id| id.as_uuid());
  let filter = request.filter();
  let after = request.after();
  let rows = sqlx::query_as::<_, ProjectBuildSummaryRow>(
    "SELECT build.id, build.project_id, build.build_configuration_id AS configuration_id, \
            build.build_configuration_version AS configuration_version, occurrence.cause, build.state, \
            FLOOR(EXTRACT(EPOCH FROM build.created_at) * 1000)::BIGINT AS created_at_millis, \
            FLOOR(EXTRACT(EPOCH FROM build.updated_at) * 1000)::BIGINT AS updated_at_millis, \
            current_attempt.id AS current_attempt_id, \
            current_attempt.attempt_number AS current_attempt_number, \
            current_attempt.state AS current_attempt_state \
     FROM builds AS build \
     JOIN trigger_occurrences AS occurrence ON occurrence.id = build.trigger_occurrence_id \
     JOIN LATERAL (\
       SELECT attempt.id, attempt.attempt_number, attempt.state \
       FROM attempts AS attempt \
       WHERE attempt.build_id = build.id \
       ORDER BY attempt.attempt_number DESC LIMIT 1\
     ) AS current_attempt ON TRUE \
     WHERE build.project_id = $1 \
       AND build.metadata_visible \
       AND ($2 OR build.id = ANY($3::uuid[])) \
       AND ($4::uuid IS NULL OR build.build_configuration_id = $4) \
       AND ($5::text IS NULL OR build.state = $5) \
       AND ($6::bigint IS NULL OR (build.created_at, build.id) < \
         (to_timestamp($6::double precision / 1000.0), $7::uuid)) \
     ORDER BY build.created_at DESC, build.id DESC LIMIT $8",
  )
  .bind(request.project_id().as_uuid())
  .bind(visibility.all)
  .bind(visibility.identities)
  .bind(filter.configuration_id.map(BuildConfigurationId::as_uuid))
  .bind(filter.state.map(build_state))
  .bind(after.map(|position| position.created_at.unix_millis()))
  .bind(after.map(|position| position.build_id.as_uuid()))
  .bind(requested_row_limit(request.limit().get()))
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;

  let (items, next_cursor) = decode_page(rows, request.limit().get(), decode, ProjectBuildSummary::page_position)?;
  Ok(ProjectBuildPage { items, next_cursor })
}

fn decode(row: ProjectBuildSummaryRow) -> Result<ProjectBuildSummary, StoreError> {
  let state = parse_build_state(&row.state)?;
  Ok(ProjectBuildSummary {
    id: BuildId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?,
    project_id: ProjectId::from_uuid(row.project_id).map_err(|_| StoreError::Unavailable)?,
    configuration_id: BuildConfigurationId::from_uuid(row.configuration_id).map_err(|_| StoreError::Unavailable)?,
    configuration_version: BuildConfigurationVersion::new(positive(row.configuration_version)?)
      .map_err(|_| StoreError::Unavailable)?,
    cause: row.cause.0,
    state,
    created_at: timestamp(row.created_at_millis)?,
    current_attempt_id: AttemptId::from_uuid(row.current_attempt_id).map_err(|_| StoreError::Unavailable)?,
    current_attempt_number: AttemptNumber::new(positive(row.current_attempt_number)?)
      .map_err(|_| StoreError::Unavailable)?,
    current_attempt_state: parse_attempt_state(&row.current_attempt_state)?,
    terminal_at: state
      .is_terminal()
      .then(|| timestamp(row.updated_at_millis))
      .transpose()?,
  })
}

async fn require_configuration(
  pool: &PgPool,
  project_id: ProjectId,
  configuration_id: BuildConfigurationId,
) -> Result<(), StoreError> {
  let exists: bool =
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM build_configurations WHERE id = $1 AND project_id = $2)")
      .bind(configuration_id.as_uuid())
      .bind(project_id.as_uuid())
      .fetch_one(pool)
      .await
      .map_err(unavailable)?;
  exists.then_some(()).ok_or(StoreError::NotFound {
    entity: EntityKind::Configuration,
  })
}
