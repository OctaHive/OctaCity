#![cfg(unix)]

//! Black-box release-candidate Agent gate against the real server and store.
//!
//! The test is ignored in portable suites because its two supported modes need
//! provisioned release machines. Linux runs the Native backend; Apple Silicon
//! macOS runs a Linux guest through Microsandbox. In both cases the server,
//! Agent, source plugin, Octa runner, and task plugins come from isolated,
//! checksummed release installations rather than `target/`.

use std::{
  env,
  fs::{self, File},
  net::{SocketAddr, TcpListener},
  path::{Path, PathBuf},
  process::Stdio,
  time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::SigningKey;
use octacity_release_harness::{InstalledRelease, InstalledToolchain as Toolchain};
use reqwest::Client;
use serde_json::{Value, json};
use tokio::{
  process::{Child, Command},
  time::{Instant, sleep, timeout},
};
use uuid::Uuid;

#[path = "release_vertical_slice/linux_native.rs"]
mod linux_native;
#[path = "release_vertical_slice/release.rs"]
mod release;
mod support;

use support::{get_json, post_management, publish_policy_and_trigger_definition, resource_id, string};

const SIGNING_SEED: [u8; 32] = [7; 32];
const SOURCE_REPOSITORY: &str = "https://github.com/OctaHive/octa.git";
const SOURCE_REVISION: &str = include_str!("../../../.github/octa-source-revision");

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires isolated released products and a provisioned Linux Native runner"]
async fn released_linux_native_matrix_satisfies_the_end_to_end_contract() {
  linux_native::run().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires isolated server, Agent, and Octa release roots plus a provisioned Native or Microsandbox machine"]
async fn released_agent_completes_a_sequential_pipeline_through_rest_and_postgres() {
  let backend = ReleaseBackend::from_environment();
  let postgres_url = required_string("OCTACITY_POSTGRES_URL");
  let object_endpoint = required_string("OCTACITY_MINIO_ENDPOINT");
  let evidence = required_path("OCTACITY_RELEASE_EVIDENCE_DIR", false);
  fs::create_dir_all(&evidence).unwrap();

  let release = release::load(&backend);
  let temporary = tempfile::tempdir().unwrap();
  let client = Client::new();
  let (management_addr, agent_addr) = unused_loopback_addresses();
  let server_config = write_server_config(
    temporary.path(),
    &ServerConfigInput {
      postgres_url: &postgres_url,
      object_endpoint: &object_endpoint,
      cache_endpoint: "https://cache.example",
      toolchain: &release.toolchain,
      management_addr,
      agent_addr,
      cache_addr: None,
    },
  );
  let server_stdout = evidence.join("server.stdout.log");
  let server_stderr = evidence.join("server.stderr.log");
  let mut server = spawn_server(&release.server_binary, &server_config, &server_stdout, &server_stderr);
  let management_origin = format!("http://{management_addr}");
  let agent_origin = format!("http://{agent_addr}");
  wait_for_server_ready(&client, &management_origin, &mut server, &server_stderr).await;
  let run = Uuid::new_v4().simple().to_string();

  let resources = create_pipeline_resources(&client, &management_origin, &run, &backend).await;
  let enrollment = issue_enrollment(&client, &management_origin, &run, &resources.pool_id, &backend).await;
  let agent_id = format!("release-{}-{run}", backend.name());
  let agent_config = write_agent_config(
    temporary.path(),
    &agent_origin,
    &agent_id,
    &enrollment,
    AgentReleasePaths::from(&release),
    &backend,
    &AgentConfigOverrides::default(),
  );
  let stdout_path = evidence.join("agent.stdout.log");
  let stderr_path = evidence.join("agent.stderr.log");
  let mut agent = spawn_agent(&release.agent_binary, &agent_config, &stdout_path, &stderr_path);

  let build = wait_for_terminal_build(
    &client,
    &management_origin,
    &resources.build_id,
    &mut agent,
    &stderr_path,
  )
  .await;
  assert_eq!(build["state"], "succeeded", "Build did not succeed: {build}");
  let attempt = get_json(
    &client,
    format!("{management_origin}/api/v1/attempts/{}", resources.attempt_id),
  )
  .await;
  let jobs = attempt["jobs"].as_array().expect("Attempt jobs must be an array");
  assert_eq!(jobs.len(), 2, "the release slice must execute two DAG nodes");
  assert!(jobs.iter().all(|job| job["state"] == "succeeded"));
  assert!(jobs.iter().all(|job| job["event_cursor"].as_u64().unwrap_or(0) > 0));

  let mut job_evidence = Vec::new();
  for job in jobs {
    let job_id = string(job, "id");
    let detail = get_json(&client, format!("{management_origin}/api/v1/jobs/{job_id}")).await;
    assert_eq!(detail["terminal"]["state"], "succeeded");
    let events = get_json(
      &client,
      format!("{management_origin}/api/v1/jobs/{job_id}/events?after=0&limit=100&wait_ms=0"),
    )
    .await;
    assert!(!events["items"].as_array().unwrap().is_empty());
    job_evidence.push(json!({"job": detail, "events": events}));
  }

  drain_agent(&client, &management_origin, &agent_id, &run).await;
  let status = timeout(Duration::from_secs(45), agent.wait())
    .await
    .expect("released Agent did not stop after drain")
    .expect("failed to wait for released Agent");
  assert!(
    status.success(),
    "released Agent exited unsuccessfully; see {stderr_path:?}"
  );

  fs::write(
    evidence.join("vertical-slice.json"),
    serde_json::to_vec_pretty(&json!({
      "backend": backend.name(),
      "server_release": release.server_manifest,
      "agent_release": release.agent_manifest,
      "octa_capabilities": release.octa_capabilities,
      "build": build,
      "attempt": attempt,
      "jobs": job_evidence
    }))
    .unwrap(),
  )
  .unwrap();
  shutdown_server(&mut server, &server_stderr).await;
}

#[derive(Clone)]
enum ReleaseBackend {
  Native {
    cgroup_root: PathBuf,
    work_root: PathBuf,
    cache_root: PathBuf,
    bubblewrap: PathBuf,
    path: String,
    workspace_bytes: u64,
  },
  Microsandbox {
    work_root: PathBuf,
    state_root: PathBuf,
    executable: PathBuf,
    libkrunfw: PathBuf,
    image: String,
    workspace_bytes: u64,
  },
}

impl ReleaseBackend {
  fn from_environment() -> Self {
    match required_string("OCTACITY_RELEASE_BACKEND").as_str() {
      "native" => {
        assert_eq!(env::consts::OS, "linux", "Native release gate requires Linux");
        Self::Native {
          cgroup_root: required_path("OCTACITY_CONTRACT_NATIVE_CGROUP_ROOT", true),
          work_root: required_path("OCTACITY_CONTRACT_NATIVE_WORK_ROOT", true),
          cache_root: required_path("OCTACITY_RELEASE_NATIVE_CACHE_ROOT", true),
          bubblewrap: required_path("OCTACITY_CONTRACT_NATIVE_BWRAP", true),
          path: required_string("OCTACITY_CONTRACT_NATIVE_PATH"),
          workspace_bytes: required_u64("OCTACITY_CONTRACT_WORKSPACE_BYTES"),
        }
      }
      "microsandbox" => {
        assert_eq!(
          env::consts::OS,
          "macos",
          "macOS release gate requires Apple Silicon macOS"
        );
        assert_eq!(
          env::consts::ARCH,
          "aarch64",
          "Microsandbox macOS gate requires Apple Silicon"
        );
        Self::Microsandbox {
          work_root: required_path("OCTACITY_CONTRACT_MICROSANDBOX_WORK_ROOT", true),
          state_root: required_path("OCTACITY_CONTRACT_MICROSANDBOX_STATE_ROOT", true),
          executable: required_path("OCTACITY_CONTRACT_MICROSANDBOX_EXECUTABLE", true),
          libkrunfw: required_path("OCTACITY_CONTRACT_MICROSANDBOX_LIBKRUNFW", true),
          image: required_string("OCTACITY_CONTRACT_MICROSANDBOX_IMAGE"),
          workspace_bytes: required_u64("OCTACITY_CONTRACT_WORKSPACE_BYTES"),
        }
      }
      value => panic!("unsupported OCTACITY_RELEASE_BACKEND '{value}'"),
    }
  }

  fn name(&self) -> &'static str {
    match self {
      Self::Native { .. } => "native",
      Self::Microsandbox { .. } => "microsandbox",
    }
  }

  fn agent_platform(&self) -> (&'static str, &'static str) {
    match self {
      Self::Native { .. } => ("linux", host_architecture()),
      Self::Microsandbox { .. } => ("macos", "arm64"),
    }
  }

  fn guest_architecture(&self) -> &'static str {
    host_architecture()
  }

  fn runtime_class(&self) -> &'static str {
    match self {
      Self::Native { .. } => "native",
      Self::Microsandbox { .. } => "oci_hypervisor",
    }
  }

  fn capability(&self) -> &'static str {
    match self {
      Self::Native { .. } => "native",
      Self::Microsandbox { .. } => "oci.hypervisor",
    }
  }

  fn immutable_image(&self) -> Value {
    match self {
      Self::Native { .. } => Value::Null,
      Self::Microsandbox { image, .. } => Value::String(image.clone()),
    }
  }

  fn workspace_bytes(&self) -> u64 {
    match self {
      Self::Native { workspace_bytes, .. } | Self::Microsandbox { workspace_bytes, .. } => *workspace_bytes,
    }
  }
}

