//! Sandbox planning and configuration boundary tests.

use super::plan::{directory_size, exact_mib, guest_path};
use super::*;
use octacity_execution::LocalCacheCapacity;

fn runner(root: &Path) -> RunnerProgram {
  RunnerProgram {
    release_root: root.to_owned(),
    executable: root.join("octa-runner"),
    plugins_dir: root.join("plugins"),
    plugin_lock: root.join("Octa.lock"),
  }
}

fn test_capability() -> OciCapability {
  GUEST_CAPABILITY.unwrap_or(OciCapability {
    platform: ExecutionPlatform {
      os: ExecutionOs::Linux,
      architecture: ExecutionArchitecture::Amd64,
    },
    isolation: OciIsolation::Hypervisor,
  })
}

fn engine_config(
  runtime_root: &Path,
  agent_id: &str,
  state_root: &Path,
  work_root: &Path,
  runner_platform: &str,
  cleanup_timeout: Duration,
) -> MicrosandboxEngineConfig {
  let executable = runtime_root.join("msb");
  let libkrunfw = runtime_root.join("libkrunfw");
  fs::write(&executable, []).unwrap();
  fs::write(&libkrunfw, []).unwrap();
  MicrosandboxEngineConfig {
    agent_id: agent_id.to_owned(),
    state_root: state_root.to_owned(),
    work_root: work_root.to_owned(),
    runner_platform: runner_platform.to_owned(),
    executable,
    libkrunfw,
    cleanup_timeout,
    metrics_sample_interval: Duration::from_secs(1),
  }
}

#[test]
fn keeps_microsandbox_state_under_the_agent_state_root() {
  let temporary = tempfile::tempdir().unwrap();
  let Some(platform) = GUEST_PLATFORM else {
    assert!(matches!(
      MicrosandboxEngine::new(engine_config(
        temporary.path(),
        "agent-1",
        temporary.path(),
        temporary.path(),
        "linux-x86_64",
        Duration::from_secs(5)
      )),
      Err(ExecutionError::Unavailable(_))
    ));
    return;
  };
  let backend = MicrosandboxEngine::new(engine_config(
    temporary.path(),
    "agent-1",
    temporary.path(),
    temporary.path(),
    platform,
    Duration::from_secs(5),
  ))
  .unwrap();
  let local = backend.backend.as_local().unwrap();
  let expected_home = temporary.path().canonicalize().unwrap().join("microsandbox");
  assert_eq!(local.config().home.as_deref(), Some(expected_home.as_path()));
  assert!(
    MicrosandboxEngine::new(engine_config(
      temporary.path(),
      "",
      temporary.path(),
      temporary.path(),
      platform,
      Duration::from_secs(5)
    ))
    .is_err()
  );
  assert!(
    MicrosandboxEngine::new(engine_config(
      temporary.path(),
      "agent-1",
      Path::new("relative"),
      temporary.path(),
      platform,
      Duration::from_secs(5)
    ))
    .is_err()
  );
  assert!(
    MicrosandboxEngine::new(engine_config(
      temporary.path(),
      "agent-1",
      temporary.path(),
      Path::new("relative"),
      platform,
      Duration::from_secs(5)
    ))
    .is_err()
  );
  assert!(
    MicrosandboxEngine::new(engine_config(
      temporary.path(),
      "agent-1",
      temporary.path(),
      temporary.path(),
      "windows-x86_64",
      Duration::from_secs(5)
    ))
    .is_err()
  );
  let mut invalid_metrics = engine_config(
    temporary.path(),
    "agent-1",
    temporary.path(),
    temporary.path(),
    platform,
    Duration::from_secs(5),
  );
  invalid_metrics.metrics_sample_interval = Duration::ZERO;
  assert!(MicrosandboxEngine::new(invalid_metrics).is_err());
}

#[tokio::test]
async fn rejects_a_workspace_outside_the_configured_work_root() {
  let temporary = tempfile::tempdir().unwrap();
  let work_root = temporary.path().join("work");
  let other_root = temporary.path().join("other");
  fs::create_dir(&work_root).unwrap();
  fs::create_dir(&other_root).unwrap();
  fs::create_dir(other_root.join(".octacity")).unwrap();
  let Some(platform) = GUEST_PLATFORM else {
    return;
  };
  let backend = MicrosandboxEngine::new(engine_config(
    temporary.path(),
    "agent-1",
    temporary.path(),
    &work_root,
    platform,
    Duration::from_secs(5),
  ))
  .unwrap();
  let result = backend
    .start(
      &runner(temporary.path()),
      request(&other_root),
      CancellationToken::new(),
    )
    .await;

  assert!(matches!(result, Err(ExecutionError::Invalid(message)) if message.contains("backend work_root")));

  let cancellation = CancellationToken::new();
  cancellation.cancel();
  let result = backend
    .start(&runner(temporary.path()), request(&work_root), cancellation)
    .await;
  assert!(matches!(result, Err(ExecutionError::Cancelled)));
}

