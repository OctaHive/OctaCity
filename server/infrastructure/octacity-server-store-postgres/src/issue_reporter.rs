//! Original Issue desired state is read from its existing atomic mutation publication.
use crate::{PostgresStore, database::unavailable};
use octacity_server_factory::FactoryDigest;
use octacity_server_store::*;
use sqlx::{Postgres, Transaction, types::Json};

pub(crate) async fn read(
  tx: &mut Transaction<'_, Postgres>,
  operation: FactoryDigest,
) -> Result<IssuePushProjection, StoreError> {
  let record = crate::factory_run::latest_outbox(tx, operation)
    .await?
    .ok_or_else(source_conflict)?;
  let FactoryOutboxSubject::Issue(subject) = record.subject else {
    return Err(source_conflict());
  };
  let rows: Vec<(Json<serde_json::Value>, i64)> = sqlx::query_as(
    "SELECT payload, FLOOR(EXTRACT(EPOCH FROM created_at)*1000)::BIGINT FROM outbox_entries \
     WHERE aggregate_identity=$1 AND (payload->'push_intent'->>'version')::bigint=$2 \
       AND payload->'push_intent'->>'project_id'=$3 \
       AND payload->'push_intent'->'desired' IS NOT NULL AND payload->'push_intent'->'desired'<>'null'::jsonb LIMIT 2",
  )
  .bind(subject.issue_id.to_string())
  .bind(subject.version.get() as i64)
  .bind(subject.project_id.to_string())
  .fetch_all(&mut **tx)
  .await
  .map_err(unavailable)?;
  if rows.len() != 1 {
    return Err(source_conflict());
  }
  let (Json(payload), at) = &rows[0];
  let intent: IssuePushIntent =
    serde_json::from_value(payload["push_intent"].clone()).map_err(|_| StoreError::Unavailable)?;
  let accepted_at = octacity_server_domain::Timestamp::from_unix_millis(*at).map_err(|_| StoreError::Unavailable)?;
  let original = intent.projection_record(accepted_at).ok_or_else(source_conflict)?;
  if original.operation_id != operation
    || original.subject != record.subject
    || original.input_digest != record.input_digest
  {
    return Err(source_conflict());
  }
  Ok(IssuePushProjection {
    intent,
    accepted_at,
    record,
  })
}
#[async_trait::async_trait]
impl IssueSourceProjectionStore for PostgresStore {
  async fn issue_push_projection(&self, operation: FactoryDigest) -> Result<IssuePushProjection, StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    let result = read(&mut tx, operation).await?;
    tx.commit().await.map_err(unavailable)?;
    Ok(result)
  }
}

const PENDING_PUBLICATIONS: &str = "SELECT entry.payload->'push_intent', FLOOR(EXTRACT(EPOCH FROM entry.created_at)*1000)::BIGINT, \
   entry.aggregate_identity,(entry.payload->'push_intent'->>'version')::bigint,entry.id FROM outbox_entries entry \
   WHERE entry.payload->'push_intent'->'desired' IS NOT NULL AND entry.payload->'push_intent'->'desired'<>'null'::jsonb \
   AND (entry.aggregate_identity,(entry.payload->'push_intent'->>'version')::bigint,entry.id)>($2,$3,$4) \
   AND NOT EXISTS(SELECT 1 FROM factory_outbox_records effect WHERE effect.issue IS NOT NULL AND effect.issue->>'project_id'=entry.payload->'push_intent'->>'project_id' \
     AND effect.issue->>'issue_id'=entry.aggregate_identity AND effect.issue->>'version'=entry.payload->'push_intent'->>'version') \
   ORDER BY entry.aggregate_identity,(entry.payload->'push_intent'->>'version')::bigint,entry.id LIMIT $1";

// The one-time recovery marker keeps historical scans out of readiness and claims.
// A missing marker is an error, never evidence that recovery is complete.
pub(crate) async fn has_pending_publications(pool: &sqlx::PgPool) -> Result<bool, sqlx::Error> {
  sqlx::query_scalar("SELECT NOT completed FROM issue_reporter_recovery WHERE singleton")
    .fetch_one(pool)
    .await
}

