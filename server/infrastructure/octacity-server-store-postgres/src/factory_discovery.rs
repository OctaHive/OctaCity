use std::str::FromStr as _;

use octacity_server_domain::{EntityKind, ProjectId};
use octacity_server_factory::{
  ExternalWorkIdentity, FactoryConfigurationId, FactoryConfigurationVersion, FactoryKey, FactoryRunId, FactoryRunState,
  FactoryRunVersion, WorkEnvelopeId,
};
use octacity_server_store::{
  CurrentFactoryConfigurationPage, CurrentFactoryConfigurationSummary, FactoryRunListVisibility, FactoryRunPage,
  FactoryRunSummary, ListFactoryRuns, ListProjectFactoryConfigurations, ReadVisibilityKind, StoreError,
};
use sqlx::{FromRow, PgPool, Postgres, Transaction};

use crate::{
  database::unavailable,
  discovery::{positive, requested_row_limit, require_project, timestamp},
  read_visibility::sql_read_visibility,
};

#[derive(FromRow)]
struct ConfigurationSummaryRow {
  id: uuid::Uuid,
  project_id: uuid::Uuid,
  version: i64,
  enabled: bool,
  created_at_millis: i64,
  published_at_millis: i64,
}

#[derive(FromRow)]
struct RunSummaryRow {
  id: uuid::Uuid,
  project_id: uuid::Uuid,
  work_id: uuid::Uuid,
  configuration_id: uuid::Uuid,
  configuration_version: i64,
  source_kind: String,
  external_identity: String,
  state: String,
  version: i64,
  admitted_at_millis: i64,
  updated_at_millis: i64,
}

pub(crate) async fn list_configurations(
  pool: &PgPool,
  request: ListProjectFactoryConfigurations,
) -> Result<CurrentFactoryConfigurationPage, StoreError> {
  require_project(pool, request.project_id()).await?;
  if request.visibility().kind() == ReadVisibilityKind::None {
    return Ok(CurrentFactoryConfigurationPage {
      total: 0,
      items: Vec::new(),
      next_cursor: None,
    });
  }
  let mut transaction = repeatable_read(pool).await?;
  let visibility = sql_read_visibility(request.visibility().view(), |id| id.as_uuid());
  let total: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM factory_configurations \
     WHERE project_id = $1 AND ($2 OR id = ANY($3::uuid[]))",
  )
  .bind(request.project_id().as_uuid())
  .bind(visibility.all)
  .bind(&visibility.identities)
  .fetch_one(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let after = request.after();
  let rows = sqlx::query_as::<_, ConfigurationSummaryRow>(
    "SELECT configuration.id, configuration.project_id, version.version, version.enabled, \
            FLOOR(EXTRACT(EPOCH FROM configuration.created_at) * 1000)::BIGINT AS created_at_millis, \
            FLOOR(EXTRACT(EPOCH FROM version.published_at) * 1000)::BIGINT AS published_at_millis \
     FROM factory_configurations AS configuration \
     JOIN factory_configuration_versions AS version \
       ON version.factory_configuration_id = configuration.id \
      AND version.version = configuration.current_version \
     WHERE configuration.project_id = $1 \
       AND ($2 OR configuration.id = ANY($3::uuid[])) \
       AND ($4::bigint IS NULL OR (configuration.created_at, configuration.id) < \
         (to_timestamp($4::double precision / 1000.0), $5::uuid)) \
     ORDER BY configuration.created_at DESC, configuration.id DESC LIMIT $6",
  )
  .bind(request.project_id().as_uuid())
  .bind(visibility.all)
  .bind(&visibility.identities)
  .bind(after.map(|position| position.created_at.unix_millis()))
  .bind(after.map(|position| position.configuration_id.as_uuid()))
  .bind(requested_row_limit(request.limit().get()))
  .fetch_all(&mut *transaction)
  .await
  .map_err(unavailable)?;
  transaction.commit().await.map_err(unavailable)?;
  let (items, next_cursor) = crate::discovery::decode_page(
    rows,
    request.limit().get(),
    decode_configuration,
    CurrentFactoryConfigurationSummary::page_position,
  )?;
  Ok(CurrentFactoryConfigurationPage {
    total: u64::try_from(total).map_err(|_| StoreError::Unavailable)?,
    items,
    next_cursor,
  })
}

