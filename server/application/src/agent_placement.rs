use std::{
  sync::Arc,
  time::{Duration, Instant},
};

use async_trait::async_trait;
use octacity_observability::{ErrorClass, Operation, ServerOperationMetric};
use octacity_protocol::{AcquireLeaseRequest, AgentCredentialToken, COORDINATOR_PROTOCOL_VERSION};
use octacity_server_domain::{LeaseId, Timestamp};
use octacity_server_store::{
  JobClaim, JobClaimOutcome, JobExecutionStore, LeaseFence, LeaseGrant, LeaseWindow, StoreError,
};
use thiserror::Error;
use uuid::Uuid;

use crate::{
  AgentOperation, AgentRegistrationError, AgentRegistrationUseCases, AuthorizeAgentInput,
  agent_registration::stable_agent_id,
};

const LEASE_ID_NAMESPACE: Uuid = Uuid::from_u128(0x239b_0ea8_b50c_4a51_89ec_3d79_07b7_d47f);

/// Complete transport-independent input for one lease long poll.
#[derive(Clone, Debug)]
pub struct AcquireAgentLeaseInput {
  /// Agent identity selected by the route.
  pub route_agent_id: String,
  /// Shared protocol request body.
  pub request: AcquireLeaseRequest,
  /// Current registration bearer.
  pub credential: AgentCredentialToken,
  /// Server-observed request arrival time.
  pub observed_at_unix_ms: i64,
}

/// Result of one bounded placement long poll.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentLeaseOutcome {
  /// One compatible ready Job committed its Lease.
  Lease(Box<LeaseGrant>),
  /// No compatible ready Job was visible before the wait ended.
  NoWork,
}

/// Replaceable wake-up hint used between authoritative queue claims.
#[async_trait]
pub trait ReadyJobWaiter: Send + Sync {
  /// Opaque observation captured before the authoritative queue read.
  type Checkpoint: Send;

  /// Captures the current notification state for one Pool.
  fn checkpoint(&self, pool_identity: &str) -> Self::Checkpoint;

  /// Waits for a possible ready-queue change or the supplied bound.
  async fn wait_for_ready_job(&self, checkpoint: Self::Checkpoint, timeout: Duration);
}

/// Application boundary used by the authenticated Agent HTTP adapter.
#[async_trait]
pub trait AgentLeaseUseCases: Send + Sync {
  /// Authenticates one current registration and performs a bounded long poll.
  async fn acquire(&self, input: AcquireAgentLeaseInput) -> Result<AgentLeaseOutcome, AgentLeaseError>;
}

/// Store-backed placement service that never holds store state while waiting.
pub struct AgentLeaseService<S, W> {
  registrations: Arc<dyn AgentRegistrationUseCases>,
  store: Arc<S>,
  waiter: Arc<W>,
  lease_lifetime: Duration,
}

impl<S, W> AgentLeaseService<S, W> {
  /// Creates a placement service with explicit lease timing policy.
  pub fn new(
    registrations: Arc<dyn AgentRegistrationUseCases>,
    store: Arc<S>,
    waiter: Arc<W>,
    lease_lifetime: Duration,
  ) -> Result<Self, AgentLeaseError> {
    if lease_lifetime.is_zero() || lease_lifetime.as_millis() > i64::MAX as u128 {
      return Err(AgentLeaseError::InvalidRequest);
    }
    Ok(Self {
      registrations,
      store,
      waiter,
      lease_lifetime,
    })
  }
}

