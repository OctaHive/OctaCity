use std::{
  collections::BTreeMap,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  time::Duration,
};

use async_trait::async_trait;
use axum::{
  body::{Body, to_bytes},
  http::{Request, StatusCode, header},
};
use octacity_protocol::{
  AcquireLeaseRequest, AcquireLeaseResponse, AgentTelemetryIsolation, AgentTelemetryRuntime, AgentTelemetrySample,
  AppendEventsRequest, AppendEventsResponse, BeginCacheSessionRequest, BeginCacheSessionResponse,
  BeginOutputUploadRequest, BeginOutputUploadResponse, COORDINATOR_PROTOCOL_VERSION, CachePolicy, CompleteLeaseRequest,
  CompleteLeaseResponse, CompleteOutputUploadRequest, CompleteOutputUploadResponse, CoordinatorErrorResponse,
  HeartbeatDirective, HeartbeatRequest, HeartbeatResponse, HostSnapshot, IngestAgentTelemetryRequest,
  IngestAgentTelemetryResponse, LeaseFence, MAX_AGENT_TELEMETRY_REQUEST_BYTES, OutputKind, OutputUploadMetadata,
  RegisterAgentRequest, RegisterAgentResponse, RemoteCacheGrant, RevokeCacheSessionRequest, RevokeCacheSessionResponse,
};
use octacity_server_api_agent::{
  AgentApiConfig, AgentRouterDependencies, AgentTelemetryIngress, AgentTelemetryPolicy,
  agent_router as build_agent_router,
};
use octacity_server_application::{
  AcquireAgentLeaseInput, AgentArtifactError, AgentArtifactTransferUseCases, AgentCacheSessionError,
  AgentCacheSessionUseCases, AgentExecutionError, AgentExecutionUseCases, AgentHeartbeatError, AgentHeartbeatInput,
  AgentHeartbeatUseCases, AgentLeaseError, AgentLeaseOutcome, AgentLeaseUseCases, AgentRegistrationError,
  AgentRegistrationInput, AgentRegistrationOutcome, AgentRegistrationUseCases, AgentTelemetryBatch,
  AgentTelemetryError, AgentTelemetryExportError, AgentTelemetryExporter, AgentTelemetryInput, AgentTelemetryUseCases,
  AppendAgentEventsInput, AuthorizeAgentInput, AuthorizedAgent, BeginAgentArtifactUploadInput,
  BeginAgentCacheSessionInput, CompleteAgentArtifactUploadInput, CompleteAgentLeaseInput, LeaseHeartbeatOutcome,
  RegistrationEpoch, RevokeAgentCacheSessionInput,
};
use tower::ServiceExt as _;

fn agent_router(
  registrations: Arc<dyn AgentRegistrationUseCases>,
  leases: Arc<dyn AgentLeaseUseCases>,
  heartbeats: Arc<dyn AgentHeartbeatUseCases>,
  execution: Arc<dyn AgentExecutionUseCases>,
  config: AgentApiConfig,
) -> axum::Router {
  agent_router_with_telemetry(
    registrations,
    leases,
    heartbeats,
    execution,
    AgentTelemetryIngress::new(
      Arc::new(TelemetryApplication),
      Arc::new(AcceptingTelemetryExporter),
      AgentTelemetryPolicy::new(4, Duration::from_secs(1)).unwrap(),
    ),
    config,
  )
}

fn agent_router_with_telemetry(
  registrations: Arc<dyn AgentRegistrationUseCases>,
  leases: Arc<dyn AgentLeaseUseCases>,
  heartbeats: Arc<dyn AgentHeartbeatUseCases>,
  execution: Arc<dyn AgentExecutionUseCases>,
  telemetry: AgentTelemetryIngress,
  config: AgentApiConfig,
) -> axum::Router {
  build_agent_router(
    AgentRouterDependencies::new(
      registrations,
      leases,
      heartbeats,
      telemetry,
      execution,
      Arc::new(ArtifactApplication),
      Arc::new(CacheApplication),
    ),
    config,
  )
}

