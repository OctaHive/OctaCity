use octacity_server_cache::{
  CacheCredentialDigest, CacheNamespace, CacheNamespacePolicy, CachePermissions, CacheSessionEvent, CacheSessionState,
  ProjectCachePolicy,
};
use octacity_server_domain::{AgentId, BuildId, CacheSessionId, EntityKind, JobId, LeaseId, ProjectId, Timestamp};
use octacity_server_job::JobSpecTemplate;
use octacity_server_store::{
  AuthorizeCacheSession, BeginCacheSession, BeginCacheSessionOutcome, CacheAuthorization, CacheAuthorizationOutcome,
  CacheSessionRecord, ListBuildCacheSessions, MutationDisposition, RegistrationEpoch, RevokeCacheSession, StoreError,
  StoreInputError, StoreOperation, cache_scope_id,
};
use serde_json::Value;
use sqlx::{FromRow, PgPool, types::Json};
use uuid::Uuid;

use crate::{database::unavailable, lease};

pub(crate) async fn begin(pool: &PgPool, request: BeginCacheSession) -> Result<BeginCacheSessionOutcome, StoreError> {
  request.validate()?;
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let lease_row = lease::load(&mut transaction, request.lease, StoreOperation::BeginCacheSession).await?;
  lease::require_current(&lease_row, request.lease, request.created_at)?;
  if lease_row.job_id != request.job_id.as_uuid() || positive_attempt(lease_row.attempt_number)? != request.attempt {
    return Err(StoreError::Fenced {
      lease: request.lease.lease_id,
    });
  }

  let policy = load_policy(&mut transaction, request.job_id, lease_row.build_id, &request).await?;
  if let Some(existing) = load_by_request(
    &mut transaction,
    request.lease.lease_id,
    request.idempotency_key.as_str(),
  )
  .await?
  {
    if existing.namespace != policy.namespace.as_str()
      || existing.permissions.0 != policy.permissions
      || existing.job_id != request.job_id.as_uuid()
    {
      return Err(StoreError::Conflict {
        entity: EntityKind::CacheSession,
      });
    }
    transaction.commit().await.map_err(unavailable)?;
    return Ok(BeginCacheSessionOutcome {
      session: existing.try_into()?,
      disposition: MutationDisposition::Replayed,
    });
  }

  let retention_until = add_seconds(request.created_at, policy.retention_seconds)?;
  let expires_at = Timestamp::from_unix_millis(request.expires_at.unix_millis().min(lease_row.expires_at_millis))
    .map_err(|_| invalid_cache())?;
  let scope_id = cache_scope_id(id::<ProjectId>(lease_row.project_id)?, &policy.namespace);
  let permissions = Json(policy.permissions);
  let quota_bytes = number(policy.quota_bytes)?;
  let epoch = number(request.lease.registration_epoch.get())?;
  let row: CacheSessionRow = sqlx::query_as(
    "INSERT INTO cache_sessions (
       id, project_id, build_id, job_id, registration_id, lease_id, namespace, permissions,
       credential_hash, quota_bytes, state, created_at, expires_at, revoked_at,
       request_identity, scope_id, agent_id, registration_epoch, retention_until
     ) VALUES (
       $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'active',
       to_timestamp($11::double precision / 1000.0), to_timestamp($12::double precision / 1000.0), NULL,
       $13, $14, $15, $16, to_timestamp($17::double precision / 1000.0)
     )
     RETURNING id, project_id, build_id, job_id, lease_id, namespace, permissions, quota_bytes, state,
       scope_id, agent_id, registration_epoch,
       FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_millis,
       FLOOR(EXTRACT(EPOCH FROM expires_at) * 1000)::BIGINT AS expires_at_millis,
       FLOOR(EXTRACT(EPOCH FROM retention_until) * 1000)::BIGINT AS retention_until_millis,
       NULL::BIGINT AS revoked_at_millis,
       TRUE AS lease_current",
  )
  .bind(request.session_id.as_uuid())
  .bind(lease_row.project_id)
  .bind(lease_row.build_id)
  .bind(request.job_id.as_uuid())
  .bind(lease_row.registration_id)
  .bind(request.lease.lease_id.as_uuid())
  .bind(policy.namespace.as_str())
  .bind(permissions)
  .bind(request.credential_digest.as_bytes().to_vec())
  .bind(quota_bytes)
  .bind(request.created_at.unix_millis())
  .bind(expires_at.unix_millis())
  .bind(request.idempotency_key.as_str())
  .bind(scope_id)
  .bind(lease_row.agent_id)
  .bind(epoch)
  .bind(retention_until.unix_millis())
  .fetch_one(&mut *transaction)
  .await
  .map_err(unavailable)?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(BeginCacheSessionOutcome {
    session: row.try_into()?,
    disposition: MutationDisposition::Applied,
  })
}

