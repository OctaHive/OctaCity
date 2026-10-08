use std::collections::{HashMap, HashSet};

use octacity_server_factory::{BudgetUsage, FactoryRunId, FactoryRunVersion, MacroCall};
use octacity_server_store::{
  FactoryBudgetRecord, FactoryLifecycleCheckpoint, FactoryRunSnapshot, MAX_FACTORY_RUN_SNAPSHOT_RECORDS, StoreError,
};
use sqlx::{PgPool, types::Json};

use super::{
  FactoryAuditRow, LifecycleDocument, OutboxRow, decode_audit_row, decode_claim, decode_outbox, digest, history_rows,
  lock_run,
};
use crate::{
  database::unavailable,
  discovery::{positive, timestamp},
};

pub(crate) async fn read_snapshot(pool: &PgPool, run_id: FactoryRunId) -> Result<FactoryRunSnapshot, StoreError> {
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let locked = lock_run(&mut transaction, run_id).await?;
  let (record_count, control_count) = snapshot_record_counts(&mut transaction, run_id).await?;
  if record_count > MAX_FACTORY_RUN_SNAPSHOT_RECORDS || control_count != 0 {
    return Err(StoreError::Unavailable);
  }
  let claims = sqlx::query_as::<_, (uuid::Uuid, String, Vec<u8>, i64, i64)>(
    "SELECT run_id, owner, fence, FLOOR(EXTRACT(EPOCH FROM claimed_at) * 1000)::BIGINT, \
            FLOOR(EXTRACT(EPOCH FROM expires_at) * 1000)::BIGINT \
     FROM factory_run_claims WHERE run_id = $1 ORDER BY id",
  )
  .bind(run_id.as_uuid())
  .fetch_all(&mut *transaction)
  .await
  .map_err(unavailable)?
  .into_iter()
  .map(decode_claim)
  .collect::<Result<Vec<_>, _>>()?;
  let budgets = sqlx::query_as::<_, (Vec<u8>, i64, Json<BudgetUsage>, i64)>(
    "SELECT id, run_version, usage, FLOOR(EXTRACT(EPOCH FROM recorded_at) * 1000)::BIGINT \
     FROM factory_run_budgets WHERE run_id = $1 ORDER BY id",
  )
  .bind(run_id.as_uuid())
  .fetch_all(&mut *transaction)
  .await
  .map_err(unavailable)?
  .into_iter()
  .map(|(id, version, Json(usage), recorded_at)| {
    Ok(FactoryBudgetRecord {
      id: digest(&id)?,
      run_version: FactoryRunVersion::new(positive(version)?).map_err(|_| StoreError::Unavailable)?,
      usage,
      recorded_at: timestamp(recorded_at)?,
    })
  })
  .collect::<Result<Vec<_>, StoreError>>()?;
  let lifecycle_checkpoints = sqlx::query_as::<_, (Vec<u8>, i64, Json<LifecycleDocument>, bool, i64)>(
    "SELECT id, run_version, lifecycle, cancellation_requested, \
            FLOOR(EXTRACT(EPOCH FROM recorded_at) * 1000)::BIGINT \
     FROM factory_lifecycle_checkpoints WHERE run_id = $1 ORDER BY id",
  )
  .bind(run_id.as_uuid())
  .fetch_all(&mut *transaction)
  .await
  .map_err(unavailable)?
  .into_iter()
  .map(|(id, version, Json(document), cancellation_requested, recorded_at)| {
    Ok(FactoryLifecycleCheckpoint {
      id: digest(&id)?,
      run_id,
      run_version: FactoryRunVersion::new(positive(version)?).map_err(|_| StoreError::Unavailable)?,
      progress: document.progress,
      signal: document.signal,
      cancellation_requested,
      recorded_at: timestamp(recorded_at)?,
    })
  })
  .collect::<Result<Vec<_>, StoreError>>()?;
  let audit = sqlx::query_as::<_, FactoryAuditRow>(
    "SELECT id, run_id, actor_kind, actor_identity_digest, operation, request_identity_digest, outcome, \
            FLOOR(EXTRACT(EPOCH FROM recorded_at) * 1000)::BIGINT AS recorded_at_millis \
     FROM factory_audit_links WHERE run_id = $1 ORDER BY id",
  )
  .bind(run_id.as_uuid())
  .fetch_all(&mut *transaction)
  .await
  .map_err(unavailable)?
  .into_iter()
  .map(decode_audit_row)
  .collect::<Result<Vec<_>, _>>()?;
  let outbox = outbox_rows(&mut transaction, run_id).await?;
  let current_claim = locked
    .claim_id
    .map(|id| {
      claims
        .iter()
        .find(|record| record.id == id)
        .cloned()
        .ok_or(StoreError::Unavailable)
    })
    .transpose()?;
  let history = history_rows(&mut transaction, run_id).await?;
  validate_history(run_id, &history)?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(FactoryRunSnapshot {
    work: locked.work,
    run: locked.run,
    current_claim,
    claims,
    budgets,
    lifecycle_checkpoints,
    stage_attempts: history.stage_attempts,
    stage_attempt_completions: history.stage_attempt_completions,
    stage_handoffs: history.stage_handoffs,
    context_manifests: history.context_manifests,
    macro_calls: history.macro_calls,
    macro_call_completions: history.macro_call_completions,
    signal_requests: history.signal_requests,
    signal_receipts: history.signal_receipts,
    linked_builds: history.linked_builds,
    build_observations: history.build_observations,
    candidates: history.candidates,
    evidence: history.evidence,
    evaluation_plans: history.evaluation_plans,
    assessments: history.assessments,
    decisions: history.decisions,
    escalations: history.escalations,
    delivery_attempts: history.delivery_attempts,
    reporting_attempts: history.reporting_attempts,
    audit,
    controls: Vec::new(),
    outbox,
    current: locked.current,
  })
}

