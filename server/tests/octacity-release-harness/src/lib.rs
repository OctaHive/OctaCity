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

use octacity_protocol::OctaSpec;
use octacity_server_job::{
  JobSpecToolchainPolicy, JobSpecValidity, MAX_JOB_SPEC_TOOLCHAIN_POLICY_BYTES, SourcePluginPolicy,
};
use serde::Serialize;
use thiserror::Error;

use bundle::{Bundle, octa_runtime_platform, read_bounded, runtime_platform};
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

/// Released Agent and Octa bundle roots supplied to a portable Agent scenario.
#[derive(Clone, Debug)]
pub struct AgentRuntimeBundles {
  /// Extracted checksummed Agent release root.
  pub agent: PathBuf,
  /// Extracted checksummed Octa release root.
  pub octa: PathBuf,
}

/// Exact release-platform identities expected from one staged Agent runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentRuntimePlatforms {
  agent_release: String,
  octa_runner: String,
}

impl AgentRuntimePlatforms {
  /// Creates the platform expectation checked before a signing policy is emitted.
  #[must_use]
  pub fn new(agent_release: impl Into<String>, octa_runner: impl Into<String>) -> Self {
    Self {
      agent_release: agent_release.into(),
      octa_runner: octa_runner.into(),
    }
  }
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

/// Validated released Agent runtime used by portable host-mode scenarios.
#[derive(Clone, Debug, Serialize)]
pub struct InstalledAgentRuntime {
  /// Isolated installation root.
  pub root: PathBuf,
  /// Validated Agent release root.
  pub agent_root: PathBuf,
  /// Validated Agent executable.
  pub agent_binary: PathBuf,
  /// Validated source-plugin registry.
  pub source_plugins: PathBuf,
  /// Validated Octa release root containing runner and task plugins.
  pub octa_root: PathBuf,
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

impl InstalledToolchain {
  /// Converts the verified executor identities into the server's signing policy.
  pub fn job_spec_policy(&self, validity_seconds: u64) -> Result<JobSpecToolchainPolicy, HarnessError> {
    let source = SourcePluginPolicy::new("git", &self.source_version, &self.source_digest, "url")
      .map_err(|_| invalid("derived source-plugin policy is invalid"))?;
    let validity =
      JobSpecValidity::new(validity_seconds).map_err(|_| invalid("JobSpec validity interval is invalid"))?;
    let policy = JobSpecToolchainPolicy {
      source,
      octa: OctaSpec {
        version: self.octa_version.clone(),
        runner_sha256: self.runner_digest.clone(),
        runner_protocol: self.runner_protocol,
        event_schema: self.event_schema,
        plugin_protocol: self.plugin_protocol,
        plugin_digests: self.plugin_digests.clone(),
      },
      validity,
    };
    policy
      .validate()
      .map_err(|_| invalid("derived JobSpec policy is invalid"))?;
    Ok(policy)
  }
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

/// Installs and revalidates only the released Agent and its Octa toolchain.
///
/// Portable host-mode gates use a narrow in-process coordinator and therefore
/// do not need to build or install a same-platform server executable.
pub fn install_agent_runtime(
  bundles: &AgentRuntimeBundles,
  destination: &Path,
) -> Result<InstalledAgentRuntime, HarnessError> {
  if destination.exists() {
    return Err(invalid(format!(
      "installation destination already exists: {}",
      destination.display()
    )));
  }
  let canonical_contract = ReleaseContract::canonical()?;
  let agent_source = Bundle::load(&bundles.agent, "octacity-agent", &canonical_contract)?;
  let octa_source = OctaBundle::load(&bundles.octa)?;
  let platforms = AgentRuntimePlatforms::new(
    &agent_source.manifest.platform,
    runtime_platform(&agent_source.manifest.platform)?,
  );
  validate_agent_runtime_platforms(&agent_source.manifest, &octa_source, &platforms)?;

  create_directory(destination)?;
  let mut guard = InstallationGuard::new(destination);
  let agent_root = destination.join("agent");
  let octa_root = destination.join("octa");
  agent_source.install(&agent_root)?;
  octa_source.install(&octa_root)?;

  let installed = load_agent_runtime(
    &AgentRuntimeBundles {
      agent: agent_root,
      octa: octa_root,
    },
    destination.to_owned(),
    &platforms,
  )?;
  guard.commit();
  Ok(installed)
}

/// Revalidates an already isolated Agent and Octa installation for the expected platforms.
pub fn validate_installed_agent_runtime(
  bundles: &AgentRuntimeBundles,
  platforms: &AgentRuntimePlatforms,
) -> Result<InstalledAgentRuntime, HarnessError> {
  let root = agent_runtime_common_parent(bundles)?;
  load_agent_runtime(bundles, root, platforms)
}

/// Derives the server's exact JobSpec signing policy from a verified Agent installation.
///
/// The same release manifests, runner capabilities, plugin lock, platforms,
/// protocols, and executable digests consumed by the Agent are revalidated
/// before any server policy is returned.
pub fn derive_job_spec_policy(
  bundles: &AgentRuntimeBundles,
  platforms: &AgentRuntimePlatforms,
  validity_seconds: u64,
) -> Result<JobSpecToolchainPolicy, HarnessError> {
  let installed = validate_installed_agent_runtime(bundles, platforms)?;
  installed.toolchain.job_spec_policy(validity_seconds)
}

/// Rejects a server JobSpec policy that differs from the verified Agent assets.
pub fn verify_job_spec_policy(
  bundles: &AgentRuntimeBundles,
  platforms: &AgentRuntimePlatforms,
  policy: &JobSpecToolchainPolicy,
  validity_seconds: u64,
) -> Result<(), HarnessError> {
  policy
    .validate()
    .map_err(|_| invalid("configured JobSpec policy is invalid"))?;
  let expected = derive_job_spec_policy(bundles, platforms, validity_seconds)?;
  if policy != &expected {
    return Err(invalid(
      "server JobSpec policy differs from the verified Agent toolchain",
    ));
  }
  Ok(())
}

/// Loads and validates one bounded server JobSpec policy document.
pub fn load_job_spec_policy(path: &Path) -> Result<JobSpecToolchainPolicy, HarnessError> {
  let contents = read_bounded(path, MAX_JOB_SPEC_TOOLCHAIN_POLICY_BYTES)?;
  let policy: JobSpecToolchainPolicy = serde_json::from_slice(&contents)?;
  policy
    .validate()
    .map_err(|_| invalid("configured JobSpec policy is invalid"))?;
  Ok(policy)
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
  let toolchain = installed_toolchain(&agent.manifest, &octa, source)?;
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

fn load_agent_runtime(
  bundles: &AgentRuntimeBundles,
  root: PathBuf,
  platforms: &AgentRuntimePlatforms,
) -> Result<InstalledAgentRuntime, HarnessError> {
  let canonical_contract = ReleaseContract::canonical()?;
  let agent = Bundle::load(&bundles.agent, "octacity-agent", &canonical_contract)?;
  let octa = OctaBundle::load(&bundles.octa)?;
  validate_agent_runtime_platforms(&agent.manifest, &octa, platforms)?;
  agent.verify_binary_version("agent")?;

  let source = agent.source_plugin()?;
  let toolchain = installed_toolchain(&agent.manifest, &octa, source)?;
  Ok(InstalledAgentRuntime {
    root,
    agent_root: bundles.agent.clone(),
    agent_binary: agent.component("agent")?,
    source_plugins: bundles.agent.join("source-plugins"),
    octa_root: bundles.octa.clone(),
    agent_manifest: agent.manifest,
    octa_capabilities: octa.capabilities,
    toolchain,
  })
}

fn validate_agent_runtime_platforms(
  agent: &ProductManifest,
  octa: &OctaBundle,
  platforms: &AgentRuntimePlatforms,
) -> Result<(), HarnessError> {
  if agent.platform != platforms.agent_release {
    return Err(invalid(
      "installed Agent runtime platforms differ from the signing policy expectation",
    ));
  }
  validate_agent_octa_compatibility(agent, octa, &platforms.octa_runner)
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
  validate_agent_octa_compatibility(agent, octa, octa_runtime_platform(&agent.platform)?)?;
  Ok(())
}

fn validate_agent_octa_compatibility(
  agent: &ProductManifest,
  octa: &OctaBundle,
  expected_octa_platform: &str,
) -> Result<(), HarnessError> {
  octa.compatible_runner_protocol(agent.protocol("octa_runner")?)?;
  octa.compatible_event_schema(agent.protocol("octa_event_schema")?)?;
  if octa.capabilities.platform != expected_octa_platform {
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

fn installed_toolchain(
  agent: &ProductManifest,
  octa: &OctaBundle,
  source: &octacity_source_plugin::SourcePluginManifest,
) -> Result<InstalledToolchain, HarnessError> {
  Ok(InstalledToolchain {
    source_version: source.version.clone(),
    source_digest: source.sha256.clone(),
    octa_version: octa.capabilities.octa_version.clone(),
    runner_digest: octa.runner_digest.clone(),
    runner_protocol: octa.compatible_runner_protocol(agent.protocol("octa_runner")?)?,
    event_schema: octa.compatible_event_schema(agent.protocol("octa_event_schema")?)?,
    plugin_protocol: octa.plugin_protocol,
    plugin_digests: octa.plugin_digests.clone(),
  })
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

fn agent_runtime_common_parent(bundles: &AgentRuntimeBundles) -> Result<PathBuf, HarnessError> {
  let root = bundles
    .agent
    .parent()
    .ok_or_else(|| invalid("installed Agent root has no parent directory"))?;
  if bundles.octa.parent() != Some(root) {
    return Err(invalid(
      "installed Agent and Octa roots do not share one installation directory",
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
