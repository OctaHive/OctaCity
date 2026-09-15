//! Host monitoring tests that avoid asserting machine-specific quantities.

use octacity_protocol::{BackendHealthStatus, OciIsolation, RuntimeMode};
use octacity_runner::{RunnerCapabilities, RunnerInstallation, RunnerPlugin};

use super::*;

#[test]
fn samples_capacity_and_bounded_availability() {
  let temporary = tempfile::tempdir().unwrap();
  let root = temporary.path().canonicalize().unwrap();
  let mut monitor = HostMonitor::new(root.clone(), root, true).unwrap();
  let snapshot = monitor
    .snapshot(
      None,
      vec![BackendHealth {
        backend: "microsandbox".to_owned(),
        status: BackendHealthStatus::Ready,
        message: None,
      }],
    )
    .unwrap();

  assert!(monitor.capacity().logical_cpu_count > 0);
  assert!(snapshot.available_memory_bytes <= monitor.capacity().total_memory_bytes);
  assert!(snapshot.work_disk_free_bytes <= monitor.capacity().work_disk_total_bytes);
}

#[test]
fn recognizes_the_current_supported_host_platform() {
  let platform = host_platform().unwrap();
  assert!(matches!(
    platform.os,
    PlatformOs::Linux | PlatformOs::Windows | PlatformOs::Macos
  ));
  assert!(matches!(
    platform.architecture,
    PlatformArchitecture::Amd64 | PlatformArchitecture::Arm64
  ));
}

#[test]
fn matches_canonical_roots_to_their_filesystem_mount_form() {
  let temporary = tempfile::tempdir().unwrap();
  let canonical = temporary.path().canonicalize().unwrap();

  assert!(super::mount_depth(&canonical, temporary.path()).is_some());
}

#[test]
fn generated_runtime_capabilities_obey_the_wire_contract() {
  let capability = RuntimeCapability {
    backend: "containerd".to_owned(),
    mode: RuntimeMode::Oci,
    platform: host_platform().unwrap(),
    isolation: Some(OciIsolation::Process),
  };
  capability.validate().unwrap();
}

#[test]
fn builds_registration_data_from_verified_component_inventories() {
  let temporary = tempfile::tempdir().unwrap();
  let sources = SourcePluginRegistry::discover(temporary.path()).unwrap();
  let mut runner = RunnerInstallation {
    root: temporary.path().to_owned(),
    executable: temporary.path().join("octa-runner"),
    plugins_dir: temporary.path().join("plugins"),
    default_plugin_lock: temporary.path().join("Octa.lock"),
    sha256: "1".repeat(64),
    capabilities: RunnerCapabilities {
      octa_version: "0.3.0".to_owned(),
      runner_protocols: vec![1],
      event_schemas: vec![1],
      plugin_protocols: vec![1],
      octafile_versions: vec![1],
      platform: "linux-x86_64".to_owned(),
      features: vec!["reports".to_owned()],
      build_commit: Some("abc123".to_owned()),
    },
    plugins: BTreeMap::from([(
      "shell".to_owned(),
      RunnerPlugin {
        version: "0.3.0".to_owned(),
        protocol: 1,
        platforms: vec!["linux-x86_64".to_owned()],
        executable: temporary.path().join("plugins/shell"),
        sha256: "2".repeat(64),
        capabilities: vec!["shell".to_owned()],
      },
    )]),
  };
  let runtime = RuntimeCapability {
    backend: "containerd".to_owned(),
    mode: RuntimeMode::Oci,
    platform: host_platform().unwrap(),
    isolation: Some(OciIsolation::Process),
  };
  let root = temporary.path().canonicalize().unwrap();
  let monitor = HostMonitor::new(root.clone(), root, false).unwrap();

  let inventory = build_inventory(
    AgentInventoryConfig {
      agent_id: "agent-1".to_owned(),
      agent_version: "agent-release".to_owned(),
      labels: BTreeMap::new(),
      remote_cache_configured: false,
    },
    vec![runtime.clone()],
    &runner,
    &sources,
    monitor.capacity().clone(),
  )
  .unwrap();

  assert_eq!(inventory.agent_version, "agent-release");
  assert_eq!(inventory.octa.plugins[0].name, "shell");
  assert!(inventory.source_plugins.is_empty());
  assert!(inventory.cache.is_none());

  runner.capabilities.runner_protocols = vec![octa_runner_protocol::RUNNER_PROTOCOL_VERSION];
  runner.capabilities.features = vec![CACHE_FEATURE_V1.to_owned(), CACHE_HTTP_FEATURE_V1.to_owned()];
  let inventory = build_inventory(
    AgentInventoryConfig {
      agent_id: "agent-1".to_owned(),
      agent_version: "agent-release".to_owned(),
      labels: BTreeMap::new(),
      remote_cache_configured: true,
    },
    vec![runtime],
    &runner,
    &sources,
    monitor.capacity().clone(),
  )
  .unwrap();
  assert_eq!(inventory.cache.as_ref().unwrap().runner_protocol, 3);
  assert!(inventory.cache.unwrap().remote_http);
}
