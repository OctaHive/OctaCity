use std::sync::{
  Arc,
  atomic::{AtomicBool, Ordering},
};

use async_trait::async_trait;
use axum::{
  body::{Body, to_bytes},
  http::{Request, StatusCode, header},
};
use octacity_protocol::{
  AcquireLeaseRequest, AcquireLeaseResponse, AppendEventsRequest, AppendEventsResponse, CompleteLeaseRequest,
  CompleteLeaseResponse, CoordinatorErrorResponse, HeartbeatDirective, HeartbeatRequest, HeartbeatResponse,
  HostSnapshot, RegisterAgentRequest, RegisterAgentResponse,
};
use octacity_server_api_agent::{AgentApiConfig, agent_router};
use octacity_server_application::{
  AcquireAgentLeaseInput, AgentExecutionError, AgentExecutionUseCases, AgentHeartbeatError, AgentHeartbeatInput,
  AgentHeartbeatUseCases, AgentLeaseError, AgentLeaseOutcome, AgentLeaseUseCases, AgentRegistrationError,
  AgentRegistrationInput, AgentRegistrationOutcome, AgentRegistrationUseCases, AppendAgentEventsInput,
  AuthorizeAgentInput, AuthorizedAgent, CompleteAgentLeaseInput, LeaseHeartbeatOutcome, RegistrationEpoch,
};
use tower::ServiceExt as _;

struct RecordingApplication {
  called: AtomicBool,
  lease_called: AtomicBool,
  heartbeat_called: AtomicBool,
}

#[async_trait]
impl AgentRegistrationUseCases for RecordingApplication {
  async fn register(&self, input: AgentRegistrationInput) -> Result<AgentRegistrationOutcome, AgentRegistrationError> {
    input
      .request
      .validate()
      .map_err(|_| AgentRegistrationError::InvalidRequest)?;
    self.called.store(true, Ordering::SeqCst);
    Ok(AgentRegistrationOutcome {
      registration_id: "registration-7".to_owned(),
      registration_epoch: RegistrationEpoch::new(7).unwrap(),
    })
  }

  async fn authorize(&self, _input: AuthorizeAgentInput) -> Result<AuthorizedAgent, AgentRegistrationError> {
    unreachable!("registration route must not invoke operation authorization")
  }
}

#[async_trait]
impl AgentLeaseUseCases for RecordingApplication {
  async fn acquire(&self, input: AcquireAgentLeaseInput) -> Result<AgentLeaseOutcome, AgentLeaseError> {
    input.request.validate().map_err(|_| AgentLeaseError::InvalidRequest)?;
    self.lease_called.store(true, Ordering::SeqCst);
    Ok(AgentLeaseOutcome::NoWork)
  }
}

#[async_trait]
impl AgentHeartbeatUseCases for RecordingApplication {
  async fn heartbeat(&self, _input: AgentHeartbeatInput) -> Result<LeaseHeartbeatOutcome, AgentHeartbeatError> {
    self.heartbeat_called.store(true, Ordering::SeqCst);
    Ok(LeaseHeartbeatOutcome::Fenced)
  }
}

#[async_trait]
impl AgentExecutionUseCases for RecordingApplication {
  async fn append_events(&self, input: AppendAgentEventsInput) -> Result<u64, AgentExecutionError> {
    input
      .request
      .validate()
      .map_err(|_| AgentExecutionError::InvalidRequest)?;
    Ok(input.request.events.last().unwrap().stream_sequence)
  }

  async fn complete_lease(&self, input: CompleteAgentLeaseInput) -> Result<(), AgentExecutionError> {
    input
      .request
      .validate()
      .map_err(|_| AgentExecutionError::InvalidRequest)
  }
}

struct FailedPlacement;

#[async_trait]
impl AgentLeaseUseCases for FailedPlacement {
  async fn acquire(&self, _input: AcquireAgentLeaseInput) -> Result<AgentLeaseOutcome, AgentLeaseError> {
    Err(AgentLeaseError::Unavailable)
  }
}

struct MissingCompletionEvents;

#[async_trait]
impl AgentExecutionUseCases for MissingCompletionEvents {
  async fn append_events(&self, _input: AppendAgentEventsInput) -> Result<u64, AgentExecutionError> {
    unreachable!("completion test does not append events")
  }

  async fn complete_lease(&self, _input: CompleteAgentLeaseInput) -> Result<(), AgentExecutionError> {
    Err(AgentExecutionError::EventsMissing {
      durable_through: 1,
      required: 2,
    })
  }
}

