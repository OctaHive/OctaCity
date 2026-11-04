use octacity_server_domain::EntityKind;
use octacity_server_store::{StoreError, StoreOperation, TriggerDefinitionRef, TriggerKind, TriggerTarget};
use sqlx::PgPool;

use crate::database::{number, unavailable};

pub(crate) async fn require_enabled(
  pool: &PgPool,
  trigger: TriggerDefinitionRef,
  kind: TriggerKind,
  target: TriggerTarget,
) -> Result<(), StoreError> {
  let version = number(trigger.version.get(), StoreOperation::AcceptTrigger)?;
  let configuration_version = number(target.configuration_version.get(), StoreOperation::AcceptTrigger)?;
  let exists: bool = sqlx::query_scalar(
    "SELECT EXISTS (\
       SELECT 1 FROM triggers \
       WHERE id = $1 AND version = $2 AND kind = $3 AND enabled \
         AND build_configuration_id = $4 AND build_configuration_version = $5\
     )",
  )
  .bind(trigger.id.as_uuid())
  .bind(version)
  .bind(kind.as_str())
  .bind(target.configuration_id.as_uuid())
  .bind(configuration_version)
  .fetch_one(pool)
  .await
  .map_err(unavailable)?;
  if exists {
    Ok(())
  } else {
    Err(StoreError::NotFound {
      entity: EntityKind::Trigger,
    })
  }
}
