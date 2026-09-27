//! Portable black-box contract for direct Host execution by a released Agent.

use std::{
  collections::BTreeMap,
  env, fs,
  net::SocketAddr,
  path::{Path, PathBuf},
  sync::{Arc, Mutex},
  time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
  Json, Router,
  extract::{Path as AxumPath, State},
  routing::post,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer as _, SigningKey};
use octacity_protocol::{
  AcquireLeaseRequest, AcquireLeaseResponse, AgentCredentialToken, AgentInventory, AppendEventsRequest,
  AppendEventsResponse, AttemptEventKind, CompleteLeaseRequest, CompleteLeaseResponse, EXECUTION_CONTRACT_V2,
  ExecutionMode, ExecutionSpec, ExecutionTargetV2, HeartbeatDirective, HeartbeatRequest, HeartbeatResponse,
  JobCompletionStatus, JobSpecV2, LeaseAssignment, MAX_APPEND_REQUEST_OVERHEAD_BYTES, NetworkPolicy, OctaSpec,
  OutputLimits, PlatformArchitecture, PlatformOs, PlatformSpec, RegisterAgentRequest, RegisterAgentResponse,
  RuntimeSpecV2, SIGNATURE_ALGORITHM, SignedEnvelope, SourceSpec, guarantees_for,
};
use octacity_release_harness::{AgentRuntimeBundles, InstalledAgentRuntime, install_agent_runtime};
use serde_json::{Value, json};
use tokio::{net::TcpListener, process::Command, sync::oneshot, time::timeout};

const SIGNING_SEED: [u8; 32] = [7; 32];
const SOURCE_REPOSITORY: &str = "https://github.com/OctaHive/octa.git";
const SOURCE_REVISION: &str = include_str!("../../../../.github/octa-source-revision");
const WORKSPACE_BYTES: u64 = 32 * 1024 * 1024;
const EVENT_BATCH_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScenarioPhase {
  SuccessReady,
  SuccessActive,
  CancellationReady,
  CancellationActive,
  Draining,
}

struct CoordinatorState {
  phase: ScenarioPhase,
  inventory: Option<AgentInventory>,
  events: Vec<octacity_protocol::AttemptEventEnvelope>,
  completions: BTreeMap<String, CompleteLeaseRequest>,
  largest_batch_bytes: usize,
  append_batches: BTreeMap<String, usize>,
}

impl Default for CoordinatorState {
  fn default() -> Self {
    Self {
      phase: ScenarioPhase::SuccessReady,
      inventory: None,
      events: Vec::new(),
      completions: BTreeMap::new(),
      largest_batch_bytes: 0,
      append_batches: BTreeMap::new(),
    }
  }
}

