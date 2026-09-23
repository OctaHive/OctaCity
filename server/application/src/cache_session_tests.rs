use std::{
  collections::BTreeSet,
  future::Future,
  sync::Arc,
  task::{Context, Poll, Waker},
  time::Duration,
};

use async_trait::async_trait;
use octacity_protocol::{
  AgentCredentialToken, BeginCacheSessionRequest, COORDINATOR_PROTOCOL_VERSION, CachePolicy,
  LeaseFence as ProtocolLeaseFence, RevokeCacheSessionRequest,
};
use octacity_server_cache::{CacheCredential, CacheCredentialKey, CacheNamespace, CacheOperation, ProjectCachePolicy};
use octacity_server_domain::{AttemptNumber, JobId, LeaseId, Timestamp};
use octacity_server_store::{
  AuthorizeCacheSession, CacheAuthorizationOutcome, CacheSessionStore as _, LeaseAccess, LeaseFence, RegistrationEpoch,
  testing::{CacheLeaseFixture, InMemoryCacheSessionStore},
};

use crate::{
  AgentCacheSessionUseCases, AgentOperation, AgentRegistrationError, AgentRegistrationInput, AgentRegistrationOutcome,
  AgentRegistrationUseCases, AuthorizeAgentInput, AuthorizedAgent, BeginAgentCacheSessionInput, CacheSessionHandlers,
  GetCacheSessionQuery, QueryHandler, RevokeAgentCacheSessionInput,
};

struct RegistrationStub;

#[async_trait]
impl AgentRegistrationUseCases for RegistrationStub {
  async fn register(&self, _input: AgentRegistrationInput) -> Result<AgentRegistrationOutcome, AgentRegistrationError> {
    unreachable!("cache operations do not register Agents")
  }

  async fn authorize(&self, input: AuthorizeAgentInput) -> Result<AuthorizedAgent, AgentRegistrationError> {
    assert_eq!(input.operation, AgentOperation::Cache);
    Ok(AuthorizedAgent::for_test(
      id(6),
      RegistrationEpoch::new(1).unwrap(),
      id(7),
      octacity_protocol::HostCapacity {
        logical_cpu_count: 1,
        total_memory_bytes: 1,
        work_disk_total_bytes: 1,
        state_disk_total_bytes: 1,
        virtualization_available: false,
      },
      input.operation,
    ))
  }
}

#[test]
fn session_authority_is_replay_safe_isolated_and_immediately_revocable() {
  let store = Arc::new(store());
  let service = CacheSessionHandlers::new(
    Arc::new(RegistrationStub),
    Arc::clone(&store),
    CacheCredentialKey::new([9; 32]),
    "https://cache.example/v1",
    Duration::from_secs(5),
  )
  .unwrap();

  let first = run_ready(service.begin_session(begin_input("cache-begin"))).unwrap();
  let replay = run_ready(service.begin_session(begin_input("cache-begin"))).unwrap();
  assert_eq!(replay.session_id, first.session_id);
  assert_eq!(replay.scope_id, first.scope_id);
  let remote = first.remote.as_ref().unwrap();
  assert_eq!(remote.endpoint, "https://cache.example/v1");
  assert!(!format!("{first:?}").contains(&remote.bearer_token));

  let session_id = first.session_id.parse().unwrap();
  let credential = CacheCredential::from_token(&remote.bearer_token).unwrap();
  let authorized = run_ready(store.authorize_cache_session(AuthorizeCacheSession {
    session_id,
    credential_digest: credential.digest(),
    namespace: CacheNamespace::new("project-cache").unwrap(),
    operation: CacheOperation::Read,
    observed_at: time(2_000),
  }))
  .unwrap();
  assert!(matches!(authorized, CacheAuthorizationOutcome::Authorized(_)));
  let isolated = run_ready(store.authorize_cache_session(AuthorizeCacheSession {
    session_id,
    credential_digest: credential.digest(),
    namespace: CacheNamespace::new("other-cache").unwrap(),
    operation: CacheOperation::Read,
    observed_at: time(2_000),
  }))
  .unwrap();
  assert_eq!(isolated, CacheAuthorizationOutcome::Rejected);

  let projection = run_ready(QueryHandler::handle_query(
    &service,
    GetCacheSessionQuery {
      session_id,
      observed_at_unix_ms: 2_000,
    },
  ))
  .unwrap();
  let diagnostic = serde_json::to_value(projection).unwrap();
  for secret in ["credential", "bearer", "token", "scope_id"] {
    assert!(diagnostic.get(secret).is_none());
  }

  let revoked = run_ready(service.revoke_session(revoke_input(&first.session_id))).unwrap();
  assert_eq!(revoked.session_id, first.session_id);
  run_ready(service.revoke_session(revoke_input(&first.session_id))).unwrap();
  assert_eq!(
    run_ready(store.authorize_cache_session(AuthorizeCacheSession {
      session_id,
      credential_digest: credential.digest(),
      namespace: CacheNamespace::new("project-cache").unwrap(),
      operation: CacheOperation::Read,
      observed_at: time(3_000),
    }))
    .unwrap(),
    CacheAuthorizationOutcome::Rejected
  );
}

