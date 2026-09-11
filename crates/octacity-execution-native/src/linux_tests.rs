//! Portable validation and naming tests for the Linux Native backend.

use super::*;
use super::{
  cgroup::{parse_number, read_control},
  security::denied_syscalls,
};

fn execution_request(root: &Path) -> StartExecution {
  let workspace = root.join("workspace");
  let data_dir = workspace.join("data");
  fs::create_dir_all(&data_dir).unwrap();
  StartExecution {
    execution_id: "job-1-attempt-1".to_owned(),
    workspace_root: root.to_owned(),
    workspace,
    data_dir,
    cpu_millis: 1500,
    memory_bytes: 64 * 1024 * 1024,
    writable_disk_bytes: u64::MAX,
    max_duration: Duration::from_secs(60),
    root: ExecutionTarget::Native {
      platform: host_execution_platform().unwrap(),
    },
    network: NetworkAccess::Disabled,
  }
}

fn runner(root: &Path) -> RunnerProgram {
  let release = root.join("release");
  RunnerProgram {
    executable: release.join("octa-runner"),
    plugins_dir: release.join("plugins"),
    plugin_lock: release.join("Octa.lock"),
    release_root: release,
  }
}

fn backend_fixture(root: &Path) -> NativeBackend {
  NativeBackend {
    cgroup_root: root.join("cgroups"),
    work_root: root.to_owned(),
    bubblewrap: PathBuf::from("/bin/false"),
    readonly_paths: Vec::new(),
    cleanup_timeout: Duration::from_millis(20),
    environment: BTreeMap::from([("PATH".to_owned(), "/usr/bin:/bin".to_owned())]),
    pids_limit: 64,
  }
}

fn native_config(cgroup_root: PathBuf, work_root: PathBuf) -> LinuxNativeConfig {
  LinuxNativeConfig {
    cgroup_root,
    work_root,
    bubblewrap: PathBuf::from("/bin/false"),
    readonly_paths: Vec::new(),
    runner_platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
    max_workspace_bytes: u64::MAX,
    cleanup_timeout: Duration::from_secs(1),
    pids_limit: 64,
    environment: BTreeMap::from([("PATH".to_owned(), "/usr/bin:/bin".to_owned())]),
  }
}

#[test]
fn creates_safe_stable_cgroup_names() {
  assert_eq!(cgroup_name("job-1"), cgroup_name("job-1"));
  assert!(cgroup_name("../../escape").starts_with("execution-"));
  assert!(!cgroup_name("../../escape").contains('/'));
  assert!(is_cgroup_name(std::ffi::OsStr::new(&cgroup_name("job-1"))));
  assert!(!is_cgroup_name(std::ffi::OsStr::new("execution-operator")));

  use std::os::unix::ffi::OsStringExt as _;
  let non_utf8 = std::ffi::OsString::from_vec(vec![0xff]);
  assert!(!is_cgroup_name(&non_utf8));
}

#[test]
fn parses_cgroup_io_counters() {
  let directory = tempfile::tempdir().unwrap();
  fs::write(
    directory.path().join("io.stat"),
    "8:0 rbytes=10 wbytes=20 rios=1 wios=2\n8:1 rbytes=3 wbytes=4\n",
  )
  .unwrap();
  assert_eq!(read_io(directory.path()).unwrap(), (13, 24));
}

#[test]
fn rejects_a_runner_built_for_another_host_platform() {
  let temporary = tempfile::tempdir().unwrap();
  let error = NativeBackend::new(LinuxNativeConfig {
    cgroup_root: temporary.path().to_owned(),
    work_root: temporary.path().to_owned(),
    bubblewrap: temporary.path().to_owned(),
    readonly_paths: Vec::new(),
    runner_platform: "macos-aarch64".to_owned(),
    max_workspace_bytes: 1024,
    cleanup_timeout: Duration::from_secs(1),
    pids_limit: 4096,
    environment: BTreeMap::from([("PATH".to_owned(), "/usr/bin:/bin".to_owned())]),
  })
  .err()
  .expect("a foreign runner must be rejected before host boundary validation");

  assert!(matches!(error, ExecutionError::Invalid(message) if message.contains("requires runner platform")));
}

