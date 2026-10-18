//! Explicit grants share the existing atomic management replay/audit/outbox boundary.
use crate::{
  database::{classify, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};
use octacity_server_domain::{EntityKind, IntegrationId, ProjectId};
use octacity_server_store::*;
use serde_json::{Value, json};
use sqlx::{PgPool, types::Json};
const KIND: MutationKind = MutationKind::PublishIntegrationProjectAccess;
fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Integration,
  }
}
fn replay(value: Value) -> Result<IntegrationProjectAccessOutcome, StoreError> {
  let mut result: IntegrationProjectAccessOutcome = decode_outcome(value)?;
  result.disposition = MutationDisposition::Replayed;
  Ok(result)
}
pub(crate) async fn publish(
  pool: &PgPool,
  request: ManagementMutation<PublishIntegrationProjectAccess>,
) -> Result<IntegrationProjectAccessOutcome, StoreError> {
  let (request, audit) = request.into_parts();
  let intent = &request.intent;
  let identity = MutationIdentity::new_management(
    &audit,
    KIND,
    request.idempotency_key.to_string(),
    request.published_at,
    EntityKind::Integration,
    intent,
  )?;
  let mut tx = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Replay(value) => return replay(value),
    MutationStart::Fresh(tx) => tx,
  };
  let current: Option<i64> =
    sqlx::query_scalar("SELECT current_version FROM integration_connections WHERE id = $1 FOR UPDATE")
      .bind(intent.connection_id.as_uuid())
      .fetch_optional(&mut *tx)
      .await
      .map_err(unavailable)?;
  if current.map(|v| v as u64) != Some(intent.connection_version.get()) {
    return Err(conflict());
  }
  let grant_version: Option<i64> = sqlx::query_scalar(
    "SELECT current_version FROM integration_project_access WHERE connection_id = $1 AND project_id = $2 FOR UPDATE",
  )
  .bind(intent.connection_id.as_uuid())
  .bind(intent.project_id.as_uuid())
  .fetch_optional(&mut *tx)
  .await
  .map_err(unavailable)?;
  if grant_version.map(|v| v as u64) != intent.expected_current_version.map(|v| v.get()) {
    return Err(conflict());
  }
  let metadata =
    crate::integration_connection::read(&mut *tx, intent.connection_id, Some(intent.connection_version)).await?;
  let access = request.resolve(&metadata)?;
  sqlx::query("INSERT INTO integration_project_access (connection_id, project_id, current_version) VALUES ($1,$2,$3) ON CONFLICT (connection_id, project_id) DO UPDATE SET current_version = EXCLUDED.current_version")
    .bind(intent.connection_id.as_uuid()).bind(intent.project_id.as_uuid()).bind(access.version.get() as i64)
    .execute(&mut *tx).await.map_err(|e| classify(e, EntityKind::Integration))?;
  sqlx::query("INSERT INTO integration_project_access_versions (connection_id, project_id, version, connection_version, document) VALUES ($1,$2,$3,$4,$5)")
    .bind(intent.connection_id.as_uuid()).bind(intent.project_id.as_uuid()).bind(access.version.get() as i64)
    .bind(access.connection_version.get() as i64).bind(Json(&access)).execute(&mut *tx).await.map_err(unavailable)?;
  let outcome = IntegrationProjectAccessOutcome {
    disposition: MutationDisposition::Applied,
    access,
  };
  let facts = MutationFacts::management(
    &audit,
    intent.connection_id.to_string(),
    json!({"project_id":intent.project_id,"version":outcome.access.version.get()}),
    json!({"event":KIND.outbox_topic(),"integration_id":intent.connection_id,"project_id":intent.project_id,"version":outcome.access.version.get()}),
  );
  crate::mutation::commit(tx, &identity, facts, encode_outcome(&outcome)?).await?;
  Ok(outcome)
}
pub(crate) async fn read<'e>(
  executor: impl sqlx::Executor<'e, Database = sqlx::Postgres>,
  connection_id: IntegrationId,
  project_id: ProjectId,
) -> Result<Option<PublishedIntegrationProjectAccess>, StoreError> {
  let row: Option<(i64, i64, Json<PublishedIntegrationProjectAccess>)> = sqlx::query_as("SELECT v.version, v.connection_version, v.document FROM integration_project_access a JOIN integration_project_access_versions v ON v.connection_id = a.connection_id AND v.project_id = a.project_id AND v.version = a.current_version WHERE a.connection_id = $1 AND a.project_id = $2")
    .bind(connection_id.as_uuid()).bind(project_id.as_uuid()).fetch_optional(executor).await.map_err(unavailable)?;
  let Some((version, metadata_version, Json(access))) = row else {
    return Ok(None);
  };
  if access.connection_id != connection_id
    || access.project_id != project_id
    || access.version.get() != version as u64
    || access.connection_version.get() != metadata_version as u64
  {
    return Err(StoreError::Unavailable);
  }
  Ok(Some(access))
}
