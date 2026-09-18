use octacity_server_domain::{EntityKind, PipelineId, PipelineName, PipelineVersion, ProjectId};
use octacity_server_pipeline::PublishablePipelineDag;
use octacity_server_store::{
  CreatePipeline, MutationDisposition, PipelineMutationOutcome, PublishPipelineVersion, PublishedPipeline, StoreError,
  StoreInputError, StoreOperation,
};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction, types::Json};

use crate::{
  database::{number, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};

#[derive(Serialize)]
struct CreateFingerprint<'a> {
  id: PipelineId,
  project_id: ProjectId,
  name: &'a PipelineName,
  dag: &'a PublishablePipelineDag,
}

#[derive(Serialize)]
struct PublishFingerprint<'a> {
  id: PipelineId,
  expected_current_version: PipelineVersion,
  dag: &'a PublishablePipelineDag,
}

pub(crate) async fn create(
  pool: &sqlx::PgPool,
  request: CreatePipeline,
) -> Result<PipelineMutationOutcome, StoreError> {
  validate_dag(&request.dag, StoreOperation::CreatePipeline)?;
  let identity = MutationIdentity::new(
    MutationKind::CreatePipeline,
    request.idempotency_key.to_string(),
    request.published_at,
    EntityKind::Pipeline,
    &CreateFingerprint {
      id: request.id,
      project_id: request.project_id,
      name: &request.name,
      dag: &request.dag,
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay(outcome),
  };
  require_project(&mut transaction, request.project_id).await?;

  sqlx::query(
    "INSERT INTO pipelines (id, project_id, name, created_at) \
     VALUES ($1, $2, $3, to_timestamp($4::double precision / 1000.0))",
  )
  .bind(request.id.as_uuid())
  .bind(request.project_id.as_uuid())
  .bind(request.name.as_str())
  .bind(request.published_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(classify_pipeline_conflict)?;
  insert_version(
    &mut transaction,
    request.id,
    PipelineVersion::INITIAL,
    &request.dag,
    request.published_at.unix_millis(),
    StoreOperation::CreatePipeline,
  )
  .await?;

  let pipeline = PublishedPipeline {
    id: request.id,
    project_id: request.project_id,
    name: request.name,
    version: PipelineVersion::INITIAL,
    dag: request.dag.into_dag(),
    published_at: request.published_at,
  };
  let outcome = PipelineMutationOutcome {
    disposition: MutationDisposition::Applied,
    pipeline,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    facts(MutationKind::CreatePipeline, &outcome.pipeline),
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

pub(crate) async fn publish(
  pool: &sqlx::PgPool,
  request: PublishPipelineVersion,
) -> Result<PipelineMutationOutcome, StoreError> {
  validate_dag(&request.dag, StoreOperation::PublishPipelineVersion)?;
  let identity = MutationIdentity::new(
    MutationKind::PublishPipelineVersion,
    request.idempotency_key.to_string(),
    request.published_at,
    EntityKind::Pipeline,
    &PublishFingerprint {
      id: request.id,
      expected_current_version: request.expected_current_version,
      dag: &request.dag,
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay(outcome),
  };
  let (project_id, name) = lock_pipeline(&mut transaction, request.id).await?;
  let (current_version, current_published_at): (i64, i64) = sqlx::query_as(
    "SELECT version, FLOOR(EXTRACT(EPOCH FROM published_at) * 1000)::BIGINT \
     FROM pipeline_versions WHERE pipeline_id = $1 ORDER BY version DESC LIMIT 1",
  )
  .bind(request.id.as_uuid())
  .fetch_one(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let expected = number(
    request.expected_current_version.get(),
    StoreOperation::PublishPipelineVersion,
  )?;
  if current_version != expected || request.published_at.unix_millis() < current_published_at {
    return Err(conflict());
  }
  let version = request
    .expected_current_version
    .next()
    .map_err(|_| StoreError::Unavailable)?;
  insert_version(
    &mut transaction,
    request.id,
    version,
    &request.dag,
    request.published_at.unix_millis(),
    StoreOperation::PublishPipelineVersion,
  )
  .await?;

  let pipeline = PublishedPipeline {
    id: request.id,
    project_id,
    name,
    version,
    dag: request.dag.into_dag(),
    published_at: request.published_at,
  };
  let outcome = PipelineMutationOutcome {
    disposition: MutationDisposition::Applied,
    pipeline,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    facts(MutationKind::PublishPipelineVersion, &outcome.pipeline),
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

async fn require_project(transaction: &mut Transaction<'_, Postgres>, project_id: ProjectId) -> Result<(), StoreError> {
  let exists = sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM projects WHERE id = $1 FOR KEY SHARE")
    .bind(project_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(unavailable)?
    .is_some();
  if !exists {
    return Err(StoreError::NotFound {
      entity: EntityKind::Project,
    });
  }
  Ok(())
}

async fn lock_pipeline(
  transaction: &mut Transaction<'_, Postgres>,
  pipeline_id: PipelineId,
) -> Result<(ProjectId, PipelineName), StoreError> {
  let (project_id, name): (uuid::Uuid, String) =
    sqlx::query_as("SELECT project_id, name FROM pipelines WHERE id = $1 FOR UPDATE")
      .bind(pipeline_id.as_uuid())
      .fetch_optional(&mut **transaction)
      .await
      .map_err(unavailable)?
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Pipeline,
      })?;
  Ok((
    ProjectId::from_uuid(project_id).map_err(|_| StoreError::Unavailable)?,
    PipelineName::new(name).map_err(|_| StoreError::Unavailable)?,
  ))
}

async fn insert_version(
  transaction: &mut Transaction<'_, Postgres>,
  pipeline_id: PipelineId,
  version: PipelineVersion,
  dag: &PublishablePipelineDag,
  published_at_millis: i64,
  operation: StoreOperation,
) -> Result<(), StoreError> {
  let version = number(version.get(), operation)?;
  let snapshot: Value = serde_json::to_value(dag).map_err(|_| StoreError::Unavailable)?;
  sqlx::query(
    "INSERT INTO pipeline_versions (pipeline_id, version, dag_snapshot, published_at) \
     VALUES ($1, $2, $3, to_timestamp($4::double precision / 1000.0))",
  )
  .bind(pipeline_id.as_uuid())
  .bind(version)
  .bind(Json(snapshot))
  .bind(published_at_millis)
  .execute(&mut **transaction)
  .await
  .map_err(classify_pipeline_conflict)?;
  Ok(())
}

fn validate_dag(dag: &PublishablePipelineDag, operation: StoreOperation) -> Result<(), StoreError> {
  dag.validate().map_err(|_| StoreError::InvalidInput {
    operation,
    source: StoreInputError::InvalidPipelineSnapshot,
  })
}

fn facts(kind: MutationKind, pipeline: &PublishedPipeline) -> MutationFacts {
  MutationFacts {
    actor_kind: "unauthenticated_management",
    actor_identity: None,
    target_identity: pipeline.id.to_string(),
    safe_metadata: json!({
      "edge_count": pipeline.dag.edges().len(),
      "node_count": pipeline.dag.nodes().len(),
      "project_id": pipeline.project_id,
      "version": pipeline.version.get(),
    }),
    outbox_payload: json!({
      "event": kind.outbox_topic(),
      "pipeline_id": pipeline.id,
      "project_id": pipeline.project_id,
      "version": pipeline.version.get(),
    }),
  }
}

fn replay(value: Value) -> Result<PipelineMutationOutcome, StoreError> {
  let mut outcome: PipelineMutationOutcome = decode_outcome(value)?;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(outcome)
}

fn classify_pipeline_conflict(error: sqlx::Error) -> StoreError {
  match error.as_database_error().and_then(|database| database.code()) {
    Some(code) if matches!(code.as_ref(), "23503" | "23505" | "23514") => conflict(),
    _ => StoreError::Unavailable,
  }
}

fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Pipeline,
  }
}
