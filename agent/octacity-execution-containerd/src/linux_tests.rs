//! Portable contract checks for the Linux containerd adapter.

use std::os::{fd::FromRawFd as _, unix::net::UnixListener};

use super::*;
use octacity_execution::{
  CACHE_CA_CERTIFICATE_PATH, CACHE_DIRECTORY_PATH, CACHE_TOKEN_PATH, ExecutionCacheMounts, LocalCacheCapacity,
  WORKLOAD_IDENTITY_PATH,
};

fn platform() -> ExecutionPlatform {
  ExecutionPlatform {
    os: ExecutionOs::Linux,
    architecture: ExecutionArchitecture::Amd64,
  }
}

struct Fixture {
  _temporary: tempfile::TempDir,
  config: ContainerdEngineConfig,
  workspace: PathBuf,
}

impl Fixture {
  fn new() -> Self {
    let temporary = tempfile::tempdir().unwrap();
    let state = temporary.path().join("state");
    let work = temporary.path().join("work");
    let workspace = work.join("job");
    fs::create_dir(&state).unwrap();
    fs::create_dir(&work).unwrap();
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(workspace.join("data")).unwrap();
    let endpoint = temporary.path().join("containerd.sock");
    let listener = UnixListener::bind(&endpoint).unwrap();
    drop(listener);
    let config = ContainerdEngineConfig {
      agent_id: "agent.one".to_owned(),
      endpoint,
      namespace: "octacity".to_owned(),
      snapshotter: "overlayfs".to_owned(),
      runtime: "io.containerd.runc.v2".to_owned(),
      registry_config_dir: None,
      state_root: state.canonicalize().unwrap(),
      work_root: work.canonicalize().unwrap(),
      max_workspace_bytes: u64::MAX,
      cleanup_timeout: Duration::from_secs(1),
      pids_limit: 4096,
      open_files_limit: 65536,
    };
    Self {
      _temporary: temporary,
      config,
      workspace,
    }
  }

  fn request(&self) -> StartExecution {
    StartExecution {
      execution_id: "job-1".to_owned(),
      workspace_root: self.config.work_root.canonicalize().unwrap(),
      workspace: self.workspace.canonicalize().unwrap(),
      data_dir: self.workspace.join("data").canonicalize().unwrap(),
      workload_identity: None,
      cache: None,
      cpu_millis: 1000,
      memory_bytes: 64 * 1024 * 1024,
      writable_disk_bytes: u64::MAX,
      max_duration: Duration::from_secs(1),
      root: ExecutionTarget::Oci {
        reference: format!("example/build@sha256:{}", "0".repeat(64)),
        platform: host_capability().unwrap().platform,
        isolation: OciIsolation::Process,
      },
      network: NetworkAccess::Disabled,
    }
  }
}

#[test]
fn validates_explicit_containerd_configuration() {
  let fixture = Fixture::new();
  assert!(
    ContainerdEngine::new(fixture.config.clone())
      .err()
      .unwrap()
      .to_string()
      .contains("dedicated filesystem mount")
  );

  let mut config = fixture.config.clone();
  config.endpoint = PathBuf::from("relative.sock");
  assert!(ContainerdEngine::new(config).is_err());

  let mut config = fixture.config.clone();
  config.endpoint = fixture.config.work_root.clone();
  assert!(ContainerdEngine::new(config).is_err());

  let mut config = fixture.config.clone();
  config.namespace = "bad namespace".to_owned();
  assert!(ContainerdEngine::new(config).is_err());

  let mut config = fixture.config;
  config.cleanup_timeout = Duration::ZERO;
  assert!(ContainerdEngine::new(config).is_err());
}

#[test]
fn rejects_requests_the_engine_cannot_enforce() {
  let fixture = Fixture::new();
  let capability = host_capability().unwrap();
  let request = fixture.request();
  assert!(validate_request(&fixture.config, capability, &request).is_ok());

  let mut invalid = request.clone();
  invalid.network = NetworkAccess::Unrestricted;
  assert!(validate_request(&fixture.config, capability, &invalid).is_err());

  invalid = request.clone();
  invalid.cpu_millis = 1;
  assert!(validate_request(&fixture.config, capability, &invalid).is_err());

  invalid = request.clone();
  invalid.memory_bytes = 1;
  assert!(validate_request(&fixture.config, capability, &invalid).is_err());

  invalid = request.clone();
  invalid.writable_disk_bytes = 1;
  assert!(validate_request(&fixture.config, capability, &invalid).is_err());

  invalid = request.clone();
  invalid.root = ExecutionTarget::Native {
    platform: capability.platform,
  };
  assert!(validate_request(&fixture.config, capability, &invalid).is_err());

  invalid = request.clone();
  invalid.root = ExecutionTarget::Oci {
    reference: format!("example/build@sha256:{}", "0".repeat(64)),
    platform: capability.platform,
    isolation: OciIsolation::Hypervisor,
  };
  assert!(validate_request(&fixture.config, capability, &invalid).is_err());

  invalid = request;
  invalid.workspace_root = fixture.config.state_root.clone();
  assert!(validate_request(&fixture.config, capability, &invalid).is_err());
}

