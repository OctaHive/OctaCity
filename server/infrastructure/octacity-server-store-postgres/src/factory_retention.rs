use async_trait::async_trait;
use octacity_server_domain::{EntityKind, Timestamp};
use octacity_server_factory::FactoryRunId;
use octacity_server_store::{
  AdvanceFactoryRetentionWork, ClaimFactoryRetentionWork, FactoryRetentionPassOutcome, FactoryRetentionPhase,
  FactoryRetentionStore, FactoryRetentionWorkClaim, FailFactoryRetentionWork, MAX_RETENTION_FAILURE_CODE_BYTES,
  StoreError, StoreInputError, StoreOperation, WorkerOwner,
};
use sqlx::{FromRow, PgPool, Postgres, Transaction};

use crate::{database::unavailable, store::PostgresStore};

#[async_trait]
impl FactoryRetentionStore for PostgresStore {
  async fn claim_factory_retention_work(
    &self,
    request: ClaimFactoryRetentionWork,
  ) -> Result<Vec<FactoryRetentionWorkClaim>, StoreError> {
    claim(self.pool(), request).await
  }

  async fn advance_factory_retention_work(
    &self,
    request: AdvanceFactoryRetentionWork,
  ) -> Result<FactoryRetentionPassOutcome, StoreError> {
    advance(self.pool(), request).await
  }

  async fn fail_factory_retention_work(&self, request: FailFactoryRetentionWork) -> Result<(), StoreError> {
    fail(self.pool(), request).await
  }
}

#[derive(FromRow)]
struct ClaimRow {
  run_id: uuid::Uuid,
  phase: String,
  attempt_count: i64,
  claim_owner: String,
  claim_expires_at_millis: i64,
}

#[derive(FromRow)]
struct FailureReplayRow {
  phase: String,
  claim_owner: Option<String>,
  last_claim_owner: Option<String>,
  last_error_code: Option<String>,
  last_failure_at_millis: Option<i64>,
  last_retry_at_millis: Option<i64>,
  completed_at_millis: Option<i64>,
  available_at_millis: i64,
}

impl FailureReplayRow {
  fn matches(&self, request: &FailFactoryRetentionWork) -> bool {
    let expected_retry = request.retry_at.map(Timestamp::unix_millis);
    self.claim_owner.is_none()
      && self.last_claim_owner.as_deref() == Some(request.owner.as_str())
      && self.last_error_code.as_deref() == Some(request.error_code.as_str())
      && self.last_failure_at_millis == Some(request.failed_at.unix_millis())
      && self.last_retry_at_millis == expected_retry
      && ((expected_retry.is_some()
        && self.phase != "dead_letter"
        && self.completed_at_millis.is_none()
        && Some(self.available_at_millis) == expected_retry)
        || (expected_retry.is_none()
          && self.phase == "dead_letter"
          && self.completed_at_millis == Some(request.failed_at.unix_millis())))
  }
}

async fn claim(
  pool: &PgPool,
  request: ClaimFactoryRetentionWork,
) -> Result<Vec<FactoryRetentionWorkClaim>, StoreError> {
  request.validate()?;
  sqlx::query_as::<_, ClaimRow>(
    "WITH candidates AS (\
       SELECT work.run_id FROM factory_retention_work AS work \
       JOIN factory_runs AS run ON run.id = work.run_id \
       WHERE work.completed_at IS NULL AND work.phase <> 'dead_letter' \
         AND (run.visible OR work.phase IN ('hidden', 'metadata')) \
         AND run.state IN ('rejected', 'cancelled', 'completed') \
         AND work.available_at <= to_timestamp($1::double precision / 1000.0) \
         AND (work.claim_owner IS NULL \
              OR work.claim_expires_at <= to_timestamp($1::double precision / 1000.0)) \
         AND NOT EXISTS (\
           SELECT 1 FROM factory_build_links AS link \
           JOIN LATERAL (\
             SELECT released_at, expired_at, expires_at \
             FROM build_result_retention_holds WHERE build_id = link.build_id \
             ORDER BY version DESC LIMIT 1\
           ) AS hold ON true \
           WHERE link.run_id = work.run_id AND hold.released_at IS NULL AND hold.expired_at IS NULL \
             AND (hold.expires_at IS NULL OR hold.expires_at > to_timestamp($1::double precision / 1000.0))\
         ) \
       ORDER BY work.available_at, work.run_id FOR UPDATE OF work SKIP LOCKED LIMIT $2\
     ), claimed AS (\
       UPDATE factory_retention_work AS work SET claim_owner = $3, \
         claim_expires_at = to_timestamp($4::double precision / 1000.0), \
         attempt_count = attempt_count + 1 \
       FROM candidates WHERE work.run_id = candidates.run_id \
       RETURNING work.run_id, work.phase, work.attempt_count, work.claim_owner, work.claim_expires_at\
     ) SELECT run_id, phase, attempt_count, claim_owner, \
       FLOOR(EXTRACT(EPOCH FROM claim_expires_at) * 1000)::BIGINT AS claim_expires_at_millis \
     FROM claimed ORDER BY run_id",
  )
  .bind(request.observed_at.unix_millis())
  .bind(i64::from(request.limit.get()))
  .bind(request.owner.as_str())
  .bind(request.claim_expires_at.unix_millis())
  .fetch_all(pool)
  .await
  .map_err(unavailable)?
  .into_iter()
  .map(ClaimRow::into_claim)
  .collect()
}

