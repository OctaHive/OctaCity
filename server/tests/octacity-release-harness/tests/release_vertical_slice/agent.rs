//! Released Agent configuration and process lifecycle helpers.

use std::{
  fs::{self, File},
  path::{Path, PathBuf},
  process::Stdio,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::SigningKey;
use octacity_protocol::OutputLimits;
use octacity_release_harness::InstalledRelease;
use tokio::process::{Child, Command};

use super::{
  RELEASE_CACHE_REQUEST_TIMEOUT_SECONDS, ReleaseBackend, SIGNING_SEED, host_architecture, set_mode, toml_string,
  toml_text,
};

#[derive(Clone, Copy)]
pub(super) struct AgentReleasePaths<'a> {
  pub(super) octa_root: &'a Path,
  pub(super) source_plugins: &'a Path,
}

impl<'a> From<&'a InstalledRelease> for AgentReleasePaths<'a> {
  fn from(release: &'a InstalledRelease) -> Self {
    Self {
      octa_root: &release.octa_root,
      source_plugins: &release.source_plugins,
    }
  }
}

pub(super) struct AgentConfigOverrides<'a> {
  pub(super) cache_read: bool,
  pub(super) cache_write: bool,
  pub(super) remote_cache_origin: Option<&'a str>,
  pub(super) cache_ca_certificate: Option<&'a Path>,
  pub(super) unrestricted_network: bool,
  pub(super) upload_origins: &'a [&'a str],
  pub(super) output_limits: Option<OutputLimits>,
  pub(super) tool_executables: &'a [AgentToolExecutable<'a>],
}

pub(super) struct AgentToolExecutable<'a> {
  pub(super) product: &'a str,
  pub(super) path: &'a Path,
  pub(super) version: &'a str,
  pub(super) platform: &'a str,
  pub(super) sha256: &'a str,
}

impl<'a> AgentConfigOverrides<'a> {
  pub(super) fn restricted_without_outputs(upload_origins: &'a [&'a str]) -> Self {
    Self {
      cache_read: true,
      cache_write: false,
      remote_cache_origin: None,
      cache_ca_certificate: None,
      unrestricted_network: false,
      upload_origins,
      output_limits: None,
      tool_executables: &[],
    }
  }
}

