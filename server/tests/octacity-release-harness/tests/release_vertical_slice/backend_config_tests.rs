use super::*;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[test]
fn installed_agent_configuration_reloads_authority_and_owned_paths_before_each_start() {
  let directory = tempfile::tempdir().unwrap();
  let native = directory.path().join("native.toml");
  fs::write(&native, "server_url='http://127.0.0.1:12345'\nagent_id='old-agent'\ncredential_file='/old/credential'\nwork_root='/old/work'\nstate_root='/old/state'\nallowed_network_hosts=['old.example']\nfactory_credential_profiles={old='/old/secret'}\n[cache]\nroot='/old/cache'\n").unwrap();
  let installed = directory.path().join("installed.toml");
  let mut permissions = serde_json::to_value(octacity_server_factory::FactoryPermissionSet::deny_all()).unwrap();
  permissions["resources"] =
    json!({"cpu_millis":1,"memory_bytes":1,"disk_bytes":1,"process_count":1,"elapsed_millis":1});
  let mut declaration = toml::Value::try_from(json!({
    "agent_id":"installed-agent","credential_file":"/installed/agent/credential",
    "work_root":"/installed/agent/work","state_root":"/installed/agent/state",
    "factory_permissions":permissions,"tool_executables":{},
    "factory_credential_profiles":{"mcp-read":"/installed/secret"},
    "allowed_network_hosts":["selected.example"],"cache":{"root":"/installed/agent/cache"}
  }))
  .unwrap();
  fs::write(&installed, toml::to_string(&declaration).unwrap()).unwrap();
  agent::apply_installed_agent_configuration(&native, &installed).unwrap();
  let first: toml::Value = toml::from_str(&fs::read_to_string(&native).unwrap()).unwrap();
  assert_eq!(first["server_url"].as_str(), Some("http://127.0.0.1:12345"));
  assert_eq!(first["agent_id"].as_str(), Some("installed-agent"));
  assert_eq!(first["credential_file"].as_str(), Some("/installed/agent/credential"));
  assert_eq!(first["state_root"].as_str(), Some("/installed/agent/state"));
  assert_eq!(first["work_root"].as_str(), Some("/installed/agent/work"));
  assert_eq!(first["cache"]["root"].as_str(), Some("/installed/agent/cache"));
  assert_eq!(
    first["factory_credential_profiles"]["mcp-read"].as_str(),
    Some("/installed/secret")
  );
  assert!(first["factory_credential_profiles"].get("old").is_none());
  declaration["allowed_network_hosts"] = toml::Value::Array(vec![toml::Value::String("updated.example".into())]);
  declaration["factory_credential_profiles"] = toml::Value::Table(toml::Table::new());
  fs::write(&installed, toml::to_string(&declaration).unwrap()).unwrap();
  agent::apply_installed_agent_configuration(&native, &installed).unwrap();
  let updated: toml::Value = toml::from_str(&fs::read_to_string(&native).unwrap()).unwrap();
  assert_eq!(updated["allowed_network_hosts"][0].as_str(), Some("updated.example"));
  assert!(updated["factory_credential_profiles"].as_table().unwrap().is_empty());
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
    environment_identity: "microsandbox-linux-guest-v1".to_owned(),
    image: format!("example.invalid/octa@sha256:{}", "a".repeat(64)),
    workspace_bytes: 1024 * 1024,
  };
  let cache = directory.path().join("agent-local/cache");
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
      factory: None,
      cache_read: true,
      cache_write: true,
      remote_cache_origin: Some("https://127.0.0.1:8443"),
      cache_ca_certificate: Some(&certificate),
      allowed_network_hosts: &[],
      unrestricted_network: true,
      upload_origins: &upload_origins,
      output_limits: Some(octacity_protocol::OutputLimits {
        artifact_count: 1,
        artifact_bytes: 4096,
        report_count: 1,
        report_bytes: 4096,
        single_output_bytes: 4096,
      }),
      tool_executables: &[],
    },
  );
  let contents = fs::read_to_string(config).unwrap();
  assert!(!contents.contains("OCTA_CODEX_EXECUTABLE"));
  assert!(!contents.contains("OPENAI_API_KEY"));
  let document: toml::Value = toml::from_str(&contents).unwrap();
  assert!(document["allowed_upload_origins"].is_array());
  assert_eq!(document["cache"]["root"].as_str(), cache.to_str());
  assert_eq!(document["cache"]["ca_certificate_file"].as_str(), certificate.to_str());
  assert_eq!(document["cache"]["allow_read"].as_bool(), Some(true));
  assert_eq!(fs::metadata(&cache).unwrap().permissions().mode() & 0o777, 0o700);
  assert_eq!(document["max_output_limits"]["artifact_bytes"].as_integer(), Some(4096));
  assert!(document["tool_executables"].as_table().unwrap().is_empty());
  assert!(document["oci_engines"].as_array().unwrap().is_empty());
  let provider = &document["virtualization_providers"][0];
  assert_eq!(provider["provider"].as_str(), Some("microsandbox"));
  assert_eq!(
    provider["environment_identity"].as_str(),
    Some("microsandbox-linux-guest-v1")
  );
}

