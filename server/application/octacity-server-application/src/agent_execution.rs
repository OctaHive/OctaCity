use std::sync::Arc;

use async_trait::async_trait;
use octacity_protocol::{AgentCredentialToken, AppendEventsRequest, CompleteLeaseRequest, JobCompletionStatus};
use octacity_server_domain::EntityKind;
use octacity_server_job::JobFailureClass;
use octacity_server_store::{
  AppendJobEvents, DurableJobEvent, EventSequence, IdempotencyKey, JobCompletion, JobCompletionKind, JobEventKind,
  JobExecutionStore, StoreError,
};
use thiserror::Error;

use crate::{AgentOperation, AgentRegistrationError, AgentRegistrationUseCases, AuthorizeAgentInput, agent_lease};

/// Complete transport-independent input for one event append.
#[derive(Clone, Debug)]
pub struct AppendAgentEventsInput {
  /// Lease identity selected by the route.
  pub route_lease_id: String,
  /// Shared protocol request body.
  pub request: AppendEventsRequest,
  /// Current registration bearer.
  pub credential: AgentCredentialToken,
  /// Server-observed request arrival time.
  pub observed_at_unix_ms: i64,
}

/// Complete transport-independent input for one terminal completion.
#[derive(Clone, Debug)]
pub struct CompleteAgentLeaseInput {
  /// Lease identity selected by the route.
  pub route_lease_id: String,
  /// Shared protocol request body.
  pub request: CompleteLeaseRequest,
  /// Current registration bearer.
  pub credential: AgentCredentialToken,
  /// Server-observed request arrival time.
  pub observed_at_unix_ms: i64,
}

/// Application boundary used by durable Agent event and completion routes.
#[async_trait]
pub trait AgentExecutionUseCases: Send + Sync {
  /// Appends one contiguous fenced event batch and returns its durable cursor.
  async fn append_events(&self, input: AppendAgentEventsInput) -> Result<u64, AgentExecutionError>;

  /// Commits terminal state after the declared event cursor is durable.
  async fn complete_lease(&self, input: CompleteAgentLeaseInput) -> Result<(), AgentExecutionError>;
}

/// Store-backed event ingestion and terminal completion service.
pub struct AgentExecutionService<S> {
  registrations: Arc<dyn AgentRegistrationUseCases>,
  store: Arc<S>,
}

impl<S> AgentExecutionService<S> {
  /// Creates a service from current-registration authority and the atomic Job store.
  pub fn new(registrations: Arc<dyn AgentRegistrationUseCases>, store: Arc<S>) -> Self {
    Self { registrations, store }
  }
}

