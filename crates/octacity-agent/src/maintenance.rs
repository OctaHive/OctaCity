//! Idle host maintenance performed before the agent acquires another lease.
//!
//! This module owns disk admission and local-cache reclamation. Keeping that
//! policy outside the daemon loop makes the lifecycle ordering visible: no
//! workspace is accepted until every mounted filesystem can hold all of the
//! reservations that share it.

use std::{collections::BTreeMap, path::PathBuf};

use octacity_cache_session::CacheSessionManager;
use octacity_config::{AgentConfig, DiskReservations};
use octacity_inventory::{FilesystemIdentity, FilesystemSample, HostMonitor, StorageFilesystems, StorageRoots};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

#[derive(Clone, Debug, Eq, PartialEq)]
struct DiskPressure {
  roots: Vec<&'static str>,
  mount_point: PathBuf,
  available_bytes: u64,
  required_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DiskRequirement {
  root: &'static str,
  filesystem: FilesystemSample,
  required_bytes: u64,
}

/// Reclaims at most one inactive cache scope and reports whether another job
/// can be admitted from the resulting filesystem snapshot.
///
/// This function deliberately performs one pass. The daemon must continue
/// polling the coordinator with `accept_jobs = false` while capacity is low so
/// drain and shutdown remain live control-plane operations.
pub(crate) async fn disk_capacity_available(
  config: &AgentConfig,
  host: &mut HostMonitor,
  cache: &CacheSessionManager,
  shutdown: CancellationToken,
) -> Result<bool, Box<dyn std::error::Error>> {
  if shutdown.is_cancelled() {
    return Ok(false);
  }
  let samples = host.sample_filesystems(StorageRoots {
    work: &config.work_root,
    state: &config.state_root,
    cache: &config.cache.root,
  })?;
  let reservations = config.disk_reservations()?;
  let (requirements, cache_filesystem) = disk_requirements(samples, reservations);
  let cache_target = required_on_filesystem(&requirements, cache_filesystem);
  let report = match cache.reclaim_inactive(cache_target, shutdown.clone()).await {
    Err(octacity_cache_session::CacheSessionError::Cancelled) if shutdown.is_cancelled() => return Ok(false),
    result => result?,
  };
  if report.removed_scopes > 0 {
    info!(
      removed_scopes = report.removed_scopes,
      initial_free_bytes = report.initial_free_bytes,
      final_free_bytes = report.final_free_bytes,
      "reclaimed inactive cache scopes under disk pressure"
    );
    // Poll once without accepting jobs before another potentially expensive
    // pass. This bounds maintenance work between control-plane observations.
    return Ok(false);
  }
  let pressure = disk_pressure(requirements);
  if !pressure.is_empty() {
    warn!(?pressure, "disk pressure paused job admission");
  }
  Ok(pressure.is_empty())
}

fn disk_requirements(
  samples: StorageFilesystems,
  reservations: DiskReservations,
) -> (Vec<DiskRequirement>, FilesystemIdentity) {
  let cache_identity = samples.cache.identity;
  let requirements = vec![
    DiskRequirement {
      root: "work",
      filesystem: samples.work,
      required_bytes: reservations.work_bytes,
    },
    DiskRequirement {
      root: "state",
      filesystem: samples.state,
      required_bytes: reservations.state_bytes,
    },
    DiskRequirement {
      root: "cache",
      filesystem: samples.cache,
      required_bytes: reservations.cache_bytes,
    },
  ];
  (requirements, cache_identity)
}

fn required_on_filesystem(requirements: &[DiskRequirement], filesystem: FilesystemIdentity) -> u64 {
  requirements
    .iter()
    .filter(|requirement| requirement.filesystem.identity == filesystem)
    .fold(0_u64, |total, requirement| {
      total.saturating_add(requirement.required_bytes)
    })
}

fn disk_pressure(requirements: Vec<DiskRequirement>) -> Vec<DiskPressure> {
  let mut pools = BTreeMap::<FilesystemIdentity, DiskPressure>::new();
  for requirement in requirements {
    let pool = pools
      .entry(requirement.filesystem.identity)
      .or_insert_with(|| DiskPressure {
        roots: Vec::new(),
        mount_point: requirement.filesystem.mount_point,
        available_bytes: requirement.filesystem.available_bytes,
        required_bytes: 0,
      });
    pool.roots.push(requirement.root);
    pool.available_bytes = pool.available_bytes.min(requirement.filesystem.available_bytes);
    pool.required_bytes = pool.required_bytes.saturating_add(requirement.required_bytes);
  }
  pools
    .into_values()
    .filter(|pool| pool.available_bytes < pool.required_bytes)
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  fn filesystem_identity(value: u64) -> FilesystemIdentity {
    #[cfg(unix)]
    {
      FilesystemIdentity::UnixDevice(value)
    }
    #[cfg(windows)]
    {
      FilesystemIdentity::WindowsVolume(value as u32)
    }
  }

  fn requirement(root: &'static str, identity: u64, mount: &str, available: u64, required: u64) -> DiskRequirement {
    DiskRequirement {
      root,
      filesystem: FilesystemSample {
        identity: filesystem_identity(identity),
        mount_point: PathBuf::from(mount),
        available_bytes: available,
      },
      required_bytes: required,
    }
  }

  #[test]
  fn adds_reservations_that_share_one_mounted_filesystem() {
    let separate = vec![
      requirement("work", 1, "/work", 2, 2),
      requirement("state", 2, "/state", 3, 3),
      requirement("cache", 3, "/cache", 1, 1),
    ];
    assert!(disk_pressure(separate).is_empty());

    let shared = vec![
      requirement("work", 1, "/bind/work", 4, 2),
      requirement("state", 1, "/bind/state", 4, 3),
      requirement("cache", 2, "/cache", 0, 1),
    ];
    assert_eq!(required_on_filesystem(&shared, filesystem_identity(1)), 5);
    let pressure = disk_pressure(shared);
    assert_eq!(pressure.len(), 2);
    assert_eq!(pressure[0].roots, vec!["work", "state"]);
    assert_eq!(pressure[1].roots, vec!["cache"]);
  }

  #[cfg(any(
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")),
    all(target_os = "macos", target_arch = "aarch64")
  ))]
  #[test]
  fn accounts_for_output_staging_and_cache_growth() {
    let fixture = crate::composition::tests::installed_agent_fixture("https://coordinator.example");
    let config = AgentConfig::load(&fixture.config).unwrap();
    let mut host = HostMonitor::new(config.work_root.clone(), config.state_root.clone(), false).unwrap();
    let samples = host
      .sample_filesystems(StorageRoots {
        work: &config.work_root,
        state: &config.state_root,
        cache: &config.cache.root,
      })
      .unwrap();
    let (requirements, _) = disk_requirements(samples, config.disk_reservations().unwrap());
    let state = requirements
      .iter()
      .find(|requirement| requirement.root == "state")
      .unwrap();
    assert_eq!(
      state.required_bytes,
      config.max_spool_bytes
        + config.max_output_limits.artifact_bytes
        + config.max_output_limits.report_bytes
        + config.maintenance.state_reserve_bytes
    );
    let cache = requirements
      .iter()
      .find(|requirement| requirement.root == "cache")
      .unwrap();
    assert_eq!(
      cache.required_bytes,
      config.cache.capacity.max_bytes + config.maintenance.cache_reserve_bytes
    );
  }

  #[cfg(any(
    all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")),
    all(target_os = "macos", target_arch = "aarch64")
  ))]
  #[tokio::test]
  async fn cancelled_admission_does_not_start_reclamation() {
    let _guard = crate::composition::tests::component_graph_guard().await;
    let fixture = crate::composition::tests::installed_agent_fixture("https://coordinator.example");
    let mut components = crate::composition::Components::load(&fixture.config).await.unwrap();
    components.validated.config.maintenance.cache_reserve_bytes = u64::MAX;
    let shutdown = CancellationToken::new();
    shutdown.cancel();

    assert!(
      !disk_capacity_available(
        &components.validated.config,
        &mut components.host,
        components.cache.as_ref(),
        shutdown,
      )
      .await
      .unwrap()
    );
  }
}
