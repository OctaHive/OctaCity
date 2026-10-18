//! Scoped projections exclude private preparation documents and filter before pagination.
use crate::{PostgresStore, database::unavailable};
use octacity_server_domain::ProjectId;
use octacity_server_factory::*;
use octacity_server_store::*;
use sqlx::types::Json;
#[async_trait::async_trait]
impl FactorySourceDiagnosticsStore for PostgresStore {
  async fn source_reporting_diagnostics(
    &self,
    project: ProjectId,
    association: &WorkSourceDiagnosticAssociation,
    limit: u16,
  ) -> Result<WorkSourceReportingPage, StoreError> {
    if !(1..=MAX_WORK_SOURCE_REPORTING_OPERATIONS).contains(&limit) {
      return Err(source_conflict());
    }
    // Registration already proved accepted-action provenance and the original Reporter
    // inside the publication transaction. Fetch one latest metadata row per operation;
    // no Flow documents, private Source snapshots or execution locks are needed here.
    let rows: Vec<(Vec<u8>, String, i32, i64)> = sqlx::query_as(
      "SELECT initial.operation_id, latest.state, latest.attempt, latest.available_at \
       FROM factory_source_preparations p \
       JOIN factory_runs run ON run.id=p.run_id AND run.visible \
       JOIN factory_outbox_records initial ON initial.run_id=p.run_id \
         AND initial.kind='work.status.projection' AND initial.attempt=0 AND initial.state='pending' \
       JOIN factory_source_projection_intents accepted ON accepted.operation_id=initial.operation_id AND accepted.project_id=p.project_id \
       CROSS JOIN LATERAL (SELECT state, attempt, FLOOR(EXTRACT(EPOCH FROM available_at)*1000)::BIGINT AS available_at \
         FROM factory_outbox_records history WHERE history.operation_id=initial.operation_id AND history.run_id=p.run_id \
         ORDER BY attempt DESC, CASE state WHEN 'pending' THEN 0 WHEN 'claimed' THEN 1 ELSE 2 END DESC, recorded_at DESC, id DESC LIMIT 1) latest \
       WHERE p.id=$1 AND p.project_id=$2 AND p.input_id=$3 AND p.input_version=$4 AND p.work_id=$5 AND p.run_id=$6 AND p.settled \
       ORDER BY initial.operation_id LIMIT $7")
      .bind(association.id.as_bytes().as_slice()).bind(project.as_uuid()).bind(association.input_id.as_uuid())
      .bind(i64::try_from(association.input_version.get()).map_err(|_| source_conflict())?)
      .bind(association.work_id.as_uuid()).bind(association.run_id.as_uuid()).bind(i64::from(limit)+1)
      .fetch_all(self.pool()).await.map_err(unavailable)?;
    let mut items = rows
      .into_iter()
      .map(|(operation, state, attempt, at)| {
        Ok(WorkSourceReportingDiagnostic {
          operation_id: read_digest(operation)?,
          state: match state.as_str() {
            "pending" => WorkSourceReportingState::Pending,
            "claimed" => WorkSourceReportingState::Claimed,
            "delivered" => WorkSourceReportingState::Applied,
            "failed" => WorkSourceReportingState::Exhausted,
            _ => return Err(StoreError::Unavailable),
          },
          attempt: u16::try_from(attempt).map_err(|_| StoreError::Unavailable)?,
          available_at: crate::discovery::timestamp(at)?,
        })
      })
      .collect::<Result<Vec<_>, StoreError>>()?;
    let truncated = items.len() > usize::from(limit);
    items.truncate(usize::from(limit));
    Ok(WorkSourceReportingPage { items, truncated })
  }
  async fn source_scan_diagnostics(
    &self,
    project: ProjectId,
    target: WorkSourceScanTarget,
  ) -> Result<Option<WorkSourceScanDiagnostics>, StoreError> {
    let (kind, id) = target.identity();
    let scan: Option<Json<WorkSourceScan>> =
      sqlx::query_scalar("SELECT document FROM factory_source_scans WHERE project_id=$1 AND kind=$2 AND id=$3")
        .bind(project.as_uuid())
        .bind(kind)
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(unavailable)?;
    Ok(
      scan
        .map(|s| s.0)
        .filter(|s| s.claim.project_id == project && s.claim.target == target)
        .map(WorkSourceScanDiagnostics::from_scan),
    )
  }
  async fn source_diagnostic_correlations(
    &self,
    request: ReadWorkSourceCorrelations,
  ) -> Result<WorkSourceDiagnosticPage, StoreError> {
    request.validate()?;
    let configs =
      crate::read_visibility::sql_read_visibility(request.visibility.configurations.view(), |id| id.as_uuid());
    let repos = crate::read_visibility::sql_read_visibility(request.visibility.repositories.view(), |id| id.as_uuid());
    let connections =
      crate::read_visibility::sql_read_visibility(request.visibility.connections.view(), |id| id.as_uuid());
    let runs = crate::read_visibility::sql_read_visibility(request.runs.view(), |id| id.as_uuid());
    let (by_inputs, inputs, issues, partition) = match &request.filter {
      WorkSourceDiagnosticFilter::Inputs(ids) => {
        (true, ids.iter().map(|i| i.as_uuid()).collect::<Vec<_>>(), vec![], None)
      }
      WorkSourceDiagnosticFilter::Native { issues, security_scope } => (
        false,
        vec![],
        issues.iter().map(|i| i.as_bytes().to_vec()).collect::<Vec<_>>(),
        Some(security_scope.as_bytes().to_vec()),
      ),
    };
    type Row = (Vec<u8>, Vec<u8>, Vec<u8>, uuid::Uuid, i64, uuid::Uuid, uuid::Uuid);
    let rows:Vec<Row>=sqlx::query_as(
      "SELECT p.id,p.issue,w.security_scope_digest,p.input_id,p.input_version,p.work_id,p.run_id \
       FROM factory_source_preparations p JOIN factory_source_inputs i ON i.id=p.input_id AND i.version=p.input_version AND i.project_id=p.project_id \
       JOIN factory_work_envelopes w ON w.id=p.work_id AND w.project_id=p.project_id \
       WHERE p.project_id=$1 AND p.settled \
       AND ($2 OR i.configuration_id=ANY($3::uuid[])) AND ($4 OR i.repository_id=ANY($5::uuid[])) \
       AND ((i.document#>>'{definition,connection,id}') IS NULL OR $6 OR (i.document#>>'{definition,connection,id}')::uuid=ANY($7::uuid[])) \
       AND ($8 OR p.run_id=ANY($9::uuid[])) \
       AND (($10 AND p.input_id=ANY($11::uuid[])) OR (NOT $10 AND p.issue=ANY($12::bytea[]) AND w.security_scope_digest=$13)) \
       AND ($14::bytea IS NULL OR p.id>$14) ORDER BY p.id LIMIT $15")
      .bind(request.project_id.as_uuid()).bind(configs.all).bind(configs.identities).bind(repos.all).bind(repos.identities).bind(connections.all).bind(connections.identities).bind(runs.all).bind(runs.identities)
      .bind(by_inputs).bind(inputs).bind(issues).bind(partition).bind(request.after.map(|id|id.as_bytes().to_vec())).bind(i64::from(request.limit)+1).fetch_all(self.pool()).await.map_err(unavailable)?;
    let mut items = rows
      .into_iter()
      .map(|(id, issue, partition, input, version, work, run)| {
        Ok(WorkSourceDiagnosticAssociation {
          id: read_digest(id)?,
          native_correlation: source_native_correlation(read_digest(issue)?, read_digest(partition)?),
          input_id: WorkSourceInputId::from_uuid(input).map_err(|_| StoreError::Unavailable)?,
          input_version: WorkSourceInputVersion::new(u64::try_from(version).map_err(|_| StoreError::Unavailable)?)
            .map_err(|_| StoreError::Unavailable)?,
          work_id: WorkEnvelopeId::from_uuid(work).map_err(|_| StoreError::Unavailable)?,
          run_id: FactoryRunId::from_uuid(run).map_err(|_| StoreError::Unavailable)?,
        })
      })
      .collect::<Result<Vec<_>, StoreError>>()?;
    let next_cursor = (items.len() > usize::from(request.limit)).then(|| items[usize::from(request.limit) - 1].id);
    items.truncate(usize::from(request.limit));
    Ok(WorkSourceDiagnosticPage { items, next_cursor })
  }
}
fn read_digest(bytes: Vec<u8>) -> Result<FactoryDigest, StoreError> {
  Ok(FactoryDigest::from_bytes(
    bytes.try_into().map_err(|_| StoreError::Unavailable)?,
  ))
}
