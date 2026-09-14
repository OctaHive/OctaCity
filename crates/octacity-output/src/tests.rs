//! End-to-end tests for the fenced presigned-upload flow.

use std::{
  collections::BTreeMap,
  sync::{Arc, Mutex},
  time::Duration,
};

use async_trait::async_trait;
use octacity_coordinator::{CoordinatorError, OutputUploadCoordinator, Registration};
use octacity_protocol::{
  BeginOutputUploadRequest, BeginOutputUploadResponse, CompleteOutputUploadRequest, LeaseAssignment, OutputLimits,
  SignedEnvelope,
};
use tokio::{
  io::{AsyncReadExt as _, AsyncWriteExt as _},
  net::TcpListener,
};
use tokio_util::sync::CancellationToken;

use super::*;

#[test]
fn upload_origins_reject_credentials_paths_and_plaintext_remote_hosts() {
  for origin in [
    "https://user:secret@objects.example",
    "https://objects.example/bucket",
    "http://objects.example",
  ] {
    assert!(canonical_origin(origin).is_err(), "accepted unsafe origin {origin}");
  }
  assert_eq!(
    canonical_origin("https://objects.example").unwrap(),
    "https://objects.example"
  );
}

struct UploadCoordinator {
  put_url: String,
  expires_at: u64,
  begun: Mutex<Vec<BeginOutputUploadRequest>>,
  completed: Mutex<Vec<CompleteOutputUploadRequest>>,
}

#[async_trait]
impl OutputUploadCoordinator for UploadCoordinator {
  async fn begin_output_upload(
    &self,
    _registration: &Registration,
    _lease: &LeaseAssignment,
    request: &BeginOutputUploadRequest,
    _cancellation: CancellationToken,
  ) -> Result<BeginOutputUploadResponse, CoordinatorError> {
    self.begun.lock().unwrap().push(request.clone());
    Ok(BeginOutputUploadResponse {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: request.request_id.clone(),
      upload_id: "upload-1".to_owned(),
      put_url: self.put_url.clone(),
      required_headers: BTreeMap::from([("x-octacity-test".to_owned(), "present".to_owned())]),
      expires_at: self.expires_at,
    })
  }

  async fn complete_output_upload(
    &self,
    _registration: &Registration,
    _lease: &LeaseAssignment,
    request: &CompleteOutputUploadRequest,
    _cancellation: CancellationToken,
  ) -> Result<(), CoordinatorError> {
    self.completed.lock().unwrap().push(request.clone());
    Ok(())
  }
}

#[tokio::test]
async fn retries_one_presigned_put_then_completes_and_removes_staging() {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = listener.local_addr().unwrap();
  let received = Arc::new(Mutex::new(Vec::new()));
  let server_received = received.clone();
  let server = tokio::spawn(async move {
    for attempt in 0..2 {
      let (mut stream, _) = listener.accept().await.unwrap();
      let (headers, body) = read_request(&mut stream).await;
      assert!(headers.to_ascii_lowercase().contains("x-octacity-test: present"));
      server_received.lock().unwrap().push(body);
      let status = if attempt == 0 {
        "500 Internal Server Error"
      } else {
        "200 OK"
      };
      stream
        .write_all(format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes())
        .await
        .unwrap();
    }
  });

  let coordinator = Arc::new(UploadCoordinator {
    put_url: format!("http://{address}/object"),
    expires_at: unix_now() + 60,
    begun: Mutex::new(Vec::new()),
    completed: Mutex::new(Vec::new()),
  });
  let publisher = PresignedOutputPublisher::new(
    coordinator.clone(),
    PresignedOutputPublisherConfig {
      allowed_origins: vec![format!("http://{address}")],
      max_archive_entries: 16,
      upload_timeout: Duration::from_secs(2),
      retry: RetryPolicy {
        max_attempts: 2,
        initial_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(1),
      },
    },
  )
  .unwrap();
  let temporary = tempfile::tempdir().unwrap();
  let workspace = temporary.path().join("workspace");
  tokio::fs::create_dir(&workspace).await.unwrap();
  tokio::fs::write(workspace.join("result.bin"), b"immutable-output")
    .await
    .unwrap();
  let staging = temporary.path().join("staging");
  let results = vec![serde_json::json!({
    "run_id": 17,
    "tasks": [{"task_id": 23, "artifacts": [{"name": "binary", "path": "result.bin"}]}]
  })];
  let registration = registration();
  let lease = lease();

  let cancellation = CancellationToken::new();
  let frozen = publisher
    .freeze(
      FreezeOutputs {
        workspace: &workspace,
        results: &results,
        limits: &OutputLimits {
          artifact_count: 1,
          artifact_bytes: 1024,
          report_count: 0,
          report_bytes: 0,
          single_output_bytes: 1024,
        },
        staging_root: &staging,
      },
      cancellation.clone(),
    )
    .await
    .unwrap();
  publisher
    .publish(
      PublishOutputs {
        registration: &registration,
        lease: &lease,
        frozen,
      },
      cancellation,
    )
    .await
    .unwrap();
  server.await.unwrap();

  assert_eq!(*received.lock().unwrap(), vec![b"immutable-output".to_vec(); 2]);
  assert_eq!(coordinator.begun.lock().unwrap().len(), 1);
  assert_eq!(coordinator.completed.lock().unwrap().len(), 1);
  assert!(!staging.exists());
}