#[test]
fn emits_short_sandbox_owned_runtime_directories() {
  let arguments = environment_arguments(&BTreeMap::from([("PATH".to_owned(), "/usr/bin:/bin".to_owned())]));
  let arguments: Vec<_> = arguments
    .iter()
    .map(|value| value.to_string_lossy().into_owned())
    .collect();

  assert!(
    arguments
      .windows(3)
      .any(|values| values == ["--setenv", "HOME", "/home/octacity"])
  );
  assert!(
    arguments
      .windows(3)
      .any(|values| values == ["--setenv", "TMPDIR", "/tmp"])
  );
  assert_eq!(network_arguments(&NetworkAccess::Disabled), &[] as &[&str]);
  assert_eq!(network_arguments(&NetworkAccess::Unrestricted), &["--share-net"]);
}

#[test]
fn builds_a_seccomp_descriptor_that_survives_exec() {
  let mut filter = create_seccomp_filter().unwrap();
  assert_eq!(unsafe { libc::fcntl(filter.as_raw_fd(), libc::F_GETFD) }, 0);
  assert!(filter.seek(std::io::SeekFrom::End(0)).unwrap() >= 8);
  assert!(denied_syscalls().contains(&libc::SYS_mount));
  assert!(denied_syscalls().contains(&libc::SYS_ptrace));
}

#[test]
fn rejects_ambient_runtime_directories_and_relative_path_entries() {
  let readonly = Vec::new();
  let error = validate_native_path(&BTreeMap::new(), &readonly).unwrap_err();
  assert!(matches!(error, ExecutionError::Invalid(message) if message.contains("requires PATH")));

  let error = validate_native_path(
    &BTreeMap::from([
      ("PATH".to_owned(), "relative:/usr/bin".to_owned()),
      ("HOME".to_owned(), "/tmp/home".to_owned()),
    ]),
    &readonly,
  )
  .unwrap_err();
  assert!(matches!(error, ExecutionError::Invalid(message) if message.contains("HOME")));

  let error = validate_native_path(
    &BTreeMap::from([("PATH".to_owned(), "relative:/usr/bin".to_owned())]),
    &readonly,
  )
  .unwrap_err();
  assert!(matches!(error, ExecutionError::Invalid(message) if message.contains("absolute")));
}

#[test]
fn validates_and_programs_a_delegated_cgroup_tree() {
  let directory = tempfile::tempdir().unwrap();
  fs::write(directory.path().join("cgroup.controllers"), "cpu memory io pids").unwrap();
  fs::write(directory.path().join("cgroup.subtree_control"), "cpu memory io pids").unwrap();
  validate_cgroup_root(directory.path()).unwrap();

  let request_root = tempfile::tempdir().unwrap();
  let request = execution_request(request_root.path());
  for name in ["cpu.stat", "memory.current", "memory.peak", "io.stat", "cgroup.kill"] {
    fs::write(directory.path().join(name), "").unwrap();
  }
  configure_cgroup(directory.path(), &request, 37).unwrap();
  assert_eq!(
    fs::read_to_string(directory.path().join("cpu.max")).unwrap(),
    "150000 100000"
  );
  assert_eq!(
    fs::read_to_string(directory.path().join("memory.max")).unwrap(),
    request.memory_bytes.to_string()
  );
  assert_eq!(
    fs::read_to_string(directory.path().join("memory.swap.max")).unwrap(),
    "0"
  );
  assert_eq!(fs::read_to_string(directory.path().join("pids.max")).unwrap(), "37");

  fs::remove_file(directory.path().join("cpu.stat")).unwrap();
  assert!(matches!(
    configure_cgroup(directory.path(), &request, 37),
    Err(ExecutionError::Unavailable(message)) if message.contains("cpu.stat")
  ));
}

#[test]
fn rejects_incomplete_or_invalid_cgroup_roots() {
  let missing = tempfile::tempdir().unwrap().path().join("missing");
  assert!(validate_cgroup_root(&missing).is_err());

  let directory = tempfile::tempdir().unwrap();
  assert!(validate_cgroup_root(directory.path()).is_err());
  fs::write(directory.path().join("cgroup.controllers"), "cpu memory io pids").unwrap();
  fs::write(directory.path().join("cgroup.subtree_control"), "cpu memory io").unwrap();
  let error = validate_cgroup_root(directory.path()).unwrap_err();
  assert!(matches!(error, ExecutionError::Unavailable(message) if message.contains("pids")));
}

