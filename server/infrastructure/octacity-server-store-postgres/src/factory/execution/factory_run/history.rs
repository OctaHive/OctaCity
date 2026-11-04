use std::collections::BTreeMap;

use octacity_server_domain::Timestamp;
use octacity_server_factory::{
  AdmittedFlow, FactoryDigest, FactoryRunId, FlowRun, NodeAttempt, NodeAttemptCompletion, NodeAttemptId, WorkflowCycle,
};
use octacity_server_store::{FactoryFlowHistory, FactoryRunHistoryAppend, StoreError};
use sqlx::{Postgres, Transaction, types::Json};

use crate::database::unavailable;

pub(crate) async fn append_history(
  transaction: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
  recorded_at: Timestamp,
  append: &FactoryRunHistoryAppend,
) -> Result<(), StoreError> {
  if append.flow.runs.iter().any(|record| record.factory_run_id() != run_id)
    || append
      .flow
      .attempts
      .iter()
      .any(|record| record.factory_run_id() != run_id)
  {
    return Err(StoreError::InvalidInput {
      operation: octacity_server_store::StoreOperation::CommitFactoryRunTransition,
      source: octacity_server_store::StoreInputError::InvalidFactoryRunTransition,
    });
  }

  insert_flow_history(transaction, run_id, recorded_at, &append.flow).await?;
  super::data::append(transaction, run_id, &append.flow.data, recorded_at).await?;
  for record in &append.flow.completions {
    insert_node_completion(transaction, run_id, record).await?;
  }
  Ok(())
}

async fn insert_flow_run(
  tx: &mut Transaction<'_, Postgres>,
  recorded_at: Timestamp,
  record: &FlowRun,
) -> Result<(), StoreError> {
  let parent = record.parent().ok_or(StoreError::Unavailable)?;
  let definition = record.definition();
  sqlx::query(
    "INSERT INTO factory_flow_runs \
       (id, factory_run_id, flow_definition_id, flow_definition_version, \
        parent_flow_run_id, parent_node_attempt_id, created_at) \
     VALUES ($1, $2, $3, $4, $5, $6, to_timestamp($7::double precision / 1000.0))",
  )
  .bind(record.id().as_uuid())
  .bind(record.factory_run_id().as_uuid())
  .bind(definition.id().as_uuid())
  .bind(number(definition.version().get())?)
  .bind(parent.flow_run_id().as_uuid())
  .bind(parent.node_attempt_id().as_uuid())
  .bind(recorded_at.unix_millis())
  .execute(&mut **tx)
  .await
  .map_err(unavailable)?;
  Ok(())
}

async fn insert_workflow_cycle(
  tx: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
  recorded_at: Timestamp,
  record: &WorkflowCycle,
) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO factory_workflow_cycles \
       (id, factory_run_id, flow_run_id, cycle_number, predecessor_id, source_attempt_id, created_at) \
     VALUES ($1, $2, $3, $4, $5, $6, to_timestamp($7::double precision / 1000.0))",
  )
  .bind(record.id().as_uuid())
  .bind(run_id.as_uuid())
  .bind(record.flow_run_id().as_uuid())
  .bind(number(record.number().get())?)
  .bind(record.predecessor().map(|id| id.as_uuid()))
  .bind(record.source_attempt_id().map(|id| id.as_uuid()))
  .bind(recorded_at.unix_millis())
  .execute(&mut **tx)
  .await
  .map_err(unavailable)?;
  Ok(())
}

async fn insert_node_attempt(
  tx: &mut Transaction<'_, Postgres>,
  recorded_at: Timestamp,
  record: &NodeAttempt,
) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO factory_node_attempts \
     (id, run_id, flow_run_id, workflow_cycle_id, node_key, attempt_number, node_kind, input_digest, node_attempt, created_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, \
             to_timestamp($10::double precision / 1000.0))",
  )
  .bind(record.id().as_uuid())
  .bind(record.factory_run_id().as_uuid())
  .bind(record.flow_run_id().as_uuid())
  .bind(record.workflow_cycle_id().as_uuid())
  .bind(record.node_key().as_str())
  .bind(number(record.number().get())?)
  .bind(record.node_kind().as_str())
  .bind(record.input_digest().as_bytes().as_slice())
  .bind(Json(record))
  .bind(recorded_at.unix_millis())
  .execute(&mut **tx)
  .await
  .map_err(unavailable)?;
  Ok(())
}

