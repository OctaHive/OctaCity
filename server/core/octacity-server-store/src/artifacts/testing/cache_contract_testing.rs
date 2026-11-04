use octacity_server_cache::{CacheCredentialKey, CacheNamespace, CacheOperation};
use octacity_server_domain::{CacheSessionId, Timestamp};

use crate::{
  AuthorizeCacheSession, BeginCacheSession, CacheAuthorizationOutcome, CacheSessionStore, IdempotencyKey,
  MutationDisposition, RevokeCacheSession,
};

/// Inputs shared by every cache-session persistence contract.
#[derive(Clone, Debug)]
pub struct CacheSessionStoreContractFixture {
  /// Fresh session identity.
  pub session_id: CacheSessionId,
  /// Different identity supplied on exact replay.
  pub replay_session_id: CacheSessionId,
  /// Current fenced Lease authority.
  pub lease: crate::LeaseAccess,
  /// Leased Job.
  pub job_id: octacity_server_domain::JobId,
  /// Leased Attempt number.
  pub attempt: octacity_server_domain::AttemptNumber,
  /// Authorized namespace.
  pub namespace: CacheNamespace,
  /// Different namespace used to prove isolation.
  pub other_namespace: CacheNamespace,
  /// Session creation time.
  pub created_at: Timestamp,
  /// Exclusive session expiry.
  pub expires_at: Timestamp,
  /// Explicit revocation time.
  pub revoked_at: Timestamp,
  /// Deterministic key used only to produce test credentials.
  pub credential_key: CacheCredentialKey,
}

/// Verifies replay, namespace isolation, explicit revocation, and secret-free reads.
pub async fn verify_cache_session_store_contract<S>(store: &S, fixture: CacheSessionStoreContractFixture)
where
  S: CacheSessionStore,
{
  let credential = fixture.credential_key.derive(fixture.session_id);
  let request = begin_request(&fixture, fixture.session_id, credential.digest());
  let applied = store.begin_cache_session(request.clone()).await.unwrap();
  assert_eq!(applied.disposition, MutationDisposition::Applied);
  assert_eq!(applied.session.id, fixture.session_id);
  assert_eq!(applied.session.policy.namespace, fixture.namespace);

  let replay = store
    .begin_cache_session(begin_request(
      &fixture,
      fixture.replay_session_id,
      fixture.credential_key.derive(fixture.replay_session_id).digest(),
    ))
    .await
    .unwrap();
  assert_eq!(replay.disposition, MutationDisposition::Replayed);
  assert_eq!(replay.session.id, fixture.session_id);

  let authorized = store
    .authorize_cache_session(AuthorizeCacheSession {
      session_id: fixture.session_id,
      credential_digest: credential.digest(),
      namespace: fixture.namespace.clone(),
      operation: CacheOperation::Read,
      observed_at: fixture.created_at,
    })
    .await
    .unwrap();
  assert!(matches!(authorized, CacheAuthorizationOutcome::Authorized(_)));

  let isolated = store
    .authorize_cache_session(AuthorizeCacheSession {
      session_id: fixture.session_id,
      credential_digest: credential.digest(),
      namespace: fixture.other_namespace.clone(),
      operation: CacheOperation::Read,
      observed_at: fixture.created_at,
    })
    .await
    .unwrap();
  assert_eq!(isolated, CacheAuthorizationOutcome::Rejected);

  assert_eq!(
    store
      .revoke_cache_session(RevokeCacheSession {
        session_id: fixture.session_id,
        lease: fixture.lease,
        job_id: fixture.job_id,
        revoked_at: fixture.revoked_at,
      })
      .await
      .unwrap(),
    MutationDisposition::Applied
  );
  assert_eq!(
    store
      .revoke_cache_session(RevokeCacheSession {
        session_id: fixture.session_id,
        lease: fixture.lease,
        job_id: fixture.job_id,
        revoked_at: fixture.revoked_at,
      })
      .await
      .unwrap(),
    MutationDisposition::Replayed
  );
  assert_eq!(
    store
      .authorize_cache_session(AuthorizeCacheSession {
        session_id: fixture.session_id,
        credential_digest: credential.digest(),
        namespace: fixture.namespace,
        operation: CacheOperation::Read,
        observed_at: fixture.revoked_at,
      })
      .await
      .unwrap(),
    CacheAuthorizationOutcome::Rejected
  );

  let diagnostic = store.cache_session(fixture.session_id).await.unwrap();
  assert_eq!(diagnostic.revoked_at, Some(fixture.revoked_at));
}

fn begin_request(
  fixture: &CacheSessionStoreContractFixture,
  session_id: CacheSessionId,
  credential_digest: octacity_server_cache::CacheCredentialDigest,
) -> BeginCacheSession {
  BeginCacheSession {
    session_id,
    idempotency_key: IdempotencyKey::new("cache:begin").unwrap(),
    lease: fixture.lease,
    job_id: fixture.job_id,
    attempt: fixture.attempt,
    requested: octacity_protocol::CachePolicy {
      namespace: fixture.namespace.to_string(),
      read: true,
      write: false,
    },
    credential_digest,
    created_at: fixture.created_at,
    expires_at: fixture.expires_at,
  }
}