pub(super) async fn snapshot_record_counts(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  run_id: FactoryRunId,
) -> Result<(usize, usize), StoreError> {
  let (record_count, control_count): (i64, i64) = sqlx::query_as(
    "SELECT \
       (SELECT COUNT(*) FROM factory_run_claims WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_run_budgets WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_lifecycle_checkpoints WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_stage_attempts WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_stage_attempt_completions WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_stage_handoffs WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_context_manifests WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_call_nodes WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_call_completions WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_decision_signal_requests WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_decision_signal_receipts WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_build_links WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_build_observations WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_changesets WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_evidence_manifests WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_evaluation_plans WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_assessments WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_decisions WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_escalations WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_delivery_attempts WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_reporting_attempts WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_audit_links WHERE run_id = $1) + \
       (SELECT COUNT(*) FROM factory_outbox_records WHERE run_id = $1), \
       (SELECT COUNT(*) FROM factory_run_controls WHERE run_id = $1)",
  )
  .bind(run_id.as_uuid())
  .fetch_one(&mut **transaction)
  .await
  .map_err(unavailable)?;
  Ok((
    usize::try_from(record_count).map_err(|_| StoreError::Unavailable)?,
    usize::try_from(control_count).map_err(|_| StoreError::Unavailable)?,
  ))
}

