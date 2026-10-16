use crate::{
  PostgresStore,
  factory_source::{load_preparation, lock_claim, selected_input},
};
use async_trait::async_trait;
use octacity_server_domain::{ArtifactId, ProjectId, Timestamp};
use octacity_server_factory::FactoryDigest;
use octacity_server_store::*;
use sqlx::{Postgres, Transaction, types::Json};
fn unavailable(_: sqlx::Error) -> StoreError {
  StoreError::Unavailable
}
async fn owner(
  tx: &mut Transaction<'_, Postgres>,
  claim: &WorkSourceScanClaim,
  id: FactoryDigest,
  now: Timestamp,
) -> Result<WorkSourcePreparation, StoreError> {
  lock_claim(tx, claim, now).await?;
  let preparation = load_preparation(tx, claim.project_id, id).await?;
  preparation.require_selected_owner(claim, &selected_input(tx, claim, preparation.input.id()).await?)?;
  Ok(preparation)
}
async fn load(
  tx: &mut Transaction<'_, Postgres>,
  project: ProjectId,
  id: FactoryDigest,
) -> Result<Option<SourceMaterialization>, StoreError> {
  let document: Option<Json<SourceMaterialization>> = sqlx::query_scalar(
    "SELECT document FROM factory_source_materializations WHERE project_id=$1 AND preparation_id=$2",
  )
  .bind(project.as_uuid())
  .bind(id.as_bytes().as_slice())
  .fetch_optional(&mut **tx)
  .await
  .map_err(unavailable)?;
  Ok(document.map(|v| v.0))
}
async fn save(tx: &mut Transaction<'_, Postgres>, manifest: &SourceMaterialization) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO factory_source_materializations
       (preparation_id,project_id,document,expires_at_millis,next_cleanup_at_millis)
     VALUES ($1,$2,$3,$4,$5)
     ON CONFLICT (preparation_id) DO UPDATE
       SET document=EXCLUDED.document, next_cleanup_at_millis=EXCLUDED.next_cleanup_at_millis",
  )
  .bind(manifest.preparation_id.as_bytes().as_slice())
  .bind(manifest.project_id.as_uuid())
  .bind(Json(manifest))
  .bind(manifest.expires_at.unix_millis())
  .bind(manifest.next_cleanup_at.map(|v| v.unix_millis()))
  .execute(&mut **tx)
  .await
  .map_err(unavailable)?;
  Ok(())
}
#[async_trait]
impl WorkSourceArtifactStore for PostgresStore {
  async fn reject_source_preparation(
    &self,
    request: RejectSourcePreparation,
  ) -> Result<SourceMaterialization, StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    let preparation = owner(&mut tx, &request.claim, request.preparation_id, request.now).await?;
    let current = load(&mut tx, request.claim.project_id, request.preparation_id).await?;
    let manifest = request.retain(&preparation, current.as_ref())?;
    save(&mut tx, &manifest).await?;
    tx.commit().await.map_err(unavailable)?;
    Ok(manifest)
  }
  async fn expired_source_preparations(
    &self,
    now: Timestamp,
    after: Option<FactoryDigest>,
    limit: u16,
  ) -> Result<Vec<SourcePreparationCleanup>, StoreError> {
    if !(1..=100).contains(&limit) {
      return Err(source_conflict());
    }
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    // First cleanup and scheduled reconciliation are disjoint indexed selections.
    let rows: Vec<(Vec<u8>, uuid::Uuid, i64)> = sqlx::query_as(
      "SELECT id,project_id,expires FROM (
         SELECT prep.id,prep.project_id,
           LEAST(prep.snapshot_expires_at_millis,
                 COALESCE(m.expires_at_millis,prep.snapshot_expires_at_millis)) AS expires
         FROM factory_source_preparations prep
         LEFT JOIN factory_source_materializations m ON m.preparation_id=prep.id
         WHERE NOT prep.snapshot_cleaned
           AND (prep.snapshot_expires_at_millis<=$1 OR m.expires_at_millis<=$1)
           AND ($2::bytea IS NULL OR prep.id>$2)
         UNION ALL
         SELECT prep.id,prep.project_id,m.expires_at_millis AS expires
         FROM factory_source_materializations m
         JOIN factory_source_preparations prep ON prep.id=m.preparation_id
         WHERE prep.snapshot_cleaned AND m.next_cleanup_at_millis<=$1
           AND ($2::bytea IS NULL OR prep.id>$2)
       ) due ORDER BY id LIMIT $3",
    )
    .bind(now.unix_millis())
    .bind(after.map(|v| v.as_bytes().to_vec()))
    .bind(i64::from(limit))
    .fetch_all(&mut *tx)
    .await
    .map_err(unavailable)?;
    let mut result = vec![];
    for (id, project, expires) in rows {
      let id = FactoryDigest::from_bytes(id.try_into().map_err(|_| StoreError::Unavailable)?);
      let project = ProjectId::from_uuid(project).map_err(|_| StoreError::Unavailable)?;
      let manifest = load(&mut tx, project, id).await?;
      result.push(SourcePreparationCleanup {
        preparation_id: id,
        project_id: project,
        expires_at: Timestamp::from_unix_millis(expires).map_err(|_| StoreError::Unavailable)?,
        cleaned_at: manifest.as_ref().and_then(|m| m.cleaned_at),
        next_cleanup_at: manifest.as_ref().and_then(|m| m.next_cleanup_at),
        objects: manifest.map_or_else(Vec::new, |m| m.objects),
      });
    }
    Ok(result)
  }
  async fn complete_source_preparation_cleanup(
    &self,
    expected: SourcePreparationCleanup,
    now: Timestamp,
    recheck_at: Timestamp,
  ) -> Result<(), StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    crate::factory_source::lock_project(&mut tx, expected.project_id).await?;
    let mut preparation = load_preparation(&mut tx, expected.project_id, expected.preparation_id).await?;
    let mut manifest = load(&mut tx, expected.project_id, expected.preparation_id).await?;
    let current = SourcePreparationCleanup::from_preparation(&preparation, manifest.as_ref());
    if !expected.requires_acknowledgement(&current, now, recheck_at)? {
      return Ok(());
    }
    if preparation.snapshot.is_empty() && current.objects.is_empty() {
      return Ok(());
    }
    if let Some(manifest) = &mut manifest {
      manifest.cleaned_at.get_or_insert(now);
      manifest.next_cleanup_at = (!manifest.objects.is_empty()).then_some(recheck_at);
      save(&mut tx, manifest).await?;
    }
    // A scheduled recheck updates only its schedule, never the frozen preparation again.
    if !preparation.snapshot.is_empty() {
      preparation.snapshot.clear();
      crate::factory_source::save_preparation(&mut tx, &preparation).await?;
      sqlx::query("UPDATE factory_source_preparations SET snapshot_cleaned=TRUE WHERE id=$1")
        .bind(expected.preparation_id.as_bytes().as_slice())
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?;
    }
    tx.commit().await.map_err(unavailable)
  }
  async fn reserve_source_artifacts(
    &self,
    request: ReserveSourceArtifacts,
  ) -> Result<SourceMaterialization, StoreError> {
    request.validate()?;
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    let preparation = owner(&mut tx, &request.claim, request.preparation_id, request.now).await?;
    let current = load(&mut tx, request.claim.project_id, request.preparation_id).await?;
    let manifest = request.retain(&preparation, current.as_ref())?;
    save(&mut tx, &manifest).await?;
    if current.is_none() {
      for object in &manifest.objects {
        sqlx::query(
          "INSERT INTO factory_source_input_objects (artifact_id,upload_id,preparation_id) VALUES ($1,$2,$3)",
        )
        .bind(object.artifact.artifact_id().as_uuid())
        .bind(object.upload_id.as_uuid())
        .bind(manifest.preparation_id.as_bytes().as_slice())
        .execute(&mut *tx)
        .await
        .map_err(|_| source_conflict())?;
      }
    }
    tx.commit().await.map_err(unavailable)?;
    Ok(manifest)
  }
  async fn publish_source_artifacts(
    &self,
    request: PublishSourceArtifacts,
  ) -> Result<SourceMaterialization, StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    owner(&mut tx, &request.claim, request.preparation_id, request.now).await?;
    let manifest = load(&mut tx, request.claim.project_id, request.preparation_id)
      .await?
      .ok_or_else(source_conflict)?
      .publish(&request)?;
    save(&mut tx, &manifest).await?;
    tx.commit().await.map_err(unavailable)?;
    Ok(manifest)
  }
  async fn source_materialization(
    &self,
    project: ProjectId,
    id: FactoryDigest,
  ) -> Result<Option<SourceMaterialization>, StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    load(&mut tx, project, id).await
  }
}
#[async_trait]
impl SourceArtifactQueryStore for PostgresStore {
  async fn source_artifact(
    &self,
    project: ProjectId,
    id: ArtifactId,
  ) -> Result<Option<PublishedSourceArtifact>, StoreError> {
    let document:Option<Json<SourceMaterialization>>=sqlx::query_scalar("SELECT manifest.document FROM factory_source_input_objects AS object JOIN factory_source_materializations AS manifest USING (preparation_id) WHERE object.artifact_id=$1 AND manifest.project_id=$2").bind(id.as_uuid()).bind(project.as_uuid()).fetch_optional(self.pool()).await.map_err(unavailable)?;
    Ok(document.and_then(|v| v.0.artifact(id)))
  }
}

