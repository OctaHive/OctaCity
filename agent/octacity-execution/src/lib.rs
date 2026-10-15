//! Backend-neutral contract for starting and supervising one job execution.
//!
//! The runner supervisor depends only on these lifecycle and accounting
//! operations. Native and sandboxed backends therefore expose identical I/O,
//! cancellation, cleanup, and resource-usage semantics.
//!
//! This crate is the execution-backend interface used by the Agent-side
//! Executor; a backend is not itself the Executor, Orchestrator, or Placement
//! Scheduler. See the [canonical glossary].
//!
//! [canonical glossary]: https://github.com/OctaHive/OctaCity/blob/main/CONTEXT.md

#![warn(missing_docs)]

mod factory_execution_partition;
pub use factory_execution_partition::FactoryExecutionPartition;

mod factory_tool_channel;
pub use factory_tool_channel::FactoryToolChannel;

use std::{
  collections::BTreeMap,
  path::{Component, Path, PathBuf},
  pin::Pin,
  time::Duration,
};

use async_trait::async_trait;
pub use octa_cache_protocol::LocalCacheCapacity;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::sync::CancellationToken;

pub use octacity_protocol::{
  FACTORY_OUTPUT_ROOT, FACTORY_SCRATCH_ROOT, FACTORY_SOURCE_ROOT, PROTECTED_INPUT_ROOT as FACTORY_PROTECTED_INPUT_ROOT,
};

/// Owned asynchronous stdout or stderr stream from an execution backend.
pub type ExecutionReader = Pin<Box<dyn AsyncRead + Send>>;
/// Owned asynchronous stdin stream into an execution backend.
pub type ExecutionWriter = Pin<Box<dyn AsyncWrite + Send>>;

/// Stable read-only path at which every backend exposes job identity.
pub const WORKLOAD_IDENTITY_PATH: &str = "/run/octa-identity";
/// Stable writable path at which isolated runners observe their local L1.
pub const CACHE_DIRECTORY_PATH: &str = "/var/cache/octa";
/// Stable read-only path at which isolated runners read a cache bearer.
pub const CACHE_TOKEN_PATH: &str = "/run/octa-cache/token";
/// Stable read-only path for an optional private cache CA certificate.
pub const CACHE_CA_CERTIFICATE_PATH: &str = "/run/octa-cache/ca.pem";
/// Stable filename prefix for operator-selected executables projected by isolated backends.
pub const EXTERNAL_EXECUTABLE_FILE_PREFIX: &str = "tool-";

/// One deterministic projection of an operator-selected executable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalExecutableProjection {
  /// Environment selector declared by the release contract.
  pub selector: String,
  /// Verified absolute path on the Agent host.
  pub source: PathBuf,
  /// Opaque filename used below a backend-owned read-only tools root.
  pub file_name: String,
}

impl ExternalExecutableProjection {
  /// Returns the projected path below a backend-owned tools root.
  pub fn destination(&self, tools_root: &Path) -> PathBuf {
    tools_root.join(&self.file_name)
  }
}

/// Canonical host paths that a backend must project into an execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionCacheMounts {
  /// Dedicated host filesystem containing every persistent cache scope.
  pub capacity_root: PathBuf,
  /// Persistent writable L1 directory outside the job workspace.
  pub local_directory: PathBuf,
  /// Per-scope capacity enforced by the selected execution backend.
  pub local_capacity: LocalCacheCapacity,
  /// Maximum physical size of the complete dedicated cache filesystem.
  pub aggregate_max_bytes: u64,
  /// Optional private bearer file for remote L2 access.
  pub token_file: Option<PathBuf>,
  /// Optional operator-owned public CA certificate.
  pub ca_certificate_file: Option<PathBuf>,
}

impl ExecutionCacheMounts {
  /// Produces the fixed paths visible to an isolated Octa runner.
  ///
  /// Every current backend uses the same guest contract; keeping the mapping
  /// here prevents a new credential or cache path from drifting by backend.
  pub fn projected_paths(&self) -> ExecutionCachePaths {
    ExecutionCachePaths {
      local_directory: PathBuf::from(CACHE_DIRECTORY_PATH),
      token_file: self.token_file.as_ref().map(|_| PathBuf::from(CACHE_TOKEN_PATH)),
      ca_certificate_file: self
        .ca_certificate_file
        .as_ref()
        .map(|_| PathBuf::from(CACHE_CA_CERTIFICATE_PATH)),
    }
  }

  /// Returns the canonical paths used by a runner executing directly on the host.
  pub fn host_paths(&self) -> ExecutionCachePaths {
    ExecutionCachePaths {
      local_directory: self.local_directory.clone(),
      token_file: self.token_file.clone(),
      ca_certificate_file: self.ca_certificate_file.clone(),
    }
  }
}

/// Protocol streams connected to the isolated `octa-runner` process.
pub struct ExecutionIo {
  /// Control input consumed by `octa-runner`.
  pub stdin: ExecutionWriter,
  /// Protocol output produced by `octa-runner`.
  pub stdout: ExecutionReader,
  /// Bounded diagnostic output kept separate from the protocol.
  pub stderr: ExecutionReader,
}

/// Paired real task-plugin paths, either both present or both absent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionPluginProjection {
  /// Locked plugin executable directory.
  pub directory: PathBuf,
  /// Exact lock file authorizing that plugin set.
  pub lock: PathBuf,
}

/// Trusted executable distribution selected by installation inventory.
/// Constructing this path value does not establish binary qualification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionProgram {
  /// Canonical root containing the installed executable distribution.
  pub release_root: PathBuf,
  /// Canonical installed executable inside `release_root`.
  pub executable: PathBuf,
  /// Actual task-plugin distribution when the program uses the Octa protocol.
  /// Standalone helper programs have no fabricated ancillary files.
  pub plugin_projection: Option<ExecutionPluginProjection>,
  /// Operator-selected executables exposed only through release-declared
  /// environment selectors. Backends project these paths read-only.
  pub external_executables: BTreeMap<String, PathBuf>,
}

/// Compatibility name for existing Octa-runner callers.
pub type RunnerProgram = ExecutionProgram;

impl ExecutionProgram {
  /// Selects an installed standalone entrypoint without an Octa distribution.
  pub fn standalone(release_root: PathBuf, executable: PathBuf) -> Result<Self, ExecutionError> {
    let program = Self {
      release_root,
      executable,
      plugin_projection: None,
      external_executables: BTreeMap::new(),
    };
    program.validate()?;
    Ok(program)
  }

  /// Refuses a standalone entrypoint in a backend qualified only for Octa.
  /// Call this before inspecting job files or creating execution resources.
  pub fn require_plugin_projection(&self) -> Result<&ExecutionPluginProjection, ExecutionError> {
    self
      .plugin_projection
      .as_ref()
      .ok_or_else(|| ExecutionError::Unavailable("backend has no qualified standalone program contract".to_owned()))
  }