type SharedState = Arc<Mutex<CoordinatorState>>;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires packaged Agent and Octa release roots for this host"]
async fn released_host_agent_satisfies_the_portable_execution_contract() {
  let root = TestRoot::new();
  let installation = install_agent_runtime(
    &AgentRuntimeBundles {
      agent: required_directory("OCTACITY_RELEASE_AGENT_ROOT"),
      octa: required_directory("OCTACITY_CONTRACT_OCTA_RELEASE_ROOT"),
    },
    &root.path().join("installed"),
  )
  .expect("released Agent runtime must install and verify");

  let state = Arc::new(Mutex::new(CoordinatorState::default()));
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = listener.local_addr().unwrap();
  let (server_stop, stop_server) = oneshot::channel();
  let server_state = state.clone();
  let server = tokio::spawn(async move {
    axum::serve(listener, coordinator_router(server_state))
      .with_graceful_shutdown(async {
        let _ = stop_server.await;
      })
      .await
  });

  let agent_root = root.path().join("agent-state");
  create_private_directory(&agent_root);
  let config = write_agent_config(&agent_root, address, &installation);
  let stdout_path = root.path().join("agent.stdout.log");
  let stdout = fs::File::create(&stdout_path).unwrap();
  let stderr_path = root.path().join("agent.stderr.log");
  let stderr = fs::File::create(&stderr_path).unwrap();
  let mut agent = Command::new(&installation.agent_binary)
    .arg("--log-format")
    .arg("json")
    .arg("run")
    .arg(&config)
    .stdout(stdout)
    .stderr(stderr)
    .spawn()
    .expect("released Agent must start");
  let status = timeout(Duration::from_secs(180), agent.wait())
    .await
    .unwrap_or_else(|_| panic!("released Host Agent timed out: {}", read_log(&stderr_path)))
    .expect("released Agent wait must succeed");
  assert!(
    status.success(),
    "released Host Agent failed with {status}; stdout: {}; stderr: {}",
    read_log(&stdout_path),
    read_log(&stderr_path)
  );

  let (inventory, success, cancelled, events, largest_batch_bytes, success_batches) = {
    let snapshot = state.lock().unwrap();
    assert_eq!(snapshot.phase, ScenarioPhase::Draining);
    (
      snapshot.inventory.clone().expect("Agent must register inventory"),
      snapshot
        .completions
        .get("host-success")
        .cloned()
        .expect("success completion"),
      snapshot
        .completions
        .get("host-cancellation")
        .cloned()
        .expect("cancellation completion"),
      snapshot.events.clone(),
      snapshot.largest_batch_bytes,
      snapshot.append_batches.get("host-success").copied().unwrap_or_default(),
    )
  };
  assert_host_inventory(&inventory);
  assert_eq!(
    success.status,
    JobCompletionStatus::Succeeded,
    "successful Host scenario completion: {}; Agent stderr: {}",
    serde_json::to_string_pretty(&success).unwrap(),
    read_log(&stderr_path)
  );
  assert_resource_accounting(&success);
  assert!(
    success
      .final_usage
      .as_ref()
      .is_some_and(|usage| usage.io_written_bytes > 0),
    "successful Host workload must account for filesystem writes"
  );
  assert_eq!(cancelled.status, JobCompletionStatus::Cancelled);
  assert_resource_accounting(&cancelled);
  assert!(
    events
      .iter()
      .any(|event| matches!(event.kind, AttemptEventKind::Runner { .. }))
  );
  assert!(
    success_batches > 1,
    "bounded Host output must be delivered in multiple batches"
  );
  assert!(largest_batch_bytes <= EVENT_BATCH_BYTES + MAX_APPEND_REQUEST_OVERHEAD_BYTES);
  if let Some(evidence) = env::var_os("OCTACITY_RELEASE_HOST_EVIDENCE_DIR") {
    let evidence = PathBuf::from(evidence);
    fs::create_dir_all(&evidence).unwrap();
    fs::write(
      evidence.join("released-host-agent.json"),
      serde_json::to_vec_pretty(&json!({
        "agent_release": installation.agent_manifest,
        "host_platform": inventory.host_platform,
        "execution": inventory.executions,
        "success": success,
        "cancellation": cancelled,
      }))
      .unwrap(),
    )
    .unwrap();
  }
  assert!(
    directory_is_empty(&agent_root.join("work")),
    "Host work root was not cleaned"
  );

  let _ = server_stop.send(());
  server.await.unwrap().unwrap();
}

fn coordinator_router(state: SharedState) -> Router {
  Router::new()
    .route("/api/v1/agents/register", post(register))
    .route("/api/v1/agents/{agent_id}/leases:acquire", post(acquire))
    .route("/api/v1/leases/{lease_id}/heartbeat", post(heartbeat))
    .route("/api/v1/leases/{lease_id}/events:append", post(append_events))
    .route("/api/v1/leases/{lease_id}/complete", post(complete))
    .with_state(state)
}

async fn register(
  State(state): State<SharedState>,
  Json(request): Json<RegisterAgentRequest>,
) -> Json<RegisterAgentResponse> {
  request.validate().unwrap();
  assert_host_inventory(&request.inventory);
  state.lock().unwrap().inventory = Some(request.inventory);
  Json(RegisterAgentResponse {
    protocol_version: octacity_protocol::COORDINATOR_PROTOCOL_VERSION,
    request_id: request.request_id,
    registration_id: "host-registration".to_owned(),
    execution_contract_version: Some(EXECUTION_CONTRACT_V2),
    max_retry_delay_ms: 1_000,
  })
}

