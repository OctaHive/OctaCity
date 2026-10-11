use std::str::FromStr;

use octacity_server_domain::EntityKind;
use octacity_server_factory::FactoryDigest;
use octacity_server_store::{
  FactoryRunDiagnosticKind, FactoryRunDiagnosticPage, FactoryRunDiagnosticRecord, ListFactoryRunDiagnostics,
  StoreError, StoreInputError, StoreOperation,
};
use serde::de::DeserializeOwned;
use sqlx::{PgPool, types::Json};

use crate::database::unavailable;

pub(crate) async fn list_diagnostics(
  pool: &PgPool,
  request: ListFactoryRunDiagnostics,
) -> Result<FactoryRunDiagnosticPage, StoreError> {
  let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM factory_runs WHERE id = $1 AND visible)")
    .bind(request.run_id.as_uuid())
    .fetch_one(pool)
    .await
    .map_err(unavailable)?;
  if !exists {
    return Err(StoreError::NotFound {
      entity: EntityKind::FactoryRun,
    });
  }
  let limit = i64::from(request.limit.get()) + 1;
  let records = match request.kind {
    FactoryRunDiagnosticKind::Claim => {
      let after = digest_cursor(&request)?;
      sqlx::query_as::<_, (Vec<u8>, uuid::Uuid, String, Vec<u8>, i64, i64)>(
        "SELECT id, run_id, owner, fence, FLOOR(EXTRACT(EPOCH FROM claimed_at)*1000)::BIGINT, \
          FLOOR(EXTRACT(EPOCH FROM expires_at)*1000)::BIGINT FROM factory_run_claims \
          WHERE run_id=$1 AND ($2::BYTEA IS NULL OR id>$2) ORDER BY id LIMIT $3"
      ).bind(request.run_id.as_uuid()).bind(after).bind(limit).fetch_all(pool).await.map_err(unavailable)?
        .into_iter().map(|(id, run, owner, fence, claimed, expires)| {
          let claim = super::decode_claim((run, owner, fence, claimed, expires))?;
          if claim.id.as_bytes().as_slice() != id { return Err(StoreError::Unavailable); }
          Ok(FactoryRunDiagnosticRecord::Claim(Box::new(claim)))
        }).collect::<Result<Vec<_>, _>>()?
    }
    FactoryRunDiagnosticKind::Audit => {
      let after = digest_cursor(&request)?;
      sqlx::query_as::<_, super::FactoryAuditRow>(
        "SELECT id,run_id,actor_kind,actor_identity_digest,operation,request_identity_digest,outcome, \
          FLOOR(EXTRACT(EPOCH FROM recorded_at)*1000)::BIGINT AS recorded_at_millis FROM factory_audit_links \
          WHERE run_id=$1 AND ($2::BYTEA IS NULL OR id>$2) ORDER BY id LIMIT $3"
      ).bind(request.run_id.as_uuid()).bind(after).bind(limit).fetch_all(pool).await.map_err(unavailable)?
        .into_iter().map(|row| super::decode_audit_row(row).map(|row| FactoryRunDiagnosticRecord::Audit(Box::new(row))))
        .collect::<Result<Vec<_>, _>>()?
    }
    FactoryRunDiagnosticKind::NodeAttempt => uuid_rows(pool, "SELECT node_attempt FROM factory_node_attempts WHERE run_id = $1 AND ($2::UUID IS NULL OR id > $2) ORDER BY id LIMIT $3", &request, limit, |value| FactoryRunDiagnosticRecord::NodeAttempt(Box::new(value))).await?,
    FactoryRunDiagnosticKind::NodeAttemptCompletion => digest_rows(pool, "SELECT completion FROM factory_node_attempt_completions WHERE run_id = $1 AND ($2::BYTEA IS NULL OR id > $2) ORDER BY id LIMIT $3", &request, limit, |value| FactoryRunDiagnosticRecord::NodeAttemptCompletion(Box::new(value))).await?,

  };
  page(records, usize::from(request.limit.get()))
}

async fn uuid_rows<T>(
  pool: &PgPool,
  query: &'static str,
  request: &ListFactoryRunDiagnostics,
  limit: i64,
  wrap: fn(T) -> FactoryRunDiagnosticRecord,
) -> Result<Vec<FactoryRunDiagnosticRecord>, StoreError>
where
  T: DeserializeOwned + Send + Unpin + 'static,
{
  let after = request
    .after
    .as_ref()
    .map(|value| uuid::Uuid::from_str(value.as_str()))
    .transpose()
    .map_err(|_| invalid_cursor())?;
  sqlx::query_scalar::<_, Json<T>>(query)
    .bind(request.run_id.as_uuid())
    .bind(after)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(unavailable)
    .map(|rows| rows.into_iter().map(|row| wrap(row.0)).collect())
}

async fn digest_rows<T>(
  pool: &PgPool,
  query: &'static str,
  request: &ListFactoryRunDiagnostics,
  limit: i64,
  wrap: fn(T) -> FactoryRunDiagnosticRecord,
) -> Result<Vec<FactoryRunDiagnosticRecord>, StoreError>
where
  T: DeserializeOwned + Send + Unpin + 'static,
{
  let after = digest_cursor(request)?;
  sqlx::query_scalar::<_, Json<T>>(query)
    .bind(request.run_id.as_uuid())
    .bind(after)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(unavailable)
    .map(|rows| rows.into_iter().map(|row| wrap(row.0)).collect())
}

fn digest_cursor(request: &ListFactoryRunDiagnostics) -> Result<Option<Vec<u8>>, StoreError> {
  request
    .after
    .as_ref()
    .map(|value| {
      FactoryDigest::from_lower_hex(value.as_str())
        .map(|digest| digest.as_bytes().to_vec())
        .map_err(|_| invalid_cursor())
    })
    .transpose()
}

fn page(mut records: Vec<FactoryRunDiagnosticRecord>, limit: usize) -> Result<FactoryRunDiagnosticPage, StoreError> {
  let has_next = records.len() > limit;
  records.truncate(limit);
  let next = has_next.then(|| records.last().expect("positive diagnostic page limit").cursor());
  if records.windows(2).any(|pair| pair[0].cursor() >= pair[1].cursor()) {
    return Err(StoreError::Unavailable);
  }
  Ok(FactoryRunDiagnosticPage { items: records, next })
}

fn invalid_cursor() -> StoreError {
  StoreError::InvalidInput {
    operation: StoreOperation::ListFactoryRunDiagnostics,
    source: StoreInputError::InvalidFactoryRunDiagnosticCursor,
  }
}