struct TelemetryApplication;

struct AcceptingTelemetryExporter;

struct FailingTelemetryExporter;

#[async_trait]
impl AgentTelemetryExporter for AcceptingTelemetryExporter {
  async fn export(&self, _batch: AgentTelemetryBatch) -> Result<(), AgentTelemetryExportError> {
    Ok(())
  }
}

#[async_trait]
impl AgentTelemetryExporter for FailingTelemetryExporter {
  async fn export(&self, _batch: AgentTelemetryBatch) -> Result<(), AgentTelemetryExportError> {
    Err(AgentTelemetryExportError)
  }
}

#[async_trait]
impl AgentTelemetryUseCases for TelemetryApplication {
  async fn authorize_ingest(&self, input: AgentTelemetryInput) -> Result<AgentTelemetryBatch, AgentTelemetryError> {
    AgentTelemetryBatch::try_from_request(input.request)
  }
}

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

struct ArtifactApplication;

struct CacheApplication;

#[async_trait]
impl AgentCacheSessionUseCases for CacheApplication {
  async fn begin_session(
    &self,
    input: BeginAgentCacheSessionInput,
  ) -> Result<BeginCacheSessionResponse, AgentCacheSessionError> {
    Ok(BeginCacheSessionResponse {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: input.request.request_id,
      session_id: "cache-session-1".to_owned(),
      scope_id: "project-scope-1".to_owned(),
      remote: Some(RemoteCacheGrant {
        endpoint: "https://cache.example/v1".to_owned(),
        bearer_token: "opaque-cache-bearer".to_owned(),
        expires_at: 4_000_000_000,
      }),
    })
  }

  async fn revoke_session(
    &self,
    input: RevokeAgentCacheSessionInput,
  ) -> Result<RevokeCacheSessionResponse, AgentCacheSessionError> {
    Ok(RevokeCacheSessionResponse {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: input.request.request_id,
      session_id: input.request.session_id,
    })
  }
}

#[async_trait]
impl AgentArtifactTransferUseCases for ArtifactApplication {
  async fn begin_upload(
    &self,
    input: BeginAgentArtifactUploadInput,
  ) -> Result<BeginOutputUploadResponse, AgentArtifactError> {
    Ok(BeginOutputUploadResponse {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: input.request.request_id,
      upload_id: "upload-1".to_owned(),
      put_url: "https://objects.example/upload?signature=opaque".to_owned(),
      required_headers: BTreeMap::from([("x-checksum".to_owned(), "opaque".to_owned())]),
      expires_at: 4_000_000_000,
    })
  }

  async fn complete_upload(
    &self,
    input: CompleteAgentArtifactUploadInput,
  ) -> Result<CompleteOutputUploadResponse, AgentArtifactError> {
    Ok(CompleteOutputUploadResponse {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: input.request.request_id,
      upload_id: input.request.upload_id,
    })
  }
}

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
  let response_request_id = response.headers()["x-request-id"].to_str().unwrap();
  assert!(uuid::Uuid::parse_str(response_request_id).is_ok());
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
async fn telemetry_uses_a_separate_route_and_reports_sample_accounting() {
  let application = Arc::new(RecordingApplication {
    called: AtomicBool::new(false),
    lease_called: AtomicBool::new(false),
    heartbeat_called: AtomicBool::new(false),
  });
  let request = IngestAgentTelemetryRequest {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "telemetry-1".to_owned(),
    registration_id: "registration-1".to_owned(),
    samples: vec![telemetry_sample()],
  };
  let response = agent_router(
    application.clone(),
    application.clone(),
    application.clone(),
    application,
    AgentApiConfig::new(5_000).unwrap(),
  )
  .oneshot(agent_request(
    "/api/v1/agents/telemetry:ingest",
    &request.request_id,
    &request,
  ))
  .await
  .unwrap();

  assert_eq!(response.status(), StatusCode::OK);
  let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
  let response: IngestAgentTelemetryResponse = serde_json::from_slice(&body).unwrap();
  response.validate(&request.request_id, request.samples.len()).unwrap();
  assert_eq!(response.accepted_samples, 1);
  assert_eq!(response.dropped_samples, 0);
}

