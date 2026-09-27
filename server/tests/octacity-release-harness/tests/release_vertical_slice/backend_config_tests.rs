use super::*;

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
    &AgentConfigOverrides::default(),
  );

  let document: toml::Value = toml::from_str(&fs::read_to_string(config).unwrap()).unwrap();
  assert_eq!(document["cache"]["root"].as_str(), cache_root.to_str());
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
  for root in ["work", "state", "octa", "sources"] {
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
    state_root: directory.path().join("state"),
    endpoint: endpoint.clone(),
    namespace: "octacity-release".to_owned(),
    snapshotter: "overlayfs".to_owned(),
    runtime: "io.containerd.runc.v2".to_owned(),
    registry_config_dir: Some(registry.clone()),
    image: format!("example.invalid/octa@sha256:{}", "b".repeat(64)),
    workspace_bytes: 1024 * 1024,
  };
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
    &AgentConfigOverrides::default(),
  );

  let document: toml::Value = toml::from_str(&fs::read_to_string(config).unwrap()).unwrap();
  let engine = &document["oci_engines"][0];
  assert_eq!(engine["engine"].as_str(), Some("containerd"));
  assert_eq!(engine["endpoint"].as_str(), endpoint.to_str());
  assert_eq!(engine["namespace"].as_str(), Some("octacity-release"));
  assert_eq!(engine["registry_config_dir"].as_str(), registry.to_str());
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
