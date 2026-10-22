//! Atomic Project import metadata; bytes and cleanup use the existing Artifact ports.
use crate::{
  PostgresStore,
  database::{classify, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind},
};
use octacity_server_domain::{ArtifactId, EntityKind, ProjectId, Timestamp};
use octacity_server_factory::{FactoryDigest, ImmutableReference};
use octacity_server_store::*;
use serde_json::json;
use sqlx::{PgPool, types::Json};

#[async_trait::async_trait]
impl ProjectSkillImportStore for PostgresStore {
  async fn list_project_skill_imports(
    &self,
    project: ProjectId,
    after: Option<ImmutableReference>,
    limit: u16,
  ) -> Result<ProjectSkillImportPage, StoreError> {
    check_factory_role_catalog_limit(limit, StoreOperation::PublishProjectSkillImport)?;
    crate::discovery::require_project(self.pool(), project).await?;
    let (total, Json(rows)): (i64, Json<Vec<ProjectSkillImportRecord>>) = sqlx::query_as("WITH owned AS (SELECT * FROM project_skill_imports WHERE project_id=$1 AND published_at_millis IS NOT NULL), page AS (SELECT * FROM owned WHERE ($2::text IS NULL OR (skill_identity,skill_version)>($2,$3)) ORDER BY skill_identity,skill_version LIMIT $4) SELECT (SELECT COUNT(*) FROM owned),COALESCE(jsonb_agg(document ORDER BY skill_identity,skill_version),'[]'::jsonb) FROM page")
      .bind(project.as_uuid()).bind(after.as_ref().map(|r| r.identity().as_str())).bind(after.as_ref().map(|r| r.version().as_str())).bind(crate::discovery::requested_row_limit(limit)).fetch_one(self.pool()).await.map_err(unavailable)?;
    let (items, next_cursor) = crate::discovery::decode_page(
      rows,
      limit,
      |record| Ok(record.intent.bundle.reference().clone()),
      Clone::clone,
    )?;
    Ok(FactoryRoleCatalogPage {
      total: u64::try_from(total).map_err(|_| StoreError::Unavailable)?,
      items,
      next_cursor,
    })
  }
  async fn reserve_project_skill_import(
    &self,
    request: ManagementMutation<PublishProjectSkillImport>,
  ) -> Result<ProjectSkillImportRecord, StoreError> {
    apply(self.pool(), request, false).await
  }
  async fn publish_project_skill_import(
    &self,
    request: ManagementMutation<PublishProjectSkillImport>,
  ) -> Result<ProjectSkillImportRecord, StoreError> {
    apply(self.pool(), request, true).await
  }
  async fn project_skill_import(
    &self,
    project: ProjectId,
    reference: &ImmutableReference,
  ) -> Result<Option<ProjectSkillImportRecord>, StoreError> {
    let row: Option<Json<ProjectSkillImportRecord>> = sqlx::query_scalar("SELECT document FROM project_skill_imports WHERE project_id=$1 AND skill_identity=$2 AND skill_version=$3 AND published_at_millis IS NOT NULL")
      .bind(project.as_uuid()).bind(reference.identity().as_str()).bind(reference.version().as_str()).fetch_optional(self.pool()).await.map_err(unavailable)?;
    Ok(
      row
        .map(|Json(record)| record)
        .filter(|record| record.intent.bundle.reference() == reference),
    )
  }
}
async fn apply(
  pool: &PgPool,
  request: ManagementMutation<PublishProjectSkillImport>,
  publish: bool,
) -> Result<ProjectSkillImportRecord, StoreError> {
  let (request, audit) = request.into_parts();
  let proposed = ProjectSkillImportRecord::reserve(request.intent.clone(), request.now)?;
  let id = FactoryDigest::sha256(
    "octacity.project-skill-import-reservation.v1",
    &[
      request.intent.project_id.as_uuid().as_bytes(),
      audit.security_scope().as_str().as_bytes(),
      request.intent.idempotency_key.as_str().as_bytes(),
    ],
  );
  let mut tx = pool.begin().await.map_err(unavailable)?;
  // Same existing Project lock as definition publication; no separate owner lease is
  // needed for identical immutable writes, while expiry fences publication and cleanup.
  sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM projects WHERE id=$1 FOR UPDATE")
    .bind(request.intent.project_id.as_uuid())
    .fetch_optional(&mut *tx)
    .await
    .map_err(unavailable)?
    .ok_or(StoreError::NotFound {
      entity: EntityKind::Project,
    })?;
  let previous: Option<(Json<ProjectSkillImportRecord>, Option<i64>)> =
    sqlx::query_as("SELECT document,cleanup_started_at_millis FROM project_skill_imports WHERE id=$1 FOR UPDATE")
      .bind(id.as_bytes().as_slice())
      .fetch_optional(&mut *tx)
      .await
      .map_err(unavailable)?;
  let mut record = if let Some((Json(previous), cleanup_started)) = previous {
    if cleanup_started.is_some() {
      return Err(conflict());
    }
    previous.require_live(&request.intent, request.now)?;
    if !publish || previous.published_at.is_some() {
      return Ok(previous);
    }
    previous
  } else {
    if publish {
      return Err(conflict());
    }
    let reference = proposed.intent.bundle.reference();
    sqlx::query("INSERT INTO project_skill_imports (id,project_id,security_scope,replay_key,skill_identity,skill_version,document,staging_until_millis) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
      .bind(id.as_bytes().as_slice()).bind(proposed.intent.project_id.as_uuid()).bind(audit.security_scope().as_str()).bind(proposed.intent.idempotency_key.as_str()).bind(reference.identity().as_str()).bind(reference.version().as_str()).bind(Json(&proposed)).bind(proposed.staging_until.unix_millis()).execute(&mut *tx).await.map_err(|e| classify(e, EntityKind::Artifact))?;
    for input in proposed.inputs()? {
      sqlx::query("INSERT INTO project_skill_input_objects (artifact_id,import_id) VALUES ($1,$2)")
        .bind(input.artifact_id.as_uuid())
        .bind(id.as_bytes().as_slice())
        .execute(&mut *tx)
        .await
        .map_err(|e| classify(e, EntityKind::Artifact))?;
    }
    proposed
  };
  if publish {
    record.published_at = Some(request.now);
    sqlx::query("UPDATE project_skill_imports SET document=$2,published_at_millis=$3 WHERE id=$1")
      .bind(id.as_bytes().as_slice())
      .bind(Json(&record))
      .bind(request.now.unix_millis())
      .execute(&mut *tx)
      .await
      .map_err(unavailable)?;
  }
  let kind = if publish {
    MutationKind::PublishProjectSkillImport
  } else {
    MutationKind::ReserveProjectSkillImport
  };
  let identity = MutationIdentity::new_management(
    &audit,
    kind,
    id.to_string(),
    request.now,
    EntityKind::Artifact,
    &record.intent,
  )?;
  let metadata = json!({"project_id": record.intent.project_id, "import_id": id, "bundle": record.intent.bundle.reference(), "file_count": record.intent.bundle.files().len()});
  crate::mutation::commit_fresh(
    tx,
    &identity,
    MutationFacts::management(
      &audit,
      record.intent.project_id.to_string(),
      metadata.clone(),
      metadata.clone(),
    ),
    crate::mutation::encode_outcome(&metadata)?,
  )
  .await?;
  Ok(record)
}
fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Artifact,
  }
}