pub(crate) async fn list_runs(pool: &PgPool, request: ListFactoryRuns) -> Result<FactoryRunPage, StoreError> {
  if let Some(project_id) = request.project_id() {
    require_project(pool, project_id).await?;
  }
  if request.visibility().kind() == ReadVisibilityKind::None {
    return Ok(FactoryRunPage {
      total: 0,
      items: Vec::new(),
      next_cursor: None,
    });
  }
  let mut transaction = repeatable_read(pool).await?;
  let visibility = sql_read_visibility(request.visibility().view(), |id| id.as_uuid());
  let filter = request.filter();
  let total: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM factory_runs AS run \
     JOIN factory_work_envelopes AS work ON work.id = run.work_envelope_id \
     WHERE run.visible AND ($1::uuid IS NULL OR run.project_id = $1) \
       AND ($2 OR run.id = ANY($3::uuid[])) \
       AND ($4::uuid IS NULL OR run.factory_configuration_id = $4) \
       AND ($5::text IS NULL OR work.source_kind = $5) \
       AND ($6::text IS NULL OR run.state = $6) \
       AND ($7::bigint IS NULL OR run.admitted_at >= to_timestamp($7::double precision / 1000.0)) \
       AND ($8::bigint IS NULL OR run.admitted_at < to_timestamp($8::double precision / 1000.0))",
  )
  .bind(request.project_id().map(ProjectId::as_uuid))
  .bind(visibility.all)
  .bind(&visibility.identities)
  .bind(filter.configuration_id.map(FactoryConfigurationId::as_uuid))
  .bind(filter.source.as_ref().map(FactoryKey::as_str))
  .bind(filter.state.map(FactoryRunState::as_str))
  .bind(filter.admitted_from.map(|time| time.unix_millis()))
  .bind(filter.admitted_before.map(|time| time.unix_millis()))
  .fetch_one(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let after = request.after();
  let rows = sqlx::query_as::<_, RunSummaryRow>(
    "SELECT run.id, run.project_id, run.work_envelope_id AS work_id, \
            run.factory_configuration_id AS configuration_id, \
            run.factory_configuration_version AS configuration_version, \
            work.source_kind, work.external_identity, run.state, run.version, \
            FLOOR(EXTRACT(EPOCH FROM run.admitted_at) * 1000)::BIGINT AS admitted_at_millis, \
            FLOOR(EXTRACT(EPOCH FROM run.updated_at) * 1000)::BIGINT AS updated_at_millis \
     FROM factory_runs AS run \
     JOIN factory_work_envelopes AS work ON work.id = run.work_envelope_id \
     WHERE run.visible AND ($1::uuid IS NULL OR run.project_id = $1) \
       AND ($2 OR run.id = ANY($3::uuid[])) \
       AND ($4::uuid IS NULL OR run.factory_configuration_id = $4) \
       AND ($5::text IS NULL OR work.source_kind = $5) \
       AND ($6::text IS NULL OR run.state = $6) \
       AND ($7::bigint IS NULL OR run.admitted_at >= to_timestamp($7::double precision / 1000.0)) \
       AND ($8::bigint IS NULL OR run.admitted_at < to_timestamp($8::double precision / 1000.0)) \
       AND ($9::bigint IS NULL OR (run.admitted_at, run.id) < \
         (to_timestamp($9::double precision / 1000.0), $10::uuid)) \
     ORDER BY run.admitted_at DESC, run.id DESC LIMIT $11",
  )
  .bind(request.project_id().map(ProjectId::as_uuid))
  .bind(visibility.all)
  .bind(&visibility.identities)
  .bind(filter.configuration_id.map(FactoryConfigurationId::as_uuid))
  .bind(filter.source.as_ref().map(FactoryKey::as_str))
  .bind(filter.state.map(FactoryRunState::as_str))
  .bind(filter.admitted_from.map(|time| time.unix_millis()))
  .bind(filter.admitted_before.map(|time| time.unix_millis()))
  .bind(after.map(|position| position.admitted_at.unix_millis()))
  .bind(after.map(|position| position.run_id.as_uuid()))
  .bind(requested_row_limit(request.limit().get()))
  .fetch_all(&mut *transaction)
  .await
  .map_err(unavailable)?;
  transaction.commit().await.map_err(unavailable)?;
  let (items, next_cursor) = crate::discovery::decode_page(
    rows,
    request.limit().get(),
    decode_run,
    FactoryRunSummary::page_position,
  )?;
  Ok(FactoryRunPage {
    total: u64::try_from(total).map_err(|_| StoreError::Unavailable)?,
    items,
    next_cursor,
  })
}

