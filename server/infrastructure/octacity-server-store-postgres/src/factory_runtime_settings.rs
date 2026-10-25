//! Immutable Runtime settings through the shared scoped mutation/audit/outbox transaction.
use crate::{
  PostgresStore,
  database::unavailable,
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};
use octacity_server_domain::{EntityKind, ProjectId, Timestamp};
use octacity_server_factory::{FactoryKey, FactoryRuntimeSettings, FactoryRuntimeSettingsVersion};
use octacity_server_store::*;
use serde_json::{Value, json};
use sqlx::types::Json;

#[async_trait::async_trait]
impl FactoryRuntimeSettingsStore for PostgresStore {
  async fn factory_runtime_settings(
    &self,
    project: ProjectId,
    runtime: &FactoryKey,
    version: Option<FactoryRuntimeSettingsVersion>,
  ) -> Result<Option<FactoryRuntimeSettings>, StoreError> {
    crate::discovery::require_project(self.pool(), project).await?;
    if version.is_some_and(|v| v.get() > i64::MAX as u64) {
      return Ok(None);
    }
    let row: Option<(i64, Json<FactoryRuntimeSettings>)> = sqlx::query_as(
      "SELECT version, settings FROM factory_runtime_settings WHERE project_id=$1 AND runtime=$2 AND ($3::bigint IS NULL OR version=$3) ORDER BY version DESC LIMIT 1",
    ).bind(project.as_uuid()).bind(runtime.as_str()).bind(version.map(|v|v.get() as i64))
      .fetch_optional(self.pool()).await.map_err(unavailable)?;
    row
      .map(|(stored_version, Json(settings))| {
        if settings.project_id() != project
          || settings.runtime() != runtime
          || settings.version().get() != stored_version as u64
        {
          return Err(StoreError::Unavailable);
        }
        Ok(settings)
      })
      .transpose()
  }
  async fn replay_factory_runtime_settings(
    &self,
    request: ManagementMutation<ReplayFactoryRuntimeSettings>,
  ) -> Result<Option<FactoryRuntimeSettingsOutcome>, StoreError> {
    let (request, audit) = request.into_parts();
    let identity = identity(
      &audit,
      &request.intent,
      &request.idempotency_key,
      Timestamp::from_unix_millis(0).map_err(|_| StoreError::Unavailable)?,
    )?;
    let row:Option<(Vec<u8>,Json<Value>)>=sqlx::query_as(
      "SELECT request_digest, outcome FROM idempotency_records WHERE scope=$1 AND security_scope=$2 AND idempotency_key=$3",
    ).bind(identity.kind.scope()).bind(identity.persisted_security_scope()).bind(&identity.key)
      .fetch_optional(self.pool()).await.map_err(unavailable)?;
    match row {
      None => Ok(None),
      Some((digest, Json(value))) if digest == identity.request_digest => Ok(Some(replay(value)?)),
      Some(_) => Err(StoreError::Conflict {
        entity: EntityKind::Project,
      }),
    }
  }
  async fn publish_factory_runtime_settings(
    &self,
    request: ManagementMutation<PublishFactoryRuntimeSettings>,
  ) -> Result<FactoryRuntimeSettingsOutcome, FactoryRuntimeSettingsPublicationError> {
    let (request, audit) = request.into_parts();
    let identity = identity(&audit, &request.intent, &request.idempotency_key, request.published_at)?;
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Fresh(tx) => tx,
      MutationStart::Replay(value) => return Ok(replay(value)?),
    };
    let settings = request.intent.settings()?;
    // Existing Project publication lock also covers creation before any settings row exists.
    sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM projects WHERE id=$1 FOR UPDATE")
      .bind(settings.project_id().as_uuid())
      .fetch_optional(&mut *tx)
      .await
      .map_err(unavailable)?
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Project,
      })?;
    let current: Option<i64> = sqlx::query_scalar(
      "SELECT version FROM factory_runtime_settings WHERE project_id=$1 AND runtime=$2 ORDER BY version DESC LIMIT 1",
    )
    .bind(settings.project_id().as_uuid())
    .bind(settings.runtime().as_str())
    .fetch_optional(&mut *tx)
    .await
    .map_err(unavailable)?;
    if current.map(|v| v as u64) != request.intent.expected_version.map(FactoryRuntimeSettingsVersion::get) {
      return Err(FactoryRuntimeSettingsPublicationError::PreconditionFailed);
    }
    sqlx::query("INSERT INTO factory_runtime_settings (project_id,runtime,version,settings) VALUES ($1,$2,$3,$4)")
      .bind(settings.project_id().as_uuid())
      .bind(settings.runtime().as_str())
      .bind(settings.version().get() as i64)
      .bind(Json(&settings))
      .execute(&mut *tx)
      .await
      .map_err(unavailable)?;
    let metadata = json!({"runtime":settings.runtime(),"version":settings.version(),"digest":settings.digest()});
    let payload = json!({"project_id":settings.project_id(),"runtime":settings.runtime(),"version":settings.version(),"digest":settings.digest()});
    let outcome = FactoryRuntimeSettingsOutcome {
      disposition: MutationDisposition::Applied,
      settings,
      published_at: request.published_at,
    };
    crate::mutation::commit(
      tx,
      &identity,
      MutationFacts::management(&audit, request.intent.project_id.to_string(), metadata, payload),
      encode_outcome(&outcome)?,
    )
    .await?;
    Ok(outcome)
  }
}
fn identity(
  audit: &MutationAuditContext,
  intent: &FactoryRuntimeSettingsIntent,
  key: &IdempotencyKey,
  time: Timestamp,
) -> Result<MutationIdentity, StoreError> {
  MutationIdentity::new_management(
    audit,
    MutationKind::PublishFactoryRuntimeSettings,
    key.to_string(),
    time,
    EntityKind::Project,
    intent,
  )
}
fn replay(value: Value) -> Result<FactoryRuntimeSettingsOutcome, StoreError> {
  let mut outcome: FactoryRuntimeSettingsOutcome = decode_outcome(value)?;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(outcome)
}