#[tokio::test]
async fn startup_rollback_is_idempotent_when_no_sandbox_was_persisted() {
  let Some(platform) = GUEST_PLATFORM else {
    return;
  };
  let temporary = tempfile::tempdir().unwrap();
  let engine = MicrosandboxEngine::new(engine_config(
    temporary.path(),
    "agent-1",
    temporary.path(),
    temporary.path(),
    platform,
    Duration::from_secs(1),
  ))
  .unwrap();
  let error = cleanup_named_sandbox_after_start_failure(
    engine.backend,
    "octacity-absent",
    Duration::from_secs(1),
    ExecutionError::TimedOut {
      operation: "fixture startup",
    },
  )
  .await;
  assert!(matches!(
    error,
    ExecutionError::TimedOut {
      operation: "fixture startup"
    }
  ));
}

fn request(workspace: &Path) -> StartExecution {
  StartExecution {
    execution_id: "job-1-attempt-1".to_owned(),
    workspace_root: workspace.to_owned(),
    workspace: workspace.to_owned(),
    data_dir: workspace.join(".octacity"),
    workload_identity: None,
    cache: None,
    cpu_millis: 2000,
    memory_bytes: 512 * MEBIBYTE,
    writable_disk_bytes: 1024 * MEBIBYTE,
    max_duration: Duration::from_secs(600),
    root: ExecutionTarget::Oci {
      reference:
        "registry.example.com/octacity/build@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
          .to_owned(),
      platform: test_capability().platform,
      isolation: OciIsolation::Hypervisor,
    },
    network: NetworkAccess::Disabled,
  }
}

#[tokio::test]
async fn maps_verified_host_paths_into_the_guest() {
  let temporary = tempfile::tempdir().unwrap();
  let root = temporary.path().canonicalize().unwrap();
  let release = root.join("release");
  let work_root = root.join("work");
  let workspace = work_root.join("workspace");
  fs::create_dir(&release).unwrap();
  fs::create_dir(&work_root).unwrap();
  fs::create_dir(&workspace).unwrap();
  fs::create_dir(workspace.join(".octacity")).unwrap();
  let identity = work_root.join("identity-token");
  fs::write(&identity, "signed-jwt").unwrap();
  let cache = root.join("cache");
  let cache_token = root.join("cache-token");
  let cache_ca = root.join("cache-ca.pem");
  fs::create_dir(&cache).unwrap();
  fs::write(&cache_token, "cache-secret").unwrap();
  fs::write(&cache_ca, "fixture-ca").unwrap();
  let mut request = request(&workspace);
  request.workspace_root = work_root;
  request.workload_identity = Some(identity.clone());
  request.cache = Some(ExecutionCacheMounts {
    capacity_root: cache.clone(),
    local_directory: cache.clone(),
    local_capacity: LocalCacheCapacity::new(2 * 1024 * 1024, 1_900_000, 1_800_000).unwrap(),
    aggregate_max_bytes: 4 * 1024 * 1024,
    token_file: Some(cache_token.clone()),
    ca_certificate_file: Some(cache_ca.clone()),
  });
  let plan = SandboxPlan::build("agent-1", &runner(&release), &request)
    .await
    .unwrap();
  assert_eq!(plan.guest_executable, "/opt/octacity/octa/octa-runner");
  assert_eq!(plan.guest_plugins_dir, Path::new("/opt/octacity/octa/plugins"));
  assert_eq!(plan.guest_data_dir, Path::new("/workspace/.octacity"));
  assert_eq!(plan.root_tmpfs_mib, 256);
  assert_eq!(plan.workspace_quota_mib, 1024);
  assert_eq!(plan.workload_identity, Some(identity));
  let cache_plan = plan.cache.unwrap();
  assert_eq!(cache_plan.mounts.capacity_root, cache);
  assert_eq!(cache_plan.mounts.token_file, Some(cache_token));
  assert_eq!(cache_plan.mounts.ca_certificate_file, Some(cache_ca));
  assert_eq!(cache_plan.quota_mib, 2);
}

#[tokio::test]
async fn rejects_unrepresentable_limits_and_host_roots() {
  let temporary = tempfile::tempdir().unwrap();
  let release = temporary.path().join("release");
  let workspace = temporary.path().join("workspace");
  fs::create_dir(&release).unwrap();
  fs::create_dir(&workspace).unwrap();
  fs::create_dir(workspace.join(".octacity")).unwrap();
  let mut value = request(&workspace);
  value.cpu_millis = 1500;
  assert!(SandboxPlan::build("agent-1", &runner(&release), &value).await.is_err());
  value = request(&workspace);
  value.root = ExecutionTarget::Native {
    platform: test_capability().platform,
  };
  assert!(SandboxPlan::build("agent-1", &runner(&release), &value).await.is_err());
}

