//! Fenced private dispatch intents, with unresolved usage held at its full ceiling.
use crate::{
  PostgresStore,
  database::unavailable,
  factory_signal_receipt::{cancellation_requested, conflict, consumed_usage, validate_owner},
};
use async_trait::async_trait;
use futures_util::TryStreamExt;
use octacity_server_factory::*;
use octacity_server_store::*;
use sqlx::Postgres;

type RequestRow = (Vec<u8>, Vec<u8>);
#[derive(sqlx::FromRow)]
struct LeaseSignalJobFacts {
  factory_origin: Option<sqlx::types::Json<FactoryBuildOrigin>>,
  job_state: String,
  attempt_state: String,
  build_state: String,
  latest_attempt: bool,
  template: sqlx::types::Json<octacity_server_job::JobSpecTemplate>,
}
fn decode((document, digest): RequestRow) -> Result<DecisionSignalProviderRequest, StoreError> {
  let request: DecisionSignalProviderRequest =
    serde_json::from_slice(&document).map_err(|_| StoreError::Unavailable)?;
  request.validate_integrity().map_err(|_| StoreError::Unavailable)?;
  if request.digest().as_bytes() != digest.as_slice() {
    return Err(StoreError::Unavailable);
  }
  Ok(request)
}
pub(super) async fn recorded<'e, E: sqlx::Executor<'e, Database = Postgres>>(
  executor: E,
  request: &DecisionSignalProviderRequest,
) -> Result<bool, StoreError> {
  request.validate_integrity().map_err(|_| conflict())?;
  let row: Option<RequestRow> =
    sqlx::query_as("SELECT document,request_digest FROM factory_decision_signal_requests WHERE request_id=$1")
      .bind(request.id().as_uuid())
      .fetch_optional(executor)
      .await
      .map_err(unavailable)?;
  match row {
    None => Ok(false),
    Some(row) => {
      if decode(row)? == *request {
        Ok(true)
      } else {
        Err(conflict())
      }
    }
  }
}