#[test]
fn codex_tool_selection_is_explicit_and_absent_from_ordinary_agent_configuration() {
  let directory = tempfile::tempdir().unwrap();
  for root in ["work", "state", "cache", "octa", "sources", "cgroups"] {
    fs::create_dir(directory.path().join(root)).unwrap();
  }
  let executable = directory.path().join("codex");
  fs::write(&executable, "fixture").unwrap();
  #[cfg(unix)]
  fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
  let source_plugins = directory.path().join("sources");
  let octa_root = directory.path().join("octa");
  let backend = ReleaseBackend::Native {
    cgroup_root: directory.path().join("cgroups"),
    work_root: directory.path().join("work"),
    cache_root: directory.path().join("cache"),
    bubblewrap: PathBuf::from("/usr/bin/bwrap"),
    path: "/usr/bin:/bin".to_owned(),
    environment_identity: "test-native-environment-v1".to_owned(),
    workspace_bytes: 1024 * 1024,
  };
  let digest = "a".repeat(64);
  let tool = AgentToolExecutable {
    standalone: false,
    product: "codex-cli",
    path: &executable,
    version: "0.161.0",
    platform: "linux-x86_64",
    sha256: &digest,
  };
  let config = write_agent_config(
    directory.path(),
    "http://127.0.0.1:12345",
    "codex-config-shape",
    "credential",
    AgentReleasePaths {
      source_plugins: &source_plugins,
      octa_root: &octa_root,
    },
    &backend,
    &AgentConfigOverrides {
      factory: None,
      cache_read: false,
      cache_write: false,
      remote_cache_origin: None,
      cache_ca_certificate: None,
      allowed_network_hosts: &[],
      unrestricted_network: false,
      upload_origins: &[],
      output_limits: None,
      tool_executables: &[tool],
    },
  );

  let document: toml::Value = toml::from_str(&fs::read_to_string(config).unwrap()).unwrap();
  let configured = &document["tool_executables"]["codex-cli"];
  assert_eq!(configured["path"].as_str(), executable.to_str());
  assert_eq!(configured["version"].as_str(), Some("0.161.0"));
  assert_eq!(configured["platform"].as_str(), Some("linux-x86_64"));
}

