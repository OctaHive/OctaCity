//! Builds bounded host-to-guest paths and the immutable sandbox plan.

use super::*;

pub(super) fn canonical_runtime_file(name: &str, path: &Path) -> Result<PathBuf, ExecutionError> {
  if !path.is_absolute() || !path.is_file() {
    return Err(invalid(format!("{name} must be an existing absolute regular file")));
  }
  path
    .canonicalize()
    .map_err(|error| backend(format!("canonicalize {name}: {error}")))
}

pub(super) struct SandboxPlan {
  /// Stable, bounded name derived from agent and execution identities.
  pub(super) name: String,
  /// Digest-pinned OCI image reference selected by the signed job.
  pub(super) image: String,
  /// Whole-vCPU limit accepted by the Microsandbox SDK.
  pub(super) cpus: u8,
  /// Guest memory limit in whole MiB.
  pub(super) memory_mib: u32,
  /// RAM-backed capacity for writes outside the mounted job root.
  pub(super) root_tmpfs_mib: u32,
  /// Canonical job-private host root mounted into the guest.
  pub(super) host_job_root: PathBuf,
  /// Remaining disk allowance after accounting for existing job files.
  pub(super) job_root_quota_mib: u32,
  /// Fixed job-private mount point visible inside the guest.
  pub(super) guest_job_root: String,
  /// Workspace path below the guest job root.
  pub(super) guest_workspace: String,
  /// Fixed read-only Octa release mount point inside the guest.
  pub(super) guest_release: String,
  /// Runner executable path translated into the guest release mount.
  pub(super) guest_executable: String,
  /// Runner data directory translated into the guest workspace mount.
  pub(super) guest_data_dir: PathBuf,
  /// Plugin directory translated into the guest release mount.
  pub(super) guest_plugins_dir: PathBuf,
  /// Plugin lock path translated into the guest release mount.
  pub(super) guest_plugin_lock: PathBuf,
  /// Selector, canonical host path, and fixed guest path for each trusted tool.
  pub(super) external_executables: Vec<(String, PathBuf, String)>,
  /// Optional job-private identity source exposed read-only in the guest.
  pub(super) workload_identity: Option<PathBuf>,
  /// Job-private credential directories hidden behind empty read-only mounts.
  pub(super) masked_job_directories: Vec<String>,
  /// Canonical cache mount plus the remaining VM-enforced growth quota.
  pub(super) cache: Option<SandboxCachePlan>,
}

/// Persistent cache projection with a Microsandbox-enforced byte boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SandboxCachePlan {
  /// Canonical host paths and semantic capacity.
  pub(super) mounts: ExecutionCacheMounts,
  /// Additional MiB the guest may allocate in the persistent scope.
  pub(super) quota_mib: u32,
}

impl SandboxPlan {
  /// Validates backend constraints and computes all guest-visible resources
  /// before creating any microVM state.
  pub(super) async fn build(
    agent_id: &str,
    runner: &RunnerProgram,
    request: &StartExecution,
  ) -> Result<Self, ExecutionError> {
    runner.validate()?;
    request.validate()?;
    let ExecutionTarget::Oci { reference, .. } = &request.root else {
      return Err(unavailable("Microsandbox execution requires an OCI root image"));
    };
    let cpus =
      u8::try_from(request.cpu_millis / 1000).map_err(|_| unavailable("Microsandbox CPU limit exceeds 255 vCPUs"))?;
    if cpus == 0 || !request.cpu_millis.is_multiple_of(1000) {
      return Err(unavailable("Microsandbox CPU limits must use whole vCPUs"));
    }
    let workspace_root = request.workspace_root.canonicalize().map_err(ExecutionError::Io)?;
    let workspace = request.workspace.canonicalize().map_err(ExecutionError::Io)?;
    let job_root = workspace
      .parent()
      .filter(|parent| parent.parent() == Some(workspace_root.as_path()))
      .ok_or_else(|| invalid("Microsandbox workspace must be inside one job-private root below work_root"))?
      .to_owned();
    if workspace.file_name() != Some(std::ffi::OsStr::new("workspace")) {
      return Err(invalid("Microsandbox job-private workspace must be named 'workspace'"));
    }
    let memory_mib = exact_mib("memory", request.memory_bytes)?;
    // Keep incidental writes outside /work off disk without allowing the
    // root overlay to consume the VM's entire memory allocation.
    let root_tmpfs_mib = (memory_mib / 2).max(1);
    let job_root_limit_mib = exact_mib("writable disk", request.writable_disk_bytes)?;
    let existing_bytes = directory_size_async(job_root.clone()).await?;
    if existing_bytes > request.writable_disk_bytes {
      return Err(unavailable(
        "materialized job root already exceeds its writable disk limit",
      ));
    }
    let existing_mib = existing_bytes.div_ceil(MEBIBYTE);
    let job_root_quota_mib = u32::try_from(u64::from(job_root_limit_mib).saturating_sub(existing_mib))
      .map_err(|_| unavailable("Microsandbox job-root quota is not representable"))?;
    let cache = match &request.cache {
      Some(cache) => {
        let limit_mib = exact_mib("cache", cache.local_capacity.max_bytes)?;
        let existing_bytes = directory_size_async(cache.local_directory.clone()).await?;
        if existing_bytes > cache.local_capacity.max_bytes {
          return Err(unavailable(
            "persistent cache scope already exceeds its configured capacity",
          ));
        }
        let existing_mib = existing_bytes.div_ceil(MEBIBYTE);
        let quota_mib = u32::try_from(u64::from(limit_mib).saturating_sub(existing_mib))
          .map_err(|_| unavailable("Microsandbox cache quota is not representable"))?;
        Some(SandboxCachePlan {
          mounts: cache.clone(),
          quota_mib,
        })
      }
      None => None,
    };

    let guest_release = "/opt/octacity/octa".to_owned();
    let external_executables = runner
      .external_executable_projections()
      .into_iter()
      .map(|projection| {
        let destination = projection
          .destination(Path::new("/opt/octacity/tools"))
          .to_string_lossy()
          .into_owned();
        (projection.selector, projection.source, destination)
      })
      .collect();
    let guest_job_root = "/work".to_owned();
    let guest_workspace = guest_path(&guest_job_root, &job_root, &workspace)?;
    let private_files = request
      .workload_identity
      .iter()
      .chain(request.cache.iter().filter_map(|cache| cache.token_file.as_ref()));
    let mut masked_job_directories = Vec::new();
    for private_file in private_files {
      if !private_file.starts_with(&job_root) {
        continue;
      }
      let directory = private_file
        .parent()
        .ok_or_else(|| invalid("job-private credential has no parent directory"))?;
      if directory == job_root || directory.starts_with(&workspace) {
        return Err(invalid(
          "job-private credentials must use a dedicated directory outside workspace",
        ));
      }
      masked_job_directories.push(guest_path(&guest_job_root, &job_root, directory)?);
    }
    masked_job_directories.sort();
    masked_job_directories.dedup();
    Ok(Self {
      name: sandbox_name(agent_id, &request.execution_id),
      image: reference.clone(),
      cpus,
      memory_mib,
      root_tmpfs_mib,
      host_job_root: job_root.clone(),
      job_root_quota_mib,
      guest_job_root: guest_job_root.clone(),
      guest_executable: guest_path(&guest_release, &runner.release_root, &runner.executable)?,
      guest_data_dir: guest_path_buf(&guest_job_root, &job_root, &request.data_dir)?,
      guest_plugins_dir: guest_path_buf(&guest_release, &runner.release_root, &runner.plugins_dir)?,
      guest_plugin_lock: guest_path_buf(&guest_release, &runner.release_root, &runner.plugin_lock)?,
      external_executables,
      workload_identity: request.workload_identity.clone(),
      masked_job_directories,
      cache,
      guest_workspace,
      guest_release,
    })
  }
}

