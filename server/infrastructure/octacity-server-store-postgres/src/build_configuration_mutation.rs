use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationName, BuildConfigurationVersion, EntityKind, ProjectId,
};
use octacity_server_store::{
  BuildConfigurationDefinition, BuildConfigurationMutationOutcome, CreateBuildConfiguration, MutationDisposition,
  PublishBuildConfigurationVersion, PublishedBuildConfiguration, StoreError, StoreOperation,
};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction, types::Json};

use crate::{
  database::{number, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};

#[derive(Serialize)]
struct CreateFingerprint<'a> {
  id: BuildConfigurationId,
  project_id: ProjectId,
  name: &'a BuildConfigurationName,
  definition: &'a BuildConfigurationDefinition,
}

#[derive(Serialize)]
struct PublishFingerprint<'a> {
  id: BuildConfigurationId,
  expected_current_version: BuildConfigurationVersion,
  definition: &'a BuildConfigurationDefinition,
}

pub(crate) async fn create(
  pool: &sqlx::PgPool,
  request: CreateBuildConfiguration,
) -> Result<BuildConfigurationMutationOutcome, StoreError> {
  validate(&request.definition, StoreOperation::CreateBuildConfiguration)?;
  let identity = MutationIdentity::new(
    MutationKind::CreateBuildConfiguration,
    request.idempotency_key.to_string(),
    request.published_at,
    EntityKind::Configuration,
    &CreateFingerprint {
      id: request.id,
      project_id: request.project_id,
      name: &request.name,
      definition: &request.definition,
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay(outcome),
  };
  require_project(&mut transaction, request.project_id).await?;
  require_references(
    &mut transaction,
    request.project_id,
    &request.definition,
    StoreOperation::CreateBuildConfiguration,
  )
  .await?;
  sqlx::query(
    "INSERT INTO build_configurations (id, project_id, name, created_at) \
     VALUES ($1, $2, $3, to_timestamp($4::double precision / 1000.0))",
  )
  .bind(request.id.as_uuid())
  .bind(request.project_id.as_uuid())
  .bind(request.name.as_str())
  .bind(request.published_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(|error| classify_conflict(error, EntityKind::Configuration))?;
  insert_version(
    &mut transaction,
    request.id,
    BuildConfigurationVersion::INITIAL,
    &request.definition,
    request.published_at.unix_millis(),
    StoreOperation::CreateBuildConfiguration,
  )
  .await?;
  let configuration = PublishedBuildConfiguration {
    id: request.id,
    project_id: request.project_id,
    name: request.name,
    version: BuildConfigurationVersion::INITIAL,
    definition: request.definition,
    published_at: request.published_at,
  };
  let outcome = BuildConfigurationMutationOutcome {
    disposition: MutationDisposition::Applied,
    configuration,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    facts(MutationKind::CreateBuildConfiguration, &outcome.configuration),
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

pub(crate) async fn publish(
  pool: &sqlx::PgPool,
  request: PublishBuildConfigurationVersion,
) -> Result<BuildConfigurationMutationOutcome, StoreError> {
  validate(&request.definition, StoreOperation::PublishBuildConfigurationVersion)?;
  let identity = MutationIdentity::new(
    MutationKind::PublishBuildConfigurationVersion,
    request.idempotency_key.to_string(),
    request.published_at,
    EntityKind::Configuration,
    &PublishFingerprint {
      id: request.id,
      expected_current_version: request.expected_current_version,
      definition: &request.definition,
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay(outcome),
  };
  let (project_id, name) = lock_configuration(&mut transaction, request.id).await?;
  require_references(
    &mut transaction,
    project_id,
    &request.definition,
    StoreOperation::PublishBuildConfigurationVersion,
  )
  .await?;
  let (current_version, current_published_at): (i64, i64) = sqlx::query_as(
    "SELECT version, FLOOR(EXTRACT(EPOCH FROM published_at) * 1000)::BIGINT \
     FROM build_configuration_versions WHERE build_configuration_id = $1 ORDER BY version DESC LIMIT 1",
  )
  .bind(request.id.as_uuid())
  .fetch_one(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let expected = number(
    request.expected_current_version.get(),
    StoreOperation::PublishBuildConfigurationVersion,
  )?;
  if current_version != expected || request.published_at.unix_millis() < current_published_at {
    return Err(conflict(EntityKind::Configuration));
  }
  let version = request
    .expected_current_version
    .next()
    .map_err(|_| StoreError::Unavailable)?;
  insert_version(
    &mut transaction,
    request.id,
    version,
    &request.definition,
    request.published_at.unix_millis(),
    StoreOperation::PublishBuildConfigurationVersion,
  )
  .await?;
  let configuration = PublishedBuildConfiguration {
    id: request.id,
    project_id,
    name,
    version,
    definition: request.definition,
    published_at: request.published_at,
  };
  let outcome = BuildConfigurationMutationOutcome {
    disposition: MutationDisposition::Applied,
    configuration,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    facts(MutationKind::PublishBuildConfigurationVersion, &outcome.configuration),
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

async fn require_project(transaction: &mut Transaction<'_, Postgres>, project_id: ProjectId) -> Result<(), StoreError> {
  let found = sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM projects WHERE id = $1 FOR KEY SHARE")
    .bind(project_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(unavailable)?
    .is_some();
  if found {
    Ok(())
  } else {
    Err(not_found(EntityKind::Project))
  }
}

async fn require_references(
  transaction: &mut Transaction<'_, Postgres>,
  project_id: ProjectId,
  definition: &BuildConfigurationDefinition,
  operation: StoreOperation,
) -> Result<(), StoreError> {
  let repository_version = number(definition.repository_version.get(), operation)?;
  let repository_exists: bool =
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM repository_versions WHERE repository_id = $1 AND version = $2)")
      .bind(definition.repository_id.as_uuid())
      .bind(repository_version)
      .fetch_one(&mut **transaction)
      .await
      .map_err(unavailable)?;
  if !repository_exists {
    return Err(not_found(EntityKind::Repository));
  }

  let pipeline_version = number(definition.pipeline_version.get(), operation)?;
  let pipeline_exists: bool = sqlx::query_scalar(
    "SELECT EXISTS (\
       SELECT 1 FROM pipeline_versions AS version \
       JOIN pipelines AS pipeline ON pipeline.id = version.pipeline_id \
       WHERE version.pipeline_id = $1 AND version.version = $2 AND pipeline.project_id = $3\
     )",
  )
  .bind(definition.pipeline_id.as_uuid())
  .bind(pipeline_version)
  .bind(project_id.as_uuid())
  .fetch_one(&mut **transaction)
  .await
  .map_err(unavailable)?;
  if !pipeline_exists {
    return Err(not_found(EntityKind::Pipeline));
  }

  let pool_ids: Vec<_> = definition.allowed_pools.iter().map(|pool| pool.as_uuid()).collect();
  let count: i64 = sqlx::query_scalar("SELECT COUNT(DISTINCT id) FROM pools WHERE id = ANY($1)")
    .bind(&pool_ids)
    .fetch_one(&mut **transaction)
    .await
    .map_err(unavailable)?;
  if usize::try_from(count).ok() != Some(pool_ids.len()) {
    return Err(not_found(EntityKind::Pool));
  }
  Ok(())
}

async fn lock_configuration(
  transaction: &mut Transaction<'_, Postgres>,
  configuration_id: BuildConfigurationId,
) -> Result<(ProjectId, BuildConfigurationName), StoreError> {
  let row: Option<(uuid::Uuid, String)> =
    sqlx::query_as("SELECT project_id, name FROM build_configurations WHERE id = $1 FOR UPDATE")
      .bind(configuration_id.as_uuid())
      .fetch_optional(&mut **transaction)
      .await
      .map_err(unavailable)?;
  let (project_id, name) = row.ok_or_else(|| not_found(EntityKind::Configuration))?;
  Ok((
    ProjectId::from_uuid(project_id).map_err(|_| StoreError::Unavailable)?,
    BuildConfigurationName::new(name).map_err(|_| StoreError::Unavailable)?,
  ))
}

async fn insert_version(
  transaction: &mut Transaction<'_, Postgres>,
  configuration_id: BuildConfigurationId,
  version: BuildConfigurationVersion,
  definition: &BuildConfigurationDefinition,
  published_at_millis: i64,
  operation: StoreOperation,
) -> Result<(), StoreError> {
  let version = number(version.get(), operation)?;
  let repository_version = number(definition.repository_version.get(), operation)?;
  let pipeline_version = number(definition.pipeline_version.get(), operation)?;
  let snapshot = serde_json::to_value(definition).map_err(|_| StoreError::Unavailable)?;
  sqlx::query(
    "INSERT INTO build_configuration_versions \
       (build_configuration_id, version, enabled, repository_id, repository_version, pipeline_id, \
        pipeline_version, configuration_snapshot, published_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, to_timestamp($9::double precision / 1000.0))",
  )
  .bind(configuration_id.as_uuid())
  .bind(version)
  .bind(definition.enabled)
  .bind(definition.repository_id.as_uuid())
  .bind(repository_version)
  .bind(definition.pipeline_id.as_uuid())
  .bind(pipeline_version)
  .bind(Json(snapshot))
  .bind(published_at_millis)
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify_conflict(error, EntityKind::Configuration))?;
  Ok(())
}

fn validate(definition: &BuildConfigurationDefinition, operation: StoreOperation) -> Result<(), StoreError> {
  definition
    .validate()
    .map_err(|source| StoreError::InvalidInput { operation, source })
}

fn facts(kind: MutationKind, configuration: &PublishedBuildConfiguration) -> MutationFacts {
  MutationFacts {
    actor_kind: "unauthenticated_management",
    actor_identity: None,
    target_identity: configuration.id.to_string(),
    safe_metadata: json!({
      "enabled": configuration.definition.enabled,
      "pipeline_id": configuration.definition.pipeline_id,
      "pipeline_version": configuration.definition.pipeline_version.get(),
      "project_id": configuration.project_id,
      "repository_id": configuration.definition.repository_id,
      "repository_version": configuration.definition.repository_version.get(),
      "version": configuration.version.get(),
    }),
    outbox_payload: json!({
      "build_configuration_id": configuration.id,
      "event": kind.outbox_topic(),
      "project_id": configuration.project_id,
      "version": configuration.version.get(),
    }),
  }
}

fn replay(value: Value) -> Result<BuildConfigurationMutationOutcome, StoreError> {
  let mut outcome: BuildConfigurationMutationOutcome = decode_outcome(value)?;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(outcome)
}

fn classify_conflict(error: sqlx::Error, entity: EntityKind) -> StoreError {
  match error.as_database_error().and_then(|database| database.code()) {
    Some(code) if matches!(code.as_ref(), "23503" | "23505" | "23514") => conflict(entity),
    _ => StoreError::Unavailable,
  }
}

const fn not_found(entity: EntityKind) -> StoreError {
  StoreError::NotFound { entity }
}

const fn conflict(entity: EntityKind) -> StoreError {
  StoreError::Conflict { entity }
}
