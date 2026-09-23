use std::{collections::BTreeMap, sync::Mutex};

use async_trait::async_trait;
use octacity_server_cache::{CacheNamespacePolicy, CachePermissions, CacheSessionEvent};
use octacity_server_domain::{BuildId, CacheSessionId, EntityKind, Timestamp};

use crate::{
  AuthorizeCacheSession, BeginCacheSession, BeginCacheSessionOutcome, CacheAuthorization, CacheAuthorizationOutcome,
  CacheSessionRecord, CacheSessionStore, LeaseAccess, ListBuildCacheSessions, MutationDisposition, ProjectCachePolicy,
  RevokeCacheSession, StoreError, cache_scope_id,
};

/// Authoritative facts seeded for one deterministic cache-session adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheLeaseFixture {
  /// Exact current fenced Lease authority.
  pub access: LeaseAccess,
  /// Owning Project.
  pub project_id: octacity_server_domain::ProjectId,
  /// Build whose immutable policy authorized the Lease.
  pub build_id: BuildId,
  /// Leased Job.
  pub job_id: octacity_server_domain::JobId,
  /// Leased Attempt number.
  pub attempt: octacity_server_domain::AttemptNumber,
  /// Authoritative Lease expiry.
  pub lease_expires_at: Timestamp,
  /// Exact cache authority signed into the JobSpec.
  pub signed_policy: octacity_protocol::CachePolicy,
  /// Effective Project cache policy frozen with the Build.
  pub project_policy: ProjectCachePolicy,
  /// Maximum cache retention captured with the Build.
  pub retention_seconds: u64,
}

#[derive(Clone)]
struct StoredSession {
  request: BeginCacheSession,
  record: CacheSessionRecord,
}

#[derive(Default)]
struct CacheMemoryState {
  sessions: BTreeMap<CacheSessionId, StoredSession>,
  requests: BTreeMap<(octacity_server_domain::LeaseId, String), CacheSessionId>,
  lease_current: bool,
}

/// Deterministic cache-session adapter used by application and shared contract tests.
pub struct InMemoryCacheSessionStore {
  fixture: CacheLeaseFixture,
  state: Mutex<CacheMemoryState>,
}

impl InMemoryCacheSessionStore {
  /// Creates an empty store with one current Lease and immutable policy snapshot.
  #[must_use]
  pub fn new(fixture: CacheLeaseFixture) -> Self {
    Self {
      fixture,
      state: Mutex::new(CacheMemoryState {
        lease_current: true,
        ..CacheMemoryState::default()
      }),
    }
  }

  /// Changes whether the seeded Lease remains current for authorization tests.
  pub fn set_lease_current(&self, current: bool) -> Result<(), StoreError> {
    self.state.lock().map_err(|_| StoreError::Unavailable)?.lease_current = current;
    Ok(())
  }

  fn require_lease(&self, access: LeaseAccess, observed_at: Timestamp) -> Result<(), StoreError> {
    let state = self.state.lock().map_err(|_| StoreError::Unavailable)?;
    if !state.lease_current || access != self.fixture.access {
      return Err(StoreError::Fenced { lease: access.lease_id });
    }
    if observed_at >= self.fixture.lease_expires_at {
      return Err(StoreError::Expired { lease: access.lease_id });
    }
    Ok(())
  }
}