#[test]
fn derives_the_containerd_snapshot_chain_id() {
  let first = format!("sha256:{}", "1".repeat(64));
  let second = format!("sha256:{}", "2".repeat(64));
  let expected = format!("sha256:{:x}", Sha256::digest(format!("{first} {second}").as_bytes()));
  assert_eq!(chain_id(&[first, second]).unwrap(), expected);
  assert!(chain_id(&[]).is_err());
  assert!(chain_id(&["sha512:bad".to_owned()]).is_err());
}

#[test]
fn preserves_image_environment_and_overrides_sandbox_owned_values() {
  let environment = process_environment(vec![
    "PATH=/image/bin".to_owned(),
    "HOME=/root".to_owned(),
    "TOOLCHAIN=/toolchain".to_owned(),
  ])
  .unwrap();

  assert!(environment.contains(&"PATH=/image/bin".to_owned()));
  assert!(environment.contains(&"HOME=/workspace".to_owned()));
  assert!(environment.contains(&"TOOLCHAIN=/toolchain".to_owned()));
  assert!(process_environment(vec!["INVALID".to_owned()]).is_err());
}

#[test]
fn creates_a_restricted_oci_spec_and_guest_paths() {
  let fixture = Fixture::new();
  let temporary = tempfile::tempdir().unwrap();
  let release = temporary.path().join("release");
  let workspace = temporary.path().join("workspace");
  fs::create_dir_all(release.join("plugins")).unwrap();
  fs::create_dir_all(workspace.join("data")).unwrap();
  let runner = RunnerProgram {
    release_root: release.clone(),
    executable: release.join("octa-runner"),
    plugins_dir: release.join("plugins"),
    plugin_lock: release.join("plugins.lock"),
  };
  let identity = temporary.path().join("identity-token");
  fs::write(&identity, "signed-jwt").unwrap();
  let cache = temporary.path().join("cache");
  let cache_token = temporary.path().join("cache-token");
  let cache_ca = temporary.path().join("cache-ca.pem");
  fs::create_dir(&cache).unwrap();
  fs::write(&cache_token, "cache-secret").unwrap();
  fs::write(&cache_ca, "fixture-ca").unwrap();
  let request = StartExecution {
    execution_id: "job-1".to_owned(),
    workspace_root: workspace.clone(),
    workspace: workspace.clone(),
    data_dir: workspace.join("data"),
    workload_identity: Some(identity.clone()),
    cache: Some(ExecutionCacheMounts {
      capacity_root: cache.clone(),
      local_directory: cache.clone(),
      local_capacity: LocalCacheCapacity::new(2 * 1024 * 1024, 1_900_000, 1_800_000).unwrap(),
      aggregate_max_bytes: 4 * 1024 * 1024,
      token_file: Some(cache_token.clone()),
      ca_certificate_file: Some(cache_ca.clone()),
    }),
    cpu_millis: 2000,
    memory_bytes: 1024,
    writable_disk_bytes: 2048,
    max_duration: Duration::from_secs(1),
    root: ExecutionTarget::Oci {
      reference: format!("example/build@sha256:{}", "0".repeat(64)),
      platform: platform(),
      isolation: OciIsolation::Process,
    },
    network: NetworkAccess::Disabled,
  };
  let paths = guest_paths(&runner, &request).unwrap();
  let environment = process_environment(vec!["PATH=/tools/bin".to_owned(), "HOME=/workspace".to_owned()]).unwrap();
  let spec = oci_spec(&fixture.config, &runner, &request, &paths, &environment, "octacity-job").unwrap();
  assert_eq!(paths.data_dir, Path::new("/workspace/data"));
  assert_eq!(
    paths.cache.as_ref().unwrap().local_directory,
    Path::new(CACHE_DIRECTORY_PATH)
  );
  assert_eq!(spec["root"]["readonly"], true);
  assert_eq!(spec["linux"]["resources"]["cpu"]["quota"], 200_000);
  assert_eq!(spec["linux"]["resources"]["memory"]["limit"], 1024);
  assert_eq!(spec["linux"]["namespaces"][1]["type"], "network");
  assert_eq!(spec["process"]["noNewPrivileges"], true);
  assert_eq!(spec["process"]["env"][0], "HOME=/workspace");
  assert_eq!(spec["process"]["env"][1], "PATH=/tools/bin");
  assert_eq!(spec["linux"]["resources"]["pids"]["limit"], 4096);
  assert_eq!(spec["process"]["rlimits"][0]["hard"], 65536);
  assert_eq!(
    spec["annotations"]["com.octacity.security-profile"],
    SECURITY_PROFILE_VERSION
  );
  assert_eq!(spec["linux"]["seccomp"]["defaultAction"], "SCMP_ACT_ALLOW");
  let identity_mount = spec["mounts"]
    .as_array()
    .unwrap()
    .iter()
    .find(|mount| mount["destination"] == WORKLOAD_IDENTITY_PATH)
    .unwrap();
  assert_eq!(identity_mount["source"], identity.to_string_lossy().as_ref());
  assert!(
    identity_mount["options"]
      .as_array()
      .unwrap()
      .iter()
      .any(|value| value == "ro")
  );
  for (destination, source, readonly) in [
    (CACHE_DIRECTORY_PATH, cache, false),
    (CACHE_TOKEN_PATH, cache_token, true),
    (CACHE_CA_CERTIFICATE_PATH, cache_ca, true),
  ] {
    let mount = spec["mounts"]
      .as_array()
      .unwrap()
      .iter()
      .find(|mount| mount["destination"] == destination)
      .unwrap();
    assert_eq!(mount["source"], source.to_string_lossy().as_ref());
    assert_eq!(
      mount["options"].as_array().unwrap().iter().any(|value| value == "ro"),
      readonly
    );
  }
}