pub(crate) async fn require_admission_inputs(
  tx: &mut Transaction<'_, Postgres>,
  preparation: &WorkSourcePreparation,
  request: &AdmitFactoryWork,
  now: Timestamp,
) -> Result<(), StoreError> {
  if let Some(manifest) = load(tx, preparation.input.project_id(), preparation.id).await? {
    manifest.require_admission(request.work.artifacts(), now)?;
  }
  Ok(())
}

/// Protected execution transport reads private source publication metadata only.
pub(crate) async fn execution_input(
  pool: &sqlx::PgPool,
  id: ArtifactId,
) -> Result<Option<PublishedExecutionInput>, StoreError> {
  let document:Option<Json<SourceMaterialization>>=sqlx::query_scalar("SELECT manifest.document FROM factory_source_input_objects AS object JOIN factory_source_materializations AS manifest USING (preparation_id) WHERE object.artifact_id=$1").bind(id.as_uuid()).fetch_optional(pool).await.map_err(unavailable)?;
  Ok(
    document
      .and_then(|v| v.0.artifact(id))
      .map(|source| PublishedExecutionInput {
        artifact_id: id,
        upload_id: source.object.upload_id,
        size_bytes: source.object.artifact.encoded_size(),
        digest: ArtifactContentDigest::from_bytes(source.object.artifact.content_digest().as_bytes()),
        media_type: source.object.media_type,
        expires_at: Some(source.expires_at),
      }),
  )
}
