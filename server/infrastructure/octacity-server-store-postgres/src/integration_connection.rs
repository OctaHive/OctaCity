//! Metadata publication uses the existing management transaction/replay boundary.
use octacity_server_domain::{EntityKind, IntegrationId, IntegrationVersion, Timestamp};
use octacity_server_store::{
  IntegrationConnectionMutationOutcome, IntegrationConnectionPage, ManagementMutation, MutationDisposition,
  PublishIntegrationConnection, PublishedIntegrationConnection, ReadIntegrationConnections,
  ReplayIntegrationConnection, StoreError,
};
use serde_json::{Value, json};
use sqlx::{PgPool, types::Json};

use crate::{
  database::{classify, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};

const KIND: MutationKind = MutationKind::PublishIntegrationConnection;
fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Integration,
  }
}
fn replay(value: Value) -> Result<IntegrationConnectionMutationOutcome, StoreError> {
  let mut outcome: IntegrationConnectionMutationOutcome = decode_outcome(value)?;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(outcome)
}
pub(crate) async fn replay_mutation(
  pool: &PgPool,
  request: ManagementMutation<ReplayIntegrationConnection>,
) -> Result<Option<IntegrationConnectionMutationOutcome>, StoreError> {
  let (request, audit) = request.into_parts();
  let identity = MutationIdentity::new_management(
    &audit,
    KIND,
    request.idempotency_key.to_string(),
    Timestamp::from_unix_millis(0).map_err(|_| StoreError::Unavailable)?,
    EntityKind::Integration,
    &request.intent,
  )?;
  let row: Option<(Vec<u8>, Json<Value>)> = sqlx::query_as(
    "SELECT request_digest, outcome FROM idempotency_records \
     WHERE scope = $1 AND security_scope = $2 AND idempotency_key = $3",
  )
  .bind(KIND.scope())
  .bind(identity.persisted_security_scope())
  .bind(&identity.key)
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?;
  match row {
    None => Ok(None),
    Some((digest, _)) if digest != identity.request_digest => Err(conflict()),
    Some((_, Json(value))) => Ok(Some(replay(value)?)),
  }
}
pub(crate) async fn publish(
  pool: &PgPool,
  request: ManagementMutation<PublishIntegrationConnection>,
) -> Result<IntegrationConnectionMutationOutcome, StoreError> {
  let (request, audit) = request.into_parts();
  let identity = MutationIdentity::new_management(
    &audit,
    KIND,
    request.idempotency_key.to_string(),
    request.published_at,
    EntityKind::Integration,
    &request.intent,
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Replay(value) => return replay(value),
    MutationStart::Fresh(transaction) => transaction,
  };
  let current: Option<i64> =
    sqlx::query_scalar("SELECT current_version FROM integration_connections WHERE id = $1 FOR UPDATE")
      .bind(request.intent.id.as_uuid())
      .fetch_optional(&mut *transaction)
      .await
      .map_err(unavailable)?;
  if current.map(|v| v as u64) != request.intent.expected_current_version.map(|v| v.get()) {
    return Err(conflict());
  }
  let connection = request.resolve()?;
  if current.is_none() {
    sqlx::query("INSERT INTO integration_connections (id, current_version) VALUES ($1, $2)")
      .bind(connection.id.as_uuid())
      .bind(connection.version.get() as i64)
      .execute(&mut *transaction)
      .await
      .map_err(|error| classify(error, EntityKind::Integration))?;
  }
  sqlx::query(
    "INSERT INTO integration_connection_versions (connection_id, version, definition_digest, document) \
    VALUES ($1, $2, $3, $4)",
  )
  .bind(connection.id.as_uuid())
  .bind(connection.version.get() as i64)
  .bind(connection.digest.as_bytes().as_slice())
  .bind(Json(&connection))
  .execute(&mut *transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Integration))?;
  if current.is_some() {
    sqlx::query("UPDATE integration_connections SET current_version = $2 WHERE id = $1")
      .bind(connection.id.as_uuid())
      .bind(connection.version.get() as i64)
      .execute(&mut *transaction)
      .await
      .map_err(unavailable)?;
  }
  let outcome = IntegrationConnectionMutationOutcome {
    disposition: MutationDisposition::Applied,
    connection,
  };
  let facts = MutationFacts::management(
    &audit,
    outcome.connection.id.to_string(),
    json!({"version":outcome.connection.version.get()}),
    json!({"event":KIND.outbox_topic(),"integration_id":outcome.connection.id,"version":outcome.connection.version.get()}),
  );
  crate::mutation::commit(transaction, &identity, facts, encode_outcome(&outcome)?).await?;
  Ok(outcome)
}
pub(crate) async fn read<'e>(
  executor: impl sqlx::Executor<'e, Database = sqlx::Postgres>,
  id: IntegrationId,
  version: Option<IntegrationVersion>,
) -> Result<PublishedIntegrationConnection, StoreError> {
  let version = version
    .map(|v| i64::try_from(v.get()).map_err(|_| StoreError::Unavailable))
    .transpose()?;
  let row: Option<(i64, Vec<u8>, Json<PublishedIntegrationConnection>)> = sqlx::query_as(
    "SELECT v.version, v.definition_digest, v.document FROM integration_connection_versions v \
     JOIN integration_connections c ON c.id = v.connection_id \
     WHERE c.id = $1 AND v.version = COALESCE($2, c.current_version)",
  )
  .bind(id.as_uuid())
  .bind(version)
  .fetch_optional(executor)
  .await
  .map_err(unavailable)?;
  let (version, digest, Json(connection)) = row.ok_or(StoreError::NotFound {
    entity: EntityKind::Integration,
  })?;
  checked(id, version, digest, connection)
}
fn checked(
  id: IntegrationId,
  version: i64,
  digest: Vec<u8>,
  connection: PublishedIntegrationConnection,
) -> Result<PublishedIntegrationConnection, StoreError> {
  if connection.id != id
    || connection.version.get() != version as u64
    || connection.digest.as_bytes().as_slice() != digest
    || connection.definition.content_digest()? != connection.digest
  {
    return Err(StoreError::Unavailable);
  }
  Ok(connection)
}

pub(crate) async fn list(
  pool: &PgPool,
  request: ReadIntegrationConnections,
) -> Result<IntegrationConnectionPage, StoreError> {
  request.validate()?;
  let scope = crate::read_visibility::sql_read_visibility(request.visibility.view(), |id| id.as_uuid());
  let rows: Vec<(uuid::Uuid, i64, Vec<u8>, Json<PublishedIntegrationConnection>)> = sqlx::query_as(
    "SELECT c.id,v.version,v.definition_digest,v.document FROM integration_connections c \
     JOIN integration_connection_versions v ON v.connection_id=c.id AND v.version=c.current_version \
     WHERE ($1 OR c.id=ANY($2::uuid[])) AND ($3::uuid IS NULL OR c.id>$3) ORDER BY c.id LIMIT $4",
  )
  .bind(scope.all)
  .bind(scope.identities)
  .bind(request.after.map(|id| id.as_uuid()))
  .bind(i64::from(request.limit) + 1)
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;
  let mut items = rows
    .into_iter()
    .map(|(id, version, digest, Json(connection))| {
      checked(
        IntegrationId::from_uuid(id).map_err(|_| StoreError::Unavailable)?,
        version,
        digest,
        connection,
      )
    })
    .collect::<Result<Vec<_>, _>>()?;
  let next_cursor = (items.len() > usize::from(request.limit)).then(|| items[usize::from(request.limit) - 1].id);
  items.truncate(usize::from(request.limit));
  Ok(IntegrationConnectionPage { items, next_cursor })
}
