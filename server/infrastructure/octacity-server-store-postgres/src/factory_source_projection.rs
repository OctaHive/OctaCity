//! Source-item ordering shares its row lock with accepted status publication.
use crate::{PostgresStore, database::unavailable, factory_run};
use octacity_server_domain::ProjectId;
use octacity_server_factory::{FactoryDigest, FactoryRunId, ImmutableReference};
use octacity_server_store::{
  FactoryOutboxRecord, SourceProjectionJournal, SourceProjectionOrder, SourceProjectionRequest, SourceProjectionState,
  StoreError, WorkSourcePreparation, source_conflict, source_projection_candidates,
};
use sqlx::{Postgres, Transaction, types::Json};

pub(crate) struct Scope {
  project: ProjectId,
  issue: Vec<u8>,
  capability: ImmutableReference,
}
pub(crate) async fn lock_issue(
  transaction: &mut Transaction<'_, Postgres>,
  project: ProjectId,
  issue: &[u8],
) -> Result<SourceProjectionJournal, StoreError> {
  sqlx::query("INSERT INTO factory_source_projection_journals (project_id,issue,document) VALUES ($1,$2,$3) ON CONFLICT (project_id,issue) DO NOTHING")
    .bind(project.as_uuid()).bind(issue).bind(Json(SourceProjectionJournal::default())).execute(&mut **transaction).await.map_err(unavailable)?;
  let journal: Json<SourceProjectionJournal> = sqlx::query_scalar(
    "SELECT document FROM factory_source_projection_journals WHERE project_id=$1 AND issue=$2 FOR UPDATE",
  )
  .bind(project.as_uuid())
  .bind(issue)
  .fetch_one(&mut **transaction)
  .await
  .map_err(unavailable)?;
  Ok(journal.0)
}
pub(crate) async fn lock_for_run(
  transaction: &mut Transaction<'_, Postgres>,
  run: FactoryRunId,
) -> Result<Option<Scope>, StoreError> {
  let rows: Vec<Vec<u8>> =
    sqlx::query_scalar("SELECT document FROM factory_source_preparations WHERE run_id=$1 AND settled LIMIT 2")
      .bind(run.as_uuid())
      .fetch_all(&mut **transaction)
      .await
      .map_err(unavailable)?;
  if rows.len() > 1 {
    return Err(source_conflict());
  }
  let Some(document) = rows.first() else { return Ok(None) };
  let preparation: WorkSourcePreparation = serde_json::from_slice(document).map_err(|_| StoreError::Unavailable)?;
  let Some(reporter) = preparation.input.definition().reporter.as_ref() else {
    return Ok(None);
  };
  let scope = Scope {
    project: preparation.input.project_id(),
    issue: preparation.projection_item().as_bytes().to_vec(),
    capability: reporter.capability.clone(),
  };
  lock_issue(transaction, scope.project, &scope.issue).await?;
  Ok(Some(scope))
}
pub(crate) async fn record_publications(
  transaction: &mut Transaction<'_, Postgres>,
  scope: Option<&Scope>,
  run: FactoryRunId,
  rows: &[FactoryOutboxRecord],
) -> Result<(), StoreError> {
  let Some(scope) = scope else { return Ok(()) };
  let snapshot = factory_run::read_snapshot_in_transaction(transaction, run).await?;
  let candidates = source_projection_candidates(&[snapshot])?;
  for row in rows {
    if !candidates
      .iter()
      .any(|c| c.operation_id == row.operation_id && c.capability == scope.capability)
    {
      continue;
    }
    let existing: Option<(uuid::Uuid, Vec<u8>)> =
      sqlx::query_as("SELECT project_id,issue FROM factory_source_projection_intents WHERE operation_id=$1")
        .bind(row.operation_id.as_bytes().to_vec())
        .fetch_optional(&mut **transaction)
        .await
        .map_err(unavailable)?;
    if let Some((project, issue)) = existing {
      if project != scope.project.as_uuid() || issue != scope.issue {
        return Err(source_conflict());
      }
      continue;
    }
    let sequence:i64=sqlx::query_scalar("UPDATE factory_source_projection_journals SET next_sequence=next_sequence+1 WHERE project_id=$1 AND issue=$2 AND next_sequence<9223372036854775807 RETURNING next_sequence")
      .bind(scope.project.as_uuid()).bind(&scope.issue).fetch_optional(&mut **transaction).await.map_err(unavailable)?.ok_or_else(source_conflict)?;
    sqlx::query(
      "INSERT INTO factory_source_projection_intents (operation_id,project_id,issue,sequence) VALUES ($1,$2,$3,$4)",
    )
    .bind(row.operation_id.as_bytes().to_vec())
    .bind(scope.project.as_uuid())
    .bind(&scope.issue)
    .bind(sequence)
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
  }
  Ok(())
}
pub(crate) async fn coordinate(
  store: &PostgresStore,
  request: SourceProjectionRequest,
) -> Result<SourceProjectionState, StoreError> {
  let mut transaction = store.pool().begin().await.map_err(unavailable)?;
  let documents: Vec<Vec<u8>> = sqlx::query_scalar(
    "SELECT document FROM factory_source_preparations WHERE project_id=$1 AND work_id=$2 AND settled LIMIT 2",
  )
  .bind(request.project_id.as_uuid())
  .bind(request.work_id.as_uuid())
  .fetch_all(&mut *transaction)
  .await
  .map_err(unavailable)?;
  if documents.len() != 1 {
    return Err(source_conflict());
  }
  let preparation: WorkSourcePreparation =
    serde_json::from_slice(&documents[0]).map_err(|_| StoreError::Unavailable)?;
  if preparation.input.project_id() != request.project_id {
    return Err(source_conflict());
  }
  let reporter = preparation
    .input
    .definition()
    .reporter
    .as_ref()
    .ok_or_else(source_conflict)?;
  let association = preparation.admission.as_ref().ok_or_else(source_conflict)?;
  let issue = preparation.projection_item().as_bytes().to_vec();
  let mut journal = lock_issue(&mut transaction, request.project_id, &issue).await?;
  let record:Option<(i64,Option<Json<Vec<FactoryDigest>>>)>=sqlx::query_as("SELECT sequence,managed_fields FROM factory_source_projection_intents WHERE operation_id=$1 AND project_id=$2 AND issue=$3 FOR UPDATE")
    .bind(request.operation_id.as_bytes().to_vec()).bind(request.project_id.as_uuid()).bind(&issue).fetch_optional(&mut *transaction).await.map_err(unavailable)?;
  let (sequence, fields) = record.ok_or_else(source_conflict)?;
  let mut order = SourceProjectionOrder {
    sequence: u64::try_from(sequence).map_err(|_| StoreError::Unavailable)?,
    fields: fields.map(|v| v.0),
  };
  let snapshot = factory_run::read_snapshot_in_transaction(&mut transaction, association.run_id).await?;
  let candidates = source_projection_candidates(&[snapshot])?;
  if !candidates
    .iter()
    .any(|c| c.operation_id == request.operation_id && c.capability == reporter.capability)
  {
    return Err(source_conflict());
  }
  let newer_unknown:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM factory_source_projection_intents WHERE project_id=$1 AND issue=$2 AND sequence>$3 AND managed_fields IS NULL)")
    .bind(request.project_id.as_uuid()).bind(&issue).bind(sequence).fetch_one(&mut *transaction).await.map_err(unavailable)?;
  let state = journal.coordinate(&request, &candidates, &mut order, newer_unknown)?;
  if serde_json::to_vec(&journal).map_err(|_| StoreError::Unavailable)?.len() > 4 * 1024 * 1024 {
    return Err(source_conflict());
  }
  sqlx::query("UPDATE factory_source_projection_intents SET managed_fields=$2 WHERE operation_id=$1")
    .bind(request.operation_id.as_bytes().to_vec())
    .bind(order.fields.map(Json))
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
  sqlx::query("UPDATE factory_source_projection_journals SET document=$3 WHERE project_id=$1 AND issue=$2")
    .bind(request.project_id.as_uuid())
    .bind(&issue)
    .bind(Json(journal))
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(state)
}