#[async_trait]
impl<S, W> AgentLeaseUseCases for AgentLeaseService<S, W>
where
  S: JobExecutionStore + 'static,
  W: ReadyJobWaiter + 'static,
{
  async fn acquire(&self, input: AcquireAgentLeaseInput) -> Result<AgentLeaseOutcome, AgentLeaseError> {
    crate::telemetry::observe(
      ServerOperationMetric::Lease,
      Operation::Claim,
      async {
        input.request.validate().map_err(|_| AgentLeaseError::InvalidRequest)?;
        if input.request.protocol_version != COORDINATOR_PROTOCOL_VERSION {
          return Err(AgentLeaseError::InvalidRequest);
        }
        let started = Instant::now();
        let authorized = self.authorize(&input, input.observed_at_unix_ms).await?;
        if stable_agent_id(&input.route_agent_id).map_err(AgentLeaseError::from)? != authorized.agent_id() {
          return Err(AgentLeaseError::Fenced);
        }
        if !input.request.accept_jobs {
          return Ok(AgentLeaseOutcome::NoWork);
        }
        input
          .request
          .snapshot
          .validate(authorized.host_capacity())
          .map_err(|_| AgentLeaseError::InvalidRequest)?;
        let checkpoint = self.waiter.checkpoint(&authorized.pool_id().to_string());
        if let Some(grant) = self.claim(&input, &authorized, input.observed_at_unix_ms).await? {
          return Ok(AgentLeaseOutcome::Lease(Box::new(grant)));
        }

        let wait = Duration::from_secs(input.request.wait_seconds);
        self.waiter.wait_for_ready_job(checkpoint, wait).await;

        let elapsed = i64::try_from(started.elapsed().as_millis()).map_err(|_| AgentLeaseError::Unavailable)?;
        let observed_at = input
          .observed_at_unix_ms
          .checked_add(elapsed)
          .ok_or(AgentLeaseError::Unavailable)?;
        let authorized = self.authorize(&input, observed_at).await?;
        self.claim(&input, &authorized, observed_at).await.map(|grant| {
          grant.map_or(AgentLeaseOutcome::NoWork, |grant| {
            AgentLeaseOutcome::Lease(Box::new(grant))
          })
        })
      },
      |error| match error {
        AgentLeaseError::InvalidRequest | AgentLeaseError::CredentialRejected => {
          crate::telemetry::rejected(ErrorClass::Invalid)
        }
        AgentLeaseError::Fenced => crate::telemetry::rejected(ErrorClass::Fenced),
        AgentLeaseError::Unavailable => crate::telemetry::failed(ErrorClass::Unavailable),
      },
    )
    .await
  }
}

impl<S, W> AgentLeaseService<S, W>
where
  S: JobExecutionStore,
  W: ReadyJobWaiter,
{
  async fn authorize(
    &self,
    input: &AcquireAgentLeaseInput,
    observed_at_unix_ms: i64,
  ) -> Result<crate::AuthorizedAgent, AgentLeaseError> {
    self
      .registrations
      .authorize(AuthorizeAgentInput {
        operation: AgentOperation::Poll,
        registration_id: input.request.registration_id.clone(),
        credential: input.credential.clone(),
        observed_at_unix_ms,
      })
      .await
      .map_err(AgentLeaseError::from)
  }

  async fn claim(
    &self,
    input: &AcquireAgentLeaseInput,
    authorized: &crate::AuthorizedAgent,
    observed_at_unix_ms: i64,
  ) -> Result<Option<LeaseGrant>, AgentLeaseError> {
    let claimed_at = Timestamp::from_unix_millis(observed_at_unix_ms).map_err(|_| AgentLeaseError::InvalidRequest)?;
    let lifetime = i64::try_from(self.lease_lifetime.as_millis()).map_err(|_| AgentLeaseError::InvalidRequest)?;
    let expires_at = Timestamp::from_unix_millis(
      observed_at_unix_ms
        .checked_add(lifetime)
        .ok_or(AgentLeaseError::InvalidRequest)?,
    )
    .map_err(|_| AgentLeaseError::InvalidRequest)?;
    let lease_id = lease_id(&input.request.registration_id, &input.request.request_id)?;
    let fence = lease_fence(&input.credential, &input.request.request_id);
    let claim = JobClaim::new(
      lease_id,
      fence,
      authorized.agent_id(),
      authorized.registration_epoch(),
      authorized.pool_id(),
      input.request.snapshot.clone(),
      LeaseWindow::new(claimed_at, expires_at)?,
    )?;
    match self.store.claim_ready_job(claim).await? {
      JobClaimOutcome::Empty => Ok(None),
      JobClaimOutcome::Claimed(grant) => Ok(Some(*grant)),
    }
  }
}