#[async_trait]
impl<S> AgentExecutionUseCases for AgentExecutionService<S>
where
  S: JobExecutionStore + 'static,
{
  async fn append_events(&self, input: AppendAgentEventsInput) -> Result<u64, AgentExecutionError> {
    input
      .request
      .validate()
      .map_err(|_| AgentExecutionError::InvalidRequest)?;
    if input.request.lease.lease_id != input.route_lease_id {
      return Err(AgentExecutionError::InvalidRequest);
    }
    let accepted_at =
      agent_lease::timestamp(input.observed_at_unix_ms).map_err(|()| AgentExecutionError::InvalidRequest)?;
    let authorized = self
      .authorize(
        AgentOperation::AppendEvents,
        &input.request.registration_id,
        input.credential,
        input.observed_at_unix_ms,
      )
      .await?;
    let lease =
      agent_lease::parse_lease(&input.request.lease, &authorized).map_err(|()| AgentExecutionError::InvalidRequest)?;
    let events = input
      .request
      .events
      .into_iter()
      .map(|event| {
        let sequence = EventSequence::new(event.stream_sequence).map_err(|_| AgentExecutionError::InvalidRequest)?;
        let occurred_at =
          agent_lease::timestamp(event.occurred_at_unix_ms).map_err(|()| AgentExecutionError::InvalidRequest)?;
        let kind = match &event.kind {
          octacity_protocol::AttemptEventKind::Runner { .. } => "runner",
          octacity_protocol::AttemptEventKind::Agent { .. } => "agent",
        };
        let kind = JobEventKind::new(kind).map_err(|_| AgentExecutionError::InvalidRequest)?;
        let payload = serde_json::to_value(event.kind).map_err(|_| AgentExecutionError::InvalidRequest)?;
        DurableJobEvent::new(sequence, kind, occurred_at, payload).map_err(|_| AgentExecutionError::InvalidRequest)
      })
      .collect::<Result<Vec<_>, _>>()?;
    let submitted_through = events
      .last()
      .expect("the shared protocol validates a non-empty event batch")
      .sequence()
      .get();
    let request = AppendJobEvents::new(lease.access, events, accepted_at)?;
    self
      .store
      .append_job_events(request)
      .await
      .map(|outcome| outcome.acknowledged_through.get().min(submitted_through))
      .map_err(AgentExecutionError::from)
  }

  async fn complete_lease(&self, input: CompleteAgentLeaseInput) -> Result<(), AgentExecutionError> {
    input
      .request
      .validate()
      .map_err(|_| AgentExecutionError::InvalidRequest)?;
    if input.request.lease.lease_id != input.route_lease_id {
      return Err(AgentExecutionError::InvalidRequest);
    }
    let completed_at =
      agent_lease::timestamp(input.observed_at_unix_ms).map_err(|()| AgentExecutionError::InvalidRequest)?;
    let authorized = self
      .authorize(
        AgentOperation::Complete,
        &input.request.registration_id,
        input.credential,
        input.observed_at_unix_ms,
      )
      .await?;
    let lease =
      agent_lease::parse_lease(&input.request.lease, &authorized).map_err(|()| AgentExecutionError::InvalidRequest)?;
    let final_sequence =
      EventSequence::new(input.request.last_event_sequence).map_err(|_| AgentExecutionError::InvalidRequest)?;
    let completion_id =
      IdempotencyKey::new(input.request.completion_id).map_err(|_| AgentExecutionError::InvalidRequest)?;
    let kind = match input.request.status {
      JobCompletionStatus::Succeeded => JobCompletionKind::Succeeded,
      JobCompletionStatus::Failed | JobCompletionStatus::TimedOut => {
        JobCompletionKind::Failed(JobFailureClass::Execution)
      }
      JobCompletionStatus::InfrastructureFailed => JobCompletionKind::Failed(JobFailureClass::Infrastructure),
      JobCompletionStatus::Cancelled => JobCompletionKind::Cancelled,
    };
    self
      .store
      .complete_job(JobCompletion {
        completion_id,
        lease: lease.access,
        final_sequence: Some(final_sequence),
        kind,
        completed_at,
      })
      .await?;
    Ok(())
  }
}

impl<S> AgentExecutionService<S> {
  async fn authorize(
    &self,
    operation: AgentOperation,
    registration_id: &str,
    credential: AgentCredentialToken,
    observed_at_unix_ms: i64,
  ) -> Result<crate::AuthorizedAgent, AgentExecutionError> {
    self
      .registrations
      .authorize(AuthorizeAgentInput {
        operation,
        registration_id: registration_id.to_owned(),
        credential,
        observed_at_unix_ms,
      })
      .await
      .map_err(AgentExecutionError::from)
  }
}

/// Stable failures understood by the Agent event and completion adapter.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AgentExecutionError {
  /// The route, shared DTO, event content, or timestamp is invalid.
  #[error("invalid Agent execution request")]
  InvalidRequest,
  /// The registration bearer was rejected.
  #[error("agent credential rejected")]
  CredentialRejected,
  /// The registration or Lease no longer owns the Job.
  #[error("lease is fenced")]
  Fenced,
  /// The current Lease reached its authoritative deadline.
  #[error("lease is expired")]
  Expired,
  /// An event range begins after the next required sequence.
  #[error("event gap: expected {expected}, received {actual}")]
  EventGap {
    /// Next durable sequence required by the server.
    expected: u64,
    /// First new sequence supplied by the Agent.
    actual: u64,
  },
  /// Completion raced ahead of the declared durable event cursor.
  #[error("events missing: durable through {durable_through}, required {required}")]
  EventsMissing {
    /// Greatest sequence durable at the completion transaction.
    durable_through: u64,
    /// Final sequence required by the Agent.
    required: u64,
  },
  /// Durable state conflicts with an earlier append or completion.
  #[error("Agent execution request conflicts with durable state")]
  Conflict,
  /// The authoritative operation could not safely complete.
  #[error("Agent execution service unavailable")]
  Unavailable,
}

impl From<AgentRegistrationError> for AgentExecutionError {
  fn from(value: AgentRegistrationError) -> Self {
    match value {
      AgentRegistrationError::InvalidRequest => Self::InvalidRequest,
      AgentRegistrationError::CredentialRejected => Self::CredentialRejected,
      AgentRegistrationError::Fenced => Self::Fenced,
      AgentRegistrationError::Unavailable => Self::Unavailable,
    }
  }
}