#[tokio::test]
async fn telemetry_route_enforces_its_smaller_body_limit_before_deserialization() {
  let application = Arc::new(RecordingApplication {
    called: AtomicBool::new(false),
    lease_called: AtomicBool::new(false),
    heartbeat_called: AtomicBool::new(false),
  });
  let response = agent_router(
    application.clone(),
    application.clone(),
    application.clone(),
    application,
    AgentApiConfig::new(5_000).unwrap(),
  )
  .oneshot(
    Request::post("/api/v1/agents/telemetry:ingest")
      .header(header::CONTENT_TYPE, "application/json")
      .header(
        header::AUTHORIZATION,
        "Bearer registration.00000000-0000-0000-0000-000000000001.BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc",
      )
      .header("idempotency-key", "oversized-telemetry")
      .body(Body::from(vec![b' '; MAX_AGENT_TELEMETRY_REQUEST_BYTES + 1]))
      .unwrap(),
  )
  .await
  .unwrap();

  assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn telemetry_exporter_outage_reports_loss_without_failing_heartbeat() {
  let application = Arc::new(RecordingApplication {
    called: AtomicBool::new(false),
    lease_called: AtomicBool::new(false),
    heartbeat_called: AtomicBool::new(false),
  });
  let router = agent_router_with_telemetry(
    application.clone(),
    application.clone(),
    application.clone(),
    application.clone(),
    AgentTelemetryIngress::new(
      Arc::new(TelemetryApplication),
      Arc::new(FailingTelemetryExporter),
      AgentTelemetryPolicy::new(1, Duration::from_secs(1)).unwrap(),
    ),
    AgentApiConfig::new(5_000).unwrap(),
  );
  let telemetry = IngestAgentTelemetryRequest {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "telemetry-outage".to_owned(),
    registration_id: "registration-1".to_owned(),
    samples: vec![telemetry_sample()],
  };
  let response = router
    .clone()
    .oneshot(agent_request(
      "/api/v1/agents/telemetry:ingest",
      &telemetry.request_id,
      &telemetry,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
  let response: IngestAgentTelemetryResponse = serde_json::from_slice(&body).unwrap();
  assert_eq!(response.accepted_samples, 0);
  assert_eq!(response.dropped_samples, 1);

  let heartbeat: HeartbeatRequest = serde_json::from_str(include_str!(
    "../../../../shared/protocol-fixtures/coordinator/heartbeat-request-v1.json"
  ))
  .unwrap();
  let response = router
    .clone()
    .oneshot(agent_request(
      "/api/v1/leases/lease-42/heartbeat",
      &heartbeat.request_id,
      &heartbeat,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert!(application.heartbeat_called.load(Ordering::SeqCst));

  let events: AppendEventsRequest = serde_json::from_str(include_str!(
    "../../../../shared/protocol-fixtures/coordinator/events-append-request-v1.json"
  ))
  .unwrap();
  let response = router
    .oneshot(agent_request(
      "/api/v1/leases/lease-42/events:append",
      &events.request_id,
      &events,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
  let response: AppendEventsResponse = serde_json::from_slice(&body).unwrap();
  response.validate(&events.request_id, 1, 2).unwrap();
}

fn telemetry_sample() -> AgentTelemetrySample {
  AgentTelemetrySample {
    observed_at_unix_ms: 1,
    runtime: AgentTelemetryRuntime::Native,
    isolation: AgentTelemetryIsolation::Native,
    cpu_time_ms: 1,
    memory_current_bytes: 2,
    io_read_bytes: 3,
    io_written_bytes: 4,
    network_received_bytes: None,
    network_transmitted_bytes: None,
  }
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

#[tokio::test]
async fn artifact_routes_use_the_existing_fenced_agent_protocol() {
  let application = Arc::new(RecordingApplication {
    called: AtomicBool::new(false),
    lease_called: AtomicBool::new(false),
    heartbeat_called: AtomicBool::new(false),
  });
  let router = agent_router(
    application.clone(),
    application.clone(),
    application.clone(),
    application,
    AgentApiConfig::new(5_000).unwrap(),
  );
  let lease = LeaseFence {
    lease_id: "lease-1".to_owned(),
    job_id: "job-1".to_owned(),
    attempt: 1,
    fencing_token: "fence-1".to_owned(),
  };
  let begin = BeginOutputUploadRequest {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "artifact-begin-1".to_owned(),
    registration_id: "registration-1".to_owned(),
    lease: lease.clone(),
    upload_key: "output-1".to_owned(),
    output: OutputUploadMetadata {
      run_id: 1,
      task_id: 2,
      kind: OutputKind::Artifact,
      name: "dist.tar".to_owned(),
      content_type: Some("application/x-tar".to_owned()),
      report_format: None,
      transport_content_type: "application/x-tar".to_owned(),
      size_bytes: 42,
      sha256: "42".repeat(32),
    },
  };
  let response = router
    .clone()
    .oneshot(agent_request(
      "/api/v1/leases/lease-1/artifacts:begin",
      &begin.request_id,
      &begin,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
  let begun: BeginOutputUploadResponse = serde_json::from_slice(&body).unwrap();
  begun.validate(&begin.request_id, 1).unwrap();

  let complete = CompleteOutputUploadRequest {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "artifact-complete-1".to_owned(),
    registration_id: "registration-1".to_owned(),
    lease,
    upload_id: begun.upload_id.clone(),
  };
  let response = router
    .oneshot(agent_request(
      "/api/v1/leases/lease-1/artifacts:complete",
      &complete.request_id,
      &complete,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
  let completed: CompleteOutputUploadResponse = serde_json::from_slice(&body).unwrap();
  completed.validate(&complete.request_id, &complete.upload_id).unwrap();
}

#[tokio::test]
async fn cache_routes_use_the_existing_fenced_agent_protocol() {
  let application = Arc::new(RecordingApplication {
    called: AtomicBool::new(false),
    lease_called: AtomicBool::new(false),
    heartbeat_called: AtomicBool::new(false),
  });
  let router = agent_router(
    application.clone(),
    application.clone(),
    application.clone(),
    application,
    AgentApiConfig::new(5_000).unwrap(),
  );
  let lease = LeaseFence {
    lease_id: "lease-1".to_owned(),
    job_id: "job-1".to_owned(),
    attempt: 1,
    fencing_token: "fence-1".to_owned(),
  };
  let begin = BeginCacheSessionRequest {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "cache-begin-1".to_owned(),
    registration_id: "registration-1".to_owned(),
    lease: lease.clone(),
    cache: CachePolicy {
      namespace: "project/main".to_owned(),
      read: true,
      write: false,
    },
  };
  let response = router
    .clone()
    .oneshot(agent_request(
      "/api/v1/leases/lease-1/cache:begin",
      &begin.request_id,
      &begin,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
  let begun: BeginCacheSessionResponse = serde_json::from_slice(&body).unwrap();
  begun.validate(&begin.request_id, 1).unwrap();

  let revoke = RevokeCacheSessionRequest {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "cache-revoke-1".to_owned(),
    registration_id: "registration-1".to_owned(),
    lease,
    session_id: begun.session_id,
  };
  let response = router
    .oneshot(agent_request(
      "/api/v1/leases/lease-1/cache:revoke",
      &revoke.request_id,
      &revoke,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
  let revoked: RevokeCacheSessionResponse = serde_json::from_slice(&body).unwrap();
  revoked.validate(&revoke.request_id, &revoke.session_id).unwrap();
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
