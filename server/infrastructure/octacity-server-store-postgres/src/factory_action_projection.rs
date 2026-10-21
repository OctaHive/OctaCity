//! Atomic derived publication, fenced by the authoritative Run version.
use crate::{
  PostgresStore,
  database::{number, unavailable},
  discovery::{positive, timestamp},
  factory_run,
};
use octacity_server_domain::{EntityKind, Timestamp};
use octacity_server_factory::{FactoryRunId, FactoryRunVersion};
use octacity_server_store::{
  FactoryActionProjection, FactoryActionProjectionPage, FactoryActionProjectionQuery, FactoryActionProjectionStore,
  FactoryActionProjectionWork, FactoryActionProjectionWorkBatch, FactoryNodeActionDescription, StoreError,
  StoreOperation, validate_action_projection_limit,
};
use sqlx::{Row, types::Json};

fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::FactoryRun,
  }
}
fn version(value: i64) -> Result<FactoryRunVersion, StoreError> {
  FactoryRunVersion::new(positive(value)?).map_err(|_| StoreError::Unavailable)
}
#[async_trait::async_trait]
impl FactoryActionProjectionStore for PostgresStore {
  async fn factory_action_projection_work(&self, limit: u16) -> Result<FactoryActionProjectionWorkBatch, StoreError> {
    validate_action_projection_limit(limit)?;
    let rows: Vec<(uuid::Uuid, i64)> = sqlx::query_as(
      "SELECT r.id,r.version FROM factory_runs r \
       LEFT JOIN factory_action_projections p ON p.run_id=r.id AND p.source_version=r.version \
       WHERE r.visible AND p.run_id IS NULL ORDER BY r.updated_at,r.id LIMIT $1",
    )
    .bind(i64::from(limit) + 1)
    .fetch_all(self.pool())
    .await
    .map_err(unavailable)?;
    let has_more = rows.len() > usize::from(limit);
    let items = rows
      .into_iter()
      .take(usize::from(limit))
      .map(|(id, v)| {
        Ok(FactoryActionProjectionWork {
          run_id: FactoryRunId::from_uuid(id).map_err(|_| StoreError::Unavailable)?,
          source_version: version(v)?,
        })
      })
      .collect::<Result<_, StoreError>>()?;
    Ok(FactoryActionProjectionWorkBatch { items, has_more })
  }
  async fn refresh_factory_action_projection(
    &self,
    work: FactoryActionProjectionWork,
    at: Timestamp,
  ) -> Result<(), StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    // Same lock order as execution: source authority first, derived rows second.
    let snapshot = factory_run::read_snapshot_in_transaction(&mut tx, work.run_id).await?;
    let previous: Option<i64> =
      sqlx::query_scalar("SELECT observed_at_millis FROM factory_action_projections WHERE run_id=$1")
        .bind(work.run_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(unavailable)?;
    if snapshot.run.version() != work.source_version || previous.is_some_and(|old| old > at.unix_millis()) {
      return Err(conflict());
    }
    let projection = FactoryActionProjection::from_snapshot(&snapshot, at)?;
    sqlx::query(
      "INSERT INTO factory_action_projections(run_id,source_version,observed_at_millis) VALUES($1,$2,$3) \
       ON CONFLICT(run_id) DO UPDATE SET \
         source_version=EXCLUDED.source_version,observed_at_millis=EXCLUDED.observed_at_millis",
    )
    .bind(work.run_id.as_uuid())
    .bind(number(
      work.source_version.get(),
      StoreOperation::ProjectFactoryActions,
    )?)
    .bind(at.unix_millis())
    .execute(&mut *tx)
    .await
    .map_err(unavailable)?;
    sqlx::query("DELETE FROM factory_action_items WHERE run_id=$1")
      .bind(work.run_id.as_uuid())
      .execute(&mut *tx)
      .await
      .map_err(unavailable)?;
    for item in projection.items {
      sqlx::query(
        "INSERT INTO factory_action_items(run_id,node_attempt_id,deadline_millis,description) VALUES($1,$2,$3,$4)",
      )
      .bind(work.run_id.as_uuid())
      .bind(item.node_attempt_id.as_uuid())
      .bind(item.deadline.map(|t| t.unix_millis()))
      .bind(Json(item))
      .execute(&mut *tx)
      .await
      .map_err(unavailable)?;
    }
    tx.commit().await.map_err(unavailable)
  }
  async fn read_factory_action_projection(
    &self,
    query: FactoryActionProjectionQuery,
  ) -> Result<FactoryActionProjectionPage, StoreError> {
    validate_action_projection_limit(query.limit)?;
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    // Lock separately from the join: a waiting READ COMMITTED join may retain an
    // old derived tuple after rechecking an updated source tuple.
    let source: Option<i64> = sqlx::query_scalar("SELECT version FROM factory_runs WHERE id=$1 AND visible FOR SHARE")
      .bind(query.run_id.as_uuid())
      .fetch_optional(&mut *tx)
      .await
      .map_err(unavailable)?;
    let current = version(source.ok_or(StoreError::NotFound {
      entity: EntityKind::FactoryRun,
    })?)?;
    let header: Option<(i64, i64)> =
      sqlx::query_as("SELECT source_version,observed_at_millis FROM factory_action_projections WHERE run_id=$1")
        .bind(query.run_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(unavailable)?;
    let mut projection = None;
    if let Some((v, t)) = header.filter(|(v, t)| *v == current.get() as i64 && *t <= query.observed_at.unix_millis()) {
      let rows = sqlx::query(
        "SELECT node_attempt_id,description FROM factory_action_items \
         WHERE run_id=$1 AND ($2::uuid IS NULL OR node_attempt_id>$2) \
           AND (deadline_millis IS NULL OR deadline_millis >= $3) \
         ORDER BY node_attempt_id LIMIT $4",
      )
      .bind(query.run_id.as_uuid())
      .bind(query.after.map(|id| id.as_uuid()))
      .bind(query.observed_at.unix_millis())
      .bind(i64::from(query.limit) + 1)
      .fetch_all(&mut *tx)
      .await
      .map_err(unavailable)?;
      let items = rows
        .into_iter()
        .map(|row| {
          let Json(item): Json<FactoryNodeActionDescription> = row.try_get("description").map_err(unavailable)?;
          let id: uuid::Uuid = row.try_get("node_attempt_id").map_err(unavailable)?;
          if item.run_id != query.run_id || item.run_version != current || item.node_attempt_id.as_uuid() != id {
            return Err(StoreError::Unavailable);
          }
          Ok(item)
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
      projection = Some(FactoryActionProjection {
        source_version: version(v)?,
        observed_at: timestamp(t)?,
        items,
      });
    }
    let page = FactoryActionProjection::page(projection.as_ref(), current, &query);
    tx.commit().await.map_err(unavailable)?;
    Ok(page)
  }
}
