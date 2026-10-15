//! Immutable private signal observations; only safe digests leave this adapter.
use crate::{PostgresStore, database::unavailable};
use async_trait::async_trait;
use futures_util::TryStreamExt;
use octacity_server_domain::EntityKind;
use octacity_server_factory::*;
use octacity_server_store::{
  AuditActorKind, FactoryAuditFact, FactoryDecisionSignalNodeUsage, FactoryDecisionSignalReceiptStore,
  FactoryDecisionSignalUsageStore, PublishFactoryDecisionSignalReceipt, StoreError,
};
use sqlx::{PgPool, Postgres};

type ReceiptRow = (Vec<u8>, Vec<u8>, Vec<u8>);

#[async_trait]
impl FactoryDecisionSignalUsageStore for PostgresStore {
  async fn factory_decision_signal_node_usage(
    &self,
    run_id: FactoryRunId,
    node_id: NodeAttemptId,
  ) -> Result<FactoryDecisionSignalNodeUsage, StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    let snapshot = crate::factory_run::read_snapshot_in_transaction(&mut tx, run_id).await?;
    let node = snapshot
      .flow
      .attempts
      .iter()
      .find(|node| node.id() == node_id)
      .ok_or_else(conflict)?;
    let result = FactoryDecisionSignalNodeUsage {
      consumed: consumed_usage(&mut tx, node).await?,
      pending: crate::factory_signal_request::pending_usage(&mut tx, node, None).await?,
    };
    result
      .consumed
      .checked_add(result.pending)
      .ok_or_else(conflict)?
      .validate(node.budget())
      .map_err(|_| conflict())?;
    tx.commit().await.map_err(unavailable)?;
    Ok(result)
  }
}
fn decode((document, request_digest, receipt_digest): ReceiptRow) -> Result<DecisionSignalReceipt, StoreError> {
  let receipt: DecisionSignalReceipt = serde_json::from_slice(&document).map_err(|_| StoreError::Unavailable)?;
  receipt.validate_integrity().map_err(|_| StoreError::Unavailable)?;
  if request_digest != receipt.request().digest().as_bytes() || receipt_digest != receipt.receipt_digest().as_bytes() {
    return Err(StoreError::Unavailable);
  }
  Ok(receipt)
}