#[test]
fn microsandbox_release_jobs_request_provider_neutral_virtualization() {
  let backend = ReleaseBackend::Microsandbox {
    work_root: PathBuf::from("/work"),
    state_root: PathBuf::from("/state"),
    executable: PathBuf::from("/opt/microsandbox/bin/msb"),
    libkrunfw: PathBuf::from("/opt/microsandbox/lib/libkrunfw.so"),
    environment_identity: "microsandbox-linux-guest-v1".to_owned(),
    image: format!("example.invalid/octa@sha256:{}", "a".repeat(64)),
    workspace_bytes: 1024 * 1024 * 1024,
  };

  let configuration = build_configuration("project", "repository", "pipeline", "pool", &backend);
  let runtime = &configuration["definition"]["runtime"];
  assert_eq!(runtime["class"], "virtualization");
  assert_eq!(
    runtime["host_platform"],
    json!({"os": std::env::consts::OS, "architecture": host_architecture()})
  );
  assert_eq!(
    runtime["required_guarantees"],
    json!([
      "filesystem_isolation",
      "process_isolation",
      "network_isolation",
      "resource_isolation",
      "hardware_virtualization"
    ])
  );
  assert_eq!(runtime["operating_system"], "linux");
  assert_eq!(runtime["architecture"], host_architecture());
  assert_eq!(
    backend.execution_target(),
    Some(json!({
      "mode": "virtualization",
      "host_platform": {"os": std::env::consts::OS, "architecture": host_architecture()},
      "target_platform": {"os": "linux", "architecture": host_architecture()},
      "required_guarantees": [
        "filesystem_isolation",
        "process_isolation",
        "network_isolation",
        "resource_isolation",
        "hardware_virtualization"
      ]
    }))
  );
  assert!(runtime.get("backend").is_none());
}

#[test]
fn generated_native_agent_configuration_uses_the_bounded_cache_filesystem() {
  let directory = tempfile::tempdir().unwrap();
  let cache_root = directory.path().join("cache-mount");
  for root in ["work", "octa", "sources", "cgroups"] {
    fs::create_dir(directory.path().join(root)).unwrap();
  }
  fs::create_dir(&cache_root).unwrap();
  let source_plugins = directory.path().join("sources");
  let octa_root = directory.path().join("octa");
  let backend = ReleaseBackend::Native {
    cgroup_root: directory.path().join("cgroups"),
    work_root: directory.path().join("work"),
    cache_root: cache_root.clone(),
    bubblewrap: PathBuf::from("/usr/bin/bwrap"),
    path: "/usr/bin:/bin".to_owned(),
    environment_identity: "test-native-environment-v1".to_owned(),
    workspace_bytes: 1024 * 1024,
  };
  let upload_origins = ["https://objects.example"];
  let config = write_agent_config(
    directory.path(),
    "http://127.0.0.1:12345",
    "native-config-shape",
    "credential",
    AgentReleasePaths {
      source_plugins: &source_plugins,
      octa_root: &octa_root,
    },
    &backend,
    &AgentConfigOverrides::restricted_without_outputs(&upload_origins),
  );

  let document: toml::Value = toml::from_str(&fs::read_to_string(config).unwrap()).unwrap();
  assert_eq!(document["cache"]["root"].as_str(), cache_root.to_str());
  assert_eq!(document["cache"]["allow_read"].as_bool(), Some(true));
  assert_eq!(document["cache"]["allow_write"].as_bool(), Some(false));
  assert_eq!(
    document["cache"]["request_timeout_seconds"].as_integer(),
    Some(RELEASE_CACHE_REQUEST_TIMEOUT_SECONDS as i64)
  );
  assert_eq!(
    document["cache"]["native_environment_identities"]
      .as_table()
      .unwrap()
      .len(),
    1,
    "cache-enabled Native release jobs require one host environment identity"
  );
  let platform = format!("linux-{}", host_architecture());
  assert_eq!(
    document["cache"]["native_environment_identities"][&platform].as_str(),
    Some("test-native-environment-v1")
  );
}

