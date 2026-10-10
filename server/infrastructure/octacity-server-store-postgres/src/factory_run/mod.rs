//! Atomic PostgreSQL persistence for fenced Factory reconciliation.

mod claim;
mod data;
mod diagnostics;
mod history;
mod outbox;
mod phase_pool;
mod recovery;
mod snapshot;
mod transition;

use std::str::FromStr;

use octacity_server_domain::{ArtifactId, EntityKind, Timestamp};
use octacity_server_factory::{
  BudgetUsage, DecisionSignalProgress, FactoryClaim, FactoryClaimFence, FactoryDigest, FactoryKey,
  FactoryLifecycleProgress, FactoryRun, FactoryRunId, FactoryRunState, FactoryRunVersion, WorkEnvelope,
};
use octacity_server_store::{
  AuditActorKind, FactoryArtifactRole, FactoryAuditFact, FactoryBudgetRecord, FactoryLifecycleCheckpoint,
  FactoryOutboxRecord, FactoryOutboxState, FactoryRunClaimRecord, FactoryRunCurrentProjection, StoreError,
  StoreInputError, StoreOperation,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{FromRow, Postgres, Transaction, types::Json};

use crate::{
  database::unavailable,
  discovery::{positive, timestamp},
};

pub(crate) use claim::{claim, claim_batch};
pub(crate) use diagnostics::list_diagnostics;
pub(crate) use history::{append_history, history_rows};
pub(crate) use outbox::{claim_outbox, settle_outbox};
pub(crate) use phase_pool::{phase_ready_entries, publish_phase_ready, select_phase_ready, selection_for_claim};
pub(crate) use recovery::recover_restored_ownership;
pub(crate) use snapshot::read_snapshot;
pub(crate) use transition::commit_transition;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LifecycleDocument {
  progress: FactoryLifecycleProgress,
  signal: DecisionSignalProgress,
}

#[derive(FromRow)]
struct LockedRunRow {
  envelope: Json<WorkEnvelope>,
  state: String,
  version: i64,
  admitted_at_millis: i64,
  claim_id: Option<Vec<u8>>,
  budget_id: Vec<u8>,
  lifecycle_checkpoint_id: Vec<u8>,
  stage_attempt_id: Option<uuid::Uuid>,
  call_node_id: Option<uuid::Uuid>,
  signal_request_id: Option<uuid::Uuid>,
  signal_receipt_id: Option<uuid::Uuid>,
  build_id: Option<uuid::Uuid>,
  changeset_id: Option<uuid::Uuid>,
  evidence_manifest_id: Option<uuid::Uuid>,
  evaluation_plan_id: Option<uuid::Uuid>,
  decision_id: Option<uuid::Uuid>,
  escalation_id: Option<uuid::Uuid>,
  delivery_attempt_id: Option<uuid::Uuid>,
  reporting_attempt_id: Option<uuid::Uuid>,
  usage: Json<BudgetUsage>,
  budget_recorded_at_millis: i64,
  lifecycle: Json<LifecycleDocument>,
  cancellation_requested: bool,
  lifecycle_recorded_at_millis: i64,
}

struct LockedRun {
  work: WorkEnvelope,
  run: FactoryRun,
  admitted_at: Timestamp,
  claim_id: Option<FactoryDigest>,
  current: FactoryRunCurrentProjection,
  budget: FactoryBudgetRecord,
  lifecycle: FactoryLifecycleCheckpoint,
}

#[derive(Clone, FromRow)]
struct OutboxRow {
  id: Vec<u8>,
  operation_id: Vec<u8>,
  run_id: uuid::Uuid,
  kind: String,
  input_digest: Vec<u8>,
  state: String,
  attempt: i32,
  available_at_millis: i64,
  recorded_at_millis: i64,
  owner: Option<String>,
  fence: Option<Vec<u8>>,
  claimed_at_millis: Option<i64>,
  claim_expires_at_millis: Option<i64>,
}

#[derive(FromRow)]
struct FactoryAuditRow {
  id: Vec<u8>,
  run_id: uuid::Uuid,
  actor_kind: String,
  actor_identity_digest: Option<Vec<u8>>,
  operation: String,
  request_identity_digest: Vec<u8>,
  outcome: String,
  recorded_at_millis: i64,
}

async fn lock_run(transaction: &mut Transaction<'_, Postgres>, run_id: FactoryRunId) -> Result<LockedRun, StoreError> {
  let row = sqlx::query_as::<_, LockedRunRow>(
    "SELECT work.envelope, run.state, run.version, \
            FLOOR(EXTRACT(EPOCH FROM run.admitted_at) * 1000)::BIGINT AS admitted_at_millis, \
            current.claim_id, current.budget_id, current.lifecycle_checkpoint_id, \
            current.stage_attempt_id, current.call_node_id, current.signal_request_id, \
            current.signal_receipt_id, current.build_id, current.changeset_id, \
            current.evidence_manifest_id, current.evaluation_plan_id, current.decision_id, \
            current.escalation_id, current.delivery_attempt_id, current.reporting_attempt_id, \
            budget.usage, FLOOR(EXTRACT(EPOCH FROM budget.recorded_at) * 1000)::BIGINT \
              AS budget_recorded_at_millis, \
            lifecycle.lifecycle, lifecycle.cancellation_requested, \
            FLOOR(EXTRACT(EPOCH FROM lifecycle.recorded_at) * 1000)::BIGINT \
              AS lifecycle_recorded_at_millis \
     FROM factory_runs AS run \
     JOIN factory_work_envelopes AS work ON work.id = run.work_envelope_id \
     JOIN factory_run_current AS current ON current.run_id = run.id \
     JOIN factory_run_budgets AS budget ON budget.id = current.budget_id \
     JOIN factory_lifecycle_checkpoints AS lifecycle ON lifecycle.id = current.lifecycle_checkpoint_id \
     WHERE run.id = $1 AND run.visible FOR UPDATE OF run, current",
  )
  .bind(run_id.as_uuid())
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::FactoryRun,
  })?;
  decode_locked_run(run_id, row)
}