struct PipelineResources {
  pool_id: String,
  build_id: String,
  attempt_id: String,
}

async fn create_pipeline_resources(
  client: &Client,
  origin: &str,
  run: &str,
  backend: &ReleaseBackend,
) -> PipelineResources {
  let (agent_os, agent_architecture) = backend.agent_platform();
  let pool = post_management(
    client,
    origin,
    "/api/v1/agent-pools",
    &format!("{run}-pool"),
    json!({
      "name": format!("release-{}-{run}", backend.name()),
      "definition": {
        "enabled": true,
        "drain_state": "accepting",
        "admission_policy": {"mode": "allowlist", "platforms": [{
          "operating_system": agent_os,
          "architecture": agent_architecture
        }]},
        "concurrency_limit": 1,
        "fairness_policy": "priority_fifo",
        "static_capacity_limit": 1
      }
    }),
  )
  .await;
  let pool_id = resource_id(&pool);
  let project = post_management(
    client,
    origin,
    "/api/v1/projects",
    &format!("{run}-project"),
    json!({
      "parent_id": null,
      "name": format!("release-{run}")
    }),
  )
  .await;
  let project_id = resource_id(&project);
  let pipeline = post_management(
    client,
    origin,
    "/api/v1/pipelines",
    &format!("{run}-pipeline"),
    json!({
      "project_id": project_id,
      "name": "release",
      "dag": {
        "nodes": [pipeline_node("build", "Build", backend), pipeline_node("test", "Test", backend)],
        "edges": [{"predecessor": "build", "dependent": "test"}]
      }
    }),
  )
  .await;
  let pipeline_id = resource_id(&pipeline);
  let repository = post_management(
    client,
    origin,
    "/api/v1/repositories",
    &format!("{run}-repository"),
    json!({
      "project_id": project_id,
      "name": "octa-fixture",
      "definition": {
        "vcs_integration_id": Uuid::new_v4().to_string(),
        "repository_locator": SOURCE_REPOSITORY,
        "selection": {"allowed_references": [], "default_reference": null, "allow_exact_revision": true}
      }
    }),
  )
  .await;
  let repository_id = resource_id(&repository);
  let configuration = post_management(
    client,
    origin,
    "/api/v1/build-configurations",
    &format!("{run}-configuration"),
    build_configuration(&project_id, &repository_id, &pipeline_id, &pool_id, backend),
  )
  .await;
  let configuration_id = resource_id(&configuration);
  let trigger_id = publish_policy_and_trigger_definition(
    client,
    origin,
    &project_id,
    &repository_id,
    &configuration_id,
    &pool_id,
    backend.runtime_class(),
  )
  .await;
  let trigger = post_management(
    client,
    origin,
    "/api/v1/triggers/manual",
    &format!("{run}-trigger"),
    json!({
      "trigger_id": trigger_id,
      "trigger_version": 1,
      "configuration_id": configuration_id,
      "configuration_version": 1,
      "deduplication_identity": format!("{run}-trigger"),
      "source": {"kind": "exact_revision", "value": SOURCE_REVISION.trim()},
      "parameters": {},
      "priority": 50
    }),
  )
  .await;
  assert_eq!(trigger["outcome"], "accepted");
  PipelineResources {
    pool_id,
    build_id: string(&trigger, "build_id"),
    attempt_id: string(&trigger, "attempt_id"),
  }
}