#[test]
fn constructs_exact_network_policies() {
  assert!(network_policy(&NetworkAccess::Disabled).unwrap().is_none());
  assert!(network_policy(&NetworkAccess::Unrestricted).unwrap().is_some());
  assert!(
    network_policy(&NetworkAccess::Restricted {
      allowed_hosts: vec!["vault.example.com".to_owned()]
    })
    .is_ok()
  );
}

#[tokio::test]
async fn enforces_deadlines_cancellation_and_capability_selection() {
  let temporary = tempfile::tempdir().unwrap();
  let oci_request = request(temporary.path());
  assert_eq!(requested_capability(&oci_request).unwrap(), test_capability());

  let mut native_request = oci_request;
  native_request.root = ExecutionTarget::Native {
    platform: test_capability().platform,
  };
  assert!(matches!(
    requested_capability(&native_request),
    Err(ExecutionError::Unavailable(message)) if message.contains("OCI root image")
  ));

  let cancellation = CancellationToken::new();
  assert_eq!(
    before_deadline(
      Instant::now() + Duration::from_secs(1),
      &cancellation,
      "fixture",
      async { 7 }
    )
    .await
    .unwrap(),
    7
  );

  cancellation.cancel();
  assert!(matches!(
    before_deadline(
      Instant::now() + Duration::from_secs(1),
      &cancellation,
      "fixture",
      async { std::future::pending::<()>().await }
    )
    .await,
    Err(ExecutionError::Cancelled)
  ));

  assert!(matches!(
    before_deadline(Instant::now(), &CancellationToken::new(), "fixture timeout", async {
      std::future::pending::<()>().await
    })
    .await,
    Err(ExecutionError::TimedOut {
      operation: "fixture timeout"
    })
  ));
}

#[test]
fn converts_sdk_durations_without_losing_limits() {
  assert_eq!(duration_seconds_ceil(Duration::ZERO), 0);
  assert_eq!(duration_seconds_ceil(Duration::from_nanos(1)), 1);
  assert_eq!(duration_seconds_ceil(Duration::from_secs(2)), 2);
  assert_eq!(millis(Duration::from_millis(123)), 123);
  assert_eq!(millis(Duration::MAX), u64::MAX);

  assert!(matches!(invalid("invalid"), ExecutionError::Invalid(message) if message == "invalid"));
  assert!(matches!(
    unavailable("unavailable"),
    ExecutionError::Unavailable(message) if message == "unavailable"
  ));
  assert!(matches!(backend("backend"), ExecutionError::Backend(message) if message == "backend"));
}

#[tokio::test]
async fn rejects_exhausted_workspace_quotas_and_invalid_mappings() {
  let temporary = tempfile::tempdir().unwrap();
  let release = temporary.path().join("release");
  let workspace = temporary.path().join("workspace");
  fs::create_dir(&release).unwrap();
  fs::create_dir(&workspace).unwrap();
  fs::create_dir(workspace.join(".octacity")).unwrap();
  fs::write(workspace.join("existing.bin"), vec![0; (MEBIBYTE + 1) as usize]).unwrap();

  let mut value = request(&workspace);
  value.writable_disk_bytes = MEBIBYTE;
  assert!(matches!(
    SandboxPlan::build("agent-1", &runner(&release), &value).await,
    Err(ExecutionError::Unavailable(message)) if message.contains("already exceeds")
  ));
  assert!(canonical_runtime_file("runtime", Path::new("relative")).is_err());
  assert_eq!(guest_path("/guest", &workspace, &workspace).unwrap(), "/guest");
  assert!(guest_path("/guest", &workspace, temporary.path()).is_err());
  assert!(exact_mib("memory", MEBIBYTE + 1).is_err());
  assert!(directory_size(&temporary.path().join("missing")).is_err());
}

#[cfg(unix)]
#[test]
fn accounts_for_nested_files_without_following_symlinks() {
  use std::{os::unix::fs::symlink, os::unix::net::UnixListener};

  let temporary = tempfile::tempdir().unwrap();
  let nested = temporary.path().join("nested");
  fs::create_dir(&nested).unwrap();
  fs::write(nested.join("one"), b"1234").unwrap();
  symlink(temporary.path().join("missing-target"), nested.join("ignored-link")).unwrap();

  assert_eq!(directory_size(temporary.path()).unwrap(), 4);

  let socket_root = tempfile::tempdir().unwrap();
  let _listener = UnixListener::bind(socket_root.path().join("socket")).unwrap();
  assert!(matches!(
    directory_size(socket_root.path()),
    Err(ExecutionError::Backend(message)) if message.contains("unsupported special file")
  ));
}