pub(super) fn guest_path(guest_root: &str, host_root: &Path, host_path: &Path) -> Result<String, ExecutionError> {
  let relative = host_path
    .strip_prefix(host_root)
    .map_err(|_| invalid("mapped path is outside its host root"))?;
  let relative = relative
    .to_str()
    .ok_or_else(|| invalid("mapped host path is not UTF-8"))?
    .replace('\\', "/");
  Ok(if relative.is_empty() {
    guest_root.to_owned()
  } else {
    format!("{guest_root}/{relative}")
  })
}

pub(super) fn guest_path_buf(guest_root: &str, host_root: &Path, host_path: &Path) -> Result<PathBuf, ExecutionError> {
  guest_path(guest_root, host_root, host_path).map(PathBuf::from)
}

pub(super) fn exact_mib(name: &str, bytes: u64) -> Result<u32, ExecutionError> {
  if !bytes.is_multiple_of(MEBIBYTE) {
    return Err(unavailable(format!("Microsandbox {name} limit must use whole MiB")));
  }
  u32::try_from(bytes / MEBIBYTE).map_err(|_| unavailable(format!("Microsandbox {name} limit is too large")))
}

pub(super) fn sandbox_name(agent_id: &str, execution_id: &str) -> String {
  let mut digest = Sha256::new();
  digest.update(agent_id.as_bytes());
  digest.update([0]);
  digest.update(execution_id.as_bytes());
  format!("octacity-{}", hex::encode(digest.finalize()))
}

/// Measures regular files without following symlinks outside the workspace.
pub(super) fn directory_size(root: &Path) -> Result<u64, ExecutionError> {
  let mut total = 0_u64;
  let mut pending = vec![root.to_owned()];
  while let Some(path) = pending.pop() {
    let metadata = match fs::symlink_metadata(&path) {
      Ok(metadata) => metadata,
      Err(error) if error.kind() == std::io::ErrorKind::NotFound && path != root => continue,
      Err(error) => return Err(backend(format!("inspect workspace '{}': {error}", path.display()))),
    };
    if metadata.file_type().is_symlink() {
      continue;
    }
    if metadata.is_file() {
      total = total
        .checked_add(metadata.len())
        .ok_or_else(|| backend("workspace size overflow"))?;
      continue;
    }
    if !metadata.is_dir() {
      return Err(backend(format!(
        "workspace contains unsupported special file '{}'",
        path.display()
      )));
    }
    for entry in
      fs::read_dir(&path).map_err(|error| backend(format!("read workspace '{}': {error}", path.display())))?
    {
      pending.push(
        entry
          .map_err(|error| backend(format!("read workspace entry: {error}")))?
          .path(),
      );
    }
  }
  Ok(total)
}

pub(super) async fn directory_size_async(root: PathBuf) -> Result<u64, ExecutionError> {
  tokio::task::spawn_blocking(move || directory_size(&root))
    .await
    .map_err(|error| backend(format!("workspace accounting task failed: {error}")))?
}