impl From<StoreError> for AgentExecutionError {
  fn from(value: StoreError) -> Self {
    match value {
      StoreError::InvalidInput { .. } => Self::InvalidRequest,
      StoreError::CredentialRejected => Self::CredentialRejected,
      StoreError::Fenced { .. } => Self::Fenced,
      StoreError::Expired { .. } => Self::Expired,
      StoreError::EventGap { expected, actual, .. } => Self::EventGap { expected, actual },
      StoreError::EventsMissing {
        durable_through,
        required,
        ..
      } => Self::EventsMissing {
        durable_through,
        required,
      },
      StoreError::Conflict {
        entity: EntityKind::Job,
      }
      | StoreError::Duplicate {
        entity: EntityKind::Job,
      } => Self::Conflict,
      StoreError::NotFound { .. }
      | StoreError::Conflict { .. }
      | StoreError::Duplicate { .. }
      | StoreError::Unavailable => Self::Unavailable,
    }
  }
}

#[cfg(test)]
mod tests {
  use std::{
    future::Future,
    sync::Mutex,
    task::{Context, Poll, Waker},
  };

  use octacity_protocol::{
    AgentLifecycleEvent, AttemptEventEnvelope, AttemptEventKind, COORDINATOR_PROTOCOL_VERSION, JobLifecycleState,
    LeaseFence as ProtocolLeaseFence,
  };
  use octacity_server_domain::{AgentId, JobId, LeaseId, PoolId};
  use octacity_server_orchestrator::{AttemptState, BuildState};
  use octacity_server_store::{
    AppendJobEventsOutcome, CompletionDisposition, JobClaim, JobClaimOutcome, MutationDisposition, RegistrationEpoch,
  };
  use uuid::Uuid;

  use super::*;

  struct RegistrationStub {
    operations: Mutex<Vec<AgentOperation>>,
  }

  #[async_trait]
  impl AgentRegistrationUseCases for RegistrationStub {
    async fn register(
      &self,
      _input: crate::AgentRegistrationInput,
    ) -> Result<crate::AgentRegistrationOutcome, AgentRegistrationError> {
      unreachable!("execution routes do not register Agents")
    }

