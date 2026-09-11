//! Agent configuration parsing and invariant tests.

use std::{fs::File, io::Write as _};

use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use tempfile::TempDir;

struct Fixture {
  _temp: TempDir,
  config: AgentConfig,
}

impl Fixture {
  fn new() -> Self {
    let temp = tempfile::tempdir().unwrap();
    let credential = temp.path().join("credential");
    File::create(&credential).unwrap().write_all(b"token").unwrap();
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt as _;
      fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let directory = |name: &str| {
      let path = temp.path().join(name);
      fs::create_dir(&path).unwrap();
      path
    };
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[9; 32]);
    let config = AgentConfig {
      agent_id: "agent-1".to_owned(),
      server_url: "https://octacity.example".to_owned(),
      credential_file: credential,
      server_signing_keys: BTreeMap::from([(
        "primary".to_owned(),
        BASE64.encode(signing_key.verifying_key().as_bytes()),
      )]),
      labels: BTreeMap::from([("region".to_owned(), "test".to_owned())]),
      work_root: directory("work"),
      state_root: directory("state"),
      octa_release_root: directory("octa"),
      source_plugins_dir: directory("sources"),
      enabled_runtime_modes: vec![RuntimeMode::Native],
      allow_native_execution: true,
      native_linux_cgroup_root: Some(directory("cgroup")),
      native_linux_bubblewrap_executable: Some({
        let path = temp.path().join("bwrap");
        File::create(&path).unwrap();
        path
      }),
      native_linux_readonly_paths: Vec::new(),
      native_linux_pids_limit: 4096,
      native_environment: BTreeMap::from([("PATH".to_owned(), "/usr/bin:/bin".to_owned())]),
      oci_engines: Vec::new(),
      allowed_upload_origins: vec!["https://objects.example".to_owned()],
      max_workspace_bytes: 1024,
      max_spool_bytes: 1024,
      poll_timeout_seconds: 30,
      coordinator_request_timeout_seconds: 10,
      coordinator_max_body_bytes: 4 * 1024 * 1024,
      retry_initial_delay_milliseconds: 100,
      retry_max_delay_seconds: 10,
      retry_max_attempts: 4,
      heartbeat_interval_seconds: 5,
      lease_safety_margin_seconds: 15,
      graceful_cancel_timeout_seconds: 5,
      cleanup_timeout_seconds: 10,
      runner_hello_timeout_seconds: 5,
      resource_sample_interval_seconds: 5,
      resource_sample_timeout_seconds: 1,
      max_accounting_failures: 3,
    };
    Self { _temp: temp, config }
  }
}

#[test]
fn validates_a_provisioned_agent() {
  let fixture = Fixture::new();
  let validated = fixture.config.validate().unwrap();
  assert_eq!(validated.signing_keys.len(), 1);
}

#[test]
fn rejects_native_execution_without_explicit_consent() {
  let mut fixture = Fixture::new();
  fixture.config.allow_native_execution = false;
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("allow_native_execution")
  );
}

#[test]
fn validates_backend_configuration_as_one_explicit_mode() {
  let mut fixture = Fixture::new();
  fixture.config.native_linux_cgroup_root = None;
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("native_linux_cgroup_root")
  );

  let mut fixture = Fixture::new();
  fixture.config.enabled_runtime_modes = vec![RuntimeMode::Native, RuntimeMode::Native];
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("duplicates")
  );

  let mut fixture = Fixture::new();
  fixture.config.enabled_runtime_modes = vec![RuntimeMode::Oci];
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("only valid")
  );
}

#[test]
fn requires_explicit_non_duplicate_oci_engines() {
  let mut fixture = Fixture::new();
  fixture.config.enabled_runtime_modes = vec![RuntimeMode::Oci];
  fixture.config.native_linux_cgroup_root = None;
  fixture.config.native_linux_bubblewrap_executable = None;
  fixture.config.native_linux_readonly_paths.clear();
  fixture.config.native_linux_pids_limit = 0;
  fixture.config.native_environment.clear();
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("oci_engine")
  );

  let mut fixture = Fixture::new();
  fixture.config.enabled_runtime_modes = vec![RuntimeMode::Oci];
  fixture.config.native_linux_cgroup_root = None;
  fixture.config.native_linux_bubblewrap_executable = None;
  fixture.config.native_linux_readonly_paths.clear();
  fixture.config.native_linux_pids_limit = 0;
  fixture.config.native_environment.clear();
  let executable = fixture._temp.path().join("msb");
  let libkrunfw = fixture._temp.path().join("libkrunfw");
  File::create(&executable).unwrap();
  File::create(&libkrunfw).unwrap();
  fixture.config.oci_engines = vec![OciEngineConfig::Microsandbox {
    executable,
    libkrunfw,
    metrics_sample_interval_seconds: 1,
  }];
  let mut invalid_interval = fixture.config.clone();
  let OciEngineConfig::Microsandbox {
    metrics_sample_interval_seconds,
    ..
  } = &mut invalid_interval.oci_engines[0]
  else {
    unreachable!();
  };
  *metrics_sample_interval_seconds = 0;
  assert!(
    invalid_interval
      .validate()
      .unwrap_err()
      .to_string()
      .contains("metrics_sample_interval_seconds")
  );
  assert!(fixture.config.validate().is_ok());

  let mut fixture = Fixture::new();
  fixture.config.enabled_runtime_modes = vec![RuntimeMode::Oci];
  fixture.config.native_linux_cgroup_root = None;
  fixture.config.native_linux_bubblewrap_executable = None;
  fixture.config.native_linux_readonly_paths.clear();
  fixture.config.native_linux_pids_limit = 0;
  fixture.config.native_environment.clear();
  fixture.config.oci_engines = vec![OciEngineConfig::Containerd {
    endpoint: PathBuf::from("/run/containerd/containerd.sock"),
    namespace: "octacity".to_owned(),
    snapshotter: "overlayfs".to_owned(),
    runtime: "io.containerd.runc.v2".to_owned(),
    registry_config_dir: None,
    pids_limit: 4096,
    open_files_limit: 65536,
  }];
  assert!(fixture.config.validate().is_ok());

  let mut fixture = Fixture::new();
  fixture.config.enabled_runtime_modes = vec![RuntimeMode::Oci];
  fixture.config.native_linux_cgroup_root = None;
  fixture.config.native_linux_bubblewrap_executable = None;
  fixture.config.native_linux_readonly_paths.clear();
  fixture.config.native_linux_pids_limit = 0;
  fixture.config.native_environment.clear();
  let executable = fixture._temp.path().join("msb");
  let libkrunfw = fixture._temp.path().join("libkrunfw");
  File::create(&executable).unwrap();
  File::create(&libkrunfw).unwrap();
  fixture.config.oci_engines = vec![
    OciEngineConfig::Microsandbox {
      executable: executable.clone(),
      libkrunfw: libkrunfw.clone(),
      metrics_sample_interval_seconds: 1,
    },
    OciEngineConfig::Microsandbox {
      executable,
      libkrunfw,
      metrics_sample_interval_seconds: 1,
    },
  ];
  assert!(fixture.config.validate().unwrap_err().to_string().contains("duplicate"));
}

