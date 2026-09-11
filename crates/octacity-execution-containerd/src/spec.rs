//! Builds the fixed OCI runtime specification and guest path mapping.

use super::*;

/// Maps verified host paths into fixed paths inside the OCI root filesystem.
pub(super) fn guest_paths(runner: &RunnerProgram, request: &StartExecution) -> Result<ExecutionPaths, ExecutionError> {
  Ok(ExecutionPaths {
    workspace: PathBuf::from(GUEST_WORKSPACE),
    data_dir: map_path(&request.workspace, &request.data_dir, Path::new(GUEST_WORKSPACE))?,
    plugins_dir: map_path(&runner.release_root, &runner.plugins_dir, Path::new(GUEST_RELEASE))?,
    plugin_lock: map_path(&runner.release_root, &runner.plugin_lock, Path::new(GUEST_RELEASE))?,
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
  let cpu_period = 100_000_u64;
  let cpu_quota = u64::from(request.cpu_millis)
    .checked_mul(cpu_period)
    .ok_or_else(|| invalid("CPU limit overflows the OCI quota representation"))?
    / 1000;
  let temporary_bytes = (request.memory_bytes / 8).min(1024 * 1024 * 1024);
  let temporary_size = format!("size={temporary_bytes}");
  Ok(serde_json::json!({
    "ociVersion": "1.1.0",
    "process": {
      "terminal": false,
      "user": { "uid": 0, "gid": 0 },
      "args": [executable],
      "env": image_environment,
      "cwd": paths.workspace,
      "noNewPrivileges": true,
      "rlimits": [{ "type": "RLIMIT_NOFILE", "hard": config.open_files_limit, "soft": config.open_files_limit }],
      "capabilities": { "bounding": [], "effective": [], "inheritable": [], "permitted": [], "ambient": [] }
    },
    "root": { "path": "rootfs", "readonly": true },
    "hostname": id,
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
        "pids": { "limit": config.pids_limit },
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
  }))
}
