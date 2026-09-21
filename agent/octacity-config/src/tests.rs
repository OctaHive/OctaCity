//! Agent configuration parsing and invariant tests.

use std::{fs::File, io::Write as _, path::Path};

use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

#[cfg(windows)]
const WINDOWS_FIXTURE_CREATE_ATTEMPTS: usize = 16;

struct Fixture {
  _temp: FixtureRoot,
  config: AgentConfig,
}

impl Fixture {
  fn new() -> Self {
    let temp = fixture_tempdir();
    let credential_root = temp.path().join("credentials");
    octacity_private_fs::create_private_directory(&credential_root).unwrap();
    let credential = credential_root.join("credential");
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
      workload_identity_profiles: BTreeMap::new(),
      cache: CacheConfig {
        root: directory("cache"),
        capacity: octa_cache_protocol::LocalCacheCapacity::new(1024 * 1024, 900 * 1024, 800 * 1024).unwrap(),
        max_scopes: 16,
        allow_read: true,
        allow_write: true,
        allowed_remote_origins: vec!["https://cache.example".to_owned()],
        ca_certificate_file: None,
        native_environment_identities: BTreeMap::from([(
          "linux-amd64".to_owned(),
          "rust-1.98-toolchain-v1".to_owned(),
        )]),
        request_timeout_seconds: 30,
        max_parallel_transfers: 4,
      },
      maintenance: MaintenanceConfig {
        work_reserve_bytes: 1024,
        state_reserve_bytes: 1024,
        cache_reserve_bytes: 1024,
        disk_check_interval_seconds: 1,
      },
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
      allow_unrestricted_network: false,
      allowed_network_hosts: vec!["vault.example.com".to_owned()],
      allowed_upload_origins: vec!["https://objects.example".to_owned()],
      max_archive_entries: 10_000,
      upload_timeout_seconds: 30,
      max_output_limits: OutputLimits {
        artifact_count: 10,
        artifact_bytes: 1024,
        report_count: 10,
        report_bytes: 1024,
        single_output_bytes: 1024,
      },
      max_workspace_bytes: 1024,
      max_spool_bytes: 1024,
      max_spool_records: 32,
      event_batch_max_bytes: 512,
      event_batch_max_records: 16,
      event_channel_capacity: 8,
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

struct FixtureRoot {
  path: PathBuf,
  #[cfg(not(windows))]
  _temporary: tempfile::TempDir,
}

impl FixtureRoot {
  fn path(&self) -> &Path {
    &self.path
  }
}

fn fixture_tempdir() -> FixtureRoot {
  #[cfg(windows)]
  {
    // The GitHub runner profile itself grants an object-applicable delete ACE
    // to another local principal. A real installation must start below a safe
    // operator-owned ancestor, so model that by placing the protected fixture
    // directly below the profile's volume root.
    let profile = std::env::var_os("USERPROFILE").expect("Windows tests require USERPROFILE");
    let profile = fs::canonicalize(profile).expect("Windows tests require a canonical USERPROFILE");
    let volume_root = profile
      .ancestors()
      .last()
      .expect("Windows USERPROFILE must have a volume root");
    for _ in 0..WINDOWS_FIXTURE_CREATE_ATTEMPTS {
      let path = volume_root.join(format!(".octacity-test-{}", uuid::Uuid::new_v4().simple()));
      match octacity_private_fs::create_private_directory(&path) {
        Ok(()) => return FixtureRoot { path },
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => panic!("failed to create protected Windows fixture root: {error}"),
      }
    }
    panic!("failed to allocate a unique protected Windows fixture root")
  }
  #[cfg(not(windows))]
  {
    let temporary = tempfile::tempdir().unwrap();
    FixtureRoot {
      path: temporary.path().to_owned(),
      _temporary: temporary,
    }
  }
}

#[cfg(windows)]
impl Drop for FixtureRoot {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.path);
  }
}

#[test]
fn validates_a_provisioned_agent() {
  let fixture = Fixture::new();
  let validated = fixture.config.validate().unwrap();
  assert_eq!(validated.signing_keys.len(), 1);
}

#[test]
fn permits_an_inventory_only_agent_without_execution_backends() {
  let mut fixture = Fixture::new();
  fixture.config.enabled_runtime_modes.clear();
  fixture.config.allow_native_execution = false;
  fixture.config.native_linux_cgroup_root = None;
  fixture.config.native_linux_bubblewrap_executable = None;
  fixture.config.native_linux_readonly_paths.clear();
  fixture.config.native_linux_pids_limit = 0;
  fixture.config.native_environment.clear();

  let validated = fixture.config.validate().unwrap();
  assert!(validated.runtimes.is_empty());
}

#[test]
fn validates_disk_pressure_reserves_without_overflow() {
  let mut fixture = Fixture::new();
  fixture.config.maintenance.disk_check_interval_seconds = 0;
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("disk_check_interval")
  );

  let mut fixture = Fixture::new();
  fixture.config.maintenance.work_reserve_bytes = u64::MAX;
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("work reserve")
  );

  let mut fixture = Fixture::new();
  fixture.config.maintenance.state_reserve_bytes = u64::MAX;
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("state reserve")
  );

  let mut fixture = Fixture::new();
  fixture.config.max_output_limits.artifact_bytes = u64::MAX;
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("output limits")
  );

  let mut fixture = Fixture::new();
  fixture.config.maintenance.cache_reserve_bytes = u64::MAX;
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("cache reserve")
  );
}

