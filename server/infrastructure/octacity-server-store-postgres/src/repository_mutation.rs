use octacity_server_domain::{EntityKind, ProjectId, RepositoryId, RepositoryName, RepositoryVersion};
use octacity_server_store::{
  CreateRepository, MutationDisposition, PublishRepositoryVersion, PublishedRepository, RepositoryDefinition,
  RepositoryMutationOutcome, StoreError, StoreOperation,
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
  project_id: ProjectId,
  name: &'a RepositoryName,
  definition: &'a RepositoryDefinition,
}

#[derive(Serialize)]
struct PublishFingerprint<'a> {
  id: RepositoryId,
  expected_current_version: RepositoryVersion,
  definition: &'a RepositoryDefinition,
}

pub(crate) async fn create(
  pool: &sqlx::PgPool,
  request: CreateRepository,
) -> Result<RepositoryMutationOutcome, StoreError> {
  validate(&request.definition, StoreOperation::CreateRepository)?;
  let identity = MutationIdentity::new(
    MutationKind::CreateRepository,
    request.idempotency_key.to_string(),
    request.published_at,
    EntityKind::Repository,
    &CreateFingerprint {
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
  sqlx::query(
    "INSERT INTO repositories (id, project_id, name, created_at) \
     VALUES ($1, $2, $3, to_timestamp($4::double precision / 1000.0))",
  )
  .bind(request.id.as_uuid())
  .bind(request.project_id.as_uuid())
  .bind(request.name.as_str())
  .bind(request.published_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(|error| classify_conflict(error, EntityKind::Repository))?;
  insert_version(
    &mut transaction,
    request.id,
    RepositoryVersion::INITIAL,
    &request.definition,
    request.published_at.unix_millis(),
    StoreOperation::CreateRepository,
  )
  .await?;
  let repository = PublishedRepository {
    id: request.id,
    project_id: request.project_id,
    name: request.name,
    version: RepositoryVersion::INITIAL,
    definition: request.definition,
    published_at: request.published_at,
  };
  let outcome = RepositoryMutationOutcome {
    disposition: MutationDisposition::Applied,
    repository,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    facts(MutationKind::CreateRepository, &outcome.repository),
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

pub(crate) async fn publish(
  pool: &sqlx::PgPool,
  request: PublishRepositoryVersion,
) -> Result<RepositoryMutationOutcome, StoreError> {
  validate(&request.definition, StoreOperation::PublishRepositoryVersion)?;
  let identity = MutationIdentity::new(
    MutationKind::PublishRepositoryVersion,
    request.idempotency_key.to_string(),
    request.published_at,
    EntityKind::Repository,
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
  let (project_id, name) = lock_repository(&mut transaction, request.id).await?;
  let (current_version, current_published_at): (i64, i64) = sqlx::query_as(
    "SELECT version, FLOOR(EXTRACT(EPOCH FROM published_at) * 1000)::BIGINT \
     FROM repository_versions WHERE repository_id = $1 ORDER BY version DESC LIMIT 1",
  )
  .bind(request.id.as_uuid())
  .fetch_one(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let expected = number(
    request.expected_current_version.get(),
    StoreOperation::PublishRepositoryVersion,
  )?;
  if current_version != expected || request.published_at.unix_millis() < current_published_at {
    return Err(conflict(EntityKind::Repository));
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
    StoreOperation::PublishRepositoryVersion,
  )
  .await?;
  let repository = PublishedRepository {
    id: request.id,
    project_id,
    name,
    version,
    definition: request.definition,
    published_at: request.published_at,
  };
  let outcome = RepositoryMutationOutcome {
    disposition: MutationDisposition::Applied,
    repository,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    facts(MutationKind::PublishRepositoryVersion, &outcome.repository),
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
    Err(StoreError::NotFound {
      entity: EntityKind::Project,
    })
  }
}

async fn lock_repository(
  transaction: &mut Transaction<'_, Postgres>,
  repository_id: RepositoryId,
) -> Result<(ProjectId, RepositoryName), StoreError> {
  let row: Option<(uuid::Uuid, String)> =
    sqlx::query_as("SELECT project_id, name FROM repositories WHERE id = $1 FOR UPDATE")
      .bind(repository_id.as_uuid())
      .fetch_optional(&mut **transaction)
      .await
      .map_err(unavailable)?;
  let (project_id, name) = row.ok_or(StoreError::NotFound {
    entity: EntityKind::Repository,
  })?;
  Ok((
    ProjectId::from_uuid(project_id).map_err(|_| StoreError::Unavailable)?,
    RepositoryName::new(name).map_err(|_| StoreError::Unavailable)?,
  ))
}

async fn insert_version(
  transaction: &mut Transaction<'_, Postgres>,
  repository_id: RepositoryId,
  version: RepositoryVersion,
  definition: &RepositoryDefinition,
  published_at_millis: i64,
  operation: StoreOperation,
) -> Result<(), StoreError> {
  let version = number(version.get(), operation)?;
  let selection = serde_json::to_value(&definition.selection).map_err(|_| StoreError::Unavailable)?;
  sqlx::query(
    "INSERT INTO repository_versions \
       (repository_id, version, vcs_integration_id, repository_locator, selection_policy, published_at) \
     VALUES ($1, $2, $3, $4, $5, to_timestamp($6::double precision / 1000.0))",
  )
  .bind(repository_id.as_uuid())
  .bind(version)
  .bind(definition.vcs_integration_id.as_uuid())
  .bind(definition.repository_locator.as_str())
  .bind(Json(selection))
  .bind(published_at_millis)
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify_conflict(error, EntityKind::Repository))?;
  Ok(())
}

fn validate(definition: &RepositoryDefinition, operation: StoreOperation) -> Result<(), StoreError> {
  definition
    .validate()
    .map_err(|source| StoreError::InvalidInput { operation, source })
}

fn facts(kind: MutationKind, repository: &PublishedRepository) -> MutationFacts {
  MutationFacts {
    actor_kind: "unauthenticated_management",
    actor_identity: None,
    target_identity: repository.id.to_string(),
    safe_metadata: json!({
      "project_id": repository.project_id,
      "version": repository.version.get(),
    }),
    outbox_payload: json!({
      "event": kind.outbox_topic(),
      "project_id": repository.project_id,
      "repository_id": repository.id,
      "version": repository.version.get(),
    }),
  }
}

fn replay(value: Value) -> Result<RepositoryMutationOutcome, StoreError> {
  let mut outcome: RepositoryMutationOutcome = decode_outcome(value)?;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(outcome)
}

fn classify_conflict(error: sqlx::Error, entity: EntityKind) -> StoreError {
  match error.as_database_error().and_then(|database| database.code()) {
    Some(code) if matches!(code.as_ref(), "23503" | "23505" | "23514") => conflict(entity),
    _ => StoreError::Unavailable,
  }
}

const fn conflict(entity: EntityKind) -> StoreError {
  StoreError::Conflict { entity }
}
