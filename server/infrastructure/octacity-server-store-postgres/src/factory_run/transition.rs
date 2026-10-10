use octacity_server_store::{
  CommitFactoryRunTransition, CommitFactoryRunTransitionOutcome, FactoryBudgetRecord, FactoryLifecycleCheckpoint,
  FactoryTransitionBaseline, StoreError, validate_factory_transition,
};
use sqlx::{PgPool, types::Json};

use super::{
  LifecycleDocument, append_history, conflict, insert_audit, insert_outbox, lock_run, read_claim, version_number,
};
use crate::database::unavailable;

pub(crate) async fn commit_transition(
  pool: &PgPool,
  request: CommitFactoryRunTransition,
) -> Result<CommitFactoryRunTransitionOutcome, StoreError> {
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let locked = lock_run(&mut transaction, request.run_id).await?;
  validate_transition(&mut transaction, &locked, &request).await?;

  insert_budget(&mut transaction, request.run_id, &request.budget).await?;
  insert_lifecycle(&mut transaction, &request.lifecycle_checkpoint).await?;
  append_history(&mut transaction, request.run_id, request.committed_at, &request.append).await?;
  insert_audit(&mut transaction, &request.audit).await?;
  for record in &request.outbox {
    insert_outbox(&mut transaction, record).await?;
  }
  update_projection(&mut transaction, &request).await?;
  update_run(&mut transaction, &request).await?;
  schedule_retention(&mut transaction, &request).await?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(CommitFactoryRunTransitionOutcome {
    version: request.next_run.version(),
    current: request.current,
  })
}

async fn validate_transition(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  locked: &super::LockedRun,
  request: &CommitFactoryRunTransition,
) -> Result<(), StoreError> {
  let claim_id = locked
    .claim_id
    .filter(|id| *id == request.claim_id)
    .ok_or_else(conflict)?;
  let claim = read_claim(transaction, claim_id)
    .await?
    .ok_or(StoreError::Unavailable)?;
  let (record_count, _) = super::snapshot::snapshot_record_counts(transaction, request.run_id).await?;
  let (stage_attempts, stage_completions) = super::history::stage_history(transaction, request.run_id).await?;
  let (node_attempts, node_completions) = super::history::node_history(transaction, request.run_id).await?;
  let admitted_flow = super::snapshot::read_admitted_flow(transaction, locked).await?;
  let flow_run_records = super::snapshot::read_flow_runs(transaction, request.run_id, &admitted_flow).await?;
  let workflow_cycles: std::collections::BTreeMap<_, _> =
    super::snapshot::read_workflow_cycles(transaction, request.run_id, &flow_run_records)
      .await?
      .into_iter()
      .map(|record| (record.id(), record))
      .collect();
  let flow_runs: std::collections::BTreeMap<_, _> = flow_run_records
    .into_iter()
    .map(|record| (record.id(), record))
    .collect();
  let (stage_handoffs, context_manifests, macro_calls, macro_call_completions) =
    super::history::call_context_history(transaction, request.run_id).await?;
  validate_factory_transition(
    &FactoryTransitionBaseline {
      run: &locked.run,
      pool_selection: super::selection_for_claim(transaction, request.run_id, claim.id)
        .await?
        .as_ref(),
      budget: &locked.budget,
      lifecycle: &locked.lifecycle,
      current: &locked.current,
      claim: &claim,
      admitted_flow: &admitted_flow,
      work: &locked.work,
      data: &super::data::read(
        transaction,
        &locked.work,
        &admitted_flow,
        octacity_server_factory::FlowRuntimeHistory {
          flow_runs: &flow_runs.values().cloned().collect::<Vec<_>>(),
          cycles: &workflow_cycles.values().cloned().collect::<Vec<_>>(),
          attempts: &node_attempts.values().cloned().collect::<Vec<_>>(),
          completions: &node_completions.values().cloned().collect::<Vec<_>>(),
        },
      )
      .await?,
      stage_attempts: &stage_attempts,
      node_attempts: &node_attempts,
      node_completions: &node_completions,
      flow_runs: &flow_runs,
      workflow_cycles: &workflow_cycles,
      stage_completions: &stage_completions,
      stage_handoffs: &stage_handoffs,
      context_manifests: &context_manifests,
      macro_calls: &macro_calls,
      macro_call_completions: &macro_call_completions,
      record_count,
    },
    request,
  )
}

