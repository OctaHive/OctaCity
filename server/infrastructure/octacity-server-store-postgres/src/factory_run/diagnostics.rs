use std::str::FromStr;

use octacity_server_domain::EntityKind;
use octacity_server_factory::FactoryDigest;
use octacity_server_store::{
  FactoryRunDiagnosticKind, FactoryRunDiagnosticPage, FactoryRunDiagnosticRecord, ListFactoryRunDiagnostics,
  StoreError, StoreInputError, StoreOperation,
};
use serde::de::DeserializeOwned;
use sqlx::{PgPool, types::Json};

use crate::database::unavailable;

pub(crate) async fn list_diagnostics(
  pool: &PgPool,
  request: ListFactoryRunDiagnostics,
) -> Result<FactoryRunDiagnosticPage, StoreError> {
  let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM factory_runs WHERE id = $1 AND visible)")
    .bind(request.run_id.as_uuid())
    .fetch_one(pool)
    .await
    .map_err(unavailable)?;
  if !exists {
    return Err(StoreError::NotFound {
      entity: EntityKind::FactoryRun,
    });
  }
  let limit = i64::from(request.limit.get()) + 1;
  let records = match request.kind {
    FactoryRunDiagnosticKind::StageAttempt => uuid_rows(
      pool,
      "SELECT stage_attempt FROM factory_stage_attempts WHERE run_id = $1 AND ($2::UUID IS NULL OR id > $2) ORDER BY id LIMIT $3",
      &request,
      limit,
      |value| FactoryRunDiagnosticRecord::StageAttempt(Box::new(value)),
    ).await?,
    FactoryRunDiagnosticKind::StageAttemptCompletion => digest_rows(pool, "SELECT completion FROM factory_stage_attempt_completions WHERE run_id = $1 AND ($2::BYTEA IS NULL OR id > $2) ORDER BY id LIMIT $3", &request, limit, |value| FactoryRunDiagnosticRecord::StageAttemptCompletion(Box::new(value))).await?,
    FactoryRunDiagnosticKind::MacroCall => uuid_rows(pool, "SELECT call_node FROM factory_call_nodes WHERE run_id = $1 AND ($2::UUID IS NULL OR id > $2) ORDER BY id LIMIT $3", &request, limit, |value| FactoryRunDiagnosticRecord::MacroCall(Box::new(value))).await?,
    FactoryRunDiagnosticKind::SignalRequest => uuid_rows(pool, "SELECT request FROM factory_decision_signal_requests WHERE run_id = $1 AND ($2::UUID IS NULL OR id > $2) ORDER BY id LIMIT $3", &request, limit, |value| FactoryRunDiagnosticRecord::SignalRequest(Box::new(value))).await?,
    FactoryRunDiagnosticKind::SignalReceipt => uuid_rows(pool, "SELECT receipt FROM factory_decision_signal_receipts WHERE run_id = $1 AND ($2::UUID IS NULL OR id > $2) ORDER BY id LIMIT $3", &request, limit, |value| FactoryRunDiagnosticRecord::SignalReceipt(Box::new(value))).await?,
    FactoryRunDiagnosticKind::BuildLink => uuid_rows(pool, "SELECT link FROM factory_build_links WHERE run_id = $1 AND ($2::UUID IS NULL OR build_id > $2) ORDER BY build_id LIMIT $3", &request, limit, |value| FactoryRunDiagnosticRecord::BuildLink(Box::new(value))).await?,
    FactoryRunDiagnosticKind::BuildObservation => digest_rows(pool, "SELECT observation FROM factory_build_observations WHERE run_id = $1 AND ($2::BYTEA IS NULL OR id > $2) ORDER BY id LIMIT $3", &request, limit, |value| FactoryRunDiagnosticRecord::BuildObservation(Box::new(value))).await?,
    FactoryRunDiagnosticKind::Candidate => uuid_rows(pool, "SELECT changeset FROM factory_changesets WHERE run_id = $1 AND ($2::UUID IS NULL OR id > $2) ORDER BY id LIMIT $3", &request, limit, |value| FactoryRunDiagnosticRecord::Candidate(Box::new(value))).await?,
    FactoryRunDiagnosticKind::Evidence => uuid_rows(pool, "SELECT manifest FROM factory_evidence_manifests WHERE run_id = $1 AND ($2::UUID IS NULL OR id > $2) ORDER BY id LIMIT $3", &request, limit, |value| FactoryRunDiagnosticRecord::Evidence(Box::new(value))).await?,
    FactoryRunDiagnosticKind::EvaluationPlan => uuid_rows(pool, "SELECT plan FROM factory_evaluation_plans WHERE run_id = $1 AND ($2::UUID IS NULL OR id > $2) ORDER BY id LIMIT $3", &request, limit, |value| FactoryRunDiagnosticRecord::EvaluationPlan(Box::new(value))).await?,
    FactoryRunDiagnosticKind::Assessment => uuid_rows(pool, "SELECT assessment FROM factory_assessments WHERE run_id = $1 AND ($2::UUID IS NULL OR id > $2) ORDER BY id LIMIT $3", &request, limit, |value| FactoryRunDiagnosticRecord::Assessment(Box::new(value))).await?,
    FactoryRunDiagnosticKind::Decision => uuid_rows(pool, "SELECT decision FROM factory_decisions WHERE run_id = $1 AND ($2::UUID IS NULL OR id > $2) ORDER BY id LIMIT $3", &request, limit, |value| FactoryRunDiagnosticRecord::Decision(Box::new(value))).await?,
    FactoryRunDiagnosticKind::Escalation => uuid_rows(pool, "SELECT escalation FROM factory_escalations WHERE run_id = $1 AND ($2::UUID IS NULL OR id > $2) ORDER BY id LIMIT $3", &request, limit, |value| FactoryRunDiagnosticRecord::Escalation(Box::new(value))).await?,
    FactoryRunDiagnosticKind::DeliveryAttempt => uuid_rows(pool, "SELECT attempt FROM factory_delivery_attempts WHERE run_id = $1 AND ($2::UUID IS NULL OR id > $2) ORDER BY id LIMIT $3", &request, limit, |value| FactoryRunDiagnosticRecord::DeliveryAttempt(Box::new(value))).await?,
    FactoryRunDiagnosticKind::ReportingAttempt => uuid_rows(pool, "SELECT attempt FROM factory_reporting_attempts WHERE run_id = $1 AND ($2::UUID IS NULL OR id > $2) ORDER BY id LIMIT $3", &request, limit, |value| FactoryRunDiagnosticRecord::ReportingAttempt(Box::new(value))).await?,
  };
  page(records, usize::from(request.limit.get()))
}

