use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use octacity_protocol::{
  AgentCredentialToken, BeginCacheSessionRequest, BeginCacheSessionResponse, COORDINATOR_PROTOCOL_VERSION,
  RemoteCacheGrant, RevokeCacheSessionRequest, RevokeCacheSessionResponse,
};
use octacity_server_cache::{CacheCredentialKey, CacheSessionState};
use octacity_server_domain::{BuildId, CacheSessionId, EntityKind, Timestamp};
use octacity_server_store::{
  BeginCacheSession, CacheSessionRecord, CacheSessionStore, IdempotencyKey, ListBuildCacheSessions, RevokeCacheSession,
  StoreError,
};
use serde::Serialize;
use thiserror::Error;
use url::Url;

use crate::{
  AgentOperation, AgentRegistrationError, AgentRegistrationUseCases, ApplicationError, Query, QueryHandler, agent_lease,
};

/// Complete transport-independent input for one Agent cache-session begin.
#[derive(Clone, Debug)]
pub struct BeginAgentCacheSessionInput {
  /// Lease identity selected by the route.
  pub route_lease_id: String,
  /// Shared coordinator protocol request.
  pub request: BeginCacheSessionRequest,
  /// Current registration bearer.
  pub credential: AgentCredentialToken,
  /// Server-observed request arrival time.
  pub observed_at_unix_ms: i64,
}

/// Complete transport-independent input for one Agent cache-session revocation.
#[derive(Clone, Debug)]
pub struct RevokeAgentCacheSessionInput {
  /// Lease identity selected by the route.
  pub route_lease_id: String,
  /// Shared coordinator protocol request.
  pub request: RevokeCacheSessionRequest,
  /// Current registration bearer.
  pub credential: AgentCredentialToken,
  /// Server-observed request arrival time.
  pub observed_at_unix_ms: i64,
}

/// Application boundary used by fenced Agent cache-session routes.
#[async_trait]
pub trait AgentCacheSessionUseCases: Send + Sync {
  /// Issues one replay-safe short-lived namespace credential.
  async fn begin_session(
    &self,
    input: BeginAgentCacheSessionInput,
  ) -> Result<BeginCacheSessionResponse, AgentCacheSessionError>;

  /// Revokes one session idempotently after runner shutdown.
  async fn revoke_session(
    &self,
    input: RevokeAgentCacheSessionInput,
  ) -> Result<RevokeCacheSessionResponse, AgentCacheSessionError>;
}

/// Effective diagnostic state without any credential material.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheSessionDiagnosticState {
  /// Session, registration, Lease, and expiry are current.
  Active,
  /// The Agent explicitly revoked the session.
  Revoked,
  /// The session credential reached its exclusive expiry.
  Expired,
  /// The bound Lease or registration is no longer current.
  Fenced,
}

/// Secret-free management projection of one cache session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CacheSessionProjection {
  /// Opaque session identity.
  pub id: CacheSessionId,
  /// Owning Project.
  pub project_id: octacity_server_domain::ProjectId,
  /// Build whose immutable policy authorized the session.
  pub build_id: BuildId,
  /// Leased Job.
  pub job_id: octacity_server_domain::JobId,
  /// Owning Agent.
  pub agent_id: octacity_server_domain::AgentId,
  /// Bound positive registration epoch.
  pub registration_epoch: u64,
  /// Bound Lease identity, without its fence.
  pub lease_id: octacity_server_domain::LeaseId,
  /// Single logical namespace.
  pub namespace: String,
  /// Whether lookup is authorized.
  pub read: bool,
  /// Whether publication is authorized.
  pub write: bool,
  /// Project quota applied to this namespace authority.
  pub quota_bytes: u64,
  /// Authoritative creation time.
  pub created_at: Timestamp,
  /// Exclusive session expiry.
  pub expires_at: Timestamp,
  /// Latest retention deadline for new entries.
  pub retention_until: Timestamp,
  /// Effective diagnostic state.
  pub state: CacheSessionDiagnosticState,
  /// Explicit revocation time, when present.
  pub revoked_at: Option<Timestamp>,
}