#[test]
fn generated_containerd_agent_configuration_keeps_runtime_authority_operator_owned() {
  let directory = tempfile::tempdir().unwrap();
  for root in ["work", "cache", "state", "octa", "sources"] {
    fs::create_dir(directory.path().join(root)).unwrap();
  }
  let endpoint = directory.path().join("containerd.sock");
  fs::write(&endpoint, "fixture").unwrap();
  let registry = directory.path().join("registry");
  fs::create_dir(&registry).unwrap();
  let source_plugins = directory.path().join("sources");
  let octa_root = directory.path().join("octa");
  let backend = ReleaseBackend::Containerd {
    work_root: directory.path().join("work"),
    cache_root: directory.path().join("cache"),
    state_root: directory.path().join("state"),
    endpoint: endpoint.clone(),
    namespace: "octacity-release".to_owned(),
    snapshotter: "overlayfs".to_owned(),
    runtime: "io.containerd.runc.v2".to_owned(),
    registry_config_dir: Some(registry.clone()),
    environment_identity: "containerd-linux-arm64-v1".to_owned(),
    image: format!("example.invalid/octa@sha256:{}", "b".repeat(64)),
    workspace_bytes: 1024 * 1024,
  };
  let upload_origins = ["https://objects.example"];
  let config = write_agent_config(
    directory.path(),
    "http://127.0.0.1:12345",
    "containerd-config-shape",
    "credential",
    AgentReleasePaths {
      source_plugins: &source_plugins,
      octa_root: &octa_root,
    },
    &backend,
    &AgentConfigOverrides::restricted_without_outputs(&upload_origins),
  );

  let document: toml::Value = toml::from_str(&fs::read_to_string(config).unwrap()).unwrap();
  assert_eq!(document["cache"]["allow_read"].as_bool(), Some(true));
  assert_eq!(document["cache"]["allow_write"].as_bool(), Some(false));
  assert!(document["oci_engines"].as_array().unwrap().is_empty());
  let provider = &document["isolation_providers"][0];
  assert_eq!(provider["provider"].as_str(), Some("containerd"));
  assert_eq!(
    provider["environment_identity"].as_str(),
    Some("containerd-linux-arm64-v1")
  );
  assert_eq!(provider["endpoint"].as_str(), endpoint.to_str());
  assert_eq!(provider["namespace"].as_str(), Some("octacity-release"));
  assert_eq!(provider["registry_config_dir"].as_str(), registry.to_str());
  assert!(document.get("containerd_credentials").is_none());
}

#[test]
fn release_jobs_use_the_bounded_workspace_limit() {
  let backend = ReleaseBackend::Native {
    cgroup_root: PathBuf::from("/cgroups"),
    work_root: PathBuf::from("/work"),
    cache_root: PathBuf::from("/cache"),
    bubblewrap: PathBuf::from("/usr/bin/bwrap"),
    path: "/usr/bin:/bin".to_owned(),
    environment_identity: "test-native-environment-v1".to_owned(),
    workspace_bytes: 1024 * 1024 * 1024,
  };

  let configuration = build_configuration("project", "repository", "pipeline", "pool", &backend);
  let definition = &configuration["definition"];
  assert_eq!(
    definition["agent_requirements"]["minimum_disk_bytes"].as_u64(),
    Some(backend.workspace_bytes())
  );
  assert_eq!(
    definition["runtime"]["writable_disk_bytes"].as_u64(),
    Some(backend.workspace_bytes())
  );
  assert_eq!(
    definition["runtime"]["timeout_seconds"].as_u64(),
    Some(RELEASE_JOB_TIMEOUT_SECONDS)
  );
}

#[test]
fn containerd_release_jobs_request_provider_neutral_isolation() {
  let backend = ReleaseBackend::Containerd {
    work_root: PathBuf::from("/work"),
    cache_root: PathBuf::from("/cache"),
    state_root: PathBuf::from("/state"),
    endpoint: PathBuf::from("/run/containerd/containerd.sock"),
    namespace: "octacity-release".to_owned(),
    snapshotter: "overlayfs".to_owned(),
    runtime: "io.containerd.runc.v2".to_owned(),
    registry_config_dir: None,
    environment_identity: "containerd-linux-amd64-v1".to_owned(),
    image: format!("example.invalid/octa@sha256:{}", "b".repeat(64)),
    workspace_bytes: 1024 * 1024 * 1024,
  };

  let configuration = build_configuration("project", "repository", "pipeline", "pool", &backend);
  let runtime = &configuration["definition"]["runtime"];
  assert_eq!(runtime["class"], "isolation");
  assert_eq!(
    runtime["host_platform"],
    json!({"os": "linux", "architecture": host_architecture()})
  );
  assert_eq!(runtime["required_guarantees"].as_array().unwrap().len(), 4);
  assert_eq!(
    backend.execution_target(),
    Some(json!({
      "mode": "isolation",
      "host_platform": {"os": "linux", "architecture": host_architecture()},
      "target_platform": {"os": "linux", "architecture": host_architecture()},
      "required_guarantees": [
        "filesystem_isolation",
        "process_isolation",
        "network_isolation",
        "resource_isolation"
      ]
    }))
  );
  let capabilities = configuration["definition"]["agent_requirements"]["capabilities"]
    .as_array()
    .unwrap();
  assert_eq!(capabilities, &[json!("shell")]);
}