async fn acquire(
  State(state): State<SharedState>,
  AxumPath(agent_id): AxumPath<String>,
  Json(request): Json<AcquireLeaseRequest>,
) -> Json<AcquireLeaseResponse> {
  request.validate().unwrap();
  assert_eq!(agent_id, "released-host");
  let mut state = state.lock().unwrap();
  let response = if !request.accept_jobs {
    no_work(request.request_id)
  } else {
    match state.phase {
      ScenarioPhase::SuccessReady => {
        state.phase = ScenarioPhase::SuccessActive;
        lease_response(&request.request_id, &state, "host-success", false)
      }
      ScenarioPhase::CancellationReady => {
        state.phase = ScenarioPhase::CancellationActive;
        lease_response(&request.request_id, &state, "host-cancellation", true)
      }
      ScenarioPhase::Draining => AcquireLeaseResponse::Drain {
        protocol_version: octacity_protocol::COORDINATOR_PROTOCOL_VERSION,
        request_id: request.request_id,
      },
      ScenarioPhase::SuccessActive | ScenarioPhase::CancellationActive => no_work(request.request_id),
    }
  };
  Json(response)
}

async fn heartbeat(
  State(state): State<SharedState>,
  AxumPath(lease_id): AxumPath<String>,
  Json(request): Json<HeartbeatRequest>,
) -> Json<HeartbeatResponse> {
  assert_eq!(lease_id, request.lease.lease_id);
  let state = state.lock().unwrap();
  let cancel_ready = request.lease.job_id == "host-cancellation"
    && state
      .events
      .iter()
      .any(|event| event.job_id == "host-cancellation" && matches!(event.kind, AttemptEventKind::Runner { .. }));
  Json(HeartbeatResponse {
    protocol_version: octacity_protocol::COORDINATOR_PROTOCOL_VERSION,
    request_id: request.request_id,
    directive: if cancel_ready {
      HeartbeatDirective::Cancel
    } else {
      HeartbeatDirective::Continue {
        expires_at: unix_now() + 120,
      }
    },
  })
}

async fn append_events(
  State(state): State<SharedState>,
  AxumPath(lease_id): AxumPath<String>,
  Json(request): Json<AppendEventsRequest>,
) -> Json<AppendEventsResponse> {
  assert_eq!(lease_id, request.lease.lease_id);
  request.validate().unwrap();
  let acknowledged_sequence = request.events.last().unwrap().stream_sequence;
  let encoded_bytes = serde_json::to_vec(&request).unwrap().len();
  let mut state = state.lock().unwrap();
  state.largest_batch_bytes = state.largest_batch_bytes.max(encoded_bytes);
  *state.append_batches.entry(request.lease.job_id.clone()).or_default() += 1;
  state.events.extend(request.events);
  Json(AppendEventsResponse {
    protocol_version: octacity_protocol::COORDINATOR_PROTOCOL_VERSION,
    request_id: request.request_id,
    acknowledged_sequence,
  })
}

async fn complete(
  State(state): State<SharedState>,
  AxumPath(lease_id): AxumPath<String>,
  Json(request): Json<CompleteLeaseRequest>,
) -> Json<CompleteLeaseResponse> {
  assert_eq!(lease_id, request.lease.lease_id);
  request.validate().unwrap();
  let mut state = state.lock().unwrap();
  match request.lease.job_id.as_str() {
    "host-success" => {
      assert_eq!(state.phase, ScenarioPhase::SuccessActive);
      state.phase = ScenarioPhase::CancellationReady;
    }
    "host-cancellation" => {
      assert_eq!(state.phase, ScenarioPhase::CancellationActive);
      state.phase = ScenarioPhase::Draining;
    }
    other => panic!("unexpected completed job '{other}'"),
  }
  state.completions.insert(request.lease.job_id.clone(), request.clone());
  Json(CompleteLeaseResponse {
    protocol_version: octacity_protocol::COORDINATOR_PROTOCOL_VERSION,
    request_id: request.request_id,
    completion_id: request.completion_id,
  })
}

