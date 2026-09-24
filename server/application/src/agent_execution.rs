use std::sync::Arc;

use async_trait::async_trait;
use octacity_artifact_store::LogChunkStore;
use octacity_protocol::{AgentCredentialToken, AppendEventsRequest, CompleteLeaseRequest, JobCompletionStatus};
use octacity_server_domain::EntityKind;
use octacity_server_job::JobFailureClass;
use octacity_server_store::{
  AppendJobEvents, DurableJobEvent, EventSequence, IdempotencyKey, JobCompletion, JobCompletionKind, JobEventKind,
  JobExecutionStore, StoreError,
};
use thiserror::Error;

use crate::log_archive::{LogRedactor, durable_event_kind, map_log_store_error, prepare_log_archive};
use crate::{AgentOperation, AgentRegistrationError, AgentRegistrationUseCases, agent_lease};

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
  log_objects: Option<Arc<dyn LogChunkStore>>,
  log_redactor: LogRedactor,
}

impl<S> AgentExecutionService<S> {
  /// Creates a service from current-registration authority and the atomic Job store.
  pub fn new(registrations: Arc<dyn AgentRegistrationUseCases>, store: Arc<S>) -> Self {
    Self {
      registrations,
      store,
      log_objects: None,
      log_redactor: LogRedactor::default(),
    }
  }