// Resume publication of already frozen 5.1 inputs after schema upgrade. Each batch
// is atomic; legacy intents lacking desired material are deliberately excluded.
pub(crate) async fn recover_publications(pool: &sqlx::PgPool) -> Result<(), StoreError> {
  loop {
    let mut tx = pool.begin().await.map_err(unavailable)?;
    // Serialize migration replicas and commit the cursor with its original effects.
    let (completed, aggregate, version, id): (bool, String, i64, uuid::Uuid) = sqlx::query_as(
      "SELECT completed,last_aggregate,last_version,last_id FROM issue_reporter_recovery WHERE singleton FOR UPDATE",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(unavailable)?;
    if completed {
      return Ok(());
    }
    let rows: Vec<(Json<serde_json::Value>, i64, String, i64, uuid::Uuid)> = sqlx::query_as(PENDING_PUBLICATIONS)
      .bind(i64::from(MAX_FACTORY_OUTBOX_BATCH_SIZE))
      .bind(aggregate)
      .bind(version)
      .bind(id)
      .fetch_all(&mut *tx)
      .await
      .map_err(unavailable)?;
    if rows.is_empty() {
      sqlx::query("UPDATE issue_reporter_recovery SET completed=TRUE WHERE singleton")
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?;
      tx.commit().await.map_err(unavailable)?;
      return Ok(());
    }
    // The cursor follows inspected input, including an already-published concurrent row.
    let (_, _, aggregate, version, id) = rows.last().ok_or(StoreError::Unavailable)?;
    let cursor = (aggregate.clone(), *version, *id);
    for (Json(value), at, _, _, _) in rows {
      let intent: IssuePushIntent = serde_json::from_value(value).map_err(|_| StoreError::Unavailable)?;
      let accepted_at = octacity_server_domain::Timestamp::from_unix_millis(at).map_err(|_| StoreError::Unavailable)?;
      let row = intent.projection_record(accepted_at).ok_or_else(source_conflict)?;
      let item = intent
        .desired
        .as_ref()
        .ok_or_else(source_conflict)?
        .link
        .selection
        .item
        .projection_item();
      crate::factory_source_projection::lock_issue(&mut tx, intent.project_id, &item.as_bytes()).await?;
      if crate::factory_run::latest_outbox(&mut tx, row.operation_id)
        .await?
        .is_some()
      {
        continue;
      }
      // Legacy publication cannot be sequenced after a known newer revision or
      // an unproven producer. Preserve the input without a native write permit.
      let unproven_order: bool = sqlx::query_scalar(
        "SELECT EXISTS( \
           SELECT 1 FROM factory_source_projection_intents i \
           JOIN factory_outbox_records o USING(operation_id) \
           WHERE i.project_id=$1 AND i.issue=$2 AND ( \
             o.run_id IS NOT NULL OR o.issue->>'issue_id'<>$3 \
             OR (o.issue->>'version')::bigint>$4)) \
         OR EXISTS(SELECT 1 FROM issue_source_links l \
           WHERE l.project_id=$1 AND l.issue_id<>$5 AND l.document->'selection'->'item'=$6)",
      )
      .bind(intent.project_id.as_uuid())
      .bind(item.as_bytes().to_vec())
      .bind(intent.issue_id.to_string())
      .bind(intent.version.get() as i64)
      .bind(intent.issue_id.as_uuid())
      .bind(Json(
        &intent.desired.as_ref().ok_or_else(source_conflict)?.link.selection.item,
      ))
      .fetch_one(&mut *tx)
      .await
      .map_err(unavailable)?;
      if !unproven_order {
        crate::factory_source_projection::record_issue_publication(&mut tx, &intent, &row).await?;
      }
      crate::factory_run::insert_outbox(&mut tx, &row).await?;
    }
    sqlx::query("UPDATE issue_reporter_recovery SET last_aggregate=$1,last_version=$2,last_id=$3 WHERE singleton")
      .bind(cursor.0)
      .bind(cursor.1)
      .bind(cursor.2)
      .execute(&mut *tx)
      .await
      .map_err(unavailable)?;
    tx.commit().await.map_err(unavailable)?;
  }
}
