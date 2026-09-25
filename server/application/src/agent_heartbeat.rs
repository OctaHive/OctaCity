use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use octacity_observability::{ErrorClass, Operation, Outcome, ServerOperationMetric};
use octacity_protocol::{AgentCredentialToken, COORDINATOR_PROTOCOL_VERSION, HeartbeatRequest};
use octacity_server_domain::Timestamp;
use octacity_server_store::{IdempotencyKey, LeaseHeartbeatOutcome, LeaseHeartbeatStore, RenewLease, StoreError};
use thiserror::Error;

use crate::{AgentOperation, AgentRegistrationError, AgentRegistrationUseCases, AuthorizeAgentInput, agent_lease};

/// Complete transport-independent input for one Lease heartbeat.
#[derive(Clone, Debug)]
pub struct AgentHeartbeatInput {
  /// Lease identity selected by the route.
  pub route_lease_id: String,
  /// Shared protocol request body.
  pub request: HeartbeatRequest,
  /// Current registration bearer.
  pub credential: AgentCredentialToken,
  /// Server-observed request arrival time.
  pub observed_at_unix_ms: i64,
}

/// Application boundary used by the authenticated heartbeat HTTP adapter.
#[async_trait]
pub trait AgentHeartbeatUseCases: Send + Sync {
  /// Authenticates current ownership and independently renews or controls a Lease.
  async fn heartbeat(&self, input: AgentHeartbeatInput) -> Result<LeaseHeartbeatOutcome, AgentHeartbeatError>;
}

/// Store-backed heartbeat service isolated from event and telemetry ingestion.
pub struct AgentHeartbeatService<S> {
  registrations: Arc<dyn AgentRegistrationUseCases>,
  store: Arc<S>,
  lease_lifetime: Duration,
}

impl<S> AgentHeartbeatService<S> {
  /// Creates a heartbeat service with an explicit server-controlled Lease lifetime.
  pub fn new(
    registrations: Arc<dyn AgentRegistrationUseCases>,
    store: Arc<S>,
    lease_lifetime: Duration,
  ) -> Result<Self, AgentHeartbeatError> {
    if lease_lifetime.is_zero() || lease_lifetime.as_millis() > i64::MAX as u128 {
      return Err(AgentHeartbeatError::InvalidRequest);
    }
    Ok(Self {
      registrations,
      store,
      lease_lifetime,
    })
  }
}

#[async_trait]
impl<S> AgentHeartbeatUseCases for AgentHeartbeatService<S>
where
  S: LeaseHeartbeatStore + 'static,
{
  async fn heartbeat(&self, input: AgentHeartbeatInput) -> Result<LeaseHeartbeatOutcome, AgentHeartbeatError> {
    crate::telemetry::observe_classified(
      ServerOperationMetric::Lease,
      Operation::Renew,
      async {
        if input.request.protocol_version != COORDINATOR_PROTOCOL_VERSION
          || input.request.lease.lease_id != input.route_lease_id
        {
          return Err(AgentHeartbeatError::InvalidRequest);
        }
        let observed_at = timestamp(input.observed_at_unix_ms)?;
        let idempotency_key =
          IdempotencyKey::new(input.request.request_id.clone()).map_err(|_| AgentHeartbeatError::InvalidRequest)?;

        let authorized = match self
          .registrations
          .authorize(AuthorizeAgentInput {
            operation: AgentOperation::Heartbeat,
            registration_id: input.request.registration_id.clone(),
            credential: input.credential,
            observed_at_unix_ms: input.observed_at_unix_ms,
          })
          .await
        {
          Ok(authorized) => authorized,
          Err(AgentRegistrationError::Unavailable) => return Err(AgentHeartbeatError::Unavailable),
          Err(AgentRegistrationError::InvalidRequest) => return Err(AgentHeartbeatError::InvalidRequest),
          Err(AgentRegistrationError::CredentialRejected | AgentRegistrationError::Fenced) => {
            return Ok(LeaseHeartbeatOutcome::Fenced);
          }
        };
        input
          .request
          .validate(authorized.host_capacity())
          .map_err(|_| AgentHeartbeatError::InvalidRequest)?;
        let parsed = agent_lease::parse_lease(&input.request.lease, &authorized)
          .map_err(|()| AgentHeartbeatError::InvalidRequest)?;
        let lifetime =
          i64::try_from(self.lease_lifetime.as_millis()).map_err(|_| AgentHeartbeatError::InvalidRequest)?;
        let expires_at = timestamp(
          input
            .observed_at_unix_ms
            .checked_add(lifetime)
            .ok_or(AgentHeartbeatError::InvalidRequest)?,
        )?;
        self
          .store
          .renew_lease(RenewLease {
            idempotency_key,
            lease: parsed.access,
            job_id: parsed.job_id,
            attempt: parsed.attempt,
            observed_at,
            expires_at,
          })
          .await
          .map_err(AgentHeartbeatError::from)
      },
      |result| match result {
        Ok(LeaseHeartbeatOutcome::Fenced) => (Outcome::Rejected, Some(ErrorClass::Fenced)),
        Ok(_) => (Outcome::Success, None),
        Err(AgentHeartbeatError::InvalidRequest) => (Outcome::Rejected, Some(ErrorClass::Invalid)),
        Err(AgentHeartbeatError::Unavailable) => (Outcome::Failure, Some(ErrorClass::Unavailable)),
      },
    )
    .await
  }
}