#[test]
fn apple_vf_release_jobs_keep_virtualization_as_an_isolation_implementation_detail() {
  let backend = ReleaseBackend::AppleVf {
    work_root: PathBuf::from("/work"),
    cache_root: PathBuf::from("/cache"),
    state_root: PathBuf::from("/state"),
    executable: PathBuf::from("/usr/local/bin/container"),
    environment_identity: "apple-vf-macos-arm64-v1".to_owned(),
    image: format!("example.invalid/octa@sha256:{}", "c".repeat(64)),
    workspace_bytes: 1024 * 1024 * 1024,
  };

  let configuration = build_configuration("project", "repository", "pipeline", "pool", &backend);
  let runtime = &configuration["definition"]["runtime"];
  assert_eq!(runtime["class"], "isolation");
  assert_eq!(
    runtime["host_platform"],
    json!({"os": "macos", "architecture": "arm64"})
  );
  assert_eq!(runtime["operating_system"], "linux");
  assert_eq!(runtime["required_guarantees"].as_array().unwrap().len(), 4);
  let execution_target = json!({
    "mode": "isolation",
    "host_platform": {"os": "macos", "architecture": "arm64"},
    "target_platform": {"os": "linux", "architecture": "arm64"},
    "required_guarantees": [
      "filesystem_isolation",
      "process_isolation",
      "network_isolation",
      "resource_isolation"
    ]
  });
  assert_eq!(backend.execution_target(), Some(execution_target.clone()));
  assert_eq!(
    backend.pool_admission_policy(),
    json!({
      "mode": "execution_allowlist",
      "platforms": [{"operating_system": "macos", "architecture": "arm64"}],
      "execution_targets": [execution_target.clone()]
    })
  );
  let policy = super::support::project_policy_body(
    "pool",
    "repository",
    backend.runtime_class(),
    Some(execution_target.clone()),
  );
  assert_eq!(
    policy["policy"]["execution_targets"],
    json!({"mode": "replace", "value": [execution_target]})
  );
  let capabilities = configuration["definition"]["agent_requirements"]["capabilities"]
    .as_array()
    .unwrap();
  assert_eq!(capabilities, &[json!("shell")]);
}

