//! Builds scheduler inventory and advisory host snapshots.
//!
//! This crate is the only place that combines system metrics with verified
//! runner and source-plugin inventories. The coordinator transport receives
//! versioned DTOs and never reaches into component-specific types.

#![warn(missing_docs)]

use std::{
  collections::BTreeMap,
  fs,
  path::{Path, PathBuf},
};

use octacity_protocol::{
  ActiveJob, AgentInventory, BackendHealth, CACHE_FEATURE_V1, CACHE_HTTP_FEATURE_V1, COORDINATOR_PROTOCOL_VERSION,
  CacheCapability, HostCapacity, HostSnapshot, OctaInventory, PlatformArchitecture, PlatformOs, PlatformSpec,
  RuntimeCapability, SourcePluginInventory, TaskPluginInventory,
};
use octacity_runner::RunnerInstallation;
use octacity_source::SourcePluginRegistry;
use sysinfo::{Disks, System};
use thiserror::Error;

/// Failure to inspect host resources or construct a valid inventory.
#[derive(Debug, Error)]
pub enum InventoryError {
  /// Logical CPU capacity could not be inspected.
  #[error("failed to inspect available parallelism: {0}")]
  Cpu(#[source] std::io::Error),
  /// A host capacity value cannot be represented by protocol v1.
  #[error("host capacity is not representable: {0}")]
  Capacity(String),
  /// Host OS or architecture is not representable by protocol v1.
  #[error("unsupported agent host platform '{os}-{architecture}'")]
  UnsupportedPlatform {
    /// Rust target operating-system identifier.
    os: &'static str,
    /// Rust target architecture identifier.
    architecture: &'static str,
  },
  /// No mounted filesystem contains one of the configured roots.
  #[error("no mounted filesystem contains '{0}'")]
  Filesystem(PathBuf),
  /// Filesystem identity could not be inspected for a configured root.
  #[error("failed to inspect filesystem identity for '{path}': {source}")]
  FilesystemIdentity {
    /// Configured path whose filesystem was inspected.
    path: PathBuf,
    /// Underlying platform error.
    source: std::io::Error,
  },
  /// Generated inventory or snapshot violates the shared protocol contract.
  #[error(transparent)]
  Protocol(#[from] octacity_protocol::CoordinatorProtocolError),
}

/// Stateful system sampler shared by registration and heartbeat production.
pub struct HostMonitor {
  work_root: PathBuf,
  state_root: PathBuf,
  logical_cpu_count: u32,
  system: System,
  disks: Disks,
  capacity: HostCapacity,
}

/// Host-local identity of one mounted filesystem.
///
/// Unlike a mount path, this value remains equal for multiple bind mounts of
/// the same filesystem. It is meaningful only within the current host; the
/// variant records which kernel identity was observed so callers cannot mix
/// unrelated raw integer namespaces accidentally.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FilesystemIdentity {
  /// Unix device number returned by `stat(2)`.
  #[cfg(unix)]
  UnixDevice(u64),
  /// Windows volume serial number returned for an open directory handle.
  #[cfg(windows)]
  WindowsVolume(u32),
}

/// One refreshed filesystem observation used by local admission policy.
///
/// The kernel identity, rather than the diagnostic mount path, lets the agent
/// combine reservations when configured roots share one filesystem.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemSample {
  /// Kernel or volume identity used to combine reservations across bind mounts.
  pub identity: FilesystemIdentity,
  /// Mount containing the inspected configured root, retained for diagnostics.
  pub mount_point: PathBuf,
  /// Bytes currently available to the service account on that mount.
  pub available_bytes: u64,
}

/// Named configured roots sampled together for disk admission.
#[derive(Clone, Copy, Debug)]
pub struct StorageRoots<'a> {
  /// Root that receives per-job workspaces.
  pub work: &'a Path,
  /// Root that holds durable lifecycle and output state.
  pub state: &'a Path,
  /// Root that holds persistent Octa cache scopes.
  pub cache: &'a Path,
}

/// One coherent disk observation for all agent storage roles.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageFilesystems {
  /// Filesystem containing the workspace root.
  pub work: FilesystemSample,
  /// Filesystem containing the durable state root.
  pub state: FilesystemSample,
  /// Filesystem containing the cache root.
  pub cache: FilesystemSample,
}

/// Operator and release values that identify one registration inventory.
#[derive(Clone, Debug)]
pub struct AgentInventoryConfig {
  /// Stable operator-assigned agent identity.
  pub agent_id: String,
  /// Running OctaCity agent release.
  pub agent_version: String,
  /// Scheduler-visible operator labels.
  pub labels: BTreeMap<String, String>,
  /// Whether local policy permits advertising Octa's HTTP L2 transport.
  pub remote_cache_configured: bool,
}

