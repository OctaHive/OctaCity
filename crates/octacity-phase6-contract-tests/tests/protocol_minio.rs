//! End-to-end server-agent upload contract against real Vault and MinIO services.
//!
//! Unlike unit fakes, this opt-in test runs the production job lifecycle, real
//! Octa runner, HTTP upload client, output publisher, server-side
//! fencing/idempotency logic, and `S3ArtifactStore` in one path. Vault and
//! MinIO are external so the portable suite stays deterministic; an explicit
//! CI job provisions them.

use std::{
  collections::BTreeMap,
  env, fs,
  io::Read as _,
  path::{Path as FilePath, PathBuf},
  process::Stdio,
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
  },
  time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use aws_sdk_s3::{
  Client,
  config::{BehaviorVersion, Credentials, Region},
};
use axum::http::StatusCode;
use base64::{
  Engine as _,
  engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use ed25519_dalek::{Signer as _, SigningKey};
use octacity_artifact_store::{ArtifactStore, S3ArtifactStore, S3ArtifactStoreConfig};
use octacity_coordinator::{
  CoordinatorClient, CoordinatorError, HttpCoordinatorClient, HttpCoordinatorConfig, OutputUploadCoordinator,
  Registration, RetryPolicy, VerifiedLease,
};
use octacity_execution::{
  ExecutionBackend, ExecutionError, ExecutionExit, ExecutionIo, ExecutionPaths, ExecutionReader, ExecutionTarget,
  ExecutionWriter, ResourceUsage, RunnerProgram, RunningExecution, StartExecution,
};
use octacity_identity::FileWorkloadIdentityProvider;
use octacity_job::{JobExecutor, JobExecutorConfig};
use octacity_lifecycle::{JobLifecycle, JobLifecycleConfig, SpoolLimits};
use octacity_output::{
  FreezeOutputs, FrozenOutputs, OutputPublisher, PresignedOutputPublisher, PresignedOutputPublisherConfig,
  PublishOutputs,
};
use octacity_protocol::{
  AGENT_PROTOCOL_VERSION, AcquireLeaseResponse, AgentInventory, AppendEventsResponse, AttemptEventEnvelope,
  COORDINATOR_PROTOCOL_VERSION, CompleteLeaseRequest, ExecutionSpec, HeartbeatDirective, HostCapacity, HostSnapshot,
  JobCompletionStatus, JobSpecV1, LeaseAssignment, NetworkPolicy, OctaSpec, OutputLimits, PlatformArchitecture,
  PlatformOs, PlatformSpec, RuntimeMode, RuntimeSpec, RuntimeTarget, SourceSpec,
};
use octacity_runner::{RunnerCapabilities, RunnerInstallation, RunnerPlugin, RunnerSupervisionPolicy};
use octacity_source::{
  MaterializedSource, SourceError, SourceHostError, SourceMaterializationRequest, SourceMaterializer,
};
use sha2::{Digest as _, Sha256};
use tokio::{
  io::{AsyncRead, AsyncReadExt as _, AsyncWriteExt as _},
  process::{Child, Command},
};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::util::SubscriberInitExt as _;
use uuid::Uuid;

const TOKEN: &str = "phase6-coordinator-token";
const SECRET: &str = "phase6-vault-secret-value";

#[tokio::test]
#[ignore = "requires explicitly configured Vault, MinIO, and built Octa runner/plugins"]
async fn real_octa_vault_job_publishes_outputs_without_leaking_secrets() {
  let agent_logs = install_log_capture();
  let endpoint = required("OCTACITY_MINIO_ENDPOINT");
  let vault_endpoint = required("OCTACITY_VAULT_ENDPOINT");
  let vault_host = reqwest::Url::parse(&vault_endpoint)
    .unwrap()
    .host_str()
    .unwrap()
    .to_owned();
  let access_key = required("OCTACITY_MINIO_ACCESS_KEY");
  let secret_key = required("OCTACITY_MINIO_SECRET_KEY");
  let region = env::var("OCTACITY_MINIO_REGION").unwrap_or_else(|_| "us-east-1".to_owned());
  let bucket = format!("octacity-test-{}", Uuid::new_v4().simple());
  let administration = minio_client(&endpoint, &region, &access_key, &secret_key);
  administration.create_bucket().bucket(&bucket).send().await.unwrap();
  let store: Arc<dyn ArtifactStore> = Arc::new(
    S3ArtifactStore::new(S3ArtifactStoreConfig {
      endpoint: endpoint.clone(),
      region,
      bucket: bucket.clone(),
      prefix: "phase6-protocol".to_owned(),
      access_key,
      secret_key,
      force_path_style: true,
      operation_timeout: Duration::from_secs(30),
    })
    .unwrap(),
  );
  let temporary = tempfile::tempdir().unwrap();
  let work_root = temporary.path().join("work");
  let state_root = temporary.path().join("state");
  fs::create_dir(&work_root).unwrap();
  fs::create_dir(&state_root).unwrap();
  let installation = phase6_installation(temporary.path());
  let spec = phase6_spec(&installation, &vault_host);
  let lease = lease(&spec);
  let registration = registration();
  let state = UploadServerState {
    store: store.clone(),
    registration_id: registration.registration_id.clone(),
    fence: (&lease).into(),
    records: Arc::new(Mutex::new(UploadRecords::default())),
  };
  let (server_origin, shutdown) = start_server(state.clone()).await;

  let identity_source = tempfile::tempdir().unwrap();
  let vault_root_token = required("OCTACITY_VAULT_ROOT_TOKEN");
  let jwt = configure_vault(&vault_endpoint, &vault_root_token).await;
  let source_identity = identity_source.path().join("workload.jwt");
  fs::write(&source_identity, &jwt).unwrap();

  let credential = temporary.path().join("coordinator.token");
  std::fs::write(&credential, TOKEN).unwrap();
  let coordinator = Arc::new(
    HttpCoordinatorClient::new(HttpCoordinatorConfig {
      server_url: server_origin,
      credential_file: credential,
      request_timeout: Duration::from_secs(5),
      max_body_bytes: 64 * 1024,
      retry: RetryPolicy {
        max_attempts: 2,
        initial_delay: Duration::from_millis(10),
        max_delay: Duration::from_millis(20),
      },
    })
    .unwrap(),
  );
  let output_coordinator: Arc<dyn OutputUploadCoordinator> = coordinator;
  let publisher: Arc<dyn OutputPublisher> = Arc::new(
    PresignedOutputPublisher::new(
      output_coordinator,
      PresignedOutputPublisherConfig {
        allowed_origins: vec![endpoint],
        max_archive_entries: 32,
        upload_timeout: Duration::from_secs(10),
        retry: RetryPolicy {
          max_attempts: 2,
          initial_delay: Duration::from_millis(10),
          max_delay: Duration::from_millis(20),
        },
      },
    )
    .unwrap(),
  );
  let lifecycle_coordinator = Arc::new(Phase6Coordinator::default());
  let inspected = Arc::new(AtomicBool::new(false));
  let runner_output = Arc::new(Mutex::new(Vec::new()));
  let outputs: Arc<dyn OutputPublisher> = Arc::new(InspectingPublisher {
    inner: publisher,
    state_root: state_root.clone(),
    work_root: work_root.clone(),
    forbidden: vec![SECRET.to_owned(), jwt.clone()],
    inspected: inspected.clone(),
  });
  let executor = Arc::new(
    JobExecutor::new(
      installation,
      Arc::new(Phase6Source {
        vault_endpoint: vault_endpoint.clone(),
      }),
      Arc::new(FileWorkloadIdentityProvider::new(BTreeMap::from([(
        "ci".to_owned(),
        source_identity,
      )]))),
      BTreeMap::from([(
        RuntimeMode::Native,
        Arc::new(HostRunnerBackend {
          captured: runner_output.clone(),
          allowed_network_host: vault_host.clone(),
        }) as Arc<dyn ExecutionBackend>,
      )]),
      JobExecutorConfig {
        work_root: work_root.clone(),
        max_workspace_bytes: 1024 * 1024,
        allow_unrestricted_network: false,
        allowed_network_hosts: vec![vault_host],
        max_output_limits: spec.outputs.clone(),
        cancellation_grace: Duration::from_secs(5),
        runner_supervision: RunnerSupervisionPolicy {
          resource_sample_interval: Duration::from_millis(50),
          ..RunnerSupervisionPolicy::default()
        },
      },
    )
    .unwrap(),
  );
  let capacity = phase6_capacity();
  let lifecycle = JobLifecycle::new(
    lifecycle_coordinator.clone(),
    lifecycle_coordinator.clone(),
    registration,
    executor,
    outputs,
    capacity.clone(),
    JobLifecycleConfig {
      state_root: state_root.clone(),
      event_channel_capacity: 32,
      event_retry_delay: Duration::from_millis(10),
      spool: SpoolLimits {
        max_bytes: 256 * 1024,
        max_records: 128,
        batch_bytes: 64 * 1024,
        batch_records: 32,
      },
      lease_monitor: octacity_coordinator::LeaseMonitorPolicy {
        heartbeat_interval: Duration::from_millis(50),
        lease_safety_margin: Duration::from_secs(5),
      },
    },
  )
  .unwrap();
  let outcome = lifecycle
    .run(
      VerifiedLease {
        lease: lease.clone(),
        spec,
      },
      phase6_snapshot(&capacity),
      CancellationToken::new(),
    )
    .await
    .unwrap();

  assert_eq!(
    outcome.status,
    JobCompletionStatus::Succeeded,
    "events: {:#?}; completions: {:#?}; runner: {}",
    lifecycle_coordinator.events.lock().unwrap(),
    lifecycle_coordinator.completions.lock().unwrap(),
    String::from_utf8_lossy(&runner_output.lock().unwrap()),
  );
  assert!(inspected.load(Ordering::SeqCst));
  assert!(fs::read_dir(&work_root).unwrap().next().is_none());
  assert!(fs::read_dir(state_root.join("jobs")).unwrap().next().is_none());
  let coordinator_bytes = serde_json::to_vec(&(
    lifecycle_coordinator.events.lock().unwrap().clone(),
    lifecycle_coordinator.completions.lock().unwrap().clone(),
  ))
  .unwrap();
  assert_secret_absent("events and terminal result", &coordinator_bytes, &[SECRET, &jwt]);
  assert_secret_absent(
    "runner protocol and diagnostics after Octa secret resolution",
    &runner_output.lock().unwrap(),
    &[SECRET],
  );
  {
    let logs = agent_logs.lock().unwrap();
    assert!(
      !logs.is_empty(),
      "the completion gate did not capture agent tracing output"
    );
    assert_secret_absent("agent tracing logs", &logs, &[SECRET, &jwt]);
  }

  let records = state.records.lock().unwrap().by_upload_id.clone();
  assert_eq!(records.len(), 2);
  assert!(records.values().all(|record| record.completed));
  assert!(records.values().any(|record| {
    record.metadata.name == "analysis" && record.metadata.report_format.as_deref() == Some("vendor/acme-v7")
  }));
  let metadata = serde_json::to_vec(&records.values().map(|record| &record.metadata).collect::<Vec<_>>()).unwrap();
  assert_secret_absent("uploaded metadata", &metadata, &[SECRET, &jwt]);
  for record in records.values() {
    let download = store
      .authorize_download(&record.object, Duration::from_secs(60))
      .await
      .unwrap();
    let bytes = reqwest::get(download.url).await.unwrap().bytes().await.unwrap();
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), record.object.sha256);
  }
  assert_tree_excludes(temporary.path(), &[SECRET, &jwt]);

  for record in records.values() {
    store.delete(&record.object).await.unwrap();
  }
  shutdown.cancel();
  administration.delete_bucket().bucket(bucket).send().await.unwrap();
}