/// Typed management query for one cache-session diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetCacheSessionQuery {
  /// Opaque session identity.
  pub session_id: CacheSessionId,
  /// Server-observed query time used to project expiry.
  pub observed_at_unix_ms: i64,
}

impl Query for GetCacheSessionQuery {
  type Outcome = CacheSessionProjection;
}

/// Typed bounded management query for one Build's cache sessions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListBuildCacheSessionsQuery {
  /// Build whose sessions are requested.
  pub build_id: BuildId,
  /// Positive result ceiling.
  pub limit: u16,
  /// Server-observed query time used to project expiry.
  pub observed_at_unix_ms: i64,
}

impl Query for ListBuildCacheSessionsQuery {
  type Outcome = Vec<CacheSessionProjection>;
}

/// Store-backed cache authority with a server-private credential derivation key.
pub struct CacheSessionHandlers<S> {
  registrations: Arc<dyn AgentRegistrationUseCases>,
  store: Arc<S>,
  credential_key: CacheCredentialKey,
  endpoint: String,
  session_lifetime: Duration,
}

impl<S> CacheSessionHandlers<S> {
  /// Creates the service from explicit endpoint, credential, and lifetime policy.
  pub fn new(
    registrations: Arc<dyn AgentRegistrationUseCases>,
    store: Arc<S>,
    credential_key: CacheCredentialKey,
    endpoint: impl Into<String>,
    session_lifetime: Duration,
  ) -> Result<Self, AgentCacheSessionError> {
    let endpoint = validate_cache_endpoint(&endpoint.into())?;
    if session_lifetime.is_zero() || session_lifetime.as_millis() > i64::MAX as u128 {
      return Err(AgentCacheSessionError::InvalidRequest);
    }
    Ok(Self {
      registrations,
      store,
      credential_key,
      endpoint,
      session_lifetime,
    })
  }
}

#[async_trait]
impl<S> AgentCacheSessionUseCases for CacheSessionHandlers<S>
where
  S: CacheSessionStore + 'static,
{
  async fn begin_session(
    &self,
    input: BeginAgentCacheSessionInput,
  ) -> Result<BeginCacheSessionResponse, AgentCacheSessionError> {
    input
      .request
      .validate()
      .map_err(|_| AgentCacheSessionError::InvalidRequest)?;
    let context = agent_lease::authorize_operation(
      self.registrations.as_ref(),
      AgentOperation::Cache,
      &input.route_lease_id,
      &input.request.lease,
      &input.request.registration_id,
      input.credential,
      input.observed_at_unix_ms,
    )
    .await
    .map_err(AgentCacheSessionError::from)?;
    let observed_at = context.observed_at;
    let lease = context.lease;
    let candidate_id = CacheSessionId::generate();
    let candidate_credential = self.credential_key.derive(candidate_id);
    let expires_at = deadline(observed_at, self.session_lifetime)?;
    let outcome = self
      .store
      .begin_cache_session(BeginCacheSession {
        session_id: candidate_id,
        idempotency_key: IdempotencyKey::new(input.request.request_id.clone())
          .map_err(|_| AgentCacheSessionError::InvalidRequest)?,
        lease: lease.access,
        job_id: lease.job_id,
        attempt: lease.attempt,
        requested: input.request.cache,
        credential_digest: candidate_credential.digest(),
        created_at: observed_at,
        expires_at,
      })
      .await?;
    let credential = self.credential_key.derive(outcome.session.id);
    Ok(BeginCacheSessionResponse {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: input.request.request_id,
      session_id: outcome.session.id.to_string(),
      scope_id: outcome.session.scope_id,
      remote: Some(RemoteCacheGrant {
        endpoint: self.endpoint.clone(),
        bearer_token: credential.token(),
        expires_at: unix_seconds(outcome.session.expires_at)?,
      }),
    })
  }

  async fn revoke_session(
    &self,
    input: RevokeAgentCacheSessionInput,
  ) -> Result<RevokeCacheSessionResponse, AgentCacheSessionError> {
    input
      .request
      .validate()
      .map_err(|_| AgentCacheSessionError::InvalidRequest)?;
    let context = agent_lease::authorize_operation(
      self.registrations.as_ref(),
      AgentOperation::Cache,
      &input.route_lease_id,
      &input.request.lease,
      &input.request.registration_id,
      input.credential,
      input.observed_at_unix_ms,
    )
    .await
    .map_err(AgentCacheSessionError::from)?;
    let revoked_at = context.observed_at;
    let lease = context.lease;
    let session_id = input
      .request
      .session_id
      .parse()
      .map_err(|_| AgentCacheSessionError::InvalidRequest)?;
    self
      .store
      .revoke_cache_session(RevokeCacheSession {
        session_id,
        lease: lease.access,
        job_id: lease.job_id,
        revoked_at,
      })
      .await?;
    Ok(RevokeCacheSessionResponse {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: input.request.request_id,
      session_id: input.request.session_id,
    })
  }
}

