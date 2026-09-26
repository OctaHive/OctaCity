//! Installs and validates released products before Agent Ready scenarios run.
//!
//! The harness accepts only self-verifying release roots. It copies their
//! checksum inventories into a fresh directory, validates them again after the
//! copy, and then checks cross-product protocol and revision compatibility.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod bundle;
mod contract;
mod octa;

use std::{
  collections::BTreeMap,
  fs,
  path::{Path, PathBuf},
};

use serde::Serialize;
use thiserror::Error;

use bundle::{Bundle, octa_runtime_platform};
use contract::ReleaseContract;
use octa::OctaBundle;

pub use bundle::ProductManifest;
pub use octa::RunnerCapabilities;

/// Released bundle roots supplied to one isolated installation.
#[derive(Clone, Debug)]
pub struct ReleaseBundles {
  /// Extracted checksummed server release root.
  pub server: PathBuf,
  /// Extracted checksummed Agent release root.
  pub agent: PathBuf,
  /// Extracted checksummed Octa release root.
  pub octa: PathBuf,
}

/// Validated paths and identities consumed by a release scenario.
#[derive(Clone, Debug, Serialize)]
pub struct InstalledRelease {
  /// Isolated installation root.
  pub root: PathBuf,
  /// Validated server release root.
  pub server_root: PathBuf,
  /// Validated server executable.
  pub server_binary: PathBuf,
  /// Validated Agent release root.
  pub agent_root: PathBuf,
  /// Validated Agent executable.
  pub agent_binary: PathBuf,
  /// Validated source-plugin registry.
  pub source_plugins: PathBuf,
  /// Validated Octa release root containing runner and task plugins.
  pub octa_root: PathBuf,
  /// Validated Octa runner executable.
  pub octa_runner: PathBuf,
  /// Octa runner platform advertised by its release manifest.
  pub octa_platform: String,
  /// Octa release version advertised by its runner manifest.
  pub octa_version: String,
  /// Validated server release manifest.
  pub server_manifest: ProductManifest,
  /// Validated Agent release manifest.
  pub agent_manifest: ProductManifest,
  /// Validated capabilities advertised by the Octa runner.
  pub octa_capabilities: RunnerCapabilities,
  /// Digests and protocol versions used to issue signed Job specifications.
  pub toolchain: InstalledToolchain,
}

/// Verified toolchain identity derived from the installed release bundles.
#[derive(Clone, Debug, Serialize)]
pub struct InstalledToolchain {
  /// Git source-plugin release version.
  pub source_version: String,
  /// Git source-plugin executable digest.
  pub source_digest: String,
  /// Octa release version.
  pub octa_version: String,
  /// Octa runner executable digest.
  pub runner_digest: String,
  /// Selected Octa runner protocol.
  pub runner_protocol: u16,
  /// Selected Octa event schema.
  pub event_schema: u16,
  /// Protocol shared by every installed Octa task plugin.
  pub plugin_protocol: u16,
  /// Installed Octa task-plugin executable digests by logical name.
  pub plugin_digests: BTreeMap<String, String>,
}