  /// Projects trusted executables in selector order without exposing product names.
  pub fn external_executable_projections(&self) -> Vec<ExternalExecutableProjection> {
    self
      .external_executables
      .iter()
      .enumerate()
      .map(|(index, (selector, source))| ExternalExecutableProjection {
        selector: selector.clone(),
        source: source.clone(),
        file_name: format!("{EXTERNAL_EXECUTABLE_FILE_PREFIX}{index}"),
      })
      .collect()
  }

  /// Checks that every distribution path is absolute and confined to its root.
  pub fn validate(&self) -> Result<(), ExecutionError> {
    if !self.release_root.is_absolute()
      || !self.executable.is_absolute()
      || !self.executable.starts_with(&self.release_root)
      || self.plugin_projection.as_ref().is_some_and(|projection| {
        !projection.directory.is_absolute()
          || !projection.lock.is_absolute()
          || !projection.directory.starts_with(&self.release_root)
          || !projection.lock.starts_with(&self.release_root)
      })
      || self.external_executables.iter().any(|(selector, executable)| {
        selector.is_empty()
          || selector.trim() != selector
          || selector.contains('=')
          || selector.chars().any(char::is_control)
          || !executable.is_absolute()
      })
    {
      return Err(ExecutionError::Invalid(
        "runner paths and external executable selectors are invalid".to_owned(),
      ));
    }
    Ok(())
  }
}

/// Root filesystem selected for an execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutionTarget {
  /// Run directly on an agent host without claiming an isolation boundary.
  Host {
    /// Exact host platform that must also be observed by the runner.
    platform: ExecutionPlatform,
  },
  /// Run directly on an agent host matching the signed platform.
  Native {
    /// Host platform that must execute the runner directly.
    platform: ExecutionPlatform,
  },
  /// Run an immutable OCI image for the exact guest platform and isolation tier.
  Oci {
    /// Immutable repository and digest selected by the signed job.
    reference: String,
    /// Required guest platform.
    platform: ExecutionPlatform,
    /// Minimum isolation boundary the engine must provide.
    isolation: OciIsolation,
  },
}

/// Operating systems understood by execution backends.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ExecutionOs {
  /// Linux host or guest.
  Linux,
  /// Windows host or guest.
  Windows,
  /// macOS host.
  Macos,
}

/// CPU architectures understood by execution backends.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ExecutionArchitecture {
  /// 64-bit x86 (`amd64`).
  Amd64,
  /// 64-bit ARM (`arm64`).
  Arm64,
}

/// Exact OS and architecture required by an execution.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ExecutionPlatform {
  /// Required operating system.
  pub os: ExecutionOs,
  /// Required CPU architecture.
  pub architecture: ExecutionArchitecture,
}

/// Isolation tier required for an OCI execution.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum OciIsolation {
  /// Linux namespaces and cgroups in the agent kernel.
  Process,
  /// A separate guest kernel managed by a hypervisor-backed engine.
  Hypervisor,
}

/// Network access that the selected backend must enforce exactly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NetworkAccess {
  /// Preserve backend network access without filtering.
  Unrestricted,
  /// Create the execution without external network access.
  Disabled,
  /// Allow only the explicitly listed hosts.
  Restricted {
    /// Non-empty host names authorized by the signed job.
    allowed_hosts: Vec<String>,
  },
}

/// Filesystem consumption charged to one execution's writable-disk ceiling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionDiskAccounting {
  /// Legacy whole Job directory accounting, including staged immutable inputs.
  WholeJob,
  /// Only this execution's physically projected writable mounts.
  /// A validated Factory partition uses this scope for both participants.
  OwnedWritableMounts,
}

/// Backend-neutral host layout for one admitted Factory execution.
///
/// Authorization and portable-path policy are decided before this value is
/// constructed. Backends only project these already verified roots at the
/// canonical paths above and enforce the effective process-tree ceiling.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryExecutionLayout {
  /// Selects an empty read-only parent as the runner workspace. Only separately
  /// admitted source, scratch, and output mounts appear below it; the host Job
  /// directory is never projected. Unsupported backends refuse this request.
  pub workspace_anchor: bool,
  /// Trusted technical scope for this execution's writable-disk accounting.
  pub disk_accounting: ExecutionDiskAccounting,
  /// Immutable host directory projected at [`FACTORY_PROTECTED_INPUT_ROOT`].
  pub protected_inputs: PathBuf,
  /// Exact private host checkout, projected at [`FACTORY_SOURCE_ROOT`] only when visible.
  pub source: PathBuf,
  /// Whether the workload receives any source mount; false keeps host source private.
  pub source_visible: bool,
  /// Whether the base source projection rejects writes outside explicit overlays.
  pub source_read_only: bool,
  /// Canonical directory overlays writable within an otherwise read-only source.
  /// Empty means no exception. Paths must be disjoint strict descendants; they
  /// narrow a signed capture policy, never grant authority beyond admission.
  pub source_writable_paths: Vec<PathBuf>,
  /// Optional trusted controlled-read stream. Its endpoint is outside the
  /// complete Job tree and its source stays outside the workload namespace.
  /// Unsupported backends must refuse this projection.
  pub tool_channel: Option<FactoryToolChannel>,
  /// Writable disposable state projected at [`FACTORY_SCRATCH_ROOT`].
  pub scratch: PathBuf,
  /// Writable declared-output root projected at [`FACTORY_OUTPUT_ROOT`].
  pub output: PathBuf,
  /// Maximum processes in the complete runner process tree, including the runner.
  /// Linux's process counter also charges threads against this ceiling.
  pub process_limit: u32,
}

