//! Operator intent is atomic with the aggregate version and its durable replay.
//! Responses are restored against the admitted schema, rather than deserialized
//! as trusted payloads or translated into a business lifecycle state.

use octacity_server_domain::{EntityKind, Timestamp};
use octacity_server_factory::{
  FactoryDigest, FactoryKey, FactoryRunId, FactoryRunVersion, FlowNodeKind, FlowPayload, NodeAttemptId,
};
use octacity_server_store::{
  ApplyFactoryRunControl, FactoryRunControlIntent, FactoryRunControlOutcome, FactoryRunControlRecord,
  ManagementMutation, MutationDisposition, StoreError, prepare_factory_run_control,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction, types::Json};

use super::{
  conflict, digest, insert_audit,
  snapshot::read_snapshot_in_transaction,
  transition::{insert_budget, insert_lifecycle},
  version_number,
};
use crate::{
  database::unavailable,
  discovery::{positive, timestamp},
  mutation::{self, MutationFacts, MutationIdentity, MutationKind, MutationStart},
};

pub(crate) async fn apply_control(
  pool: &PgPool,
  request: ManagementMutation<ApplyFactoryRunControl>,
) -> Result<FactoryRunControlOutcome, StoreError> {
  let (request, context) = request.into_parts();
  let identity = MutationIdentity::new_management(
    &context,
    MutationKind::ControlFactoryRun,
    format!("{}:{}", request.intent.operation(), request.idempotency_key.as_str()),
    request.requested_at,
    EntityKind::FactoryRun,
    &request.replay_intent(),
  )?;
  let mut transaction = match mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(value) => {
      let mut outcome: FactoryRunControlOutcome = mutation::decode_outcome(value)?;
      outcome.disposition = MutationDisposition::Replayed;
      return Ok(outcome);
    }
  };
  let snapshot = read_snapshot_in_transaction(&mut transaction, request.run_id).await?;
  let (count, _) = super::snapshot::snapshot_record_counts(&mut transaction, request.run_id).await?;
  let change = prepare_factory_run_control(&snapshot, &request, &context, count)?;
  if let Some(role) = &change.selected_role {
    crate::factory_role_deletion::require_roles_available(&mut transaction, role.project_id, &[role.id].into()).await?;
  }
  insert_budget(&mut transaction, request.run_id, &change.budget).await?;
  insert_lifecycle(&mut transaction, &change.lifecycle).await?;
  insert_audit(&mut transaction, &change.audit).await?;
  sqlx::query("INSERT INTO factory_run_controls (id, run_id, run_version, control_kind, request_identity_digest, control, recorded_at) \
    VALUES ($1, $2, $3, $4, $5, $6, to_timestamp($7::double precision / 1000.0))")
    .bind(change.control.id.as_bytes().as_slice()).bind(request.run_id.as_uuid()).bind(version_number(change.control.run_version)?)
    .bind(request.intent.operation()).bind(change.audit.request_identity_digest.as_bytes().as_slice())
    .bind(Json(&change.control)).bind(request.requested_at.unix_millis()).execute(&mut *transaction).await.map_err(unavailable)?;
  let updated = sqlx::query("UPDATE factory_runs SET version = $1, updated_at = to_timestamp($2::double precision / 1000.0) WHERE id = $3 AND version = $4")
    .bind(version_number(change.run.version())?).bind(request.requested_at.unix_millis()).bind(request.run_id.as_uuid())
    .bind(version_number(request.expected_version)?).execute(&mut *transaction).await.map_err(unavailable)?;
  if updated.rows_affected() != 1 {
    return Err(conflict());
  }
  let updated = sqlx::query(
    "UPDATE factory_run_current SET run_version = $1, budget_id = $2, lifecycle_checkpoint_id = $3, claim_id = NULL \
    WHERE run_id = $4 AND run_version = $5",
  )
  .bind(version_number(change.run.version())?)
  .bind(change.budget.id.as_bytes().as_slice())
  .bind(change.lifecycle.id.as_bytes().as_slice())
  .bind(request.run_id.as_uuid())
  .bind(version_number(request.expected_version)?)
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?;
  if updated.rows_affected() != 1 {
    return Err(conflict());
  }
  let facts = MutationFacts::management(
    &context,
    request.run_id.to_string(),
    json!({}),
    json!({"run_id": request.run_id, "run_version": change.run.version(), "operation": request.intent.operation()}),
  );
  mutation::commit(
    transaction,
    &identity,
    facts,
    mutation::encode_outcome(&change.outcome)?,
  )
  .await?;
  Ok(change.outcome)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlDocument {
  id: FactoryDigest,
  run_id: FactoryRunId,
  run_version: FactoryRunVersion,
  intent: ControlIntentDocument,
  recorded_at: Timestamp,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
enum ControlIntentDocument {
  Cancel,
  SelectProfile {
    input_digest: FactoryDigest,
    profile: octacity_server_factory::FactoryRoleProfileRef,
  },
  RetryNode {
    node_attempt_id: NodeAttemptId,
  },
  ResolveNode {
    node_attempt_id: NodeAttemptId,
    outcome: FactoryKey,
    payload: Value,
  },
}

/// The bounded snapshot is assembled first so each persisted response can be
/// checked against its exact attempt, admitted schema and immutable audit link.
pub(super) async fn read_controls(
  transaction: &mut Transaction<'_, Postgres>,
  run: &octacity_server_factory::FactoryRun,
  admitted: &octacity_server_factory::AdmittedFlow,
  attempts: &[octacity_server_factory::NodeAttempt],
  runs: &[octacity_server_factory::FlowRun],
  stops: &[octacity_server_factory::FlowNodeStop],
) -> Result<Vec<FactoryRunControlRecord>, StoreError> {
  let rows = sqlx::query_as::<_, (Vec<u8>, i64, String, Vec<u8>, Json<Value>, i64)>(
    "SELECT id, run_version, control_kind, request_identity_digest, control, FLOOR(EXTRACT(EPOCH FROM recorded_at) * 1000)::BIGINT \
     FROM factory_run_controls WHERE run_id = $1 ORDER BY run_version, id")
    .bind(run.id().as_uuid()).fetch_all(&mut **transaction).await.map_err(unavailable)?;
  // Only the audit proof for each retained control belongs in its validation
  // working set. Unbounded claim/audit history remains separately pageable.
  let audits = sqlx::query_as::<_, super::FactoryAuditRow>(
    "SELECT audit.id, audit.run_id, audit.actor_kind, audit.actor_identity_digest, audit.operation, \
       audit.request_identity_digest, audit.outcome, \
       FLOOR(EXTRACT(EPOCH FROM audit.recorded_at) * 1000)::BIGINT AS recorded_at_millis \
     FROM factory_run_controls AS control JOIN LATERAL ( \
       SELECT * FROM factory_audit_links WHERE run_id = control.run_id \
         AND operation = control.control_kind AND request_identity_digest = control.request_identity_digest \
         AND recorded_at = control.recorded_at ORDER BY id LIMIT 1 \
     ) AS audit ON TRUE WHERE control.run_id = $1",
  )
  .bind(run.id().as_uuid())
  .fetch_all(&mut **transaction)
  .await
  .map_err(unavailable)?
  .into_iter()
  .map(super::decode_audit_row)
  .collect::<Result<Vec<_>, _>>()?;
  let mut controls = Vec::with_capacity(rows.len());
  for (id, version, kind, request_digest, Json(value), at) in rows {
    let document: ControlDocument = serde_json::from_value(value.clone()).map_err(|_| StoreError::Unavailable)?;
    let intent = match document.intent {
      ControlIntentDocument::Cancel => FactoryRunControlIntent::Cancel,
      ControlIntentDocument::SelectProfile { input_digest, profile } => {
        FactoryRunControlIntent::SelectProfile { input_digest, profile }
      }
      ControlIntentDocument::RetryNode { node_attempt_id } => {
        if controls.iter().any(|row: &FactoryRunControlRecord| matches!(row.intent, FactoryRunControlIntent::RetryNode { node_attempt_id: previous } if previous == node_attempt_id))
          || !stops.iter().any(|row| row.node_attempt_id() == node_attempt_id && row.reason() == octacity_server_factory::FlowNodeStopReason::ExecutionFailed && row.observed_at() <= document.recorded_at)
        { return Err(StoreError::Unavailable); }
        FactoryRunControlIntent::RetryNode { node_attempt_id }
      }
      ControlIntentDocument::ResolveNode {
        node_attempt_id,
        outcome,
        payload,
      } => {
        if controls.iter().any(|record: &FactoryRunControlRecord| matches!(record.intent, FactoryRunControlIntent::ResolveNode { node_attempt_id: previous, .. } if previous == node_attempt_id)) { return Err(StoreError::Unavailable); }
        let attempt = attempts
          .iter()
          .find(|record| record.id() == node_attempt_id)
          .ok_or(StoreError::Unavailable)?;
        let node = runs
          .iter()
          .find(|run| run.id() == attempt.flow_run_id())
          .and_then(|run| admitted.closure().definition(run.definition()))
          .and_then(|definition| definition.node(attempt.node_key()))
          .ok_or(StoreError::Unavailable)?;
        if node.kind() != FlowNodeKind::HumanGate
          || document.recorded_at < attempt.claim().claimed_at()
          || document.recorded_at > attempt.deadline()
        {
          return Err(StoreError::Unavailable);
        }
        let schema = node
          .outcome(&outcome)
          .and_then(|outcome| admitted.data_schema(outcome.schema()))
          .ok_or(StoreError::Unavailable)?;
        let payload = FlowPayload::restore(
          &serde_json::to_vec(&payload).map_err(|_| StoreError::Unavailable)?,
          schema,
        )
        .map_err(|_| StoreError::Unavailable)?;
        FactoryRunControlIntent::ResolveNode {
          node_attempt_id,
          outcome,
          payload,
        }
      }
    };
    let record = FactoryRunControlRecord::new(document.run_id, document.run_version, intent, document.recorded_at);
    if record.id != document.id
      || record.id != digest(&id)?
      || record.run_id != run.id()
      || record.run_version.get() != positive(version)?
      || record.run_version > run.version()
      || record.recorded_at != timestamp(at)?
      || record.intent.operation() != kind
      || serde_json::to_value(&record).map_err(|_| StoreError::Unavailable)? != value
      || !audits.iter().any(|audit| {
        audit.operation.as_str() == kind
          && audit.recorded_at == record.recorded_at
          && audit.request_identity_digest.as_bytes().as_slice() == request_digest
      })
    {
      return Err(StoreError::Unavailable);
    }
    controls.push(record);
  }
  Ok(controls)
}
