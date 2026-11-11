//! Fences canonical Issue input under the same transaction that admits Work/Run.
use crate::database::unavailable;
use octacity_server_domain::EntityKind;
use octacity_server_factory::FactoryConfiguration;
use octacity_server_store::*;
use sqlx::{PgConnection, types::Json};

fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Issue,
  }
}

pub(crate) async fn require_current(
  connection: &mut PgConnection,
  request: &AdmitFactoryWork,
  preparation: Option<&ProjectTextPreparation>,
  source: Option<&mut WorkSourcePreparation>,
  configuration: &FactoryConfiguration,
  now: octacity_server_domain::Timestamp,
) -> Result<(), StoreError> {
  if configuration.flow().issue_snapshot.is_none()
    && request
      .incoming
      .as_ref()
      .and_then(|input| input.issue_snapshot())
      .is_none()
    && preparation.is_none_or(|p| p.intent.snapshot_digest.is_none())
  {
    return Ok(());
  }
  if let Some(source) = source {
    return require_source_current(connection, request, source, configuration, now).await;
  }
  // Snapshot inputs enter only through the managed Issue preparation, including
  // their complete payload. An ordinary Artifact admission cannot invent one.
  let preparation = preparation.ok_or_else(conflict)?;
  let Json(documents): Json<ProjectTextDocuments> =
    sqlx::query_scalar("SELECT documents FROM project_text_documents WHERE preparation_id=$1")
      .bind(preparation.id.as_bytes().as_slice())
      .fetch_optional(&mut *connection)
      .await
      .map_err(unavailable)?
      .ok_or_else(conflict)?;
  let snapshot = documents.issue_snapshot().ok_or_else(conflict)?;
  snapshot.validate_prepared_admission(preparation, request)?;
  if configuration.flow().issue_snapshot.as_ref() != Some(&snapshot.input) {
    return Err(conflict());
  }
  require_snapshot_head(connection, snapshot, now).await?;
  // Bind the private preparation to the original authorized request, not merely
  // to another Issue with identical text in the same Project.
  let (_, id) = preparation
    .intent
    .external_identity
    .as_str()
    .split_once('/')
    .ok_or_else(conflict)?;
  let id = uuid::Uuid::parse_str(id).map_err(|_| conflict())?;
  // The parsed ID is only a lookup hint. Canonical domain identities below
  // verify both namespaces without duplicating their encodings in SQL.
  let Json(reviewed): Json<IssueExecutionRequest> = sqlx::query_scalar(
    "SELECT document FROM issue_execution_requests WHERE project_id=$1 AND issue_id=$2 AND security_scope=$3
       AND id=$4 FOR SHARE",
  )
  .bind(snapshot.project_id.as_uuid())
  .bind(snapshot.issue_id.as_uuid())
  .bind(preparation.security_scope.as_str())
  .bind(id)
  .fetch_optional(&mut *connection)
  .await
  .map_err(unavailable)?
  .ok_or_else(conflict)?;
  if reviewed.external_identity() != preparation.intent.external_identity
    || reviewed.admission_key() != preparation.intent.idempotency_key
    || reviewed.snapshot_digest != Some(snapshot.digest()?)
    || reviewed.admission_classification != snapshot.admission_classification
    || reviewed.intent.expected_version != snapshot.version
    || reviewed.intent.target != snapshot.target
    || reviewed.title_digest != snapshot.title_digest
    || reviewed.description_digest != snapshot.description_digest
    || reviewed.acceptance_digest != snapshot.acceptance_digest
  {
    return Err(conflict());
  }
  AssociateIssueWork {
    project_id: snapshot.project_id,
    issue_id: snapshot.issue_id,
    request_id: Some(reviewed.id),
    admission: PublishedFactoryAdmission {
      work: request.work.clone(),
      run: request.run.clone(),
      flow: request.flow.clone(),
      admitted_at: request.admitted_at,
    },
  }
  .association(Some(&reviewed))?;
  Ok(())
}

// Both input owners fence the same canonical Issue and definition/material heads.
async fn require_snapshot_head(
  connection: &mut PgConnection,
  snapshot: &IssueExecutionSnapshot,
  now: octacity_server_domain::Timestamp,
) -> Result<(), StoreError> {
  // Match the Project -> Issue order used by catalog/planning writes and keep
  // both locks until the ordinary admission transaction commits.
  crate::issues::issue_workflow::lock_project(connection, snapshot.project_id).await?;
  let current = crate::issues::issue::lock(connection, snapshot.project_id, snapshot.issue_id).await?;
  let definitions = crate::issues::issue_planning::read(connection, snapshot.project_id, None).await?;
  if !current.content_available
    || current.version != snapshot.version
    || current.applicability_version != snapshot.applicability_version
    || definitions.version != snapshot.definition_head.unwrap_or(snapshot.input.definition_version)
  {
    return Err(conflict());
  }
  let selections: Vec<_> = snapshot
    .materials
    .values()
    .flatten()
    .map(|entry| &entry.selection)
    .collect();
  if selections
    .iter()
    .any(|selection| !current.materials.entries().contains(selection))
  {
    return Err(conflict());
  }
  let references: Vec<_> = selections.into_iter().map(|s| s.reference.clone()).collect();
  if !super::issue_material::sources_available(connection, snapshot.project_id, snapshot.issue_id, &references, now)
    .await?
  {
    return Err(conflict());
  }
  Ok(())
}

async fn require_source_current(
  connection: &mut PgConnection,
  request: &AdmitFactoryWork,
  preparation: &mut WorkSourcePreparation,
  configuration: &FactoryConfiguration,
  now: octacity_server_domain::Timestamp,
) -> Result<(), StoreError> {
  preparation.validate_issue_input(request)?;
  let snapshot = preparation
    .issue_input
    .as_ref()
    .and_then(|documents| documents.issue_snapshot())
    .ok_or_else(conflict)?;
  if configuration.flow().issue_snapshot.as_ref() != Some(&snapshot.input) {
    return Err(conflict());
  }
  require_snapshot_head(connection, snapshot, now).await?;
  let scope = &request.probe.source_scope.security_scope;
  let correlation = source_native_correlation(preparation.issue, source_security_partition(scope));
  let link = super::issue_source::native_link(connection, snapshot.project_id, scope, correlation)
    .await?
    .filter(|link| link.issue_id == snapshot.issue_id)
    .ok_or_else(conflict)?;
  if preparation
    .issue_revision
    .as_ref()
    .is_some_and(|pinned| pinned.correlation != correlation || pinned.issue_id != link.issue_id)
  {
    return Err(conflict());
  }
  preparation.issue_revision = Some(WorkSourceIssueRevision {
    correlation,
    issue_id: link.issue_id,
    version: snapshot.version,
  });
  Ok(())
}
