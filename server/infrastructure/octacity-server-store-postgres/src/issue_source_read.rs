//! Narrow persistence of the shared Source scan/fence/progress model, without admission.
use crate::{PostgresStore, database::unavailable};
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_store::*;
use sqlx::PgConnection;

pub(super) async fn load(
  connection: &mut PgConnection,
  project: ProjectId,
  scope: &ManagementSecurityScope,
  target: &IssueSourceReadTarget,
) -> Result<Option<IssueSourceReadScan>, StoreError> {
  let bytes: Option<Vec<u8>> = sqlx::query_scalar(
    "SELECT document FROM issue_source_reads WHERE project_id=$1 AND security_scope=$2 AND target=$3 FOR UPDATE",
  )
  .bind(project.as_uuid())
  .bind(scope.as_str())
  .bind(target.identity(project, scope)?.as_bytes().as_slice())
  .fetch_optional(connection)
  .await
  .map_err(unavailable)?;
  bytes
    .map(|v| serde_json::from_slice(&v).map_err(|_| StoreError::Unavailable))
    .transpose()
}
async fn save(
  connection: &mut PgConnection,
  scope: &ManagementSecurityScope,
  scan: &IssueSourceReadScan,
) -> Result<(), StoreError> {
  sqlx::query("INSERT INTO issue_source_reads(project_id,security_scope,target,document) VALUES($1,$2,$3,$4) ON CONFLICT(project_id,security_scope,target) DO UPDATE SET document=excluded.document")
    .bind(scan.claim.project_id.as_uuid()).bind(scope.as_str()).bind(scan.claim.target.identity(scan.claim.project_id,scope)?.as_bytes().as_slice()).bind(serde_json::to_vec(scan).map_err(|_|StoreError::Unavailable)?).execute(connection).await.map_err(unavailable)?;
  Ok(())
}
#[async_trait::async_trait]
impl IssueSourceReadStore for PostgresStore {
  async fn active_issue_source_reads(
    &self,
    scope: &ManagementSecurityScope,
    after: Option<(ProjectId, octacity_server_domain::IssueId)>,
    limit: u16,
  ) -> Result<Vec<(ProjectId, IssueSourceReadTarget)>, StoreError> {
    if !(1..=MAX_SOURCE_POLL_PAGE_ITEMS).contains(&limit) {
      return Err(source_conflict());
    }
    let rows:Vec<(sqlx::types::Json<IssueSourceLink>,Option<sqlx::types::Json<PublishedIssueSourceMapping>>)> = sqlx::query_as(
      "SELECT l.document,m.document FROM issue_source_links l JOIN issue_source_refresh r USING(project_id,issue_id) LEFT JOIN LATERAL (SELECT document FROM issue_source_mappings WHERE project_id=l.project_id AND issue_id=l.issue_id ORDER BY version DESC LIMIT 1) m ON TRUE WHERE l.security_scope=$1 AND (r.document->>'enabled')::boolean AND l.document->'selection'->'connection' IS NOT NULL AND ($2::uuid IS NULL OR (l.project_id,l.issue_id)>($2,$3)) ORDER BY l.project_id,l.issue_id LIMIT $4"
    ).bind(scope.as_str()).bind(after.map(|v|v.0.as_uuid())).bind(after.map(|v|v.1.as_uuid())).bind(i64::from(limit))
      .fetch_all(self.pool()).await.map_err(unavailable)?;
    Ok(
      rows
        .into_iter()
        .map(|(sqlx::types::Json(link), mapping)| {
          (
            link.selection.project_id,
            IssueSourceReadTarget {
              source: mapping.map_or(link.selection.source, |m| m.0.definition.source),
              issue_id: Some(link.issue_id),
              connection: link.selection.connection,
              import: None,
              pull_generation: 0,
            },
          )
        })
        .collect(),
    )
  }