pub(super) fn write_agent_config(
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
  set_mode(&local, 0o700);
  let local_work = local.join("work");
  let local_state = local.join("state");
  let local_cache = local.join("cache");
  for path in [&local_work, &local_state, &local_cache] {
    fs::create_dir(path).unwrap();
    set_mode(path, 0o700);
  }
  let native_cache_identities = match backend {
    ReleaseBackend::Native {
      environment_identity, ..
    } => format!(
      "{{ {} = {} }}",
      toml_text(&format!("linux-{}", host_architecture())),
      toml_text(environment_identity)
    ),
    ReleaseBackend::Microsandbox { .. } | ReleaseBackend::Containerd { .. } | ReleaseBackend::AppleVf { .. } => {
      "{}".to_owned()
    }
  };
  let (work, state, cache, runtime, cache_max_bytes, cache_scopes) = match backend {
    ReleaseBackend::Native {
      cgroup_root,
      work_root,
      cache_root,
      bubblewrap,
      path,
      workspace_bytes,
      ..
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
      environment_identity,
      ..
    } => (
      work_root,
      state_root,
      &local_cache,
      format!(
        r#"
enabled_runtime_modes = []
allow_native_execution = false
native_linux_readonly_paths = []
native_linux_pids_limit = 0
native_environment = {{}}
oci_engines = []
virtualization_providers = [{{ provider = "microsandbox", environment_identity = {}, executable = {}, libkrunfw = {}, metrics_sample_interval_seconds = 1 }}]
"#,
        toml_text(environment_identity),
        toml_string(executable),
        toml_string(libkrunfw)
      ),
      16 * 1024 * 1024,
      1,
    ),
    ReleaseBackend::Containerd {
      work_root,
      cache_root,
      state_root,
      endpoint,
      namespace,
      snapshotter,
      runtime: container_runtime,
      registry_config_dir,
      environment_identity,
      ..
    } => {
      let registry_config = registry_config_dir.as_ref().map_or_else(String::new, |path| {
        format!(", registry_config_dir = {}", toml_string(path))
      });
      (
        work_root,
        state_root,
        cache_root,
        format!(
          r#"
enabled_runtime_modes = []
allow_native_execution = false
native_linux_readonly_paths = []
native_linux_pids_limit = 0
native_environment = {{}}
oci_engines = []
isolation_providers = [{{ provider = "containerd", environment_identity = {}, endpoint = {}, namespace = {}, snapshotter = {}, runtime = {}, pids_limit = 4096, open_files_limit = 65536{} }}]
"#,
          toml_text(environment_identity),
          toml_string(endpoint),
          toml_text(namespace),
          toml_text(snapshotter),
          toml_text(container_runtime),
          registry_config
        ),
        16 * 1024 * 1024,
        1,
      )
    }
    ReleaseBackend::AppleVf {
      work_root,
      cache_root,
      state_root,
      executable,
      environment_identity,
      ..
    } => (
      work_root,
      state_root,
      cache_root,
      format!(
        r#"
enabled_runtime_modes = []
allow_native_execution = false
native_linux_readonly_paths = []
native_linux_pids_limit = 0
native_environment = {{}}
oci_engines = []
isolation_providers = [{{ provider = "apple_vf", environment_identity = {}, executable = {}, open_files_limit = 65536 }}]
"#,
        toml_text(environment_identity),
        toml_string(executable)
      ),
      64 * 1024 * 1024,
      1,
    ),
  };
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
  let output_limits = overrides.output_limits.as_ref().map_or_else(
    || "{ artifact_count = 0, artifact_bytes = 0, report_count = 0, report_bytes = 0, single_output_bytes = 0 }".to_owned(),
    |limits| format!(
      "{{ artifact_count = {}, artifact_bytes = {}, report_count = {}, report_bytes = {}, single_output_bytes = {} }}",
      limits.artifact_count,
      limits.artifact_bytes,
      limits.report_count,
      limits.report_bytes,
      limits.single_output_bytes
    ),
  );
  let tool_executables = format!(
    "{{{}}}",
    overrides
      .tool_executables
      .iter()
      .map(|tool| format!(
        "{} = {{ path = {}, version = {}, platform = {}, sha256 = {} }}",
        toml_text(tool.product),
        toml_string(tool.path),
        toml_text(tool.version),
        toml_text(tool.platform),
        toml_text(tool.sha256)
      ))
      .collect::<Vec<_>>()
      .join(", ")
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
tool_executables = {tool_executables}
cache.root = {}
cache.capacity.max_bytes = {}
cache.capacity.high_watermark_bytes = {}
cache.capacity.low_watermark_bytes = {}
cache.max_scopes = {}
cache.allow_read = {}
cache.allow_write = {}
cache.allowed_remote_origins = {}
{}
cache.native_environment_identities = {}
cache.request_timeout_seconds = {RELEASE_CACHE_REQUEST_TIMEOUT_SECONDS}
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
resource_sample_timeout_seconds = {}
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
    native_cache_identities,
    runtime,
    overrides.unrestricted_network,
    upload_origins,
    output_limits,
    backend.workspace_bytes(),
    backend.resource_sample_timeout_seconds(),
    toml_text(&STANDARD.encode(verifying_key.as_bytes())),
    toml_text(backend.name()),
  );
  let path = directory.join("agent.toml");
  fs::write(&path, config).unwrap();
  path
}

pub(super) fn spawn_agent(binary: &Path, config: &Path, stdout: &Path, stderr: &Path) -> Child {
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
