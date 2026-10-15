//! Fenced private compiler publications; ordinary output tables remain Agent-owned.
use crate::{PostgresStore, database::unavailable};
use async_trait::async_trait;
use octacity_server_domain::{ArtifactId, ArtifactUploadId, EntityKind, Timestamp};
use octacity_server_factory::{FactoryDigest, FactoryKey};
use octacity_server_store::{
  ArtifactContentDigest, ArtifactMediaType, AuditActorKind, CompleteFactoryExecutionInputDeletion, FactoryAuditFact,
  FactoryExecutionInputDisposition, FactoryExecutionInputRetentionStore, FactoryExecutionInputStore,
  PublishFactoryExecutionInputs, PublishedExecutionInput, StoreError, StoreInputError, StoreOperation,
};
use sqlx::{PgPool, Postgres};

fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Artifact,
  }
}
async fn retained<'e, E: sqlx::Executor<'e, Database = Postgres>>(
  executor: E,
  operation: FactoryDigest,
) -> Result<Option<(Vec<u8>, bool)>, StoreError> {
  sqlx::query_as("SELECT manifest_digest, published_at_millis IS NOT NULL FROM factory_execution_input_manifests WHERE operation_id=$1")
    .bind(operation.as_bytes().as_slice())
    .fetch_optional(executor)
    .await
    .map_err(unavailable)
}
fn identical(retained: Vec<u8>, expected: FactoryDigest) -> Result<(), StoreError> {
  if retained == expected.as_bytes() {
    Ok(())
  } else {
    Err(conflict())
  }
}
#[async_trait]
impl FactoryExecutionInputStore for PostgresStore {
  async fn reserve_factory_execution_inputs(
    &self,
    request: PublishFactoryExecutionInputs,
  ) -> Result<FactoryExecutionInputDisposition, StoreError> {
    apply(self.pool(), request, false).await
  }
  async fn publish_factory_execution_inputs(&self, request: PublishFactoryExecutionInputs) -> Result<(), StoreError> {
    apply(self.pool(), request, true).await.map(|_| ())
  }
}

async fn apply(
  pool: &PgPool,
  request: PublishFactoryExecutionInputs,
  publish: bool,
) -> Result<FactoryExecutionInputDisposition, StoreError> {
  request.validate()?;
  let operation = request.intent.operation_id().map_err(|_| conflict())?;
  let bytes = serde_json::to_vec(&(&request.intent, &request.inputs)).map_err(|_| conflict())?;
  let digest = FactoryDigest::sha256("octacity.factory.execution-input-publication.v1", &[&bytes]);
  if let Some((previous, published)) = retained(pool, operation).await? {
    identical(previous, digest)?;
    if published {
      return Ok(FactoryExecutionInputDisposition::Published);
    }
  }
  let mut tx = pool.begin().await.map_err(unavailable)?;
  crate::factory_run::verify_node_dispatch(&mut tx, &request.intent, &request.ownership, request.published_at).await?;
  match retained(&mut *tx, operation).await? {
    Some((previous, published)) => {
      identical(previous, digest)?;
      if published {
        return Ok(FactoryExecutionInputDisposition::Published);
      }
      if !publish {
        return Ok(FactoryExecutionInputDisposition::Pending);
      }
    }
    None => {
      sqlx::query("INSERT INTO factory_execution_input_manifests (operation_id,run_id,manifest_digest,reserved_at_millis,expires_at_millis) VALUES ($1,$2,$3,$4,$5)")
        .bind(operation.as_bytes().as_slice()).bind(request.intent.input().factory_run_id().as_uuid())
        .bind(digest.as_bytes().as_slice()).bind(request.published_at.unix_millis())
        .bind(request.intent.node().deadline().unix_millis()).execute(&mut *tx).await.map_err(unavailable)?;
      for entry in &request.inputs {
        let input = &entry.input;
        let artifact = input.artifact_id.parse::<ArtifactId>().map_err(|_| conflict())?;
        let content = ArtifactContentDigest::from_lower_hex(&input.sha256).map_err(|_| conflict())?;
        sqlx::query("INSERT INTO factory_execution_inputs (artifact_id,upload_id,operation_id,size_bytes,content_digest,media_type) VALUES ($1,$2,$3,$4,$5,$6)")
          .bind(artifact.as_uuid()).bind(entry.upload_id.as_uuid()).bind(operation.as_bytes().as_slice())
          .bind(i64::try_from(input.size_bytes).map_err(|_| conflict())?).bind(content.as_bytes().as_slice())
          .bind(&input.media_type).execute(&mut *tx).await.map_err(unavailable)?;
      }
    }
  }
  if publish {
    sqlx::query("UPDATE factory_execution_input_manifests SET published_at_millis=$2 WHERE operation_id=$1")
      .bind(operation.as_bytes().as_slice())
      .bind(request.published_at.unix_millis())
      .execute(&mut *tx)
      .await
      .map_err(unavailable)?;
  }
  crate::factory_run::insert_audit(
    &mut tx,
    &FactoryAuditFact::new(
      request.intent.input().factory_run_id(),
      AuditActorKind::Worker,
      None,
      FactoryKey::new(if publish {
        "factory.execution_inputs_published"
      } else {
        "factory.execution_inputs_reserved"
      })
      .map_err(|_| conflict())?,
      digest,
      FactoryKey::new("accepted").map_err(|_| conflict())?,
      request.published_at,
    ),
  )
  .await?;
  tx.commit().await.map_err(unavailable)?;
  Ok(if publish {
    FactoryExecutionInputDisposition::Published
  } else {
    FactoryExecutionInputDisposition::Pending
  })
}