impl FactoryExecutionLayout {
  fn validate(&self, workspace_root: &Path, workspace: &Path, data_dir: &Path) -> Result<(), ExecutionError> {
    if self.process_limit == 0 {
      return Err(ExecutionError::Invalid(
        "Factory process limit must be greater than zero".to_owned(),
      ));
    }
    let canonical_work_root = canonical_directory(workspace_root, "workspace_root")?;
    let source = canonical_directory(&self.source, "Factory source")?;
    if self.source_writable_paths.len() > octacity_protocol::MAX_FACTORY_PERMISSION_ENTRIES
      || !self.source_read_only && !self.source_writable_paths.is_empty()
      || !self.source_visible && (!self.source_read_only || !self.source_writable_paths.is_empty())
    {
      return Err(ExecutionError::Invalid(
        "source write overlays exceed the signed count ceiling or require a read-only source".to_owned(),
      ));
    }
    for (index, path) in self.source_writable_paths.iter().enumerate() {
      let relative = path
        .strip_prefix(&source)
        .map_err(|_| ExecutionError::Invalid("source write path escapes source".to_owned()))?;
      if relative.as_os_str().is_empty()
        || relative
          .components()
          .any(|component| !matches!(component, Component::Normal(_)))
        || self
          .source_writable_paths
          .iter()
          .take(index)
          .any(|other| path.starts_with(other) || other.starts_with(path))
      {
        return Err(ExecutionError::Invalid(
          "source write paths must be disjoint strict descendants".to_owned(),
        ));
      }
      let mut resolved = source.clone();
      for component in relative.components() {
        resolved.push(component);
        let metadata = std::fs::symlink_metadata(&resolved).map_err(ExecutionError::Io)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
          return Err(ExecutionError::Invalid(
            "source write path must contain only directories".to_owned(),
          ));
        }
        #[cfg(windows)]
        if octacity_private_fs::is_reparse_point(&resolved).map_err(ExecutionError::Io)? {
          return Err(ExecutionError::Invalid(
            "source write path contains a reparse point".to_owned(),
          ));
        }
      }
    }
    let scratch = canonical_directory(&self.scratch, "Factory scratch")?;
    let output = canonical_directory(&self.output, "Factory output")?;
    let protected = canonical_directory(&self.protected_inputs, "Factory protected inputs")?;
    let Some(job_root) = source.parent() else {
      return Err(ExecutionError::Invalid("Factory source has no job root".to_owned()));
    };
    let roots = [
      source.as_path(),
      scratch.as_path(),
      output.as_path(),
      protected.as_path(),
    ];
    if let Some(channel) = &self.tool_channel {
      channel.validate()?;
      if self.source_visible || channel.host_socket().starts_with(job_root) {
        return Err(ExecutionError::Invalid(
          "controlled reads require a hidden source and private endpoint outside the Job tree".to_owned(),
        ));
      }
    }
    if job_root.parent() != Some(canonical_work_root.as_path())
      || workspace != source
      || scratch.parent() != Some(job_root)
      || output.parent() != Some(job_root)
      || protected.parent() != Some(job_root)
      || data_dir.strip_prefix(&scratch).is_err()
      || roots
        .iter()
        .enumerate()
        .any(|(index, path)| roots.iter().skip(index + 1).any(|other| path == other))
    {
      return Err(ExecutionError::Invalid(
        "Factory source, scratch, output, and protected roots must be distinct siblings below one job root".to_owned(),
      ));
    }
    if !std::fs::symlink_metadata(&protected)
      .map_err(ExecutionError::Io)?
      .permissions()
      .readonly()
    {
      return Err(ExecutionError::Invalid(
        "Factory protected inputs must be read-only before backend projection".to_owned(),
      ));
    }
    Ok(())
  }
}

/// Resources and filesystem locations needed to start a job.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartExecution {
  /// Unique, log-safe identity used to name owned backend resources.
  pub execution_id: String,
  /// Agent-owned filesystem root containing this execution's workspace.
  pub workspace_root: PathBuf,
  /// Existing workspace directory visible to the runner.
  pub workspace: PathBuf,
  /// Existing Octa state directory contained by `workspace`.
  pub data_dir: PathBuf,
  /// Optional job-private identity file mounted read-only at
  /// [`WORKLOAD_IDENTITY_PATH`].
  pub workload_identity: Option<PathBuf>,
  /// Optional cache paths mounted with backend-appropriate permissions.
  pub cache: Option<ExecutionCacheMounts>,
  /// Optional admitted Factory layout. Absence preserves the ordinary CI/CD layout.
  pub factory: Option<FactoryExecutionLayout>,
  /// CPU allocation in thousandths of one logical CPU.
  pub cpu_millis: u32,
  /// Maximum addressable memory in bytes.
  pub memory_bytes: u64,
  /// Maximum writable workspace filesystem capacity in bytes.
  pub writable_disk_bytes: u64,
  /// Maximum duration for backend preparation and the running execution.
  pub max_duration: Duration,
  /// Native or OCI root selected for this execution.
  pub root: ExecutionTarget,
  /// Network policy the backend must enforce exactly.
  pub network: NetworkAccess,
}

impl StartExecution {
  /// Refuses a parent workspace projection in backends that do not qualify it.
  /// This runs before any Job inspection or resource creation.
  pub fn require_unanchored_factory_workspace(&self) -> Result<(), ExecutionError> {
    if self.factory.as_ref().is_some_and(|factory| factory.workspace_anchor) {
      return Err(ExecutionError::Unavailable(
        "backend does not provide an isolated Factory workspace anchor".to_owned(),
      ));
    }
    Ok(())
  }

  /// Refuses partition-specific disk semantics in a legacy backend before it
  /// inspects private Job files. This check grants no execution authority;
  /// normal request validation still follows it.
  pub fn require_whole_job_disk_accounting(&self) -> Result<(), ExecutionError> {
    if self
      .factory
      .as_ref()
      .is_some_and(|factory| factory.disk_accounting != ExecutionDiskAccounting::WholeJob)
    {
      return Err(ExecutionError::Unavailable(
        "backend does not support owned writable-mount disk accounting".to_owned(),
      ));
    }
    Ok(())
  }

