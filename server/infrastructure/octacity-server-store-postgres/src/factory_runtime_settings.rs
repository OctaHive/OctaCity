//! Immutable Runtime settings through the shared scoped mutation/audit/outbox transaction.
use crate::{
  PostgresStore,
  database::unavailable,
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};
use octacity_server_domain::{EntityKind, ProjectId, Timestamp};
use octacity_server_factory::{
  FactoryKey, FactoryMcpConfiguration, FactoryRuntimeSettings, FactoryRuntimeSettingsVersion, ImmutableReference,
};
use octacity_server_store::*;
use serde_json::{Value, json};
use sqlx::types::Json;

#[async_trait::async_trait]
impl FactoryRuntimeSettingsStore for PostgresStore {
  async fn factory_runtime_mcp_configurations(
    &self,
    project: ProjectId,
    references: &[ImmutableReference],
  ) -> Result<std::collections::BTreeMap<ImmutableReference, FactoryMcpConfiguration>, StoreError> {
    crate::discovery::require_project(self.pool(), project).await?;
    let versions = references
      .iter()
      .filter_map(factory_runtime_mcp_revision)
      .map(|version| version.get() as i64)
      .collect::<std::collections::BTreeSet<_>>();
    if versions.is_empty() {
      return Ok(Default::default());
    }
    let names = references
      .iter()
      .map(|reference| reference.identity().as_str().to_owned())
      .collect::<Vec<_>>();
    let rows: Vec<(String, i64, Json<FactoryRuntimeSettings>)> =
      sqlx::query_as("SELECT runtime, version, settings FROM factory_runtime_settings WHERE project_id=$1 AND version=ANY($2) AND settings->'definition'->'mcp_servers' ?| $3::text[]")
        .bind(project.as_uuid())
        .bind(versions.into_iter().collect::<Vec<_>>())
        .bind(names)
        .fetch_all(self.pool())
        .await
        .map_err(unavailable)?;
    let settings = rows
      .into_iter()
      .map(|(runtime, version, Json(settings))| checked_settings(project, &runtime, version as u64, settings))
      .collect::<Result<Vec<_>, _>>()?;
    resolve_factory_runtime_mcp_configurations(references, &settings)
  }
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
      .map(|(version, Json(settings))| checked_settings(project, runtime.as_str(), version as u64, settings))
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
    let installed = self.factory_capabilities().availability(settings.project_id()).await?;
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
    if let FactoryConfigurationAvailability::Available(choices) = &installed {
      let aliases: Vec<_> = choices
        .role_profile_capabilities()
        .keys()
        .map(FactoryKey::as_str)
        .collect();
      let mut retained = current_settings(&mut *tx, settings.project_id(), aliases).await?;
      retained.retain(|value| value.runtime() != settings.runtime());
      retained.push(settings.clone());
      configured_factory_catalog(installed, retained).map_err(|_| StoreError::InvalidInput {
        operation: StoreOperation::PublishFactoryRuntimeSettings,
        source: StoreInputError::InvalidFactoryRuntimeSettings,
      })?;
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

// One bounded current-head read shared by discovery and publication under its Project lock.
async fn current_settings<'e>(
  executor: impl sqlx::PgExecutor<'e>,
  project: ProjectId,
  aliases: Vec<&str>,
) -> Result<Vec<FactoryRuntimeSettings>, StoreError> {
  let rows: Vec<(String, i64, Json<FactoryRuntimeSettings>)> = sqlx::query_as(
    "SELECT DISTINCT ON (runtime) runtime,version,settings FROM factory_runtime_settings WHERE project_id=$1 AND runtime=ANY($2) ORDER BY runtime,version DESC",
  ).bind(project.as_uuid()).bind(aliases).fetch_all(executor).await.map_err(unavailable)?;
  rows
    .into_iter()
    .map(|(runtime, version, Json(settings))| checked_settings(project, &runtime, version as u64, settings))
    .collect()
}

pub(crate) async fn configured_availability(
  pool: &sqlx::PgPool,
  project: ProjectId,
  installed: FactoryConfigurationAvailability,
) -> Result<FactoryConfigurationAvailability, StoreError> {
  let FactoryConfigurationAvailability::Available(choices) = &installed else {
    return Ok(installed);
  };
  if choices.role_profile_capabilities().is_empty() {
    return Ok(installed);
  }
  let aliases = choices
    .role_profile_capabilities()
    .keys()
    .map(FactoryKey::as_str)
    .collect();
  let settings = current_settings(pool, project, aliases).await?;
  configured_factory_catalog(installed, settings).map_err(|_| StoreError::Unavailable)
}

fn checked_settings(
  project: ProjectId,
  runtime: &str,
  version: u64,
  settings: FactoryRuntimeSettings,
) -> Result<FactoryRuntimeSettings, StoreError> {
  if settings.project_id() != project || settings.runtime().as_str() != runtime || settings.version().get() != version {
    return Err(StoreError::Unavailable);
  }
  Ok(settings)
}