#[tokio::test]
async fn transport_errors_never_expose_presigned_query_credentials() {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = listener.local_addr().unwrap();
  drop(listener);
  let coordinator = Arc::new(UploadCoordinator {
    put_url: format!("http://{address}/object?signature=top-secret"),
    expires_at: unix_now() + 60,
    begun: Mutex::new(Vec::new()),
    completed: Mutex::new(Vec::new()),
  });
  let publisher = publisher(
    coordinator,
    format!("http://{address}"),
    RetryPolicy {
      max_attempts: 1,
      initial_delay: Duration::from_millis(1),
      max_delay: Duration::from_millis(1),
    },
  );

  let error = publish_fixture(&publisher).await.unwrap_err().to_string();

  assert!(!error.contains("top-secret"), "presigned credential leaked: {error}");
  assert!(!error.contains("signature="), "presigned query leaked: {error}");
}

#[tokio::test]
async fn retry_does_not_outlive_the_presigned_target() {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = listener.local_addr().unwrap();
  // A connection to a closed port can consume the entire connect timeout on
  // Windows. Return one immediate retryable response so the test measures the
  // retry-deadline decision rather than platform TCP behavior.
  let server = tokio::spawn(async move {
    let (mut stream, _) = listener.accept().await.unwrap();
    let _ = read_request(&mut stream).await;
    stream
      .write_all(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
      .await
      .unwrap();
  });
  let coordinator = Arc::new(UploadCoordinator {
    put_url: format!("http://{address}/object"),
    expires_at: unix_now() + 2,
    begun: Mutex::new(Vec::new()),
    completed: Mutex::new(Vec::new()),
  });
  let publisher = publisher(
    coordinator,
    format!("http://{address}"),
    RetryPolicy {
      max_attempts: 2,
      initial_delay: Duration::from_secs(3),
      max_delay: Duration::from_secs(3),
    },
  );
  let started = tokio::time::Instant::now();

  let error = publish_fixture(&publisher).await.unwrap_err();
  server.await.unwrap();

  assert!(started.elapsed() < Duration::from_secs(1));
  assert!(error.to_string().contains("expired before retry"));
}

fn publisher(coordinator: Arc<UploadCoordinator>, origin: String, retry: RetryPolicy) -> PresignedOutputPublisher {
  PresignedOutputPublisher::new(
    coordinator,
    PresignedOutputPublisherConfig {
      allowed_origins: vec![origin],
      max_archive_entries: 16,
      upload_timeout: Duration::from_secs(2),
      retry,
    },
  )
  .unwrap()
}

async fn publish_fixture(publisher: &PresignedOutputPublisher) -> Result<(), OutputError> {
  let temporary = tempfile::tempdir().unwrap();
  let workspace = temporary.path().join("workspace");
  tokio::fs::create_dir(&workspace).await.unwrap();
  tokio::fs::write(workspace.join("result.bin"), b"immutable-output")
    .await
    .unwrap();
  let results = [serde_json::json!({
    "run_id": 17,
    "tasks": [{"task_id": 23, "artifacts": [{"name": "binary", "path": "result.bin"}]}]
  })];
  let cancellation = CancellationToken::new();
  let frozen = publisher
    .freeze(
      FreezeOutputs {
        workspace: &workspace,
        results: &results,
        limits: &OutputLimits {
          artifact_count: 1,
          artifact_bytes: 1024,
          report_count: 0,
          report_bytes: 0,
          single_output_bytes: 1024,
        },
        staging_root: &temporary.path().join("staging"),
      },
      cancellation.clone(),
    )
    .await?;
  publisher
    .publish(
      PublishOutputs {
        registration: &registration(),
        lease: &lease(),
        frozen,
      },
      cancellation,
    )
    .await
}

fn unix_now() -> u64 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs()
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> (String, Vec<u8>) {
  let mut received = Vec::new();
  let header_end = loop {
    let mut buffer = [0_u8; 1024];
    let read = stream.read(&mut buffer).await.unwrap();
    assert!(read > 0, "connection closed before HTTP headers");
    received.extend_from_slice(&buffer[..read]);
    if let Some(position) = received.windows(4).position(|window| window == b"\r\n\r\n") {
      break position + 4;
    }
  };
  let headers = String::from_utf8(received[..header_end].to_vec()).unwrap();
  let content_length = headers
    .lines()
    .find_map(|line| {
      line
        .to_ascii_lowercase()
        .strip_prefix("content-length: ")
        .map(str::to_owned)
    })
    .unwrap()
    .parse::<usize>()
    .unwrap();
  while received.len() - header_end < content_length {
    let mut buffer = [0_u8; 1024];
    let read = stream.read(&mut buffer).await.unwrap();
    assert!(read > 0, "connection closed before HTTP body");
    received.extend_from_slice(&buffer[..read]);
  }
  (headers, received[header_end..header_end + content_length].to_vec())
}

fn registration() -> Registration {
  Registration {
    agent_id: "agent-1".to_owned(),
    registration_id: "registration-1".to_owned(),
    max_retry_delay: Duration::from_secs(1),
  }
}

fn lease() -> LeaseAssignment {
  LeaseAssignment {
    lease_id: "lease-1".to_owned(),
    job_id: "job-1".to_owned(),
    attempt: 1,
    fencing_token: "fence-1".to_owned(),
    issued_at: 1,
    expires_at: u64::MAX,
    signed_job_spec: SignedEnvelope {
      key_id: "unused".to_owned(),
      algorithm: "unused".to_owned(),
      payload: "unused".to_owned(),
      signature: "unused".to_owned(),
    },
  }
}