impl HostMonitor {
  /// Inspects static capacity and prepares incremental CPU/memory refreshes.
  pub fn new(work_root: PathBuf, state_root: PathBuf, virtualization_available: bool) -> Result<Self, InventoryError> {
    let logical_cpu_count = u32::try_from(std::thread::available_parallelism().map_err(InventoryError::Cpu)?.get())
      .map_err(|_| InventoryError::Capacity("logical CPU count exceeds u32".to_owned()))?;
    let system = System::new_all();
    let disks = Disks::new_with_refreshed_list();
    let capacity = HostCapacity {
      logical_cpu_count,
      total_memory_bytes: system.total_memory(),
      work_disk_total_bytes: filesystem_values(&disks, &work_root)?.0,
      state_disk_total_bytes: filesystem_values(&disks, &state_root)?.0,
      virtualization_available,
    };
    capacity.validate()?;
    Ok(Self {
      work_root,
      state_root,
      logical_cpu_count,
      system,
      disks,
      capacity,
    })
  }

  /// Returns the immutable capacity advertised during registration.
  pub fn capacity(&self) -> &HostCapacity {
    &self.capacity
  }

  /// Refreshes advisory availability and validates it against static capacity.
  pub fn snapshot(
    &mut self,
    active_job: Option<ActiveJob>,
    backends: Vec<BackendHealth>,
  ) -> Result<HostSnapshot, InventoryError> {
    self.system.refresh_cpu_usage();
    self.system.refresh_memory();
    self.disks.refresh(true);
    let busy_fraction = f64::from(self.system.global_cpu_usage()).clamp(0.0, 100.0) / 100.0;
    let total_cpu_millis = u64::from(self.logical_cpu_count) * 1000;
    let snapshot = HostSnapshot {
      available_cpu_millis: ((total_cpu_millis as f64) * (1.0 - busy_fraction)).round() as u64,
      available_memory_bytes: self.system.available_memory().min(self.capacity.total_memory_bytes),
      work_disk_free_bytes: filesystem_values(&self.disks, &self.work_root)?
        .1
        .min(self.capacity.work_disk_total_bytes),
      state_disk_free_bytes: filesystem_values(&self.disks, &self.state_root)?
        .1
        .min(self.capacity.state_disk_total_bytes),
      active_job,
      backends,
    };
    snapshot.validate(&self.capacity)?;
    Ok(snapshot)
  }

  /// Refreshes disks once and samples every storage role from that coherent
  /// observation. Naming the roles prevents positional path/result coupling.
  pub fn sample_filesystems(&mut self, roots: StorageRoots<'_>) -> Result<StorageFilesystems, InventoryError> {
    self.disks.refresh(true);
    Ok(StorageFilesystems {
      work: sample_filesystem(&self.disks, roots.work)?,
      state: sample_filesystem(&self.disks, roots.state)?,
      cache: sample_filesystem(&self.disks, roots.cache)?,
    })
  }
}

fn sample_filesystem(disks: &Disks, path: &Path) -> Result<FilesystemSample, InventoryError> {
  let disk = filesystem_disk(disks, path)?;
  Ok(FilesystemSample {
    identity: filesystem_identity(path)?,
    mount_point: disk.mount_point().to_owned(),
    available_bytes: disk.available_space(),
  })
}

#[cfg(unix)]
fn filesystem_identity(path: &Path) -> Result<FilesystemIdentity, InventoryError> {
  use std::os::unix::fs::MetadataExt as _;

  fs::metadata(path)
    .map(|metadata| FilesystemIdentity::UnixDevice(metadata.dev()))
    .map_err(|source| InventoryError::FilesystemIdentity {
      path: path.to_owned(),
      source,
    })
}

#[cfg(windows)]
fn filesystem_identity(path: &Path) -> Result<FilesystemIdentity, InventoryError> {
  use std::{
    mem::MaybeUninit,
    os::windows::{fs::OpenOptionsExt as _, io::AsRawHandle as _},
  };
  use windows_sys::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS, GetFileInformationByHandle},
  };

  let file = fs::OpenOptions::new()
    .read(true)
    .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
    .open(path)
    .map_err(|source| InventoryError::FilesystemIdentity {
      path: path.to_owned(),
      source,
    })?;
  let mut information = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::zeroed();
  // SAFETY: `information` points to writable storage of the required type and
  // `file` keeps the directory handle alive for the complete synchronous call.
  let succeeded = unsafe { GetFileInformationByHandle(file.as_raw_handle() as HANDLE, information.as_mut_ptr()) };
  if succeeded == 0 {
    return Err(InventoryError::FilesystemIdentity {
      path: path.to_owned(),
      source: std::io::Error::last_os_error(),
    });
  }
  // SAFETY: a successful call initialized every field in the structure.
  let information = unsafe { information.assume_init() };
  Ok(FilesystemIdentity::WindowsVolume(information.dwVolumeSerialNumber))
}

