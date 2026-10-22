use crate::database::unavailable;
use octacity_server_store::{AgentResourceObservation, AgentResources, StoreError};
use sqlx::{PgConnection, types::Json};

pub(crate) async fn record(connection: &mut PgConnection, input: AgentResourceObservation) -> Result<(), StoreError> {
  let value = AgentResources {
    observed_at: input.observed_at,
    available_cpu_millis: input.snapshot.available_cpu_millis,
    available_memory_bytes: input.snapshot.available_memory_bytes,
    work_disk_free_bytes: input.snapshot.work_disk_free_bytes,
    state_disk_free_bytes: input.snapshot.state_disk_free_bytes,
  };
  sqlx::query("UPDATE agents agent SET resources = $1, last_seen_at = GREATEST(last_seen_at, to_timestamp($2::double precision / 1000.0)) \
    WHERE agent.id = $3 AND (resources IS NULL OR (resources->>'observed_at')::bigint < $2) \
    AND EXISTS (SELECT 1 FROM agent_registrations registration WHERE registration.agent_id = agent.id \
      AND registration.epoch = $4 AND registration.revoked_at IS NULL \
      AND registration.expires_at > to_timestamp($2::double precision / 1000.0)) \
    AND $5 <= (inventory->'host_capacity'->>'logical_cpu_count')::bigint * 1000 \
    AND $6 <= (inventory->'host_capacity'->>'total_memory_bytes')::bigint \
    AND $7 <= (inventory->'host_capacity'->>'work_disk_total_bytes')::bigint \
    AND $8 <= (inventory->'host_capacity'->>'state_disk_total_bytes')::bigint")
    .bind(Json(&value)).bind(input.observed_at.unix_millis()).bind(input.agent_id.as_uuid())
    .bind(i64::try_from(input.registration_epoch.get()).map_err(|_| StoreError::Unavailable)?)
    .bind(i64::try_from(value.available_cpu_millis).map_err(|_| StoreError::Unavailable)?)
    .bind(i64::try_from(value.available_memory_bytes).map_err(|_| StoreError::Unavailable)?)
    .bind(i64::try_from(value.work_disk_free_bytes).map_err(|_| StoreError::Unavailable)?)
    .bind(i64::try_from(value.state_disk_free_bytes).map_err(|_| StoreError::Unavailable)?)
    .execute(connection).await.map_err(unavailable)?;
  Ok(())
}
