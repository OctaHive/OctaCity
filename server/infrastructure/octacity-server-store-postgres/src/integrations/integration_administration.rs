//! Recoverable safe journal over the existing management mutation/audit authority.
use crate::{
  database::{classify, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, encode_outcome},
};
use octacity_server_domain::{EntityKind, IntegrationId};
use octacity_server_store::CredentialPublication;
use octacity_server_store::*;
use sqlx::{PgPool, Postgres, Transaction, types::Json};

fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Integration,
  }
}
async fn metadata(
  tx: &mut Transaction<'_, Postgres>,
  id: IntegrationId,
) -> Result<PublishedIntegrationConnection, StoreError> {
  let row: Option<(i64, Vec<u8>, Json<PublishedIntegrationConnection>)> = sqlx::query_as(
    "SELECT c.current_version, v.definition_digest, v.document FROM integration_connections c \
     JOIN integration_connection_versions v ON v.connection_id = c.id AND v.version = c.current_version \
     WHERE c.id = $1 FOR UPDATE OF c",
  )
  .bind(id.as_uuid())
  .fetch_optional(&mut **tx)
  .await
  .map_err(unavailable)?;
  let (version, digest, Json(metadata)) = row.ok_or_else(conflict)?;
  if metadata.id != id
    || metadata.version.get() != version as u64
    || metadata.digest.as_bytes().as_slice() != digest
    || metadata.definition.content_digest()? != metadata.digest
  {
    return Err(StoreError::Unavailable);
  }
  Ok(metadata)
}
pub(crate) async fn read_state(
  tx: &mut Transaction<'_, Postgres>,
  id: IntegrationId,
) -> Result<IntegrationAdministrationState, StoreError> {
  let row: Option<(i64, Json<IntegrationAdministrationState>)> =
    sqlx::query_as("SELECT revision, document FROM integration_administration WHERE connection_id = $1")
      .bind(id.as_uuid())
      .fetch_optional(&mut **tx)
      .await
      .map_err(unavailable)?;
  checked_state(id, row)
}
fn checked_state(
  id: IntegrationId,
  row: Option<(i64, Json<IntegrationAdministrationState>)>,
) -> Result<IntegrationAdministrationState, StoreError> {
  match row {
    None => Ok(IntegrationAdministrationState::new(id)),
    Some((revision, Json(state))) if state.id == id && state.revision == revision as u64 => Ok(state),
    _ => Err(StoreError::Unavailable),
  }
}
async fn write_state(
  tx: &mut Transaction<'_, Postgres>,
  state: &IntegrationAdministrationState,
) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO integration_administration (connection_id, revision, document) VALUES ($1, $2, $3) \
    ON CONFLICT (connection_id) DO UPDATE SET revision = EXCLUDED.revision, document = EXCLUDED.document",
  )
  .bind(state.id.as_uuid())
  .bind(state.revision as i64)
  .bind(Json(state))
  .execute(&mut **tx)
  .await
  .map_err(unavailable)?;
  Ok(())
}
async fn by_key(pool: &PgPool, scope: &str, key: &str) -> Result<Option<IntegrationAdministrationReceipt>, StoreError> {
  let row: Option<Json<IntegrationAdministrationReceipt>> = sqlx::query_scalar(
    "SELECT receipt FROM integration_administration_operations WHERE security_scope = $1 AND idempotency_key = $2",
  )
  .bind(scope)
  .bind(key)
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?;
  Ok(row.map(|Json(receipt)| receipt))
}
fn facts(audit: &MutationAuditContext, receipt: &IntegrationAdministrationReceipt) -> MutationFacts {
  MutationFacts::management(
    audit,
    receipt.intent.id.to_string(),
    serde_json::json!({"revision":receipt.revision,"status":receipt.status}),
    serde_json::json!({"integration_id":receipt.intent.id,"operation":receipt.operation,"revision":receipt.revision,"status":receipt.status}),
  )
}
pub(crate) async fn begin(
  pool: &PgPool,
  request: ManagementMutation<BeginIntegrationAdministration>,
) -> Result<IntegrationAdministrationReceipt, StoreError> {
  let (request, audit) = request.into_parts();
  if let Some(receipt) = by_key(pool, audit.security_scope().as_str(), request.idempotency_key.as_str()).await? {
    if receipt.intent != request.intent {
      return Err(conflict());
    }
    return Ok(receipt);
  }
  let identity = MutationIdentity::new_management(
    &audit,
    MutationKind::BeginIntegrationAdministration,
    request.idempotency_key.to_string(),
    request.requested_at,
    EntityKind::Integration,
    &request.intent,
  )?;
  let mut tx = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Replay(_) => {
      return by_key(pool, audit.security_scope().as_str(), request.idempotency_key.as_str())
        .await?
        .ok_or(StoreError::Unavailable);
    }
    MutationStart::Fresh(tx) => tx,
  };
  let metadata = metadata(&mut tx, request.intent.id).await?;
  let mut state = read_state(&mut tx, request.intent.id).await?;
  let old_pending = state.pending.as_ref().map(|p| p.operation);
  let receipt = state.begin(&request, &metadata, audit.security_scope())?;
  if let IntegrationAdministrationAction::Credential { reference, .. } = &request.intent.action {
    sqlx::query(
      "INSERT INTO integration_credential_owners (reference, connection_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(Json(reference))
    .bind(request.intent.id.as_uuid())
    .execute(&mut *tx)
    .await
    .map_err(unavailable)?;
    let owner: uuid::Uuid =
      sqlx::query_scalar("SELECT connection_id FROM integration_credential_owners WHERE reference = $1")
        .bind(Json(reference))
        .fetch_one(&mut *tx)
        .await
        .map_err(unavailable)?;
    if owner != request.intent.id.as_uuid() {
      return Err(conflict());
    }
  }
  if let Some(old) = old_pending {
    sqlx::query("UPDATE integration_administration_operations SET receipt = jsonb_set(receipt, '{status}', '\"abandoned\"'::jsonb) WHERE operation = $1")
      .bind(old.as_slice()).execute(&mut *tx).await.map_err(unavailable)?;
  }
  sqlx::query("INSERT INTO integration_administration_operations (operation, connection_id, security_scope, idempotency_key, requested_at, receipt, metadata_version) VALUES ($1, $2, $3, $4, $5, $6, $7)")
    .bind(receipt.operation.as_slice()).bind(request.intent.id.as_uuid()).bind(audit.security_scope().as_str())
    .bind(request.idempotency_key.as_str()).bind(request.requested_at.unix_millis()).bind(Json(&receipt))
    .bind(receipt.intent.expected_metadata_version.get() as i64).execute(&mut *tx).await.map_err(|e| classify(e, EntityKind::Integration))?;
  write_state(&mut tx, &state).await?;
  crate::mutation::commit(tx, &identity, facts(&audit, &receipt), encode_outcome(&receipt)?).await?;
  Ok(receipt)
}
pub(crate) async fn complete(
  pool: &PgPool,
  request: ManagementMutation<IntegrationAdministrationReceipt>,
) -> Result<IntegrationAdministrationReceipt, StoreError> {
  let (request, audit) = request.into_parts();
  let original: Option<Json<IntegrationAdministrationReceipt>> = sqlx::query_scalar(
    "SELECT receipt FROM integration_administration_operations WHERE operation = $1 AND security_scope = $2",
  )
  .bind(request.operation.as_slice())
  .bind(audit.security_scope().as_str())
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?;
  let Json(mut receipt) = original.ok_or_else(conflict)?;
  if receipt.intent != request.intent || receipt.publication != request.publication {
    return Err(conflict());
  }
  if receipt.status == IntegrationAdministrationStatus::Completed {
    return Ok(receipt);
  }
  let key = octacity_server_factory::FactoryDigest::from_bytes(receipt.operation).to_string();
  let identity = MutationIdentity::new_management(
    &audit,
    MutationKind::CompleteIntegrationAdministration,
    key,
    receipt.requested_at,
    EntityKind::Integration,
    &receipt,
  )?;
  let mut tx = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Replay(value) => return crate::mutation::decode_outcome(value),
    MutationStart::Fresh(tx) => tx,
  };
  let metadata = metadata(&mut tx, receipt.intent.id).await?;
  if metadata.version != receipt.intent.expected_metadata_version {
    return Err(conflict());
  }
  // Re-read under the connection lock: disablement/cleanup may have superseded it.
  let Json(current): Json<IntegrationAdministrationReceipt> =
    sqlx::query_scalar("SELECT receipt FROM integration_administration_operations WHERE operation = $1")
      .bind(receipt.operation.as_slice())
      .fetch_one(&mut *tx)
      .await
      .map_err(unavailable)?;
  receipt = current;
  let mut state = read_state(&mut tx, receipt.intent.id).await?;
  state.complete(&mut receipt, &metadata)?;
  write_state(&mut tx, &state).await?;
  sqlx::query("UPDATE integration_administration_operations SET receipt = $2 WHERE operation = $1")
    .bind(receipt.operation.as_slice())
    .bind(Json(&receipt))
    .execute(&mut *tx)
    .await
    .map_err(unavailable)?;
  crate::mutation::commit(tx, &identity, facts(&audit, &receipt), encode_outcome(&receipt)?).await?;
  Ok(receipt)
}
pub(crate) async fn read(pool: &PgPool, id: IntegrationId) -> Result<IntegrationAdministrationState, StoreError> {
  let row: Option<(Option<i64>, Option<Json<IntegrationAdministrationState>>)> = sqlx::query_as(
    "SELECT a.revision, a.document FROM integration_connections c \
     LEFT JOIN integration_administration a ON a.connection_id = c.id WHERE c.id = $1",
  )
  .bind(id.as_uuid())
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?;
  match row {
    None => Err(conflict()),
    Some((None, None)) => checked_state(id, None),
    Some((Some(revision), Some(document))) => checked_state(id, Some((revision, document))),
    _ => Err(StoreError::Unavailable),
  }
}
pub(crate) async fn cleanup(
  pool: &PgPool,
  request: ManagementMutation<IntegrationPreparationCleanup>,
) -> Result<Vec<CredentialPublication>, StoreError> {
  let (request, audit) = request.into_parts();
  if request.limit == 0 || request.limit > 100 {
    return Err(conflict());
  }
  let mut tx = pool.begin().await.map_err(unavailable)?;
  metadata(&mut tx, request.id).await?;
  let mut state = read_state(&mut tx, request.id).await?;
  let rows: Vec<Json<IntegrationAdministrationReceipt>> = sqlx::query_scalar("SELECT receipt FROM integration_administration_operations \
    WHERE connection_id = $1 AND requested_at <= $2 AND NOT cleaned AND receipt->'publication' <> 'null'::jsonb ORDER BY requested_at, operation LIMIT $3 FOR UPDATE")
    .bind(request.id.as_uuid()).bind(request.older_than.unix_millis()).bind(i64::from(request.limit)).fetch_all(&mut *tx).await.map_err(unavailable)?;
  let mut result = Vec::new();
  let mut abandoned = None;
  for Json(mut receipt) in rows {
    if receipt.status == IntegrationAdministrationStatus::Prepared {
      receipt.status = IntegrationAdministrationStatus::Abandoned;
      state.abandon(receipt.publication.as_ref().ok_or(StoreError::Unavailable)?);
      sqlx::query("UPDATE integration_administration_operations SET receipt = $2 WHERE operation = $1")
        .bind(receipt.operation.as_slice())
        .bind(Json(&receipt))
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?;
      abandoned = Some(receipt.clone());
    }
    result.push(receipt.publication.ok_or(StoreError::Unavailable)?);
  }
  if state.revision > 0 {
    write_state(&mut tx, &state).await?;
  }
  // A connection has at most one prepared operation. Its transition and evidence
  // commit together; retries of already terminal preparations only repeat private cleanup.
  if let Some(receipt) = abandoned {
    let identity = MutationIdentity::new_management(
      &audit,
      MutationKind::AbandonIntegrationPreparation,
      octacity_server_factory::FactoryDigest::from_bytes(receipt.operation).to_string(),
      request.requested_at,
      EntityKind::Integration,
      &receipt,
    )?;
    crate::mutation::commit_fresh(tx, &identity, facts(&audit, &receipt), encode_outcome(&receipt)?).await?;
  } else {
    tx.commit().await.map_err(unavailable)?;
  }
  Ok(result)
}
pub(crate) async fn acknowledge(
  pool: &PgPool,
  request: ManagementMutation<CredentialPublication>,
) -> Result<(), StoreError> {
  let (request, _) = request.into_parts();
  let result = sqlx::query(
    "UPDATE integration_administration_operations SET cleaned = TRUE \
    WHERE operation = $1 AND receipt->>'status' <> 'prepared' AND receipt->'publication' = $2",
  )
  .bind(request.operation.as_slice())
  .bind(Json(request))
  .execute(pool)
  .await
  .map_err(unavailable)?;
  if result.rows_affected() != 1 {
    return Err(conflict());
  }
  Ok(())
}
