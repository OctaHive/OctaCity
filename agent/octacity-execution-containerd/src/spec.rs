//! Builds the fixed OCI runtime specification and guest path mapping.

use super::*;
use octacity_execution::{
  CACHE_CA_CERTIFICATE_PATH, CACHE_DIRECTORY_PATH, CACHE_TOKEN_PATH, FACTORY_OUTPUT_ROOT, FACTORY_PROTECTED_INPUT_ROOT,
  FACTORY_SCRATCH_ROOT, FACTORY_SOURCE_ROOT, WORKLOAD_IDENTITY_PATH,
};

pub(super) const LINUX_UTS_HOSTNAME_MAX_BYTES: usize = 64;

/// Maps verified host paths into fixed paths inside the OCI root filesystem.
pub(super) fn guest_paths(runner: &RunnerProgram, request: &StartExecution) -> Result<ExecutionPaths, ExecutionError> {
  let (workspace, data_dir) = match &request.factory {
    Some(factory) => (
      PathBuf::from(FACTORY_SOURCE_ROOT),
      map_path(&factory.scratch, &request.data_dir, Path::new(FACTORY_SCRATCH_ROOT))?,
    ),
    None => (
      PathBuf::from(GUEST_WORKSPACE),
      map_path(&request.workspace, &request.data_dir, Path::new(GUEST_WORKSPACE))?,
    ),
  };
  Ok(ExecutionPaths {
    workspace,
    data_dir,
    plugins_dir: map_path(&runner.release_root, &runner.plugins_dir, Path::new(GUEST_RELEASE))?,
    plugin_lock: map_path(&runner.release_root, &runner.plugin_lock, Path::new(GUEST_RELEASE))?,
    cache: request
      .cache
      .as_ref()
      .map(octacity_execution::ExecutionCacheMounts::projected_paths),
  })
}

