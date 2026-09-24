use octacity_server_cache::{
  CacheCredentialDigest, CacheNamespace, CacheNamespacePolicy, CachePermissions, CacheSessionState, ProjectCachePolicy,
};
use octacity_server_domain::{AgentId, BuildId, CacheSessionId, EntityKind, JobId, LeaseId, ProjectId, Timestamp};
use octacity_server_job::JobSpecTemplate;
use octacity_server_store::{
  BeginCacheSession, CacheSessionRecord, ListBuildCacheSessions, RegistrationEpoch, StoreError, StoreInputError,
  StoreOperation,
};
use serde_json::Value;
use sqlx::{FromRow, PgPool, types::Json};
use uuid::Uuid;

use crate::database::unavailable;

pub(crate) async fn read(pool: &PgPool, session_id: CacheSessionId) -> Result<CacheSessionRecord, StoreError> {
  load_one(pool, session_id)
    .await?
    .ok_or(StoreError::NotFound {
      entity: EntityKind::CacheSession,
    })?
    .try_into()
}

pub(crate) async fn list(
  pool: &PgPool,
  request: ListBuildCacheSessions,
) -> Result<Vec<CacheSessionRecord>, StoreError> {
  if request.limit == 0 || request.limit > octacity_server_store::MAX_CACHE_SESSION_PAGE_SIZE {
    return Err(StoreError::InvalidInput {
      operation: StoreOperation::ListCacheSessions,
      source: StoreInputError::InvalidCacheSession,
    });
  }
  let rows: Vec<CacheSessionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
    "{} WHERE session.build_id = $1 ORDER BY session.created_at, session.id LIMIT $2",
    SELECT_SESSION
  )))
  .bind(request.build_id.as_uuid())
  .bind(i64::from(request.limit))
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;
  rows.into_iter().map(TryInto::try_into).collect()
}

async fn load_one(pool: &PgPool, session_id: CacheSessionId) -> Result<Option<CacheSessionRow>, StoreError> {
  sqlx::query_as(sqlx::AssertSqlSafe(format!("{} WHERE session.id = $1", SELECT_SESSION)))
    .bind(session_id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(unavailable)
}

pub(super) async fn load_by_request(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  lease_id: LeaseId,
  request_identity: &str,
) -> Result<Option<CacheSessionRow>, StoreError> {
  sqlx::query_as(sqlx::AssertSqlSafe(format!(
    "{} WHERE session.lease_id = $1 AND session.request_identity = $2 FOR UPDATE OF session",
    SELECT_SESSION
  )))
  .bind(lease_id.as_uuid())
  .bind(request_identity)
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)
}

pub(super) async fn load_policy(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  job_id: JobId,
  build_id: Uuid,
  request: &BeginCacheSession,
) -> Result<CacheNamespacePolicy, StoreError> {
  let row: (Json<JobSpecTemplate>, Json<Value>) = sqlx::query_as(
    "SELECT job.job_spec_template, build.effective_policy_snapshot
     FROM jobs AS job JOIN attempts AS attempt ON attempt.id = job.attempt_id
     JOIN builds AS build ON build.id = attempt.build_id
     WHERE job.id = $1 AND build.id = $2",
  )
  .bind(job_id.as_uuid())
  .bind(build_id)
  .fetch_one(&mut **transaction)
  .await
  .map_err(unavailable)?;
  let signed = row.0.cache_policy().ok_or_else(invalid_cache)?;
  let project: ProjectCachePolicy = serde_json::from_value(
    row
      .1
      .pointer("/project/policy/cache")
      .cloned()
      .ok_or_else(invalid_cache)?,
  )
  .map_err(|_| invalid_cache())?;
  let retention_seconds = row
    .1
    .pointer("/project/policy/retention/cache_seconds")
    .and_then(Value::as_u64)
    .ok_or_else(invalid_cache)?;
  CacheNamespacePolicy::select(
    &project,
    retention_seconds,
    CacheNamespace::new(request.requested.namespace.clone()).map_err(|_| invalid_cache())?,
    CachePermissions::new(request.requested.read, request.requested.write).map_err(|_| invalid_cache())?,
    signed,
  )
  .map_err(|_| invalid_cache())
}

