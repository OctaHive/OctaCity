//! Portable validation and naming tests for the Linux Native backend.

use super::*;

#[test]
fn creates_safe_stable_cgroup_names() {
  assert_eq!(cgroup_name("job-1"), cgroup_name("job-1"));
  assert!(cgroup_name("../../escape").starts_with("execution-"));
  assert!(!cgroup_name("../../escape").contains('/'));
  assert!(is_cgroup_name(std::ffi::OsStr::new(&cgroup_name("job-1"))));
  assert!(!is_cgroup_name(std::ffi::OsStr::new("execution-operator")));
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