async fn insert_node_completion(
  tx: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
  record: &NodeAttemptCompletion,
) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO factory_node_attempt_completions \
     (id, node_attempt_id, run_id, outcome, output_schema_digest, output_digest, completion, completed_at) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, to_timestamp($8::double precision / 1000.0))",
  )
  .bind(record.id().as_bytes().as_slice())
  .bind(record.node_attempt_id().as_uuid())
  .bind(run_id.as_uuid())
  .bind(record.outcome().as_str())
  .bind(record.output_schema().digest().as_bytes().as_slice())
  .bind(record.output_digest().as_bytes().as_slice())
  .bind(Json(record))
  .bind(record.observed_at().unix_millis())
  .execute(&mut **tx)
  .await
  .map_err(unavailable)?;
  Ok(())
}

pub(super) async fn insert_shared_artifact_reference(
  tx: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
  artifact_id: octacity_server_domain::ArtifactId,
  role: octacity_server_store::FactoryArtifactRole,
  digest: FactoryDigest,
  recorded_at: Timestamp,
) -> Result<(), StoreError> {
  let result = sqlx::query(
    "INSERT INTO factory_artifact_references \
       (run_id, artifact_id, role, expected_sha256, created_at) \
     VALUES ($1, $2, $3, $4, to_timestamp($5::double precision / 1000.0)) \
     ON CONFLICT (run_id, artifact_id, role) DO UPDATE SET artifact_id = EXCLUDED.artifact_id \
     WHERE factory_artifact_references.expected_sha256 = EXCLUDED.expected_sha256 \
       AND factory_artifact_references.released_at IS NULL",
  )
  .bind(run_id.as_uuid())
  .bind(artifact_id.as_uuid())
  .bind(role.as_str())
  .bind(digest.as_bytes().as_slice())
  .bind(recorded_at.unix_millis())
  .execute(&mut **tx)
  .await
  .map_err(unavailable)?;
  if result.rows_affected() == 1 {
    Ok(())
  } else {
    Err(StoreError::Conflict {
      entity: octacity_server_domain::EntityKind::Artifact,
    })
  }
}

pub(crate) async fn history_rows(
  tx: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
  admitted_flow: &AdmittedFlow,
  flow_runs: &[FlowRun],
  cycles: &[WorkflowCycle],
  work: &octacity_server_factory::WorkEnvelope,
) -> Result<FactoryRunHistoryAppend, StoreError> {
  let node_attempts = node_attempt_rows(tx, run_id).await?;
  let completions = json_rows(
    tx,
    "SELECT completion FROM factory_node_attempt_completions WHERE run_id = $1 ORDER BY completed_at, id",
    run_id,
  )
  .await?;
  let data = super::data::read(
    tx,
    work,
    admitted_flow,
    octacity_server_factory::FlowRuntimeHistory {
      stops: &[],
      flow_runs,
      cycles,
      attempts: &node_attempts,
      completions: &completions,
    },
  )
  .await?;
  Ok(FactoryRunHistoryAppend {
    flow: FactoryFlowHistory {
      runs: Vec::new(),
      cycles: Vec::new(),
      attempts: node_attempts,
      completions,
      data,
    },
  })
}

async fn node_attempt_rows(
  tx: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
) -> Result<Vec<NodeAttempt>, StoreError> {
  let rows = sqlx::query_as::<
    _,
    (
      uuid::Uuid,
      uuid::Uuid,
      uuid::Uuid,
      String,
      i64,
      String,
      Vec<u8>,
      Json<NodeAttempt>,
    ),
  >(
    "SELECT id, flow_run_id, workflow_cycle_id, node_key, attempt_number, node_kind, input_digest, node_attempt \
     FROM factory_node_attempts WHERE run_id = $1 ORDER BY created_at, id",
  )
  .bind(run_id.as_uuid())
  .fetch_all(&mut **tx)
  .await
  .map_err(unavailable)?;
  rows
    .into_iter()
    .map(
      |(id, flow_run_id, cycle_id, node_key, number, kind, input_digest, Json(node))| {
        let matches = node.id().as_uuid() == id
          && node.flow_run_id().as_uuid() == flow_run_id
          && node.workflow_cycle_id().as_uuid() == cycle_id
          && node.node_key().as_str() == node_key
          && i64::try_from(node.number().get()).ok() == Some(number)
          && node.node_kind().as_str() == kind
          && node.input_digest().as_bytes().as_slice() == input_digest;
        matches.then_some(node).ok_or(StoreError::Unavailable)
      },
    )
    .collect()
}

