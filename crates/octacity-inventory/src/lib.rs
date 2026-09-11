//! Builds scheduler inventory and advisory host snapshots.
//!
//! This crate is the only place that combines system metrics with verified
//! runner and source-plugin inventories. The coordinator transport receives
//! versioned DTOs and never reaches into component-specific types.

use std::{collections::BTreeMap, path::PathBuf};

use octacity_protocol::{
  ActiveJob, AgentInventory, BackendHealth, COORDINATOR_PROTOCOL_VERSION, HostCapacity, HostSnapshot, OctaInventory,
  PlatformArchitecture, PlatformOs, PlatformSpec, RuntimeCapability, SourcePluginInventory, TaskPluginInventory,
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
}

/// Builds the immutable registration inventory from verified components.
pub fn build_inventory(
  agent_id: String,
  agent_version: String,
  labels: BTreeMap<String, String>,
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
  let inventory = AgentInventory {
    agent_id,
    agent_version,
    coordinator_protocols: vec![COORDINATOR_PROTOCOL_VERSION],
    labels,
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

fn filesystem_values(disks: &Disks, path: &std::path::Path) -> Result<(u64, u64), InventoryError> {
  disks
    .list()
    .iter()
    .filter_map(|disk| mount_depth(path, disk.mount_point()).map(|depth| (depth, disk)))
    .max_by_key(|(depth, _)| *depth)
    .map(|(_, disk)| (disk.total_space(), disk.available_space()))
    .ok_or_else(|| InventoryError::Filesystem(path.to_owned()))
}

fn mount_depth(path: &std::path::Path, mount: &std::path::Path) -> Option<usize> {
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