#[async_trait]
impl<S> QueryHandler<GetCacheSessionQuery> for CacheSessionHandlers<S>
where
  S: CacheSessionStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_query(&self, query: GetCacheSessionQuery) -> Result<CacheSessionProjection, Self::Error> {
    project(
      self.store.cache_session(query.session_id).await?,
      query_timestamp(query.observed_at_unix_ms)?,
    )
  }
}

#[async_trait]
impl<S> QueryHandler<ListBuildCacheSessionsQuery> for CacheSessionHandlers<S>
where
  S: CacheSessionStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_query(&self, query: ListBuildCacheSessionsQuery) -> Result<Vec<CacheSessionProjection>, Self::Error> {
    let observed_at = query_timestamp(query.observed_at_unix_ms)?;
    self
      .store
      .list_build_cache_sessions(ListBuildCacheSessions {
        build_id: query.build_id,
        limit: query.limit,
      })
      .await?
      .into_iter()
      .map(|record| project(record, observed_at))
      .collect()
  }
}

fn project(record: CacheSessionRecord, observed_at: Timestamp) -> Result<CacheSessionProjection, ApplicationError> {
  let state = match record.state {
    CacheSessionState::Revoked => CacheSessionDiagnosticState::Revoked,
    CacheSessionState::Expired => CacheSessionDiagnosticState::Expired,
    CacheSessionState::Active if observed_at >= record.expires_at => CacheSessionDiagnosticState::Expired,
    CacheSessionState::Active if !record.lease_current => CacheSessionDiagnosticState::Fenced,
    CacheSessionState::Active => CacheSessionDiagnosticState::Active,
  };
  Ok(CacheSessionProjection {
    id: record.id,
    project_id: record.project_id,
    build_id: record.build_id,
    job_id: record.job_id,
    agent_id: record.agent_id,
    registration_epoch: record.registration_epoch.get(),
    lease_id: record.lease_id,
    namespace: record.policy.namespace.to_string(),
    read: record.policy.permissions.read,
    write: record.policy.permissions.write,
    quota_bytes: record.policy.quota_bytes,
    created_at: record.created_at,
    expires_at: record.expires_at,
    retention_until: record.retention_until,
    state,
    revoked_at: record.revoked_at,
  })
}