pub(super) async fn node_history(
  tx: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
) -> Result<
  (
    BTreeMap<NodeAttemptId, NodeAttempt>,
    BTreeMap<NodeAttemptId, NodeAttemptCompletion>,
  ),
  StoreError,
> {
  let attempts = json_rows(
    tx,
    "SELECT node_attempt FROM factory_node_attempts WHERE run_id = $1 ORDER BY created_at, id",
    run_id,
  )
  .await?
  .into_iter()
  .map(|record: NodeAttempt| (record.id(), record))
  .collect();
  let completions = json_rows(
    tx,
    "SELECT completion FROM factory_node_attempt_completions WHERE run_id = $1 ORDER BY completed_at, id",
    run_id,
  )
  .await?
  .into_iter()
  .map(|record: NodeAttemptCompletion| (record.node_attempt_id(), record))
  .collect();
  Ok((attempts, completions))
}

async fn json_rows<T: for<'de> serde::Deserialize<'de> + Send + Unpin + 'static>(
  tx: &mut Transaction<'_, Postgres>,
  query: &'static str,
  run_id: FactoryRunId,
) -> Result<Vec<T>, StoreError> {
  sqlx::query_scalar::<_, Json<T>>(query)
    .bind(run_id.as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(unavailable)
    .map(|rows| rows.into_iter().map(|row| row.0).collect())
}

fn number(value: u64) -> Result<i64, StoreError> {
  i64::try_from(value).map_err(|_| StoreError::Unavailable)
}

async fn insert_flow_history(
  transaction: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
  recorded_at: Timestamp,
  flow: &FactoryFlowHistory,
) -> Result<(), StoreError> {
  let mut runs = flow.runs.iter().collect::<Vec<_>>();
  let mut cycles = flow.cycles.iter().collect::<Vec<_>>();
  let mut attempts = flow.attempts.iter().collect::<Vec<_>>();
  while !runs.is_empty() || !cycles.is_empty() || !attempts.is_empty() {
    let before = runs.len() + cycles.len() + attempts.len();
    while let Some(index) = runs.iter().position(|run| {
      run.parent().is_none_or(|parent| {
        !runs.iter().any(|run| run.id() == parent.flow_run_id())
          && !attempts.iter().any(|attempt| attempt.id() == parent.node_attempt_id())
      })
    }) {
      let run = runs.remove(index);
      insert_flow_run(transaction, recorded_at, run).await?;
    }
    while let Some(index) = cycles.iter().position(|cycle| {
      !runs.iter().any(|run| run.id() == cycle.flow_run_id())
        && cycle
          .source_attempt_id()
          .is_none_or(|id| !attempts.iter().any(|row| row.id() == id))
        && cycle
          .predecessor()
          .is_none_or(|id| !cycles.iter().any(|cycle| cycle.id() == id))
    }) {
      let cycle = cycles.remove(index);
      insert_workflow_cycle(transaction, run_id, recorded_at, cycle).await?;
    }
    while let Some(index) = attempts.iter().position(|attempt| {
      !runs.iter().any(|run| run.id() == attempt.flow_run_id())
        && !cycles.iter().any(|cycle| cycle.id() == attempt.workflow_cycle_id())
    }) {
      let attempt = attempts.remove(index);
      insert_node_attempt(transaction, recorded_at, attempt).await?;
    }
    if before == runs.len() + cycles.len() + attempts.len() {
      return Err(StoreError::Unavailable);
    }
  }
  Ok(())
}
