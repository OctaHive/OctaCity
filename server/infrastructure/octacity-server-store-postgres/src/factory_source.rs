use crate::PostgresStore;
use async_trait::async_trait;
use octacity_server_domain::ProjectId;
use octacity_server_factory::{
  WorkSourceGroup, WorkSourceGroupId, WorkSourceGroupVersion, WorkSourceInput, WorkSourceInputId,
  WorkSourceInputVersion,
};
use octacity_server_store::{
  ClaimWorkSourceScan, WorkSourceProgress, WorkSourceScan, WorkSourceScanClaim, WorkSourceScanTarget, next_source_scan,
  require_source_scan,
};
use octacity_server_store::{
  FactorySourceStore, PublishWorkSources, StoreError, source_conflict, validate_source_input_dependencies,
};
use octacity_server_store::{ReserveWorkSourcePreparation, WorkSourcePreparation};
use sqlx::{Postgres, Transaction, types::Json};

#[async_trait]
impl FactorySourceStore for PostgresStore {
  async fn coordinate_source_projection(
    &self,
    request: octacity_server_store::SourceProjectionRequest,
  ) -> Result<octacity_server_store::SourceProjectionState, StoreError> {
    crate::factory_source_projection::coordinate(self, request).await
  }
  async fn source_preparation_for_work(
    &self,
    project: ProjectId,
    work: octacity_server_factory::WorkEnvelopeId,
  ) -> Result<Option<WorkSourcePreparation>, StoreError> {
    let documents: Vec<Vec<u8>> = sqlx::query_scalar(
      "SELECT document FROM factory_source_preparations WHERE project_id=$1 AND work_id=$2 AND settled LIMIT 2",
    )
    .bind(project.as_uuid())
    .bind(work.as_uuid())
    .fetch_all(self.pool())
    .await
    .map_err(unavailable)?;
    if documents.len() > 1 {
      return Err(source_conflict());
    }
    documents
      .first()
      .map(|bytes| serde_json::from_slice(bytes).map_err(|_| StoreError::Unavailable))
      .transpose()
  }
  async fn source_issue_correlations(
    &self,
    project: ProjectId,
    issue: octacity_server_factory::FactoryDigest,
    after: Option<WorkSourceInputId>,
    limit: u16,
  ) -> Result<Vec<octacity_server_store::WorkSourceIssueAssociation>, StoreError> {
    if !(1..=64).contains(&limit) {
      return Err(source_conflict());
    }
    let rows: Vec<(uuid::Uuid,i64,uuid::Uuid,uuid::Uuid)> = sqlx::query_as("SELECT input_id,input_version,work_id,run_id FROM factory_source_preparations WHERE project_id=$1 AND issue=$2 AND settled AND ($3::uuid IS NULL OR input_id>$3) ORDER BY input_id LIMIT $4")
      .bind(project.as_uuid()).bind(issue.as_bytes().as_slice()).bind(after.map(|v| v.as_uuid())).bind(i64::from(limit)).fetch_all(self.pool()).await.map_err(unavailable)?;
    rows
      .into_iter()
      .map(|(id, version, work, run)| {
        Ok(octacity_server_store::WorkSourceIssueAssociation {
          input_id: WorkSourceInputId::from_uuid(id).map_err(|_| StoreError::Unavailable)?,
          input_version: WorkSourceInputVersion::new(u64::try_from(version).map_err(|_| StoreError::Unavailable)?)
            .map_err(|_| StoreError::Unavailable)?,
          admission: octacity_server_store::WorkSourceCorrelation {
            work_id: octacity_server_factory::WorkEnvelopeId::from_uuid(work).map_err(|_| StoreError::Unavailable)?,
            run_id: octacity_server_factory::FactoryRunId::from_uuid(run).map_err(|_| StoreError::Unavailable)?,
          },
        })
      })
      .collect()
  }
  async fn admit_prepared_source_work(
    &self,
    guard: octacity_server_store::AdmitPreparedWorkSource,
    request: octacity_server_store::ManagementMutation<octacity_server_store::AdmitFactoryWork>,
  ) -> Result<octacity_server_store::FactoryAdmissionMutationOutcome, StoreError> {
    crate::factory_admission::admit_prepared(self.pool(), guard, request).await
  }
  async fn pending_source_preparations(
    &self,
    project: ProjectId,
    target: WorkSourceScanTarget,
    after: Option<octacity_server_factory::FactoryDigest>,
    limit: u16,
  ) -> Result<Vec<WorkSourcePreparation>, StoreError> {
    if !(1..=64).contains(&limit) {
      return Err(source_conflict());
    }
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    lock_project(&mut tx, project).await?;
    require_current(&mut tx, project, target).await?;
    let pending = pending_preparations(&mut tx, project, target, after, limit).await?;
    tx.commit().await.map_err(unavailable)?;
    Ok(pending)
  }
  async fn publish_source_preparation_intent(
    &self,
    request: octacity_server_store::PublishSourcePreparationIntent,
  ) -> Result<WorkSourcePreparation, StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    lock_claim(&mut tx, &request.claim, request.now).await?;
    let preparation = load_preparation(&mut tx, request.claim.project_id, request.preparation_id).await?;
    preparation.require_selected_owner(
      &request.claim,
      &selected_input(&mut tx, &request.claim, preparation.input.id()).await?,
    )?;
    let retained = preparation.with_intent(request)?;
    save_preparation(&mut tx, &retained).await?;
    tx.commit().await.map_err(unavailable)?;
    Ok(retained)
  }
  async fn reserve_source_preparation(
    &self,
    request: ReserveWorkSourcePreparation,
  ) -> Result<WorkSourcePreparation, StoreError> {
    request.validate()?;
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    lock_claim(&mut tx, &request.claim, request.now).await?;
    let input = selected_input(&mut tx, &request.claim, request.input_id).await?;
    let preparation = request.freeze(input)?;
    let (kind, id) = preparation.target.identity();
    sqlx::query("INSERT INTO factory_source_preparations (id,project_id,input_id,input_version,scan_kind,scan_id,document,issue,snapshot_expires_at_millis) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT (id) DO NOTHING")
      .bind(preparation.id.as_bytes().as_slice()).bind(preparation.input.project_id().as_uuid()).bind(preparation.input.id().as_uuid()).bind(number(preparation.input.version().get())?).bind(kind).bind(id).bind(serde_json::to_vec(&preparation).map_err(|_| StoreError::Unavailable)?)
      .bind(preparation.issue.as_bytes().as_slice()).bind(preparation.snapshot_expires_at.unix_millis()).execute(&mut *tx).await.map_err(unavailable)?;
    let retained = load_preparation(&mut tx, preparation.input.project_id(), preparation.id).await?;
    tx.commit().await.map_err(unavailable)?;
    Ok(retained)
  }
  async fn source_preparation(
    &self,
    project: ProjectId,
    id: octacity_server_factory::FactoryDigest,
  ) -> Result<WorkSourcePreparation, StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    load_preparation(&mut tx, project, id).await
  }
  async fn source_scan_members(
    &self,
    claim: WorkSourceScanClaim,
    now: octacity_server_domain::Timestamp,
  ) -> Result<Vec<WorkSourceInput>, StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    lock_claim(&mut tx, &claim, now).await?;
    scan_members(&mut tx, claim.project_id, claim.target).await
  }
  async fn select_source_input(
    &self,
    claim: WorkSourceScanClaim,
    now: octacity_server_domain::Timestamp,
    item: octacity_server_store::WorkSourceItem,
    matched: Vec<WorkSourceInputId>,
  ) -> Result<octacity_server_factory::WorkSourceSelection, StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    let mut scan = lock_claim(&mut tx, &claim, now).await?;
    let members = scan_members(&mut tx, claim.project_id, claim.target).await?;
    let result = octacity_server_store::record_source_selection(&mut scan, &claim, now, members, item, matched)?;
    save_scan(&mut tx, &scan).await?;
    tx.commit().await.map_err(unavailable)?;
    Ok(result)
  }
  async fn finish_source_scan(
    &self,
    claim: WorkSourceScanClaim,
    now: octacity_server_domain::Timestamp,
    schedule: octacity_server_store::WorkSourceSchedule,
  ) -> Result<(), StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    let mut scan = lock_claim(&mut tx, &claim, now).await?;
    scan.schedule = Some(schedule);
    scan.claim.until = now;
    save_scan(&mut tx, &scan).await?;
    tx.commit().await.map_err(unavailable)
  }
  async fn source_scan_schedule(
    &self,
    project: ProjectId,
    target: WorkSourceScanTarget,
  ) -> Result<Option<octacity_server_store::WorkSourceSchedule>, StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    let scan = load_scan(&mut tx, target).await?;
    Ok(
      scan
        .filter(|s| s.claim.project_id == project && s.claim.target == target)
        .and_then(|s| s.schedule),
    )
  }
  async fn source_scan_targets(
    &self,
    project: ProjectId,
    after: Option<WorkSourceScanTarget>,
    limit: u16,
  ) -> Result<Vec<WorkSourceScanTarget>, StoreError> {
    if !(1..=64).contains(&limit) {
      return Err(source_conflict());
    }
    let cursor = after.map(|v| v.identity());
    let rows:Vec<(String,uuid::Uuid,i64)>=sqlx::query_as(r#"
      WITH inputs AS (SELECT DISTINCT ON(id) id,version,document FROM factory_source_inputs WHERE project_id=$1 ORDER BY id,version DESC),
      groups AS (SELECT DISTINCT ON(id) id,version,document FROM factory_source_groups WHERE project_id=$1 ORDER BY id,version DESC),
      targets AS (
        SELECT 'input'::text AS kind,i.id,i.version FROM inputs i WHERE (i.document#>>'{definition,enabled}')::boolean
          AND NOT EXISTS(SELECT 1 FROM groups g,jsonb_array_elements(g.document->'members') m WHERE (m->>'id')::uuid=i.id)
        UNION ALL
        SELECT 'group'::text,g.id,g.version FROM groups g
          WHERE EXISTS(SELECT 1 FROM jsonb_array_elements(g.document->'members') m WHERE (m#>>'{definition,enabled}')::boolean)
          AND NOT EXISTS(SELECT 1 FROM jsonb_array_elements(g.document->'members') m WHERE NOT EXISTS(SELECT 1 FROM inputs i WHERE i.id=(m->>'id')::uuid AND i.document=m))
      ) SELECT kind,id,version FROM targets WHERE ($2::text IS NULL OR (kind,id)>($2,$3::uuid)) ORDER BY kind,id LIMIT $4
    "#).bind(project.as_uuid()).bind(cursor.map(|v|v.0)).bind(cursor.map(|v|v.1)).bind(i64::from(limit)).fetch_all(self.pool()).await.map_err(unavailable)?;
    rows
      .into_iter()
      .map(|(kind, id, version)| {
        let version = u64::try_from(version).map_err(|_| StoreError::Unavailable)?;
        match kind.as_str() {
          "input" => Ok(WorkSourceScanTarget::Input {
            id: WorkSourceInputId::from_uuid(id).map_err(|_| StoreError::Unavailable)?,
            version: WorkSourceInputVersion::new(version).map_err(|_| StoreError::Unavailable)?,
          }),
          "group" => Ok(WorkSourceScanTarget::Group {
            id: WorkSourceGroupId::from_uuid(id).map_err(|_| StoreError::Unavailable)?,
            version: WorkSourceGroupVersion::new(version).map_err(|_| StoreError::Unavailable)?,
          }),
          _ => Err(StoreError::Unavailable),
        }
      })
      .collect()
  }
  async fn claim_source_scan(&self, request: ClaimWorkSourceScan) -> Result<Option<WorkSourceScan>, StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    lock_project(&mut tx, request.project_id).await?;
    require_current(&mut tx, request.project_id, request.target).await?;
    let current = load_scan(&mut tx, request.target).await?;
    let next = next_source_scan(request, current.as_ref())?;
    if let Some(scan) = &next {
      save_scan(&mut tx, scan).await?;
    }
    tx.commit().await.map_err(unavailable)?;
    Ok(next)
  }
  async fn checkpoint_source_scan(
    &self,
    claim: WorkSourceScanClaim,
    now: octacity_server_domain::Timestamp,
    expected: Option<WorkSourceProgress>,
    progress: WorkSourceProgress,
  ) -> Result<(), StoreError> {
    progress.validate()?;
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    let mut scan = lock_claim(&mut tx, &claim, now).await?;
    if !pending_preparations(&mut tx, claim.project_id, claim.target, None, 1)
      .await?
      .is_empty()
    {
      return Err(source_conflict());
    }
    if scan.progress != expected {
      return Err(source_conflict());
    }
    scan.progress = Some(progress);
    save_scan(&mut tx, &scan).await?;
    tx.commit().await.map_err(unavailable)
  }
  async fn expire_source_poll_pages(
    &self,
    now: octacity_server_domain::Timestamp,
    limit: u16,
  ) -> Result<usize, StoreError> {
    if !(1..=64).contains(&limit) {
      return Err(source_conflict());
    }
    let candidates:Vec<(uuid::Uuid,String,uuid::Uuid)>=sqlx::query_as("SELECT project_id,kind,id FROM factory_source_scans WHERE (document#>>'{progress,polling,pending_page,expires_at}')::bigint <= $1 ORDER BY (document#>>'{progress,polling,pending_page,expires_at}')::bigint,kind,id LIMIT $2")
      .bind(now.unix_millis()).bind(i64::from(limit)).fetch_all(self.pool()).await.map_err(unavailable)?;
    let mut count = 0;
    for (project, kind, id) in candidates {
      let mut tx = self.pool().begin().await.map_err(unavailable)?;
      // Share the writer's Project lock, then recheck expiry: cleanup cannot resurrect stale pages.
      lock_project(
        &mut tx,
        ProjectId::from_uuid(project).map_err(|_| StoreError::Unavailable)?,
      )
      .await?;
      count+=sqlx::query("UPDATE factory_source_scans SET document=jsonb_set(document,'{progress}','null'::jsonb) WHERE kind=$1 AND id=$2 AND (document#>>'{progress,polling,pending_page,expires_at}')::bigint <= $3")
        .bind(kind).bind(id).bind(now.unix_millis()).execute(&mut *tx).await.map_err(unavailable)?.rows_affected() as usize;
      tx.commit().await.map_err(unavailable)?;
    }
    Ok(count)
  }
  async fn publish_work_sources(&self, request: PublishWorkSources) -> Result<(), StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    publish_in(&mut tx, request).await?;
    tx.commit().await.map_err(unavailable)
  }
  async fn source_input(
    &self,
    project: ProjectId,
    id: WorkSourceInputId,
    version: WorkSourceInputVersion,
  ) -> Result<WorkSourceInput, StoreError> {
    let document: Option<Json<WorkSourceInput>> =
      sqlx::query_scalar("SELECT document FROM factory_source_inputs WHERE project_id=$1 AND id=$2 AND version=$3")
        .bind(project.as_uuid())
        .bind(id.as_uuid())
        .bind(number(version.get())?)
        .fetch_optional(self.pool())
        .await
        .map_err(unavailable)?;
    document.map(|v| v.0).ok_or_else(source_conflict)
  }
  async fn source_group(
    &self,
    project: ProjectId,
    id: WorkSourceGroupId,
    version: WorkSourceGroupVersion,
  ) -> Result<WorkSourceGroup, StoreError> {
    let document: Option<Json<WorkSourceGroup>> =
      sqlx::query_scalar("SELECT document FROM factory_source_groups WHERE project_id=$1 AND id=$2 AND version=$3")
        .bind(project.as_uuid())
        .bind(id.as_uuid())
        .bind(number(version.get())?)
        .fetch_optional(self.pool())
        .await
        .map_err(unavailable)?;
    document.map(|v| v.0).ok_or_else(source_conflict)
  }
}

pub(super) async fn lock_project(tx: &mut Transaction<'_, Postgres>, project: ProjectId) -> Result<(), StoreError> {
  let exists: Option<uuid::Uuid> = sqlx::query_scalar("SELECT id FROM projects WHERE id=$1 FOR UPDATE")
    .bind(project.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(unavailable)?;
  exists.map(|_| ()).ok_or_else(source_conflict)
}
fn number(value: u64) -> Result<i64, StoreError> {
  i64::try_from(value).map_err(|_| source_conflict())
}
fn unavailable(_: sqlx::Error) -> StoreError {
  StoreError::Unavailable
}

// Callers hold the Project lock and validate the current target/claim before using these members.
async fn scan_members(
  tx: &mut Transaction<'_, Postgres>,
  project: ProjectId,
  target: WorkSourceScanTarget,
) -> Result<Vec<WorkSourceInput>, StoreError> {
  match target {
    WorkSourceScanTarget::Input { id, .. } => Ok(vec![current_input(tx, project, id).await?]),
    WorkSourceScanTarget::Group { id, version } => {
      let group: Json<WorkSourceGroup> =
        sqlx::query_scalar("SELECT document FROM factory_source_groups WHERE project_id=$1 AND id=$2 AND version=$3")
          .bind(project.as_uuid())
          .bind(id.as_uuid())
          .bind(number(version.get())?)
          .fetch_one(&mut **tx)
          .await
          .map_err(unavailable)?;
      Ok(group.0.members().to_vec())
    }
  }
}

// Recovery and checkpointing use the same pending predicate. Retired members remain for retention.
async fn pending_preparations(
  tx: &mut Transaction<'_, Postgres>,
  project: ProjectId,
  target: WorkSourceScanTarget,
  after: Option<octacity_server_factory::FactoryDigest>,
  limit: u16,
) -> Result<Vec<WorkSourcePreparation>, StoreError> {
  let members = scan_members(tx, project, target)
    .await?
    .into_iter()
    .filter(|m| m.definition().enabled)
    .map(|m| m.id().as_uuid())
    .collect::<Vec<_>>();
  let documents: Vec<Vec<u8>> = sqlx::query_scalar("SELECT prep.document FROM factory_source_preparations prep WHERE prep.project_id=$1 AND prep.input_id=ANY($2) AND NOT prep.settled AND NOT prep.snapshot_cleaned AND NOT EXISTS(SELECT 1 FROM factory_source_materializations m WHERE m.preparation_id=prep.id AND (m.document->>'rejection' IS NOT NULL OR m.document->>'cleaned_at' IS NOT NULL)) AND ($3::bytea IS NULL OR prep.id>$3) ORDER BY prep.id LIMIT $4")
    .bind(project.as_uuid()).bind(members).bind(after.map(|v| v.as_bytes().to_vec())).bind(i64::from(limit))
    .fetch_all(&mut **tx).await.map_err(unavailable)?;
  documents
    .iter()
    .map(|v| serde_json::from_slice(v).map_err(|_| StoreError::Unavailable))
    .collect()
}

async fn current_input(
  tx: &mut Transaction<'_, Postgres>,
  project: ProjectId,
  id: WorkSourceInputId,
) -> Result<WorkSourceInput, StoreError> {
  let document: Option<Json<WorkSourceInput>> = sqlx::query_scalar(
    "SELECT document FROM factory_source_inputs WHERE project_id=$1 AND id=$2 ORDER BY version DESC LIMIT 1",
  )
  .bind(project.as_uuid())
  .bind(id.as_uuid())
  .fetch_optional(&mut **tx)
  .await
  .map_err(unavailable)?;
  document.map(|v| v.0).ok_or_else(source_conflict)
}
async fn require_current(
  tx: &mut Transaction<'_, Postgres>,
  project: ProjectId,
  target: WorkSourceScanTarget,
) -> Result<(), StoreError> {
  match target {
    WorkSourceScanTarget::Input { id, version } => {
      let input = current_input(tx, project, id).await?;
      if input.version() != version || !input.definition().enabled {
        return Err(source_conflict());
      }
      let grouped:bool=sqlx::query_scalar("WITH groups AS (SELECT DISTINCT ON(id) document FROM factory_source_groups WHERE project_id=$1 ORDER BY id,version DESC) SELECT EXISTS(SELECT 1 FROM groups g,jsonb_array_elements(g.document->'members') m WHERE (m->>'id')::uuid=$2)")
        .bind(project.as_uuid()).bind(id.as_uuid()).fetch_one(&mut **tx).await.map_err(unavailable)?;
      if grouped {
        return Err(source_conflict());
      }
    }
    WorkSourceScanTarget::Group { id, version } => {
      let document: Option<Json<WorkSourceGroup>> = sqlx::query_scalar(
        "SELECT document FROM factory_source_groups WHERE project_id=$1 AND id=$2 ORDER BY version DESC LIMIT 1",
      )
      .bind(project.as_uuid())
      .bind(id.as_uuid())
      .fetch_optional(&mut **tx)
      .await
      .map_err(unavailable)?;
      let group = document.map(|v| v.0).ok_or_else(source_conflict)?;
      if group.version() != version {
        return Err(source_conflict());
      }
      let mut enabled = false;
      for member in group.members() {
        if current_input(tx, project, member.id()).await? != *member {
          return Err(source_conflict());
        }
        enabled |= member.definition().enabled;
      }
      if !enabled {
        return Err(source_conflict());
      }
    }
  }
  Ok(())
}
async fn load_scan(
  tx: &mut Transaction<'_, Postgres>,
  target: WorkSourceScanTarget,
) -> Result<Option<WorkSourceScan>, StoreError> {
  let (kind, id) = target.identity();
  let document: Option<Json<WorkSourceScan>> =
    sqlx::query_scalar("SELECT document FROM factory_source_scans WHERE kind=$1 AND id=$2")
      .bind(kind)
      .bind(id)
      .fetch_optional(&mut **tx)
      .await
      .map_err(unavailable)?;
  Ok(document.map(|v| v.0))
}
async fn save_scan(tx: &mut Transaction<'_, Postgres>, scan: &WorkSourceScan) -> Result<(), StoreError> {
  let (kind, id) = scan.claim.target.identity();
  sqlx::query("INSERT INTO factory_source_scans (kind,id,project_id,document) VALUES ($1,$2,$3,$4) ON CONFLICT (kind,id) DO UPDATE SET document=EXCLUDED.document")
    .bind(kind).bind(id).bind(scan.claim.project_id.as_uuid()).bind(Json(scan)).execute(&mut **tx).await.map_err(unavailable)?;
  Ok(())
}
pub(super) async fn lock_claim(
  tx: &mut Transaction<'_, Postgres>,
  claim: &WorkSourceScanClaim,
  now: octacity_server_domain::Timestamp,
) -> Result<WorkSourceScan, StoreError> {
  lock_project(tx, claim.project_id).await?;
  require_current(tx, claim.project_id, claim.target).await?;
  let scan = load_scan(tx, claim.target).await?.ok_or_else(source_conflict)?;
  require_source_scan(&scan, claim, now)?;
  Ok(scan)
}

pub(super) async fn selected_input(
  tx: &mut Transaction<'_, Postgres>,
  claim: &WorkSourceScanClaim,
  input: WorkSourceInputId,
) -> Result<WorkSourceInput, StoreError> {
  scan_members(tx, claim.project_id, claim.target)
    .await?
    .into_iter()
    .find(|member| member.id() == input)
    .ok_or_else(source_conflict)
}
pub(super) async fn load_preparation(
  tx: &mut Transaction<'_, Postgres>,
  project: ProjectId,
  id: octacity_server_factory::FactoryDigest,
) -> Result<WorkSourcePreparation, StoreError> {
  let document: Option<Vec<u8>> =
    sqlx::query_scalar("SELECT document FROM factory_source_preparations WHERE project_id=$1 AND id=$2")
      .bind(project.as_uuid())
      .bind(id.as_bytes().as_slice())
      .fetch_optional(&mut **tx)
      .await
      .map_err(unavailable)?;
  serde_json::from_slice(&document.ok_or_else(source_conflict)?).map_err(|_| StoreError::Unavailable)
}

pub(super) async fn save_preparation(
  tx: &mut Transaction<'_, Postgres>,
  preparation: &WorkSourcePreparation,
) -> Result<(), StoreError> {
  sqlx::query("UPDATE factory_source_preparations SET document=$1, settled=$2, work_id=$5, run_id=$6 WHERE project_id=$3 AND id=$4")
    .bind(serde_json::to_vec(preparation).map_err(|_| StoreError::Unavailable)?).bind(preparation.admission.is_some()).bind(preparation.input.project_id().as_uuid()).bind(preparation.id.as_bytes().as_slice()).bind(preparation.admission.as_ref().map(|v| v.work_id.as_uuid())).bind(preparation.admission.as_ref().map(|v| v.run_id.as_uuid())).execute(&mut **tx).await.map_err(unavailable)?;
  Ok(())
}

pub(crate) async fn lock_preparation(
  tx: &mut Transaction<'_, Postgres>,
  guard: &octacity_server_store::AdmitPreparedWorkSource,
  request: &octacity_server_store::AdmitFactoryWork,
) -> Result<WorkSourcePreparation, StoreError> {
  lock_claim(tx, &guard.claim, guard.now).await?;
  let preparation = load_preparation(tx, guard.claim.project_id, guard.preparation_id).await?;
  preparation.require_selected_owner(
    &guard.claim,
    &selected_input(tx, &guard.claim, preparation.input.id()).await?,
  )?;
  preparation.validate_admission(guard, request)?;
  Ok(preparation)
}
pub(crate) async fn correlate(
  tx: &mut Transaction<'_, Postgres>,
  mut preparation: WorkSourcePreparation,
  receipt: &octacity_server_store::PublishedFactoryAdmission,
) -> Result<(), StoreError> {
  preparation.correlate(receipt);
  save_preparation(tx, &preparation).await?;
  // Share recoverable association with upgrade Work. Admission keeps its own
  // authority/lock order; the Issue worker resolves verified native identity.
  sqlx::query("INSERT INTO issue_legacy_backfill(work_id) SELECT $1 WHERE NOT EXISTS(SELECT 1 FROM issue_work_associations WHERE work_id=$1) ON CONFLICT DO NOTHING")
    .bind(receipt.work.id().as_uuid())
    .execute(&mut **tx)
    .await
    .map_err(unavailable)?;
  Ok(())
}
pub(crate) async fn correlate_replay(
  pool: &sqlx::PgPool,
  guard: &octacity_server_store::AdmitPreparedWorkSource,
  request: &octacity_server_store::AdmitFactoryWork,
  receipt: &octacity_server_store::PublishedFactoryAdmission,
) -> Result<(), StoreError> {
  let mut tx = pool.begin().await.map_err(unavailable)?;
  let preparation = lock_preparation(&mut tx, guard, request).await?;
  correlate(&mut tx, preparation, receipt).await?;
  tx.commit().await.map_err(unavailable)
}

// Shared transaction body; callers own management evidence and the final commit.
pub(crate) async fn publish_in(
  tx: &mut Transaction<'_, Postgres>,
  request: PublishWorkSources,
) -> Result<(), StoreError> {
  request.validate()?;
  crate::factory_source_configuration::lock_connections(tx, &request).await?;
  lock_project(tx, request.project_id).await?;
  for change in request.inputs {
    crate::factory_source_configuration::validate_managed(tx, &change.input).await?;
    let id = change.input.id().as_uuid();
    let current: Option<(i64, uuid::Uuid)> =
      sqlx::query_as("SELECT version, project_id FROM factory_source_inputs WHERE id=$1 ORDER BY version DESC LIMIT 1")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(unavailable)?;
    if current.map(|v| v.0) != change.expected_version.map(|v| number(v.get())).transpose()?
      || current.is_some_and(|v| v.1 != request.project_id.as_uuid())
    {
      return Err(source_conflict());
    }
    let definition = change.input.definition();
    let target =
      crate::factory_configuration::read(&mut **tx, definition.target.id(), definition.target.version()).await?;
    let repository =
      crate::configuration_query::repository(&mut **tx, definition.repository.id, definition.repository.version)
        .await?;
    validate_source_input_dependencies(&change.input, &target.configuration, &repository)?;
    sqlx::query("INSERT INTO factory_source_inputs (id,version,project_id,configuration_id,configuration_version,repository_id,repository_version,document) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(id).bind(number(change.input.version().get())?).bind(request.project_id.as_uuid()).bind(definition.target.id().as_uuid()).bind(number(definition.target.version().get())?).bind(definition.repository.id.as_uuid()).bind(number(definition.repository.version.get())?).bind(Json(&change.input))
        .execute(&mut **tx).await.map_err(unavailable)?;
  }
  if let Some(change) = request.group {
    let current: Option<(i64, uuid::Uuid)> =
      sqlx::query_as("SELECT version, project_id FROM factory_source_groups WHERE id=$1 ORDER BY version DESC LIMIT 1")
        .bind(change.group.id().as_uuid())
        .fetch_optional(&mut **tx)
        .await
        .map_err(unavailable)?;
    if current.map(|v| v.0) != change.expected_version.map(|v| number(v.get())).transpose()?
      || current.is_some_and(|v| v.1 != request.project_id.as_uuid())
    {
      return Err(source_conflict());
    }
    for member in change.group.members() {
      crate::factory_source_configuration::validate_managed(tx, member).await?;
      let current: Option<Json<WorkSourceInput>> = sqlx::query_scalar(
        "SELECT document FROM factory_source_inputs WHERE id=$1 AND project_id=$2 ORDER BY version DESC LIMIT 1",
      )
      .bind(member.id().as_uuid())
      .bind(request.project_id.as_uuid())
      .fetch_optional(&mut **tx)
      .await
      .map_err(unavailable)?;
      if current.as_ref().map(|v| &v.0) != Some(member) {
        return Err(source_conflict());
      }
    }
    sqlx::query("INSERT INTO factory_source_groups (id,version,project_id,document) VALUES ($1,$2,$3,$4)")
      .bind(change.group.id().as_uuid())
      .bind(number(change.group.version().get())?)
      .bind(request.project_id.as_uuid())
      .bind(Json(&change.group))
      .execute(&mut **tx)
      .await
      .map_err(unavailable)?;
  }
  Ok(())
}