#[path = "protocol_minio/runtime_fixture.rs"]
mod runtime_fixture;

use runtime_fixture::{
  HostRunnerBackend, InspectingPublisher, Phase6Coordinator, Phase6Source, phase6_capacity, phase6_installation,
  phase6_snapshot, phase6_spec,
};
async fn configure_vault(endpoint: &str, root_token: &str) -> String {
  let client = reqwest::Client::new();
  let signing = SigningKey::from_bytes(&[9_u8; 32]);
  let response = client
    .delete(format!("{endpoint}/v1/sys/auth/octacity-jwt"))
    .header("x-vault-token", root_token)
    .send()
    .await
    .unwrap();
  assert!(response.status().is_success() || response.status() == StatusCode::NOT_FOUND);
  vault_write(
    &client,
    endpoint,
    root_token,
    "sys/auth/octacity-jwt",
    serde_json::json!({"type": "jwt"}),
  )
  .await;
  vault_write(
    &client,
    endpoint,
    root_token,
    "auth/octacity-jwt/config",
    serde_json::json!({
      "jwt_validation_pubkeys": [ed25519_public_key_pem(&signing)],
      "bound_issuer": "octacity-phase6",
      "jwt_supported_algs": ["EdDSA"]
    }),
  )
  .await;
  vault_write(
    &client,
    endpoint,
    root_token,
    "sys/policies/acl/octacity-phase6",
    serde_json::json!({"policy": "path \"secret/data/phase6\" { capabilities = [\"read\"] }"}),
  )
  .await;
  vault_write(
    &client,
    endpoint,
    root_token,
    "auth/octacity-jwt/role/octacity-ci",
    serde_json::json!({
      "role_type": "jwt",
      "bound_audiences": ["octacity"],
      "bound_subject": "agent-1",
      "user_claim": "sub",
      "token_policies": ["octacity-phase6"],
      "token_ttl": "60s",
      "token_max_ttl": "120s"
    }),
  )
  .await;
  vault_write(
    &client,
    endpoint,
    root_token,
    "secret/data/phase6",
    serde_json::json!({"data": {"token": SECRET}}),
  )
  .await;

  let now = unix_now();
  let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"EdDSA","typ":"JWT"}"#);
  let claims = URL_SAFE_NO_PAD.encode(
    serde_json::to_vec(&serde_json::json!({
      "iss": "octacity-phase6",
      "sub": "agent-1",
      "aud": "octacity",
      "iat": now,
      "nbf": now.saturating_sub(1),
      "exp": now + 300
    }))
    .unwrap(),
  );
  let signing_input = format!("{header}.{claims}");
  let signature = URL_SAFE_NO_PAD.encode(signing.sign(signing_input.as_bytes()).to_bytes());
  format!("{signing_input}.{signature}")
}