#[test]
fn generated_apple_vf_agent_configuration_keeps_provider_authority_operator_owned() {
  let directory = tempfile::tempdir().unwrap();
  for root in ["work", "cache", "state", "octa", "sources"] {
    fs::create_dir(directory.path().join(root)).unwrap();
  }
  let executable = directory.path().join("container");
  fs::write(&executable, "fixture").unwrap();
  let source_plugins = directory.path().join("sources");
  let octa_root = directory.path().join("octa");
  let backend = ReleaseBackend::AppleVf {
    work_root: directory.path().join("work"),
    cache_root: directory.path().join("cache"),
    state_root: directory.path().join("state"),
    executable: executable.clone(),
    environment_identity: "apple-vf-macos-arm64-v1".to_owned(),
    image: format!("example.invalid/octa@sha256:{}", "c".repeat(64)),
    workspace_bytes: 1024 * 1024,
  };
  let upload_origins = ["https://objects.example"];
  let config = write_agent_config(
    directory.path(),
    "http://127.0.0.1:12345",
    "apple-vf-config-shape",
    "credential",
    AgentReleasePaths {
      source_plugins: &source_plugins,
      octa_root: &octa_root,
    },
    &backend,
    &AgentConfigOverrides::restricted_without_outputs(&upload_origins),
  );

  let document: toml::Value = toml::from_str(&fs::read_to_string(config).unwrap()).unwrap();
  assert!(document["oci_engines"].as_array().unwrap().is_empty());
  let provider = &document["isolation_providers"][0];
  assert_eq!(provider["provider"].as_str(), Some("apple_vf"));
  assert_eq!(
    provider["environment_identity"].as_str(),
    Some("apple-vf-macos-arm64-v1")
  );
  assert_eq!(provider["executable"].as_str(), executable.to_str());
}
#[test]
fn factory_agent_configuration_preserves_role_refs_and_exact_capture_without_secret_values() {
  use std::collections::BTreeMap;
  let directory = tempfile::tempdir().unwrap();
  for name in ["work", "state", "octa", "sources"] {
    fs::create_dir(directory.path().join(name)).unwrap();
  }
  let backend = ReleaseBackend::Microsandbox {
    work_root: directory.path().join("work"),
    state_root: directory.path().join("state"),
    executable: directory.path().join("msb"),
    libkrunfw: directory.path().join("libkrunfw"),
    environment_identity: "installed-sandbox-v7".into(),
    image: format!("example.invalid/image@sha256:{}", "a".repeat(64)),
    workspace_bytes: 1024 * 1024,
  };
  let profiles = BTreeMap::from([
    ("author-role".to_owned(), directory.path().join("author-private.json")),
    ("review-role".to_owned(), directory.path().join("review-private.json")),
  ]);
  for path in profiles.values() {
    fs::write(path, b"not-part-of-public-config-canary").unwrap();
  }
  let permissions: octacity_protocol::FactoryPermissionSetV3 = serde_json::from_value(json!({
    "plugins":[],"executables":[],"tools":[],"commands":[],"max_descendants":0,"mounts":[],"network_hosts":[],"secret_profiles":["author-role","review-role"],"workload_identity_profiles":[],
    "resources":{"cpu_millis":1,"memory_bytes":1,"disk_bytes":1,"process_count":1,"elapsed_millis":1},
    "outputs":{"kinds":[],"max_artifact_count":0,"max_artifact_bytes":0,"max_report_count":0,"max_report_bytes":0}
  })).unwrap();
  let capture_path = directory.path().join("git");
  let capture_sha = "c".repeat(64);
  let capture = AgentToolExecutable {
    standalone: false,
    product: "git",
    path: &capture_path,
    version: "2.56.0",
    platform: "macos-aarch64",
    sha256: &capture_sha,
  };
  let config = write_agent_config(
    directory.path(),
    "http://127.0.0.1:12345",
    "factory-config",
    "enrollment",
    AgentReleasePaths {
      octa_root: &directory.path().join("octa"),
      source_plugins: &directory.path().join("sources"),
    },
    &backend,
    &AgentConfigOverrides {
      factory: Some(agent::AgentFactoryOverrides {
        permissions: &permissions,
        credential_profiles: &profiles,
        capture: Some(&capture),
        protected_reader: None,
      }),
      ..AgentConfigOverrides::restricted_without_outputs(&[])
    },
  );
  let contents = fs::read_to_string(config).unwrap();
  let document: toml::Value = toml::from_str(&contents).unwrap();
  assert_eq!(
    document["factory_credential_profiles"]["author-role"].as_str(),
    profiles["author-role"].to_str()
  );
  assert_eq!(
    document["factory_credential_profiles"]["review-role"].as_str(),
    profiles["review-role"].to_str()
  );
  assert_ne!(
    document["factory_credential_profiles"]["author-role"],
    document["factory_credential_profiles"]["review-role"]
  );
  let decoded: octacity_protocol::FactoryPermissionSetV3 = document["factory_permissions"].clone().try_into().unwrap();
  assert_eq!(
    serde_json::to_value(decoded).unwrap(),
    serde_json::to_value(permissions).unwrap()
  );
  assert_eq!(document["change_set_capture"]["provider"].as_str(), Some("git"));
  assert_eq!(
    document["change_set_capture"]["executable"]["path"].as_str(),
    capture_path.to_str()
  );
  assert_eq!(
    document["change_set_capture"]["executable"]["sha256"].as_str(),
    Some(capture_sha.as_str())
  );
  assert!(!contents.contains("not-part-of-public-config-canary"));
  assert_eq!(
    document["virtualization_providers"][0]["environment_identity"].as_str(),
    Some("installed-sandbox-v7")
  );
}