#[tokio::test]
async fn shared_registration_dto_reaches_only_the_application_boundary() {
  let application = Arc::new(RecordingApplication {
    called: AtomicBool::new(false),
    lease_called: AtomicBool::new(false),
    heartbeat_called: AtomicBool::new(false),
  });
  let request: RegisterAgentRequest = serde_json::from_str(include_str!(
    "../../../../shared/protocol-fixtures/coordinator/register-request-v1.json"
  ))
  .unwrap();
  let response = agent_router(
    application.clone(),
    application.clone(),
    application.clone(),
    application.clone(),
    AgentApiConfig::new(5_000).unwrap(),
  )
  .oneshot(
    Request::post("/api/v1/agents/register")
      .header(header::CONTENT_TYPE, "application/json")
      .header(
        header::AUTHORIZATION,
        "Bearer enrollment.00000000-0000-0000-0000-000000000001.BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc",
      )
      .header("idempotency-key", &request.request_id)
      .body(Body::from(serde_json::to_vec(&request).unwrap()))
      .unwrap(),
  )
  .await
  .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert!(application.called.load(Ordering::SeqCst));
  assert!(!application.lease_called.load(Ordering::SeqCst));
  let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
  let response: RegisterAgentResponse = serde_json::from_slice(&body).unwrap();
  response.validate(&request.request_id).unwrap();
  assert_eq!(response.registration_id, "registration-7");
  assert_eq!(response.max_retry_delay_ms, 5_000);
}

#[tokio::test]
async fn transport_rejections_never_reach_the_application() {
  let application = Arc::new(RecordingApplication {
    called: AtomicBool::new(false),
    lease_called: AtomicBool::new(false),
    heartbeat_called: AtomicBool::new(false),
  });
  let response = agent_router(
    application.clone(),
    application.clone(),
    application.clone(),
    application.clone(),
    AgentApiConfig::new(5_000).unwrap(),
  )
  .oneshot(
    Request::post("/api/v1/agents/register")
      .header(header::CONTENT_TYPE, "application/json")
      .body(Body::from("{}"))
      .unwrap(),
  )
  .await
  .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  assert!(!application.called.load(Ordering::SeqCst));
  assert!(!application.lease_called.load(Ordering::SeqCst));
}

#[tokio::test]
async fn shared_lease_request_reaches_the_application_and_returns_no_work() {
  let application = Arc::new(RecordingApplication {
    called: AtomicBool::new(false),
    lease_called: AtomicBool::new(false),
    heartbeat_called: AtomicBool::new(false),
  });
  let request = AcquireLeaseRequest {
    protocol_version: 1,
    request_id: "acquire-1".to_owned(),
    registration_id: "00000000-0000-0000-0000-000000000001".to_owned(),
    wait_seconds: 1,
    accept_jobs: true,
    snapshot: idle_snapshot(),
  };
  let response = agent_router(
    application.clone(),
    application.clone(),
    application.clone(),
    application.clone(),
    AgentApiConfig::new(5_000).unwrap(),
  )
  .oneshot(
    Request::post("/api/v1/agents/linux-builder-01/leases:acquire")
      .header(header::CONTENT_TYPE, "application/json")
      .header(
        header::AUTHORIZATION,
        "Bearer registration.00000000-0000-0000-0000-000000000001.BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc",
      )
      .header("idempotency-key", &request.request_id)
      .body(Body::from(serde_json::to_vec(&request).unwrap()))
      .unwrap(),
  )
  .await
  .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert!(application.lease_called.load(Ordering::SeqCst));
  assert!(!application.called.load(Ordering::SeqCst));
  let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
  let response: AcquireLeaseResponse = serde_json::from_slice(&body).unwrap();
  response.validate(&request.request_id, 0, 0).unwrap();
  assert!(matches!(
    response,
    AcquireLeaseResponse::NoWork {
      retry_after_ms: 5_000,
      ..
    }
  ));
}

#[tokio::test]
async fn failed_placement_commit_cannot_be_serialized_as_an_assignment() {
  let registrations = Arc::new(RecordingApplication {
    called: AtomicBool::new(false),
    lease_called: AtomicBool::new(false),
    heartbeat_called: AtomicBool::new(false),
  });
  let request = AcquireLeaseRequest {
    protocol_version: 1,
    request_id: "acquire-uncommitted".to_owned(),
    registration_id: "00000000-0000-0000-0000-000000000001".to_owned(),
    wait_seconds: 1,
    accept_jobs: true,
    snapshot: idle_snapshot(),
  };
  let response = agent_router(
    registrations.clone(),
    Arc::new(FailedPlacement),
    registrations.clone(),
    registrations,
    AgentApiConfig::new(5_000).unwrap(),
  )
  .oneshot(
    Request::post("/api/v1/agents/linux-builder-01/leases:acquire")
      .header(header::CONTENT_TYPE, "application/json")
      .header(
        header::AUTHORIZATION,
        "Bearer registration.00000000-0000-0000-0000-000000000001.BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc",
      )
      .header("idempotency-key", &request.request_id)
      .body(Body::from(serde_json::to_vec(&request).unwrap()))
      .unwrap(),
  )
  .await
  .unwrap();
  assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
  let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
  assert!(serde_json::from_slice::<AcquireLeaseResponse>(&body).is_err());
  let error: CoordinatorErrorResponse = serde_json::from_slice(&body).unwrap();
  error.validate(&request.request_id).unwrap();
  assert_eq!(error.code, "unavailable");
  assert!(error.retryable);
}