  async fn claim_issue_source_read(
    &self,
    scope: &ManagementSecurityScope,
    mut request: ClaimWorkSourceScan<IssueSourceReadTarget>,
  ) -> Result<Option<IssueSourceReadScan>, StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    crate::issue_workflow::lock_project(&mut tx, request.project_id).await?;
    // Keep the same journal -> freshness lock order as guarded refresh publication.
    let current = load(&mut tx, request.project_id, scope, &request.target).await?;
    let Some(generation) = read_generation(&mut tx, request.project_id, scope, &request.target).await? else {
      return Ok(None);
    };
    request.target.pull_generation = generation;
    let scan = next_source_scan(request, current.as_ref())?;
    if let Some(scan) = &scan {
      save(&mut tx, scope, scan).await?;
    }
    tx.commit().await.map_err(unavailable)?;
    Ok(scan)
  }
  async fn checkpoint_issue_source_read(
    &self,
    scope: &ManagementSecurityScope,
    claim: IssueSourceReadClaim,
    at: Timestamp,
    expected: Option<WorkSourceProgress>,
    progress: WorkSourceProgress,
  ) -> Result<(), StoreError> {
    update_read(self.pool(), scope, &claim, at, |scan| {
      progress.validate()?;
      if scan.progress != expected {
        return Err(source_conflict());
      }
      scan.progress = Some(progress);
      Ok(())
    })
    .await
  }
  async fn finish_issue_source_read(
    &self,
    scope: &ManagementSecurityScope,
    claim: IssueSourceReadClaim,
    at: Timestamp,
    schedule: WorkSourceSchedule,
  ) -> Result<(), StoreError> {
    update_read(self.pool(), scope, &claim, at, |scan| {
      scan.schedule = Some(schedule);
      scan.claim.until = at;
      Ok(())
    })
    .await
  }
}
async fn update_read(
  pool: &sqlx::PgPool,
  scope: &ManagementSecurityScope,
  claim: &IssueSourceReadClaim,
  at: Timestamp,
  change: impl FnOnce(&mut IssueSourceReadScan) -> Result<(), StoreError> + Send,
) -> Result<(), StoreError> {
  let mut tx = pool.begin().await.map_err(unavailable)?;
  let mut scan = load(&mut tx, claim.project_id, scope, &claim.target)
    .await?
    .ok_or_else(source_conflict)?;
  require_target(&mut tx, scope, claim).await?;
  require_source_scan(&scan, claim, at)?;
  change(&mut scan)?;
  save(&mut tx, scope, &scan).await?;
  tx.commit().await.map_err(unavailable)?;
  Ok(())
}

pub(super) async fn require_claim(
  connection: &mut PgConnection,
  scope: &ManagementSecurityScope,
  claim: &IssueSourceReadClaim,
  selection: &IssueSourceSelection,
  issue: Option<octacity_server_domain::IssueId>,
  at: Timestamp,
) -> Result<(), StoreError> {
  if claim.project_id != selection.project_id
    || claim.target.source != selection.source
    || (claim.target.read_connection().is_some() && claim.target.read_connection() != selection.connection.as_ref())
    || claim.target.issue_id.is_some_and(|id| Some(id) != issue)
  {
    return Err(source_conflict());
  }
  let scan = load(connection, claim.project_id, scope, &claim.target)
    .await?
    .ok_or_else(source_conflict)?;
  require_target(connection, scope, claim).await?;
  require_source_scan(&scan, claim, at)
}

async fn read_generation(
  connection: &mut PgConnection,
  project: ProjectId,
  scope: &ManagementSecurityScope,
  target: &IssueSourceReadTarget,
) -> Result<Option<u64>, StoreError> {
  use sqlx::types::Json;
  if let Some(import) = &target.import {
    if target.connection.is_some()
      || target.issue_id.is_some()
      || import.project_id != project
      || import.definition.source != target.source
    {
      return Err(source_conflict());
    }
    let current = crate::issue_source_import::read(connection, project, scope, import.id, None).await?;
    return Ok((current == *import && current.definition.enabled).then_some(current.version.get()));
  }
  let Some(issue) = target.issue_id else {
    if target.connection.is_some() {
      return Err(source_conflict());
    }
    return Ok(Some(0));
  };
  let (Json(link),Json(state)):(Json<IssueSourceLink>,Json<IssueSourceRefreshState>)=sqlx::query_as("SELECT l.document,r.document FROM issue_source_links l JOIN issue_source_refresh r USING(project_id,issue_id) WHERE l.project_id=$1 AND l.issue_id=$2 AND l.security_scope=$3 FOR SHARE OF r").bind(project.as_uuid()).bind(issue.as_uuid()).bind(scope.as_str()).fetch_optional(connection).await.map_err(unavailable)?.ok_or_else(source_conflict)?;
  if link.selection.connection != target.connection
    || link.selection.source.installation.identity() != target.source.installation.identity()
  {
    return Err(source_conflict());
  }
  Ok(state.enabled.then_some(state.generation))
}
async fn require_target(
  connection: &mut PgConnection,
  scope: &ManagementSecurityScope,
  claim: &IssueSourceReadClaim,
) -> Result<(), StoreError> {
  if read_generation(connection, claim.project_id, scope, &claim.target).await? != Some(claim.target.pull_generation) {
    return Err(source_conflict());
  }
  Ok(())
}