  /// Validates backend-independent execution invariants.
  pub fn validate(&self) -> Result<(), ExecutionError> {
    if self.execution_id.is_empty() || self.execution_id.chars().any(char::is_control) {
      return Err(ExecutionError::Invalid(
        "execution_id must not be empty or contain control characters".to_owned(),
      ));
    }
    if !self.workspace_root.is_absolute() || !self.workspace_root.is_dir() {
      return Err(ExecutionError::Invalid(
        "workspace_root must be an existing absolute directory".to_owned(),
      ));
    }
    if !self.workspace.is_absolute() || !self.workspace.is_dir() {
      return Err(ExecutionError::Invalid(
        "workspace must be an existing absolute directory inside workspace_root".to_owned(),
      ));
    }
    if !self.data_dir.is_absolute() || !self.data_dir.is_dir() {
      return Err(ExecutionError::Invalid(
        "data_dir must be an existing absolute directory inside workspace".to_owned(),
      ));
    }
    let canonical_root = canonical_directory(&self.workspace_root, "workspace_root")?;
    let canonical_workspace = canonical_directory(&self.workspace, "workspace")?;
    let canonical_data = canonical_directory(&self.data_dir, "data_dir")?;
    if !canonical_workspace.starts_with(&canonical_root) {
      return Err(ExecutionError::Invalid(
        "workspace must resolve inside workspace_root".to_owned(),
      ));
    }
    if self.factory.is_none() && !canonical_data.starts_with(&canonical_workspace) {
      return Err(ExecutionError::Invalid(
        "data_dir must resolve inside workspace".to_owned(),
      ));
    }
    if let Some(identity) = &self.workload_identity {
      let metadata = std::fs::symlink_metadata(identity).map_err(|error| {
        ExecutionError::Invalid(format!("workload identity must be an existing regular file: {error}"))
      })?;
      if !identity.is_absolute()
        || !metadata.file_type().is_file()
        || !is_canonical_path(identity)
        || !identity.starts_with(&canonical_root)
        || identity.starts_with(&canonical_workspace)
      {
        return Err(ExecutionError::Invalid(
          "workload identity must be a canonical absolute regular file inside workspace_root and outside workspace"
            .to_owned(),
        ));
      }
    }
    if let Some(cache) = &self.cache {
      cache
        .local_capacity
        .validate()
        .map_err(|error| ExecutionError::Invalid(error.to_string()))?;
      let capacity_root = canonical_directory(&cache.capacity_root, "cache capacity_root")?;
      let local = canonical_directory(&cache.local_directory, "cache local_directory")?;
      if !local.starts_with(&capacity_root) {
        return Err(ExecutionError::Invalid(
          "cache local_directory must be below cache capacity_root".to_owned(),
        ));
      }
      if capacity_root.starts_with(&canonical_root) || local.starts_with(&canonical_root) {
        return Err(ExecutionError::Invalid(
          "cache capacity_root and local_directory must be outside workspace_root".to_owned(),
        ));
      }
      if cache.aggregate_max_bytes < cache.local_capacity.max_bytes {
        return Err(ExecutionError::Invalid(
          "cache aggregate capacity must cover one local scope".to_owned(),
        ));
      }
      for (name, path) in [
        ("cache token_file", cache.token_file.as_ref()),
        ("cache ca_certificate_file", cache.ca_certificate_file.as_ref()),
      ] {
        if let Some(path) = path {
          let metadata = std::fs::symlink_metadata(path)
            .map_err(|error| ExecutionError::Invalid(format!("{name} must be an existing regular file: {error}")))?;
          if !path.is_absolute() || !metadata.file_type().is_file() || !is_canonical_path(path) {
            return Err(ExecutionError::Invalid(format!(
              "{name} must be a canonical absolute regular file"
            )));
          }
        }
      }
    }
    if let Some(factory) = &self.factory {
      factory.validate(&canonical_root, &canonical_workspace, &canonical_data)?;
      if factory.tool_channel.is_some()
        && !matches!(
          self.root,
          ExecutionTarget::Oci {
            isolation: OciIsolation::Hypervisor,
            ..
          }
        )
      {
        return Err(ExecutionError::Unavailable(
          "private tool-action streams require an OCI hypervisor transport".to_owned(),
        ));
      }
    }
    if self.cpu_millis == 0 || self.memory_bytes == 0 || self.writable_disk_bytes == 0 || self.max_duration.is_zero() {
      return Err(ExecutionError::Invalid(
        "CPU, memory, and writable disk limits must be greater than zero".to_owned(),
      ));
    }
    if let ExecutionTarget::Oci { reference, .. } = &self.root
      && !is_immutable_oci_reference(reference)
    {
      return Err(ExecutionError::Invalid(
        "OCI image must be an immutable repository@sha256:<digest> reference".to_owned(),
      ));
    }
    if matches!(&self.network, NetworkAccess::Restricted { allowed_hosts } if allowed_hosts.is_empty() || allowed_hosts.iter().any(|host| host.trim().is_empty()))
    {
      return Err(ExecutionError::Invalid(
        "restricted network access requires non-empty allowed_hosts".to_owned(),
      ));
    }
    Ok(())
  }
}

/// Requires a process-isolated cache to live on its own bounded filesystem.
///
/// Native and container runtimes expose a host bind mount directly to job
/// code, so Octa's cooperative GC cannot be the physical disk boundary. A
/// dedicated filesystem keeps arbitrary writes away from agent state. VM
/// adapters instead apply their own per-volume quota and need not call this.
#[cfg(unix)]
pub fn validate_process_cache_filesystem(cache: &ExecutionCacheMounts) -> Result<(), ExecutionError> {
  validate_process_cache_capacity_root(&cache.capacity_root, cache.aggregate_max_bytes)
}

/// Validates the persistent cache filesystem before process backends advertise
/// themselves as ready.
///
/// The composition root can call this once at startup, while each backend
/// repeats it against the concrete job projection before exposing the bind
/// mount. Hypervisor backends instead enforce a per-volume quota.
#[cfg(unix)]
pub fn validate_process_cache_capacity_root(
  capacity_root: &Path,
  aggregate_max_bytes: u64,
) -> Result<(), ExecutionError> {
  use std::{ffi::CString, mem::MaybeUninit, os::unix::ffi::OsStrExt as _, os::unix::fs::MetadataExt as _};

  let root = capacity_root.canonicalize().map_err(ExecutionError::Io)?;
  let parent = root
    .parent()
    .ok_or_else(|| ExecutionError::Unavailable("cache capacity_root has no parent directory".to_owned()))?;
  if std::fs::metadata(&root).map_err(ExecutionError::Io)?.dev()
    == std::fs::metadata(parent).map_err(ExecutionError::Io)?.dev()
  {
    return Err(ExecutionError::Unavailable(
      "process-isolated cache capacity_root must be a dedicated filesystem mount".to_owned(),
    ));
  }
  let path = CString::new(root.as_os_str().as_bytes())
    .map_err(|_| ExecutionError::Invalid("cache capacity_root contains a NUL byte".to_owned()))?;
  let mut stats = MaybeUninit::<libc::statvfs>::uninit();
  // SAFETY: `path` is NUL-terminated and `stats` points to writable memory.
  if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
    return Err(ExecutionError::Io(std::io::Error::last_os_error()));
  }
  // SAFETY: statvfs initialized `stats` after returning success.
  let stats = unsafe { stats.assume_init() };
  #[cfg(target_os = "macos")]
  let blocks = u64::from(stats.f_blocks);
  #[cfg(not(target_os = "macos"))]
  let blocks = stats.f_blocks;
  let capacity = blocks.saturating_mul(stats.f_frsize);
  if capacity > aggregate_max_bytes {
    return Err(ExecutionError::Unavailable(format!(
      "cache filesystem capacity {capacity} exceeds configured aggregate limit {aggregate_max_bytes}"
    )));
  }
  Ok(())
}

fn canonical_directory(path: &Path, name: &str) -> Result<PathBuf, ExecutionError> {
  std::fs::canonicalize(path).map_err(|error| ExecutionError::Invalid(format!("failed to resolve {name}: {error}")))
}

fn is_canonical_path(path: &Path) -> bool {
  // Windows may preserve `..` in the extended-length path returned by
  // `canonicalize`, so reject lexical aliases before consulting the
  // filesystem. The exact comparison then rejects symlink aliases and other
  // spellings that differ from the filesystem's canonical name.
  path.is_absolute()
    && !path
      .components()
      .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    && std::fs::canonicalize(path).is_ok_and(|canonical| canonical.as_os_str() == path.as_os_str())
}

