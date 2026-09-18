use octacity_server_domain::{EntityKind, ProjectId, ProjectVersion, Timestamp};
use octacity_server_store::{
  CreateProject, DeleteProject, DeleteProjectOutcome, MoveProject, MutationDisposition, Project,
  ProjectMutationOutcome, RenameProject, StoreError, StoreOperation, validate_project_ancestry,
};
use serde::Serialize;
use serde_json::json;
use sqlx::{Postgres, Transaction};

use crate::{
  database::{number, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
  project_row::ProjectRow,
};

#[derive(Serialize)]
struct CreateFingerprint<'a> {
  parent_id: Option<ProjectId>,
  name: &'a octacity_server_domain::ProjectName,
}

#[derive(Serialize)]
struct RenameFingerprint<'a> {
  id: ProjectId,
  expected_version: ProjectVersion,
  name: &'a octacity_server_domain::ProjectName,
}

#[derive(Serialize)]
struct MoveFingerprint {
  id: ProjectId,
  expected_version: ProjectVersion,
  parent_id: Option<ProjectId>,
}

#[derive(Serialize)]
struct DeleteFingerprint {
  id: ProjectId,
  expected_version: ProjectVersion,
}

pub(crate) async fn create(pool: &sqlx::PgPool, request: CreateProject) -> Result<ProjectMutationOutcome, StoreError> {
  let identity = MutationIdentity::new(
    MutationKind::CreateProject,
    request.idempotency_key.to_string(),
    request.created_at,
    EntityKind::Project,
    &CreateFingerprint {
      parent_id: request.parent_id,
      name: &request.name,
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay_project(outcome),
  };
  require_parent(&mut transaction, request.parent_id).await?;

  let row = sqlx::query_as::<_, ProjectRow>(
    "INSERT INTO projects (id, parent_id, name, version, created_at, updated_at) \
     VALUES ($1, $2, $3, 1, to_timestamp($4::double precision / 1000.0), \
             to_timestamp($4::double precision / 1000.0)) \
     RETURNING id, parent_id, name, version, \
       FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT AS updated_at_millis",
  )
  .bind(request.id.as_uuid())
  .bind(request.parent_id.map(ProjectId::as_uuid))
  .bind(request.name.as_str())
  .bind(request.created_at.unix_millis())
  .fetch_one(&mut *transaction)
  .await
  .map_err(classify_create)?;
  let outcome = ProjectMutationOutcome {
    disposition: MutationDisposition::Applied,
    project: row.try_into()?,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    project_facts(MutationKind::CreateProject, &outcome.project),
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

pub(crate) async fn rename(pool: &sqlx::PgPool, request: RenameProject) -> Result<ProjectMutationOutcome, StoreError> {
  let identity = MutationIdentity::new(
    MutationKind::RenameProject,
    request.idempotency_key.to_string(),
    request.renamed_at,
    EntityKind::Project,
    &RenameFingerprint {
      id: request.id,
      expected_version: request.expected_version,
      name: &request.name,
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay_project(outcome),
  };
  let current = lock_current(&mut transaction, request.id, request.expected_version).await?;
  require_mutation_time(&current, request.renamed_at)?;
  let next_version = number(
    request
      .expected_version
      .next()
      .map_err(|_| StoreError::Unavailable)?
      .get(),
    StoreOperation::RenameProject,
  )?;
  let row = sqlx::query_as::<_, ProjectRow>(
    "UPDATE projects SET name = $1, version = $2, \
       updated_at = to_timestamp($3::double precision / 1000.0) WHERE id = $4 \
     RETURNING id, parent_id, name, version, \
       FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT AS updated_at_millis",
  )
  .bind(request.name.as_str())
  .bind(next_version)
  .bind(request.renamed_at.unix_millis())
  .bind(request.id.as_uuid())
  .fetch_one(&mut *transaction)
  .await
  .map_err(classify_project_conflict)?;
  let outcome = ProjectMutationOutcome {
    disposition: MutationDisposition::Applied,
    project: row.try_into()?,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    project_facts(MutationKind::RenameProject, &outcome.project),
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

pub(crate) async fn move_project(
  pool: &sqlx::PgPool,
  request: MoveProject,
) -> Result<ProjectMutationOutcome, StoreError> {
  let identity = MutationIdentity::new(
    MutationKind::MoveProject,
    request.idempotency_key.to_string(),
    request.moved_at,
    EntityKind::Project,
    &MoveFingerprint {
      id: request.id,
      expected_version: request.expected_version,
      parent_id: request.parent_id,
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay_project(outcome),
  };

  // Take the hierarchy transaction lock before reading descendants. This
  // closes the concurrent A-under-B / B-under-A race across server replicas
  // while keeping the acyclicity decision in the Rust adapter.
  sqlx::query("SELECT pg_advisory_xact_lock(hashtext('octacity.projects.hierarchy'))")
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
  let current = lock_current(&mut transaction, request.id, request.expected_version).await?;
  require_mutation_time(&current, request.moved_at)?;
  require_parent(&mut transaction, request.parent_id).await?;
  require_acyclic(&mut transaction, request.id, request.parent_id).await?;
  let next_version = number(
    request
      .expected_version
      .next()
      .map_err(|_| StoreError::Unavailable)?
      .get(),
    StoreOperation::MoveProject,
  )?;
  let row = sqlx::query_as::<_, ProjectRow>(
    "UPDATE projects SET parent_id = $1, version = $2, \
       updated_at = to_timestamp($3::double precision / 1000.0) WHERE id = $4 \
     RETURNING id, parent_id, name, version, \
       FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT AS updated_at_millis",
  )
  .bind(request.parent_id.map(ProjectId::as_uuid))
  .bind(next_version)
  .bind(request.moved_at.unix_millis())
  .bind(request.id.as_uuid())
  .fetch_one(&mut *transaction)
  .await
  .map_err(classify_project_conflict)?;
  let outcome = ProjectMutationOutcome {
    disposition: MutationDisposition::Applied,
    project: row.try_into()?,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    project_facts(MutationKind::MoveProject, &outcome.project),
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

pub(crate) async fn delete(pool: &sqlx::PgPool, request: DeleteProject) -> Result<DeleteProjectOutcome, StoreError> {
  let identity = MutationIdentity::new(
    MutationKind::DeleteProject,
    request.idempotency_key.to_string(),
    request.deleted_at,
    EntityKind::Project,
    &DeleteFingerprint {
      id: request.id,
      expected_version: request.expected_version,
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay_delete(outcome),
  };
  let current = lock_current(&mut transaction, request.id, request.expected_version).await?;
  require_mutation_time(&current, request.deleted_at)?;
  sqlx::query("DELETE FROM projects WHERE id = $1")
    .bind(request.id.as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(classify_project_conflict)?;
  let outcome = DeleteProjectOutcome {
    disposition: MutationDisposition::Applied,
    project_id: request.id,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    MutationFacts {
      actor_kind: "unauthenticated_management",
      actor_identity: None,
      target_identity: request.id.to_string(),
      safe_metadata: json!({"version": request.expected_version.get()}),
      outbox_payload: json!({"project_id": request.id}),
    },
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

async fn lock_current(
  transaction: &mut Transaction<'_, Postgres>,
  project_id: ProjectId,
  expected_version: ProjectVersion,
) -> Result<Project, StoreError> {
  let project: Project = sqlx::query_as::<_, ProjectRow>(
    "SELECT id, parent_id, name, version, \
       FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT AS updated_at_millis \
     FROM projects WHERE id = $1 FOR UPDATE",
  )
  .bind(project_id.as_uuid())
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Project,
  })?
  .try_into()?;
  if project.version != expected_version {
    return Err(StoreError::Conflict {
      entity: EntityKind::Project,
    });
  }
  Ok(project)
}

async fn require_parent(
  transaction: &mut Transaction<'_, Postgres>,
  parent_id: Option<ProjectId>,
) -> Result<(), StoreError> {
  let Some(parent_id) = parent_id else {
    return Ok(());
  };
  let exists = sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM projects WHERE id = $1 FOR KEY SHARE")
    .bind(parent_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(unavailable)?
    .is_some();
  if !exists {
    return Err(StoreError::NotFound {
      entity: EntityKind::Project,
    });
  }
  Ok(())
}

async fn require_acyclic(
  transaction: &mut Transaction<'_, Postgres>,
  project_id: ProjectId,
  parent_id: Option<ProjectId>,
) -> Result<(), StoreError> {
  let Some(parent_id) = parent_id else {
    return Ok(());
  };
  let ancestry = sqlx::query_scalar::<_, uuid::Uuid>(
    "WITH RECURSIVE ancestors(id, parent_id) AS ( \
       SELECT id, parent_id FROM projects WHERE id = $1 \
       UNION \
       SELECT parent.id, parent.parent_id FROM projects AS parent \
       JOIN ancestors AS child ON parent.id = child.parent_id \
     ) SELECT id FROM ancestors",
  )
  .bind(parent_id.as_uuid())
  .fetch_all(&mut **transaction)
  .await
  .map_err(unavailable)?;
  let ancestry = ancestry
    .into_iter()
    .map(|id| ProjectId::from_uuid(id).map_err(|_| StoreError::Unavailable))
    .collect::<Result<Vec<_>, _>>()?;
  validate_project_ancestry(project_id, ancestry).map_err(|_| StoreError::Conflict {
    entity: EntityKind::Project,
  })
}

fn require_mutation_time(project: &Project, timestamp: Timestamp) -> Result<(), StoreError> {
  if timestamp < project.updated_at {
    return Err(StoreError::Conflict {
      entity: EntityKind::Project,
    });
  }
  Ok(())
}

fn project_facts(kind: MutationKind, project: &Project) -> MutationFacts {
  MutationFacts {
    actor_kind: "unauthenticated_management",
    actor_identity: None,
    target_identity: project.id.to_string(),
    safe_metadata: json!({
      "version": project.version.get(),
      "parent_id": project.parent_id,
    }),
    outbox_payload: json!({
      "event": kind.outbox_topic(),
      "project_id": project.id,
      "version": project.version.get(),
    }),
  }
}

fn replay_project(value: serde_json::Value) -> Result<ProjectMutationOutcome, StoreError> {
  let mut outcome: ProjectMutationOutcome = decode_outcome(value)?;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(outcome)
}

fn replay_delete(value: serde_json::Value) -> Result<DeleteProjectOutcome, StoreError> {
  let mut outcome: DeleteProjectOutcome = decode_outcome(value)?;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(outcome)
}

fn classify_create(error: sqlx::Error) -> StoreError {
  let Some(database) = error.as_database_error() else {
    return StoreError::Unavailable;
  };
  match (database.code().as_deref(), database.constraint()) {
    (Some("23505"), Some("projects_pkey")) => StoreError::Duplicate {
      entity: EntityKind::Project,
    },
    (Some("23505" | "23514"), _) => StoreError::Conflict {
      entity: EntityKind::Project,
    },
    _ => StoreError::Unavailable,
  }
}

fn classify_project_conflict(error: sqlx::Error) -> StoreError {
  match error.as_database_error().and_then(|database| database.code()) {
    Some(code) if matches!(code.as_ref(), "23503" | "23505" | "23514") => StoreError::Conflict {
      entity: EntityKind::Project,
    },
    _ => StoreError::Unavailable,
  }
}