fn decode_locked_run(run_id: FactoryRunId, row: LockedRunRow) -> Result<LockedRun, StoreError> {
  let work = row.envelope.0;
  let state = FactoryRunState::from_str(&row.state).map_err(|_| StoreError::Unavailable)?;
  let version = FactoryRunVersion::new(positive(row.version)?).map_err(|_| StoreError::Unavailable)?;
  let run = FactoryRun::restore(
    run_id,
    work.configuration().clone(),
    &work,
    work.subject().clone(),
    state,
    version,
  )
  .map_err(|_| StoreError::Unavailable)?;
  let budget_id = digest(&row.budget_id)?;
  let lifecycle_checkpoint_id = digest(&row.lifecycle_checkpoint_id)?;
  let current = FactoryRunCurrentProjection {
    budget_id,
    lifecycle_checkpoint_id,
    stage_attempt_id: optional_id(row.stage_attempt_id)?,
    macro_call_id: optional_id(row.call_node_id)?,
    signal_request_id: optional_id(row.signal_request_id)?,
    signal_receipt_id: optional_id(row.signal_receipt_id)?,
    build_id: optional_domain_id(row.build_id)?,
    candidate_id: optional_id(row.changeset_id)?,
    evidence_id: optional_id(row.evidence_manifest_id)?,
    evaluation_plan_id: optional_id(row.evaluation_plan_id)?,
    decision_id: optional_id(row.decision_id)?,
    escalation_id: optional_id(row.escalation_id)?,
    delivery_attempt_id: optional_id(row.delivery_attempt_id)?,
    reporting_attempt_id: optional_id(row.reporting_attempt_id)?,
  };
  let LifecycleDocument { progress, signal } = row.lifecycle.0;
  Ok(LockedRun {
    work,
    run,
    admitted_at: timestamp(row.admitted_at_millis)?,
    claim_id: row.claim_id.as_deref().map(digest).transpose()?,
    current,
    budget: FactoryBudgetRecord {
      id: budget_id,
      run_version: version,
      usage: row.usage.0,
      recorded_at: timestamp(row.budget_recorded_at_millis)?,
    },
    lifecycle: FactoryLifecycleCheckpoint {
      id: lifecycle_checkpoint_id,
      run_id,
      run_version: version,
      progress,
      signal,
      cancellation_requested: row.cancellation_requested,
      recorded_at: timestamp(row.lifecycle_recorded_at_millis)?,
    },
  })
}

