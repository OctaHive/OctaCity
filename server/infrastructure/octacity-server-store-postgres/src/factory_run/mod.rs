//! Atomic PostgreSQL persistence for fenced Factory reconciliation.

mod claim;
mod control;
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
  BudgetUsage, FactoryClaim, FactoryClaimFence, FactoryDigest, FactoryKey, FactoryLifecycleProgress, FactoryRun,
  FactoryRunId, FactoryRunState, FactoryRunVersion, WorkEnvelope,
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
pub(crate) use control::apply_control;
pub(crate) use diagnostics::list_diagnostics;
pub(crate) use history::{append_history, history_rows};
pub(crate) use outbox::{claim_outbox, latest as latest_outbox, lock_subject as lock_outbox_subject, settle_outbox};
pub(crate) use phase_pool::{
  list_phase_ready_entries, phase_ready_entries, publish_phase_ready, ready_phase_policies, select_phase_ready,
  selection_for_claim,
};
pub(crate) use recovery::recover_restored_ownership;
pub(crate) use snapshot::{read_snapshot, read_snapshot_in_transaction};
pub(crate) use transition::commit_transition;

pub(crate) async fn verify_cancellation_owner(
  transaction: &mut Transaction<'_, Postgres>,
  run: FactoryRunId,
  request: &octacity_server_store::CancelFactoryBuildExecution,
) -> Result<(), StoreError> {
  let locked = lock_run(transaction, run).await?;
  let claim = read_claim(transaction, locked.claim_id.ok_or_else(conflict)?)
    .await?
    .ok_or(StoreError::Unavailable)?;
  if claim.owner != *request.ownership.owner()
    || claim.claim != request.ownership.claim()
    || request.observed_at < claim.claim.claimed_at()
    || request.observed_at >= claim.claim.expires_at()
  {
    return Err(conflict());
  }
  Ok(())
}

pub(crate) async fn verify_build_dispatch(
  transaction: &mut Transaction<'_, Postgres>,
  request: &octacity_server_store::AcceptFactoryBuild,
) -> Result<(), StoreError> {
  verify_node_dispatch(transaction, &request.intent, &request.ownership, request.accepted_at).await
}

pub(crate) async fn verify_node_dispatch(
  transaction: &mut Transaction<'_, Postgres>,
  intent: &octacity_server_factory::FlowBuildIntent,
  ownership: &octacity_server_factory::FactoryClaimOwnership,
  observed_at: Timestamp,
) -> Result<(), StoreError> {
  let run_id = intent.input().factory_run_id();
  let locked = lock_run(transaction, run_id).await?;
  let claim_id = locked.claim_id.ok_or_else(conflict)?;
  let claim = read_claim(transaction, claim_id)
    .await?
    .ok_or(StoreError::Unavailable)?;
  if locked.lifecycle.cancellation_requested
    || locked.run.state().is_terminal()
    || claim.owner != *ownership.owner()
    || claim.claim != ownership.claim()
    || observed_at < claim.claim.claimed_at()
    || observed_at >= claim.claim.expires_at()
    || observed_at >= intent.node().deadline()
    || intent.node().verify_observer(ownership, observed_at).is_err()
  {
    return Err(conflict());
  }
  let document: Option<Vec<u8>> = sqlx::query_scalar(
    "SELECT intent.document FROM factory_flow_build_intents intent \
     WHERE intent.run_id=$1 AND intent.node_attempt_id=$2 AND intent.operation_id=$3 \
       AND NOT EXISTS (SELECT 1 FROM factory_node_stops stop WHERE stop.node_attempt_id=intent.node_attempt_id) \
       AND NOT EXISTS (SELECT 1 FROM factory_node_attempt_completions completion WHERE completion.node_attempt_id=intent.node_attempt_id)",
  ).bind(run_id.as_uuid()).bind(intent.node().id().as_uuid())
    .bind(intent.operation_id().map_err(|_| conflict())?.as_bytes().as_slice())
    .fetch_optional(&mut **transaction).await.map_err(unavailable)?;
  let expected = serde_json::to_vec(intent).map_err(|_| conflict())?;
  if document.as_ref() != Some(&expected) {
    return Err(conflict());
  }
  Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LifecycleDocument {
  progress: FactoryLifecycleProgress,
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
  run_id: Option<uuid::Uuid>,
  issue: Option<Json<octacity_server_store::IssueProjectionSubject>>,
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
  // Lock the stable aggregate before reading mutable current pointers. A joined
  // SELECT FOR UPDATE can wait on an older tuple, then PostgreSQL's row recheck
  // sees the new pointer against its old joined budget/lifecycle row and drops
  // an existing Run. The next statement gets a fresh snapshot under this lock.
  sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM factory_runs WHERE id=$1 AND visible FOR UPDATE")
    .bind(run_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(unavailable)?
    .ok_or(StoreError::NotFound {
      entity: EntityKind::FactoryRun,
    })?;
  let row = sqlx::query_as::<_, LockedRunRow>(
    "SELECT work.envelope, run.state, run.version, \
            FLOOR(EXTRACT(EPOCH FROM run.admitted_at) * 1000)::BIGINT AS admitted_at_millis, \
            current.claim_id, current.budget_id, current.lifecycle_checkpoint_id, \
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
  };
  let LifecycleDocument { progress } = row.lifecycle.0;
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

pub(crate) async fn insert_audit(
  transaction: &mut Transaction<'_, Postgres>,
  fact: &FactoryAuditFact,
) -> Result<(), StoreError> {
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

pub(crate) async fn insert_outbox(
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
        owner, fence, claimed_at, claim_expires_at, issue) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, to_timestamp($8::double precision / 1000.0), \
             to_timestamp($9::double precision / 1000.0), $10, $11, \
             CASE WHEN $12::BIGINT IS NULL THEN NULL ELSE to_timestamp($12::double precision / 1000.0) END, \
             CASE WHEN $13::BIGINT IS NULL THEN NULL ELSE to_timestamp($13::double precision / 1000.0) END, $14)",
  )
  .bind(record.id.as_bytes().as_slice())
  .bind(record.operation_id.as_bytes().as_slice())
  .bind(record.subject.run_id().map(|r| r.as_uuid()))
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
  .bind(match record.subject {
    octacity_server_store::FactoryOutboxSubject::Issue(issue) => Some(Json(issue)),
    _ => None,
  })
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
    subject: match (row.run_id, row.issue) {
      (Some(run), None) => octacity_server_store::FactoryOutboxSubject::Run(
        FactoryRunId::from_uuid(run).map_err(|_| StoreError::Unavailable)?,
      ),
      (None, Some(Json(issue))) => octacity_server_store::FactoryOutboxSubject::Issue(issue),
      _ => return Err(StoreError::Unavailable),
    },
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