fn pipeline_node(id: &str, name: &str, backend: &ReleaseBackend) -> Value {
  json!({
    "id": id,
    "name": name,
    "dependency_policy": "all_succeeded",
    "required_capabilities": [backend.capability(), "shell"],
    "execution": {
      "octafile": "example/simple/Octafile.yml",
      "commands": ["echo"],
      "parallel": false,
      "failfast": true
    }
  })
}

fn build_configuration(project: &str, repository: &str, pipeline: &str, pool: &str, backend: &ReleaseBackend) -> Value {
  json!({
    "project_id": project,
    "name": "release",
    "definition": {
      "enabled": true,
      "job_concurrency_limit": 1,
      "repository_id": repository,
      "repository_version": 1,
      "pipeline_id": pipeline,
      "pipeline_version": 1,
      "parameters": {"parameters": {}, "deny_unknown": true},
      "triggers": ["manual"],
      "agent_requirements": {
        "capabilities": [backend.capability(), "shell"],
        "labels": {},
        "minimum_cpu_millis": 1000,
        "minimum_memory_bytes": 536870912_u64,
        "minimum_disk_bytes": backend.workspace_bytes()
      },
      "allowed_pools": [pool],
      "runtime": {
        "class": backend.runtime_class(),
        "operating_system": "linux",
        "architecture": backend.guest_architecture(),
        "immutable_image": backend.immutable_image(),
        "cpu_millis": 1000,
        "memory_bytes": 536870912_u64,
        "writable_disk_bytes": backend.workspace_bytes(),
        "timeout_seconds": 300,
        "network": {"mode": "disabled"},
        "workload_identity_profile": null
      },
      "cache": {"namespace": null, "read": false, "write": false},
      "artifacts": {"artifact_count": 0, "artifact_bytes": 0, "report_count": 0, "report_bytes": 0, "single_output_bytes": 0},
      "retry": {"max_attempts": 1, "retry_on": []}
    }
  })
}