async fn vault_write(client: &reqwest::Client, endpoint: &str, root_token: &str, path: &str, body: serde_json::Value) {
  client
    .post(format!("{endpoint}/v1/{path}"))
    .header("x-vault-token", root_token)
    .json(&body)
    .send()
    .await
    .unwrap()
    .error_for_status()
    .unwrap();
}

fn ed25519_public_key_pem(signing: &SigningKey) -> String {
  // RFC 8410 SubjectPublicKeyInfo prefix for an Ed25519 public key.
  let mut der = vec![0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00];
  der.extend_from_slice(signing.verifying_key().as_bytes());
  format!(
    "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----",
    STANDARD.encode(der)
  )
}

fn assert_secret_absent(context: &str, bytes: &[u8], forbidden: &[&str]) {
  for value in forbidden {
    assert!(
      !bytes.windows(value.len()).any(|window| window == value.as_bytes()),
      "{context} contains protected bytes"
    );
  }
}

#[derive(Clone)]
struct CapturedLogs(Arc<Mutex<Vec<u8>>>);

struct CapturedLogWriter(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for CapturedLogWriter {
  fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
    self.0.lock().unwrap().extend_from_slice(buffer);
    Ok(buffer.len())
  }

  fn flush(&mut self) -> std::io::Result<()> {
    Ok(())
  }
}