#[async_trait]
impl CacheSessionStore for InMemoryCacheSessionStore {
  async fn begin_cache_session(&self, request: BeginCacheSession) -> Result<BeginCacheSessionOutcome, StoreError> {
    request.validate()?;
    self.require_lease(request.lease, request.created_at)?;
    if request.job_id != self.fixture.job_id || request.attempt != self.fixture.attempt {
      return Err(StoreError::Fenced {
        lease: request.lease.lease_id,
      });
    }
    let requested_namespace = request
      .requested
      .namespace
      .parse()
      .map_err(|_| crate::cache_model::invalid())?;
    let requested_permissions = CachePermissions::new(request.requested.read, request.requested.write)
      .map_err(|_| crate::cache_model::invalid())?;
    let policy = CacheNamespacePolicy::select(
      &self.fixture.project_policy,
      self.fixture.retention_seconds,
      requested_namespace,
      requested_permissions,
      &self.fixture.signed_policy,
    )
    .map_err(|_| crate::cache_model::invalid())?;
    let key = (request.lease.lease_id, request.idempotency_key.as_str().to_owned());
    let mut state = self.state.lock().map_err(|_| StoreError::Unavailable)?;
    if let Some(session_id) = state.requests.get(&key) {
      let existing = state
        .sessions
        .get(session_id)
        .expect("request must reference a session");
      if same_intent(&existing.request, &request) {
        return Ok(BeginCacheSessionOutcome {
          session: existing.record.clone(),
          disposition: MutationDisposition::Replayed,
        });
      }
      return Err(StoreError::Conflict {
        entity: EntityKind::CacheSession,
      });
    }
    if state.sessions.contains_key(&request.session_id) {
      return Err(StoreError::Duplicate {
        entity: EntityKind::CacheSession,
      });
    }
    let retention_until = add_seconds(request.created_at, policy.retention_seconds)?;
    let expires_at = request.expires_at.min(self.fixture.lease_expires_at);
    let record = CacheSessionRecord {
      id: request.session_id,
      scope_id: cache_scope_id(self.fixture.project_id, &policy.namespace),
      project_id: self.fixture.project_id,
      build_id: self.fixture.build_id,
      job_id: self.fixture.job_id,
      agent_id: request.lease.agent_id,
      registration_epoch: request.lease.registration_epoch,
      lease_id: request.lease.lease_id,
      policy,
      state: octacity_server_cache::CacheSessionState::Active,
      created_at: request.created_at,
      expires_at,
      retention_until,
      revoked_at: None,
      lease_current: true,
    };
    state.requests.insert(key, request.session_id);
    state.sessions.insert(
      request.session_id,
      StoredSession {
        request,
        record: record.clone(),
      },
    );
    Ok(BeginCacheSessionOutcome {
      session: record,
      disposition: MutationDisposition::Applied,
    })
  }

  async fn revoke_cache_session(&self, request: RevokeCacheSession) -> Result<MutationDisposition, StoreError> {
    self.require_lease(request.lease, request.revoked_at)?;
    let mut state = self.state.lock().map_err(|_| StoreError::Unavailable)?;
    let stored = state
      .sessions
      .get_mut(&request.session_id)
      .ok_or(StoreError::NotFound {
        entity: EntityKind::CacheSession,
      })?;
    if stored.record.lease_id != request.lease.lease_id || stored.record.job_id != request.job_id {
      return Err(StoreError::Fenced {
        lease: request.lease.lease_id,
      });
    }
    if stored.record.state == octacity_server_cache::CacheSessionState::Revoked {
      return Ok(MutationDisposition::Replayed);
    }
    stored.record.state =
      stored
        .record
        .state
        .transition(CacheSessionEvent::Revoke)
        .map_err(|_| StoreError::Conflict {
          entity: EntityKind::CacheSession,
        })?;
    stored.record.revoked_at = Some(request.revoked_at);
    Ok(MutationDisposition::Applied)
  }

  async fn authorize_cache_session(
    &self,
    request: AuthorizeCacheSession,
  ) -> Result<CacheAuthorizationOutcome, StoreError> {
    let state = self.state.lock().map_err(|_| StoreError::Unavailable)?;
    let Some(stored) = state.sessions.get(&request.session_id) else {
      return Ok(CacheAuthorizationOutcome::Rejected);
    };
    let current = state.lease_current
      && stored.record.state == octacity_server_cache::CacheSessionState::Active
      && request.observed_at < stored.record.expires_at
      && request.observed_at < self.fixture.lease_expires_at
      && stored.request.credential_digest.matches(request.credential_digest)
      && stored.record.policy.namespace == request.namespace
      && stored.record.policy.permissions.allows(request.operation);
    if !current {
      return Ok(CacheAuthorizationOutcome::Rejected);
    }
    Ok(CacheAuthorizationOutcome::Authorized(CacheAuthorization {
      project_id: stored.record.project_id,
      policy: stored.record.policy.clone(),
      retention_until: stored.record.retention_until,
    }))
  }