#[test]
fn reads_and_rejects_cgroup_accounting_values() {
  let directory = tempfile::tempdir().unwrap();
  fs::write(directory.path().join("cpu.stat"), "user_usec 9\nusage_usec 12345\n").unwrap();
  fs::write(directory.path().join("memory.current"), "42\n").unwrap();
  fs::write(directory.path().join("io.stat"), "8:0 rbytes=10 wbytes=20\n").unwrap();
  assert_eq!(read_cpu_time(directory.path()).unwrap(), 12);
  assert_eq!(read_number(&directory.path().join("memory.current")).unwrap(), 42);
  assert_eq!(
    read_control(directory.path().join("io.stat")).unwrap(),
    "8:0 rbytes=10 wbytes=20\n"
  );
  assert!(parse_number("counter", "invalid").is_err());

  fs::write(directory.path().join("cpu.stat"), "user_usec 9\n").unwrap();
  assert!(read_cpu_time(directory.path()).is_err());
  fs::write(
    directory.path().join("io.stat"),
    format!("8:0 rbytes={}\n8:1 rbytes=1\n", u64::MAX),
  )
  .unwrap();
  assert!(read_io(directory.path()).is_err());
  fs::write(directory.path().join("io.stat"), "8:0 wbytes=invalid\n").unwrap();
  assert!(read_io(directory.path()).is_err());
}

#[tokio::test]
async fn kills_and_removes_fake_cgroup_directories_safely() {
  let root = tempfile::tempdir().unwrap();
  let cgroup = root.path().join("execution-test");
  fs::create_dir(&cgroup).unwrap();
  fs::write(cgroup.join("cgroup.kill"), "0").unwrap();
  kill_cgroup(&cgroup).unwrap();
  assert_eq!(fs::read_to_string(cgroup.join("cgroup.kill")).unwrap(), "1");
  fs::remove_file(cgroup.join("cgroup.kill")).unwrap();
  remove_cgroup(&cgroup, Duration::from_millis(10)).await.unwrap();
  assert!(kill_cgroup(&cgroup).is_ok());
  assert!(remove_cgroup(&cgroup, Duration::from_millis(10)).await.is_ok());

  let populated = root.path().join("populated");
  fs::create_dir(&populated).unwrap();
  fs::write(populated.join("member"), "still present").unwrap();
  assert!(remove_cgroup(&populated, Duration::from_millis(60)).await.is_err());

  let invalid_kill = root.path().join("invalid-kill");
  fs::create_dir_all(invalid_kill.join("cgroup.kill")).unwrap();
  assert!(kill_cgroup(&invalid_kill).is_err());
}

#[test]
fn validates_native_filesystem_and_path_boundaries() {
  let root = tempfile::tempdir().unwrap();
  let usage = filesystem_usage(root.path()).unwrap();
  assert!(usage.capacity_bytes >= usage.used_bytes);
  assert!(validate_workspace_root(root.path(), u64::MAX).is_err());
  assert!(validate_workspace_filesystem(root.path(), root.path(), u64::MAX).is_err());

  let readonly = tempfile::tempdir().unwrap();
  let executable_directory = readonly.path().join("bin");
  fs::create_dir(&executable_directory).unwrap();
  let environment = BTreeMap::from([("PATH".to_owned(), executable_directory.to_string_lossy().into_owned())]);
  assert!(validate_native_path(&environment, &[readonly.path().canonicalize().unwrap()]).is_ok());

  let outside = tempfile::tempdir().unwrap();
  let environment = BTreeMap::from([("PATH".to_owned(), outside.path().to_string_lossy().into_owned())]);
  assert!(validate_native_path(&environment, &[]).is_err());
  let missing = root.path().join("missing");
  let environment = BTreeMap::from([("PATH".to_owned(), missing.to_string_lossy().into_owned())]);
  assert!(validate_native_path(&environment, &[]).is_err());
}

