use octacity_server_domain::{
  BuildConfigurationId, BuildConfigurationVersion, EntityKind, Timestamp, TriggerId, TriggerVersion,
};
use octacity_server_store::{ManualTriggerDefinitionRecord, StoreError, StoreOperation};
use serde_json::Value;
use sqlx::{FromRow, PgPool, types::Json};

use crate::database::{number, unavailable};

#[derive(FromRow)]
struct ManualTriggerDefinitionRow {
  id: uuid::Uuid,
  version: i64,
  build_configuration_id: uuid::Uuid,
  build_configuration_version: i64,
  enabled: bool,
  definition: Json<Value>,
  created_at_millis: i64,
}

pub(crate) async fn read_manual(
  pool: &PgPool,
  trigger_id: TriggerId,
  version: TriggerVersion,
) -> Result<ManualTriggerDefinitionRecord, StoreError> {
  let row = sqlx::query_as::<_, ManualTriggerDefinitionRow>(
    "SELECT id, version, build_configuration_id, build_configuration_version, enabled, definition, \
       FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_millis \
     FROM triggers WHERE id = $1 AND version = $2 AND kind = 'manual'",
  )
  .bind(trigger_id.as_uuid())
  .bind(number(version.get(), StoreOperation::ReadManualTriggerDefinition)?)
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Trigger,
  })?;

  Ok(ManualTriggerDefinitionRecord {
    id: TriggerId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?,
    version: positive_version(row.version)?,
    configuration_id: BuildConfigurationId::from_uuid(row.build_configuration_id)
      .map_err(|_| StoreError::Unavailable)?,
    configuration_version: positive_configuration_version(row.build_configuration_version)?,
    enabled: row.enabled,
    definition: row.definition.0,
    created_at: Timestamp::from_unix_millis(row.created_at_millis).map_err(|_| StoreError::Unavailable)?,
  })
}

fn positive_version(value: i64) -> Result<TriggerVersion, StoreError> {
  u64::try_from(value)
    .ok()
    .and_then(|value| TriggerVersion::new(value).ok())
    .ok_or(StoreError::Unavailable)
}

fn positive_configuration_version(value: i64) -> Result<BuildConfigurationVersion, StoreError> {
  u64::try_from(value)
    .ok()
    .and_then(|value| BuildConfigurationVersion::new(value).ok())
    .ok_or(StoreError::Unavailable)
}
