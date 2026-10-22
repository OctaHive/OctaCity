//! Project inputs use the existing supervised private-input cleanup worker.
use crate::{
  database::unavailable,
  project_text_input::{conflict, input},
};
use octacity_server_domain::Timestamp;
use octacity_server_store::*;
use sqlx::{PgPool, types::Json};

use crate::factory_execution_input::PRIVATE_INPUT_CLEANUP_RECHECK_MILLIS;
// Cleanup needs neither private bodies nor the original admission Flow.
type CleanupRow = (Option<uuid::Uuid>, i64, Json<Vec<FactoryInputObject>>);

pub(crate) async fn expired(
  pool: &PgPool,
  now: Timestamp,
  limit: u16,
) -> Result<Vec<PublishedExecutionInput>, StoreError> {
  if limit == 0 {
    return Ok(vec![]);
  }
  let mut tx = pool.begin().await.map_err(unavailable)?;
  let records: Vec<CleanupRow> = sqlx::query_as(
    r#"
    SELECT p.run_id,(p.document->>'expires_at')::bigint,p.document->'objects'
    FROM project_text_preparations p
    WHERE p.cleanup_at_millis <= $1 AND NOT EXISTS (
      SELECT 1 FROM factory_run_builds l
      JOIN LATERAL (
        SELECT released_at,expired_at,expires_at FROM build_result_retention_holds
        WHERE build_id=l.build_id ORDER BY version DESC LIMIT 1
      ) h ON true
      WHERE l.run_id=p.run_id AND h.released_at IS NULL AND h.expired_at IS NULL
        AND (h.expires_at IS NULL OR h.expires_at>to_timestamp($1::double precision/1000.0))
    )
    ORDER BY p.cleanup_at_millis,p.id LIMIT $2 FOR UPDATE OF p SKIP LOCKED
    "#,
  )
  .bind(now.unix_millis())
  .bind(i64::from(limit))
  .fetch_all(&mut *tx)
  .await
  .map_err(unavailable)?;
  let mut result = vec![];
  for (run, expires, Json(objects)) in records {
    let expires = Timestamp::from_unix_millis(expires).map_err(|_| conflict())?;
    if let Some(run) = run {
      // Serialize cleanup eligibility with ordinary Build hold placement.
      sqlx::query("SELECT b.id FROM builds b JOIN factory_run_builds l ON l.build_id=b.id WHERE l.run_id=$1 ORDER BY b.id FOR UPDATE OF b")
        .bind(run).fetch_all(&mut *tx).await.map_err(unavailable)?;
      let held: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM factory_run_builds l JOIN LATERAL(SELECT released_at,expired_at,expires_at FROM build_result_retention_holds WHERE build_id=l.build_id ORDER BY version DESC LIMIT 1) h ON true WHERE l.run_id=$1 AND h.released_at IS NULL AND h.expired_at IS NULL AND (h.expires_at IS NULL OR h.expires_at>to_timestamp($2::double precision/1000.0)))")
        .bind(run).bind(now.unix_millis()).fetch_one(&mut *tx).await.map_err(unavailable)?;
      if held {
        continue;
      }
    }
    for object in &objects {
      if result.len() == usize::from(limit) {
        break;
      }
      let chosen = sqlx::query("UPDATE project_text_input_objects SET deletion_started_at_millis=COALESCE(deletion_started_at_millis,$2) WHERE artifact_id=$1 AND (recheck_at_millis IS NULL OR recheck_at_millis <= $2)")
        .bind(object.artifact.artifact_id().as_uuid()).bind(now.unix_millis()).execute(&mut *tx).await.map_err(unavailable)?;
      if chosen.rows_affected() == 1 {
        result.push(input(object, expires));
      }
    }
  }
  tx.commit().await.map_err(unavailable)?;
  Ok(result)
}

pub(crate) async fn complete(
  pool: &PgPool,
  request: &CompleteFactoryExecutionInputDeletion,
) -> Result<bool, StoreError> {
  let mut tx = pool.begin().await.map_err(unavailable)?;
  let id: Option<Vec<u8>> =
    sqlx::query_scalar("SELECT preparation_id FROM project_text_input_objects WHERE artifact_id=$1")
      .bind(request.expected.artifact_id.as_uuid())
      .fetch_optional(&mut *tx)
      .await
      .map_err(unavailable)?;
  let Some(id) = id else {
    return Ok(false);
  };
  let (_, expires, Json(objects)): CleanupRow = sqlx::query_as("SELECT run_id,(document->>'expires_at')::bigint,document->'objects' FROM project_text_preparations WHERE id=$1 FOR UPDATE")
    .bind(&id).fetch_one(&mut *tx).await.map_err(unavailable)?;
  let expires = Timestamp::from_unix_millis(expires).map_err(|_| conflict())?;
  let object = objects
    .iter()
    .find(|o| o.artifact.artifact_id() == request.expected.artifact_id)
    .ok_or_else(conflict)?;
  if input(object, expires) != request.expected || request.deleted_at < expires {
    return Err(conflict());
  }
  let recheck = request
    .deleted_at
    .unix_millis()
    .checked_add(PRIVATE_INPUT_CLEANUP_RECHECK_MILLIS)
    .ok_or_else(conflict)?;
  let updated=sqlx::query("UPDATE project_text_input_objects SET deleted_at_millis=COALESCE(deleted_at_millis,$2),recheck_at_millis=GREATEST(COALESCE(recheck_at_millis,$3),$3) WHERE artifact_id=$1 AND deletion_started_at_millis IS NOT NULL")
    .bind(request.expected.artifact_id.as_uuid()).bind(request.deleted_at.unix_millis()).bind(recheck).execute(&mut *tx).await.map_err(unavailable)?;
  if updated.rows_affected() != 1 {
    return Err(conflict());
  }
  // The next due object keeps partially acknowledged pairs immediately retryable.
  sqlx::query("UPDATE project_text_preparations SET cleanup_at_millis=(SELECT MIN(COALESCE(recheck_at_millis,$2)) FROM project_text_input_objects WHERE preparation_id=$1) WHERE id=$1")
    .bind(&id).bind(expires.unix_millis())
    .execute(&mut *tx).await.map_err(unavailable)?;
  // The shared operation is the only retained copy of standalone submitted bodies.
  // Keep its immutable receipt/tombstone, never the expired private document bytes.
  sqlx::query("DELETE FROM project_text_documents WHERE preparation_id=$1 AND NOT EXISTS(SELECT 1 FROM project_text_input_objects o WHERE o.preparation_id=$1 AND o.deleted_at_millis IS NULL)")
    .bind(&id).execute(&mut *tx).await.map_err(unavailable)?;
  tx.commit().await.map_err(unavailable)?;
  Ok(true)
}

pub(crate) async fn started_for_build(
  connection: &mut sqlx::PgConnection,
  build: octacity_server_domain::BuildId,
) -> Result<bool, StoreError> {
  sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM factory_run_builds l JOIN project_text_preparations p ON p.run_id=l.run_id JOIN project_text_input_objects o ON o.preparation_id=p.id WHERE l.build_id=$1 AND o.deletion_started_at_millis IS NOT NULL)")
    .bind(build.as_uuid()).fetch_one(connection).await.map_err(unavailable)
}
