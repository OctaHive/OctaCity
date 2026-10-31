//! One durable journal behind the shared provider-independent text publisher.
use crate::{PostgresStore, database::unavailable};
use octacity_server_domain::{ArtifactId, EntityKind, ImmutableRevision, ProjectId};
use octacity_server_store::*;
use serde_json::Value;
use sqlx::{PgConnection, types::Json};

pub(crate) fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::FactoryRun,
  }
}
// Both purposes share a journal key. A different purpose is an immutable replay conflict,
// rather than a retryable SQL decoding failure.
fn decode_work(document: Json<Value>) -> Result<ProjectTextPreparation, StoreError> {
  serde_json::from_value(document.0).map_err(|_| conflict())
}
pub(crate) async fn load(
  connection: &mut PgConnection,
  id: octacity_server_factory::FactoryDigest,
) -> Result<ProjectTextPreparation, StoreError> {
  let document: Option<Json<Value>> =
    sqlx::query_scalar("SELECT document FROM project_text_preparations WHERE id=$1 FOR UPDATE")
      .bind(id.as_bytes().as_slice())
      .fetch_optional(connection)
      .await
      .map_err(unavailable)?;
  decode_work(document.ok_or_else(conflict)?)
}
pub(crate) async fn save(connection: &mut PgConnection, record: &ProjectTextPreparation) -> Result<(), StoreError> {
  sqlx::query("UPDATE project_text_preparations SET document=$2 WHERE id=$1")
    .bind(record.id.as_bytes().as_slice())
    .bind(Json(record))
    .execute(connection)
    .await
    .map_err(unavailable)?;
  Ok(())
}
#[async_trait::async_trait]
impl ProjectInputQueryStore for PostgresStore {
  async fn project_input_artifact(
    &self,
    project: ProjectId,
    artifact: ArtifactId,
  ) -> Result<Option<PublishedProjectInput>, StoreError> {
    published_input(self.pool(), artifact, Some(project)).await
  }
}
#[async_trait::async_trait]
impl ProjectTextInputStore for PostgresStore {
  async fn project_text_preparation(
    &self,
    project: ProjectId,
    scope: &ManagementSecurityScope,
    key: &IdempotencyKey,
  ) -> Result<Option<ProjectTextPreparation>, StoreError> {
    let document: Option<Json<Value>> = sqlx::query_scalar(
      "SELECT document FROM project_text_preparations WHERE project_id=$1 AND security_scope=$2 AND replay_key=$3",
    )
    .bind(project.as_uuid())
    .bind(scope.as_str())
    .bind(key.as_str())
    .fetch_optional(self.pool())
    .await
    .map_err(unavailable)?;
    document.map(decode_work).transpose()
  }
  async fn reserve_project_text(
    &self,
    request: ManagementMutation<ReserveProjectText>,
  ) -> Result<ProjectTextPreparation, StoreError> {
    let (request, audit) = request.into_parts();
    let proposed =
      ProjectTextPreparation::reserve(request.submission.clone(), audit.security_scope().clone(), request.now)?;
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    // A transaction-level lock serializes both first reservation and concurrent claims.
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
      .bind(i64::from_be_bytes(
        proposed.id.as_bytes()[..8].try_into().map_err(|_| conflict())?,
      ))
      .execute(&mut *tx)
      .await
      .map_err(unavailable)?;
    let retained: Option<Json<Value>> =
      sqlx::query_scalar("SELECT document FROM project_text_preparations WHERE id=$1 FOR UPDATE")
        .bind(proposed.id.as_bytes().as_slice())
        .fetch_optional(&mut *tx)
        .await
        .map_err(unavailable)?;
    if let Some(document) = retained {
      let original = decode_work(document)?;
      if !original.matches_submission(&request.submission) {
        return Err(conflict());
      }
      if original.admission.is_some() {
        return Ok(original);
      }
      let next = original.reclaim(request.now)?;
      save(&mut tx, &next).await?;
      tx.commit().await.map_err(unavailable)?;
      return Ok(next);
    }
    sqlx::query(
      "INSERT INTO project_text_preparations(id,project_id,security_scope,replay_key,document,cleanup_at_millis) VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(proposed.id.as_bytes().as_slice())
    .bind(proposed.intent.target.project_id.as_uuid())
    .bind(audit.security_scope().as_str())
    .bind(proposed.intent.idempotency_key.as_str())
    .bind(Json(&proposed))
    .bind(proposed.expires_at.unix_millis())
    .execute(&mut *tx)
    .await
    .map_err(unavailable)?;
    sqlx::query("INSERT INTO project_text_documents(preparation_id,documents) VALUES ($1,$2)")
      .bind(proposed.id.as_bytes().as_slice())
      .bind(Json(&request.submission.documents))
      .execute(&mut *tx)
      .await
      .map_err(unavailable)?;
    for object in &proposed.objects {
      sqlx::query(
        "INSERT INTO project_text_input_objects(artifact_id,preparation_id,upload_id,role) VALUES ($1,$2,$3,$4)",
      )
      .bind(object.artifact.artifact_id().as_uuid())
      .bind(proposed.id.as_bytes().as_slice())
      .bind(object.upload_id.as_uuid())
      .bind(match object.role {
        FactoryInputRole::Task => "task",
        FactoryInputRole::Acceptance => "acceptance",
        FactoryInputRole::Document => "document",
      })
      .execute(&mut *tx)
      .await
      .map_err(unavailable)?;
    }
    let identity = crate::mutation::MutationIdentity::new_management(
      &audit,
      crate::mutation::MutationKind::ReserveProjectTextInput,
      proposed.id.to_string(),
      request.now,
      EntityKind::FactoryRun,
      &request.submission,
    )?;
    let metadata = serde_json::json!({"project_id":proposed.intent.target.project_id,"preparation_id":proposed.id,
      "artifacts":proposed.objects.iter().map(|o|o.artifact.artifact_id()).collect::<Vec<_>>()});
    crate::mutation::commit_fresh(
      tx,
      &identity,
      crate::mutation::MutationFacts::management(
        &audit,
        proposed.intent.target.project_id.to_string(),
        metadata.clone(),
        metadata.clone(),
      ),
      crate::mutation::encode_outcome(&metadata)?,
    )
    .await?;
    Ok(proposed)
  }
  async fn require_project_text_owner(&self, guard: ProjectTextGuard) -> Result<(), StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    load(&mut tx, guard.id).await?.require_owner(&guard)
  }
  async fn project_text_documents(
    &self,
    project: ProjectId,
    scope: &ManagementSecurityScope,
    key: &IdempotencyKey,
  ) -> Result<Option<ProjectTextDocuments>, StoreError> {
    let documents: Option<Json<ProjectTextDocuments>> = sqlx::query_scalar(
      "SELECT d.documents FROM project_text_documents d JOIN project_text_preparations p ON p.id=d.preparation_id WHERE p.project_id=$1 AND p.security_scope=$2 AND p.replay_key=$3")
      .bind(project.as_uuid()).bind(scope.as_str()).bind(key.as_str())
      .fetch_optional(self.pool()).await.map_err(unavailable)?;
    Ok(documents.map(|d| d.0))
  }
  async fn release_project_text(&self, guard: ProjectTextGuard) -> Result<(), StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    let mut record = load(&mut tx, guard.id).await?;
    if record.generation == guard.generation && record.intent.target.project_id == guard.project_id {
      record.lease_until = record.lease_until.min(guard.now.max(record.claimed_at));
      save(&mut tx, &record).await?;
    }
    tx.commit().await.map_err(unavailable)
  }
  async fn freeze_project_text(
    &self,
    guard: ProjectTextGuard,
    revision: ImmutableRevision,
  ) -> Result<ProjectTextPreparation, StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    let mut record = load(&mut tx, guard.id).await?;
    record.require_owner(&guard)?;
    if record.frozen_revision.as_ref().is_some_and(|r| r != &revision) {
      return Err(conflict());
    }
    record.frozen_revision.get_or_insert(revision);
    save(&mut tx, &record).await?;
    tx.commit().await.map_err(unavailable)?;
    Ok(record)
  }
  async fn publish_project_text(&self, guard: ProjectTextGuard) -> Result<ProjectTextPreparation, StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    let mut record = load(&mut tx, guard.id).await?;
    record.require_owner(&guard)?;
    if record.frozen_revision.is_none() {
      return Err(conflict());
    }
    record.published_at.get_or_insert(guard.now);
    save(&mut tx, &record).await?;
    tx.commit().await.map_err(unavailable)?;
    Ok(record)
  }
  async fn admit_project_text(
    &self,
    guard: ProjectTextGuard,
    request: ManagementMutation<AdmitFactoryWork>,
  ) -> Result<FactoryAdmissionMutationOutcome, StoreError> {
    crate::factory_admission::admit_project_text(self.pool(), guard, request).await
  }
}