fn is_immutable_oci_reference(value: &str) -> bool {
  let Some((repository, digest)) = value.rsplit_once("@sha256:") else {
    return false;
  };
  !repository.is_empty()
    && !repository.contains('@')
    && !repository.contains("://")
    && !repository.chars().any(char::is_whitespace)
    && digest.len() == 64
    && digest
      .bytes()
      .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Paths as seen by the installed program inside the selected backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionPaths {
  /// Workspace path as observed by the runner.
  pub workspace: PathBuf,
  /// Octa state path as observed by the runner.
  pub data_dir: PathBuf,
  /// Actual task-plugin distribution as observed inside this execution.
  /// A standalone program has no plugin projection.
  pub plugin_projection: Option<ExecutionPluginProjection>,
  /// Cache paths after backend-specific host-to-guest translation.
  pub cache: Option<ExecutionCachePaths>,
}

/// Cache paths as observed by the runner inside the selected backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionCachePaths {
  /// Writable local L1 directory.
  pub local_directory: PathBuf,
  /// Optional read-only bearer file.
  pub token_file: Option<PathBuf>,
  /// Optional read-only private CA certificate.
  pub ca_certificate_file: Option<PathBuf>,
}

/// Monotonic resource-accounting snapshot for a running job.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResourceUsage {
  /// Monotonic elapsed execution time in milliseconds.
  pub elapsed_ms: u64,
  /// Cumulative CPU time used by the complete execution tree.
  pub cpu_time_ms: u64,
  /// Current accounted memory in bytes.
  pub memory_current_bytes: u64,
  /// Peak accounted memory in bytes.
  pub memory_peak_bytes: u64,
  /// Current writable filesystem consumption in bytes.
  pub disk_current_bytes: u64,
  /// Peak writable filesystem consumption observed by the backend.
  pub disk_peak_bytes: u64,
  /// Cumulative bytes read from accounted block devices.
  pub io_read_bytes: u64,
  /// Cumulative bytes written to accounted block devices.
  pub io_written_bytes: u64,
  /// Received network bytes, or `None` when the backend cannot account them.
  pub network_received_bytes: Option<u64>,
  /// Transmitted network bytes, or `None` when the backend cannot account them.
  pub network_transmitted_bytes: Option<u64>,
}

/// Process exit observed by the backend after the runner has stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionExit {
  /// Portable process exit code, or `None` when the backend cannot represent it.
  pub code: Option<i32>,
}

/// Failure reported by an execution backend or its lifecycle contract.
#[derive(Debug, Error)]
pub enum ExecutionError {
  /// Caller supplied an invalid request or path boundary.
  #[error("invalid execution request: {0}")]
  Invalid(String),
  /// The configured backend cannot enforce the requested execution.
  #[error("execution backend is unavailable: {0}")]
  Unavailable(String),
  /// The backend failed while creating, driving, or removing resources.
  #[error("execution backend failed: {0}")]
  Backend(String),
  /// Local asynchronous or filesystem I/O failed.
  #[error("execution I/O failed: {0}")]
  Io(#[source] std::io::Error),
  /// Protocol streams were requested more than once.
  #[error("execution I/O has already been taken")]
  IoTaken,
  /// Process exit was requested more than once.
  #[error("execution process has already been reaped")]
  Reaped,
  /// Cancellation won before the requested operation completed.
  #[error("execution was cancelled")]
  Cancelled,
  /// The shared execution deadline expired during the named operation.
  #[error("execution deadline expired while {operation}")]
  TimedOut {
    /// Operation active when the deadline expired.
    operation: &'static str,
  },
  /// Both an execution operation and its mandatory cleanup failed.
  #[error("execution failed ({operation}) and cleanup also failed: {cleanup}")]
  OperationAndCleanup {
    /// Original lifecycle failure.
    operation: Box<ExecutionError>,
    /// Additional failure that prevented complete cleanup.
    cleanup: Box<ExecutionError>,
  },
}

impl ExecutionError {
  /// Preserves the original failure while reporting that rollback was incomplete.
  pub fn with_cleanup(self, cleanup: ExecutionError) -> Self {
    Self::OperationAndCleanup {
      operation: Box::new(self),
      cleanup: Box::new(cleanup),
    }
  }
}

/// Creates executions while hiding native or sandbox-specific mechanics.
///
/// `start` must honor `StartExecution::max_duration` and `cancellation`. If
/// startup fails, is cancelled, or reaches its deadline after creating backend
/// resources, it must roll them back before returning an error.
#[async_trait]
pub trait ExecutionBackend: Send + Sync {
  /// Exact target's immutable executable projection root when private reader
  /// transport and partition accounting are both available. Default absence
  /// refuses controlled setup before Source or credentials are materialized.
  /// This technical metadata alone does not qualify a complete controlled Job.
  fn factory_tool_projection(&self, _target: &ExecutionTarget) -> Option<PathBuf> {
    None
  }

  /// Starts one execution and rolls back all created resources on failure.
  async fn start(
    &self,
    runner: &RunnerProgram,
    request: StartExecution,
    cancellation: CancellationToken,
  ) -> Result<Box<dyn RunningExecution>, ExecutionError>;
  /// Removes abandoned resources owned by this agent and backend.
  async fn cleanup_orphans(&self) -> Result<(), ExecutionError>;
}

/// Exclusive handle to one running execution and all resources it owns.
#[async_trait]
pub trait RunningExecution: Send {
  /// Returns paths translated into the execution's filesystem namespace.
  fn paths(&self) -> &ExecutionPaths;
  /// Transfers exclusive ownership of runner protocol streams to the caller.
  fn take_io(&mut self) -> Result<ExecutionIo, ExecutionError>;
  /// Samples accounting for the complete process tree or sandbox.
  async fn sample_usage(&mut self) -> Result<ResourceUsage, ExecutionError>;
  /// Seals owned auxiliary work and samples terminal accounting while the main
  /// process counters still exist. Ordinary executions use their usual sample;
  /// composed owners must await their auxiliary teardown before returning.
  /// The caller still owns closing input, waiting, killing and destruction.
  async fn sample_final_usage(&mut self) -> Result<ResourceUsage, ExecutionError> {
    self.sample_usage().await
  }
  /// Closes any backend-owned input transport after the caller drops its
  /// `ExecutionIo::stdin`. Most process transports need no extra action;
  /// runtimes such as containerd must explicitly signal EOF to their shim.
  async fn close_input(&mut self) -> Result<(), ExecutionError> {
    Ok(())
  }
  /// Waits once for the complete backend process tree to stop.
  ///
  /// A second call must return [`ExecutionError::Reaped`] rather than repeat a
  /// cached exit or wait on resources that have already been consumed.
  async fn wait(&mut self) -> Result<ExecutionExit, ExecutionError>;
  /// Forcefully terminates the complete execution process tree.
  async fn kill(&mut self) -> Result<(), ExecutionError>;
  /// Removes every backend resource owned by this execution.
  async fn destroy(self: Box<Self>) -> Result<(), ExecutionError>;
}

#[cfg(test)]
mod tests {
  use super::*;

  include!("private_tool_channel_tests.rs");

