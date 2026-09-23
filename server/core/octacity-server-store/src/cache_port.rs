use async_trait::async_trait;
use octacity_server_domain::CacheSessionId;

use crate::{
  AuthorizeCacheSession, BeginCacheSession, BeginCacheSessionOutcome, CacheAuthorizationOutcome, CacheSessionRecord,
  ListBuildCacheSessions, MutationDisposition, RevokeCacheSession, StoreError,
};

/// Authoritative port for fenced cache-session lifecycle and diagnostics.
#[async_trait]
pub trait CacheSessionStore: Send + Sync {
  /// Begins or exactly replays one session after validating current Lease and immutable policy.
  async fn begin_cache_session(&self, request: BeginCacheSession) -> Result<BeginCacheSessionOutcome, StoreError>;

  /// Revokes one session idempotently under its bound current Lease.
  async fn revoke_cache_session(&self, request: RevokeCacheSession) -> Result<MutationDisposition, StoreError>;

  /// Authorizes one operation without distinguishing protected rejection reasons.
  async fn authorize_cache_session(
    &self,
    request: AuthorizeCacheSession,
  ) -> Result<CacheAuthorizationOutcome, StoreError>;

  /// Reads one secret-free session diagnostic.
  async fn cache_session(&self, session_id: CacheSessionId) -> Result<CacheSessionRecord, StoreError>;

  /// Lists a bounded set of secret-free diagnostics for one Build.
  async fn list_build_cache_sessions(
    &self,
    request: ListBuildCacheSessions,
  ) -> Result<Vec<CacheSessionRecord>, StoreError>;
}