#[test]
fn decodes_containerd_cgroup_v2_metrics() {
  let encoded = CgroupV2Metrics {
    cpu: Some(CpuStat { usage_usec: 42_000 }),
    memory: Some(MemoryStat {
      usage: 123,
      max_usage: 456,
    }),
    io: Some(IoStat {
      usage: vec![IoEntry { rbytes: 7, wbytes: 9 }],
    }),
  }
  .encode_to_vec();
  let decoded = decode_cgroup_v2_metrics(&Any {
    type_url: "types.containerd.io/io.containerd.cgroups.v2.Metrics".to_owned(),
    value: encoded,
  })
  .unwrap();
  assert_eq!(decoded.cpu.unwrap().usage_usec, 42_000);
  assert_eq!(decoded.memory.unwrap().max_usage, 456);
  assert!(
    decode_cgroup_v2_metrics(&Any {
      type_url: "unsupported".to_owned(),
      value: Vec::new(),
    })
    .is_err()
  );
  assert!(
    decode_cgroup_v2_metrics(&Any {
      type_url: CGROUP_V2_METRICS_SUFFIX.to_owned(),
      value: vec![0xff],
    })
    .is_err()
  );
}

#[test]
fn uses_stable_bounded_resource_names() {
  let first = resource_id("agent-1", "job/attempt/1");
  let second = resource_id("agent-1", "job/attempt/2");
  assert!(first.starts_with("octacity-agent-1-"));
  assert!(first.len() < 80);
  assert_ne!(first, second);
  assert_ne!(agent_resource_prefix("agent.one"), agent_resource_prefix("agentone"));
}