/// Failure to install or validate released bundles.
#[derive(Debug, Error)]
pub enum HarnessError {
  /// A filesystem operation failed.
  #[error("release harness I/O failed for '{path}': {source}")]
  Io {
    /// Path being accessed.
    path: PathBuf,
    /// Underlying filesystem error.
    source: std::io::Error,
  },
  /// A bundle or compatibility invariant was violated.
  #[error("invalid released bundle: {0}")]
  Invalid(String),
  /// A release JSON document was invalid.
  #[error("invalid release JSON: {0}")]
  Json(#[from] serde_json::Error),
  /// An Octa plugin lock was invalid.
  #[error("invalid Octa.lock: {0}")]
  PluginLock(#[from] serde_yaml_ng::Error),
  /// An Agent source-plugin manifest was invalid.
  #[error("invalid source-plugin manifest: {0}")]
  SourceManifest(#[from] toml::de::Error),
}

/// Installs three released bundles into a fresh isolated directory.
pub fn install(bundles: &ReleaseBundles, destination: &Path) -> Result<InstalledRelease, HarnessError> {
  if destination.exists() {
    return Err(invalid(format!(
      "installation destination already exists: {}",
      destination.display()
    )));
  }
  let canonical_contract = ReleaseContract::canonical()?;
  let server_source = Bundle::load(&bundles.server, "octacity-server", &canonical_contract)?;
  let agent_source = Bundle::load(&bundles.agent, "octacity-agent", &canonical_contract)?;
  let octa_source = OctaBundle::load(&bundles.octa)?;
  validate_compatibility(&server_source.manifest, &agent_source.manifest, &octa_source)?;

  create_directory(destination)?;
  let mut guard = InstallationGuard::new(destination);
  let server_root = destination.join("server");
  let agent_root = destination.join("agent");
  let octa_root = destination.join("octa");
  server_source.install(&server_root)?;
  agent_source.install(&agent_root)?;
  octa_source.install(&octa_root)?;

  let installed = load_verified(
    &ReleaseBundles {
      server: server_root,
      agent: agent_root,
      octa: octa_root,
    },
    destination.to_owned(),
  )?;
  guard.commit();
  Ok(installed)
}

/// Revalidates an already isolated installation and returns its typed identity.
pub fn validate_installed(bundles: &ReleaseBundles) -> Result<InstalledRelease, HarnessError> {
  let root = common_parent(bundles)?;
  load_verified(bundles, root)
}

fn load_verified(bundles: &ReleaseBundles, root: PathBuf) -> Result<InstalledRelease, HarnessError> {
  let canonical_contract = ReleaseContract::canonical()?;
  let server = Bundle::load(&bundles.server, "octacity-server", &canonical_contract)?;
  let agent = Bundle::load(&bundles.agent, "octacity-agent", &canonical_contract)?;
  let octa = OctaBundle::load(&bundles.octa)?;
  validate_compatibility(&server.manifest, &agent.manifest, &octa)?;
  server.verify_binary_version("server")?;
  agent.verify_binary_version("agent")?;

  let source = agent.source_plugin()?;
  let runner_protocol = octa.compatible_runner_protocol(agent.manifest.protocol("octa_runner")?)?;
  let event_schema = octa.compatible_event_schema(agent.manifest.protocol("octa_event_schema")?)?;
  let toolchain = InstalledToolchain {
    source_version: source.version.clone(),
    source_digest: source.sha256.clone(),
    octa_version: octa.capabilities.octa_version.clone(),
    runner_digest: octa.runner_digest.clone(),
    runner_protocol,
    event_schema,
    plugin_protocol: octa.plugin_protocol,
    plugin_digests: octa.plugin_digests.clone(),
  };
  Ok(InstalledRelease {
    root,
    server_root: bundles.server.clone(),
    server_binary: server.component("server")?,
    agent_root: bundles.agent.clone(),
    agent_binary: agent.component("agent")?,
    source_plugins: bundles.agent.join("source-plugins"),
    octa_root: bundles.octa.clone(),
    octa_runner: octa.runner.clone(),
    octa_platform: octa.capabilities.platform.clone(),
    octa_version: octa.capabilities.octa_version.clone(),
    server_manifest: server.manifest,
    agent_manifest: agent.manifest,
    octa_capabilities: octa.capabilities,
    toolchain,
  })
}

fn validate_compatibility(
  server: &ProductManifest,
  agent: &ProductManifest,
  octa: &OctaBundle,
) -> Result<(), HarnessError> {
  if server.version != agent.version {
    return Err(invalid("server and Agent release versions differ"));
  }
  if server.platform != agent.platform {
    return Err(invalid("server and Agent release platforms differ"));
  }
  if server.build_inputs.octacity_revision != agent.build_inputs.octacity_revision {
    return Err(invalid("server and Agent were built from different OctaCity revisions"));
  }
  for protocol in ["agent", "artifact", "coordinator"] {
    if !server.protocol(protocol)?.overlaps(agent.protocol(protocol)?) {
      return Err(invalid(format!(
        "server and Agent have incompatible {protocol} protocol ranges"
      )));
    }
  }
  octa.compatible_runner_protocol(agent.protocol("octa_runner")?)?;
  octa.compatible_event_schema(agent.protocol("octa_event_schema")?)?;
  if octa.capabilities.platform != octa_runtime_platform(&agent.platform)? {
    return Err(invalid("Agent and Octa bundles target different runtime platforms"));
  }
  let expected_octa_revision = agent
    .build_inputs
    .octa_revision
    .as_deref()
    .ok_or_else(|| invalid("Agent manifest does not identify its Octa revision"))?;
  let actual = octa
    .capabilities
    .build_commit
    .as_deref()
    .ok_or_else(|| invalid("Octa runner capabilities do not identify their source revision"))?;
  if actual != expected_octa_revision {
    return Err(invalid(
      "Agent and Octa bundles were built from different Octa revisions",
    ));
  }
  Ok(())
}

fn common_parent(bundles: &ReleaseBundles) -> Result<PathBuf, HarnessError> {
  let root = bundles
    .server
    .parent()
    .ok_or_else(|| invalid("installed server root has no parent directory"))?;
  if bundles.agent.parent() != Some(root) || bundles.octa.parent() != Some(root) {
    return Err(invalid(
      "installed release roots do not share one installation directory",
    ));
  }
  Ok(root.to_owned())
}

fn create_directory(path: &Path) -> Result<(), HarnessError> {
  fs::create_dir(path).map_err(|source| HarnessError::Io {
    path: path.to_owned(),
    source,
  })
}

pub(crate) fn invalid(message: impl Into<String>) -> HarnessError {
  HarnessError::Invalid(message.into())
}

struct InstallationGuard<'a> {
  path: &'a Path,
  committed: bool,
}

impl<'a> InstallationGuard<'a> {
  const fn new(path: &'a Path) -> Self {
    Self { path, committed: false }
  }

  fn commit(&mut self) {
    self.committed = true;
  }
}

impl Drop for InstallationGuard<'_> {
  fn drop(&mut self) {
    if !self.committed {
      let _ = fs::remove_dir_all(self.path);
    }
  }
}
