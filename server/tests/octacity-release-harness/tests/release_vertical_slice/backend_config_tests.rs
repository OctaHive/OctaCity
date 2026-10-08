use super::*;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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
      cache_read: true,
      cache_write: true,
      remote_cache_origin: Some("https://127.0.0.1:8443"),
      cache_ca_certificate: Some(&certificate),
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
      cache_read: false,
      cache_write: false,
      remote_cache_origin: None,
      cache_ca_certificate: None,
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