/// Stable failures understood by the Agent heartbeat adapter.
#[derive(Debug, Error)]
pub enum AgentHeartbeatError {
  /// The route, shared DTO, fence, or server timing policy is invalid.
  #[error("invalid lease heartbeat request")]
  InvalidRequest,
  /// The heartbeat store could not safely complete the operation.
  #[error("lease heartbeat unavailable")]
  Unavailable,
}

impl From<StoreError> for AgentHeartbeatError {
  fn from(value: StoreError) -> Self {
    match value {
      StoreError::InvalidInput { .. } | StoreError::Conflict { .. } => Self::InvalidRequest,
      _ => Self::Unavailable,
    }
  }
}

fn timestamp(value: i64) -> Result<Timestamp, AgentHeartbeatError> {
  agent_lease::timestamp(value).map_err(|()| AgentHeartbeatError::InvalidRequest)
}

#[cfg(test)]
mod tests {
  use std::{
    future::Future,
    sync::atomic::{AtomicUsize, Ordering},
    task::{Context, Poll, Waker},
  };

  use octacity_protocol::{HostCapacity, HostSnapshot, LeaseFence as ProtocolLeaseFence};
  use octacity_server_domain::{AgentId, PoolId};
  use octacity_server_store::{
    AppendJobEvents, AppendJobEventsOutcome, CompletionDisposition, JobClaim, JobClaimOutcome, JobCompletion,
    JobExecutionStore, RegistrationEpoch,
  };
  use uuid::Uuid;

  use super::*;

  struct RegistrationStub(crate::AuthorizedAgent);

  #[async_trait]
  impl AgentRegistrationUseCases for RegistrationStub {
    async fn register(
      &self,
      _input: crate::AgentRegistrationInput,
    ) -> Result<crate::AgentRegistrationOutcome, AgentRegistrationError> {
      unreachable!("heartbeat does not register an Agent")
    }

    async fn authorize(&self, input: AuthorizeAgentInput) -> Result<crate::AuthorizedAgent, AgentRegistrationError> {
      assert_eq!(input.operation, AgentOperation::Heartbeat);
      Ok(self.0.clone())
    }
  }

  struct BackpressuredEventStore {
    renewals: AtomicUsize,
    event_appends: AtomicUsize,
  }

  #[async_trait]
  impl LeaseHeartbeatStore for BackpressuredEventStore {
    async fn renew_lease(&self, request: RenewLease) -> Result<LeaseHeartbeatOutcome, StoreError> {
      self.renewals.fetch_add(1, Ordering::SeqCst);
      Ok(LeaseHeartbeatOutcome::Continue {
        expires_at: request.expires_at,
      })
    }
  }

  #[async_trait]
  impl JobExecutionStore for BackpressuredEventStore {
    async fn claim_ready_job(&self, _request: JobClaim) -> Result<JobClaimOutcome, StoreError> {
      unreachable!("heartbeat does not perform placement")
    }

