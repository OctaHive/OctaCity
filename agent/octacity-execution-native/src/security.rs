//! Builds the Bubblewrap filesystem and seccomp security profile.

use super::*;
use octacity_execution::{CACHE_CA_CERTIFICATE_PATH, CACHE_DIRECTORY_PATH, CACHE_TOKEN_PATH, WORKLOAD_IDENTITY_PATH};

/// Converts the signed network policy into Bubblewrap namespace arguments.
pub(super) fn network_arguments(network: &NetworkAccess) -> &'static [&'static str] {
  match network {
    NetworkAccess::Unrestricted => &["--share-net"],
    NetworkAccess::Disabled => &[],
    NetworkAccess::Restricted { .. } => unreachable!("restricted networking is rejected before command assembly"),
  }
}

/// Constructs the complete environment passed to the Native runner.
pub(super) fn environment_arguments(environment: &BTreeMap<String, String>) -> Vec<std::ffi::OsString> {
  environment
    .iter()
    .flat_map(|(name, value)| ["--setenv".into(), name.into(), value.into()])
    .chain([
      "--setenv".into(),
      "HOME".into(),
      "/home/octacity".into(),
      "--setenv".into(),
      "TMPDIR".into(),
      "/tmp".into(),
    ])
    .collect()
}

/// Confines every executable search path to an operator-approved read-only root.
pub(super) fn validate_native_path(
  environment: &BTreeMap<String, String>,
  readonly_paths: &[PathBuf],
) -> Result<(), ExecutionError> {
  if environment.contains_key("HOME") || environment.contains_key("TMPDIR") {
    return Err(ExecutionError::Invalid(
      "native environment must not override sandbox-owned HOME or TMPDIR".to_owned(),
    ));
  }
  let path = environment
    .get("PATH")
    .ok_or_else(|| ExecutionError::Invalid("native environment requires PATH".to_owned()))?;
  for entry in std::env::split_paths(path) {
    if !entry.is_absolute() {
      return Err(ExecutionError::Invalid(
        "native PATH entries must be absolute and must not be empty".to_owned(),
      ));
    }
    let canonical = entry
      .canonicalize()
      .map_err(|error| unavailable(format!("native PATH entry '{}': {error}", entry.display())))?;
    if !canonical.starts_with("/usr") && !readonly_paths.iter().any(|root| canonical.starts_with(root)) {
      return Err(ExecutionError::Invalid(format!(
        "native PATH entry '{}' is outside /usr and native_linux_readonly_paths",
        entry.display()
      )));
    }
  }
  Ok(())
}

/// Adds the fixed Native filesystem layout and explicit read-only host paths.
pub(super) fn add_native_filesystem(
  command: &mut Command,
  runner: &RunnerProgram,
  request: &StartExecution,
  temporary_directory: &Path,
  home_directory: &Path,
  readonly_paths: &[PathBuf],
) -> Result<(), ExecutionError> {
  for path in ["/usr", "/bin", "/sbin", "/lib", "/lib64"] {
    expose_host_path(command, Path::new(path))?;
  }
  for path in [
    "/etc/alternatives",
    "/etc/ca-certificates",
    "/etc/group",
    "/etc/hosts",
    "/etc/ld.so.cache",
    "/etc/localtime",
    "/etc/nsswitch.conf",
    "/etc/passwd",
    "/etc/pki",
    "/etc/protocols",
    "/etc/resolv.conf",
    "/etc/services",
    "/etc/ssl",
    "/etc/ssh/ssh_config",
  ] {
    expose_host_path(command, Path::new(path))?;
  }
  for path in readonly_paths {
    expose_host_path(command, path)?;
  }
  ro_bind(command, &runner.release_root, &runner.release_root);
  // The backend-wide work root may contain identity material, abandoned state,
  // and eventually concurrent jobs. Expose only this job's writable workspace;
  // quota accounting can still inspect `workspace_root` from the host.
  command.arg("--dir").arg(&request.workspace);
  bind(command, &request.workspace, &request.workspace);
  if request.workload_identity.is_some()
    || request
      .cache
      .as_ref()
      .is_some_and(|cache| cache.token_file.is_some() || cache.ca_certificate_file.is_some())
  {
    command.arg("--dir").arg("/run");
  }
  if let Some(identity) = &request.workload_identity {
    ro_bind(command, identity, Path::new(WORKLOAD_IDENTITY_PATH));
  }
  if let Some(cache) = &request.cache {
    command.arg("--dir").arg("/var").arg("--dir").arg("/var/cache");
    bind(command, &cache.local_directory, Path::new(CACHE_DIRECTORY_PATH));
    if cache.token_file.is_some() || cache.ca_certificate_file.is_some() {
      command.arg("--dir").arg("/run/octa-cache");
    }
    if let Some(token) = &cache.token_file {
      ro_bind(command, token, Path::new(CACHE_TOKEN_PATH));
    }
    if let Some(certificate) = &cache.ca_certificate_file {
      ro_bind(command, certificate, Path::new(CACHE_CA_CERTIFICATE_PATH));
    }
  }
  bind(command, temporary_directory, Path::new("/tmp"));
  bind(command, home_directory, Path::new("/home/octacity"));
  command
    .arg("--dev")
    .arg("/dev")
    .arg("--proc")
    .arg("/proc")
    // Only child mounts remain writable after the anonymous root is sealed.
    .arg("--remount-ro")
    .arg("/");
  Ok(())
}