  #[test]
  fn projects_external_executables_in_stable_selector_order() {
    let runner = RunnerProgram {
      release_root: PathBuf::from("/release"),
      executable: PathBuf::from("/release/octa-runner"),
      plugin_projection: Some(crate::ExecutionPluginProjection {
        directory: PathBuf::from("/release/plugins"),
        lock: PathBuf::from("/release/Octa.lock"),
      }),
      external_executables: BTreeMap::from([
        ("Z_TOOL".to_owned(), PathBuf::from("/tools/z")),
        ("A_TOOL".to_owned(), PathBuf::from("/tools/a")),
      ]),
    };

    let projections = runner.external_executable_projections();
    assert_eq!(projections[0].selector, "A_TOOL");
    assert_eq!(projections[0].source, PathBuf::from("/tools/a"));
    assert_eq!(
      projections[0].destination(Path::new("/guest/tools")),
      PathBuf::from("/guest/tools/tool-0")
    );
    assert_eq!(projections[1].selector, "Z_TOOL");
    assert_eq!(
      projections[1].destination(Path::new("/guest/tools")),
      PathBuf::from("/guest/tools/tool-1")
    );
  }

  #[test]
  fn a_standalone_execution_program_projects_no_invented_plugin_files() {
    let temporary = tempfile::tempdir().unwrap();
    let release = temporary.path().canonicalize().unwrap();
    let executable = release.join("reader");
    std::fs::write(&executable, b"selected-helper").unwrap();
    let program = ExecutionProgram::standalone(release.clone(), executable.clone()).unwrap();
    program.validate().unwrap();
    assert_eq!(program.executable, executable);
    assert!(program.plugin_projection.is_none());
    assert_eq!(std::fs::read_dir(&release).unwrap().count(), 1);
    assert!(ExecutionProgram::standalone(release.clone(), release.parent().unwrap().join("outside")).is_err());
  }

  #[test]
  fn validates_execution_boundaries() {
    let temporary = tempfile::tempdir().unwrap();
    let request = StartExecution {
      execution_id: "job-1-attempt-1".to_owned(),
      workspace_root: temporary.path().canonicalize().unwrap(),
      workspace: temporary.path().canonicalize().unwrap(),
      data_dir: temporary.path().canonicalize().unwrap().join("data"),
      workload_identity: None,
      cache: None,
      factory: None,
      cpu_millis: 1000,
      memory_bytes: 512 * 1024 * 1024,
      writable_disk_bytes: 1024 * 1024 * 1024,
      max_duration: Duration::from_secs(600),
      root: ExecutionTarget::Native {
        platform: ExecutionPlatform {
          os: ExecutionOs::Linux,
          architecture: ExecutionArchitecture::Amd64,
        },
      },
      network: NetworkAccess::Unrestricted,
    };
    std::fs::create_dir(&request.data_dir).unwrap();
    assert!(request.validate().is_ok());

    let mut invalid = request.clone();
    invalid.execution_id.clear();
    assert!(invalid.validate().is_err());
    invalid = request;
    invalid.memory_bytes = 0;
    assert!(invalid.validate().is_err());
  }

  fn factory_execution_fixture() -> (tempfile::TempDir, StartExecution, PathBuf) {
    let temporary = tempfile::tempdir().unwrap();
    let work_root = temporary.path().join("work");
    let job_root = work_root.join("job");
    let source = job_root.join("source");
    let scratch = job_root.join("scratch");
    let output = job_root.join("output");
    let protected = job_root.join("protected");
    let writable_protected = job_root.join("writable-protected");
    let data_dir = scratch.join(".octacity");
    for directory in [
      &work_root,
      &job_root,
      &source,
      &scratch,
      &output,
      &protected,
      &writable_protected,
      &data_dir,
    ] {
      std::fs::create_dir(directory).unwrap();
    }
    let mut protected_permissions = std::fs::metadata(&protected).unwrap().permissions();
    protected_permissions.set_readonly(true);
    std::fs::set_permissions(&protected, protected_permissions).unwrap();
    let layout = FactoryExecutionLayout {
      workspace_anchor: false,
      disk_accounting: crate::ExecutionDiskAccounting::WholeJob,
      protected_inputs: protected.canonicalize().unwrap(),
      source: source.canonicalize().unwrap(),
      source_visible: true,
      source_read_only: false,
      source_writable_paths: Vec::new(),
      tool_channel: None,
      scratch: scratch.canonicalize().unwrap(),
      output: output.canonicalize().unwrap(),
      process_limit: 4,
    };
    let request = StartExecution {
      execution_id: "factory-job".to_owned(),
      workspace_root: work_root.canonicalize().unwrap(),
      workspace: source.canonicalize().unwrap(),
      data_dir: data_dir.canonicalize().unwrap(),
      workload_identity: None,
      cache: None,
      factory: Some(layout.clone()),
      cpu_millis: 1000,
      memory_bytes: 1024,
      writable_disk_bytes: 1024,
      max_duration: Duration::from_secs(1),
      root: ExecutionTarget::Oci {
        reference: format!("example.invalid/factory@sha256:{}", "0".repeat(64)),
        platform: ExecutionPlatform {
          os: ExecutionOs::Linux,
          architecture: ExecutionArchitecture::Amd64,
        },
        isolation: OciIsolation::Process,
      },
      network: NetworkAccess::Disabled,
    };
    (temporary, request, writable_protected)
  }

  #[test]
  fn validates_the_factory_layout_before_backend_projection() {
    let (_temporary, mut request, writable_protected) = factory_execution_fixture();
    let layout = request.factory.clone().unwrap();
    assert!(request.validate().is_ok());
    let mut scoped = request.clone();
    let policy = scoped.factory.as_mut().unwrap();
    policy.source_read_only = true;
    policy.source_writable_paths = (0..=octacity_protocol::MAX_FACTORY_PERMISSION_ENTRIES)
      .map(|index| {
        let path = policy.source.join(format!("contract-{index}"));
        std::fs::create_dir(&path).unwrap();
        path
      })
      .collect();
    assert!(
      scoped.validate().is_err(),
      "effective write projections retain the signed permission count ceiling"
    );
    scoped.factory.as_mut().unwrap().source_writable_paths.truncate(2);
    assert!(scoped.validate().is_ok());
    scoped
      .factory
      .as_mut()
      .unwrap()
      .source_writable_paths
      .push(scoped.workspace.clone());
    assert!(scoped.validate().is_err(), "a narrow grant cannot reopen all source");

    request.factory.as_mut().unwrap().process_limit = 0;
    assert!(request.validate().is_err());
    request.factory = Some(FactoryExecutionLayout {
      output: layout.scratch.clone(),
      ..layout.clone()
    });
    assert!(request.validate().is_err());

    request.factory = Some(FactoryExecutionLayout {
      protected_inputs: writable_protected.canonicalize().unwrap(),
      ..layout
    });
    assert!(request.validate().is_err());
  }