impl ClaimRow {
  fn into_claim(self) -> Result<FactoryRetentionWorkClaim, StoreError> {
    Ok(FactoryRetentionWorkClaim {
      run_id: FactoryRunId::from_uuid(self.run_id).map_err(|_| StoreError::Unavailable)?,
      phase: phase(&self.phase)?,
      attempt: u16::try_from(self.attempt_count).map_err(|_| StoreError::Unavailable)?,
      owner: WorkerOwner::new(self.claim_owner)?,
      claim_expires_at: timestamp(self.claim_expires_at_millis)?,
    })
  }
}

async fn advance(
  pool: &PgPool,
  request: AdvanceFactoryRetentionWork,
) -> Result<FactoryRetentionPassOutcome, StoreError> {
  request.validate()?;
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  sqlx::query_scalar::<_, uuid::Uuid>(
    "SELECT build.id FROM builds AS build \
     JOIN factory_build_links AS link ON link.build_id = build.id \
     WHERE link.run_id = $1 ORDER BY build.id FOR UPDATE OF build",
  )
  .bind(request.run_id.as_uuid())
  .fetch_all(&mut *transaction)
  .await
  .map_err(unavailable)?;
  lock_eligible_work(&mut transaction, &request).await?;

  sqlx::query(
    "UPDATE factory_runs SET visible = false, \
       hidden_at = COALESCE(hidden_at, to_timestamp($1::double precision / 1000.0)) \
     WHERE id = $2 AND visible",
  )
  .bind(request.observed_at.unix_millis())
  .bind(request.run_id.as_uuid())
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?;
  sqlx::query("UPDATE factory_retention_work SET phase = 'hidden' WHERE run_id = $1 AND phase = 'pending'")
    .bind(request.run_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;

  let released: i64 = sqlx::query_scalar(
    "WITH selected AS (\
       SELECT run_id, artifact_id, role FROM factory_artifact_references \
       WHERE run_id = $1 AND released_at IS NULL ORDER BY artifact_id, role \
       LIMIT $2 FOR UPDATE SKIP LOCKED\
     ), released AS (\
       UPDATE factory_artifact_references AS reference \
       SET released_at = to_timestamp($3::double precision / 1000.0) \
       FROM selected WHERE reference.run_id = selected.run_id \
         AND reference.artifact_id = selected.artifact_id AND reference.role = selected.role \
       RETURNING 1\
     ) SELECT COUNT(*) FROM released",
  )
  .bind(request.run_id.as_uuid())
  .bind(i64::from(request.page_limit.get()))
  .bind(request.observed_at.unix_millis())
  .fetch_one(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let remaining: bool = sqlx::query_scalar(
    "SELECT EXISTS(SELECT 1 FROM factory_artifact_references WHERE run_id = $1 AND released_at IS NULL)",
  )
  .bind(request.run_id.as_uuid())
  .fetch_one(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let mut deleted_records = 0_u16;
  if remaining {
    sqlx::query(
      "UPDATE factory_retention_work SET available_at = to_timestamp($1::double precision / 1000.0), \
       claim_owner = NULL, claim_expires_at = NULL WHERE run_id = $2",
    )
    .bind(request.observed_at.unix_millis())
    .bind(request.run_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
  } else {
    sqlx::query("UPDATE factory_retention_work SET phase = 'metadata' WHERE run_id = $1 AND phase = 'hidden'")
      .bind(request.run_id.as_uuid())
      .execute(&mut *transaction)
      .await
      .map_err(unavailable)?;
    deleted_records = delete_metadata_page(&mut transaction, request.run_id, request.page_limit.get()).await?;
    if deleted_records == 0 {
      sqlx::query(
        "UPDATE factory_retention_work SET phase = 'completed', \
         completed_at = to_timestamp($1::double precision / 1000.0), \
         claim_owner = NULL, claim_expires_at = NULL WHERE run_id = $2",
      )
      .bind(request.observed_at.unix_millis())
      .bind(request.run_id.as_uuid())
      .execute(&mut *transaction)
      .await
      .map_err(unavailable)?;
    } else {
      sqlx::query(
        "UPDATE factory_retention_work SET available_at = to_timestamp($1::double precision / 1000.0), \
         claim_owner = NULL, claim_expires_at = NULL WHERE run_id = $2",
      )
      .bind(request.observed_at.unix_millis())
      .bind(request.run_id.as_uuid())
      .execute(&mut *transaction)
      .await
      .map_err(unavailable)?;
    }
  }
  transaction.commit().await.map_err(unavailable)?;
  Ok(FactoryRetentionPassOutcome {
    released_references: u16::try_from(released).map_err(|_| StoreError::Unavailable)?,
    deleted_records,
    completed: !remaining && deleted_records == 0,
  })
}

async fn delete_metadata_page(
  transaction: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
  limit: u16,
) -> Result<u16, StoreError> {
  const DELETE_STEPS: &[&str] = &[
    "WITH selected AS (SELECT ctid FROM factory_triage_records WHERE run_id = $1 ORDER BY ctid LIMIT $2) DELETE FROM factory_triage_records WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_phase_pool_selections WHERE run_id = $1 ORDER BY ctid LIMIT $2) DELETE FROM factory_phase_pool_selections WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_phase_pool_entries WHERE run_id = $1 ORDER BY ctid LIMIT $2) DELETE FROM factory_phase_pool_entries WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_run_current WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_run_current WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT relation.ctid FROM factory_decision_assessments AS relation \
       JOIN factory_decisions AS parent ON parent.id = relation.decision_id \
       WHERE parent.run_id = $1 ORDER BY relation.ctid LIMIT $2) \
     DELETE FROM factory_decision_assessments AS relation USING selected WHERE relation.ctid = selected.ctid",
    "WITH selected AS (SELECT relation.ctid FROM factory_build_link_jobs AS relation \
       JOIN factory_build_links AS parent ON parent.build_id = relation.build_id \
       WHERE parent.run_id = $1 ORDER BY relation.ctid LIMIT $2) \
     DELETE FROM factory_build_link_jobs AS relation USING selected WHERE relation.ctid = selected.ctid",
    "WITH selected AS (SELECT ctid FROM factory_stage_attempt_completions WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_stage_attempt_completions WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_decision_signal_receipts WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_decision_signal_receipts WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_build_observations WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_build_observations WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_delivery_attempts WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_delivery_attempts WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_escalations WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_escalations WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_decisions WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_decisions WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_assessments WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_assessments WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_evaluation_plans WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_evaluation_plans WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_evidence_manifests WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_evidence_manifests WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_changesets WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_changesets WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_decision_signal_requests WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_decision_signal_requests WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_call_completions WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_call_completions WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT relation.ctid FROM factory_call_dependencies AS relation \
       JOIN factory_call_nodes AS parent ON parent.id = relation.call_id \
       WHERE parent.run_id = $1 ORDER BY relation.ctid LIMIT $2) \
     DELETE FROM factory_call_dependencies AS relation USING selected WHERE relation.ctid = selected.ctid",
    "WITH selected AS (SELECT relation.ctid FROM factory_call_stage_dependencies AS relation \
       JOIN factory_call_nodes AS parent ON parent.id = relation.call_id \
       WHERE parent.run_id = $1 ORDER BY relation.ctid LIMIT $2) \
     DELETE FROM factory_call_stage_dependencies AS relation USING selected WHERE relation.ctid = selected.ctid",
    "WITH selected AS (SELECT node.ctid FROM factory_call_nodes AS node \
       WHERE node.run_id = $1 AND NOT EXISTS (SELECT 1 FROM factory_call_nodes AS child WHERE child.parent_call_id = node.id) \
       ORDER BY node.ctid LIMIT $2) DELETE FROM factory_call_nodes AS node USING selected WHERE node.ctid = selected.ctid",
    "WITH selected AS (SELECT ctid FROM factory_context_manifests WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_context_manifests WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_stage_handoffs WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_stage_handoffs WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_reporting_attempts WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_reporting_attempts WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_outbox_records WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_outbox_records WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_audit_links WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_audit_links WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_run_controls WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_run_controls WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_artifact_references WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_artifact_references WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_build_links WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_build_links WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_stage_attempts WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_stage_attempts WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_node_attempt_completions WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_node_attempt_completions WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT node.ctid FROM factory_node_attempts AS node \
       WHERE node.run_id = $1 \
         AND NOT EXISTS (SELECT 1 FROM factory_stage_attempts AS stage WHERE stage.id = node.id) \
         AND NOT EXISTS (SELECT 1 FROM factory_flow_runs AS child WHERE child.parent_node_attempt_id = node.id) \
       ORDER BY node.ctid LIMIT $2) \
     DELETE FROM factory_node_attempts AS node USING selected WHERE node.ctid = selected.ctid",
    "WITH selected AS (SELECT cycle.ctid FROM factory_workflow_cycles AS cycle \
       WHERE cycle.factory_run_id = $1 \
         AND NOT EXISTS (SELECT 1 FROM factory_node_attempts AS node WHERE node.workflow_cycle_id = cycle.id) \
       ORDER BY cycle.ctid LIMIT $2) \
     DELETE FROM factory_workflow_cycles AS cycle USING selected WHERE cycle.ctid = selected.ctid",
    "WITH selected AS (SELECT flow.ctid FROM factory_flow_runs AS flow \
       WHERE flow.factory_run_id = $1 \
         AND NOT EXISTS (SELECT 1 FROM factory_node_attempts AS node WHERE node.flow_run_id = flow.id) \
         AND NOT EXISTS (SELECT 1 FROM factory_workflow_cycles AS cycle WHERE cycle.flow_run_id = flow.id) \
         AND NOT EXISTS (SELECT 1 FROM factory_flow_runs AS child WHERE child.parent_flow_run_id = flow.id) \
       ORDER BY flow.ctid LIMIT $2) \
     DELETE FROM factory_flow_runs AS flow USING selected WHERE flow.ctid = selected.ctid",
    "WITH selected AS (SELECT ctid FROM factory_run_budgets WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_run_budgets WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_lifecycle_checkpoints WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_lifecycle_checkpoints WHERE ctid IN (SELECT ctid FROM selected)",
    "WITH selected AS (SELECT ctid FROM factory_run_claims WHERE run_id = $1 ORDER BY ctid LIMIT $2) \
     DELETE FROM factory_run_claims WHERE ctid IN (SELECT ctid FROM selected)",
  ];
  for statement in DELETE_STEPS {
    let deleted = sqlx::query(*statement)
      .bind(run_id.as_uuid())
      .bind(i64::from(limit))
      .execute(&mut **transaction)
      .await
      .map_err(unavailable)?
      .rows_affected();
    if deleted != 0 {
      return u16::try_from(deleted).map_err(|_| StoreError::Unavailable);
    }
  }
  Ok(0)
}

async fn lock_eligible_work(
  transaction: &mut Transaction<'_, Postgres>,
  request: &AdvanceFactoryRetentionWork,
) -> Result<(), StoreError> {
  let eligible: Option<bool> = sqlx::query_scalar(
    "SELECT run.state IN ('rejected', 'cancelled', 'completed') AND NOT EXISTS (\
       SELECT 1 FROM factory_build_links AS link \
       JOIN LATERAL (\
         SELECT released_at, expired_at, expires_at FROM build_result_retention_holds \
         WHERE build_id = link.build_id ORDER BY version DESC LIMIT 1\
       ) AS hold ON true \
       WHERE link.run_id = work.run_id AND hold.released_at IS NULL AND hold.expired_at IS NULL \
         AND (hold.expires_at IS NULL OR hold.expires_at > to_timestamp($3::double precision / 1000.0))\
     ) FROM factory_retention_work AS work \
     JOIN factory_runs AS run ON run.id = work.run_id \
     WHERE work.run_id = $1 AND work.claim_owner = $2 AND work.completed_at IS NULL \
       AND work.claim_expires_at > to_timestamp($3::double precision / 1000.0) \
     FOR UPDATE OF work, run",
  )
  .bind(request.run_id.as_uuid())
  .bind(request.owner.as_str())
  .bind(request.observed_at.unix_millis())
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)?;
  match eligible {
    Some(true) => Ok(()),
    Some(false) => Err(conflict()),
    None => Err(StoreError::NotFound {
      entity: EntityKind::FactoryRun,
    }),
  }
}

async fn fail(pool: &PgPool, request: FailFactoryRetentionWork) -> Result<(), StoreError> {
  if request.error_code.is_empty()
    || request.error_code.len() > MAX_RETENTION_FAILURE_CODE_BYTES
    || request.error_code.chars().any(char::is_whitespace)
    || request.retry_at.is_some_and(|retry_at| retry_at <= request.failed_at)
  {
    return Err(StoreError::InvalidInput {
      operation: StoreOperation::FailFactoryRetentionWork,
      source: StoreInputError::InvalidWorkerClaim,
    });
  }
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let updated = sqlx::query(
    "UPDATE factory_retention_work SET \
       available_at = COALESCE(to_timestamp($1::double precision / 1000.0), available_at), \
       phase = CASE WHEN $1::BIGINT IS NULL THEN 'dead_letter' ELSE phase END, \
       completed_at = CASE WHEN $1::BIGINT IS NULL THEN to_timestamp($2::double precision / 1000.0) ELSE NULL END, \
       last_error_code = $3, last_failure_at = to_timestamp($2::double precision / 1000.0), \
       last_claim_owner = claim_owner, last_retry_at = to_timestamp($1::double precision / 1000.0), \
       claim_owner = NULL, claim_expires_at = NULL \
     WHERE run_id = $4 AND claim_owner = $5 \
       AND claim_expires_at > to_timestamp($2::double precision / 1000.0)",
  )
  .bind(request.retry_at.map(Timestamp::unix_millis))
  .bind(request.failed_at.unix_millis())
  .bind(&request.error_code)
  .bind(request.run_id.as_uuid())
  .bind(request.owner.as_str())
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?;
  if updated.rows_affected() == 1 {
    transaction.commit().await.map_err(unavailable)?;
    Ok(())
  } else {
    let replay = sqlx::query_as::<_, FailureReplayRow>(
      "SELECT phase, claim_owner, last_claim_owner, last_error_code, \
           FLOOR(EXTRACT(EPOCH FROM last_failure_at) * 1000)::BIGINT AS last_failure_at_millis, \
           FLOOR(EXTRACT(EPOCH FROM last_retry_at) * 1000)::BIGINT AS last_retry_at_millis, \
           FLOOR(EXTRACT(EPOCH FROM completed_at) * 1000)::BIGINT AS completed_at_millis, \
           FLOOR(EXTRACT(EPOCH FROM available_at) * 1000)::BIGINT AS available_at_millis \
         FROM factory_retention_work WHERE run_id = $1 FOR UPDATE",
    )
    .bind(request.run_id.as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(unavailable)?;
    let is_replay = replay.as_ref().is_some_and(|row| row.matches(&request));
    if is_replay {
      transaction.commit().await.map_err(unavailable)?;
      Ok(())
    } else {
      Err(conflict())
    }
  }
}

fn phase(value: &str) -> Result<FactoryRetentionPhase, StoreError> {
  match value {
    "pending" => Ok(FactoryRetentionPhase::Pending),
    "hidden" => Ok(FactoryRetentionPhase::Hidden),
    "metadata" => Ok(FactoryRetentionPhase::Metadata),
    "completed" => Ok(FactoryRetentionPhase::Completed),
    "dead_letter" => Ok(FactoryRetentionPhase::DeadLetter),
    _ => Err(StoreError::Unavailable),
  }
}

fn timestamp(value: i64) -> Result<Timestamp, StoreError> {
  Timestamp::from_unix_millis(value).map_err(|_| StoreError::Unavailable)
}

const fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::FactoryRun,
  }
}
