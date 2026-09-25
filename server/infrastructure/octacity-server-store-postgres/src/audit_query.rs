use std::str::FromStr as _;

use octacity_server_domain::{AuditFactId, Timestamp};
use octacity_server_store::{
  AuditActor, AuditActorKind, AuditCursor, AuditFact, AuditFactPage, AuditFactQuery, AuditMetadata, AuditOutcome,
  StoreError, StoreInputError, StoreOperation,
};
use serde_json::Value;
use sqlx::{FromRow, PgPool, types::Json};

use crate::database::unavailable;

#[derive(FromRow)]
struct AuditFactRow {
  id: uuid::Uuid,
  actor_kind: String,
  actor_identity: Option<String>,
  operation: String,
  target_kind: String,
  target_identity: String,
  request_identity: Option<String>,
  idempotency_key: Option<String>,
  outcome: String,
  safe_metadata: Json<Value>,
  occurred_at_unix_ms: i64,
}

pub(crate) async fn list(pool: &PgPool, query: AuditFactQuery) -> Result<AuditFactPage, StoreError> {
  query.validate().map_err(|_| StoreError::InvalidInput {
    operation: StoreOperation::ListAuditFacts,
    source: StoreInputError::InvalidAuditQuery,
  })?;
  let after_time = query.after.map(|cursor| cursor.occurred_at.unix_millis());
  let after_id = query.after.map(|cursor| cursor.id.as_uuid());
  let mut rows = sqlx::query_as::<_, AuditFactRow>(
    "SELECT id, actor_kind, actor_identity, operation, target_kind, target_identity, request_identity, \
       idempotency_key, outcome, safe_metadata, \
       FLOOR(EXTRACT(EPOCH FROM occurred_at) * 1000)::BIGINT AS occurred_at_unix_ms \
     FROM audit_facts \
     WHERE ($1::text IS NULL OR actor_kind = $1) \
       AND ($2::text IS NULL OR actor_identity = $2) \
       AND ($3::text IS NULL OR operation = $3) \
       AND ($4::text IS NULL OR target_kind = $4) \
       AND ($5::text IS NULL OR target_identity = $5) \
       AND ($6::text IS NULL OR request_identity = $6) \
       AND ($7::bigint IS NULL OR occurred_at >= to_timestamp($7::double precision / 1000.0)) \
       AND ($8::bigint IS NULL OR occurred_at <= to_timestamp($8::double precision / 1000.0)) \
       AND ($9::bigint IS NULL OR (occurred_at, id) < \
         (to_timestamp($9::double precision / 1000.0), $10::uuid)) \
     ORDER BY occurred_at DESC, id DESC LIMIT $11",
  )
  .bind(query.actor_kind.map(AuditActorKind::as_str))
  .bind(query.actor_identity.as_deref())
  .bind(query.operation.as_deref())
  .bind(query.target_kind.as_deref())
  .bind(query.target_identity.as_deref())
  .bind(query.request_identity.as_deref())
  .bind(query.occurred_from.map(Timestamp::unix_millis))
  .bind(query.occurred_through.map(Timestamp::unix_millis))
  .bind(after_time)
  .bind(after_id)
  .bind(i64::from(query.limit) + 1)
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;

  let has_more = rows.len() > usize::from(query.limit);
  if has_more {
    rows.pop();
  }
  let items = rows
    .into_iter()
    .map(AuditFactRow::into_fact)
    .collect::<Result<Vec<_>, _>>()?;
  let next_cursor = has_more.then(|| items.last()).flatten().map(|fact| AuditCursor {
    occurred_at: fact.occurred_at,
    id: fact.id,
  });
  Ok(AuditFactPage { items, next_cursor })
}

impl AuditFactRow {
  fn into_fact(self) -> Result<AuditFact, StoreError> {
    Ok(AuditFact {
      id: AuditFactId::from_uuid(self.id).map_err(|_| StoreError::Unavailable)?,
      actor: AuditActor {
        kind: AuditActorKind::from_str(&self.actor_kind).map_err(|_| StoreError::Unavailable)?,
        identity: self.actor_identity,
      },
      operation: self.operation,
      target_kind: self.target_kind,
      target_identity: self.target_identity,
      request_identity: self.request_identity,
      idempotency_key: self.idempotency_key,
      outcome: AuditOutcome::from_str(&self.outcome).map_err(|_| StoreError::Unavailable)?,
      metadata: AuditMetadata::try_new(self.safe_metadata.0).map_err(|_| StoreError::Unavailable)?,
      occurred_at: Timestamp::from_unix_millis(self.occurred_at_unix_ms).map_err(|_| StoreError::Unavailable)?,
    })
  }
}