async fn issue_enrollment(client: &Client, origin: &str, run: &str, pool: &str, backend: &ReleaseBackend) -> String {
  let (operating_system, architecture) = backend.agent_platform();
  let response = post_management(
    client,
    origin,
    "/api/v1/agent-enrollments",
    &format!("{run}-enrollment"),
    json!({
      "pool_id": pool,
      "pool_version": 1,
      "expected_platform": {"operating_system": operating_system, "architecture": architecture}
    }),
  )
  .await;
  string(&response, "credential")
}

#[derive(Clone, Copy)]
struct AgentReleasePaths<'a> {
  octa_root: &'a Path,
  source_plugins: &'a Path,
}

impl<'a> From<&'a InstalledRelease> for AgentReleasePaths<'a> {
  fn from(release: &'a InstalledRelease) -> Self {
    Self {
      octa_root: &release.octa_root,
      source_plugins: &release.source_plugins,
    }
  }
}

#[derive(Default)]
struct AgentConfigOverrides<'a> {
  cache_root: Option<&'a Path>,
  cache_read: bool,
  cache_write: bool,
  remote_cache_origin: Option<&'a str>,
  cache_ca_certificate: Option<&'a Path>,
  unrestricted_network: bool,
  upload_origins: &'a [&'a str],
  output_limit_bytes: Option<u64>,
}