#[async_trait]
impl FactoryDecisionSignalRequestStore for PostgresStore {
  async fn factory_decision_signal_request_reserved(
    &self,
    request: &DecisionSignalProviderRequest,
  ) -> Result<bool, StoreError> {
    recorded(self.pool(), request).await
  }
  async fn reserve_factory_decision_signal_request(
    &self,
    reservation: ReserveFactoryDecisionSignalRequest,
  ) -> Result<FactoryDecisionSignalRequestDisposition, StoreError> {
    let request = &reservation.request;
    request.validate_integrity().map_err(|_| conflict())?;
    let document = serde_json::to_vec(request).map_err(|_| conflict())?;
    if document.len() > 4 * 1024 * 1024 || reservation.observed_at >= request.deadline() {
      return Err(conflict());
    }
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    let snapshot = crate::factory_run::read_snapshot_in_transaction(&mut tx, request.request().run_id()).await?;
    let node = validate_owner(&snapshot, request, &reservation.ownership, reservation.observed_at)?;
    if cancellation_requested(&snapshot)? {
      return Err(conflict());
    }
    if request.request().purpose() == DecisionSignalPurpose::ToolRisk && reservation.lease_guard.is_none() {
      return Err(conflict());
    }
    if let Some(guard) = reservation.lease_guard {
      let lease = crate::lease::authorize_tool_action_in_transaction(
        &mut tx,
        guard.lease,
        guard.job_id,
        guard.attempt,
        reservation.observed_at,
      )
      .await?;
      let facts: LeaseSignalJobFacts=sqlx::query_as(
        "SELECT build.factory_origin,job.state AS job_state,attempt.state AS attempt_state,build.state AS build_state, \
          NOT EXISTS (SELECT 1 FROM attempts successor WHERE successor.build_id=build.id AND successor.attempt_number>attempt.attempt_number) AS latest_attempt,job.job_spec_template AS template \
         FROM jobs job JOIN attempts attempt ON attempt.id=job.attempt_id JOIN builds build ON build.id=attempt.build_id WHERE job.id=$1",
      ).bind(guard.job_id.as_uuid()).fetch_one(&mut *tx).await.map_err(unavailable)?;
      let origin = facts.factory_origin.ok_or_else(conflict)?.0;
      if origin.run_id != node.factory_run_id()
        || origin.node_attempt_id != node.id()
        || origin.input_digest != node.input_digest()
        || lease.project_id != request.request().subject().project_id().as_uuid()
        || !matches!(facts.job_state.as_str(), "leased" | "running")
        || !matches!(facts.attempt_state.as_str(), "created" | "running")
        || !matches!(facts.build_state.as_str(), "queued" | "running")
        || !facts.latest_attempt
      {
        return Err(conflict());
      }
      facts.template.0.validate().map_err(|_| conflict())?;
      let managed = facts.template.0.managed_intent().ok_or_else(conflict)?;
      let control = managed.execution().tool_control.as_ref().ok_or_else(conflict)?;
      if control.mode != octacity_protocol::FactoryToolControlModeV3::ToolRisk {
        return Err(conflict());
      }
    }
    let limit = reservation.reservation_limit.unwrap_or(node.budget());
    if !limit.fits_within(node.budget()) {
      return Err(conflict());
    }
    if recorded(&mut *tx, request).await? {
      return Ok(FactoryDecisionSignalRequestDisposition::Replayed);
    }
    consumed_usage(&mut tx, node)
      .await?
      .checked_add(pending_usage(&mut tx, node, None).await?)
      .ok_or_else(conflict)?
      .checked_add(reserved_budget(request.request().budget()))
      .ok_or_else(conflict)?
      .validate(limit)
      .map_err(|_| conflict())?;
    sqlx::query("INSERT INTO factory_decision_signal_requests (request_id,run_id,node_attempt_id,input_digest,request_digest,document,reserved_at_millis) VALUES ($1,$2,$3,$4,$5,$6,$7)")
      .bind(request.id().as_uuid()).bind(node.factory_run_id().as_uuid()).bind(node.id().as_uuid())
      .bind(node.input_digest().as_bytes().as_slice()).bind(request.digest().as_bytes().as_slice()).bind(document)
      .bind(reservation.observed_at.unix_millis()).execute(&mut *tx).await.map_err(unavailable)?;
    let audit = FactoryAuditFact::new(
      node.factory_run_id(),
      AuditActorKind::Worker,
      None,
      FactoryKey::new("factory.decision_signal_reserved").map_err(|_| conflict())?,
      request.digest(),
      FactoryKey::new("accepted").map_err(|_| conflict())?,
      reservation.observed_at,
    );
    crate::factory_run::insert_audit(&mut tx, &audit).await?;
    tx.commit().await.map_err(unavailable)?;
    crate::factory_signal_receipt::emit_committed(
      "octacity.factory.signal.reserved",
      request,
      &reservation.ownership,
      &audit,
      None,
    );
    Ok(FactoryDecisionSignalRequestDisposition::Reserved)
  }
}

/// Pending calls are not assumed free merely because no result was retained.
pub(super) async fn pending_usage(
  tx: &mut sqlx::Transaction<'_, Postgres>,
  node: &NodeAttempt,
  except: Option<DecisionSignalRequestId>,
) -> Result<BudgetUsage, StoreError> {
  let mut rows = sqlx::query_as::<_, RequestRow>(
    "SELECT request.document,request.request_digest FROM factory_decision_signal_requests request WHERE request.node_attempt_id=$1 AND NOT EXISTS (SELECT 1 FROM factory_decision_signal_receipts receipt WHERE receipt.request_id=request.request_id) LIMIT $2",
  ).bind(node.id().as_uuid()).bind(i64::from(MAX_FACTORY_ATTEMPTS) + 1).fetch(&mut **tx);
  let mut usage = BudgetUsage::default();
  let mut count = 0_u32;
  let mut bytes = 0_usize;
  while let Some(row) = rows.try_next().await.map_err(unavailable)? {
    count += 1;
    bytes = bytes.checked_add(row.0.len()).ok_or_else(conflict)?;
    if count > MAX_FACTORY_ATTEMPTS || bytes > 64 * 1024 * 1024 {
      return Err(conflict());
    }
    let request = decode(row)?;
    if request.request().node_attempt_id() != node.id() || request.request().run_id() != node.factory_run_id() {
      return Err(StoreError::Unavailable);
    }
    if Some(request.id()) != except {
      usage = usage
        .checked_add(reserved_budget(request.request().budget()))
        .ok_or_else(conflict)?;
    }
  }
  Ok(usage)
}
fn reserved_budget(budget: BudgetLimit) -> BudgetUsage {
  BudgetUsage {
    attempts: 1,
    elapsed_millis: budget.max_elapsed_millis(),
    tokens: budget.max_tokens(),
    cost_micro_units: budget.max_cost_micro_units(),
    output_bytes: budget.max_output_bytes(),
  }
}