const SELECT_SESSION: &str =
  "SELECT session.id, session.project_id, session.build_id, session.job_id, session.lease_id,
          session.namespace, session.permissions, session.quota_bytes, session.state,
          session.scope_id, session.agent_id, session.registration_epoch,
          FLOOR(EXTRACT(EPOCH FROM session.created_at) * 1000)::BIGINT AS created_at_millis,
          FLOOR(EXTRACT(EPOCH FROM session.expires_at) * 1000)::BIGINT AS expires_at_millis,
          FLOOR(EXTRACT(EPOCH FROM session.retention_until) * 1000)::BIGINT AS retention_until_millis,
          FLOOR(EXTRACT(EPOCH FROM session.revoked_at) * 1000)::BIGINT AS revoked_at_millis,
          (lease.state IN ('active', 'cancellation_requested', 'drain_requested')
            AND lease.expires_at > now()
            AND registration.revoked_at IS NULL
            AND registration.expires_at > now()) AS lease_current
   FROM cache_sessions AS session
   JOIN leases AS lease ON lease.id = session.lease_id AND lease.registration_id = session.registration_id
   JOIN agent_registrations AS registration ON registration.id = session.registration_id";

#[derive(FromRow)]
pub(super) struct CacheSessionRow {
  pub(super) id: Uuid,
  pub(super) project_id: Uuid,
  pub(super) build_id: Uuid,
  pub(super) job_id: Uuid,
  pub(super) lease_id: Uuid,
  pub(super) namespace: String,
  pub(super) permissions: Json<CachePermissions>,
  pub(super) quota_bytes: i64,
  pub(super) state: String,
  pub(super) scope_id: String,
  pub(super) agent_id: Uuid,
  pub(super) registration_epoch: i64,
  pub(super) created_at_millis: i64,
  pub(super) expires_at_millis: i64,
  pub(super) retention_until_millis: i64,
  pub(super) revoked_at_millis: Option<i64>,
  pub(super) lease_current: bool,
}

impl TryFrom<CacheSessionRow> for CacheSessionRecord {
  type Error = StoreError;

  fn try_from(row: CacheSessionRow) -> Result<Self, Self::Error> {
    let created_at = timestamp(row.created_at_millis)?;
    let retention_until = timestamp(row.retention_until_millis)?;
    Ok(Self {
      id: id(row.id)?,
      scope_id: row.scope_id,
      project_id: id(row.project_id)?,
      build_id: id(row.build_id)?,
      job_id: id(row.job_id)?,
      agent_id: id(row.agent_id)?,
      registration_epoch: positive_epoch(row.registration_epoch)?,
      lease_id: id(row.lease_id)?,
      policy: CacheNamespacePolicy {
        namespace: CacheNamespace::new(row.namespace).map_err(|_| StoreError::Unavailable)?,
        permissions: row.permissions.0,
        quota_bytes: unsigned(row.quota_bytes)?,
        retention_seconds: retention_seconds(created_at, row.retention_until_millis)?,
      },
      state: parse_state(&row.state)?,
      created_at,
      expires_at: timestamp(row.expires_at_millis)?,
      retention_until,
      revoked_at: row.revoked_at_millis.map(timestamp).transpose()?,
      lease_current: row.lease_current,
    })
  }
}

#[derive(FromRow)]
pub(super) struct AuthorizationRow {
  pub(super) project_id: Uuid,
  pub(super) namespace: String,
  pub(super) permissions: Json<CachePermissions>,
  pub(super) credential_hash: Vec<u8>,
  pub(super) quota_bytes: i64,
  pub(super) state: String,
  pub(super) created_at_millis: i64,
  pub(super) expires_at_millis: i64,
  pub(super) retention_until_millis: i64,
  pub(super) lease_state: String,
  pub(super) lease_expires_at_millis: i64,
  pub(super) registration_revoked: bool,
  pub(super) registration_expires_at_millis: i64,
}