async fn read_claim(
  transaction: &mut Transaction<'_, Postgres>,
  id: FactoryDigest,
) -> Result<Option<FactoryRunClaimRecord>, StoreError> {
  let row: Option<(uuid::Uuid, String, Vec<u8>, i64, i64)> = sqlx::query_as(
    "SELECT run_id, owner, fence, \
            FLOOR(EXTRACT(EPOCH FROM claimed_at) * 1000)::BIGINT, \
            FLOOR(EXTRACT(EPOCH FROM expires_at) * 1000)::BIGINT \
     FROM factory_run_claims WHERE id = $1",
  )
  .bind(id.as_bytes().as_slice())
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)?;
  row.map(decode_claim).transpose()
}

fn decode_claim(row: (uuid::Uuid, String, Vec<u8>, i64, i64)) -> Result<FactoryRunClaimRecord, StoreError> {
  let (run_id, owner, fence, claimed_at, expires_at) = row;
  let run_id = FactoryRunId::from_uuid(run_id).map_err(|_| StoreError::Unavailable)?;
  let owner = FactoryKey::new(owner).map_err(|_| StoreError::Unavailable)?;
  let claim = FactoryClaim::new(
    FactoryClaimFence::new(digest(&fence)?),
    timestamp(claimed_at)?,
    timestamp(expires_at)?,
  )
  .map_err(|_| StoreError::Unavailable)?;
  Ok(FactoryRunClaimRecord::new(run_id, owner, claim))
}

async fn insert_claim(
  transaction: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
  record: &FactoryRunClaimRecord,
) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO factory_run_claims (id, run_id, owner, fence, claimed_at, expires_at) \
     VALUES ($1, $2, $3, $4, to_timestamp($5::double precision / 1000.0), \
             to_timestamp($6::double precision / 1000.0))",
  )
  .bind(record.id.as_bytes().as_slice())
  .bind(run_id.as_uuid())
  .bind(record.owner.as_str())
  .bind(record.claim.fence().digest().as_bytes().as_slice())
  .bind(record.claim.claimed_at().unix_millis())
  .bind(record.claim.expires_at().unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  Ok(())
}