pub(crate) async fn read_run(
  pool: &PgPool,
  run_id: FactoryRunId,
  visibility: FactoryRunListVisibility,
) -> Result<FactoryRunSummary, StoreError> {
  if !visibility.allows(&run_id) {
    return Err(not_found(EntityKind::FactoryRun));
  }
  sqlx::query_as::<_, RunSummaryRow>(
    "SELECT run.id, run.project_id, run.work_envelope_id AS work_id, \
            run.factory_configuration_id AS configuration_id, \
            run.factory_configuration_version AS configuration_version, \
            work.source_kind, work.external_identity, run.state, run.version, \
            FLOOR(EXTRACT(EPOCH FROM run.admitted_at) * 1000)::BIGINT AS admitted_at_millis, \
            FLOOR(EXTRACT(EPOCH FROM run.updated_at) * 1000)::BIGINT AS updated_at_millis \
     FROM factory_runs AS run \
     JOIN factory_work_envelopes AS work ON work.id = run.work_envelope_id \
     WHERE run.id = $1 AND run.visible",
  )
  .bind(run_id.as_uuid())
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?
  .map(decode_run)
  .transpose()?
  .ok_or_else(|| not_found(EntityKind::FactoryRun))
}

fn decode_configuration(row: ConfigurationSummaryRow) -> Result<CurrentFactoryConfigurationSummary, StoreError> {
  Ok(CurrentFactoryConfigurationSummary {
    id: FactoryConfigurationId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?,
    project_id: ProjectId::from_uuid(row.project_id).map_err(|_| StoreError::Unavailable)?,
    version: FactoryConfigurationVersion::new(positive(row.version)?).map_err(|_| StoreError::Unavailable)?,
    enabled: row.enabled,
    created_at: timestamp(row.created_at_millis)?,
    published_at: timestamp(row.published_at_millis)?,
  })
}

fn decode_run(row: RunSummaryRow) -> Result<FactoryRunSummary, StoreError> {
  Ok(FactoryRunSummary {
    id: FactoryRunId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?,
    project_id: ProjectId::from_uuid(row.project_id).map_err(|_| StoreError::Unavailable)?,
    work_id: WorkEnvelopeId::from_uuid(row.work_id).map_err(|_| StoreError::Unavailable)?,
    configuration_id: FactoryConfigurationId::from_uuid(row.configuration_id).map_err(|_| StoreError::Unavailable)?,
    configuration_version: FactoryConfigurationVersion::new(positive(row.configuration_version)?)
      .map_err(|_| StoreError::Unavailable)?,
    source: FactoryKey::new(row.source_kind).map_err(|_| StoreError::Unavailable)?,
    external_identity: ExternalWorkIdentity::new(row.external_identity).map_err(|_| StoreError::Unavailable)?,
    state: FactoryRunState::from_str(&row.state).map_err(|_| StoreError::Unavailable)?,
    version: FactoryRunVersion::new(positive(row.version)?).map_err(|_| StoreError::Unavailable)?,
    admitted_at: timestamp(row.admitted_at_millis)?,
    updated_at: timestamp(row.updated_at_millis)?,
  })
}

async fn repeatable_read(pool: &PgPool) -> Result<Transaction<'_, Postgres>, StoreError> {
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
  Ok(transaction)
}

const fn not_found(entity: EntityKind) -> StoreError {
  StoreError::NotFound { entity }
}