fn write_agent_config(
  directory: &Path,
  server: &str,
  agent_id: &str,
  enrollment: &str,
  release: AgentReleasePaths<'_>,
  backend: &ReleaseBackend,
  overrides: &AgentConfigOverrides<'_>,
) -> PathBuf {
  let credentials = directory.join("credentials");
  fs::create_dir(&credentials).unwrap();
  set_mode(&credentials, 0o700);
  let credential = credentials.join("agent-token");
  fs::write(&credential, enrollment).unwrap();
  set_mode(&credential, 0o600);
  let local = directory.join("agent-local");
  fs::create_dir(&local).unwrap();
  let local_work = local.join("work");
  let local_state = local.join("state");
  let local_cache = local.join("cache");
  for path in [&local_work, &local_state, &local_cache] {
    fs::create_dir(path).unwrap();
  }
  let (work, state, cache, runtime, cache_max_bytes, cache_scopes) = match backend {
    ReleaseBackend::Native {
      cgroup_root,
      work_root,
      cache_root,
      bubblewrap,
      path,
      workspace_bytes,
    } => {
      let cache_scope = workspace_bytes.checked_div(2).filter(|value| *value > 0).unwrap();
      (
        work_root,
        &local_state,
        cache_root,
        format!(
          r#"
enabled_runtime_modes = ["native"]
allow_native_execution = true
native_linux_cgroup_root = {}
native_linux_bubblewrap_executable = {}
native_linux_readonly_paths = []
native_linux_pids_limit = 4096
native_environment = {{ PATH = {} }}
oci_engines = []
"#,
          toml_string(cgroup_root),
          toml_string(bubblewrap),
          toml_text(path)
        ),
        cache_scope,
        2,
      )
    }
    ReleaseBackend::Microsandbox {
      work_root,
      state_root,
      executable,
      libkrunfw,
      ..
    } => (
      work_root,
      state_root,
      &local_cache,
      format!(
        r#"
enabled_runtime_modes = ["oci"]
allow_native_execution = false
native_linux_readonly_paths = []
native_linux_pids_limit = 0
native_environment = {{}}
oci_engines = [{{ engine = "microsandbox", executable = {}, libkrunfw = {}, metrics_sample_interval_seconds = 1 }}]
"#,
        toml_string(executable),
        toml_string(libkrunfw)
      ),
      16 * 1024 * 1024,
      1,
    ),
  };
  let cache = overrides.cache_root.unwrap_or(cache);
  let remote_cache_origins = overrides
    .remote_cache_origin
    .map_or_else(|| "[]".to_owned(), |origin| format!("[{}]", toml_text(origin)));
  let cache_ca_certificate = overrides.cache_ca_certificate.map_or_else(String::new, |path| {
    format!("cache.ca_certificate_file = {}\n", toml_string(path))
  });
  let upload_origins = format!(
    "[{}]",
    overrides
      .upload_origins
      .iter()
      .map(|origin| toml_text(origin))
      .collect::<Vec<_>>()
      .join(", ")
  );
  let output_limits = overrides.output_limit_bytes.map_or_else(
    || "{ artifact_count = 0, artifact_bytes = 0, report_count = 0, report_bytes = 0, single_output_bytes = 0 }".to_owned(),
    |bytes| format!("{{ artifact_count = 1, artifact_bytes = {bytes}, report_count = 1, report_bytes = {bytes}, single_output_bytes = {bytes} }}"),
  );
  let verifying_key = SigningKey::from_bytes(&SIGNING_SEED).verifying_key();
  let config = format!(
    r#"
agent_id = {}
server_url = {}
credential_file = {}
work_root = {}
state_root = {}
octa_release_root = {}
source_plugins_dir = {}
workload_identity_profiles = {{}}
cache.root = {}
cache.capacity.max_bytes = {}
cache.capacity.high_watermark_bytes = {}
cache.capacity.low_watermark_bytes = {}
cache.max_scopes = {}
cache.allow_read = {}
cache.allow_write = {}
cache.allowed_remote_origins = {}
{}
cache.native_environment_identities = {{}}
cache.request_timeout_seconds = 5
cache.max_parallel_transfers = 1
maintenance.work_reserve_bytes = 0
maintenance.state_reserve_bytes = 0
maintenance.cache_reserve_bytes = 0
maintenance.disk_check_interval_seconds = 1
{}
allow_unrestricted_network = {}
allowed_network_hosts = []
allowed_upload_origins = {}
max_archive_entries = 1000
upload_timeout_seconds = 30
max_output_limits = {}
max_workspace_bytes = {}
max_spool_bytes = 16777216
max_spool_records = 10000
event_batch_max_bytes = 1048576
event_batch_max_records = 256
event_channel_capacity = 128
poll_timeout_seconds = 2
coordinator_request_timeout_seconds = 10
coordinator_max_body_bytes = 4194304
retry_initial_delay_milliseconds = 100
retry_max_delay_seconds = 2
retry_max_attempts = 4
heartbeat_interval_seconds = 2
lease_safety_margin_seconds = 10
graceful_cancel_timeout_seconds = 5
cleanup_timeout_seconds = 15
runner_hello_timeout_seconds = 5
resource_sample_interval_seconds = 1
resource_sample_timeout_seconds = 1
max_accounting_failures = 3

[server_signing_keys]
test-key = {}

[labels]
release_gate = {}
"#,
    toml_text(agent_id),
    toml_text(server),
    toml_string(&credential),
    toml_string(work),
    toml_string(state),
    toml_string(release.octa_root),
    toml_string(release.source_plugins),
    toml_string(cache),
    cache_max_bytes,
    cache_max_bytes * 9 / 10,
    cache_max_bytes * 8 / 10,
    cache_scopes,
    overrides.cache_read,
    overrides.cache_write,
    remote_cache_origins,
    cache_ca_certificate,
    runtime,
    overrides.unrestricted_network,
    upload_origins,
    output_limits,
    backend.workspace_bytes(),
    toml_text(&STANDARD.encode(verifying_key.as_bytes())),
    toml_text(backend.name()),
  );
  let path = directory.join("agent.toml");
  fs::write(&path, config).unwrap();
  path
}