pub(crate) async fn execution_input(
  pool: &PgPool,
  artifact: ArtifactId,
) -> Result<Option<PublishedExecutionInput>, StoreError> {
  let row: Option<Json<ProjectSkillImportRecord>> = sqlx::query_scalar("SELECT p.document FROM project_skill_input_objects o JOIN project_skill_imports p ON p.id=o.import_id WHERE o.artifact_id=$1 AND p.published_at_millis IS NOT NULL").bind(artifact.as_uuid()).fetch_optional(pool).await.map_err(unavailable)?;
  row
    .map(|Json(record)| {
      record
        .inputs()
        .map(|inputs| inputs.into_iter().find(|input| input.artifact_id == artifact))
    })
    .transpose()
    .map(Option::flatten)
}
/// Pending publications remain discoverable after process loss, including partial writes.
pub(crate) async fn expired(
  pool: &PgPool,
  now: Timestamp,
  limit: u16,
) -> Result<Vec<PublishedExecutionInput>, StoreError> {
  if limit == 0 {
    return Ok(vec![]);
  }
  let mut tx = pool.begin().await.map_err(unavailable)?;
  let rows: Vec<(Vec<u8>, uuid::Uuid, Json<ProjectSkillImportRecord>)> = sqlx::query_as("SELECT o.import_id,o.artifact_id,p.document FROM project_skill_input_objects o JOIN project_skill_imports p ON p.id=o.import_id WHERE p.published_at_millis IS NULL AND p.staging_until_millis<=$1 AND COALESCE(o.recheck_at_millis,p.staging_until_millis)<=$1 ORDER BY p.staging_until_millis,o.artifact_id LIMIT $2 FOR UPDATE OF p SKIP LOCKED").bind(now.unix_millis()).bind(i64::from(limit)).fetch_all(&mut *tx).await.map_err(unavailable)?;
  let mut inputs = Vec::new();
  for (id, artifact, Json(record)) in rows {
    // This durable fence also rejects a publisher carrying a pre-wait timestamp.
    sqlx::query(
      "UPDATE project_skill_imports SET cleanup_started_at_millis=COALESCE(cleanup_started_at_millis,$2) WHERE id=$1",
    )
    .bind(id)
    .bind(now.unix_millis())
    .execute(&mut *tx)
    .await
    .map_err(unavailable)?;
    inputs.push(
      record
        .inputs()?
        .into_iter()
        .find(|input| input.artifact_id.as_uuid() == artifact)
        .ok_or_else(conflict)?,
    );
  }
  tx.commit().await.map_err(unavailable)?;
  Ok(inputs)
}
pub(crate) async fn complete(
  pool: &PgPool,
  request: &CompleteFactoryExecutionInputDeletion,
) -> Result<bool, StoreError> {
  let mut tx = pool.begin().await.map_err(unavailable)?;
  let row: Option<(Json<ProjectSkillImportRecord>, Option<i64>)> = sqlx::query_as("SELECT p.document,p.cleanup_started_at_millis FROM project_skill_input_objects o JOIN project_skill_imports p ON p.id=o.import_id WHERE o.artifact_id=$1 FOR UPDATE OF p").bind(request.expected.artifact_id.as_uuid()).fetch_optional(&mut *tx).await.map_err(unavailable)?;
  let Some((Json(record), cleanup_started)) = row else {
    return Ok(false);
  };
  if cleanup_started.is_none()
    || record.published_at.is_some()
    || request.deleted_at < record.staging_until
    || !record.inputs()?.contains(&request.expected)
  {
    return Err(conflict());
  }
  // Recheck late immutable writes from an interrupted publisher, just like text intake.
  let recheck = request
    .deleted_at
    .unix_millis()
    .checked_add(crate::factory_execution_input::PRIVATE_INPUT_CLEANUP_RECHECK_MILLIS)
    .ok_or_else(conflict)?;
  sqlx::query("UPDATE project_skill_input_objects SET recheck_at_millis=GREATEST(COALESCE(recheck_at_millis,$2),$2) WHERE artifact_id=$1").bind(request.expected.artifact_id.as_uuid()).bind(recheck).execute(&mut *tx).await.map_err(unavailable)?;
  tx.commit().await.map_err(unavailable)?;
  Ok(true)
}