fn validate_history(
  run_id: FactoryRunId,
  history: &octacity_server_store::FactoryRunHistoryAppend,
) -> Result<(), StoreError> {
  let stages = history
    .stage_attempts
    .iter()
    .map(|record| record.id())
    .collect::<HashSet<_>>();
  let calls = history
    .macro_calls
    .iter()
    .map(|record| record.id())
    .collect::<HashSet<_>>();
  let call_records = history
    .macro_calls
    .iter()
    .map(|record| (record.id(), record))
    .collect::<HashMap<_, _>>();
  let handoffs = history
    .stage_handoffs
    .iter()
    .map(|record| (record.stage_attempt_id(), record))
    .collect::<HashMap<_, _>>();
  let manifests = history
    .context_manifests
    .iter()
    .map(|record| (record.id(), record))
    .collect::<HashMap<_, _>>();
  let requests = history
    .signal_requests
    .iter()
    .map(|record| record.id())
    .collect::<HashSet<_>>();
  let builds = history
    .linked_builds
    .iter()
    .map(|record| (record.build_id, record))
    .collect::<HashMap<_, _>>();
  let candidates = history
    .candidates
    .iter()
    .map(|record| record.id())
    .collect::<HashSet<_>>();
  let evidence = history
    .evidence
    .iter()
    .map(|record| record.id())
    .collect::<HashSet<_>>();
  let plans = history
    .evaluation_plans
    .iter()
    .map(|record| record.id())
    .collect::<HashSet<_>>();
  let assessments = history
    .assessments
    .iter()
    .map(|record| (record.id(), record))
    .collect::<HashMap<_, _>>();
  let decisions = history
    .decisions
    .iter()
    .map(|record| record.id())
    .collect::<HashSet<_>>();
  let invalid = history.stage_attempts.iter().any(|record| record.run_id() != run_id)
    || history
      .stage_attempt_completions
      .iter()
      .any(|record| record.run_id() != run_id || !stages.contains(&record.stage_attempt_id()))
    || history.stage_handoffs.iter().any(|record| {
      history
        .stage_attempts
        .iter()
        .find(|stage| stage.id() == record.stage_attempt_id())
        .is_none_or(|stage| record.subject().exact() != stage.subject())
    })
    || history.macro_calls.iter().any(|record| {
      record.run_id() != run_id
        || !stages.contains(&record.stage_attempt_id())
        || record.parent_id().is_some_and(|id| !calls.contains(&id))
        || record.stage_dependencies().iter().any(|id| !handoffs.contains_key(id))
        || record.call_dependencies().iter().any(|id| !calls.contains(id))
        || record.depth()
          != record
            .call_dependencies()
            .iter()
            .filter_map(|id| history.macro_calls.iter().find(|call| call.id() == *id))
            .map(MacroCall::depth)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
        || manifests.get(&record.context_manifest_id()).is_none_or(|manifest| {
          manifest.subject() != record.subject() || manifest.digest().ok() != Some(record.context_digest())
        })
    })
    || history.macro_call_completions.iter().any(|completion| {
      call_records.get(&completion.call_id()).is_none_or(|call| {
        completion.subject() != call.subject()
          || !history
            .stage_attempts
            .iter()
            .find(|stage| stage.id() == call.stage_attempt_id())
            .is_some_and(|stage| completion.task_envelope_digest() == stage.input_digest())
          || !completion.usage_is_valid_for(call.budget())
      })
    })
    || history
      .signal_requests
      .iter()
      .any(|record| record.run_id() != run_id || !stages.contains(&record.stage_attempt_id()))
    || history
      .signal_receipts
      .iter()
      .any(|record| record.run_id() != run_id || !requests.contains(&record.request_id()))
    || history
      .linked_builds
      .iter()
      .any(|record| record.run_id != run_id || !stages.contains(&record.stage_attempt_id))
    || history.build_observations.iter().any(|record| {
      record.run_id != run_id
        || builds
          .get(&record.build_id)
          .is_none_or(|link| link.stage_attempt_id != record.stage_attempt_id || link.target != record.target)
    })
    || history
      .candidates
      .iter()
      .any(|record| !stages.contains(&record.stage_attempt_id()))
    || history
      .evidence
      .iter()
      .any(|record| !candidates.contains(&record.changeset_id()))
    || history
      .evaluation_plans
      .iter()
      .any(|record| !evidence.contains(&record.evidence_id()))
    || history
      .assessments
      .iter()
      .any(|record| !plans.contains(&record.plan_id()))
    || history.decisions.iter().any(|record| {
      !plans.contains(&record.plan_id())
        || record.assessment_ids().iter().any(|id| {
          assessments
            .get(id)
            .is_none_or(|assessment| assessment.plan_id() != record.plan_id())
        })
    })
    || history
      .escalations
      .iter()
      .any(|record| record.run_id() != run_id || record.decision_id().is_some_and(|id| !decisions.contains(&id)))
    || history
      .delivery_attempts
      .iter()
      .any(|record| !decisions.contains(&record.decision_id()))
    || history
      .reporting_attempts
      .iter()
      .any(|record| record.run_id() != run_id);
  if invalid {
    return Err(StoreError::Unavailable);
  }
  Ok(())
}

async fn outbox_rows(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  run_id: FactoryRunId,
) -> Result<Vec<octacity_server_store::FactoryOutboxRecord>, StoreError> {
  sqlx::query_as::<_, OutboxRow>(
    "SELECT id, operation_id, run_id, kind, input_digest, state, attempt, \
            FLOOR(EXTRACT(EPOCH FROM available_at) * 1000)::BIGINT AS available_at_millis, \
            FLOOR(EXTRACT(EPOCH FROM recorded_at) * 1000)::BIGINT AS recorded_at_millis, \
            owner, fence, FLOOR(EXTRACT(EPOCH FROM claimed_at) * 1000)::BIGINT AS claimed_at_millis, \
            FLOOR(EXTRACT(EPOCH FROM claim_expires_at) * 1000)::BIGINT AS claim_expires_at_millis \
     FROM factory_outbox_records WHERE run_id = $1 ORDER BY id",
  )
  .bind(run_id.as_uuid())
  .fetch_all(&mut **transaction)
  .await
  .map_err(unavailable)?
  .into_iter()
  .map(decode_outbox)
  .collect()
}