fn spawn_agent(binary: &Path, config: &Path, stdout: &Path, stderr: &Path) -> Child {
  Command::new(binary)
    .arg("--log-format")
    .arg("json")
    .arg("--log-filter")
    .arg("info")
    .arg("run")
    .arg(config)
    .stdout(Stdio::from(File::create(stdout).unwrap()))
    .stderr(Stdio::from(File::create(stderr).unwrap()))
    .kill_on_drop(true)
    .spawn()
    .unwrap()
}

async fn wait_for_terminal_build(
  client: &Client,
  origin: &str,
  build_id: &str,
  agent: &mut Child,
  log: &Path,
) -> Value {
  let deadline = Instant::now() + Duration::from_secs(600);
  loop {
    if let Some(status) = agent.try_wait().unwrap() {
      panic!(
        "released Agent exited before Build completion with {status}: {}",
        fs::read_to_string(log).unwrap_or_default()
      );
    }
    let build = get_json(client, format!("{origin}/api/v1/builds/{build_id}")).await;
    if matches!(build["state"].as_str(), Some("succeeded" | "failed" | "cancelled")) {
      return build;
    }
    assert!(
      Instant::now() < deadline,
      "timed out waiting for released Agent Build; see {log:?}"
    );
    sleep(Duration::from_millis(500)).await;
  }
}

async fn drain_agent(client: &Client, origin: &str, agent_name: &str, run: &str) {
  let page = get_json(client, format!("{origin}/api/v1/agents?limit=100")).await;
  let agent = page["items"]
    .as_array()
    .unwrap()
    .iter()
    .find(|agent| agent["name"] == agent_name)
    .unwrap_or_else(|| panic!("registered Agent '{agent_name}' was not queryable"));
  let response = client
    .post(format!("{origin}/api/v1/agents/{}/drain", string(agent, "id")))
    .header("idempotency-key", format!("{run}-drain"))
    .header("if-match", format!("\"{}\"", agent["version"].as_u64().unwrap()))
    .json(&json!({"mode": "graceful"}))
    .send()
    .await
    .unwrap();
  let status = response.status();
  let body = response.bytes().await.unwrap();
  assert!(
    status.is_success(),
    "Agent drain returned {status}: {}",
    String::from_utf8_lossy(&body)
  );
}

struct ServerConfigInput<'a> {
  postgres_url: &'a str,
  object_endpoint: &'a str,
  cache_endpoint: &'a str,
  toolchain: &'a Toolchain,
  management_addr: SocketAddr,
  agent_addr: SocketAddr,
  cache_addr: Option<SocketAddr>,
}

