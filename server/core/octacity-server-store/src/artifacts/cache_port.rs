use async_trait::async_trait;
use octacity_server_domain::CacheSessionId;

use crate::{
  AuthorizeCacheSession, BeginCacheSession, BeginCacheSessionOutcome, CacheAuthorizationOutcome,
  CacheBlobPreparationOutcome, CacheDataAccess, CachePublicationOutcome, CacheRetentionOutcome, CacheSessionRecord,
  ListBuildCacheSessions, MutationDisposition, PublishCacheAction, PublishCacheBlob, RevokeCacheSession, StoreError,
};
use octacity_server_cache::{ActionResultV1, BlobDescriptor, CacheBlobObject, Digest};
use octacity_server_cache::{CacheCredentialDigest, CacheNamespace};
use octacity_server_domain::Timestamp;

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

/// Authoritative metadata boundary for Octa's HTTP L2 cache protocol.
#[async_trait]
pub trait CacheDataStore: Send + Sync {
  /// Resolves the single namespace bound to a current bearer without disclosing other session facts.
  async fn resolve_cache_namespace(
    &self,
    credential_digest: CacheCredentialDigest,
    observed_at: Timestamp,
  ) -> Result<CacheNamespace, StoreError>;

  /// Returns only requested representations not visible in this exact scope.
  async fn find_missing_cache_blobs(
    &self,
    access: CacheDataAccess,
    blobs: Vec<BlobDescriptor>,
  ) -> Result<Vec<BlobDescriptor>, StoreError>;

  /// Resolves visible metadata to one isolated byte-store object.
  async fn cache_blob(
    &self,
    access: CacheDataAccess,
    blob: BlobDescriptor,
  ) -> Result<Option<CacheBlobObject>, StoreError>;

  /// Authorizes an upload and derives its isolated byte-store identity without publishing metadata.
  async fn prepare_cache_blob(
    &self,
    access: CacheDataAccess,
    blob: BlobDescriptor,
  ) -> Result<CacheBlobPreparationOutcome, StoreError>;

  /// Atomically publishes verified blob metadata under current write authority.
  async fn publish_cache_blob(&self, request: PublishCacheBlob) -> Result<CachePublicationOutcome, StoreError>;

  /// Reads one opaque action result without revealing any other namespace.
  async fn cache_action(&self, access: CacheDataAccess, action: Digest) -> Result<Option<ActionResultV1>, StoreError>;

  /// Atomically publishes an action only after its referenced blob is visible.
  async fn publish_cache_action(&self, request: PublishCacheAction) -> Result<CachePublicationOutcome, StoreError>;

  /// Removes expired logical metadata while retaining blobs referenced by live actions.
  async fn prune_cache(&self, access: CacheDataAccess) -> Result<CacheRetentionOutcome, StoreError>;
}