/// Validates and normalizes the credential-free HTTPS cache data-plane endpoint.
pub fn validate_cache_endpoint(value: &str) -> Result<String, AgentCacheSessionError> {
  let mut endpoint = Url::parse(value).map_err(|_| AgentCacheSessionError::InvalidRequest)?;
  if endpoint.scheme() != "https"
    || endpoint.host_str().is_none()
    || !endpoint.username().is_empty()
    || endpoint.password().is_some()
    || endpoint.query().is_some()
    || endpoint.fragment().is_some()
  {
    return Err(AgentCacheSessionError::InvalidRequest);
  }
  endpoint.set_query(None);
  endpoint.set_fragment(None);
  Ok(endpoint.to_string().trim_end_matches('/').to_owned())
}

fn query_timestamp(value: i64) -> Result<Timestamp, ApplicationError> {
  Timestamp::from_unix_millis(value).map_err(|_| ApplicationError::invalid())
}

fn deadline(start: Timestamp, lifetime: Duration) -> Result<Timestamp, AgentCacheSessionError> {
  let milliseconds = i64::try_from(lifetime.as_millis()).map_err(|_| AgentCacheSessionError::InvalidRequest)?;
  let value = start
    .unix_millis()
    .checked_add(milliseconds)
    .ok_or(AgentCacheSessionError::InvalidRequest)?;
  Timestamp::from_unix_millis(value).map_err(|_| AgentCacheSessionError::InvalidRequest)
}

impl From<agent_lease::AuthorizationError> for AgentCacheSessionError {
  fn from(value: agent_lease::AuthorizationError) -> Self {
    match value {
      agent_lease::AuthorizationError::InvalidRequest => Self::InvalidRequest,
      agent_lease::AuthorizationError::Registration(error) => error.into(),
    }
  }
}

fn unix_seconds(value: Timestamp) -> Result<u64, AgentCacheSessionError> {
  u64::try_from(value.unix_millis().div_euclid(1_000)).map_err(|_| AgentCacheSessionError::InvalidRequest)
}

/// Stable failures understood by the Agent cache HTTP adapter.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AgentCacheSessionError {
  /// Route, shared DTO, endpoint, or timestamp is invalid.
  #[error("invalid Agent cache-session request")]
  InvalidRequest,
  /// Registration bearer was rejected.
  #[error("agent credential rejected")]
  CredentialRejected,
  /// Registration or Lease no longer owns the Job.
  #[error("lease is fenced")]
  Fenced,
  /// Current Lease reached its authoritative deadline.
  #[error("lease is expired")]
  Expired,
  /// Session intent conflicts with a prior request.
  #[error("cache session conflicts with durable state")]
  Conflict,
  /// Session does not exist.
  #[error("cache session was not found")]
  NotFound,
  /// Authoritative operation could not safely complete.
  #[error("cache session service unavailable")]
  Unavailable,
}

impl From<AgentRegistrationError> for AgentCacheSessionError {
  fn from(value: AgentRegistrationError) -> Self {
    match value {
      AgentRegistrationError::InvalidRequest => Self::InvalidRequest,
      AgentRegistrationError::CredentialRejected => Self::CredentialRejected,
      AgentRegistrationError::Fenced => Self::Fenced,
      AgentRegistrationError::Unavailable => Self::Unavailable,
    }
  }
}

impl From<StoreError> for AgentCacheSessionError {
  fn from(value: StoreError) -> Self {
    match value {
      StoreError::InvalidInput { .. } => Self::InvalidRequest,
      StoreError::CredentialRejected => Self::CredentialRejected,
      StoreError::Fenced { .. } => Self::Fenced,
      StoreError::Expired { .. } => Self::Expired,
      StoreError::Conflict {
        entity: EntityKind::CacheSession,
      }
      | StoreError::Duplicate {
        entity: EntityKind::CacheSession,
      } => Self::Conflict,
      StoreError::NotFound {
        entity: EntityKind::CacheSession,
      } => Self::NotFound,
      StoreError::Conflict { .. }
      | StoreError::Duplicate { .. }
      | StoreError::NotFound { .. }
      | StoreError::EventGap { .. }
      | StoreError::EventsMissing { .. }
      | StoreError::Unavailable => Self::Unavailable,
    }
  }
}