fn write_server_config(directory: &Path, input: &ServerConfigInput<'_>) -> PathBuf {
  let postgres = private_file(directory, "postgres-url", input.postgres_url);
  let access_key = private_file(directory, "object-access-key", "octacity");
  let secret_key = private_file(directory, "object-secret-key", "octacity-secret");
  let signing_key = private_file(directory, "signing-key", &STANDARD.encode(SIGNING_SEED));
  let enrollment_key = private_file(directory, "agent-enrollment-key", &STANDARD.encode([8_u8; 32]));
  let cache_key = private_file(directory, "cache-credential-key", &STANDARD.encode([9_u8; 32]));
  let policy = directory.join("job-spec-policy.json");
  fs::write(&policy, serde_json::to_vec(&json!({
    "source": {"provider": "git", "plugin_version": input.toolchain.source_version, "plugin_sha256": input.toolchain.source_digest, "repository_parameter": "url"},
    "octa": {
      "version": input.toolchain.octa_version,
      "runner_sha256": input.toolchain.runner_digest,
      "runner_protocol": input.toolchain.runner_protocol,
      "event_schema": input.toolchain.event_schema,
      "plugin_protocol": input.toolchain.plugin_protocol,
      "plugin_digests": input.toolchain.plugin_digests
    },
    "validity": 900
  })).unwrap()).unwrap();
  let cache_bind = input
    .cache_addr
    .map_or_else(String::new, |address| format!("cache_bind = \"{address}\"\n"));
  let contents = format!(
    r#"
management_bind = "{}"
agent_bind = "{}"
{}
shutdown_grace_milliseconds = 5000
readiness_check_interval_milliseconds = 100
readiness_check_timeout_milliseconds = 2000
agent_registration_lifetime_milliseconds = 900000
agent_enrollment_lifetime_milliseconds = 900000
agent_lease_lifetime_milliseconds = 60000
supported_pipeline_capabilities = ["native", "oci.hypervisor", "shell"]

[postgres]
url_file = {}

[object_storage]
endpoint = {}
region = "us-east-1"
bucket = "octacity-artifacts"
access_key_file = {}
secret_key_file = {}

[signing]
key_id = "test-key"
key_file = {}

[agent_credentials]
enrollment_key_file = {}

[cache]
endpoint = {}
credential_key_file = {}
session_lifetime_milliseconds = 300000

[job_spec]
policy_file = {}
"#,
    input.management_addr,
    input.agent_addr,
    cache_bind,
    toml_string(&postgres),
    toml_text(input.object_endpoint),
    toml_string(&access_key),
    toml_string(&secret_key),
    toml_string(&signing_key),
    toml_string(&enrollment_key),
    toml_text(input.cache_endpoint),
    toml_string(&cache_key),
    toml_string(&policy)
  );
  let path = directory.join("server.toml");
  fs::write(&path, contents).unwrap();
  path
}

fn unused_loopback_addresses() -> (SocketAddr, SocketAddr) {
  let management = TcpListener::bind("127.0.0.1:0").unwrap();
  let agent = TcpListener::bind("127.0.0.1:0").unwrap();
  (management.local_addr().unwrap(), agent.local_addr().unwrap())
}

fn spawn_server(binary: &Path, config: &Path, stdout: &Path, stderr: &Path) -> Child {
  Command::new(binary)
    .arg("--log-format")
    .arg("json")
    .arg("--log-filter")
    .arg("info")
    .arg("run")
    .arg(config)
    .stdout(Stdio::from(File::create(stdout).unwrap()))
    .stderr(Stdio::from(File::create(stderr).unwrap()))
    .kill_on_drop(true)
    .spawn()
    .unwrap()
}

async fn wait_for_server_ready(client: &Client, origin: &str, server: &mut Child, log: &Path) {
  let deadline = Instant::now() + Duration::from_secs(60);
  loop {
    if let Some(status) = server.try_wait().unwrap() {
      panic!(
        "released server exited before readiness with {status}: {}",
        fs::read_to_string(log).unwrap_or_default()
      );
    }
    if client
      .get(format!("{origin}/health/ready"))
      .send()
      .await
      .is_ok_and(|response| response.status().is_success())
    {
      return;
    }
    assert!(
      Instant::now() < deadline,
      "timed out waiting for released server readiness: {}",
      fs::read_to_string(log).unwrap_or_default()
    );
    sleep(Duration::from_millis(100)).await;
  }
}