/// Stable application failures understood by the Agent API adapter.
#[derive(Debug, Error)]
pub enum AgentLeaseError {
  /// Request fields or server-controlled timing policy are invalid.
  #[error("invalid lease acquisition request")]
  InvalidRequest,
  /// The registration bearer was rejected.
  #[error("agent credential rejected")]
  CredentialRejected,
  /// The registration is no longer current.
  #[error("agent registration is fenced")]
  Fenced,
  /// Placement could not safely complete.
  #[error("lease placement unavailable")]
  Unavailable,
}

impl From<AgentRegistrationError> for AgentLeaseError {
  fn from(value: AgentRegistrationError) -> Self {
    match value {
      AgentRegistrationError::InvalidRequest => Self::InvalidRequest,
      AgentRegistrationError::CredentialRejected => Self::CredentialRejected,
      AgentRegistrationError::Fenced => Self::Fenced,
      AgentRegistrationError::Unavailable => Self::Unavailable,
    }
  }
}

impl From<StoreError> for AgentLeaseError {
  fn from(value: StoreError) -> Self {
    match value {
      StoreError::InvalidInput { .. } => Self::InvalidRequest,
      StoreError::CredentialRejected => Self::CredentialRejected,
      StoreError::Fenced { .. } | StoreError::Expired { .. } => Self::Fenced,
      _ => Self::Unavailable,
    }
  }
}

fn lease_id(registration_id: &str, request_id: &str) -> Result<LeaseId, AgentLeaseError> {
  let identity = format!("{registration_id}:{request_id}");
  LeaseId::from_uuid(Uuid::new_v5(&LEASE_ID_NAMESPACE, identity.as_bytes()))
    .map_err(|_| AgentLeaseError::InvalidRequest)
}

