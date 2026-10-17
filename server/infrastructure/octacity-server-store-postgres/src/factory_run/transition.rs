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
  // Serialize configuration capacity before locking any Run. Every node append
  // observes the committed usage of earlier owners of this exact configuration.
  let capacity: Option<i32> = sqlx::query_scalar(
    "SELECT 1 FROM factory_configuration_versions AS configuration \
     JOIN factory_runs AS run ON run.factory_configuration_id = configuration.factory_configuration_id \
       AND run.factory_configuration_version = configuration.version \
     WHERE run.id = $1 FOR UPDATE OF configuration",
  )
  .bind(request.run_id.as_uuid())
  .fetch_optional(&mut *transaction)
  .await
  .map_err(unavailable)?;
  if capacity.is_none() {
    return Err(StoreError::NotFound {
      entity: octacity_server_domain::EntityKind::FactoryRun,
    });
  }
  let projection_scope = if request
    .outbox
    .iter()
    .any(|record| record.kind.as_str() == "work.status.projection")
  {
    crate::factory_source_projection::lock_for_run(&mut transaction, request.run_id).await?
  } else {
    None
  };
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
  crate::factory_source_projection::record_publications(
    &mut transaction,
    projection_scope.as_ref(),
    request.run_id,
    &request.outbox,
  )
  .await?;
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
  validate_factory_transition(
    &FactoryTransitionBaseline {
      configuration_wip_usage: super::claim::wip_usage(transaction, locked).await?,
      run: &locked.run,
      pool_selection: super::selection_for_claim(transaction, request.run_id, claim.id)
        .await?
        .as_ref(),
      budget: &locked.budget,
      cancellation_requested_at: sqlx::query_scalar::<_, Option<i64>>("SELECT FLOOR(EXTRACT(EPOCH FROM MIN(recorded_at)) * 1000)::BIGINT FROM factory_run_controls WHERE run_id=$1 AND control_kind='cancel-factory-run'")
        .bind(request.run_id.as_uuid()).fetch_one(&mut **transaction).await.map_err(unavailable)?.map(octacity_server_domain::Timestamp::from_unix_millis).transpose().map_err(|_| StoreError::Unavailable)?,
      lifecycle: &locked.lifecycle,
      retry_requests: &super::control::read_retry_requests(transaction, &locked.run).await?,
      current: &locked.current,
      claim: &claim,
      admitted_flow: &admitted_flow,
      work: &locked.work,
      data: &super::data::read(
        transaction,
        &locked.work,
        &admitted_flow,
        octacity_server_factory::FlowRuntimeHistory {
          stops: &[],
          flow_runs: &flow_runs.values().cloned().collect::<Vec<_>>(),
          cycles: &workflow_cycles.values().cloned().collect::<Vec<_>>(),
          attempts: &node_attempts.values().cloned().collect::<Vec<_>>(),
          completions: &node_completions.values().cloned().collect::<Vec<_>>(),
        },
      )
      .await?,
      node_attempts: &node_attempts,
      node_completions: &node_completions,
      flow_runs: &flow_runs,
      workflow_cycles: &workflow_cycles,
      record_count,
    },
    request,
  )
}

pub(super) async fn insert_budget(
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

pub(super) async fn insert_lifecycle(
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
    "UPDATE factory_run_current SET run_version = $1, budget_id = $2, lifecycle_checkpoint_id = $3 \
     WHERE run_id = $4 AND run_version = $5 AND claim_id = $6",
  )
  .bind(version_number(request.next_run.version())?)
  .bind(request.current.budget_id.as_bytes().as_slice())
  .bind(request.current.lifecycle_checkpoint_id.as_bytes().as_slice())
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
         SELECT MAX(work.deadline_at) FROM factory_run_builds AS link \
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