  /// Creates a service that archives and verifies stdout/stderr before acknowledgement.
  pub fn with_log_archive(
    registrations: Arc<dyn AgentRegistrationUseCases>,
    store: Arc<S>,
    log_objects: Arc<dyn LogChunkStore>,
    log_redactor: LogRedactor,
  ) -> Self {
    Self {
      registrations,
      store,
      log_objects: Some(log_objects),
      log_redactor,
    }
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
    let context = agent_lease::authorize_operation(
      self.registrations.as_ref(),
      AgentOperation::AppendEvents,
      &input.route_lease_id,
      &input.request.lease,
      &input.request.registration_id,
      input.credential,
      input.observed_at_unix_ms,
    )
    .await
    .map_err(AgentExecutionError::from)?;
    let accepted_at = context.observed_at;
    let lease = context.lease;
    let has_log_output = input.request.events.iter().any(|envelope| {
      matches!(
        &envelope.kind,
        octacity_protocol::AttemptEventKind::Runner { event }
          if event.data.get("type").and_then(serde_json::Value::as_str) == Some("output")
      )
    });
    let durable_through = if has_log_output {
      self
        .store
        .prepare_job_event_append(lease.access, accepted_at)
        .await?
        .durable_through
    } else {
      0
    };
    let prepared = prepare_log_archive(input.request.events, durable_through, lease.job_id, &self.log_redactor)?;
    if !prepared.chunks.is_empty() {
      let objects = self.log_objects.as_ref().ok_or(AgentExecutionError::Unavailable)?;
      for (manifest, bytes) in &prepared.chunks {
        objects
          .put_verified(manifest, bytes.clone())
          .await
          .map_err(map_log_store_error)?;
      }
    }
    let log_chunks = prepared.chunks.iter().map(|(manifest, _)| manifest.clone()).collect();
    let events = prepared
      .events
      .into_iter()
      .map(|event| {
        let sequence = EventSequence::new(event.stream_sequence).map_err(|_| AgentExecutionError::InvalidRequest)?;
        let occurred_at =
          agent_lease::timestamp(event.occurred_at_unix_ms).map_err(|()| AgentExecutionError::InvalidRequest)?;
        let kind = durable_event_kind(&event);
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
    let request = AppendJobEvents::new(lease.access, events, accepted_at)?.with_log_chunks(log_chunks)?;
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
    let context = agent_lease::authorize_operation(
      self.registrations.as_ref(),
      AgentOperation::Complete,
      &input.route_lease_id,
      &input.request.lease,
      &input.request.registration_id,
      input.credential,
      input.observed_at_unix_ms,
    )
    .await
    .map_err(AgentExecutionError::from)?;
    let completed_at = context.observed_at;
    let lease = context.lease;
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

impl From<agent_lease::AuthorizationError> for AgentExecutionError {
  fn from(value: agent_lease::AuthorizationError) -> Self {
    match value {
      agent_lease::AuthorizationError::InvalidRequest => Self::InvalidRequest,
      agent_lease::AuthorizationError::Registration(error) => error.into(),
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
    collections::BTreeMap,
    future::Future,
    sync::{
      Mutex,
      atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Waker},
  };

  use base64::{Engine as _, engine::general_purpose::STANDARD};
  use octacity_artifact_store::{LogChunkStoreError, LogChunkWrite};
  use octacity_protocol::{
    AgentLifecycleEvent, AttemptEventEnvelope, AttemptEventKind, COORDINATOR_PROTOCOL_VERSION, JobLifecycleState,
    LeaseFence as ProtocolLeaseFence, RunnerEventPayload,
  };
  use octacity_server_domain::{AgentId, JobId, LeaseId, LogChunkId, PoolId};
  use octacity_server_orchestrator::{AttemptState, BuildState};
  use octacity_server_store::{
    AppendJobEventsOutcome, CompletionDisposition, JobClaim, JobClaimOutcome, JobEventAppendPreparation, LeaseAccess,
    LogChunkManifest, MutationDisposition, RegistrationEpoch,
  };
  use uuid::Uuid;

  use super::*;
  use crate::AuthorizeAgentInput;

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
    fail_append: AtomicBool,
  }

  #[async_trait]
  impl JobExecutionStore for RecordingStore {
    async fn claim_ready_job(&self, _request: JobClaim) -> Result<JobClaimOutcome, StoreError> {
      unreachable!("execution routes do not place Jobs")
    }

    async fn append_job_events(&self, request: AppendJobEvents) -> Result<AppendJobEventsOutcome, StoreError> {
      if self.fail_append.load(Ordering::Acquire) {
        return Err(StoreError::Unavailable);
      }
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

    async fn prepare_job_event_append(
      &self,
      _lease: LeaseAccess,
      _accepted_at: octacity_server_domain::Timestamp,
    ) -> Result<JobEventAppendPreparation, StoreError> {
      let durable_through = self
        .appends
        .lock()
        .unwrap()
        .last()
        .map_or(0, |request| request.events.last().unwrap().sequence().get());
      Ok(JobEventAppendPreparation { durable_through })
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

  #[derive(Default)]
  struct MemoryLogObjects {
    unavailable: AtomicBool,
    objects: Mutex<BTreeMap<LogChunkId, (LogChunkManifest, Vec<u8>)>>,
  }

  #[async_trait]
  impl LogChunkStore for MemoryLogObjects {
    async fn put_verified(
      &self,
      manifest: &LogChunkManifest,
      bytes: Vec<u8>,
    ) -> Result<LogChunkWrite, LogChunkStoreError> {
      if self.unavailable.load(Ordering::Acquire) {
        return Err(LogChunkStoreError::Unavailable);
      }
      manifest.verify(&bytes).map_err(|_| LogChunkStoreError::Integrity)?;
      let mut objects = self.objects.lock().unwrap();
      match objects.get(&manifest.chunk_id()) {
        Some((existing_manifest, existing_bytes)) if existing_manifest == manifest && existing_bytes == &bytes => {
          Ok(LogChunkWrite::AlreadyPresent)
        }
        Some(_) => Err(LogChunkStoreError::Integrity),
        None => {
          objects.insert(manifest.chunk_id(), (manifest.clone(), bytes));
          Ok(LogChunkWrite::Written)
        }
      }
    }

    async fn read_verified(&self, manifest: &LogChunkManifest) -> Result<Vec<u8>, LogChunkStoreError> {
      let objects = self.objects.lock().unwrap();
      let (_, bytes) = objects.get(&manifest.chunk_id()).ok_or(LogChunkStoreError::NotFound)?;
      manifest.verify(bytes).map_err(|_| LogChunkStoreError::Integrity)?;
      Ok(bytes.clone())
    }

    async fn delete_chunk(&self, manifest: &LogChunkManifest) -> Result<(), LogChunkStoreError> {
      self.objects.lock().unwrap().remove(&manifest.chunk_id());
      Ok(())
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
  fn stdout_is_redacted_and_verified_before_the_atomic_acknowledgement() {
    let registrations = Arc::new(RegistrationStub {
      operations: Mutex::new(Vec::new()),
    });
    let store = Arc::new(RecordingStore::default());
    let objects = Arc::new(MemoryLogObjects::default());
    let service = AgentExecutionService::with_log_archive(
      registrations,
      store.clone(),
      objects.clone(),
      LogRedactor::new([b"private-token".to_vec()]).unwrap(),
    );

    assert_eq!(run_ready(service.append_events(output_input())), Ok(2));
    let appends = store.appends.lock().unwrap();
    assert_eq!(appends[0].log_chunks.len(), 1);
    assert_eq!(appends[0].log_chunks[0].first_sequence(), 1);
    assert_eq!(appends[0].log_chunks[0].last_sequence(), 2);
    let payload = serde_json::to_string(appends[0].events[0].payload()).unwrap();
    assert!(!payload.contains("private-token"));
    let archived = objects.objects.lock().unwrap();
    let (_, bytes) = archived.get(&appends[0].log_chunks[0].chunk_id()).unwrap();
    assert!(!String::from_utf8_lossy(bytes).contains("private-token"));
  }

  #[test]
  fn object_outage_and_database_rollback_never_acknowledge_output() {
    let registrations = Arc::new(RegistrationStub {
      operations: Mutex::new(Vec::new()),
    });
    let store = Arc::new(RecordingStore::default());
    let objects = Arc::new(MemoryLogObjects::default());
    let service =
      AgentExecutionService::with_log_archive(registrations, store.clone(), objects.clone(), LogRedactor::default());

    objects.unavailable.store(true, Ordering::Release);
    assert_eq!(
      run_ready(service.append_events(output_input())),
      Err(AgentExecutionError::Unavailable)
    );
    assert!(store.appends.lock().unwrap().is_empty());
    objects.unavailable.store(false, Ordering::Release);
    store.fail_append.store(true, Ordering::Release);
    assert_eq!(
      run_ready(service.append_events(output_input())),
      Err(AgentExecutionError::Unavailable)
    );
    assert_eq!(
      objects.objects.lock().unwrap().len(),
      1,
      "rollback leaves one invisible orphan"
    );
    assert!(store.appends.lock().unwrap().is_empty());
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

  fn output_input() -> AppendAgentEventsInput {
    let lease = protocol_lease();
    let output = |sequence, bytes: &[u8]| AttemptEventEnvelope {
      job_id: lease.job_id.clone(),
      attempt: lease.attempt,
      lease_id: lease.lease_id.clone(),
      fencing_token: lease.fencing_token.clone(),
      stream_sequence: sequence,
      occurred_at_unix_ms: 1_000 + sequence as i64,
      kind: AttemptEventKind::Runner {
        event: RunnerEventPayload {
          schema_version: 4,
          sequence,
          timestamp: "2026-09-24T00:00:00Z".to_owned(),
          category: "execution".to_owned(),
          data: serde_json::json!({
            "type": "output",
            "stream": "stdout",
            "payload": {"format": "bytes", "data": STANDARD.encode(bytes)}
          })
          .as_object()
          .unwrap()
          .clone(),
        },
      },
    };
    let events = vec![output(1, b"private-"), output(2, b"token\n")];
    AppendAgentEventsInput {
      route_lease_id: lease.lease_id.clone(),
      request: AppendEventsRequest {
        protocol_version: COORDINATOR_PROTOCOL_VERSION,
        request_id: "append-output".to_owned(),
        registration_id: "registration-7".to_owned(),
        lease,
        events,
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