pub(crate) async fn correlate_replay(
  pool: &sqlx::PgPool,
  guard: &ProjectTextGuard,
  request: &AdmitFactoryWork,
  audit: &MutationAuditContext,
  admission: &PublishedFactoryAdmission,
) -> Result<(), StoreError> {
  let mut tx = pool.begin().await.map_err(unavailable)?;
  let mut record = load(&mut tx, guard.id).await?;
  record.validate_admission(guard, &ManagementMutation::new(request.clone(), audit.clone()))?;
  // A concurrent ordinary observer may have won; compare retained immutable input.
  if admission.work.external_identity() != &record.intent.external_identity
    || admission.work.artifacts() != &record.work_artifacts()?
    || record.frozen_revision.as_ref()
      != Some(
        admission
          .work
          .subject()
          .repository()
          .map_err(|_| conflict())?
          .base_revision(),
      )
  {
    return Err(conflict());
  }
  record.admission = Some(admission.clone());
  save(&mut tx, &record).await?;
  tx.commit().await.map_err(unavailable)
}

pub(crate) async fn execution_input(
  pool: &sqlx::PgPool,
  artifact: ArtifactId,
) -> Result<Option<PublishedExecutionInput>, StoreError> {
  Ok(
    published_input(pool, artifact, None)
      .await?
      .map(|p| input(&p.object, p.expires_at)),
  )
}