pub(crate) async fn revoke(pool: &PgPool, request: RevokeCacheSession) -> Result<MutationDisposition, StoreError> {
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let lease_row = lease::load(&mut transaction, request.lease, StoreOperation::RevokeCacheSession).await?;
  lease::require_current(&lease_row, request.lease, request.revoked_at)?;
  let row: (Uuid, String, Option<i64>) = sqlx::query_as(
    "SELECT job_id, state, FLOOR(EXTRACT(EPOCH FROM revoked_at) * 1000)::BIGINT
     FROM cache_sessions WHERE id = $1 AND lease_id = $2 FOR UPDATE",
  )
  .bind(request.session_id.as_uuid())
  .bind(request.lease.lease_id.as_uuid())
  .fetch_optional(&mut *transaction)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::CacheSession,
  })?;
  if row.0 != request.job_id.as_uuid() {
    return Err(StoreError::Fenced {
      lease: request.lease.lease_id,
    });
  }
  let state = parse_state(&row.1)?;
  if state == CacheSessionState::Revoked {
    transaction.commit().await.map_err(unavailable)?;
    return Ok(MutationDisposition::Replayed);
  }
  state
    .transition(CacheSessionEvent::Revoke)
    .map_err(|_| StoreError::Conflict {
      entity: EntityKind::CacheSession,
    })?;
  sqlx::query(
    "UPDATE cache_sessions SET state = 'revoked', revoked_at = to_timestamp($2::double precision / 1000.0)
     WHERE id = $1",
  )
  .bind(request.session_id.as_uuid())
  .bind(request.revoked_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(MutationDisposition::Applied)
}

pub(crate) async fn authorize(
  pool: &PgPool,
  request: AuthorizeCacheSession,
) -> Result<CacheAuthorizationOutcome, StoreError> {
  let row: Option<AuthorizationRow> = sqlx::query_as(
    "SELECT session.project_id, session.namespace, session.permissions, session.credential_hash,
            session.quota_bytes, session.state,
            FLOOR(EXTRACT(EPOCH FROM session.created_at) * 1000)::BIGINT AS created_at_millis,
            FLOOR(EXTRACT(EPOCH FROM session.expires_at) * 1000)::BIGINT AS expires_at_millis,
            FLOOR(EXTRACT(EPOCH FROM session.retention_until) * 1000)::BIGINT AS retention_until_millis,
            lease.state AS lease_state,
            FLOOR(EXTRACT(EPOCH FROM lease.expires_at) * 1000)::BIGINT AS lease_expires_at_millis,
            registration.revoked_at IS NOT NULL AS registration_revoked,
            FLOOR(EXTRACT(EPOCH FROM registration.expires_at) * 1000)::BIGINT AS registration_expires_at_millis
     FROM cache_sessions AS session
     JOIN leases AS lease ON lease.id = session.lease_id AND lease.registration_id = session.registration_id
     JOIN agent_registrations AS registration ON registration.id = session.registration_id
     WHERE session.id = $1",
  )
  .bind(request.session_id.as_uuid())
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?;
  let Some(row) = row else {
    return Ok(CacheAuthorizationOutcome::Rejected);
  };
  let digest = digest(&row.credential_hash)?;
  let permissions = row.permissions.0;
  let now = request.observed_at.unix_millis();
  let allowed = row.state == "active"
    && row.namespace == request.namespace.as_str()
    && permissions.allows(request.operation)
    && digest.matches(request.credential_digest)
    && now < row.expires_at_millis
    && current_lease_state(&row.lease_state)
    && now < row.lease_expires_at_millis
    && !row.registration_revoked
    && now < row.registration_expires_at_millis;
  if !allowed {
    return Ok(CacheAuthorizationOutcome::Rejected);
  }
  Ok(CacheAuthorizationOutcome::Authorized(CacheAuthorization {
    project_id: id(row.project_id)?,
    policy: CacheNamespacePolicy {
      namespace: request.namespace,
      permissions,
      quota_bytes: unsigned(row.quota_bytes)?,
      retention_seconds: retention_seconds(timestamp(row.created_at_millis)?, row.retention_until_millis)?,
    },
    retention_until: timestamp(row.retention_until_millis)?,
  }))
}

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

