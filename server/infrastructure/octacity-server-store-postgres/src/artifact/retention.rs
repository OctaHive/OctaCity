//! Artifact-specific retention persistence.

use octacity_server_domain::{ArtifactId, BuildId, EntityKind, Timestamp};
use octacity_server_store::{
  ArtifactEvent, ArtifactRecord, ArtifactState, ArtifactUploadRecord, StoreError, StoreOperation,
};
use sqlx::{Postgres, Transaction};

use super::{ArtifactUploadRow, lock_artifact, update_artifact, update_artifact_and_upload};
use crate::database::unavailable;

pub(crate) async fn retention_page(
  transaction: &mut Transaction<'_, Postgres>,
  build_id: BuildId,
  reports: bool,
  limit: u16,
  observed_at: Timestamp,
) -> Result<Vec<ArtifactUploadRecord>, StoreError> {
  let rows = if reports {
    sqlx::query_as::<_, ArtifactUploadRow>(upload_query!(
      "artifact.build_id = $1 AND artifact.artifact_type = 'report' AND NOT build.reports_visible \
       AND artifact.state <> 'deleted' ORDER BY artifact.id LIMIT $2 FOR UPDATE OF upload, artifact"
    ))
    .bind(build_id.as_uuid())
    .bind(i64::from(limit))
    .fetch_all(&mut **transaction)
    .await
  } else {
    sqlx::query_as::<_, ArtifactUploadRow>(upload_query!(
      "artifact.build_id = $1 AND artifact.artifact_type = 'artifact' AND NOT build.artifacts_visible \
       AND artifact.state <> 'deleted' ORDER BY artifact.id LIMIT $2 FOR UPDATE OF upload, artifact"
    ))
    .bind(build_id.as_uuid())
    .bind(i64::from(limit))
    .fetch_all(&mut **transaction)
    .await
  }
  .map_err(unavailable)?;
  let mut uploads = Vec::with_capacity(rows.len());
  for row in rows {
    let mut upload: ArtifactUploadRecord = row.try_into()?;
    let event = match upload.artifact.state() {
      ArtifactState::Published => Some(ArtifactEvent::Expire),
      ArtifactState::Verifying => Some(ArtifactEvent::VerificationFailed),
      ArtifactState::Pending | ArtifactState::Expired => None,
      ArtifactState::Deleted => return Err(StoreError::Unavailable),
    };
    if let Some(event) = event {
      upload.artifact = upload
        .artifact
        .transition(
          upload.artifact.identity(),
          upload.artifact.version(),
          event,
          observed_at,
        )
        .map_err(|_| StoreError::Conflict {
          entity: EntityKind::Artifact,
        })?;
      if event == ArtifactEvent::VerificationFailed {
        update_artifact_and_upload(transaction, &upload, event, StoreOperation::PrepareRetentionWork).await?;
      } else {
        let previous_version = upload
          .artifact
          .version()
          .get()
          .checked_sub(1)
          .ok_or(StoreError::Unavailable)?;
        update_artifact(
          transaction,
          &upload.artifact,
          previous_version,
          StoreOperation::PrepareRetentionWork,
        )
        .await?;
      }
    }
    uploads.push(upload);
  }
  Ok(uploads)
}

pub(crate) async fn complete_retention(
  transaction: &mut Transaction<'_, Postgres>,
  artifact_id: ArtifactId,
  completed_at: Timestamp,
) -> Result<(), StoreError> {
  let current: ArtifactRecord = lock_artifact(transaction, artifact_id).await?.try_into()?;
  if current.state() != ArtifactState::Deleted {
    if !matches!(current.state(), ArtifactState::Pending | ArtifactState::Expired) {
      return Err(StoreError::Conflict {
        entity: EntityKind::Artifact,
      });
    }
    let updated = current
      .transition(
        current.identity(),
        current.version(),
        ArtifactEvent::Delete,
        completed_at,
      )
      .map_err(|_| StoreError::Conflict {
        entity: EntityKind::Artifact,
      })?;
    update_artifact(
      transaction,
      &updated,
      current.version().get(),
      StoreOperation::CompleteRetentionObject,
    )
    .await?;
  }
  sqlx::query("UPDATE artifact_uploads SET state = 'deleted', completed_at = COALESCE(completed_at, to_timestamp($1::double precision / 1000.0)) WHERE artifact_id = $2")
    .bind(completed_at.unix_millis())
    .bind(artifact_id.as_uuid())
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
  Ok(())
}