#[test]
fn creates_private_fifos_and_removes_only_owned_directories() {
  use std::os::unix::fs::FileTypeExt as _;

  let fixture = Fixture::new();
  let io_directory = fixture.config.state_root.join("single-io");
  let io = ContainerIo::create(&io_directory).unwrap();
  assert!(fs::metadata(&io.stdin_path).unwrap().file_type().is_fifo());
  assert!(fs::metadata(&io.stdout_path).unwrap().file_type().is_fifo());
  assert!(fs::metadata(&io.stderr_path).unwrap().file_type().is_fifo());
  // Duplicated descriptors share their file status flags, so they let this
  // regression test observe the descriptors after `into_execution_io`
  // transfers the originals into Tokio files.
  let probes = [&io.stdin, &io.stdout, &io.stderr]
    .into_iter()
    .map(|file| {
      // SAFETY: dup returns a new descriptor or -1 without taking ownership
      // of the original descriptor.
      let descriptor = unsafe { libc::dup(file.as_raw_fd()) };
      assert!(descriptor >= 0);
      // SAFETY: a successful dup returns a uniquely owned descriptor.
      unsafe { File::from_raw_fd(descriptor) }
    })
    .collect::<Vec<_>>();
  assert!(
    probes
      .iter()
      .all(|file| file_status_flags(file) & libc::O_NONBLOCK != 0)
  );
  drop(io.into_execution_io().unwrap());
  assert!(
    probes
      .iter()
      .all(|file| file_status_flags(file) & libc::O_NONBLOCK == 0)
  );

  let root = fixture.config.state_root.join("cleanup");
  fs::create_dir(&root).unwrap();
  let owned = root.join(resource_id(&fixture.config.agent_id, "orphan"));
  let foreign = root.join(resource_id("other-agent", "orphan"));
  fs::create_dir(&owned).unwrap();
  fs::create_dir(&foreign).unwrap();
  remove_abandoned_io_directories(&root, &fixture.config.agent_id).unwrap();
  assert!(!owned.exists());
  assert!(foreign.exists());
}

fn file_status_flags(file: &File) -> libc::c_int {
  // SAFETY: F_GETFL only reads status flags from a valid owned descriptor.
  let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFL) };
  assert!(flags >= 0);
  flags
}

#[test]
fn validates_metadata_paths_platforms_and_namespaces() {
  assert!(validate_digest(&format!("sha256:{}", "a".repeat(64))).is_ok());
  assert!(validate_digest(&format!("sha256:{}", "A".repeat(64))).is_err());
  assert!(is_index_media_type("application/vnd.oci.image.index.v1+json"));
  assert!(is_manifest_media_type(
    "application/vnd.docker.distribution.manifest.v2+json"
  ));
  assert!(!is_manifest_media_type("text/plain"));
  assert_eq!(execution_os(ExecutionOs::Windows), "windows");
  assert_eq!(execution_os(ExecutionOs::Macos), "darwin");
  assert_eq!(execution_architecture(ExecutionArchitecture::Arm64), "arm64");
  let platform = containerd_platform(platform());
  assert_eq!(platform.os, "linux");
  assert_eq!(platform.architecture, "amd64");
  assert!(
    ImagePlatform {
      os: "linux".to_owned(),
      architecture: "amd64".to_owned(),
    }
    .matches(super::tests::platform())
  );
  assert!(namespaced((), "octacity").is_ok());
  assert!(namespaced((), "bad\nnamespace").is_err());
  assert!(validate_identifier("value", "safe.name-1").is_ok());
  assert!(validate_identifier("value", "not safe").is_err());

  let root = Path::new("/opt/octa");
  assert_eq!(
    map_path(root, Path::new("/opt/octa/plugins/shell"), Path::new("/guest")).unwrap(),
    Path::new("/guest/plugins/shell")
  );
  assert!(map_path(root, Path::new("/outside"), Path::new("/guest")).is_err());
}

#[tokio::test]
async fn bounds_grpc_operations_by_the_shared_deadline() {
  let deadline = operation_deadline(Duration::from_secs(1)).unwrap();
  assert_eq!(
    grpc_before(deadline, None, "fixture operation", async { Ok::<_, Status>(7) })
      .await
      .unwrap(),
    7
  );

  let error = grpc_before(deadline, None, "fixture operation", async {
    Err::<(), _>(Status::unavailable("fixture unavailable"))
  })
  .await
  .unwrap_err();
  assert!(error.to_string().contains("fixture unavailable"));

  let error = grpc_before(
    Instant::now(),
    None,
    "fixture operation",
    std::future::pending::<Result<(), Status>>(),
  )
  .await
  .unwrap_err();
  assert!(matches!(
    error,
    ExecutionError::TimedOut {
      operation: "fixture operation"
    }
  ));

  let cancellation = CancellationToken::new();
  cancellation.cancel();
  let error = grpc_before(
    deadline,
    Some(&cancellation),
    "fixture operation",
    std::future::pending::<Result<(), Status>>(),
  )
  .await
  .unwrap_err();
  assert!(matches!(error, ExecutionError::Cancelled));
}