  async fn cache_session(&self, session_id: CacheSessionId) -> Result<CacheSessionRecord, StoreError> {
    let state = self.state.lock().map_err(|_| StoreError::Unavailable)?;
    state
      .sessions
      .get(&session_id)
      .map(|stored| {
        let mut record = stored.record.clone();
        record.lease_current = state.lease_current;
        record
      })
      .ok_or(StoreError::NotFound {
        entity: EntityKind::CacheSession,
      })
  }

  async fn list_build_cache_sessions(
    &self,
    request: ListBuildCacheSessions,
  ) -> Result<Vec<CacheSessionRecord>, StoreError> {
    if request.limit == 0 || request.limit > crate::MAX_CACHE_SESSION_PAGE_SIZE {
      return Err(StoreError::invalid(
        crate::StoreOperation::ListCacheSessions,
        crate::StoreInputError::InvalidCacheSession,
      ));
    }
    let state = self.state.lock().map_err(|_| StoreError::Unavailable)?;
    Ok(
      state
        .sessions
        .values()
        .filter(|stored| stored.record.build_id == request.build_id)
        .take(usize::from(request.limit))
        .map(|stored| {
          let mut record = stored.record.clone();
          record.lease_current = state.lease_current;
          record
        })
        .collect(),
    )
  }
}

fn same_intent(left: &BeginCacheSession, right: &BeginCacheSession) -> bool {
  left.idempotency_key == right.idempotency_key
    && left.lease == right.lease
    && left.job_id == right.job_id
    && left.attempt == right.attempt
    && left.requested == right.requested
}

fn add_seconds(timestamp: Timestamp, seconds: u64) -> Result<Timestamp, StoreError> {
  let milliseconds = i64::try_from(seconds)
    .ok()
    .and_then(|value| value.checked_mul(1_000))
    .and_then(|value| timestamp.unix_millis().checked_add(value))
    .ok_or_else(crate::cache_model::invalid)?;
  Timestamp::from_unix_millis(milliseconds).map_err(|_| crate::cache_model::invalid())
}

#[cfg(test)]
mod tests {
  use std::{
    collections::BTreeSet,
    future::Future,
    task::{Context, Poll, Waker},
  };

  use octacity_server_cache::{CacheCredentialKey, CacheNamespace};

  use super::*;
  use crate::test_support::{id, time};
  use crate::{LeaseFence, RegistrationEpoch, cache_contract_testing::*};

  #[test]
  fn in_memory_adapter_satisfies_cache_session_contract() {
    let namespace = CacheNamespace::new("project-cache").unwrap();
    let lease = LeaseAccess {
      lease_id: id(1),
      fence: LeaseFence::from_bytes([1; 32]),
      agent_id: id(2),
      registration_epoch: RegistrationEpoch::new(1).unwrap(),
    };
    let fixture = CacheLeaseFixture {
      access: lease,
      project_id: id(3),
      build_id: id(4),
      job_id: id(5),
      attempt: octacity_server_domain::AttemptNumber::FIRST,
      lease_expires_at: time(10_000),
      signed_policy: octacity_protocol::CachePolicy {
        namespace: namespace.to_string(),
        read: true,
        write: true,
      },
      project_policy: ProjectCachePolicy {
        namespaces: BTreeSet::from([namespace.clone()]),
        read: true,
        write: true,
        max_bytes: 1_024,
      },
      retention_seconds: 60,
    };
    let store = InMemoryCacheSessionStore::new(fixture);
    run_ready(verify_cache_session_store_contract(
      &store,
      CacheSessionStoreContractFixture {
        session_id: id(6),
        replay_session_id: id(7),
        lease,
        job_id: id(5),
        attempt: octacity_server_domain::AttemptNumber::FIRST,
        namespace,
        other_namespace: CacheNamespace::new("other-cache").unwrap(),
        created_at: time(1_000),
        expires_at: time(9_000),
        revoked_at: time(2_000),
        credential_key: CacheCredentialKey::new([8; 32]),
      },
    ));
  }

  fn run_ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
      Poll::Ready(value) => value,
      Poll::Pending => panic!("in-memory cache store unexpectedly awaited external I/O"),
    }
  }
}