#[test]
fn assembles_the_complete_bubblewrap_filesystem_without_starting_it() {
  use std::os::unix::fs::symlink;

  let root = tempfile::tempdir().unwrap();
  let request = execution_request(root.path());
  let runner = runner(root.path());
  let temporary = request.data_dir.join("tmp");
  let home = request.data_dir.join("home");
  fs::create_dir(&temporary).unwrap();
  fs::create_dir(&home).unwrap();
  let readonly_file = root.path().join("readonly-file");
  let readonly_link = root.path().join("readonly-link");
  fs::write(&readonly_file, "fixture").unwrap();
  symlink(&readonly_file, &readonly_link).unwrap();
  let missing = root.path().join("missing-readonly");
  let mut command = Command::new("/bin/false");

  add_native_filesystem(
    &mut command,
    &runner,
    &request,
    &temporary,
    &home,
    &[readonly_file.clone(), readonly_link.clone(), missing],
  )
  .unwrap();
  let arguments: Vec<_> = command
    .as_std()
    .get_args()
    .map(|argument| argument.to_string_lossy().into_owned())
    .collect();
  assert!(arguments.iter().any(|argument| argument == "--ro-bind"));
  assert!(arguments.iter().any(|argument| argument == "--symlink"));
  assert!(arguments.iter().any(|argument| argument == "--remount-ro"));
  assert!(arguments.iter().any(|argument| argument == "/home/octacity"));
  assert!(
    std::panic::catch_unwind(|| network_arguments(&NetworkAccess::Restricted {
      allowed_hosts: vec!["example.com".to_owned()]
    }))
    .is_err()
  );
  assert_eq!(host_execution_platform().unwrap().os, ExecutionOs::Linux);
}

#[tokio::test]
async fn rejects_native_start_requests_before_creating_kernel_state() {
  let root = tempfile::tempdir().unwrap();
  let backend = backend_fixture(root.path());
  let request = execution_request(root.path());
  let cancellation = CancellationToken::new();
  cancellation.cancel();
  assert!(matches!(
    backend.start(&runner(root.path()), request.clone(), cancellation).await,
    Err(ExecutionError::Cancelled)
  ));

  let mut invalid_runner = runner(root.path());
  invalid_runner.executable = PathBuf::from("relative");
  assert!(
    backend
      .start(&invalid_runner, request.clone(), CancellationToken::new())
      .await
      .is_err()
  );

  let mut foreign = request.clone();
  foreign.root = ExecutionTarget::Native {
    platform: ExecutionPlatform {
      os: ExecutionOs::Windows,
      architecture: ExecutionArchitecture::Amd64,
    },
  };
  assert!(
    backend
      .start(&runner(root.path()), foreign, CancellationToken::new())
      .await
      .is_err()
  );

  let mut restricted = request;
  restricted.network = NetworkAccess::Restricted {
    allowed_hosts: vec!["example.com".to_owned()],
  };
  assert!(
    backend
      .start(&runner(root.path()), restricted, CancellationToken::new())
      .await
      .is_err()
  );
}

#[tokio::test]
async fn cleans_only_recognized_native_orphan_directories() {
  let root = tempfile::tempdir().unwrap();
  let backend = backend_fixture(root.path());
  assert!(backend.cleanup_orphans().await.is_err());
  fs::create_dir(&backend.cgroup_root).unwrap();
  fs::write(backend.cgroup_root.join("controller-file"), "fixture").unwrap();
  backend.cleanup_orphans().await.unwrap();

  fs::create_dir(backend.cgroup_root.join("operator-owned")).unwrap();
  assert!(backend.cleanup_orphans().await.is_err());
  fs::remove_dir(backend.cgroup_root.join("operator-owned")).unwrap();

  let owned = backend.cgroup_root.join(cgroup_name("orphan"));
  fs::create_dir(&owned).unwrap();
  fs::write(owned.join("cgroup.kill"), "0").unwrap();
  assert!(backend.cleanup_orphans().await.is_err());
}