async fn insert_audit(transaction: &mut Transaction<'_, Postgres>, fact: &FactoryAuditFact) -> Result<(), StoreError> {
  let audit_id = uuid_from_digest(fact.id);
  sqlx::query(
    "INSERT INTO audit_facts \
       (id, actor_kind, actor_identity, operation, target_kind, target_identity, request_identity, \
        idempotency_key, outcome, safe_metadata, occurred_at) \
     VALUES ($1, $2, NULL, $3, 'factory_run', $4, NULL, NULL, 'accepted', $5, \
             to_timestamp($6::double precision / 1000.0))",
  )
  .bind(audit_id)
  .bind(fact.actor_kind.as_str())
  .bind(fact.operation.as_str())
  .bind(fact.run_id.to_string())
  .bind(Json(json!({})))
  .bind(fact.recorded_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  sqlx::query(
    "INSERT INTO factory_audit_links \
       (id, run_id, audit_fact_id, actor_kind, actor_identity_digest, operation, \
        request_identity_digest, outcome, recorded_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, to_timestamp($9::double precision / 1000.0))",
  )
  .bind(fact.id.as_bytes().as_slice())
  .bind(fact.run_id.as_uuid())
  .bind(audit_id)
  .bind(fact.actor_kind.as_str())
  .bind(fact.actor_identity_digest.map(|value| value.as_bytes().to_vec()))
  .bind(fact.operation.as_str())
  .bind(fact.request_identity_digest.as_bytes().as_slice())
  .bind(fact.outcome.as_str())
  .bind(fact.recorded_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  Ok(())
}

async fn read_audit(
  transaction: &mut Transaction<'_, Postgres>,
  id: FactoryDigest,
) -> Result<Option<FactoryAuditFact>, StoreError> {
  let row = sqlx::query_as::<_, FactoryAuditRow>(
    "SELECT id, run_id, actor_kind, actor_identity_digest, operation, request_identity_digest, outcome, \
            FLOOR(EXTRACT(EPOCH FROM recorded_at) * 1000)::BIGINT AS recorded_at_millis \
     FROM factory_audit_links WHERE id = $1",
  )
  .bind(id.as_bytes().as_slice())
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)?;
  row.map(decode_audit_row).transpose()
}

async fn insert_outbox(
  transaction: &mut Transaction<'_, Postgres>,
  record: &FactoryOutboxRecord,
) -> Result<(), StoreError> {
  let ownership = record.claim.map(|claim| {
    (
      record.owner.as_ref().map(FactoryKey::as_str),
      Some(claim.fence().digest().as_bytes().to_vec()),
      Some(claim.claimed_at().unix_millis()),
      Some(claim.expires_at().unix_millis()),
    )
  });
  let (owner, fence, claimed_at, expires_at) = ownership.unwrap_or((None, None, None, None));
  sqlx::query(
    "INSERT INTO factory_outbox_records \
       (id, operation_id, run_id, kind, input_digest, state, attempt, available_at, recorded_at, \
        owner, fence, claimed_at, claim_expires_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, to_timestamp($8::double precision / 1000.0), \
             to_timestamp($9::double precision / 1000.0), $10, $11, \
             CASE WHEN $12::BIGINT IS NULL THEN NULL ELSE to_timestamp($12::double precision / 1000.0) END, \
             CASE WHEN $13::BIGINT IS NULL THEN NULL ELSE to_timestamp($13::double precision / 1000.0) END)",
  )
  .bind(record.id.as_bytes().as_slice())
  .bind(record.operation_id.as_bytes().as_slice())
  .bind(record.run_id.as_uuid())
  .bind(record.kind.as_str())
  .bind(record.input_digest.as_bytes().as_slice())
  .bind(outbox_state(record.state))
  .bind(i32::from(record.attempt))
  .bind(record.available_at.unix_millis())
  .bind(record.recorded_at.unix_millis())
  .bind(owner)
  .bind(fence)
  .bind(claimed_at)
  .bind(expires_at)
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  Ok(())
}

pub(crate) async fn insert_artifact_reference(
  transaction: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
  artifact_id: ArtifactId,
  role: FactoryArtifactRole,
  expected_sha256: Option<[u8; 32]>,
  created_at: Timestamp,
) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO factory_artifact_references \
       (run_id, artifact_id, role, expected_sha256, created_at) \
     VALUES ($1, $2, $3, $4, to_timestamp($5::double precision / 1000.0)) \
     ON CONFLICT (run_id, artifact_id, role) DO UPDATE SET artifact_id = EXCLUDED.artifact_id \
     WHERE factory_artifact_references.expected_sha256 IS NOT DISTINCT FROM EXCLUDED.expected_sha256 \
       AND factory_artifact_references.created_at = EXCLUDED.created_at",
  )
  .bind(run_id.as_uuid())
  .bind(artifact_id.as_uuid())
  .bind(role.as_str())
  .bind(expected_sha256.map(|digest| digest.to_vec()))
  .bind(created_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)
  .and_then(|result| {
    if result.rows_affected() == 1 {
      Ok(())
    } else {
      Err(StoreError::Conflict {
        entity: EntityKind::Artifact,
      })
    }
  })
}