#[test]
fn validates_local_network_and_output_policy() {
  let mut fixture = Fixture::new();
  assert!(fixture.config.clone().validate().is_ok());

  fixture
    .config
    .allowed_network_hosts
    .push("vault.example.com".to_owned());
  assert!(fixture.config.clone().validate().is_err());
  fixture.config.allowed_network_hosts.pop();
  fixture.config.allowed_network_hosts.push(" bad.example.com".to_owned());
  assert!(fixture.config.clone().validate().is_err());
  fixture.config.allowed_network_hosts.pop();

  fixture.config.max_output_limits.single_output_bytes = 2048;
  assert!(fixture.config.validate().is_err());
}

#[test]
fn validates_restricted_workload_identity_profiles() {
  let mut fixture = Fixture::new();
  let identity_root = fixture._temp.path().join("identities");
  octacity_private_fs::create_private_directory(&identity_root).unwrap();
  let identity = identity_root.join("identity");
  File::create(&identity).unwrap().write_all(b"signed-jwt").unwrap();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(&identity, fs::Permissions::from_mode(0o600)).unwrap();
  }
  fixture
    .config
    .workload_identity_profiles
    .insert("ci".to_owned(), identity.clone());
  let validated = fixture.config.validate().unwrap();
  assert_eq!(
    validated.config.workload_identity_profiles["ci"],
    identity.canonicalize().unwrap()
  );

  let mut fixture = Fixture::new();
  fixture
    .config
    .workload_identity_profiles
    .insert("ci\ninvalid".to_owned(), identity);
  assert!(fixture.config.validate().unwrap_err().to_string().contains("control"));

  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;

    let mut fixture = Fixture::new();
    let identity_directory = fixture._temp.path().join("replaceable-identity");
    fs::create_dir(&identity_directory).unwrap();
    fs::set_permissions(&identity_directory, fs::Permissions::from_mode(0o777)).unwrap();
    let identity = identity_directory.join("token");
    File::create(&identity).unwrap().write_all(b"signed-jwt").unwrap();
    fs::set_permissions(&identity, fs::Permissions::from_mode(0o600)).unwrap();
    fixture
      .config
      .workload_identity_profiles
      .insert("ci".to_owned(), identity);
    assert!(
      fixture
        .config
        .validate()
        .unwrap_err()
        .to_string()
        .contains("must not be writable")
    );
  }
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
    endpoint: fixture._temp.path().join("containerd.sock"),
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
  fixture.config.allowed_upload_origins = vec![
    "https://OBJECTS.example:443".to_owned(),
    "https://objects.example".to_owned(),
  ];
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
  fixture.config.coordinator_max_body_bytes =
    fixture.config.event_batch_max_bytes + octacity_protocol::MAX_APPEND_REQUEST_OVERHEAD_BYTES - 1;
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("protocol metadata")
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

  for server_url in ["http://localhost:8080", "http://127.0.0.1:8080", "http://[::1]:8080"] {
    let mut fixture = Fixture::new();
    fixture.config.server_url = server_url.to_owned();
    fixture.config.validate().unwrap();
  }

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
fn validates_cache_capacity_origins_and_native_runtime_identity() {
  let mut fixture = Fixture::new();
  fixture.config.cache.capacity.low_watermark_bytes = fixture.config.cache.capacity.high_watermark_bytes + 1;
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("cache capacity")
  );

  let mut fixture = Fixture::new();
  fixture.config.cache.max_scopes = 0;
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("max_scopes")
  );

  let mut fixture = Fixture::new();
  fixture.config.cache.allowed_remote_origins = vec!["https://cache.example/path".to_owned()];
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("only scheme")
  );

  let mut fixture = Fixture::new();
  fixture.config.cache.allowed_remote_origins = vec!["http://localhost".to_owned()];
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("must use https")
  );

  let mut fixture = Fixture::new();
  fixture
    .config
    .cache
    .native_environment_identities
    .insert("linux-amd64".to_owned(), String::new());
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("must contain")
  );

  let mut fixture = Fixture::new();
  fixture.config.cache.root = fixture.config.state_root.clone();
  assert!(
    fixture
      .config
      .validate()
      .unwrap_err()
      .to_string()
      .contains("must not overlap")
  );
}

#[test]
fn protects_the_remote_cache_ca_as_trust_material() {
  let mut fixture = Fixture::new();
  let certificate = fixture.config.state_root.join("cache-ca.pem");
  File::create(&certificate).unwrap().write_all(b"public-ca").unwrap();
  fixture.config.cache.ca_certificate_file = Some(certificate.clone());

  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(&certificate, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(
      fixture
        .config
        .clone()
        .validate()
        .unwrap_err()
        .to_string()
        .contains("cache.ca_certificate_file")
    );
    fs::set_permissions(&certificate, fs::Permissions::from_mode(0o600)).unwrap();
  }

  assert!(fixture.config.validate().is_ok());
}

#[test]
fn rejects_unknown_configuration_fields() {
  assert!(toml::from_str::<AgentConfig>("agent_id = 'a'\nunknown = true").is_err());
}

#[test]
fn example_configuration_stays_parseable() {
  let examples = [
    include_str!("../../../docs/agent.example.toml"),
    include_str!("../../../docs/agent.linux-arm64.example.toml"),
    include_str!("../../../docs/agent.macos.example.toml"),
    include_str!("../../../docs/agent.windows.example.toml"),
  ];
  let configs = examples
    .into_iter()
    .map(|example| toml::from_str::<AgentConfig>(example).unwrap())
    .collect::<Vec<_>>();
  assert_eq!(configs[0].agent_id, "linux-builder-01");
  assert_eq!(configs[1].agent_id, "linux-arm64-builder-01");
  assert_eq!(configs[2].agent_id, "macos-builder-01");
  assert_eq!(configs[3].agent_id, "windows-builder-01");
  for config in configs {
    assert!(decode_signing_key("primary", &config.server_signing_keys["primary"]).is_ok());
  }
}