impl<'writer> tracing_subscriber::fmt::MakeWriter<'writer> for CapturedLogs {
  type Writer = CapturedLogWriter;

  fn make_writer(&'writer self) -> Self::Writer {
    CapturedLogWriter(self.0.clone())
  }
}

/// Installs one process-wide sink because this ignored contract runs as an
/// exact, isolated test binary in CI. A thread-local subscriber would not
/// observe lifecycle work that Tokio schedules on other worker threads.
fn install_log_capture() -> Arc<Mutex<Vec<u8>>> {
  let bytes = Arc::new(Mutex::new(Vec::new()));
  tracing_subscriber::fmt()
    .with_ansi(false)
    .without_time()
    .with_writer(CapturedLogs(bytes.clone()))
    .finish()
    .try_init()
    .expect("the isolated phase-six contract must own the tracing subscriber");
  bytes
}

fn assert_tree_excludes(root: &FilePath, forbidden: &[&str]) {
  let mut pending = vec![root.to_owned()];
  while let Some(path) = pending.pop() {
    for entry in std::fs::read_dir(path).unwrap() {
      let entry = entry.unwrap();
      let kind = entry.file_type().unwrap();
      if kind.is_dir() {
        pending.push(entry.path());
      } else if kind.is_file() {
        assert_secret_absent(
          &entry.path().display().to_string(),
          &std::fs::read(entry.path()).unwrap(),
          forbidden,
        );
      }
    }
  }
}

#[path = "protocol_minio/upload_server.rs"]
mod upload_server;

use upload_server::{UploadRecords, UploadServerState, lease, registration, start_server};
fn required(name: &str) -> String {
  env::var(name).unwrap_or_else(|_| panic!("{name} must be set for this explicit MinIO protocol test"))
}

fn required_path(name: &str) -> PathBuf {
  let path = PathBuf::from(required(name));
  assert!(
    path.is_absolute() && path.exists(),
    "{name} must be an existing absolute path"
  );
  path
}

fn minio_client(endpoint: &str, region: &str, access_key: &str, secret_key: &str) -> Client {
  let config = aws_sdk_s3::Config::builder()
    .behavior_version(BehaviorVersion::latest())
    .endpoint_url(endpoint)
    .region(Region::new(region.to_owned()))
    .credentials_provider(Credentials::new(
      access_key,
      secret_key,
      None,
      None,
      "octacity-minio-protocol-test",
    ))
    .force_path_style(true)
    .build();
  Client::from_conf(config)
}

fn unix_now() -> u64 {
  SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}
