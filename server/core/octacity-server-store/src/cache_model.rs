use octacity_protocol::CachePolicy;
use octacity_server_cache::{
  CacheCredentialDigest, CacheNamespace, CacheNamespacePolicy, CacheOperation, CacheSessionState,
};
use octacity_server_domain::{AgentId, BuildId, CacheSessionId, JobId, LeaseId, ProjectId, Timestamp};
use sha2::{Digest as _, Sha256};

use crate::{
  IdempotencyKey, LeaseAccess, MutationDisposition, RegistrationEpoch, StoreError, StoreInputError, StoreOperation,
};

/// Maximum cache-session diagnostics returned by one management query.
pub const MAX_CACHE_SESSION_PAGE_SIZE: u16 = 100;

/// Complete request to begin one fenced short-lived session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BeginCacheSession {
  /// Fresh identity used only when this request is not a replay.
  pub session_id: CacheSessionId,
  /// Agent-owned replay identity scoped to the current Lease.
  pub idempotency_key: IdempotencyKey,
  /// Current fenced Lease authority.
  pub lease: LeaseAccess,
  /// Job identity echoed by the Agent's Lease assignment.
  pub job_id: JobId,
  /// Attempt number echoed by the Agent's Lease assignment.
  pub attempt: octacity_server_domain::AttemptNumber,
  /// Namespace and requested permission subset copied from the signed JobSpec.
  pub requested: CachePolicy,
  /// Digest of the bearer derived for `session_id`.
  pub credential_digest: CacheCredentialDigest,
  /// Server-observed creation time.
  pub created_at: Timestamp,
  /// Exclusive server-controlled credential expiry.
  pub expires_at: Timestamp,
}

impl BeginCacheSession {
  /// Revalidates caller-controlled bounds before an adapter opens a transaction.
  pub fn validate(&self) -> Result<(), StoreError> {
    self.requested.validate().map_err(|_| invalid())?;
    if self.expires_at <= self.created_at {
      return Err(invalid());
    }
    Ok(())
  }
}

/// Durable non-secret session facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheSessionRecord {
  /// Opaque session identity.
  pub id: CacheSessionId,
  /// Opaque stable L1 trust scope for the Project namespace.
  pub scope_id: String,
  /// Owning Project.
  pub project_id: ProjectId,
  /// Build whose immutable policy authorized the session.
  pub build_id: BuildId,
  /// Leased Job.
  pub job_id: JobId,
  /// Agent that owned the registration.
  pub agent_id: AgentId,
  /// Registration epoch bound to the Lease.
  pub registration_epoch: RegistrationEpoch,
  /// Lease whose current ownership gates every authorization.
  pub lease_id: LeaseId,
  /// Complete namespace, permission, quota, and retention policy.
  pub policy: CacheNamespacePolicy,
  /// Persisted lifecycle state.
  pub state: CacheSessionState,
  /// Authoritative creation time.
  pub created_at: Timestamp,
  /// Exclusive credential expiry.
  pub expires_at: Timestamp,
  /// Latest time at which newly published entries may be retained.
  pub retention_until: Timestamp,
  /// Explicit revocation time, when present.
  pub revoked_at: Option<Timestamp>,
  /// Whether the bound Lease and registration are current at query time.
  pub lease_current: bool,
}

/// Result of an idempotent session begin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BeginCacheSessionOutcome {
  /// Newly inserted or replayed non-secret session facts.
  pub session: CacheSessionRecord,
  /// Whether this call applied or replayed the mutation.
  pub disposition: MutationDisposition,
}

/// Fenced idempotent revocation request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevokeCacheSession {
  /// Session returned by the corresponding begin operation.
  pub session_id: CacheSessionId,
  /// Current fenced Lease authority.
  pub lease: LeaseAccess,
  /// Leased Job echoed by the Agent.
  pub job_id: JobId,
  /// Server-observed revocation time.
  pub revoked_at: Timestamp,
}

/// Credential and namespace presented by the cache data plane.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizeCacheSession {
  /// Opaque session selected by the request path or bearer envelope.
  pub session_id: CacheSessionId,
  /// Irreversible digest of the presented bearer.
  pub credential_digest: CacheCredentialDigest,
  /// Exact namespace named by the cache request.
  pub namespace: CacheNamespace,
  /// Read or publication authority requested by the operation.
  pub operation: CacheOperation,
  /// Server-observed authorization time.
  pub observed_at: Timestamp,
}

/// Scoped authority returned only after every current-state check succeeds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheAuthorization {
  /// Owning Project used for physical isolation and quota accounting.
  pub project_id: ProjectId,
  /// Exact single-namespace session policy.
  pub policy: CacheNamespacePolicy,
  /// Latest retention deadline for newly published entries.
  pub retention_until: Timestamp,
}

/// Non-disclosing cache authorization decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CacheAuthorizationOutcome {
  /// The credential, namespace, permission, Lease, registration, and time are current.
  Authorized(CacheAuthorization),
  /// Authorization failed without revealing which protected fact differed.
  Rejected,
}

/// Bounded management query for one Build's session diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListBuildCacheSessions {
  /// Build whose sessions are requested.
  pub build_id: BuildId,
  /// Positive result ceiling.
  pub limit: u16,
}

/// Derives a stable opaque L1 scope without exposing a physical store layout.
#[must_use]
pub fn cache_scope_id(project_id: ProjectId, namespace: &CacheNamespace) -> String {
  let mut digest = Sha256::new();
  digest.update(b"octacity.cache-scope.v1\0");
  digest.update(project_id.to_string().as_bytes());
  digest.update([0]);
  digest.update(namespace.as_str().as_bytes());
  let digest: [u8; 32] = digest.finalize().into();
  let mut result = String::with_capacity(64);
  use std::fmt::Write as _;
  for byte in digest {
    write!(&mut result, "{byte:02x}").expect("writing to a String cannot fail");
  }
  result
}

pub(crate) const fn invalid() -> StoreError {
  StoreError::invalid(StoreOperation::BeginCacheSession, StoreInputError::InvalidCacheSession)
}