fn store() -> InMemoryCacheSessionStore {
  let namespace = CacheNamespace::new("project-cache").unwrap();
  InMemoryCacheSessionStore::new(CacheLeaseFixture {
    access: lease_access(),
    project_id: id(1),
    build_id: id(2),
    job_id: id(4),
    attempt: AttemptNumber::FIRST,
    lease_expires_at: time(10_000),
    signed_policy: CachePolicy {
      namespace: namespace.to_string(),
      read: true,
      write: true,
    },
    project_policy: ProjectCachePolicy {
      namespaces: BTreeSet::from([namespace]),
      read: true,
      write: true,
      max_bytes: 1_024,
    },
    retention_seconds: 60,
  })
}

fn begin_input(request_id: &str) -> BeginAgentCacheSessionInput {
  BeginAgentCacheSessionInput {
    route_lease_id: id::<LeaseId>(5).to_string(),
    request: BeginCacheSessionRequest {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: request_id.to_owned(),
      registration_id: "registration-1".to_owned(),
      lease: protocol_lease(),
      cache: CachePolicy {
        namespace: "project-cache".to_owned(),
        read: true,
        write: false,
      },
    },
    credential: credential(),
    observed_at_unix_ms: 1_000,
  }
}

fn revoke_input(session_id: &str) -> RevokeAgentCacheSessionInput {
  RevokeAgentCacheSessionInput {
    route_lease_id: id::<LeaseId>(5).to_string(),
    request: RevokeCacheSessionRequest {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: "cache-revoke".to_owned(),
      registration_id: "registration-1".to_owned(),
      lease: protocol_lease(),
      session_id: session_id.to_owned(),
    },
    credential: credential(),
    observed_at_unix_ms: 3_000,
  }
}

fn protocol_lease() -> ProtocolLeaseFence {
  ProtocolLeaseFence {
    lease_id: id::<LeaseId>(5).to_string(),
    job_id: id::<JobId>(4).to_string(),
    attempt: 1,
    fencing_token: "05".repeat(32),
  }
}

fn lease_access() -> LeaseAccess {
  LeaseAccess {
    lease_id: id(5),
    fence: LeaseFence::from_bytes([5; 32]),
    agent_id: id(6),
    registration_epoch: RegistrationEpoch::new(1).unwrap(),
  }
}

fn credential() -> AgentCredentialToken {
  AgentCredentialToken::parse(
    "registration.00000000-0000-0000-0000-000000000001.BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc",
  )
  .unwrap()
}

fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}

fn id<T>(value: u64) -> T
where
  T: std::str::FromStr,
  T::Err: std::fmt::Debug,
{
  format!("00000000-0000-0000-0000-{value:012x}").parse().unwrap()
}

fn run_ready<T>(future: impl Future<Output = T>) -> T {
  let mut future = std::pin::pin!(future);
  let mut context = Context::from_waker(Waker::noop());
  match future.as_mut().poll(&mut context) {
    Poll::Ready(value) => value,
    Poll::Pending => panic!("in-memory cache test unexpectedly awaited external I/O"),
  }
}