#[derive(FromRow)]
pub(super) struct DataAuthorizationRow {
  pub(super) project_id: Uuid,
  pub(super) scope_id: String,
  pub(super) permissions: Json<CachePermissions>,
  pub(super) credential_hash: Vec<u8>,
  pub(super) quota_bytes: i64,
  pub(super) state: String,
  pub(super) session_expires_at_millis: i64,
  pub(super) retention_until_millis: i64,
  pub(super) lease_state: String,
  pub(super) lease_expires_at_millis: i64,
  pub(super) registration_revoked: bool,
  pub(super) registration_expires_at_millis: i64,
  pub(super) namespace: String,
}

pub(super) fn parse_state(value: &str) -> Result<CacheSessionState, StoreError> {
  match value {
    "active" => Ok(CacheSessionState::Active),
    "revoked" => Ok(CacheSessionState::Revoked),
    "expired" => Ok(CacheSessionState::Expired),
    _ => Err(StoreError::Unavailable),
  }
}

pub(super) fn current_lease_state(value: &str) -> bool {
  matches!(value, "active" | "cancellation_requested" | "drain_requested")
}

pub(super) fn digest(value: &[u8]) -> Result<CacheCredentialDigest, StoreError> {
  value
    .try_into()
    .map(CacheCredentialDigest::from_bytes)
    .map_err(|_| StoreError::Unavailable)
}

pub(super) fn id<T>(value: Uuid) -> Result<T, StoreError>
where
  T: TryFromUuid,
{
  T::try_from_uuid(value)
}

pub(super) trait TryFromUuid: Sized {
  fn try_from_uuid(value: Uuid) -> Result<Self, StoreError>;
}

macro_rules! uuid_id {
  ($($type:ty),+ $(,)?) => {$ (
    impl TryFromUuid for $type {
      fn try_from_uuid(value: Uuid) -> Result<Self, StoreError> {
        <$type>::from_uuid(value).map_err(|_| StoreError::Unavailable)
      }
    }
  )+ };
}

uuid_id!(ProjectId, BuildId, JobId, AgentId, LeaseId, CacheSessionId);

pub(super) fn positive_attempt(value: i64) -> Result<octacity_server_domain::AttemptNumber, StoreError> {
  octacity_server_domain::AttemptNumber::new(unsigned(value)?).map_err(|_| StoreError::Unavailable)
}

fn positive_epoch(value: i64) -> Result<RegistrationEpoch, StoreError> {
  RegistrationEpoch::new(unsigned(value)?).map_err(|_| StoreError::Unavailable)
}

pub(super) fn timestamp(value: i64) -> Result<Timestamp, StoreError> {
  Timestamp::from_unix_millis(value).map_err(|_| StoreError::Unavailable)
}

pub(super) fn number(value: u64) -> Result<i64, StoreError> {
  i64::try_from(value).map_err(|_| invalid_cache())
}

pub(super) fn unsigned(value: i64) -> Result<u64, StoreError> {
  u64::try_from(value).map_err(|_| StoreError::Unavailable)
}

pub(super) fn add_seconds(value: Timestamp, seconds: u64) -> Result<Timestamp, StoreError> {
  let milliseconds = number(seconds)?
    .checked_mul(1_000)
    .and_then(|duration| value.unix_millis().checked_add(duration))
    .ok_or_else(invalid_cache)?;
  Timestamp::from_unix_millis(milliseconds).map_err(|_| invalid_cache())
}

pub(super) fn retention_seconds(start: Timestamp, retention_until_millis: i64) -> Result<u64, StoreError> {
  let difference = retention_until_millis
    .checked_sub(start.unix_millis())
    .ok_or(StoreError::Unavailable)?;
  if difference <= 0 || difference % 1_000 != 0 {
    return Err(StoreError::Unavailable);
  }
  unsigned(difference / 1_000)
}

pub(super) const fn invalid_cache() -> StoreError {
  StoreError::InvalidInput {
    operation: StoreOperation::BeginCacheSession,
    source: StoreInputError::InvalidCacheSession,
  }
}