pub(super) fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::FactoryRun,
  }
}
async fn read<'e, E: sqlx::Executor<'e, Database = Postgres>>(
  executor: E,
  request: &DecisionSignalProviderRequest,
) -> Result<Option<DecisionSignalReceipt>, StoreError> {
  let row: Option<ReceiptRow> = sqlx::query_as(
    "SELECT document,request_digest,receipt_digest FROM factory_decision_signal_receipts WHERE request_id=$1",
  )
  .bind(request.id().as_uuid())
  .fetch_optional(executor)
  .await
  .map_err(unavailable)?;
  row
    .map(|row| {
      let receipt = decode(row)?;
      receipt.replay(request).map_err(|_| conflict())?;
      Ok(receipt)
    })
    .transpose()
}
fn identical(receipt: &DecisionSignalReceipt, expected: &DecisionSignalReceipt) -> Result<(), StoreError> {
  if receipt == expected { Ok(()) } else { Err(conflict()) }
}
#[async_trait]
impl FactoryDecisionSignalReceiptStore for PostgresStore {
  async fn factory_decision_signal_receipt(
    &self,
    request: &DecisionSignalProviderRequest,
  ) -> Result<Option<DecisionSignalReceipt>, StoreError> {
    request.validate_integrity().map_err(|_| conflict())?;
    read(self.pool(), request).await
  }
  async fn publish_factory_decision_signal_receipt(
    &self,
    request: PublishFactoryDecisionSignalReceipt,
  ) -> Result<(), StoreError> {
    publish(self.pool(), request).await
  }
}
async fn publish(pool: &PgPool, request: PublishFactoryDecisionSignalReceipt) -> Result<(), StoreError> {
  let receipt = &request.receipt;
  receipt.validate_integrity().map_err(|_| conflict())?;
  let provider_request = receipt.request();
  if let Some(previous) = read(pool, provider_request).await? {
    return identical(&previous, receipt);
  }
  let document = serde_json::to_vec(receipt).map_err(|_| conflict())?;
  if document.len() > 4 * 1024 * 1024 {
    return Err(conflict());
  }
  let mut tx = pool.begin().await.map_err(unavailable)?;
  // The common bounded snapshot takes the same Run lock as owner transitions
  // and revalidates the retained graph, input context and node journal.
  let snapshot = crate::factory_run::read_snapshot_in_transaction(&mut tx, receipt.run_id()).await?;
  if let Some(previous) = read(&mut *tx, provider_request).await? {
    return identical(&previous, receipt);
  }
  let node = validate_owner(&snapshot, provider_request, &request.ownership, request.observed_at)?;
  // Cancellation revokes dispatch, but an already reserved external operation
  // still needs its measured settlement before the owner can retain a stop.
  if cancellation_requested(&snapshot)? && !crate::factory_signal_request::recorded(&mut *tx, provider_request).await? {
    return Err(conflict());
  }
  consumed_usage(&mut tx, node)
    .await?
    .checked_add(crate::factory_signal_request::pending_usage(&mut tx, node, Some(provider_request.id())).await?)
    .ok_or_else(conflict)?
    .checked_add(receipt.usage())
    .ok_or_else(conflict)?
    .validate(node.budget())
    .map_err(|_| conflict())?;
  sqlx::query("INSERT INTO factory_decision_signal_receipts (request_id,run_id,node_attempt_id,input_digest,request_digest,receipt_digest,document,published_at_millis) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
    .bind(receipt.request_id().as_uuid()).bind(receipt.run_id().as_uuid()).bind(node.id().as_uuid())
    .bind(node.input_digest().as_bytes().as_slice()).bind(provider_request.digest().as_bytes().as_slice())
    .bind(receipt.receipt_digest().as_bytes().as_slice()).bind(document).bind(request.observed_at.unix_millis())
    .execute(&mut *tx).await.map_err(unavailable)?;
  let audit = FactoryAuditFact::new(
    receipt.run_id(),
    AuditActorKind::Worker,
    None,
    FactoryKey::new("factory.decision_signal_published").map_err(|_| conflict())?,
    receipt.receipt_digest(),
    FactoryKey::new("accepted").map_err(|_| conflict())?,
    request.observed_at,
  );
  crate::factory_run::insert_audit(&mut tx, &audit).await?;
  tx.commit().await.map_err(unavailable)?;
  emit_committed(
    "octacity.factory.signal.published",
    provider_request,
    &request.ownership,
    &audit,
    Some(receipt),
  );
  Ok(())
}

/// One allowlist shared by dispatch reservation and measured settlement.
/// Callers invoke this only after the audit and immutable journal commit.
pub(super) fn emit_committed(
  event: &'static str,
  request: &DecisionSignalProviderRequest,
  ownership: &FactoryClaimOwnership,
  audit: &FactoryAuditFact,
  receipt: Option<&DecisionSignalReceipt>,
) {
  let budget = request.request().budget();
  let usage = receipt.map(DecisionSignalReceipt::usage);
  tracing::info!(
    event.name = event,
    factory.run.id = %request.request().run_id(),
    factory.node.attempt.id = %request.request().node_attempt_id(),
    factory.node.fence.digest = %ownership.claim().fence().digest(),
    factory.audit.id = %audit.id,
    factory.signal.request.id = %request.id(),
    factory.signal.request.digest = %request.digest(),
    factory.signal.input.digest = %request.request().frozen_input_digest(),
    factory.signal.provider.digest = %request.provider().digest(),
    factory.signal.adapter.digest = %request.adapter().digest(),
    factory.signal.model.digest = %request.model().digest(),
    factory.signal.policy.digest = %request.policy().digest(),
    factory.signal.calibration.digest = %request.threshold().calibration().digest(),
    factory.signal.mode = ?request.mode(),
    factory.signal.receipt.digest = receipt.map(|receipt| receipt.receipt_digest().to_string()),
    factory.signal.budget.attempts = budget.max_attempts(),
    factory.signal.budget.elapsed_ms = budget.max_elapsed_millis(),
    factory.signal.budget.tokens = budget.max_tokens(),
    factory.signal.budget.cost_micro_units = budget.max_cost_micro_units(),
    factory.signal.budget.output_bytes = budget.max_output_bytes(),
    factory.signal.usage.attempts = usage.map(|usage| usage.attempts),
    factory.signal.usage.elapsed_ms = usage.map(|usage| usage.elapsed_millis),
    factory.signal.usage.tokens = usage.map(|usage| usage.tokens),
    factory.signal.usage.cost_micro_units = usage.map(|usage| usage.cost_micro_units),
    factory.signal.usage.output_bytes = usage.map(|usage| usage.output_bytes),
    "Factory Signal journal committed"
  );
}

/// The Run lock serializes this bounded scan with all competing publications.
/// Stream private documents individually instead of allocating the whole journal.
pub(super) async fn consumed_usage(
  tx: &mut sqlx::Transaction<'_, Postgres>,
  node: &NodeAttempt,
) -> Result<BudgetUsage, StoreError> {
  let mut rows = sqlx::query_as::<_, ReceiptRow>(
    "SELECT document,request_digest,receipt_digest FROM factory_decision_signal_receipts WHERE node_attempt_id=$1 LIMIT $2",
  ).bind(node.id().as_uuid()).bind(i64::from(MAX_FACTORY_ATTEMPTS) + 1).fetch(&mut **tx);
  let mut usage = BudgetUsage::default();
  let mut bytes = 0_usize;
  let mut count = 0_u32;
  while let Some(row) = rows.try_next().await.map_err(unavailable)? {
    count += 1;
    bytes = bytes.checked_add(row.0.len()).ok_or_else(conflict)?;
    if count > MAX_FACTORY_ATTEMPTS || bytes > 64 * 1024 * 1024 {
      return Err(conflict());
    }
    let receipt = decode(row)?;
    if receipt.request().request().node_attempt_id() != node.id() || receipt.run_id() != node.factory_run_id() {
      return Err(StoreError::Unavailable);
    }
    usage = usage.checked_add(receipt.usage()).ok_or_else(conflict)?;
  }
  Ok(usage)
}

pub(super) fn cancellation_requested(snapshot: &octacity_server_store::FactoryRunSnapshot) -> Result<bool, StoreError> {
  snapshot
    .lifecycle_checkpoints
    .iter()
    .find(|row| row.id == snapshot.current.lifecycle_checkpoint_id)
    .map(|row| row.cancellation_requested)
    .ok_or_else(conflict)
}

/// Common authoritative observer fence; dispatch and settlement separately
/// enforce cancellation and retain their own deadline rules.
pub(super) fn validate_owner<'a>(
  snapshot: &'a octacity_server_store::FactoryRunSnapshot,
  provider_request: &DecisionSignalProviderRequest,
  ownership: &FactoryClaimOwnership,
  observed_at: octacity_server_domain::Timestamp,
) -> Result<&'a NodeAttempt, StoreError> {
  let node_request = provider_request.request();
  let node = snapshot
    .flow
    .attempts
    .iter()
    .find(|node| node.id() == node_request.node_attempt_id())
    .ok_or_else(conflict)?;
  let input = snapshot
    .flow
    .data
    .inputs
    .iter()
    .find(|input| input.digest().ok() == Some(node_request.frozen_input_digest()))
    .ok_or_else(conflict)?;
  let claim = snapshot.current_claim.as_ref().ok_or_else(conflict)?;
  if snapshot.run.state().is_terminal()
    || claim.owner != *ownership.owner()
    || claim.claim != ownership.claim()
    || node.input_digest() != node_request.frozen_input_digest()
    || input.subject() != node_request.subject()
    || provider_request.deadline() > node.deadline()
    || !node_request.budget().fits_within(node.budget())
    || snapshot
      .flow
      .completions
      .iter()
      .any(|row| row.node_attempt_id() == node.id())
    || snapshot
      .flow
      .data
      .stops
      .iter()
      .any(|row| row.node_attempt_id() == node.id())
  {
    return Err(conflict());
  }
  node.verify_observer(ownership, observed_at).map_err(|_| conflict())?;
  Ok(node)
}