/// Builds the complete OCI process and mount contract for one runner job.
///
/// The caller may choose the runtime implementation, but cannot add mounts,
/// capabilities, environment variables, or namespaces outside this contract.
pub(super) fn oci_spec(
  config: &ContainerdEngineConfig,
  runner: &RunnerProgram,
  request: &StartExecution,
  paths: &ExecutionPaths,
  image_environment: &[String],
  id: &str,
) -> Result<serde_json::Value, ExecutionError> {
  let executable = map_path(&runner.release_root, &runner.executable, Path::new(GUEST_RELEASE))?;
  let hostname = oci_hostname(id);
  // The agent keeps each workspace private (0700). Matching its ownership lets
  // the capability-free runner and its plugin children traverse the bind mount
  // without granting container root or weakening the host-side permissions.
  let workspace_metadata = fs::metadata(&request.workspace)
    .map_err(|error| backend(format!("inspect workspace '{}': {error}", request.workspace.display())))?;
  let cpu_period = 100_000_u64;
  let cpu_quota = u64::from(request.cpu_millis)
    .checked_mul(cpu_period)
    .ok_or_else(|| invalid("CPU limit overflows the OCI quota representation"))?
    / 1000;
  let temporary_bytes = (request.memory_bytes / 8).min(1024 * 1024 * 1024);
  let temporary_size = format!("size={temporary_bytes}");
  let mut spec = serde_json::json!({
    "ociVersion": "1.1.0",
    "process": {
      "terminal": false,
      "user": { "uid": workspace_metadata.uid(), "gid": workspace_metadata.gid() },
      "args": [executable],
      "env": image_environment,
      "cwd": paths.workspace,
      "noNewPrivileges": true,
      "rlimits": [{ "type": "RLIMIT_NOFILE", "hard": config.open_files_limit, "soft": config.open_files_limit }],
      "capabilities": { "bounding": [], "effective": [], "inheritable": [], "permitted": [], "ambient": [] }
    },
    "root": { "path": "rootfs", "readonly": true },
    "hostname": hostname,
    "mounts": [
      { "destination": "/proc", "type": "proc", "source": "proc", "options": ["nosuid", "noexec", "nodev"] },
      { "destination": "/dev", "type": "tmpfs", "source": "tmpfs", "options": ["nosuid", "strictatime", "mode=755", "size=65536k"] },
      { "destination": "/dev/pts", "type": "devpts", "source": "devpts", "options": ["nosuid", "noexec", "newinstance", "ptmxmode=0666", "mode=0620", "gid=5"] },
      { "destination": "/dev/shm", "type": "tmpfs", "source": "shm", "options": ["nosuid", "noexec", "nodev", "mode=1777", "size=65536k"] },
      { "destination": "/dev/mqueue", "type": "mqueue", "source": "mqueue", "options": ["nosuid", "noexec", "nodev"] },
      { "destination": "/tmp", "type": "tmpfs", "source": "tmpfs", "options": ["nosuid", "nodev", "mode=1777", temporary_size] },
      { "destination": "/run", "type": "tmpfs", "source": "tmpfs", "options": ["nosuid", "noexec", "nodev", "mode=755", "size=16777216"] },
      { "destination": "/sys", "type": "sysfs", "source": "sysfs", "options": ["nosuid", "noexec", "nodev", "ro"] },
      { "destination": "/sys/fs/cgroup", "type": "cgroup", "source": "cgroup", "options": ["nosuid", "noexec", "nodev", "relatime", "ro"] },
      { "destination": GUEST_WORKSPACE, "type": "bind", "source": request.workspace, "options": ["rbind", "rw", "nosuid", "nodev"] },
      { "destination": GUEST_RELEASE, "type": "bind", "source": runner.release_root, "options": ["rbind", "ro", "nosuid", "nodev"] }
    ],
    "linux": {
      "cgroupsPath": format!("octacity/{id}"),
      "resources": {
        "cpu": { "quota": cpu_quota, "period": cpu_period },
        "memory": { "limit": request.memory_bytes },
        "pids": { "limit": request.factory.as_ref().map_or(config.pids_limit, |factory| factory.process_limit) },
        "devices": [
          { "allow": false, "access": "rwm" },
          { "allow": true, "type": "c", "major": 1, "minor": 3, "access": "rwm" },
          { "allow": true, "type": "c", "major": 1, "minor": 5, "access": "rwm" },
          { "allow": true, "type": "c", "major": 1, "minor": 7, "access": "rwm" },
          { "allow": true, "type": "c", "major": 1, "minor": 8, "access": "rwm" },
          { "allow": true, "type": "c", "major": 1, "minor": 9, "access": "rwm" },
          { "allow": true, "type": "c", "major": 5, "minor": 0, "access": "rwm" },
          { "allow": true, "type": "c", "major": 5, "minor": 1, "access": "rwm" },
          { "allow": true, "type": "c", "major": 5, "minor": 2, "access": "rwm" }
        ]
      },
      "namespaces": [
        { "type": "pid" }, { "type": "network" }, { "type": "ipc" }, { "type": "uts" }, { "type": "mount" }
      ],
      "maskedPaths": ["/proc/acpi", "/proc/asound", "/proc/kcore", "/proc/keys", "/proc/latency_stats", "/proc/timer_list", "/proc/timer_stats", "/proc/sched_debug", "/proc/scsi", "/sys/firmware"],
      "readonlyPaths": ["/proc/bus", "/proc/fs", "/proc/irq", "/proc/sys", "/proc/sysrq-trigger"],
      "seccomp": {
        "defaultAction": "SCMP_ACT_ALLOW",
        "syscalls": [{
          "names": ["bpf", "delete_module", "finit_module", "init_module", "kexec_file_load", "kexec_load", "keyctl", "lookup_dcookie", "mount", "open_by_handle_at", "perf_event_open", "process_vm_readv", "process_vm_writev", "ptrace", "reboot", "setns", "swapoff", "swapon", "umount2", "unshare", "userfaultfd"],
          "action": "SCMP_ACT_ERRNO",
          "errnoRet": 1
        }]
      }
    },
    "annotations": { "com.octacity.security-profile": SECURITY_PROFILE_VERSION }
  });
  let mut external_environment = Vec::with_capacity(runner.external_executables.len());
  let mut external_mounts = Vec::with_capacity(runner.external_executables.len());
  for projection in runner.external_executable_projections() {
    let destination = projection
      .destination(Path::new(GUEST_TOOLS))
      .to_string_lossy()
      .into_owned();
    external_environment.push(serde_json::Value::String(format!(
      "{}={destination}",
      projection.selector
    )));
    external_mounts.push(serde_json::json!({
      "destination": destination,
      "type": "bind",
      "source": projection.source,
      "options": ["bind", "ro", "nosuid", "nodev"]
    }));
  }
  spec["process"]["env"]
    .as_array_mut()
    .expect("OCI process environment is an array")
    .extend(external_environment);
  spec["mounts"]
    .as_array_mut()
    .expect("OCI mounts are an array")
    .extend(external_mounts);
  if let Some(factory) = &request.factory {
    if factory.process_limit > config.pids_limit {
      return Err(unavailable(
        "Factory process limit exceeds the containerd backend ceiling",
      ));
    }
    let mounts = spec["mounts"]
      .as_array_mut()
      .expect("the static OCI specification always contains a mounts array");
    mounts.retain(|mount| mount["destination"] != GUEST_WORKSPACE);
    for (destination, source, readonly) in [
      (FACTORY_SOURCE_ROOT, &factory.source, factory.source_read_only),
      (FACTORY_SCRATCH_ROOT, &factory.scratch, false),
      (FACTORY_OUTPUT_ROOT, &factory.output, false),
      (FACTORY_PROTECTED_INPUT_ROOT, &factory.protected_inputs, true),
    ] {
      let mut options = vec!["rbind", "rw", "nosuid", "nodev"];
      if readonly {
        options[1] = "ro";
        options.push("noexec");
      }
      mounts.push(serde_json::json!({
        "destination": destination,
        "type": "bind",
        "source": source,
        "options": options,
      }));
    }
  }
  if let Some(identity) = &request.workload_identity {
    spec["mounts"]
      .as_array_mut()
      .expect("the static OCI specification always contains a mounts array")
      .push(serde_json::json!({
        "destination": WORKLOAD_IDENTITY_PATH,
        "type": "bind",
        "source": identity,
        "options": ["bind", "ro", "nosuid", "nodev", "noexec"]
      }));
  }
  if let Some(cache) = &request.cache {
    push_bind_mount(&mut spec, CACHE_DIRECTORY_PATH, &cache.local_directory, false);
    if let Some(token) = &cache.token_file {
      push_bind_mount(&mut spec, CACHE_TOKEN_PATH, token, true);
    }
    if let Some(certificate) = &cache.ca_certificate_file {
      push_bind_mount(&mut spec, CACHE_CA_CERTIFICATE_PATH, certificate, true);
    }
  }
  Ok(spec)
}

/// Keeps OCI hostnames within Linux's 64-byte UTS limit without shortening resource IDs.
fn oci_hostname(id: &str) -> String {
  if id.len() <= LINUX_UTS_HOSTNAME_MAX_BYTES {
    id.to_owned()
  } else {
    let digest = hex::encode(Sha256::digest(id.as_bytes()));
    format!("octacity-{}", &digest[..32])
  }
}

fn push_bind_mount(spec: &mut serde_json::Value, destination: &str, source: &Path, readonly: bool) {
  let mut options = vec!["bind", "rw", "nosuid", "nodev", "noexec"];
  if readonly {
    options[1] = "ro";
  }
  spec["mounts"]
    .as_array_mut()
    .expect("the static OCI specification always contains a mounts array")
    .push(serde_json::json!({ "destination": destination, "type": "bind", "source": source, "options": options }));
}