/// Exposes one canonical host path read-only at the same absolute guest path.
pub(super) fn expose_host_path(command: &mut Command, path: &Path) -> Result<(), ExecutionError> {
  let metadata = match fs::symlink_metadata(path) {
    Ok(metadata) => metadata,
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
    Err(error) => return Err(ExecutionError::Io(error)),
  };
  if metadata.file_type().is_symlink() {
    let target = fs::read_link(path).map_err(ExecutionError::Io)?;
    command.arg("--symlink").arg(target).arg(path);
  } else {
    ro_bind(command, path, path);
  }
  Ok(())
}

pub(super) fn ro_bind(command: &mut Command, source: &Path, destination: &Path) {
  command.arg("--ro-bind").arg(source).arg(destination);
}

pub(super) fn bind(command: &mut Command, source: &Path, destination: &Path) {
  command.arg("--bind").arg(source).arg(destination);
}

/// Creates an anonymous descriptor in the exact binary format consumed by
/// Bubblewrap's `--seccomp` option. The descriptor deliberately survives
/// exec into Bubblewrap and has no filesystem name to clean up.
pub(super) fn create_seccomp_filter() -> Result<File, ExecutionError> {
  let architecture: TargetArch = std::env::consts::ARCH.try_into().map_err(|_| {
    unavailable(format!(
      "seccomp does not support host architecture '{}'",
      std::env::consts::ARCH
    ))
  })?;
  let denied = denied_syscalls()
    .into_iter()
    .map(|number| (number, Vec::new()))
    .collect();
  let program: BpfProgram = SeccompFilter::new(
    denied,
    SeccompAction::Allow,
    SeccompAction::Errno(libc::EPERM as u32),
    architecture,
  )
  .and_then(TryInto::try_into)
  .map_err(|error| backend(format!("compile native seccomp profile: {error}")))?;

  let name = c"octacity-native-seccomp";
  // SAFETY: `name` is NUL terminated and the returned descriptor is owned by
  // the `File` constructed immediately below.
  let descriptor = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC) };
  if descriptor < 0 {
    return Err(ExecutionError::Io(std::io::Error::last_os_error()));
  }
  // SAFETY: `descriptor` is a fresh owned descriptor after successful
  // `memfd_create`; `File` becomes its sole owner.
  let mut file = unsafe { File::from_raw_fd(descriptor) };
  for instruction in program {
    file
      .write_all(&instruction.code.to_ne_bytes())
      .map_err(ExecutionError::Io)?;
    file
      .write_all(&[instruction.jt, instruction.jf])
      .map_err(ExecutionError::Io)?;
    file
      .write_all(&instruction.k.to_ne_bytes())
      .map_err(ExecutionError::Io)?;
  }
  file.rewind().map_err(ExecutionError::Io)?;
  // Bubblewrap reads the filter after exec. Clear only FD_CLOEXEC; the parent
  // retains ownership and closes its copy immediately after spawn returns.
  if unsafe { libc::fcntl(descriptor, libc::F_SETFD, 0) } != 0 {
    return Err(ExecutionError::Io(std::io::Error::last_os_error()));
  }
  Ok(file)
}

pub(super) fn denied_syscalls() -> Vec<i64> {
  let denied = vec![
    libc::SYS_acct,
    libc::SYS_add_key,
    libc::SYS_bpf,
    libc::SYS_chroot,
    libc::SYS_delete_module,
    libc::SYS_finit_module,
    libc::SYS_init_module,
    libc::SYS_kcmp,
    libc::SYS_kexec_file_load,
    libc::SYS_kexec_load,
    libc::SYS_keyctl,
    libc::SYS_lookup_dcookie,
    libc::SYS_mount,
    libc::SYS_move_mount,
    libc::SYS_open_by_handle_at,
    libc::SYS_open_tree,
    libc::SYS_perf_event_open,
    libc::SYS_pivot_root,
    libc::SYS_ptrace,
    libc::SYS_quotactl,
    libc::SYS_reboot,
    libc::SYS_request_key,
    libc::SYS_setns,
    libc::SYS_swapoff,
    libc::SYS_swapon,
    libc::SYS_syslog,
    libc::SYS_umount2,
    libc::SYS_unshare,
    libc::SYS_userfaultfd,
  ];
  #[cfg(target_arch = "x86_64")]
  return denied.into_iter().chain([libc::SYS_ioperm, libc::SYS_iopl]).collect();
  #[cfg(not(target_arch = "x86_64"))]
  denied
}

/// Resolves the compile-time host tuple advertised by the Native backend.
pub(super) fn host_execution_platform() -> Result<ExecutionPlatform, ExecutionError> {
  let architecture = match std::env::consts::ARCH {
    "x86_64" => ExecutionArchitecture::Amd64,
    "aarch64" => ExecutionArchitecture::Arm64,
    architecture => {
      return Err(ExecutionError::Unavailable(format!(
        "Native execution does not support host architecture '{architecture}'"
      )));
    }
  };
  Ok(ExecutionPlatform {
    os: ExecutionOs::Linux,
    architecture,
  })
}