async fn insert_budget(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  run_id: octacity_server_factory::FactoryRunId,
  budget: &FactoryBudgetRecord,
) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO factory_run_budgets (id, run_id, run_version, usage, recorded_at) \
     VALUES ($1, $2, $3, $4, to_timestamp($5::double precision / 1000.0))",
  )
  .bind(budget.id.as_bytes().as_slice())
  .bind(run_id.as_uuid())
  .bind(version_number(budget.run_version)?)
  .bind(Json(budget.usage))
  .bind(budget.recorded_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  Ok(())
}

async fn insert_lifecycle(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  lifecycle: &FactoryLifecycleCheckpoint,
) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO factory_lifecycle_checkpoints \
       (id, run_id, run_version, lifecycle, cancellation_requested, recorded_at) \
     VALUES ($1, $2, $3, $4, $5, to_timestamp($6::double precision / 1000.0))",
  )
  .bind(lifecycle.id.as_bytes().as_slice())
  .bind(lifecycle.run_id.as_uuid())
  .bind(version_number(lifecycle.run_version)?)
  .bind(Json(LifecycleDocument {
    progress: lifecycle.progress.clone(),
    signal: lifecycle.signal,
  }))
  .bind(lifecycle.cancellation_requested)
  .bind(lifecycle.recorded_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  Ok(())
}

async fn update_projection(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  request: &CommitFactoryRunTransition,
) -> Result<(), StoreError> {
  let updated = sqlx::query(
    "UPDATE factory_run_current SET run_version = $1, budget_id = $2, lifecycle_checkpoint_id = $3, \
       stage_attempt_id = $4, call_node_id = $5, signal_request_id = $6, signal_receipt_id = $7, \
       build_id = $8, changeset_id = $9, evidence_manifest_id = $10, evaluation_plan_id = $11, \
       decision_id = $12, escalation_id = $13, delivery_attempt_id = $14, reporting_attempt_id = $15 \
     WHERE run_id = $16 AND run_version = $17 AND claim_id = $18",
  )
  .bind(version_number(request.next_run.version())?)
  .bind(request.current.budget_id.as_bytes().as_slice())
  .bind(request.current.lifecycle_checkpoint_id.as_bytes().as_slice())
  .bind(request.current.stage_attempt_id.map(|id| id.as_uuid()))
  .bind(request.current.macro_call_id.map(|id| id.as_uuid()))
  .bind(request.current.signal_request_id.map(|id| id.as_uuid()))
  .bind(request.current.signal_receipt_id.map(|id| id.as_uuid()))
  .bind(request.current.build_id.map(|id| id.as_uuid()))
  .bind(request.current.candidate_id.map(|id| id.as_uuid()))
  .bind(request.current.evidence_id.map(|id| id.as_uuid()))
  .bind(request.current.evaluation_plan_id.map(|id| id.as_uuid()))
  .bind(request.current.decision_id.map(|id| id.as_uuid()))
  .bind(request.current.escalation_id.map(|id| id.as_uuid()))
  .bind(request.current.delivery_attempt_id.map(|id| id.as_uuid()))
  .bind(request.current.reporting_attempt_id.map(|id| id.as_uuid()))
  .bind(request.run_id.as_uuid())
  .bind(version_number(request.expected_version)?)
  .bind(request.claim_id.as_bytes().as_slice())
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  if updated.rows_affected() != 1 {
    return Err(conflict());
  }
  Ok(())
}

async fn update_run(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  request: &CommitFactoryRunTransition,
) -> Result<(), StoreError> {
  let updated = sqlx::query(
    "UPDATE factory_runs SET state = $1, version = $2, \
       updated_at = to_timestamp($3::double precision / 1000.0) \
     WHERE id = $4 AND version = $5",
  )
  .bind(request.next_run.state().as_str())
  .bind(version_number(request.next_run.version())?)
  .bind(request.committed_at.unix_millis())
  .bind(request.run_id.as_uuid())
  .bind(version_number(request.expected_version)?)
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  if updated.rows_affected() != 1 {
    return Err(conflict());
  }
  Ok(())
}

async fn schedule_retention(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  request: &CommitFactoryRunTransition,
) -> Result<(), StoreError> {
  if request.next_run.state().is_active() {
    return Ok(());
  }
  sqlx::query(
    "INSERT INTO factory_retention_work (run_id, available_at) \
     SELECT $1, GREATEST(\
       to_timestamp($2::double precision / 1000.0), \
       COALESCE((\
         SELECT MAX(work.deadline_at) FROM factory_build_links AS link \
         JOIN retention_work AS work ON work.build_id = link.build_id WHERE link.run_id = $1\
       ), to_timestamp($2::double precision / 1000.0))\
     ) ON CONFLICT (run_id) DO NOTHING",
  )
  .bind(request.run_id.as_uuid())
  .bind(request.committed_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  Ok(())
}