    async fn append_job_events(&self, _request: AppendJobEvents) -> Result<AppendJobEventsOutcome, StoreError> {
      self.event_appends.fetch_add(1, Ordering::SeqCst);
      Err(StoreError::Unavailable)
    }

    async fn complete_job(&self, _request: JobCompletion) -> Result<CompletionDisposition, StoreError> {
      unreachable!("heartbeat does not complete Jobs")
    }
  }

  struct DirectiveStore(LeaseHeartbeatOutcome);

  #[async_trait]
  impl LeaseHeartbeatStore for DirectiveStore {
    async fn renew_lease(&self, _request: RenewLease) -> Result<LeaseHeartbeatOutcome, StoreError> {
      Ok(self.0)
    }
  }

  #[test]
  fn event_ingestion_backpressure_is_not_on_the_heartbeat_path() {
    let store = Arc::new(BackpressuredEventStore {
      renewals: AtomicUsize::new(0),
      event_appends: AtomicUsize::new(0),
    });
    let service = AgentHeartbeatService::new(registration(), store.clone(), Duration::from_secs(60)).unwrap();

    let outcome = run_ready(service.heartbeat(input())).unwrap();

    assert!(matches!(outcome, LeaseHeartbeatOutcome::Continue { .. }));
    assert_eq!(store.renewals.load(Ordering::SeqCst), 1);
    assert_eq!(store.event_appends.load(Ordering::SeqCst), 0);
  }

  #[test]
  fn application_preserves_every_store_directive() {
    let expires_at = Timestamp::from_unix_millis(61_000).unwrap();
    for expected in [
      LeaseHeartbeatOutcome::Continue { expires_at },
      LeaseHeartbeatOutcome::Cancel,
      LeaseHeartbeatOutcome::Fenced,
      LeaseHeartbeatOutcome::Drain { expires_at },
    ] {
      let service = AgentHeartbeatService::new(
        registration(),
        Arc::new(DirectiveStore(expected)),
        Duration::from_secs(60),
      )
      .unwrap();
      assert_eq!(run_ready(service.heartbeat(input())).unwrap(), expected);
    }
  }

  fn registration() -> Arc<dyn AgentRegistrationUseCases> {
    Arc::new(RegistrationStub(crate::AuthorizedAgent::for_test(
      AgentId::from_uuid(Uuid::from_u128(1)).unwrap(),
      RegistrationEpoch::new(1).unwrap(),
      PoolId::from_uuid(Uuid::from_u128(2)).unwrap(),
      capacity(),
      AgentOperation::Heartbeat,
    )))
  }

  fn input() -> AgentHeartbeatInput {
    AgentHeartbeatInput {
      route_lease_id: "00000000-0000-0000-0000-000000000003".to_owned(),
      request: HeartbeatRequest {
        protocol_version: COORDINATOR_PROTOCOL_VERSION,
        request_id: "heartbeat-1".to_owned(),
        registration_id: "00000000-0000-0000-0000-000000000004".to_owned(),
        lease: ProtocolLeaseFence {
          lease_id: "00000000-0000-0000-0000-000000000003".to_owned(),
          job_id: "00000000-0000-0000-0000-000000000005".to_owned(),
          attempt: 1,
          fencing_token: "07".repeat(32),
        },
        snapshot: HostSnapshot {
          available_cpu_millis: 1_000,
          available_memory_bytes: 1,
          work_disk_free_bytes: 1,
          state_disk_free_bytes: 1,
          active_job: None,
          backends: Vec::new(),
        },
      },
      credential: AgentCredentialToken::parse(
        "registration.00000000-0000-0000-0000-000000000004.BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc",
      )
      .unwrap(),
      observed_at_unix_ms: 1_000,
    }
  }

  fn capacity() -> HostCapacity {
    HostCapacity {
      logical_cpu_count: 1,
      total_memory_bytes: 1,
      work_disk_total_bytes: 1,
      state_disk_total_bytes: 1,
      virtualization_available: false,
    }
  }

  fn run_ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
      Poll::Ready(value) => value,
      Poll::Pending => panic!("in-memory heartbeat test unexpectedly waited for external I/O"),
    }
  }
}