fn idle_snapshot() -> HostSnapshot {
  HostSnapshot {
    available_cpu_millis: 1_000,
    available_memory_bytes: 1,
    work_disk_free_bytes: 1,
    state_disk_free_bytes: 1,
    active_job: None,
    backends: Vec::new(),
  }
}

#[tokio::test]
async fn shared_heartbeat_request_reaches_its_independent_application_boundary() {
  let application = Arc::new(RecordingApplication {
    called: AtomicBool::new(false),
    lease_called: AtomicBool::new(false),
    heartbeat_called: AtomicBool::new(false),
  });
  let request: HeartbeatRequest = serde_json::from_str(include_str!(
    "../../../../shared/protocol-fixtures/coordinator/heartbeat-request-v1.json"
  ))
  .unwrap();
  let response = agent_router(
    application.clone(),
    application.clone(),
    application.clone(),
    application.clone(),
    AgentApiConfig::new(5_000).unwrap(),
  )
  .oneshot(
    Request::post("/api/v1/leases/lease-42/heartbeat")
      .header(header::CONTENT_TYPE, "application/json")
      .header(
        header::AUTHORIZATION,
        "Bearer registration.00000000-0000-0000-0000-000000000001.BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc",
      )
      .header("idempotency-key", &request.request_id)
      .body(Body::from(serde_json::to_vec(&request).unwrap()))
      .unwrap(),
  )
  .await
  .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert!(application.heartbeat_called.load(Ordering::SeqCst));
  assert!(!application.called.load(Ordering::SeqCst));
  assert!(!application.lease_called.load(Ordering::SeqCst));
  let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
  let response: HeartbeatResponse = serde_json::from_slice(&body).unwrap();
  assert_eq!(response.request_id, request.request_id);
  assert_eq!(response.directive, HeartbeatDirective::Fenced);
}

#[tokio::test]
async fn event_append_and_completion_routes_use_shared_protocol_documents() {
  let application = Arc::new(RecordingApplication {
    called: AtomicBool::new(false),
    lease_called: AtomicBool::new(false),
    heartbeat_called: AtomicBool::new(false),
  });
  let append: AppendEventsRequest = serde_json::from_str(include_str!(
    "../../../../shared/protocol-fixtures/coordinator/events-append-request-v1.json"
  ))
  .unwrap();
  let router = agent_router(
    application.clone(),
    application.clone(),
    application.clone(),
    application.clone(),
    AgentApiConfig::new(5_000).unwrap(),
  );
  let response = router
    .clone()
    .oneshot(agent_request(
      "/api/v1/leases/lease-42/events:append",
      &append.request_id,
      &append,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
  let append_response: AppendEventsResponse = serde_json::from_slice(&body).unwrap();
  append_response.validate(&append.request_id, 1, 2).unwrap();

  let completion: CompleteLeaseRequest = serde_json::from_str(include_str!(
    "../../../../shared/protocol-fixtures/coordinator/complete-request-v1.json"
  ))
  .unwrap();
  let response = router
    .oneshot(agent_request(
      "/api/v1/leases/lease-42/complete",
      &completion.request_id,
      &completion,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
  let completion_response: CompleteLeaseResponse = serde_json::from_slice(&body).unwrap();
  completion_response
    .validate(&completion.request_id, &completion.completion_id)
    .unwrap();
}

#[tokio::test]
async fn premature_completion_is_explicitly_retryable() {
  let application = Arc::new(RecordingApplication {
    called: AtomicBool::new(false),
    lease_called: AtomicBool::new(false),
    heartbeat_called: AtomicBool::new(false),
  });
  let completion: CompleteLeaseRequest = serde_json::from_str(include_str!(
    "../../../../shared/protocol-fixtures/coordinator/complete-request-v1.json"
  ))
  .unwrap();
  let response = agent_router(
    application.clone(),
    application.clone(),
    application,
    Arc::new(MissingCompletionEvents),
    AgentApiConfig::new(5_000).unwrap(),
  )
  .oneshot(agent_request(
    "/api/v1/leases/lease-42/complete",
    &completion.request_id,
    &completion,
  ))
  .await
  .unwrap();
  assert_eq!(response.status(), StatusCode::TOO_EARLY);
  let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
  let error: CoordinatorErrorResponse = serde_json::from_slice(&body).unwrap();
  error.validate(&completion.request_id).unwrap();
  assert_eq!(error.code, "events_missing");
  assert!(error.retryable);
}

fn agent_request<T: serde::Serialize>(path: &str, request_id: &str, body: &T) -> Request<Body> {
  Request::post(path)
    .header(header::CONTENT_TYPE, "application/json")
    .header(
      header::AUTHORIZATION,
      "Bearer registration.00000000-0000-0000-0000-000000000001.BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc",
    )
    .header("idempotency-key", request_id)
    .body(Body::from(serde_json::to_vec(body).unwrap()))
    .unwrap()
}