pub(crate) async fn read(pool: &PgPool, artifact: ArtifactId) -> Result<Option<PublishedExecutionInput>, StoreError> {
  type Row = (uuid::Uuid, i64, Vec<u8>, String, i64);
  let row: Option<Row> = sqlx::query_as("SELECT input.upload_id,input.size_bytes,input.content_digest,input.media_type,manifest.expires_at_millis FROM factory_execution_inputs input JOIN factory_execution_input_manifests manifest USING (operation_id) WHERE input.artifact_id=$1 AND manifest.published_at_millis IS NOT NULL AND input.deleted_at_millis IS NULL")
    .bind(artifact.as_uuid()).fetch_optional(pool).await.map_err(unavailable)?;
  row
    .map(|(upload, size, digest, media, expires)| {
      Ok(PublishedExecutionInput {
        artifact_id: artifact,
        upload_id: ArtifactUploadId::from_uuid(upload).map_err(|_| StoreError::Unavailable)?,
        size_bytes: u64::try_from(size).map_err(|_| StoreError::Unavailable)?,
        digest: ArtifactContentDigest::from_bytes(digest.try_into().map_err(|_| StoreError::Unavailable)?),
        media_type: ArtifactMediaType::new(media).map_err(|_| StoreError::Unavailable)?,
        expires_at: Some(Timestamp::from_unix_millis(expires).map_err(|_| StoreError::Unavailable)?),
      })
    })
    .transpose()
}