fn lease_response(
  request_id: &str,
  state: &CoordinatorState,
  job_id: &str,
  cancellation: bool,
) -> AcquireLeaseResponse {
  let inventory = state.inventory.as_ref().expect("Agent must register before polling");
  let now = unix_now();
  let spec = host_spec(inventory, job_id, cancellation, now);
  AcquireLeaseResponse::Lease {
    protocol_version: octacity_protocol::COORDINATOR_PROTOCOL_VERSION,
    request_id: request_id.to_owned(),
    lease: LeaseAssignment {
      lease_id: format!("lease-{job_id}"),
      job_id: job_id.to_owned(),
      attempt: 1,
      fencing_token: format!("fence-{job_id}"),
      issued_at: now,
      expires_at: now + 120,
      signed_job_spec: signed_spec(&spec),
    },
  }
}

fn no_work(request_id: String) -> AcquireLeaseResponse {
  AcquireLeaseResponse::NoWork {
    protocol_version: octacity_protocol::COORDINATOR_PROTOCOL_VERSION,
    request_id,
    retry_after_ms: 100,
  }
}

fn host_spec(inventory: &AgentInventory, job_id: &str, cancellation: bool, now: u64) -> JobSpecV2 {
  let source = inventory
    .source_plugins
    .iter()
    .find(|plugin| plugin.name == "git")
    .expect("released Agent must advertise Git source");
  let platform = host_platform();
  JobSpecV2 {
    protocol_version: EXECUTION_CONTRACT_V2,
    job_id: job_id.to_owned(),
    attempt: 1,
    issued_at: now.saturating_sub(1),
    expires_at: now + 120,
    source: SourceSpec {
      provider: "git".to_owned(),
      plugin_version: source.version.clone(),
      plugin_sha256: source.sha256.clone(),
      revision: SOURCE_REVISION.trim().to_owned(),
      reference: None,
      parameters: BTreeMap::from([("url".to_owned(), Value::String(SOURCE_REPOSITORY.to_owned()))]),
    },
    octa: OctaSpec {
      version: inventory.octa.version.clone(),
      runner_sha256: inventory.octa.runner_sha256.clone(),
      runner_protocol: *inventory.octa.runner_protocols.first().unwrap(),
      event_schema: *inventory.octa.event_schemas.first().unwrap(),
      plugin_protocol: *inventory.octa.plugin_protocols.first().unwrap(),
      plugin_digests: inventory
        .octa
        .plugins
        .iter()
        .map(|plugin| (plugin.name.clone(), plugin.sha256.clone()))
        .collect(),
    },
    execution: ExecutionSpec {
      octafile: Some("example/simple/Octafile.yml".to_owned()),
      commands: vec!["echo".to_owned()],
      variables: BTreeMap::from([(
        "NAME".to_owned(),
        if cancellation {
          "OctaCity; sleep 60".to_owned()
        } else {
          "OctaCity; i=0; while [ $i -lt 12000 ]; do printf '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\\n' >> host-resource-probe; i=$((i+1)); done; head -c 20000 host-resource-probe; sleep 2".to_owned()
        },
      )]),
      arguments: Vec::new(),
      concurrency: None,
      parallel: false,
      failfast: true,
      secrets_profile: None,
    },
    runtime: RuntimeSpecV2 {
      target: ExecutionTargetV2 {
        mode: ExecutionMode::Host,
        host_platform: platform,
        target_platform: platform,
        required_guarantees: guarantees_for(ExecutionMode::Host),
        immutable_image: None,
      },
      cpu_millis: 1_000,
      memory_bytes: 512 * 1024 * 1024,
      writable_disk_bytes: WORKSPACE_BYTES,
      timeout_seconds: 90,
      network: NetworkPolicy::Unrestricted,
      workload_identity_profile: None,
    },
    cache: None,
    outputs: OutputLimits {
      artifact_count: 0,
      artifact_bytes: 0,
      report_count: 0,
      report_bytes: 0,
      single_output_bytes: 0,
    },
  }
}

fn signed_spec(spec: &JobSpecV2) -> SignedEnvelope {
  let payload = serde_json::to_vec(spec).unwrap();
  let key = SigningKey::from_bytes(&SIGNING_SEED);
  SignedEnvelope {
    key_id: "test-key".to_owned(),
    algorithm: SIGNATURE_ALGORITHM.to_owned(),
    payload: STANDARD.encode(&payload),
    signature: STANDARD.encode(key.sign(&payload).to_bytes()),
  }
}

