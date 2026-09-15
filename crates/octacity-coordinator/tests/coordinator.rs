//! End-to-end transport and lease-control tests over a real TCP HTTP boundary.

use std::{
  collections::{BTreeMap, VecDeque},
  fs,
  sync::{Arc, Mutex},
  time::Duration,
};

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signer as _, SigningKey, VerifyingKey};
use octacity_coordinator::{
  CacheSessionCoordinator, CoordinatorClient, CoordinatorError, HttpCoordinatorClient, HttpCoordinatorConfig,
  LeaseMonitor, LeaseMonitorOutcome, LeaseMonitorPolicy, LeasePollOutcome, LeasePoller, OutputUploadCoordinator,
  Registration, RetryPolicy,
};
use octacity_protocol::{
  AcquireLeaseResponse, ActiveJob, AgentInventory, AgentLifecycleEvent, AppendEventsResponse, AttemptEventEnvelope,
  AttemptEventKind, BackendHealth, BackendHealthStatus, BeginCacheSessionRequest, BeginCacheSessionResponse,
  COORDINATOR_PROTOCOL_VERSION, CachePolicy, CompleteLeaseRequest, CompleteLeaseResponse, CompleteOutputUploadRequest,
  CompleteOutputUploadResponse, CoordinatorErrorResponse, ExecutionSpec, HeartbeatDirective, HostCapacity,
  HostSnapshot, JobCompletionStatus, JobLifecycleState, JobSpecV1, LeaseAssignment, NetworkPolicy, OciIsolation,
  OctaInventory, OctaSpec, OutputKind, OutputLimits, OutputUploadMetadata, PlatformArchitecture, PlatformOs,
  PlatformSpec, RegisterAgentResponse, RemoteCacheGrant, RevokeCacheSessionRequest, RevokeCacheSessionResponse,
  RuntimeCapability, RuntimeMode, RuntimeSpec, RuntimeTarget, SIGNATURE_ALGORITHM, SignedEnvelope, SourceSpec,
};
use serde_json::json;
use tokio::{
  io::{AsyncReadExt as _, AsyncWriteExt as _},
  net::{TcpListener, TcpStream},
  sync::watch,
  task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
enum Action {
  Disconnect,
  Hang,
  PartialBodyHang,
  Delay(Duration, Box<Action>),
  Error { status: u16, retryable: bool },
  Register { request_id: Option<String> },
  Acquire(LeaseAssignment),
  Heartbeat(HeartbeatDirective),
  AppendEvents(u64),
  BeginOutput,
  CompleteOutput,
  BeginCache,
  RevokeCache,
  Complete,
  Oversized(usize),
}

#[derive(Clone, Debug)]
struct RecordedRequest {
  path: String,
  request_id: String,
  idempotency_key: String,
  authorization: String,
  body: serde_json::Value,
}

struct MockServer {
  url: String,
  records: Arc<Mutex<Vec<RecordedRequest>>>,
  task: JoinHandle<()>,
}

impl MockServer {
  async fn start(actions: Vec<Action>) -> Self {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let records = Arc::new(Mutex::new(Vec::new()));
    let task_records = records.clone();
    let task = tokio::spawn(async move {
      for action in actions {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_request(&mut stream).await;
        task_records.lock().unwrap().push(request.clone());
        respond(&mut stream, action, &request).await;
      }
    });
    Self { url, records, task }
  }
}

impl Drop for MockServer {
  fn drop(&mut self) {
    self.task.abort();
  }
}

async fn read_request(stream: &mut TcpStream) -> RecordedRequest {
  let mut bytes = Vec::new();
  let header_end = loop {
    let mut chunk = [0_u8; 1024];
    let read = stream.read(&mut chunk).await.unwrap();
    assert!(read > 0, "client closed before sending headers");
    bytes.extend_from_slice(&chunk[..read]);
    if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
      break position + 4;
    }
  };
  let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
  let content_length = headers
    .lines()
    .find_map(|line| {
      line
        .split_once(':')
        .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map(|(_, value)| value.trim().parse::<usize>().unwrap())
    })
    .unwrap_or(0);
  while bytes.len() - header_end < content_length {
    let mut chunk = [0_u8; 1024];
    let read = stream.read(&mut chunk).await.unwrap();
    assert!(read > 0, "client closed before sending its body");
    bytes.extend_from_slice(&chunk[..read]);
  }
  let body: serde_json::Value = serde_json::from_slice(&bytes[header_end..header_end + content_length]).unwrap();
  let header = |name: &str| {
    headers
      .lines()
      .find_map(|line| {
        line
          .split_once(':')
          .filter(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
          .map(|(_, value)| value.trim().to_owned())
      })
      .unwrap_or_default()
  };
  RecordedRequest {
    path: headers
      .lines()
      .next()
      .unwrap()
      .split_whitespace()
      .nth(1)
      .unwrap()
      .to_owned(),
    request_id: body["request_id"].as_str().unwrap().to_owned(),
    idempotency_key: header("idempotency-key"),
    authorization: header("authorization"),
    body,
  }
}

async fn respond(stream: &mut TcpStream, action: Action, request: &RecordedRequest) {
  let mut action = action;
  loop {
    match action {
      Action::Delay(delay, next) => {
        tokio::time::sleep(delay).await;
        action = *next;
      }
      Action::Disconnect => return,
      Action::Hang => std::future::pending::<()>().await,
      Action::PartialBodyHang => {
        let _ = stream
          .write_all(
            b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 100\r\nconnection: close\r\n\r\n{",
          )
          .await;
        std::future::pending::<()>().await
      }
      Action::Error { status, retryable } => {
        let body = serde_json::to_vec(&CoordinatorErrorResponse {
          protocol_version: COORDINATOR_PROTOCOL_VERSION,
          request_id: request.request_id.clone(),
          code: "temporarily_unavailable".to_owned(),
          message: "try again".to_owned(),
          retryable,
          retry_after_ms: Some(1),
        })
        .unwrap();
        write_response(stream, status, &body).await;
        return;
      }
      Action::Register { request_id } => {
        let body = serde_json::to_vec(&RegisterAgentResponse {
          protocol_version: COORDINATOR_PROTOCOL_VERSION,
          request_id: request_id.unwrap_or_else(|| request.request_id.clone()),
          registration_id: "registration-1".to_owned(),
          max_retry_delay_ms: 50,
        })
        .unwrap();
        write_response(stream, 200, &body).await;
        return;
      }
      Action::Acquire(lease) => {
        let body = serde_json::to_vec(&AcquireLeaseResponse::Lease {
          protocol_version: COORDINATOR_PROTOCOL_VERSION,
          request_id: request.request_id.clone(),
          lease,
        })
        .unwrap();
        write_response(stream, 200, &body).await;
        return;
      }
      Action::Heartbeat(directive) => {
        let body = serde_json::to_vec(&octacity_protocol::HeartbeatResponse {
          protocol_version: COORDINATOR_PROTOCOL_VERSION,
          request_id: request.request_id.clone(),
          directive,
        })
        .unwrap();
        write_response(stream, 200, &body).await;
        return;
      }
      Action::AppendEvents(acknowledged_sequence) => {
        let body = serde_json::to_vec(&AppendEventsResponse {
          protocol_version: COORDINATOR_PROTOCOL_VERSION,
          request_id: request.request_id.clone(),
          acknowledged_sequence,
        })
        .unwrap();
        write_response(stream, 200, &body).await;
        return;
      }
      Action::BeginOutput => {
        let body = serde_json::to_vec(&octacity_protocol::BeginOutputUploadResponse {
          protocol_version: COORDINATOR_PROTOCOL_VERSION,
          request_id: request.request_id.clone(),
          upload_id: "upload-1".to_owned(),
          put_url: "https://objects.example/upload-1?signature=opaque".to_owned(),
          required_headers: BTreeMap::new(),
          expires_at: unix_now() + 60,
        })
        .unwrap();
        write_response(stream, 200, &body).await;
        return;
      }
      Action::CompleteOutput => {
        let body = serde_json::to_vec(&CompleteOutputUploadResponse {
          protocol_version: COORDINATOR_PROTOCOL_VERSION,
          request_id: request.request_id.clone(),
          upload_id: request.body["upload_id"].as_str().unwrap().to_owned(),
        })
        .unwrap();
        write_response(stream, 200, &body).await;
        return;
      }
      Action::BeginCache => {
        let body = serde_json::to_vec(&BeginCacheSessionResponse {
          protocol_version: COORDINATOR_PROTOCOL_VERSION,
          request_id: request.request_id.clone(),
          session_id: "cache-session-1".to_owned(),
          scope_id: "project-trust-domain-1".to_owned(),
          remote: Some(RemoteCacheGrant {
            endpoint: "https://cache.example/v1".to_owned(),
            bearer_token: "cache-secret".to_owned(),
            expires_at: unix_now() + 60,
          }),
        })
        .unwrap();
        write_response(stream, 200, &body).await;
        return;
      }
      Action::RevokeCache => {
        let body = serde_json::to_vec(&RevokeCacheSessionResponse {
          protocol_version: COORDINATOR_PROTOCOL_VERSION,
          request_id: request.request_id.clone(),
          session_id: request.body["session_id"].as_str().unwrap().to_owned(),
        })
        .unwrap();
        write_response(stream, 200, &body).await;
        return;
      }
      Action::Complete => {
        let body = serde_json::to_vec(&CompleteLeaseResponse {
          protocol_version: COORDINATOR_PROTOCOL_VERSION,
          request_id: request.request_id.clone(),
          completion_id: request.body["completion_id"].as_str().unwrap().to_owned(),
        })
        .unwrap();
        write_response(stream, 200, &body).await;
        return;
      }
      Action::Oversized(size) => {
        write_response(stream, 200, &vec![b'x'; size]).await;
        return;
      }
    }
  }
}

async fn write_response(stream: &mut TcpStream, status: u16, body: &[u8]) {
  let reason = if status == 200 { "OK" } else { "Service Unavailable" };
  let headers = format!(
    "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
    body.len()
  );
  let _ = stream.write_all(headers.as_bytes()).await;
  let _ = stream.write_all(body).await;
}

fn inventory() -> AgentInventory {
  AgentInventory {
    agent_id: "agent-1".to_owned(),
    agent_version: "0.1.0".to_owned(),
    coordinator_protocols: vec![COORDINATOR_PROTOCOL_VERSION],
    labels: BTreeMap::new(),
    host_platform: platform(),
    host_capacity: capacity(),
    runtimes: vec![RuntimeCapability {
      backend: "native".to_owned(),
      mode: RuntimeMode::Native,
      platform: platform(),
      isolation: None,
    }],
    octa: OctaInventory {
      version: "0.3.0".to_owned(),
      runner_sha256: "1".repeat(64),
      build_commit: None,
      runner_protocols: vec![1],
      event_schemas: vec![1],
      plugin_protocols: vec![1],
      octafile_versions: vec![1],
      features: Vec::new(),
      plugins: Vec::new(),
    },
    source_plugins: Vec::new(),
    cache: None,
  }
}

fn platform() -> PlatformSpec {
  PlatformSpec {
    os: PlatformOs::Linux,
    architecture: PlatformArchitecture::Amd64,
  }
}

fn capacity() -> HostCapacity {
  HostCapacity {
    logical_cpu_count: 4,
    total_memory_bytes: 16 * 1024,
    work_disk_total_bytes: 32 * 1024,
    state_disk_total_bytes: 32 * 1024,
    virtualization_available: false,
  }
}

fn snapshot() -> HostSnapshot {
  HostSnapshot {
    available_cpu_millis: 2_000,
    available_memory_bytes: 8 * 1024,
    work_disk_free_bytes: 16 * 1024,
    state_disk_free_bytes: 16 * 1024,
    active_job: Some(ActiveJob {
      job_id: "job-1".to_owned(),
      attempt: 1,
      lease_id: "lease-1".to_owned(),
      resource_usage: None,
    }),
    backends: vec![BackendHealth {
      backend: "native".to_owned(),
      status: BackendHealthStatus::Ready,
      message: None,
    }],
  }
}

fn client(
  server: &MockServer,
  max_attempts: usize,
  request_timeout: Duration,
  max_body_bytes: usize,
) -> HttpCoordinatorClient {
  let directory = tempfile::tempdir().unwrap();
  let credential = directory.path().join("credential");
  fs::write(&credential, "enrollment-token\n").unwrap();
  // The client reads the credential during construction, so the temporary
  // directory can disappear before the first request.
  HttpCoordinatorClient::new(HttpCoordinatorConfig {
    server_url: server.url.clone(),
    credential_file: credential,
    request_timeout,
    max_body_bytes,
    retry: RetryPolicy {
      max_attempts,
      initial_delay: Duration::from_millis(1),
      max_delay: Duration::from_millis(100),
    },
  })
  .unwrap()
}

fn http_config(server_url: String, credential_file: std::path::PathBuf) -> HttpCoordinatorConfig {
  HttpCoordinatorConfig {
    server_url,
    credential_file,
    request_timeout: Duration::from_secs(1),
    max_body_bytes: 4096,
    retry: RetryPolicy {
      max_attempts: 1,
      initial_delay: Duration::from_millis(1),
      max_delay: Duration::from_secs(1),
    },
  }
}

fn registration() -> Registration {
  Registration {
    agent_id: "agent-1".to_owned(),
    registration_id: "registration-1".to_owned(),
    max_retry_delay: Duration::from_millis(100),
  }
}

#[test]
fn rejects_non_tls_remote_origins_and_bounded_invalid_credentials() {
  let directory = tempfile::tempdir().unwrap();
  let credential = directory.path().join("credential");
  fs::write(&credential, "token").unwrap();
  assert!(matches!(
    HttpCoordinatorClient::new(http_config("http://example.com".to_owned(), credential.clone())),
    Err(CoordinatorError::Invalid(_))
  ));

  fs::write(&credential, "contains whitespace").unwrap();
  assert!(matches!(
    HttpCoordinatorClient::new(http_config("https://example.com".to_owned(), credential.clone())),
    Err(CoordinatorError::Invalid(_))
  ));

  fs::write(&credential, vec![b'x'; 64 * 1024 + 1]).unwrap();
  assert!(matches!(
    HttpCoordinatorClient::new(http_config("https://example.com".to_owned(), credential)),
    Err(CoordinatorError::Invalid(_))
  ));
}

#[tokio::test]
async fn retries_disconnects_and_retryable_statuses_with_one_idempotency_key() {
  let server = MockServer::start(vec![
    Action::Disconnect,
    Action::Error {
      status: 503,
      retryable: true,
    },
    Action::Register { request_id: None },
  ])
  .await;
  let client = client(&server, 3, Duration::from_secs(1), 4096);

  let registered = client.register(&inventory(), CancellationToken::new()).await.unwrap();
  assert_eq!(registered.registration_id, "registration-1");
  assert_eq!(registered.max_retry_delay, Duration::from_millis(50));
  let records = server.records.lock().unwrap();
  assert_eq!(records.len(), 3);
  assert!(records.iter().all(|request| request.path == "/api/v1/agents/register"));
  assert!(
    records
      .iter()
      .all(|request| request.authorization == "Bearer enrollment-token")
  );
  assert!(
    records
      .iter()
      .all(|request| request.request_id == records[0].request_id)
  );
  assert!(
    records
      .iter()
      .all(|request| request.idempotency_key == records[0].request_id)
  );
}

#[tokio::test]
async fn rejects_mismatched_and_oversized_responses() {
  let server = MockServer::start(vec![Action::Register {
    request_id: Some("stale-request".to_owned()),
  }])
  .await;
  let error = client(&server, 1, Duration::from_secs(1), 4096)
    .register(&inventory(), CancellationToken::new())
    .await
    .unwrap_err();
  assert!(matches!(error, CoordinatorError::Protocol(_)));

  let server = MockServer::start(vec![Action::Oversized(4097)]).await;
  let error = client(&server, 1, Duration::from_secs(1), 4096)
    .register(&inventory(), CancellationToken::new())
    .await
    .unwrap_err();
  assert!(matches!(error, CoordinatorError::ResponseTooLarge { .. }));

  let server = MockServer::start(vec![Action::Hang]).await;
  let error = client(&server, 1, Duration::from_secs(1), 64)
    .register(&inventory(), CancellationToken::new())
    .await
    .unwrap_err();
  assert!(matches!(error, CoordinatorError::RequestTooLarge { .. }));
}

#[tokio::test]
async fn shutdown_cancels_a_blocked_long_poll_and_requests_time_out() {
  let server = MockServer::start(vec![Action::Hang]).await;
  let http_client = Arc::new(client(&server, 1, Duration::from_millis(20), 4096));
  let shutdown = CancellationToken::new();
  let polling = tokio::spawn({
    let client = http_client.clone();
    let shutdown = shutdown.clone();
    async move {
      client
        .acquire_lease(
          &registration(),
          Duration::from_secs(1),
          Duration::from_secs(1),
          shutdown,
        )
        .await
    }
  });
  tokio::time::sleep(Duration::from_millis(20)).await;
  shutdown.cancel();
  assert!(matches!(polling.await.unwrap(), Err(CoordinatorError::Cancelled)));

  let server = MockServer::start(vec![Action::Delay(
    Duration::from_millis(100),
    Box::new(Action::Register { request_id: None }),
  )])
  .await;
  let error = client(&server, 1, Duration::from_millis(10), 4096)
    .register(&inventory(), CancellationToken::new())
    .await
    .unwrap_err();
  assert!(matches!(error, CoordinatorError::RetriesExhausted { .. }));

  let server = MockServer::start(vec![Action::PartialBodyHang]).await;
  let error = client(&server, 1, Duration::from_millis(10), 4096)
    .register(&inventory(), CancellationToken::new())
    .await
    .unwrap_err();
  assert!(matches!(error, CoordinatorError::RetriesExhausted { .. }));
}

#[tokio::test]
async fn sends_fenced_lease_and_heartbeat_documents_to_exact_endpoints() {
  let signing_key = SigningKey::from_bytes(&[7; 32]);
  let lease = signed_lease(&signing_key, unix_now());
  let server = MockServer::start(vec![
    Action::Acquire(lease.clone()),
    Action::Heartbeat(HeartbeatDirective::Fenced),
  ])
  .await;
  let client = client(&server, 1, Duration::from_secs(1), 16 * 1024);
  let registration = registration();

  let acquired = client
    .acquire_lease(
      &registration,
      Duration::from_secs(1),
      Duration::from_secs(10),
      CancellationToken::new(),
    )
    .await
    .unwrap();
  assert!(matches!(
    acquired,
    AcquireLeaseResponse::Lease { lease: received, .. } if received == lease
  ));
  let directive = client
    .heartbeat(
      &registration,
      &lease,
      &snapshot(),
      &capacity(),
      Duration::from_secs(10),
      CancellationToken::new(),
    )
    .await
    .unwrap();
  assert_eq!(directive, HeartbeatDirective::Fenced);

  let records = server.records.lock().unwrap();
  assert_eq!(records[0].path, "/api/v1/agents/agent-1/leases:acquire");
  assert_eq!(records[0].body["registration_id"], "registration-1");
  assert_eq!(records[1].path, "/api/v1/leases/lease-1/heartbeat");
  assert_eq!(records[1].body["registration_id"], "registration-1");
  assert_eq!(records[1].body["lease"]["fencing_token"], "fence-1");
}

#[tokio::test]
async fn appends_fenced_events_and_completes_with_stable_idempotency() {
  let signing_key = SigningKey::from_bytes(&[7; 32]);
  let lease = signed_lease(&signing_key, unix_now());
  let server = MockServer::start(vec![Action::AppendEvents(1), Action::Complete]).await;
  let client = client(&server, 1, Duration::from_secs(1), 16 * 1024);
  let fence = (&lease).into();
  let events = vec![AttemptEventEnvelope {
    job_id: lease.job_id.clone(),
    attempt: lease.attempt,
    lease_id: lease.lease_id.clone(),
    fencing_token: lease.fencing_token.clone(),
    stream_sequence: 1,
    kind: AttemptEventKind::Agent {
      event: AgentLifecycleEvent::StateChanged {
        state: JobLifecycleState::Preparing,
      },
    },
  }];
  let response = client
    .append_events(&registration(), &lease, &events, CancellationToken::new())
    .await
    .unwrap();
  assert_eq!(response.acknowledged_sequence, 1);
  let completion = CompleteLeaseRequest {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "complete-request-1".to_owned(),
    registration_id: registration().registration_id,
    lease: fence,
    completion_id: "completion-1".to_owned(),
    last_event_sequence: 1,
    status: JobCompletionStatus::Succeeded,
    final_usage: None,
    results: Vec::new(),
  };
  client
    .complete_lease(&registration(), &lease, &completion, CancellationToken::new())
    .await
    .unwrap();

  let records = server.records.lock().unwrap();
  assert_eq!(records[0].path, "/api/v1/leases/lease-1/events:append");
  assert_eq!(records[0].body["events"][0]["stream_sequence"], 1);
  assert_eq!(records[1].path, "/api/v1/leases/lease-1/complete");
  assert_eq!(records[1].request_id, "complete-request-1");
  assert_eq!(records[1].idempotency_key, "complete-request-1");
}

#[tokio::test]
async fn authorizes_and_completes_outputs_on_exact_fenced_endpoints() {
  let signing_key = SigningKey::from_bytes(&[7; 32]);
  let lease = signed_lease(&signing_key, unix_now());
  let server = MockServer::start(vec![Action::BeginOutput, Action::CompleteOutput]).await;
  let client = client(&server, 1, Duration::from_secs(1), 16 * 1024);
  let begin = octacity_protocol::BeginOutputUploadRequest {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "begin-output-1".to_owned(),
    registration_id: registration().registration_id,
    lease: (&lease).into(),
    upload_key: "upload-key-1".to_owned(),
    output: OutputUploadMetadata {
      run_id: 17,
      task_id: 23,
      kind: OutputKind::Artifact,
      name: "application".to_owned(),
      content_type: None,
      report_format: None,
      transport_content_type: "application/octet-stream".to_owned(),
      size_bytes: 7,
      sha256: "a".repeat(64),
    },
  };
  let target = client
    .begin_output_upload(&registration(), &lease, &begin, CancellationToken::new())
    .await
    .unwrap();
  let complete = CompleteOutputUploadRequest {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "complete-output-1".to_owned(),
    registration_id: registration().registration_id,
    lease: (&lease).into(),
    upload_id: target.upload_id,
  };
  client
    .complete_output_upload(&registration(), &lease, &complete, CancellationToken::new())
    .await
    .unwrap();

  let records = server.records.lock().unwrap();
  assert_eq!(records[0].path, "/api/v1/leases/lease-1/artifacts:begin");
  assert_eq!(records[0].body["lease"]["fencing_token"], "fence-1");
  assert_eq!(records[1].path, "/api/v1/leases/lease-1/artifacts:complete");
  assert_eq!(records[1].body["upload_id"], "upload-1");
}

#[tokio::test]
async fn begins_and_revokes_cache_authority_on_exact_fenced_endpoints() {
  let signing_key = SigningKey::from_bytes(&[7; 32]);
  let lease = signed_lease(&signing_key, unix_now());
  let server = MockServer::start(vec![Action::BeginCache, Action::RevokeCache]).await;
  let client = client(&server, 1, Duration::from_secs(1), 16 * 1024);
  let registration = registration();
  let begin = BeginCacheSessionRequest {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "begin-cache-1".to_owned(),
    registration_id: registration.registration_id.clone(),
    lease: (&lease).into(),
    cache: CachePolicy {
      namespace: "project/main".to_owned(),
      read: true,
      write: true,
    },
  };
  let grant = client
    .begin_cache_session(&registration, &lease, &begin, CancellationToken::new())
    .await
    .unwrap();
  assert_eq!(grant.session_id, "cache-session-1");
  assert!(!format!("{grant:?}").contains("cache-secret"));
  let revoke = RevokeCacheSessionRequest {
    protocol_version: COORDINATOR_PROTOCOL_VERSION,
    request_id: "revoke-cache-1".to_owned(),
    registration_id: registration.registration_id.clone(),
    lease: (&lease).into(),
    session_id: grant.session_id,
  };
  client
    .revoke_cache_session(&registration, &lease, &revoke, CancellationToken::new())
    .await
    .unwrap();

  let records = server.records.lock().unwrap();
  assert_eq!(records[0].path, "/api/v1/leases/lease-1/cache:begin");
  assert_eq!(records[0].body["cache"]["namespace"], "project/main");
  assert_eq!(records[1].path, "/api/v1/leases/lease-1/cache:revoke");
  assert_eq!(records[1].body["session_id"], "cache-session-1");
  assert!(
    records
      .iter()
      .all(|record| !record.authorization.contains("cache-secret"))
  );
}

struct ScriptedClient {
  leases: Mutex<VecDeque<Result<AcquireLeaseResponse, CoordinatorError>>>,
  heartbeats: Mutex<VecDeque<Result<HeartbeatDirective, CoordinatorError>>>,
}

#[async_trait]
impl CoordinatorClient for ScriptedClient {
  async fn register(
    &self,
    _inventory: &AgentInventory,
    _cancellation: CancellationToken,
  ) -> Result<Registration, CoordinatorError> {
    Ok(registration())
  }

  async fn acquire_lease(
    &self,
    _registration: &Registration,
    _wait: Duration,
    _lease_safety_margin: Duration,
    cancellation: CancellationToken,
  ) -> Result<AcquireLeaseResponse, CoordinatorError> {
    if let Some(result) = self.leases.lock().unwrap().pop_front() {
      return result;
    }
    cancellation.cancelled().await;
    Err(CoordinatorError::Cancelled)
  }

  async fn heartbeat(
    &self,
    _registration: &Registration,
    _lease: &LeaseAssignment,
    _snapshot: &HostSnapshot,
    _capacity: &HostCapacity,
    _lease_safety_margin: Duration,
    cancellation: CancellationToken,
  ) -> Result<HeartbeatDirective, CoordinatorError> {
    if let Some(result) = self.heartbeats.lock().unwrap().pop_front() {
      return result;
    }
    cancellation.cancelled().await;
    Err(CoordinatorError::Cancelled)
  }

  async fn append_events(
    &self,
    _registration: &Registration,
    _lease: &LeaseAssignment,
    _events: &[AttemptEventEnvelope],
    _cancellation: CancellationToken,
  ) -> Result<AppendEventsResponse, CoordinatorError> {
    unreachable!("event delivery is not used by lease-monitor tests")
  }

  async fn complete_lease(
    &self,
    _registration: &Registration,
    _lease: &LeaseAssignment,
    _completion: &CompleteLeaseRequest,
    _cancellation: CancellationToken,
  ) -> Result<(), CoordinatorError> {
    unreachable!("completion is not used by lease-monitor tests")
  }
}

fn signed_lease(signing_key: &SigningKey, now: u64) -> LeaseAssignment {
  let spec = JobSpecV1 {
    protocol_version: 1,
    job_id: "job-1".to_owned(),
    attempt: 1,
    issued_at: now.saturating_sub(1),
    expires_at: now + 300,
    source: SourceSpec {
      provider: "git".to_owned(),
      plugin_version: "0.1.0".to_owned(),
      plugin_sha256: "1".repeat(64),
      revision: "a".repeat(40),
      reference: None,
      parameters: BTreeMap::from([("url".to_owned(), json!("https://example.com/repository.git"))]),
    },
    octa: OctaSpec {
      version: "0.3.0".to_owned(),
      runner_sha256: "2".repeat(64),
      runner_protocol: 1,
      event_schema: 1,
      plugin_protocol: 1,
      plugin_digests: BTreeMap::new(),
    },
    execution: ExecutionSpec {
      octafile: None,
      commands: vec!["build".to_owned()],
      variables: BTreeMap::new(),
      arguments: Vec::new(),
      concurrency: None,
      parallel: false,
      failfast: true,
      secrets_profile: None,
    },
    runtime: RuntimeSpec {
      target: RuntimeTarget::Oci {
        platform: platform(),
        isolation: OciIsolation::Hypervisor,
        image: format!("example/build@sha256:{}", "3".repeat(64)),
      },
      cpu_millis: 1000,
      memory_bytes: 1024,
      writable_disk_bytes: 1024,
      timeout_seconds: 60,
      network: NetworkPolicy::Disabled,
      workload_identity_profile: None,
    },
    cache: None,
    outputs: OutputLimits {
      artifact_count: 1,
      artifact_bytes: 1024,
      report_count: 1,
      report_bytes: 1024,
      single_output_bytes: 1024,
    },
  };
  let payload = serde_json::to_vec(&spec).unwrap();
  LeaseAssignment {
    lease_id: "lease-1".to_owned(),
    job_id: spec.job_id.clone(),
    attempt: spec.attempt,
    fencing_token: "fence-1".to_owned(),
    issued_at: now.saturating_sub(1),
    expires_at: now + 120,
    signed_job_spec: SignedEnvelope {
      key_id: "primary".to_owned(),
      algorithm: SIGNATURE_ALGORITHM.to_owned(),
      payload: BASE64.encode(&payload),
      signature: BASE64.encode(signing_key.sign(&payload).to_bytes()),
    },
  }
}

fn unix_now() -> u64 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs()
}

#[tokio::test]
async fn poller_waits_through_no_work_and_exposes_only_a_verified_lease() {
  let signing_key = SigningKey::from_bytes(&[7; 32]);
  let lease = signed_lease(&signing_key, unix_now());
  let client = Arc::new(ScriptedClient {
    leases: Mutex::new(VecDeque::from([
      Ok(AcquireLeaseResponse::NoWork {
        protocol_version: 1,
        request_id: "poll-1".to_owned(),
        retry_after_ms: 1,
      }),
      Ok(AcquireLeaseResponse::Lease {
        protocol_version: 1,
        request_id: "poll-2".to_owned(),
        lease,
      }),
    ])),
    heartbeats: Mutex::new(VecDeque::new()),
  });
  let keys: BTreeMap<String, VerifyingKey> = BTreeMap::from([("primary".to_owned(), signing_key.verifying_key())]);
  let poller = LeasePoller::new(
    client,
    registration(),
    Arc::new(keys),
    Duration::from_secs(1),
    Duration::from_secs(10),
  )
  .unwrap();

  let LeasePollOutcome::Lease(lease) = poller.next(CancellationToken::new()).await.unwrap() else {
    panic!("expected a lease");
  };
  assert_eq!(lease.spec.job_id, "job-1");
  assert_eq!(lease.lease.fencing_token, "fence-1");
}

#[tokio::test]
async fn poller_rejects_a_job_signature_that_does_not_match_the_lease() {
  let signing_key = SigningKey::from_bytes(&[7; 32]);
  let mut lease = signed_lease(&signing_key, unix_now());
  lease.signed_job_spec.signature = BASE64.encode([0_u8; 64]);
  let client = Arc::new(ScriptedClient {
    leases: Mutex::new(VecDeque::from([Ok(AcquireLeaseResponse::Lease {
      protocol_version: 1,
      request_id: "poll-1".to_owned(),
      lease,
    })])),
    heartbeats: Mutex::new(VecDeque::new()),
  });
  let keys = BTreeMap::from([("primary".to_owned(), signing_key.verifying_key())]);
  let poller = LeasePoller::new(
    client,
    registration(),
    Arc::new(keys),
    Duration::from_secs(1),
    Duration::from_secs(10),
  )
  .unwrap();
  assert!(matches!(
    poller.next(CancellationToken::new()).await,
    Err(CoordinatorError::JobSpec(_))
  ));
}

#[tokio::test]
async fn heartbeat_keeps_drain_sticky_then_fences_and_cancels_the_job() {
  let now = unix_now();
  let signing_key = SigningKey::from_bytes(&[7; 32]);
  let lease = signed_lease(&signing_key, now);
  let client = Arc::new(ScriptedClient {
    leases: Mutex::new(VecDeque::new()),
    heartbeats: Mutex::new(VecDeque::from([
      Ok(HeartbeatDirective::Drain { expires_at: now + 120 }),
      Ok(HeartbeatDirective::Fenced),
    ])),
  });
  let (_snapshot_sender, snapshots) = watch::channel(snapshot());
  let monitor = LeaseMonitor::start(
    client,
    registration(),
    lease,
    capacity(),
    snapshots,
    LeaseMonitorPolicy {
      heartbeat_interval: Duration::from_millis(10),
      lease_safety_margin: Duration::from_secs(10),
    },
    CancellationToken::new(),
  )
  .unwrap();
  let cancellation = monitor.job_cancellation();
  let draining = monitor.draining();
  let outcome = monitor.wait().await.unwrap();

  assert!(matches!(outcome, LeaseMonitorOutcome::Fenced));
  assert!(cancellation.is_cancelled());
  assert!(*draining.borrow());
}

#[tokio::test]
async fn heartbeat_failure_cancels_at_the_lease_safety_deadline() {
  let now = unix_now();
  let signing_key = SigningKey::from_bytes(&[7; 32]);
  let mut lease = signed_lease(&signing_key, now);
  lease.expires_at = now + 2;
  let failures = (0..20)
    .map(|_| {
      Err(CoordinatorError::Rejected {
        operation: "heartbeat lease",
        status: 503,
        code: "unavailable".to_owned(),
        message: "retry".to_owned(),
        retryable: true,
      })
    })
    .collect();
  let client = Arc::new(ScriptedClient {
    leases: Mutex::new(VecDeque::new()),
    heartbeats: Mutex::new(failures),
  });
  let (_snapshot_sender, snapshots) = watch::channel(snapshot());
  let monitor = LeaseMonitor::start(
    client,
    registration(),
    lease,
    capacity(),
    snapshots,
    LeaseMonitorPolicy {
      heartbeat_interval: Duration::from_millis(50),
      lease_safety_margin: Duration::from_secs(1),
    },
    CancellationToken::new(),
  )
  .unwrap();
  let cancellation = monitor.job_cancellation();
  let outcome = monitor.wait().await.unwrap();

  assert!(matches!(outcome, LeaseMonitorOutcome::Expired));
  assert!(cancellation.is_cancelled());
}
