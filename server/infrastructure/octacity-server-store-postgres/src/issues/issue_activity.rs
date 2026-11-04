//! Derived activity reads existing metadata under a read-only repeatable observation.
use crate::{PostgresStore, database::unavailable};
use octacity_server_domain::{EntityKind, IssueVersion};
use octacity_server_factory::FactoryDigest;
use octacity_server_store::*;
use sqlx::{PgConnection, types::Json};

#[derive(sqlx::FromRow)]
struct SourceActivityRow {
  native: Option<Vec<u8>>,
  membership: Vec<u8>,
  generation: i64,
  mapping: i64,
  rows: Json<Vec<IssueTimelineReference>>,
}
#[async_trait::async_trait]
impl IssueTimelineStore for PostgresStore {
  async fn issue_timeline(&self, request: ReadIssueTimeline) -> Result<IssueTimelineReadPage, StoreError> {
    request.validate()?;
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
      .execute(&mut *tx)
      .await
      .map_err(unavailable)?;
    let (version,discussion,executions):(i64,i64,i64)=sqlx::query_as("SELECT current_version,discussion_count,(SELECT COALESCE(max(sequence),0) FROM issue_execution_requests WHERE project_id=$1 AND issue_id=$2) FROM issues WHERE project_id=$1 AND id=$2")
      .bind(request.project_id.as_uuid()).bind(request.issue_id.as_uuid()).fetch_optional(&mut *tx).await.map_err(unavailable)?.ok_or(StoreError::NotFound{entity:EntityKind::Issue})?;
    let current_version = IssueVersion::new(version as u64).map_err(|_| StoreError::Unavailable)?;
    request.validate_boundary(current_version, executions as u64, discussion as u64)?;
    sqlx::query("SAVEPOINT source_section")
      .execute(&mut *tx)
      .await
      .map_err(unavailable)?;
    let source = match source_section(&mut tx, &request).await {
      Ok(source) => Some(source),
      Err(_) => {
        sqlx::query("ROLLBACK TO SAVEPOINT source_section")
          .execute(&mut *tx)
          .await
          .map_err(unavailable)?;
        None
      }
    };
    sqlx::query("RELEASE SAVEPOINT source_section")
      .execute(&mut *tx)
      .await
      .map_err(unavailable)?;
    let boundary = request.boundary.unwrap_or(IssueTimelineBoundary {
      issue_version: current_version,
      discussion: discussion as u64,
      executions: executions as u64,
      source: source.as_ref().map(|(watermark, _)| *watermark),
    });
    let kinds: Vec<i32> = request.kinds.iter().map(|k| *k as i32).collect();
    let position = request.after.as_ref();
    let rows: Vec<(Json<IssueTimelineReference>,)> = sqlx::query_as(include_str!("issue_activity.sql"))
      .bind(request.project_id.as_uuid())
      .bind(request.issue_id.as_uuid())
      .bind(boundary.issue_version.get() as i64)
      .bind(boundary.executions as i64)
      .bind(boundary.discussion as i64)
      .bind(kinds)
      .bind(position.map(|p| p.at.unix_millis()))
      .bind(position.map(|p| p.kind as i32))
      .bind(position.map(|p| p.identity.as_str()))
      .bind(i64::from(request.limit) + 1)
      .fetch_all(&mut *tx)
      .await
      .map_err(unavailable)?;
    let mut rows: Vec<_> = rows.into_iter().map(|(Json(row),)| row).collect();
    let source_unavailable = source.is_none() || boundary.source.is_none();
    let source_stale = !source_unavailable && boundary.source != source.as_ref().map(|(watermark, _)| *watermark);
    if !source_unavailable && !source_stale {
      rows.extend(source.unwrap().1);
    }
    tx.commit().await.map_err(unavailable)?;
    Ok(IssueTimelineReadPage::bounded(
      rows,
      boundary,
      current_version,
      source_stale,
      source_unavailable,
      &request,
    ))
  }
}
// A failed optional section rolls back to its savepoint without aborting local metadata reads.
async fn source_section(
  connection: &mut PgConnection,
  request: &ReadIssueTimeline,
) -> Result<(FactoryDigest, Vec<IssueTimelineReference>), StoreError> {
  let position = request.after.as_ref();
  let limit = if request.kinds.contains(&IssueTimelineKind::Source) {
    i64::from(request.limit) + 1
  } else {
    0
  };
  let source: SourceActivityRow = sqlx::query_as(include_str!("issue_activity_source.sql"))
    .bind(request.project_id.as_uuid())
    .bind(request.issue_id.as_uuid())
    .bind(request.scope.as_str())
    .bind(position.map(|p| p.at.unix_millis()))
    .bind(position.map(|p| p.kind as i32))
    .bind(position.map(|p| p.identity.as_str()))
    .bind(limit)
    .fetch_one(connection)
    .await
    .map_err(unavailable)?;
  let native = source
    .native
    .map(|bytes| {
      bytes
        .try_into()
        .map(FactoryDigest::from_bytes)
        .map_err(|_| StoreError::Unavailable)
    })
    .transpose()?;
  let membership = FactoryDigest::from_bytes(source.membership.try_into().map_err(|_| StoreError::Unavailable)?);
  let watermark = issue_timeline_source_watermark(native, membership, source.generation as u64, source.mapping as u64)?;
  Ok((watermark, source.rows.0))
}