fn lease_fence(credential: &AgentCredentialToken, request_id: &str) -> LeaseFence {
  let secret = credential.secret();
  let namespace = Uuid::from_bytes(secret[..16].try_into().expect("credential secret has a fixed length"));
  let first = Uuid::new_v5(
    &namespace,
    format!("octacity-lease-fence-v1:first:{request_id}").as_bytes(),
  );
  let second = Uuid::new_v5(
    &namespace,
    format!("octacity-lease-fence-v1:second:{request_id}").as_bytes(),
  );
  let mut bytes = [0_u8; 32];
  bytes[..16].copy_from_slice(first.as_bytes());
  bytes[16..].copy_from_slice(second.as_bytes());
  LeaseFence::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
  use std::{
    future::Future,
    sync::{
      Mutex,
      atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Waker},
  };

  use octacity_server_domain::PoolId;
  use octacity_server_store::{
    AppendJobEvents, AppendJobEventsOutcome, CompletionDisposition, JobCompletion, RegistrationEpoch,
  };

  use super::*;

  struct RegistrationStub {
    authority: crate::AuthorizedAgent,
    calls: AtomicUsize,
  }

  #[async_trait]
  impl AgentRegistrationUseCases for RegistrationStub {
    async fn register(
      &self,
      _input: crate::AgentRegistrationInput,
    ) -> Result<crate::AgentRegistrationOutcome, AgentRegistrationError> {
      unreachable!("placement does not register an Agent")
    }

    async fn authorize(&self, _input: AuthorizeAgentInput) -> Result<crate::AuthorizedAgent, AgentRegistrationError> {
      self.calls.fetch_add(1, Ordering::SeqCst);
      Ok(self.authority.clone())
    }
  }

  #[derive(Default)]
  struct EmptyStore {
    claims: Mutex<Vec<JobClaim>>,
  }

  #[async_trait]
  impl JobExecutionStore for EmptyStore {
    async fn claim_ready_job(&self, request: JobClaim) -> Result<JobClaimOutcome, StoreError> {
      self.claims.lock().unwrap().push(request);
      Ok(JobClaimOutcome::Empty)
    }

    async fn append_job_events(&self, _request: AppendJobEvents) -> Result<AppendJobEventsOutcome, StoreError> {
      unreachable!("placement does not append events")
    }

    async fn complete_job(&self, _request: JobCompletion) -> Result<CompletionDisposition, StoreError> {
      unreachable!("placement does not complete Jobs")
    }
  }

  #[derive(Default)]
  struct ImmediateWaiter(AtomicUsize);

  #[async_trait]
  impl ReadyJobWaiter for ImmediateWaiter {
    type Checkpoint = ();

    fn checkpoint(&self, _pool_identity: &str) {}

    async fn wait_for_ready_job(&self, (): Self::Checkpoint, _timeout: Duration) {
      self.0.fetch_add(1, Ordering::SeqCst);
    }
  }

  #[test]
  fn empty_long_poll_reauthorizes_and_reclaims_after_waiting_without_store_state() {
    let registration = Arc::new(RegistrationStub {
      authority: authority(),
      calls: AtomicUsize::new(0),
    });
    let store = Arc::new(EmptyStore::default());
    let waiter = Arc::new(ImmediateWaiter::default());
    let service = AgentLeaseService::new(
      registration.clone(),
      store.clone(),
      waiter.clone(),
      Duration::from_secs(60),
    )
    .unwrap();

    assert_eq!(
      run_ready(service.acquire(input(true))).unwrap(),
      AgentLeaseOutcome::NoWork
    );
    assert_eq!(registration.calls.load(Ordering::SeqCst), 2);
    assert_eq!(store.claims.lock().unwrap().len(), 2);
    assert_eq!(waiter.0.load(Ordering::SeqCst), 1);
  }

  #[test]
  fn agent_admission_false_never_reaches_the_ready_queue() {
    let registration = Arc::new(RegistrationStub {
      authority: authority(),
      calls: AtomicUsize::new(0),
    });
    let store = Arc::new(EmptyStore::default());
    let waiter = Arc::new(ImmediateWaiter::default());
    let service = AgentLeaseService::new(registration, store.clone(), waiter.clone(), Duration::from_secs(60)).unwrap();

    assert_eq!(
      run_ready(service.acquire(input(false))).unwrap(),
      AgentLeaseOutcome::NoWork
    );
    assert!(store.claims.lock().unwrap().is_empty());
    assert_eq!(waiter.0.load(Ordering::SeqCst), 0);
  }

  fn authority() -> crate::AuthorizedAgent {
    crate::AuthorizedAgent::for_test(
      stable_agent_id("agent").unwrap(),
      RegistrationEpoch::new(1).unwrap(),
      PoolId::from_uuid(Uuid::from_u128(2)).unwrap(),
      octacity_protocol::HostCapacity {
        logical_cpu_count: 1,
        total_memory_bytes: 1,
        work_disk_total_bytes: 1,
        state_disk_total_bytes: 1,
        virtualization_available: false,
      },
      AgentOperation::Poll,
    )
  }

  fn input(accept_jobs: bool) -> AcquireAgentLeaseInput {
    AcquireAgentLeaseInput {
      route_agent_id: "agent".to_owned(),
      request: AcquireLeaseRequest {
        protocol_version: COORDINATOR_PROTOCOL_VERSION,
        request_id: "request-1".to_owned(),
        registration_id: "00000000-0000-0000-0000-000000000001".to_owned(),
        wait_seconds: 1,
        accept_jobs,
        snapshot: octacity_protocol::HostSnapshot {
          available_cpu_millis: 1_000,
          available_memory_bytes: 1,
          work_disk_free_bytes: 1,
          state_disk_free_bytes: 1,
          active_job: None,
          backends: Vec::new(),
        },
      },
      credential: AgentCredentialToken::parse(
        "registration.00000000-0000-0000-0000-000000000001.BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc",
      )
      .unwrap(),
      observed_at_unix_ms: 1_000,
    }
  }

  fn run_ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
      Poll::Ready(value) => value,
      Poll::Pending => panic!("in-memory placement test unexpectedly waited for external I/O"),
    }
  }
}