#[test]
fn requires_a_complete_explicit_native_environment() {
  let mut fixture = Fixture::new();
  fixture.config.native_environment.clear();
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("must define a non-empty PATH")
  );

  let mut fixture = Fixture::new();
  fixture
    .config
    .native_environment
    .insert("INVALID=NAME".to_owned(), "value".to_owned());
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("invalid name")
  );

  let mut fixture = Fixture::new();
  fixture.config.enabled_runtime_modes = vec![RuntimeMode::Oci];
  fixture.config.native_linux_cgroup_root = None;
  fixture.config.native_linux_bubblewrap_executable = None;
  fixture.config.native_linux_readonly_paths.clear();
  fixture.config.native_linux_pids_limit = 0;
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("native_environment is only valid")
  );
}

#[test]
fn rejects_overlapping_roots() {
  let mut fixture = Fixture::new();
  fixture.config.state_root = fixture.config.work_root.join("state");
  fs::create_dir(&fixture.config.state_root).unwrap();
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("must not overlap")
  );
}

#[cfg(unix)]
#[test]
fn rejects_a_work_root_writable_by_other_users() {
  use std::os::unix::fs::PermissionsExt as _;

  let fixture = Fixture::new();
  fs::set_permissions(&fixture.config.work_root, fs::Permissions::from_mode(0o777)).unwrap();
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("must not be writable by group or other users")
  );
}

#[test]
fn rejects_non_origin_upload_urls() {
  let mut fixture = Fixture::new();
  fixture.config.allowed_upload_origins = vec!["https://objects.example/bucket".to_owned()];
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("only scheme")
  );
}

#[test]
fn validates_upload_origins_limits_and_lease_timing() {
  let mut fixture = Fixture::new();
  fixture.config.allowed_upload_origins.clear();
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("at least one")
  );

  let mut fixture = Fixture::new();
  fixture.config.allowed_upload_origins = vec!["https://objects.example".to_owned(); 2];
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("duplicates")
  );

  let mut fixture = Fixture::new();
  fixture.config.max_workspace_bytes = 0;
  assert!(fixture.config.validate().unwrap_err().to_string().contains("limits"));

  let mut fixture = Fixture::new();
  fixture.config.poll_timeout_seconds = 0;
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("poll_timeout_seconds")
  );

  let mut fixture = Fixture::new();
  fixture.config.coordinator_max_body_bytes = 0;
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("coordinator body")
  );

  let mut fixture = Fixture::new();
  fixture.config.retry_initial_delay_milliseconds = fixture.config.retry_max_delay_seconds * 1000 + 1;
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("retry_initial_delay_milliseconds")
  );

  let mut fixture = Fixture::new();
  fixture.config.heartbeat_interval_seconds = fixture.config.lease_safety_margin_seconds;
  assert!(fixture.config.validate().unwrap_err().to_string().contains("shorter"));
}

#[test]
fn rejects_invalid_identity_and_server_material() {
  let mut fixture = Fixture::new();
  fixture.config.labels.insert(String::new(), "value".to_owned());
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("label name")
  );

  let mut fixture = Fixture::new();
  fixture.config.server_signing_keys.clear();
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("at least one")
  );

  let mut fixture = Fixture::new();
  fixture
    .config
    .server_signing_keys
    .insert("primary".to_owned(), "not-base64".to_owned());
  assert!(fixture.config.validate().unwrap_err().to_string().contains("base64"));

  let mut fixture = Fixture::new();
  fixture.config.server_url = "http://octacity.example".to_owned();
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("must use https")
  );

  let mut fixture = Fixture::new();
  fixture.config.server_url = "https://octacity.example/api".to_owned();
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("only scheme")
  );
}

#[test]
fn rejects_unknown_configuration_fields() {
  assert!(toml::from_str::<AgentConfig>("agent_id = 'a'\nunknown = true").is_err());
}

#[test]
fn example_configuration_stays_parseable() {
  let config: AgentConfig = toml::from_str(include_str!("../../../docs/agent.example.toml")).unwrap();
  assert_eq!(config.agent_id, "linux-builder-01");
  assert!(decode_signing_key("primary", &config.server_signing_keys["primary"]).is_ok());
}