async fn load_by_request(
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

async fn load_policy(
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
struct CacheSessionRow {
  id: Uuid,
  project_id: Uuid,
  build_id: Uuid,
  job_id: Uuid,
  lease_id: Uuid,
  namespace: String,
  permissions: Json<CachePermissions>,
  quota_bytes: i64,
  state: String,
  scope_id: String,
  agent_id: Uuid,
  registration_epoch: i64,
  created_at_millis: i64,
  expires_at_millis: i64,
  retention_until_millis: i64,
  revoked_at_millis: Option<i64>,
  lease_current: bool,
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
struct AuthorizationRow {
  project_id: Uuid,
  namespace: String,
  permissions: Json<CachePermissions>,
  credential_hash: Vec<u8>,
  quota_bytes: i64,
  state: String,
  created_at_millis: i64,
  expires_at_millis: i64,
  retention_until_millis: i64,
  lease_state: String,
  lease_expires_at_millis: i64,
  registration_revoked: bool,
  registration_expires_at_millis: i64,
}

fn parse_state(value: &str) -> Result<CacheSessionState, StoreError> {
  match value {
    "active" => Ok(CacheSessionState::Active),
    "revoked" => Ok(CacheSessionState::Revoked),
    "expired" => Ok(CacheSessionState::Expired),
    _ => Err(StoreError::Unavailable),
  }
}

fn current_lease_state(value: &str) -> bool {
  matches!(value, "active" | "cancellation_requested" | "drain_requested")
}

fn digest(value: &[u8]) -> Result<CacheCredentialDigest, StoreError> {
  value
    .try_into()
    .map(CacheCredentialDigest::from_bytes)
    .map_err(|_| StoreError::Unavailable)
}

fn id<T>(value: Uuid) -> Result<T, StoreError>
where
  T: TryFromUuid,
{
  T::try_from_uuid(value)
}

trait TryFromUuid: Sized {
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

fn positive_attempt(value: i64) -> Result<octacity_server_domain::AttemptNumber, StoreError> {
  octacity_server_domain::AttemptNumber::new(unsigned(value)?).map_err(|_| StoreError::Unavailable)
}

fn positive_epoch(value: i64) -> Result<RegistrationEpoch, StoreError> {
  RegistrationEpoch::new(unsigned(value)?).map_err(|_| StoreError::Unavailable)
}

fn timestamp(value: i64) -> Result<Timestamp, StoreError> {
  Timestamp::from_unix_millis(value).map_err(|_| StoreError::Unavailable)
}

fn number(value: u64) -> Result<i64, StoreError> {
  i64::try_from(value).map_err(|_| invalid_cache())
}

fn unsigned(value: i64) -> Result<u64, StoreError> {
  u64::try_from(value).map_err(|_| StoreError::Unavailable)
}

fn add_seconds(value: Timestamp, seconds: u64) -> Result<Timestamp, StoreError> {
  let milliseconds = number(seconds)?
    .checked_mul(1_000)
    .and_then(|duration| value.unix_millis().checked_add(duration))
    .ok_or_else(invalid_cache)?;
  Timestamp::from_unix_millis(milliseconds).map_err(|_| invalid_cache())
}

fn retention_seconds(start: Timestamp, retention_until_millis: i64) -> Result<u64, StoreError> {
  let difference = retention_until_millis
    .checked_sub(start.unix_millis())
    .ok_or(StoreError::Unavailable)?;
  if difference <= 0 || difference % 1_000 != 0 {
    return Err(StoreError::Unavailable);
  }
  unsigned(difference / 1_000)
}

const fn invalid_cache() -> StoreError {
  StoreError::InvalidInput {
    operation: StoreOperation::BeginCacheSession,
    source: StoreInputError::InvalidCacheSession,
  }
}