#[async_trait]
impl FactoryExecutionInputRetentionStore for PostgresStore {
  async fn expired_factory_execution_inputs(
    &self,
    observed_at: Timestamp,
    limit: u16,
  ) -> Result<Vec<PublishedExecutionInput>, StoreError> {
    if !(1..=100).contains(&limit) {
      return Err(StoreError::InvalidInput {
        operation: StoreOperation::RetainFactoryExecutionInputs,
        source: StoreInputError::InvalidFactoryBuild,
      });
    }
    let rows: Vec<CleanupRow> = sqlx::query_as("SELECT input.artifact_id,input.upload_id,input.size_bytes,input.content_digest,input.media_type,manifest.expires_at_millis,manifest.run_id,input.deleted_at_millis FROM factory_execution_inputs input JOIN factory_execution_input_manifests manifest USING (operation_id) WHERE manifest.expires_at_millis <= $1 AND input.deleted_at_millis IS NULL ORDER BY manifest.expires_at_millis,input.artifact_id LIMIT $2")
      .bind(observed_at.unix_millis()).bind(i64::from(limit)).fetch_all(self.pool()).await.map_err(unavailable)?;
    rows.iter().map(CleanupRow::input).collect()
  }
  async fn complete_factory_execution_input_deletion(
    &self,
    request: CompleteFactoryExecutionInputDeletion,
  ) -> Result<(), StoreError> {
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    let row: Option<CleanupRow> = sqlx::query_as("SELECT input.artifact_id,input.upload_id,input.size_bytes,input.content_digest,input.media_type,manifest.expires_at_millis,manifest.run_id,input.deleted_at_millis FROM factory_execution_inputs input JOIN factory_execution_input_manifests manifest USING (operation_id) WHERE input.artifact_id=$1 FOR UPDATE OF input")
      .bind(request.expected.artifact_id.as_uuid()).fetch_optional(&mut *tx).await.map_err(unavailable)?;
    let row = row.ok_or_else(conflict)?;
    if row.input()? != request.expected || request.deleted_at.unix_millis() < row.expires_at_millis {
      return Err(conflict());
    }
    if row.deleted_at_millis.is_some() {
      return Ok(());
    }
    sqlx::query("UPDATE factory_execution_inputs SET deleted_at_millis=$2 WHERE artifact_id=$1")
      .bind(request.expected.artifact_id.as_uuid())
      .bind(request.deleted_at.unix_millis())
      .execute(&mut *tx)
      .await
      .map_err(unavailable)?;
    let digest = FactoryDigest::sha256(
      "octacity.factory.execution-input-deleted.v1",
      &[
        request.expected.artifact_id.as_uuid().as_bytes(),
        request.expected.upload_id.as_uuid().as_bytes(),
        &request.expected.digest.as_bytes(),
      ],
    );
    crate::factory_run::insert_audit(
      &mut tx,
      &FactoryAuditFact::new(
        octacity_server_factory::FactoryRunId::from_uuid(row.run_id).map_err(|_| StoreError::Unavailable)?,
        AuditActorKind::Worker,
        None,
        FactoryKey::new("factory.execution_input_deleted").map_err(|_| conflict())?,
        digest,
        FactoryKey::new("accepted").map_err(|_| conflict())?,
        request.deleted_at,
      ),
    )
    .await?;
    tx.commit().await.map_err(unavailable)
  }
}
#[derive(sqlx::FromRow)]
struct CleanupRow {
  artifact_id: uuid::Uuid,
  upload_id: uuid::Uuid,
  size_bytes: i64,
  content_digest: Vec<u8>,
  media_type: String,
  expires_at_millis: i64,
  run_id: uuid::Uuid,
  deleted_at_millis: Option<i64>,
}
impl CleanupRow {
  fn input(&self) -> Result<PublishedExecutionInput, StoreError> {
    Ok(PublishedExecutionInput {
      artifact_id: ArtifactId::from_uuid(self.artifact_id).map_err(|_| StoreError::Unavailable)?,
      upload_id: ArtifactUploadId::from_uuid(self.upload_id).map_err(|_| StoreError::Unavailable)?,
      size_bytes: u64::try_from(self.size_bytes).map_err(|_| StoreError::Unavailable)?,
      digest: ArtifactContentDigest::from_bytes(
        self
          .content_digest
          .as_slice()
          .try_into()
          .map_err(|_| StoreError::Unavailable)?,
      ),
      media_type: ArtifactMediaType::new(self.media_type.clone()).map_err(|_| StoreError::Unavailable)?,
      expires_at: Some(Timestamp::from_unix_millis(self.expires_at_millis).map_err(|_| StoreError::Unavailable)?),
    })
  }
}