/// Builds the immutable registration inventory from verified components.
pub fn build_inventory(
  config: AgentInventoryConfig,
  runtimes: Vec<RuntimeCapability>,
  runner: &RunnerInstallation,
  sources: &SourcePluginRegistry,
  capacity: HostCapacity,
) -> Result<AgentInventory, InventoryError> {
  let plugins = runner
    .plugins
    .iter()
    .map(|(name, plugin)| TaskPluginInventory {
      name: name.clone(),
      version: plugin.version.clone(),
      protocol: plugin.protocol,
      platforms: plugin.platforms.clone(),
      sha256: plugin.sha256.clone(),
      capabilities: plugin.capabilities.clone(),
    })
    .collect();
  let source_plugins = sources
    .iter()
    .map(|(_, plugin)| SourcePluginInventory {
      name: plugin.manifest.name.clone(),
      version: plugin.manifest.version.clone(),
      protocol_min: plugin.manifest.protocol_min,
      protocol_max: plugin.manifest.protocol_max,
      platforms: plugin.manifest.platforms.clone(),
      sha256: plugin.manifest.sha256.clone(),
    })
    .collect();
  let cache = runner
    .capabilities
    .runner_protocols
    .contains(&octa_runner_protocol::RUNNER_PROTOCOL_VERSION)
    .then(|| {
      runner
        .capabilities
        .features
        .iter()
        .any(|feature| feature == CACHE_FEATURE_V1)
    })
    .filter(|supported| *supported)
    .map(|_| CacheCapability {
      runner_protocol: octa_runner_protocol::RUNNER_PROTOCOL_VERSION,
      action_key_format: octa_cache_protocol::ACTION_KEY_FORMAT_V1,
      remote_http: config.remote_cache_configured
        && runner
          .capabilities
          .features
          .iter()
          .any(|feature| feature == CACHE_HTTP_FEATURE_V1),
    });
  let inventory = AgentInventory {
    agent_id: config.agent_id,
    agent_version: config.agent_version,
    coordinator_protocols: vec![COORDINATOR_PROTOCOL_VERSION],
    labels: config.labels,
    host_platform: host_platform()?,
    host_capacity: capacity,
    runtimes,
    octa: OctaInventory {
      version: runner.capabilities.octa_version.clone(),
      runner_sha256: runner.sha256.clone(),
      build_commit: runner.capabilities.build_commit.clone(),
      runner_protocols: runner.capabilities.runner_protocols.clone(),
      event_schemas: runner.capabilities.event_schemas.clone(),
      plugin_protocols: runner.capabilities.plugin_protocols.clone(),
      octafile_versions: runner.capabilities.octafile_versions.clone(),
      features: runner.capabilities.features.clone(),
      plugins,
    },
    source_plugins,
    cache,
  };
  inventory.validate()?;
  Ok(inventory)
}

/// Converts the current Rust target into the coordinator platform vocabulary.
pub fn host_platform() -> Result<PlatformSpec, InventoryError> {
  let os = match std::env::consts::OS {
    "linux" => PlatformOs::Linux,
    "windows" => PlatformOs::Windows,
    "macos" => PlatformOs::Macos,
    _ => {
      return Err(InventoryError::UnsupportedPlatform {
        os: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
      });
    }
  };
  let architecture = match std::env::consts::ARCH {
    "x86_64" => PlatformArchitecture::Amd64,
    "aarch64" => PlatformArchitecture::Arm64,
    _ => {
      return Err(InventoryError::UnsupportedPlatform {
        os: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
      });
    }
  };
  Ok(PlatformSpec { os, architecture })
}

fn filesystem_values(disks: &Disks, path: &Path) -> Result<(u64, u64), InventoryError> {
  let disk = filesystem_disk(disks, path)?;
  Ok((disk.total_space(), disk.available_space()))
}

fn filesystem_disk<'a>(disks: &'a Disks, path: &Path) -> Result<&'a sysinfo::Disk, InventoryError> {
  disks
    .list()
    .iter()
    .filter_map(|disk| mount_depth(path, disk.mount_point()).map(|depth| (depth, disk)))
    .max_by_key(|(depth, _)| *depth)
    .map(|(_, disk)| disk)
    .ok_or_else(|| InventoryError::Filesystem(path.to_owned()))
}

fn mount_depth(path: &Path, mount: &Path) -> Option<usize> {
  if path.starts_with(mount) {
    return Some(mount.components().count());
  }
  // `canonicalize` adds a verbatim prefix on Windows while sysinfo normally
  // reports drive roots without it. Comparing both forms keeps disk discovery
  // correct without maintaining a second platform-specific path normalizer.
  let canonical = mount.canonicalize().ok()?;
  path.starts_with(&canonical).then(|| canonical.components().count())
}

#[cfg(test)]
mod tests;