#[test]
#[ignore = "requires the staged current Agent, pinned Octa/MSB and installed Git for the Factory pilot"]
fn factory_agent_configuration_validates_against_the_real_installed_process() {
  use sha2::Digest as _;
  use std::collections::BTreeMap;
  let runtime = octacity_release_harness::validate_installed_agent_runtime(
    &octacity_release_harness::AgentRuntimeBundles {
      agent: required_path("OCTACITY_RELEASE_AGENT_ROOT", true),
      octa: required_path("OCTACITY_CONTRACT_OCTA_RELEASE_ROOT", true),
    },
    &octacity_release_harness::AgentRuntimePlatforms::new(
      format!("{}-{}", std::env::consts::OS, host_architecture()),
      format!("linux-{}", std::env::consts::ARCH),
    ),
  )
  .expect("the exact staged Agent and pinned Octa must validate before configuration");
  let directory = tempfile::tempdir().unwrap();
  set_mode(directory.path(), 0o700);
  let permissions: octacity_protocol::FactoryPermissionSetV3 = serde_json::from_value(json!({
    "plugins":[],"executables":[],"tools":[],"commands":[],"max_descendants":0,"mounts":[],"network_hosts":[],"secret_profiles":["author-role","review-role"],"workload_identity_profiles":[],
    "resources":{"cpu_millis":1000,"memory_bytes":536870912,"disk_bytes":67108864,"process_count":64,"elapsed_millis":120000},
    "outputs":{"kinds":[],"max_artifact_count":0,"max_artifact_bytes":0,"max_report_count":0,"max_report_bytes":0}
  })).unwrap();
  let profiles = BTreeMap::from([
    ("author-role".to_owned(), directory.path().join("author-private.json")),
    ("review-role".to_owned(), directory.path().join("review-private.json")),
  ]);
  for (name, path) in &profiles {
    fs::write(
      path,
      serde_json::to_vec(&json!({"OCTACITY_CODEX_AUTH":format!("private-{name}-canary")})).unwrap(),
    )
    .unwrap();
    set_mode(path, 0o600);
  }
  let git = required_path("OCTACITY_CONTRACT_GIT_EXECUTABLE", true)
    .canonicalize()
    .unwrap();
  let version = std::process::Command::new(&git).arg("--version").output().unwrap();
  assert!(
    version.status.success(),
    "the configured capture binary must identify itself"
  );
  let version = String::from_utf8(version.stdout).unwrap();
  let sha = hex::encode(sha2::Sha256::digest(fs::read(&git).unwrap()));
  let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
  let capture = AgentToolExecutable {
    standalone: false,
    product: "git",
    path: &git,
    version: version.split_whitespace().nth(2).unwrap(),
    platform: &platform,
    sha256: &sha,
  };
  let config = write_agent_config(
    directory.path(),
    "http://127.0.0.1:12345",
    "factory-validation",
    "fixture-enrollment",
    AgentReleasePaths {
      octa_root: &runtime.octa_root,
      source_plugins: &runtime.source_plugins,
    },
    &ReleaseBackend::from_environment(),
    &AgentConfigOverrides {
      factory: Some(agent::AgentFactoryOverrides {
        permissions: &permissions,
        credential_profiles: &profiles,
        capture: Some(&capture),
        protected_reader: None,
      }),
      ..AgentConfigOverrides::restricted_without_outputs(&["http://127.0.0.1:9000"])
    },
  );
  let output = std::process::Command::new(&runtime.agent_binary)
    .arg("validate")
    .arg(config)
    .output()
    .unwrap();
  assert!(
    output.status.success(),
    "the real staged Agent must accept the generated exact Factory configuration"
  );
  let diagnostic = [output.stdout, output.stderr].concat();
  for name in profiles.keys() {
    assert!(
      !diagnostic
        .windows(format!("private-{name}-canary").len())
        .any(|part| part == format!("private-{name}-canary").as_bytes()),
      "configuration diagnostics must not expose role credentials"
    );
  }
}