async fn shutdown_server(server: &mut Child, log: &Path) {
  let process_id = server.id().expect("released server must still be running");
  let process_id = rustix::process::Pid::from_raw(process_id as i32).unwrap();
  rustix::process::kill_process(process_id, rustix::process::Signal::TERM).unwrap();
  let status = timeout(Duration::from_secs(30), server.wait())
    .await
    .expect("released server did not stop before its graceful-shutdown deadline")
    .unwrap();
  assert!(
    status.success(),
    "released server exited unsuccessfully: {}",
    fs::read_to_string(log).unwrap_or_default()
  );
}

fn host_architecture() -> &'static str {
  if cfg!(target_arch = "aarch64") {
    "arm64"
  } else {
    "amd64"
  }
}
fn required_string(name: &str) -> String {
  env::var(name)
    .unwrap_or_else(|_| panic!("{name} must be set"))
    .trim()
    .to_owned()
}
fn required_u64(name: &str) -> u64 {
  required_string(name)
    .parse()
    .unwrap_or_else(|_| panic!("{name} must be a positive integer"))
}
fn required_path(name: &str, must_exist: bool) -> PathBuf {
  let path = PathBuf::from(required_string(name));
  assert!(path.is_absolute(), "{name} must be absolute");
  if must_exist {
    assert!(path.exists(), "{name} does not exist: {path:?}");
  }
  path
}
fn private_file(directory: &Path, name: &str, contents: &str) -> PathBuf {
  let path = directory.join(name);
  fs::write(&path, contents).unwrap();
  set_mode(&path, 0o600);
  path
}
fn set_mode(path: &Path, mode: u32) {
  use std::os::unix::fs::PermissionsExt as _;
  fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
fn toml_string(path: &Path) -> String {
  toml_text(path.to_str().expect("release paths must be UTF-8"))
}
fn toml_text(value: &str) -> String {
  serde_json::to_string(value).unwrap()
}

#[test]
fn generated_agent_configuration_keeps_backend_fields_at_the_top_level() {
  let directory = tempfile::tempdir().unwrap();
  let roots = ["work", "state", "cache", "octa", "sources", "msb", "libkrunfw"];
  for root in roots {
    fs::create_dir(directory.path().join(root)).unwrap();
  }
  let source_plugins = directory.path().join("sources");
  let octa_root = directory.path().join("octa");
  let release = AgentReleasePaths {
    source_plugins: &source_plugins,
    octa_root: &octa_root,
  };
  let backend = ReleaseBackend::Microsandbox {
    work_root: directory.path().join("work"),
    state_root: directory.path().join("state"),
    executable: directory.path().join("msb"),
    libkrunfw: directory.path().join("libkrunfw"),
    image: format!("example.invalid/octa@sha256:{}", "a".repeat(64)),
    workspace_bytes: 1024 * 1024,
  };
  let cache = directory.path().join("matrix-cache");
  fs::create_dir(&cache).unwrap();
  let certificate = directory.path().join("cache-ca.pem");
  fs::write(&certificate, "test certificate").unwrap();
  let upload_origins = ["http://127.0.0.1:9000"];
  let config = write_agent_config(
    directory.path(),
    "http://127.0.0.1:12345",
    "config-shape",
    "credential",
    release,
    &backend,
    &AgentConfigOverrides {
      cache_root: Some(&cache),
      cache_read: true,
      cache_write: true,
      remote_cache_origin: Some("https://127.0.0.1:8443"),
      cache_ca_certificate: Some(&certificate),
      unrestricted_network: true,
      upload_origins: &upload_origins,
      output_limit_bytes: Some(4096),
    },
  );
  let document: toml::Value = toml::from_str(&fs::read_to_string(config).unwrap()).unwrap();
  assert!(document["allowed_upload_origins"].is_array());
  assert_eq!(document["cache"]["root"].as_str(), cache.to_str());
  assert_eq!(document["cache"]["ca_certificate_file"].as_str(), certificate.to_str());
  assert_eq!(document["cache"]["allow_read"].as_bool(), Some(true));
  assert_eq!(document["max_output_limits"]["artifact_bytes"].as_integer(), Some(4096));
  assert_eq!(document["oci_engines"].as_array().unwrap().len(), 1);
  assert_eq!(document["oci_engines"][0]["engine"].as_str(), Some("microsandbox"));
}