fn assert_host_inventory(inventory: &AgentInventory) {
  assert_eq!(inventory.agent_id, "released-host");
  assert!(inventory.execution_contract.contains(EXECUTION_CONTRACT_V2));
  let capability = inventory
    .executions
    .iter()
    .find(|capability| capability.mode == ExecutionMode::Host)
    .expect("released Agent must advertise Host execution");
  assert_eq!(capability.provider.as_str(), "host");
  assert_eq!(capability.host_platform, host_platform());
  assert_eq!(capability.target_platform, host_platform());
  assert!(capability.guarantees.is_empty(), "Host must not claim isolation");
  assert!(!capability.immutable_images);
}

fn assert_resource_accounting(completion: &CompleteLeaseRequest) {
  let evidence = completion
    .execution
    .as_ref()
    .expect("Host completion must retain provider evidence");
  assert_eq!(evidence.provider.as_str(), "host");
  assert_eq!(evidence.target.mode, ExecutionMode::Host);
  let usage = completion.final_usage.as_ref().expect("Host must report final usage");
  assert!(usage.cpu_time_ms > 0);
  assert!(usage.memory_peak_bytes > 0);
  assert!(usage.memory_peak_bytes >= usage.memory_current_bytes);
  assert!(usage.disk_peak_bytes >= usage.disk_current_bytes);
  assert!(usage.disk_peak_bytes >= 512 * 1024);
}

fn write_agent_config(root: &Path, address: SocketAddr, release: &InstalledAgentRuntime) -> PathBuf {
  let credentials = root.join("credentials");
  let work = root.join("work");
  let state = root.join("state");
  let cache = root.join("cache");
  for path in [&credentials, &work, &state, &cache] {
    create_private_directory(path);
  }
  let credential = credentials.join("agent-token");
  let token = AgentCredentialToken::enrollment("host-enrollment", [8; 32]).unwrap();
  fs::write(&credential, token.encode().as_bytes()).unwrap();
  set_private_file(&credential);
  let environment = host_environment()
    .into_iter()
    .map(|(name, value)| format!("{} = {}", toml_string(&name), toml_string(&value)))
    .collect::<Vec<_>>()
    .join(", ");
  let verifying_key = SigningKey::from_bytes(&SIGNING_SEED).verifying_key();
  let config = root.join("agent.toml");
  fs::write(
    &config,
    format!(
      r#"agent_id = "released-host"
server_url = "http://{address}"
credential_file = {credential}
work_root = {work}
state_root = {state}
octa_release_root = {octa}
source_plugins_dir = {sources}
enabled_runtime_modes = []
allow_native_execution = false
native_linux_readonly_paths = []
native_linux_pids_limit = 0
native_environment = {{}}
allow_host_execution = true
host_environment_identity = "host-release-v1"
host_environment = {{ {environment} }}
oci_engines = []
workload_identity_profiles = {{}}
allow_unrestricted_network = true
host_accounting_max_entries = 100000
allowed_network_hosts = []
allowed_upload_origins = ["https://objects.invalid"]
max_archive_entries = 1000
upload_timeout_seconds = 30
max_output_limits = {{ artifact_count = 0, artifact_bytes = 0, report_count = 0, report_bytes = 0, single_output_bytes = 0 }}
max_workspace_bytes = {WORKSPACE_BYTES}
max_spool_bytes = 16777216
max_spool_records = 10000
event_batch_max_bytes = {EVENT_BATCH_BYTES}
event_batch_max_records = 256
event_channel_capacity = 128
poll_timeout_seconds = 2
coordinator_request_timeout_seconds = 10
coordinator_max_body_bytes = 4194304
retry_initial_delay_milliseconds = 100
retry_max_delay_seconds = 2
retry_max_attempts = 4
heartbeat_interval_seconds = 1
lease_safety_margin_seconds = 10
graceful_cancel_timeout_seconds = 5
cleanup_timeout_seconds = 15
runner_hello_timeout_seconds = 5
resource_sample_interval_seconds = 1
resource_sample_timeout_seconds = 1
max_accounting_failures = 3

[cache]
root = {cache}
capacity = {{ max_bytes = 4194304, high_watermark_bytes = 3145728, low_watermark_bytes = 2097152 }}
max_scopes = 1
allow_read = true
allow_write = false
allowed_remote_origins = []
native_environment_identities = {{}}
request_timeout_seconds = 5
max_parallel_transfers = 1

[maintenance]
work_reserve_bytes = 0
state_reserve_bytes = 0
cache_reserve_bytes = 0
disk_check_interval_seconds = 1

[server_signing_keys]
test-key = "{}"
"#,
      STANDARD.encode(verifying_key.as_bytes()),
      credential = toml_string(&credential.display().to_string()),
      work = toml_string(&work.display().to_string()),
      state = toml_string(&state.display().to_string()),
      octa = toml_string(&release.octa_root.display().to_string()),
      sources = toml_string(&release.source_plugins.display().to_string()),
      cache = toml_string(&cache.display().to_string()),
    ),
  )
  .unwrap();
  config
}