  #[test]
  fn rejects_mutable_execution_images_at_the_backend_boundary() {
    let temporary = tempfile::tempdir().unwrap();
    let data_dir = temporary.path().join("data");
    std::fs::create_dir(&data_dir).unwrap();
    let mut request = StartExecution {
      execution_id: "job-1-attempt-1".to_owned(),
      workspace_root: temporary.path().canonicalize().unwrap(),
      workspace: temporary.path().canonicalize().unwrap(),
      data_dir: data_dir.canonicalize().unwrap(),
      workload_identity: None,
      cache: None,
      factory: None,
      cpu_millis: 1000,
      memory_bytes: 512 * 1024 * 1024,
      writable_disk_bytes: 1024 * 1024 * 1024,
      max_duration: Duration::from_secs(600),
      root: ExecutionTarget::Oci {
        reference: "registry.example.com/octacity/build:latest".to_owned(),
        platform: ExecutionPlatform {
          os: ExecutionOs::Linux,
          architecture: ExecutionArchitecture::Amd64,
        },
        isolation: OciIsolation::Process,
      },
      network: NetworkAccess::Disabled,
    };
    assert!(request.validate().is_err());

    request.root = ExecutionTarget::Oci {
      reference:
        "registry.example.com/octacity/build@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
          .to_owned(),
      platform: ExecutionPlatform {
        os: ExecutionOs::Linux,
        architecture: ExecutionArchitecture::Amd64,
      },
      isolation: OciIsolation::Hypervisor,
    };
    assert!(request.validate().is_ok());
  }

  #[test]
  fn rejects_noncanonical_identity_paths_before_a_backend_sees_them() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let workspace = root.join("workspace");
    let identity_directory = root.join("identities");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::create_dir(&identity_directory).unwrap();
    let data_dir = workspace.join("data");
    std::fs::create_dir(&data_dir).unwrap();
    let identity = identity_directory.join("job.identity");
    std::fs::write(&identity, b"identity").unwrap();
    let mut request = StartExecution {
      execution_id: "job-1-attempt-1".to_owned(),
      workspace_root: root,
      workspace,
      data_dir,
      workload_identity: Some(identity.clone()),
      cache: None,
      factory: None,
      cpu_millis: 1000,
      memory_bytes: 512 * 1024 * 1024,
      writable_disk_bytes: 1024 * 1024 * 1024,
      max_duration: Duration::from_secs(600),
      root: ExecutionTarget::Native {
        platform: ExecutionPlatform {
          os: ExecutionOs::Linux,
          architecture: ExecutionArchitecture::Amd64,
        },
      },
      network: NetworkAccess::Disabled,
    };
    assert!(request.validate().is_ok());

    // `PathBuf::push` normalizes `..` when the base has the verbatim `\\?\`
    // prefix returned by Windows canonicalization. Build the spelling as an
    // `OsString` so this test actually reaches the boundary with a parent
    // component on every supported platform.
    let mut noncanonical_identity = identity_directory.into_os_string();
    for component in ["..", "identities", "job.identity"] {
      noncanonical_identity.push(std::path::MAIN_SEPARATOR_STR);
      noncanonical_identity.push(component);
    }
    let noncanonical_identity = PathBuf::from(noncanonical_identity);
    assert!(
      noncanonical_identity
        .components()
        .any(|component| component == Component::ParentDir),
      "test fixture must preserve its parent component"
    );
    request.workload_identity = Some(noncanonical_identity);
    assert!(request.validate().is_err());
  }

  #[test]
  fn validates_cache_capacity_and_uses_one_guest_projection() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let cache_root = root.join("cache");
    let scope = cache_root.join("scope");
    std::fs::create_dir(&cache_root).unwrap();
    std::fs::create_dir(&scope).unwrap();
    let mounts = ExecutionCacheMounts {
      capacity_root: cache_root,
      local_directory: scope,
      local_capacity: LocalCacheCapacity::new(1024, 900, 800).unwrap(),
      aggregate_max_bytes: 2048,
      token_file: None,
      ca_certificate_file: None,
    };
    assert_eq!(
      mounts.projected_paths(),
      ExecutionCachePaths {
        local_directory: PathBuf::from(CACHE_DIRECTORY_PATH),
        token_file: None,
        ca_certificate_file: None,
      }
    );

    let work_root = root.join("work");
    let workspace = work_root.join("workspace");
    let data_dir = workspace.join("data");
    std::fs::create_dir(&work_root).unwrap();
    std::fs::create_dir(&workspace).unwrap();
    std::fs::create_dir(&data_dir).unwrap();
    let request = StartExecution {
      execution_id: "job-1-attempt-1".to_owned(),
      workspace_root: work_root.canonicalize().unwrap(),
      workspace: workspace.canonicalize().unwrap(),
      data_dir: data_dir.canonicalize().unwrap(),
      workload_identity: None,
      cache: Some(mounts.clone()),
      factory: None,
      cpu_millis: 1000,
      memory_bytes: 1024,
      writable_disk_bytes: 1024,
      max_duration: Duration::from_secs(1),
      root: ExecutionTarget::Native {
        platform: ExecutionPlatform {
          os: ExecutionOs::Linux,
          architecture: ExecutionArchitecture::Amd64,
        },
      },
      network: NetworkAccess::Disabled,
    };
    assert!(request.validate().is_ok());