#[tokio::test]
async fn reports_native_usage_and_one_shot_process_state() {
  let root = tempfile::tempdir().unwrap();
  let cgroup = root.path().join("cgroup");
  fs::create_dir(&cgroup).unwrap();
  fs::write(cgroup.join("cpu.stat"), "usage_usec 2000\n").unwrap();
  fs::write(cgroup.join("memory.current"), "10\n").unwrap();
  fs::write(cgroup.join("memory.peak"), "20\n").unwrap();
  fs::write(cgroup.join("io.stat"), "8:0 rbytes=30 wbytes=40\n").unwrap();
  fs::write(cgroup.join("cgroup.kill"), "0").unwrap();
  let child = Command::new("/bin/true").spawn().unwrap();
  let mut execution = NativeExecution {
    child,
    io: None,
    paths: ExecutionPaths {
      workspace: root.path().to_owned(),
      data_dir: root.path().to_owned(),
      plugins_dir: root.path().to_owned(),
      plugin_lock: root.path().join("Octa.lock"),
    },
    cgroup,
    filesystem_root: root.path().to_owned(),
    cleanup_timeout: Duration::from_millis(20),
    started: Instant::now(),
    disk_peak_bytes: 0,
    reaped: false,
  };
  assert_eq!(execution.paths().workspace, root.path());
  assert!(matches!(execution.take_io(), Err(ExecutionError::IoTaken)));
  execution.kill().await.unwrap();
  let usage = execution.sample_usage().await.unwrap();
  assert_eq!(usage.cpu_time_ms, 2);
  assert_eq!(usage.memory_current_bytes, 10);
  assert_eq!(usage.memory_peak_bytes, 20);
  assert_eq!((usage.io_read_bytes, usage.io_written_bytes), (30, 40));
  assert_eq!(execution.wait().await.unwrap().code, Some(0));
  assert!(matches!(execution.wait().await, Err(ExecutionError::Reaped)));
  fs::remove_dir_all(&execution.cgroup).unwrap();
  Box::new(execution).destroy().await.unwrap();
}

#[tokio::test]
async fn preserves_start_failure_when_fake_cgroup_cleanup_succeeds() {
  let root = tempfile::tempdir().unwrap();
  let absent = root.path().join("absent");
  let error = rollback_start_failure(ExecutionError::Cancelled, &absent, None, Duration::from_millis(10)).await;
  assert!(matches!(error, ExecutionError::Cancelled));

  let cgroup = root.path().join("failed-cgroup");
  fs::create_dir_all(cgroup.join("cgroup.kill")).unwrap();
  let mut command = Command::new("/bin/sleep");
  command.arg("5").process_group(0);
  let mut child = command.spawn().unwrap();
  let error = rollback_start_failure(
    ExecutionError::Cancelled,
    &cgroup,
    Some(&mut child),
    Duration::from_millis(20),
  )
  .await;
  assert!(matches!(error, ExecutionError::OperationAndCleanup { .. }));
}

#[test]
fn rejects_invalid_native_backend_roots_and_limits_in_order() {
  let root = tempfile::tempdir().unwrap();
  let mut config = native_config(root.path().join("missing-cgroup"), root.path().to_owned());
  config.cleanup_timeout = Duration::ZERO;
  assert!(matches!(
    NativeBackend::new(config),
    Err(ExecutionError::Invalid(message)) if message.contains("cleanup timeout")
  ));

  assert!(matches!(
    NativeBackend::new(native_config(root.path().join("missing-cgroup"), root.path().to_owned())),
    Err(ExecutionError::Unavailable(message)) if message.contains("native cgroup root")
  ));

  let cgroup = root.path().join("cgroup");
  fs::create_dir(&cgroup).unwrap();
  fs::write(cgroup.join("cgroup.controllers"), "cpu memory io pids").unwrap();
  fs::write(cgroup.join("cgroup.subtree_control"), "cpu memory io pids").unwrap();
  assert!(matches!(
    NativeBackend::new(native_config(cgroup, root.path().join("missing-work-root"))),
    Err(ExecutionError::Unavailable(message)) if message.contains("native work root")
  ));
}

#[test]
fn rejects_a_nul_in_the_pre_exec_cgroup_path() {
  use std::os::unix::ffi::OsStringExt as _;

  let path = PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/cgroup\0invalid".to_vec()));
  let mut command = Command::new("/bin/true");
  assert!(matches!(
    attach_to_cgroup_before_exec(&mut command, &path),
    Err(ExecutionError::Invalid(message)) if message.contains("NUL")
  ));
}