fn decode_audit_row(row: FactoryAuditRow) -> Result<FactoryAuditFact, StoreError> {
  let fact = FactoryAuditFact::new(
    FactoryRunId::from_uuid(row.run_id).map_err(|_| StoreError::Unavailable)?,
    AuditActorKind::from_str(&row.actor_kind).map_err(|_| StoreError::Unavailable)?,
    row.actor_identity_digest.as_deref().map(digest).transpose()?,
    FactoryKey::new(row.operation).map_err(|_| StoreError::Unavailable)?,
    digest(&row.request_identity_digest)?,
    FactoryKey::new(row.outcome).map_err(|_| StoreError::Unavailable)?,
    timestamp(row.recorded_at_millis)?,
  );
  if fact.id != digest(&row.id)? {
    return Err(StoreError::Unavailable);
  }
  Ok(fact)
}

fn digest(bytes: &[u8]) -> Result<FactoryDigest, StoreError> {
  let bytes: [u8; 32] = bytes.try_into().map_err(|_| StoreError::Unavailable)?;
  Ok(FactoryDigest::from_bytes(bytes))
}

fn uuid_from_digest(value: FactoryDigest) -> uuid::Uuid {
  let mut bytes: [u8; 16] = value.as_bytes()[..16]
    .try_into()
    .expect("digest prefix has fixed length");
  bytes[6] = (bytes[6] & 0x0f) | 0x50;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  uuid::Uuid::from_bytes(bytes)
}

fn optional_id<T>(value: Option<uuid::Uuid>) -> Result<Option<T>, StoreError>
where
  T: FromStr,
{
  value
    .map(|value| T::from_str(&value.hyphenated().to_string()).map_err(|_| StoreError::Unavailable))
    .transpose()
}

fn optional_domain_id<T>(value: Option<uuid::Uuid>) -> Result<Option<T>, StoreError>
where
  T: FromStr,
{
  optional_id(value)
}

fn outbox_state(state: FactoryOutboxState) -> &'static str {
  match state {
    FactoryOutboxState::Pending => "pending",
    FactoryOutboxState::Claimed => "claimed",
    FactoryOutboxState::Delivered => "delivered",
    FactoryOutboxState::Failed => "failed",
  }
}

fn decode_outbox(row: OutboxRow) -> Result<FactoryOutboxRecord, StoreError> {
  let state = match row.state.as_str() {
    "pending" => FactoryOutboxState::Pending,
    "claimed" => FactoryOutboxState::Claimed,
    "delivered" => FactoryOutboxState::Delivered,
    "failed" => FactoryOutboxState::Failed,
    _ => return Err(StoreError::Unavailable),
  };
  let owner = row
    .owner
    .map(FactoryKey::new)
    .transpose()
    .map_err(|_| StoreError::Unavailable)?;
  let claim = match (row.fence, row.claimed_at_millis, row.claim_expires_at_millis) {
    (Some(fence), Some(claimed_at), Some(expires_at)) => Some(
      FactoryClaim::new(
        FactoryClaimFence::new(digest(&fence)?),
        timestamp(claimed_at)?,
        timestamp(expires_at)?,
      )
      .map_err(|_| StoreError::Unavailable)?,
    ),
    (None, None, None) => None,
    _ => return Err(StoreError::Unavailable),
  };
  let record = FactoryOutboxRecord {
    id: digest(&row.id)?,
    operation_id: digest(&row.operation_id)?,
    run_id: FactoryRunId::from_uuid(row.run_id).map_err(|_| StoreError::Unavailable)?,
    kind: FactoryKey::new(row.kind).map_err(|_| StoreError::Unavailable)?,
    input_digest: digest(&row.input_digest)?,
    state,
    attempt: u16::try_from(row.attempt).map_err(|_| StoreError::Unavailable)?,
    available_at: timestamp(row.available_at_millis)?,
    recorded_at: timestamp(row.recorded_at_millis)?,
    owner,
    claim,
  };
  if !record.is_canonical() {
    return Err(StoreError::Unavailable);
  }
  Ok(record)
}

fn invalid(operation: StoreOperation, source: StoreInputError) -> StoreError {
  StoreError::InvalidInput { operation, source }
}

const fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::FactoryRun,
  }
}

fn version_number(version: FactoryRunVersion) -> Result<i64, StoreError> {
  i64::try_from(version.get()).map_err(|_| StoreError::Unavailable)
}
