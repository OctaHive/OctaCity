//! Exact retained data for arbitrary configured nodes; bytes never restore authority by themselves.

use crate::database::unavailable;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use octacity_server_store::StoreError;
use serde::Serialize;
use sqlx::{Postgres, Transaction, types::Json};

pub(super) async fn append(
  tx: &mut Transaction<'_, Postgres>,
  run: FactoryRunId,
  data: &FlowDataHistory,
  at: Timestamp,
) -> Result<(), StoreError> {
  for incoming in &data.incoming {
    sqlx::query("INSERT INTO factory_flow_incoming (run_id,id,schema_reference,document) VALUES ($1,$2,$3,$4)")
      .bind(run.as_uuid())
      .bind(incoming.digest().map_err(invalid)?.as_bytes().as_slice())
      .bind(Json(incoming.payload().schema()))
      .bind(bytes(incoming)?)
      .execute(&mut **tx)
      .await
      .map_err(unavailable)?;
  }
  for input in &data.inputs {
    sqlx::query("INSERT INTO factory_flow_inputs (run_id,id,flow_run_id,cycle_id,generation,schema_reference,document) VALUES ($1,$2,$3,$4,$5,$6,$7)")
      .bind(run.as_uuid()).bind(input.digest().map_err(invalid)?.as_bytes().as_slice())
      .bind(input.flow_run_id().as_uuid()).bind(input.cycle_id().as_uuid()).bind(i64::from(input.generation()))
      .bind(Json(input.payload().schema())).bind(bytes(input)?).execute(&mut **tx).await.map_err(unavailable)?;
    if let Some(context) = input.context() {
      for entry in context.entries() {
        let artifact = match entry.source() {
          FactoryContextReference::Artifact(artifact) => Some(artifact),
          FactoryContextReference::RepositoryFragment(fragment) => Some(fragment.artifact()),
          FactoryContextReference::RepositoryRange(_) => None,
        };
        if let Some(artifact) = artifact {
          super::history::insert_shared_artifact_reference(
            tx,
            run,
            artifact.artifact_id(),
            octacity_server_store::FactoryArtifactRole::CallContext,
            artifact.content_digest(),
            at,
          )
          .await?;
        }
      }
    }
  }
  for intent in &data.build_intents {
    sqlx::query("INSERT INTO factory_flow_build_intents (run_id,node_attempt_id,operation_id,input_digest,document) VALUES ($1,$2,$3,$4,$5)")
      .bind(run.as_uuid()).bind(intent.node().id().as_uuid()).bind(intent.operation_id().map_err(invalid)?.as_bytes().as_slice())
      .bind(intent.input().digest().map_err(invalid)?.as_bytes().as_slice()).bind(bytes(intent)?).execute(&mut **tx).await.map_err(unavailable)?;
  }
  for execution in &data.build_executions {
    sqlx::query("INSERT INTO factory_flow_build_executions (run_id,id,node_attempt_id,operation_id,build_id,attempt_id,document) VALUES ($1,$2,$3,$4,$5,$6,$7)")
      .bind(run.as_uuid()).bind(execution.digest().map_err(invalid)?.as_bytes().as_slice()).bind(execution.node_attempt_id().as_uuid())
      .bind(execution.operation_id().as_bytes().as_slice()).bind(execution.build_id().as_uuid()).bind(execution.attempt_id().as_uuid()).bind(bytes(execution)?)
      .execute(&mut **tx).await.map_err(unavailable)?;
  }
  for record in &data.records {
    sqlx::query("INSERT INTO factory_flow_records (run_id,node_attempt_id,id,input_digest,schema_reference,document) VALUES ($1,$2,$3,$4,$5,$6)")
      .bind(run.as_uuid()).bind(record.node_attempt_id().as_uuid()).bind(record.digest().map_err(invalid)?.as_bytes().as_slice())
      .bind(record.input().digest().map_err(invalid)?.as_bytes().as_slice()).bind(Json(record.payload().schema())).bind(bytes(record)?)
      .execute(&mut **tx).await.map_err(unavailable)?;
    if let Some(provenance) = record.build_provenance() {
      for output in provenance.outputs() {
        super::history::insert_shared_artifact_reference(
          tx,
          run,
          output.artifact.artifact_id(),
          octacity_server_store::FactoryArtifactRole::BuildOutput,
          output.artifact.content_digest(),
          at,
        )
        .await?;
      }
    }
  }
  Ok(())
}
pub(super) async fn read(
  tx: &mut Transaction<'_, Postgres>,
  work: &WorkEnvelope,
  admitted: &AdmittedFlow,
  history: FlowRuntimeHistory<'_>,
) -> Result<FlowDataHistory, StoreError> {
  let run = admitted.root_run().factory_run_id();
  let mut data = FlowDataHistory::default();
  let incoming = sqlx::query_as::<_, (Vec<u8>, Json<ImmutableReference>, Vec<u8>)>(
    "SELECT id,schema_reference,document FROM factory_flow_incoming WHERE run_id=$1",
  )
  .bind(run.as_uuid())
  .fetch_all(&mut **tx)
  .await
  .map_err(unavailable)?;
  for (digest, Json(schema), document) in incoming {
    data.incoming.push(
      FlowIncomingData::restore(
        &document,
        work,
        admitted,
        admitted.data_schema(&schema).ok_or(StoreError::Unavailable)?,
        super::digest(&digest)?,
      )
      .map_err(invalid)?,
    );
  }
  let inputs = sqlx::query_as::<_, (Vec<u8>, uuid::Uuid, uuid::Uuid, i64, Json<ImmutableReference>, Vec<u8>)>(
    "SELECT id,flow_run_id,cycle_id,generation,schema_reference,document FROM factory_flow_inputs WHERE run_id=$1 ORDER BY id",
  )
  .bind(run.as_uuid())
  .fetch_all(&mut **tx)
  .await
  .map_err(unavailable)?;
  for (digest, flow, cycle, generation, Json(schema), document) in inputs {
    let flow = history
      .flow_runs
      .iter()
      .find(|row| row.id().as_uuid() == flow)
      .ok_or(StoreError::Unavailable)?;
    let cycle = history
      .cycles
      .iter()
      .find(|row| row.id().as_uuid() == cycle)
      .ok_or(StoreError::Unavailable)?;
    let input = FlowNodeInput::restore(
      &document,
      work,
      admitted,
      flow,
      cycle,
      admitted.data_schema(&schema).ok_or(StoreError::Unavailable)?,
      super::digest(&digest)?,
    )
    .map_err(invalid)?;
    if i64::from(input.generation()) != generation {
      return Err(StoreError::Unavailable);
    }
    data.inputs.push(input);
  }
  let intents = sqlx::query_as::<_, (uuid::Uuid, Vec<u8>, Vec<u8>, Vec<u8>)>("SELECT node_attempt_id,operation_id,input_digest,document FROM factory_flow_build_intents WHERE run_id=$1 ORDER BY node_attempt_id")
    .bind(run.as_uuid()).fetch_all(&mut **tx).await.map_err(unavailable)?;
  for (attempt, operation, input_digest, document) in intents {
    let attempt = history
      .attempts
      .iter()
      .find(|row| row.id().as_uuid() == attempt)
      .ok_or(StoreError::Unavailable)?;
    let input = data
      .inputs
      .iter()
      .find(|row| row.digest().ok() == Some(attempt.input_digest()))
      .ok_or(StoreError::Unavailable)?;
    if input.digest().map_err(invalid)? != super::digest(&input_digest)? {
      return Err(StoreError::Unavailable);
    }
    data.build_intents.push(
      FlowBuildIntent::restore(&document, input, admitted, attempt, super::digest(&operation)?).map_err(invalid)?,
    );
  }
  let executions=sqlx::query_as::<_,(Vec<u8>,uuid::Uuid,Vec<u8>,uuid::Uuid,uuid::Uuid,Vec<u8>)>("SELECT id,node_attempt_id,operation_id,build_id,attempt_id,document FROM factory_flow_build_executions WHERE run_id=$1 ORDER BY id")
    .bind(run.as_uuid()).fetch_all(&mut **tx).await.map_err(unavailable)?;
  for (id, node, operation, build, attempt, document) in executions {
    let intent = data
      .build_intents
      .iter()
      .find(|intent| intent.node().id().as_uuid() == node)
      .ok_or(StoreError::Unavailable)?;
    let execution = FlowBuildExecution::restore(&document, intent, super::digest(&id)?).map_err(invalid)?;
    if execution.operation_id() != super::digest(&operation)?
      || execution.build_id().as_uuid() != build
      || execution.attempt_id().as_uuid() != attempt
    {
      return Err(StoreError::Unavailable);
    }
    data.build_executions.push(execution);
  }
  let records = sqlx::query_as::<_,(uuid::Uuid,Vec<u8>,Vec<u8>,Json<ImmutableReference>,Vec<u8>)>("SELECT node_attempt_id,id,input_digest,schema_reference,document FROM factory_flow_records WHERE run_id=$1 ORDER BY node_attempt_id")
    .bind(run.as_uuid()).fetch_all(&mut **tx).await.map_err(unavailable)?;
  for (attempt, digest, input_digest, Json(schema), document) in records {
    let attempt = history
      .attempts
      .iter()
      .find(|row| row.id().as_uuid() == attempt)
      .ok_or(StoreError::Unavailable)?;
    let completion = history
      .completions
      .iter()
      .find(|row| row.node_attempt_id() == attempt.id())
      .ok_or(StoreError::Unavailable)?;
    let input = data
      .inputs
      .iter()
      .find(|row| row.digest().ok() == Some(attempt.input_digest()))
      .ok_or(StoreError::Unavailable)?;
    if input.digest().map_err(invalid)? != super::digest(&input_digest)? {
      return Err(StoreError::Unavailable);
    }
    let definition = admitted
      .closure()
      .definition(input.definition())
      .ok_or(StoreError::Unavailable)?;
    let record = FlowNodeRecord::restore(
      &document,
      input,
      attempt,
      definition,
      admitted.data_schema(&schema).ok_or(StoreError::Unavailable)?,
      completion,
    )
    .map_err(invalid)?;
    if record.digest().map_err(invalid)? != super::digest(&digest)? {
      return Err(StoreError::Unavailable);
    }
    data.records.push(record);
  }
  if !admitted.data_schemas().is_empty() || data.record_count() != 0 {
    data.validate(work, admitted, history).map_err(invalid)?;
  }
  Ok(data)
}
fn bytes(value: &impl Serialize) -> Result<Vec<u8>, StoreError> {
  serde_json::to_vec(value).map_err(|_| StoreError::Unavailable)
}
fn invalid(_: FactoryError) -> StoreError {
  StoreError::Unavailable
}
