//! The document variant shares the existing Project text preparation/object/cleanup journal.
use crate::mutation::{MutationFacts, MutationIdentity, MutationKind};
use crate::{PostgresStore, database::unavailable};
use octacity_server_domain::{EntityKind, ProjectId};
use octacity_server_factory::FactoryDigest;
use octacity_server_store::*;
use serde_json::{Value, json};
use sqlx::{PgConnection, types::Json};

fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Artifact,
  }
}
fn decode(Json(value): Json<Value>) -> Result<ProjectDocumentPreparation, StoreError> {
  serde_json::from_value(value).map_err(|_| conflict())
}
async fn load(connection: &mut PgConnection, id: FactoryDigest) -> Result<ProjectDocumentPreparation, StoreError> {
  let row: Option<Json<Value>> =
    sqlx::query_scalar("SELECT document FROM project_text_preparations WHERE id=$1 FOR UPDATE")
      .bind(id.as_bytes().as_slice())
      .fetch_optional(connection)
      .await
      .map_err(unavailable)?;
  decode(row.ok_or_else(conflict)?)
}
async fn save(connection: &mut PgConnection, record: &ProjectDocumentPreparation) -> Result<(), StoreError> {
  sqlx::query("UPDATE project_text_preparations SET document=$2 WHERE id=$1")
    .bind(record.id.as_bytes().as_slice())
    .bind(Json(record))
    .execute(connection)
    .await
    .map_err(unavailable)?;
  Ok(())
}
#[async_trait::async_trait]
impl ProjectDocumentInputStore for PostgresStore {
  async fn list_project_documents(&self, request: ListProjectDocuments) -> Result<ProjectDocumentPage, StoreError> {
    request.validate()?;
    crate::discovery::require_project(self.pool(), request.project_id).await?;
    let (all, visible): (bool, Vec<uuid::Uuid>) = match request.visibility.view() {
      ReadVisibilityView::All => (true, vec![]),
      ReadVisibilityView::None => (false, vec![]),
      ReadVisibilityView::Restricted(ids) => (false, ids.iter().map(|id| id.as_uuid()).collect()),
    };
    let rows: Vec<Json<Value>> = sqlx::query_scalar("SELECT p.document FROM project_text_preparations p JOIN project_text_input_objects o ON o.preparation_id=p.id WHERE p.project_id=$1 AND o.role='document' AND p.document->>'published_at' IS NOT NULL AND (p.document->>'expires_at')::bigint>$2 AND o.deleted_at_millis IS NULL AND ($3::uuid IS NULL OR o.artifact_id>$3) AND ($4 OR o.artifact_id=ANY($5::uuid[])) ORDER BY o.artifact_id LIMIT $6")
      .bind(request.project_id.as_uuid()).bind(request.observed_at.unix_millis()).bind(request.after.map(|id| id.as_uuid())).bind(all).bind(visible).bind(crate::discovery::requested_row_limit(request.limit)).fetch_all(self.pool()).await.map_err(unavailable)?;
    let (items, next_cursor) = crate::discovery::decode_page(
      rows,
      request.limit,
      |row| decode(row)?.resource(),
      |resource| resource.artifact.artifact_id(),
    )?;
    Ok(ProjectDocumentPage { items, next_cursor })
  }
  async fn project_document_preparation(
    &self,
    project: ProjectId,
    scope: &ManagementSecurityScope,
    key: &IdempotencyKey,
  ) -> Result<Option<ProjectDocumentPreparation>, StoreError> {
    let row: Option<Json<Value>> = sqlx::query_scalar(
      "SELECT document FROM project_text_preparations WHERE project_id=$1 AND security_scope=$2 AND replay_key=$3",
    )
    .bind(project.as_uuid())
    .bind(scope.as_str())
    .bind(key.as_str())
    .fetch_optional(self.pool())
    .await
    .map_err(unavailable)?;
    row.map(decode).transpose()
  }
  async fn project_document_content(
    &self,
    project: ProjectId,
    scope: &ManagementSecurityScope,
    key: &IdempotencyKey,
  ) -> Result<Option<String>, StoreError> {
    let row: Option<Json<Value>> = sqlx::query_scalar("SELECT d.documents FROM project_text_documents d JOIN project_text_preparations p ON p.id=d.preparation_id WHERE p.project_id=$1 AND p.security_scope=$2 AND p.replay_key=$3").bind(project.as_uuid()).bind(scope.as_str()).bind(key.as_str()).fetch_optional(self.pool()).await.map_err(unavailable)?;
    row
      .map(|Json(value)| {
        value
          .get("content")
          .and_then(Value::as_str)
          .map(str::to_owned)
          .ok_or_else(conflict)
      })
      .transpose()
  }
  async fn reserve_project_document(
    &self,
    request: ManagementMutation<ReserveProjectDocument>,
  ) -> Result<ProjectDocumentPreparation, StoreError> {
    let (request, audit) = request.into_parts();
    let proposed =
      ProjectDocumentPreparation::reserve(&request.submission, audit.security_scope().clone(), request.now)?;
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
      .bind(i64::from_be_bytes(
        proposed.id.as_bytes()[..8].try_into().map_err(|_| conflict())?,
      ))
      .execute(&mut *tx)
      .await
      .map_err(unavailable)?;
    let previous: Option<Json<Value>> =
      sqlx::query_scalar("SELECT document FROM project_text_preparations WHERE id=$1 FOR UPDATE")
        .bind(proposed.id.as_bytes().as_slice())
        .fetch_optional(&mut *tx)
        .await
        .map_err(unavailable)?;
    if let Some(previous) = previous {
      let original = decode(previous)?;
      if !original.matches_submission(&request.submission) {
        return Err(conflict());
      }
      if original.published_at.is_some() {
        return Ok(original);
      }
      let next = original.reclaim(request.now)?;
      save(&mut tx, &next).await?;
      tx.commit().await.map_err(unavailable)?;
      return Ok(next);
    }
    sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM projects WHERE id=$1 FOR KEY SHARE")
      .bind(request.submission.project_id.as_uuid())
      .fetch_optional(&mut *tx)
      .await
      .map_err(unavailable)?
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Project,
      })?;
    sqlx::query("INSERT INTO project_text_preparations(id,project_id,security_scope,replay_key,document,cleanup_at_millis) VALUES ($1,$2,$3,$4,$5,$6)").bind(proposed.id.as_bytes().as_slice()).bind(proposed.intent.project_id.as_uuid()).bind(proposed.security_scope.as_str()).bind(proposed.intent.idempotency_key.as_str()).bind(Json(&proposed)).bind(proposed.expires_at.unix_millis()).execute(&mut *tx).await.map_err(unavailable)?;
    sqlx::query("INSERT INTO project_text_documents(preparation_id,documents) VALUES ($1,$2)")
      .bind(proposed.id.as_bytes().as_slice())
      .bind(Json(json!({"content":request.submission.content})))
      .execute(&mut *tx)
      .await
      .map_err(unavailable)?;
    for object in &proposed.objects {
      sqlx::query("INSERT INTO project_text_input_objects(artifact_id,preparation_id,upload_id,role) VALUES ($1,$2,$3,'document')").bind(object.artifact.artifact_id().as_uuid()).bind(proposed.id.as_bytes().as_slice()).bind(object.upload_id.as_uuid()).execute(&mut *tx).await.map_err(unavailable)?;
    }
    tx.commit().await.map_err(unavailable)?;
    Ok(proposed)
  }
  async fn require_project_document_owner(&self, guard: ProjectTextGuard) -> Result<(), StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    let record = load(&mut tx, guard.id).await?;
    record.require_owner(&guard)?;
    tx.commit().await.map_err(unavailable)
  }
  async fn release_project_document(&self, guard: ProjectTextGuard) -> Result<(), StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    let mut record = load(&mut tx, guard.id).await?;
    if record.intent.project_id == guard.project_id && record.generation == guard.generation {
      record.lease_until = record.lease_until.min(guard.now.max(record.claimed_at));
      save(&mut tx, &record).await?;
    }
    tx.commit().await.map_err(unavailable)
  }
  async fn publish_project_document(
    &self,
    request: ManagementMutation<ProjectTextGuard>,
  ) -> Result<ProjectDocumentPreparation, StoreError> {
    let (guard, audit) = request.into_parts();
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    let mut record = load(&mut tx, guard.id).await?;
    record.require_owner(&guard)?;
    if record.security_scope != *audit.security_scope() {
      return Err(conflict());
    }
    if record.published_at.is_some() {
      return Ok(record);
    }
    record.published_at = Some(guard.now);
    save(&mut tx, &record).await?;
    let identity = MutationIdentity::new_management(
      &audit,
      MutationKind::PublishProjectDocument,
      record.id.to_string(),
      guard.now,
      EntityKind::Artifact,
      &(&record.intent, &record.objects),
    )?;
    let metadata = json!({"project_id":record.intent.project_id, "artifact":record.resource()?.artifact});
    crate::mutation::commit_fresh(
      tx,
      &identity,
      MutationFacts::management(
        &audit,
        record.resource()?.artifact.artifact_id().to_string(),
        metadata.clone(),
        metadata.clone(),
      ),
      crate::mutation::encode_outcome(&metadata)?,
    )
    .await?;
    Ok(record)
  }
}