fn host_environment() -> BTreeMap<String, String> {
  let mut environment = BTreeMap::new();
  for name in [
    "PATH",
    "COMSPEC",
    "HOME",
    "PATHEXT",
    "SYSTEMROOT",
    "TMP",
    "TEMP",
    "WINDIR",
  ] {
    if let Ok(value) = env::var(name)
      && !value.is_empty()
    {
      environment.insert(name.to_owned(), value);
    }
  }
  assert!(environment.contains_key("PATH"), "Host contract requires PATH");
  environment
}

fn host_platform() -> PlatformSpec {
  PlatformSpec {
    os: match env::consts::OS {
      "linux" => PlatformOs::Linux,
      "macos" => PlatformOs::Macos,
      "windows" => PlatformOs::Windows,
      other => panic!("unsupported Host operating system '{other}'"),
    },
    architecture: match env::consts::ARCH {
      "x86_64" => PlatformArchitecture::Amd64,
      "aarch64" => PlatformArchitecture::Arm64,
      other => panic!("unsupported Host architecture '{other}'"),
    },
  }
}

fn unix_now() -> u64 {
  SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

fn toml_string(value: &str) -> String {
  toml::Value::String(value.to_owned()).to_string()
}

fn read_log(path: &Path) -> String {
  fs::read_to_string(path).unwrap_or_default()
}

fn directory_is_empty(path: &Path) -> bool {
  path.read_dir().unwrap().next().is_none()
}

fn required_directory(name: &str) -> PathBuf {
  let path = PathBuf::from(env::var_os(name).unwrap_or_else(|| panic!("{name} must be set")));
  assert!(
    path.is_absolute() && path.is_dir(),
    "{name} must be an absolute directory"
  );
  path
}

fn create_private_directory(path: &Path) {
  octacity_private_fs::create_private_directory(path).unwrap();
}

#[cfg(unix)]
fn set_private_file(path: &Path) {
  use std::os::unix::fs::PermissionsExt as _;
  fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

#[cfg(windows)]
fn set_private_file(_path: &Path) {}

struct TestRoot {
  path: PathBuf,
  #[cfg(not(windows))]
  _temporary: tempfile::TempDir,
}

impl TestRoot {
  fn new() -> Self {
    #[cfg(windows)]
    {
      let profile = fs::canonicalize(env::var_os("USERPROFILE").expect("Windows requires USERPROFILE")).unwrap();
      let volume = profile.ancestors().last().expect("USERPROFILE must have a volume");
      let path = volume.join(format!(".octacity-host-release-{}", uuid::Uuid::new_v4().simple()));
      create_private_directory(&path);
      Self { path }
    }
    #[cfg(not(windows))]
    {
      let temporary = tempfile::tempdir().unwrap();
      Self {
        path: temporary.path().to_owned(),
        _temporary: temporary,
      }
    }
  }

  fn path(&self) -> &Path {
    &self.path
  }
}

#[cfg(windows)]
impl Drop for TestRoot {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.path);
  }
}