async fn uuid_rows<T>(
  pool: &PgPool,
  query: &'static str,
  request: &ListFactoryRunDiagnostics,
  limit: i64,
  wrap: fn(T) -> FactoryRunDiagnosticRecord,
) -> Result<Vec<FactoryRunDiagnosticRecord>, StoreError>
where
  T: DeserializeOwned + Send + Unpin + 'static,
{
  let after = request
    .after
    .as_ref()
    .map(|value| uuid::Uuid::from_str(value.as_str()))
    .transpose()
    .map_err(|_| invalid_cursor())?;
  sqlx::query_scalar::<_, Json<T>>(query)
    .bind(request.run_id.as_uuid())
    .bind(after)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(unavailable)
    .map(|rows| rows.into_iter().map(|row| wrap(row.0)).collect())
}

async fn digest_rows<T>(
  pool: &PgPool,
  query: &'static str,
  request: &ListFactoryRunDiagnostics,
  limit: i64,
  wrap: fn(T) -> FactoryRunDiagnosticRecord,
) -> Result<Vec<FactoryRunDiagnosticRecord>, StoreError>
where
  T: DeserializeOwned + Send + Unpin + 'static,
{
  let after = request
    .after
    .as_ref()
    .map(|value| FactoryDigest::from_lower_hex(value.as_str()).map(FactoryDigest::as_bytes))
    .transpose()
    .map_err(|_| invalid_cursor())?;
  sqlx::query_scalar::<_, Json<T>>(query)
    .bind(request.run_id.as_uuid())
    .bind(after.as_ref().map(<[u8; 32]>::as_slice))
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(unavailable)
    .map(|rows| rows.into_iter().map(|row| wrap(row.0)).collect())
}

fn page(mut records: Vec<FactoryRunDiagnosticRecord>, limit: usize) -> Result<FactoryRunDiagnosticPage, StoreError> {
  let has_next = records.len() > limit;
  records.truncate(limit);
  let next = has_next.then(|| records.last().expect("positive diagnostic page limit").cursor());
  if records.windows(2).any(|pair| pair[0].cursor() >= pair[1].cursor()) {
    return Err(StoreError::Unavailable);
  }
  Ok(FactoryRunDiagnosticPage { items: records, next })
}

fn invalid_cursor() -> StoreError {
  StoreError::InvalidInput {
    operation: StoreOperation::ListFactoryRunDiagnostics,
    source: StoreInputError::InvalidFactoryRunDiagnosticCursor,
  }
}