    let mut invalid = request;
    invalid.cache.as_mut().unwrap().aggregate_max_bytes = 512;
    assert!(invalid.validate().is_err());
  }

  #[cfg(unix)]
  #[test]
  fn process_cache_requires_a_dedicated_bounded_filesystem() {
    let temporary = tempfile::tempdir().unwrap();
    let capacity_root = temporary.path().join("cache");
    let local_directory = capacity_root.join("scope");
    std::fs::create_dir(&capacity_root).unwrap();
    std::fs::create_dir(&local_directory).unwrap();
    let cache = ExecutionCacheMounts {
      capacity_root,
      local_directory,
      local_capacity: LocalCacheCapacity::new(1024, 900, 800).unwrap(),
      aggregate_max_bytes: 2048,
      token_file: None,
      ca_certificate_file: None,
    };

    assert!(matches!(
      validate_process_cache_filesystem(&cache),
      Err(ExecutionError::Unavailable(message)) if message.contains("dedicated filesystem")
    ));
  }

  #[cfg(unix)]
  #[test]
  fn rejects_identity_paths_through_an_intermediate_symlink() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let workspace = root.join("workspace");
    let identity_directory = root.join("identities");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::create_dir(&identity_directory).unwrap();
    let data_dir = workspace.join("data");
    std::fs::create_dir(&data_dir).unwrap();
    std::fs::write(identity_directory.join("job.identity"), b"identity").unwrap();
    let alias = root.join("identity-alias");
    symlink(&identity_directory, &alias).unwrap();
    let request = StartExecution {
      execution_id: "job-1-attempt-1".to_owned(),
      workspace_root: root,
      workspace,
      data_dir,
      workload_identity: Some(alias.join("job.identity")),
      cache: None,
      factory: None,
      cpu_millis: 1000,
      memory_bytes: 512 * 1024 * 1024,
      writable_disk_bytes: 1024 * 1024 * 1024,
      max_duration: Duration::from_secs(600),
      root: ExecutionTarget::Native {
        platform: ExecutionPlatform {
          os: ExecutionOs::Linux,
          architecture: ExecutionArchitecture::Amd64,
        },
      },
      network: NetworkAccess::Disabled,
    };

    assert!(request.validate().is_err());
  }
  #[test]
  fn an_octa_only_backend_refuses_a_standalone_program_projection() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    let program = ExecutionProgram::standalone(root.clone(), root.join("reader")).unwrap();
    assert!(matches!(
      program.require_plugin_projection(),
      Err(ExecutionError::Unavailable(_))
    ));
    let plugins = ExecutionPluginProjection {
      directory: root.join("plugins"),
      lock: root.join("Octa.lock"),
    };
    let runner = ExecutionProgram {
      plugin_projection: Some(plugins.clone()),
      ..program
    };
    assert_eq!(runner.require_plugin_projection().unwrap(), &plugins);
  }
  #[test]
  fn a_factory_helper_reserves_resources_from_the_original_job_allocation() {
    let (_directory, mut main, _) = factory_execution_fixture();
    main.cpu_millis = 3000;
    main.memory_bytes = 768 * 1024 * 1024;
    main.writable_disk_bytes = 64 * 1024 * 1024;
    main.factory.as_mut().unwrap().process_limit = 64;
    main.root = ExecutionTarget::Oci {
      reference: format!("example.invalid/factory@sha256:{}", "0".repeat(64)),
      platform: ExecutionPlatform {
        os: ExecutionOs::Linux,
        architecture: ExecutionArchitecture::Amd64,
      },
      isolation: OciIsolation::Hypervisor,
    };
    let layout = main.factory.as_mut().unwrap();
    layout.source_visible = false;
    layout.source_read_only = true;
    let root = layout.source.parent().unwrap().to_owned();
    let helper_scratch = root.join("reader-scratch");
    let helper_output = root.join("reader-output");
    let helper_protected = root.join("reader-protected");
    for directory in [&helper_scratch, &helper_output, &helper_protected] {
      std::fs::create_dir(directory).unwrap();
    }
    let helper_data = helper_scratch.join("data");
    std::fs::create_dir(&helper_data).unwrap();
    let mut permissions = std::fs::metadata(&helper_protected).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&helper_protected, permissions).unwrap();
    let mut helper = main.clone();
    helper.execution_id = "selected-reader".to_owned();
    helper.data_dir = helper_data;
    helper.cpu_millis = 1000;
    helper.memory_bytes = 256 * 1024 * 1024;
    helper.writable_disk_bytes = 16 * 1024 * 1024;
    helper.max_duration = Duration::from_millis(500);
    let helper_layout = helper.factory.as_mut().unwrap();
    helper_layout.source_visible = true;
    helper_layout.scratch = helper_scratch;
    helper_layout.output = helper_output;
    helper_layout.protected_inputs = helper_protected;
    helper_layout.process_limit = 8;
    let original = main.clone();
    let (narrowed, reserved) = FactoryExecutionPartition::new(main, helper.clone())
      .unwrap()
      .into_parts();
    assert_eq!(
      narrowed.factory.as_ref().unwrap().disk_accounting,
      ExecutionDiskAccounting::OwnedWritableMounts
    );
    assert_eq!(
      reserved.factory.as_ref().unwrap().disk_accounting,
      ExecutionDiskAccounting::OwnedWritableMounts
    );
    assert_eq!(narrowed.cpu_millis, 2000);
    assert_eq!(narrowed.memory_bytes, 512 * 1024 * 1024);
    assert_eq!(narrowed.writable_disk_bytes, 48 * 1024 * 1024);
    assert_eq!(narrowed.factory.as_ref().unwrap().process_limit, 56);
    assert_eq!(reserved.cpu_millis, 1000);
    assert_eq!(reserved.memory_bytes, 256 * 1024 * 1024);
    assert_eq!(reserved.writable_disk_bytes, 16 * 1024 * 1024);
    assert_eq!(reserved.factory.as_ref().unwrap().process_limit, 8);
    assert_eq!(narrowed.max_duration, original.max_duration);
    assert_eq!(reserved.max_duration, Duration::from_millis(500));
    assert!(narrowed.validate().is_ok());
    assert!(reserved.validate().is_ok());
    for dimension in 0..4 {
      let mut oversized = helper.clone();
      match dimension {
        0 => oversized.cpu_millis = original.cpu_millis,
        1 => oversized.memory_bytes = original.memory_bytes,
        2 => oversized.writable_disk_bytes = original.writable_disk_bytes,
        _ => oversized.factory.as_mut().unwrap().process_limit = original.factory.as_ref().unwrap().process_limit,
      }
      assert!(
        FactoryExecutionPartition::new(original.clone(), oversized).is_err(),
        "dimension {dimension} has no main remainder"
      );
    }
    let mut writable = helper.clone();
    writable.factory.as_mut().unwrap().source_read_only = false;
    assert!(writable.validate().is_ok());
    assert!(FactoryExecutionPartition::new(original.clone(), writable).is_err());
    let mut overlapping = helper.clone();
    overlapping.factory.as_mut().unwrap().scratch = original.factory.as_ref().unwrap().scratch.clone();
    overlapping.data_dir = original.data_dir.clone();
    assert!(overlapping.validate().is_ok());
    assert!(FactoryExecutionPartition::new(original.clone(), overlapping).is_err());
    let foreign_source = root.join("other-source");
    std::fs::create_dir(&foreign_source).unwrap();
    let mut foreign = helper.clone();
    foreign.workspace = foreign_source.clone();
    foreign.factory.as_mut().unwrap().source = foreign_source;
    assert!(foreign.validate().is_ok());
    assert!(FactoryExecutionPartition::new(original.clone(), foreign).is_err());
    let mut foreign_target = helper.clone();
    foreign_target.root = ExecutionTarget::Oci {
      reference: format!("example.invalid/factory@sha256:{}", "1".repeat(64)),
      platform: ExecutionPlatform {
        os: ExecutionOs::Linux,
        architecture: ExecutionArchitecture::Amd64,
      },
      isolation: OciIsolation::Hypervisor,
    };
    assert!(foreign_target.validate().is_ok());
    assert!(FactoryExecutionPartition::new(original.clone(), foreign_target).is_err());
    let identity = root.join("private-identity");
    std::fs::write(&identity, b"private").unwrap();
    helper.workload_identity = Some(identity);
    assert!(helper.validate().is_ok());
    assert!(FactoryExecutionPartition::new(original, helper).is_err());
  }
  #[test]
  fn a_legacy_backend_refuses_partition_disk_accounting_before_job_inspection() {
    let (_directory, mut request, _) = factory_execution_fixture();
    assert!(request.require_whole_job_disk_accounting().is_ok());
    request.factory.as_mut().unwrap().disk_accounting = ExecutionDiskAccounting::OwnedWritableMounts;
    request.workspace = request.workspace_root.join("unopened-job");
    assert!(matches!(
      request.require_whole_job_disk_accounting(),
      Err(ExecutionError::Unavailable(_))
    ));
    assert!(!request.workspace.exists());
  }
}