    async fn authorize(&self, input: AuthorizeAgentInput) -> Result<crate::AuthorizedAgent, AgentRegistrationError> {
      self.operations.lock().unwrap().push(input.operation);
      Ok(crate::AuthorizedAgent::for_test(
        AgentId::from_uuid(Uuid::from_u128(1)).unwrap(),
        RegistrationEpoch::new(7).unwrap(),
        PoolId::from_uuid(Uuid::from_u128(2)).unwrap(),
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

  #[derive(Default)]
  struct RecordingStore {
    appends: Mutex<Vec<AppendJobEvents>>,
    completions: Mutex<Vec<JobCompletion>>,
  }

  #[async_trait]
  impl JobExecutionStore for RecordingStore {
    async fn claim_ready_job(&self, _request: JobClaim) -> Result<JobClaimOutcome, StoreError> {
      unreachable!("execution routes do not place Jobs")
    }

    async fn append_job_events(&self, request: AppendJobEvents) -> Result<AppendJobEventsOutcome, StoreError> {
      let mut appends = self.appends.lock().unwrap();
      let inserted = usize::from(appends.is_empty());
      let acknowledged_through = if appends.is_empty() {
        request.events.last().unwrap().sequence()
      } else {
        EventSequence::new(3).unwrap()
      };
      appends.push(request);
      Ok(AppendJobEventsOutcome {
        acknowledged_through,
        inserted,
      })
    }

    async fn complete_job(&self, request: JobCompletion) -> Result<CompletionDisposition, StoreError> {
      let failure_class = request.kind.failure_class();
      self.completions.lock().unwrap().push(request);
      Ok(CompletionDisposition {
        disposition: MutationDisposition::Applied,
        job_id: request_lease_job(),
        failure_class,
        ready_jobs: vec![JobId::from_uuid(Uuid::from_u128(6)).unwrap()],
        ready_pools: std::collections::BTreeSet::from([PoolId::from_uuid(Uuid::from_u128(7)).unwrap()]),
        skipped_jobs: Vec::new(),
        attempt_state: AttemptState::Succeeded,
        build_state: BuildState::Succeeded,
      })
    }
  }

  #[test]
  fn lost_append_ack_rebuilds_the_same_durable_event_identity() {
    let registrations = Arc::new(RegistrationStub {
      operations: Mutex::new(Vec::new()),
    });
    let store = Arc::new(RecordingStore::default());
    let service = AgentExecutionService::new(registrations.clone(), store.clone());
    let first = run_ready(service.append_events(append_input(2_000))).unwrap();
    let replay = run_ready(service.append_events(append_input(9_000))).unwrap();

    assert_eq!((first, replay), (1, 1));
    let appends = store.appends.lock().unwrap();
    assert_ne!(appends[0].accepted_at, appends[1].accepted_at);
    assert_eq!(appends[0].events, appends[1].events);
    assert_eq!(
      *registrations.operations.lock().unwrap(),
      [AgentOperation::AppendEvents, AgentOperation::AppendEvents]
    );
  }

  #[test]
  fn completion_maps_the_typed_failure_class() {
    let registrations = Arc::new(RegistrationStub {
      operations: Mutex::new(Vec::new()),
    });
    let store = Arc::new(RecordingStore::default());
    let service = AgentExecutionService::new(registrations.clone(), store.clone());

    run_ready(service.complete_lease(completion_input())).unwrap();

    let completions = store.completions.lock().unwrap();
    assert_eq!(completions.len(), 1);
    assert_eq!(
      completions[0].kind,
      JobCompletionKind::Failed(JobFailureClass::Execution)
    );
    assert_eq!(completions[0].final_sequence.unwrap().get(), 1);
    assert_eq!(*registrations.operations.lock().unwrap(), [AgentOperation::Complete]);
  }

  #[test]
  fn sequencing_and_completion_race_failures_keep_stable_meaning() {
    let job = request_lease_job();
    assert_eq!(
      AgentExecutionError::from(StoreError::EventGap {
        job,
        expected: 2,
        actual: 4,
      }),
      AgentExecutionError::EventGap { expected: 2, actual: 4 }
    );
    assert_eq!(
      AgentExecutionError::from(StoreError::EventsMissing {
        job,
        durable_through: 2,
        required: 3,
      }),
      AgentExecutionError::EventsMissing {
        durable_through: 2,
        required: 3,
      }
    );
    assert_eq!(
      AgentExecutionError::from(StoreError::Conflict {
        entity: EntityKind::Job,
      }),
      AgentExecutionError::Conflict
    );
  }

  fn append_input(observed_at_unix_ms: i64) -> AppendAgentEventsInput {
    let lease = protocol_lease();
    AppendAgentEventsInput {
      route_lease_id: lease.lease_id.clone(),
      request: AppendEventsRequest {
        protocol_version: COORDINATOR_PROTOCOL_VERSION,
        request_id: "append-1".to_owned(),
        registration_id: "registration-7".to_owned(),
        lease: lease.clone(),
        events: vec![AttemptEventEnvelope {
          job_id: lease.job_id,
          attempt: lease.attempt,
          lease_id: lease.lease_id,
          fencing_token: lease.fencing_token,
          stream_sequence: 1,
          occurred_at_unix_ms: 1_234,
          kind: AttemptEventKind::Agent {
            event: AgentLifecycleEvent::StateChanged {
              state: JobLifecycleState::Preparing,
            },
          },
        }],
      },
      credential: credential(),
      observed_at_unix_ms,
    }
  }

  fn completion_input() -> CompleteAgentLeaseInput {
    let lease = protocol_lease();
    CompleteAgentLeaseInput {
      route_lease_id: lease.lease_id.clone(),
      request: CompleteLeaseRequest {
        protocol_version: COORDINATOR_PROTOCOL_VERSION,
        request_id: "complete-1".to_owned(),
        registration_id: "registration-7".to_owned(),
        lease,
        completion_id: "completion-1".to_owned(),
        last_event_sequence: 1,
        status: JobCompletionStatus::TimedOut,
        final_usage: None,
        results: Vec::new(),
      },
      credential: credential(),
      observed_at_unix_ms: 2_000,
    }
  }

  fn protocol_lease() -> ProtocolLeaseFence {
    ProtocolLeaseFence {
      lease_id: LeaseId::from_uuid(Uuid::from_u128(3)).unwrap().to_string(),
      job_id: request_lease_job().to_string(),
      attempt: 1,
      fencing_token: "07".repeat(32),
    }
  }

  fn request_lease_job() -> JobId {
    JobId::from_uuid(Uuid::from_u128(5)).unwrap()
  }

  fn credential() -> AgentCredentialToken {
    AgentCredentialToken::parse(
      "registration.00000000-0000-0000-0000-000000000004.BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc",
    )
    .unwrap()
  }

  fn run_ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
      Poll::Ready(value) => value,
      Poll::Pending => panic!("in-memory execution test unexpectedly waited for external I/O"),
    }
  }
}