// Project reads and protected transfer select only publication metadata, never the admission Flow.
async fn published_input(
  pool: &sqlx::PgPool,
  artifact: ArtifactId,
  project: Option<ProjectId>,
) -> Result<Option<PublishedProjectInput>, StoreError> {
  type Row = (
    uuid::Uuid,
    Vec<u8>,
    Json<octacity_server_factory::FactoryKey>,
    Json<FactoryInputObject>,
    i64,
    i64,
  );
  let row: Option<Row> = sqlx::query_as(
    r#"
    SELECT p.project_id,p.id,p.document->'intent'->'mapping',entry.object,
      (p.document->>'published_at')::bigint,(p.document->>'expires_at')::bigint
    FROM project_text_input_objects o JOIN project_text_preparations p ON p.id=o.preparation_id
    JOIN LATERAL jsonb_array_elements(p.document->'objects') entry(object)
      ON entry.object->'artifact'->>'artifact_id'=o.artifact_id::text
    WHERE o.artifact_id=$1 AND ($2::uuid IS NULL OR p.project_id=$2)
      AND o.deleted_at_millis IS NULL AND p.document->>'published_at' IS NOT NULL
    "#,
  )
  .bind(artifact.as_uuid())
  .bind(project.map(|p| p.as_uuid()))
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?;
  row
    .map(|(project, id, Json(mapping), Json(object), published, expires)| {
      Ok(PublishedProjectInput {
        project_id: ProjectId::from_uuid(project).map_err(|_| conflict())?,
        preparation_id: octacity_server_factory::FactoryDigest::from_bytes(id.try_into().map_err(|_| conflict())?),
        mapping,
        object,
        published_at: octacity_server_domain::Timestamp::from_unix_millis(published).map_err(|_| conflict())?,
        expires_at: octacity_server_domain::Timestamp::from_unix_millis(expires).map_err(|_| conflict())?,
      })
    })
    .transpose()
}

pub(crate) fn input(o: &FactoryInputObject, expires_at: octacity_server_domain::Timestamp) -> PublishedExecutionInput {
  PublishedExecutionInput {
    artifact_id: o.artifact.artifact_id(),
    upload_id: o.upload_id,
    size_bytes: o.artifact.encoded_size(),
    digest: ArtifactContentDigest::from_bytes(o.artifact.content_digest().as_bytes()),
    media_type: o.media_type.clone(),
    expires_at: Some(expires_at),
  }
}
