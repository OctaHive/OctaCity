//! Inventories an operator-installed Octa release before it can execute jobs.
//!
//! The inventory binds the runner binary, its release-time capabilities
//! manifest, and every locked plugin to immutable digests. Reading a manifest
//! is essential when the host and execution guest use different binary formats,
//! as on macOS with a Linux Microsandbox guest. A signed job is accepted only
//! if its exact Octa requirement matches this local inventory; the live runner
//! confirms protocol compatibility again during its `Hello` handshake.

use std::{
  collections::BTreeMap,
  fs,
  path::{Path, PathBuf},
};

use octa_plugin_lock::{PLUGIN_LOCK_VERSION, PluginLock};
use octacity_execution::RunnerProgram;
use octacity_protocol::OctaSpec;
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use tracing::{debug, info};

use crate::protocol::RunnerMessage;

const MAX_CAPABILITIES_BYTES: usize = 1024 * 1024;
const MAX_PLUGIN_LOCK_BYTES: u64 = 1024 * 1024;
const RUNNER_CAPABILITIES_FILE: &str = "octa-runner-capabilities.json";

/// Capabilities recorded alongside the installed `octa-runner` executable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunnerCapabilities {
  /// Octa release version.
  pub octa_version: String,
  /// Supported runner protocol versions.
  pub runner_protocols: Vec<u16>,
  /// Supported event schema versions.
  pub event_schemas: Vec<u16>,
  /// Supported task-plugin protocol versions.
  pub plugin_protocols: Vec<u16>,
  /// Supported Octafile schema versions.
  pub octafile_versions: Vec<u8>,
  /// Guest platform for which the runner was built.
  pub platform: String,
  /// Optional compiled feature identifiers.
  pub features: Vec<String>,
  /// Optional source commit recorded at build time.
  pub build_commit: Option<String>,
}

/// Verified runner executable and plugin bundle available to the agent.
#[derive(Clone, Debug)]
pub struct RunnerInstallation {
  /// Canonical release root.
  pub root: PathBuf,
  /// Verified runner executable.
  pub executable: PathBuf,
  /// Verified plugin directory.
  pub plugins_dir: PathBuf,
  /// Default shared-schema `Octa.lock` path.
  pub default_plugin_lock: PathBuf,
  /// Runner executable SHA-256 digest.
  pub sha256: String,
  /// Verified release capability manifest.
  pub capabilities: RunnerCapabilities,
  /// Plugins verified from the default lock file.
  pub plugins: BTreeMap<String, RunnerPlugin>,
}

/// One plugin verified against the installed `Octa.lock`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunnerPlugin {
  /// Locked plugin version.
  pub version: String,
  /// Locked process-protocol version.
  pub protocol: u16,
  /// Host platforms declared by the lock file.
  pub platforms: Vec<String>,
  /// Canonical executable path.
  pub executable: PathBuf,
  /// Verified executable digest.
  pub sha256: String,
  /// Semantic plugin capabilities.
  pub capabilities: Vec<String>,
}

#[derive(Debug, Error)]
/// Failure while inventorying or matching an installed Octa release.
pub enum RunnerInstallationError {
  #[error("invalid Octa installation: {0}")]
  /// The on-disk release layout violates an installation invariant.
  Invalid(String),
  /// An installed release file could not be hashed.
  #[error("failed to hash installed file '{path}': {source}")]
  Hash {
    /// Installed file whose digest could not be computed.
    path: PathBuf,
    /// Underlying filesystem error.
    source: std::io::Error,
  },
  #[error("Octa runner capabilities manifest contains invalid JSON: {0}")]
  /// The capabilities manifest is not valid JSON.
  Json(#[source] Box<serde_json::Error>),
  #[error("failed to parse the installed Octa.lock: {0}")]
  /// The default plugin lock is not valid YAML.
  PluginLock(#[source] Box<serde_yaml_ng::Error>),
  #[error("Octa runner capabilities manifest is invalid: {0}")]
  /// The decoded capabilities violate a release invariant.
  Capabilities(String),
  #[error("installed Octa does not satisfy the signed job: {0}")]
  /// The installed release differs from the version or digests in the job.
  Requirement(String),
}

impl RunnerInstallation {
  /// Builds a trusted inventory from an operator-controlled release directory.
  pub fn load(root: &Path) -> Result<Self, RunnerInstallationError> {
    let root = canonical_directory("octa_release_root", root)?;
    let executable = root.join(runner_filename());
    validate_regular_file("octa-runner", &executable, true)?;
    let plugins_dir = root.join("plugins");
    let plugins_dir = canonical_directory("Octa plugins directory", &plugins_dir)?;
    let default_plugin_lock = root.join("Octa.lock");
    validate_regular_file("default plugin lock", &default_plugin_lock, false)?;
    let default_plugin_lock = default_plugin_lock
      .canonicalize()
      .map_err(|error| invalid(format!("default plugin lock: {error}")))?;
    let sha256 = file_sha256(&executable)?;
    let capabilities_path = root.join(RUNNER_CAPABILITIES_FILE);
    validate_regular_file("runner capabilities manifest", &capabilities_path, false)?;
    debug!(runner = %executable.display(), sha256 = %sha256, manifest = %capabilities_path.display(), "inventorying Octa runner");
    let capabilities = load_capabilities(&capabilities_path)?;
    let plugins = load_plugins(&default_plugin_lock, &plugins_dir, &capabilities)?;
    info!(
      version = %capabilities.octa_version,
      platform = %capabilities.platform,
      runner_sha256 = %sha256,
      "validated Octa installation"
    );
    Ok(Self {
      root,
      executable,
      plugins_dir,
      default_plugin_lock,
      sha256,
      capabilities,
      plugins,
    })
  }

  /// Matches the inventory against the exact requirement signed by the server.
  pub fn verify(&self, requirement: &OctaSpec) -> Result<(), RunnerInstallationError> {
    if self.capabilities.octa_version != requirement.version {
      return requirement_error(format!(
        "installed version '{}' differs from required version '{}'",
        self.capabilities.octa_version, requirement.version
      ));
    }
    if self.sha256 != requirement.runner_sha256 {
      return requirement_error("octa-runner SHA-256 differs from the required digest");
    }
    for (name, required, supported) in [
      (
        "runner protocol",
        requirement.runner_protocol,
        &self.capabilities.runner_protocols,
      ),
      (
        "event schema",
        requirement.event_schema,
        &self.capabilities.event_schemas,
      ),
      (
        "plugin protocol",
        requirement.plugin_protocol,
        &self.capabilities.plugin_protocols,
      ),
    ] {
      if !supported.contains(&required) {
        return requirement_error(format!("{name} {required} is not supported"));
      }
    }
    if requirement.plugin_digests.len() != self.plugins.len() {
      return requirement_error("signed plugin set differs from the installed Octa.lock");
    }
    for (name, plugin) in &self.plugins {
      let required_digest = requirement
        .plugin_digests
        .get(name)
        .ok_or_else(|| RunnerInstallationError::Requirement(format!("plugin '{name}' is not signed for this job")))?;
      if required_digest != &plugin.sha256 {
        return requirement_error(format!("plugin '{name}' SHA-256 differs from the required digest"));
      }
      if plugin.protocol != requirement.plugin_protocol {
        return requirement_error(format!(
          "plugin '{name}' uses protocol {}, expected {}",
          plugin.protocol, requirement.plugin_protocol
        ));
      }
    }
    Ok(())
  }

  /// Returns the verified host paths that an execution backend may expose to
  /// the runner. Protocol requirements remain owned by this inventory.
  pub fn program(&self) -> RunnerProgram {
    RunnerProgram {
      release_root: self.root.clone(),
      executable: self.executable.clone(),
      plugins_dir: self.plugins_dir.clone(),
      plugin_lock: self.default_plugin_lock.clone(),
    }
  }
}

fn load_plugins(
  lock_path: &Path,
  plugins_dir: &Path,
  runner: &RunnerCapabilities,
) -> Result<BTreeMap<String, RunnerPlugin>, RunnerInstallationError> {
  let metadata = fs::metadata(lock_path).map_err(|error| invalid(format!("Octa.lock: {error}")))?;
  if metadata.len() > MAX_PLUGIN_LOCK_BYTES {
    return Err(invalid(format!(
      "Octa.lock exceeds the {MAX_PLUGIN_LOCK_BYTES}-byte limit"
    )));
  }
  let contents = fs::read_to_string(lock_path).map_err(|error| invalid(format!("Octa.lock: {error}")))?;
  let lock: PluginLock =
    serde_yaml_ng::from_str(&contents).map_err(|error| RunnerInstallationError::PluginLock(Box::new(error)))?;
  if lock.version != PLUGIN_LOCK_VERSION {
    return Err(invalid(format!("unsupported Octa.lock version {}", lock.version)));
  }

  let mut plugins = BTreeMap::new();
  for (name, locked) in lock.plugins {
    if !logical_name(&name) || locked.version.trim().is_empty() {
      return Err(invalid(format!("plugin '{name}' has an invalid name or version")));
    }
    if !runner.plugin_protocols.contains(&locked.protocol) {
      return Err(invalid(format!(
        "plugin '{name}' uses unsupported protocol {}",
        locked.protocol
      )));
    }
    if !locked.platforms.iter().any(|platform| platform == &runner.platform) {
      return Err(invalid(format!(
        "plugin '{name}' does not support runner platform '{}'",
        runner.platform
      )));
    }
    if locked
      .capabilities
      .iter()
      .any(|capability| capability.trim().is_empty())
      || has_duplicates(&locked.capabilities)
    {
      return Err(invalid(format!("plugin '{name}' has invalid capabilities")));
    }
    if !relative_path(&locked.entrypoint) || !relative_path(Path::new(&locked.source)) {
      return Err(invalid(format!("plugin '{name}' has an unsafe distribution path")));
    }
    if !sha256_digest(&locked.sha256) {
      return Err(invalid(format!("plugin '{name}' has an invalid SHA-256")));
    }
    let executable = plugins_dir.join(&locked.entrypoint);
    validate_regular_file(&format!("plugin '{name}' executable"), &executable, true)?;
    let executable = executable
      .canonicalize()
      .map_err(|error| invalid(format!("plugin '{name}' executable: {error}")))?;
    if !executable.starts_with(plugins_dir) {
      return Err(invalid(format!(
        "plugin '{name}' executable escapes the plugin directory"
      )));
    }
    let digest = file_sha256(&executable)?;
    if digest != locked.sha256 {
      return Err(invalid(format!(
        "plugin '{name}' executable digest does not match Octa.lock"
      )));
    }
    plugins.insert(
      name,
      RunnerPlugin {
        version: locked.version,
        protocol: locked.protocol,
        platforms: locked.platforms,
        executable,
        sha256: digest,
        capabilities: locked.capabilities,
      },
    );
  }
  Ok(plugins)
}

fn load_capabilities(path: &Path) -> Result<RunnerCapabilities, RunnerInstallationError> {
  let metadata = fs::metadata(path).map_err(|error| invalid(format!("runner capabilities manifest: {error}")))?;
  if metadata.len() > MAX_CAPABILITIES_BYTES as u64 {
    return Err(invalid(format!(
      "runner capabilities manifest exceeds the {MAX_CAPABILITIES_BYTES}-byte limit"
    )));
  }
  let bytes = fs::read(path).map_err(|error| invalid(format!("runner capabilities manifest: {error}")))?;
  let message: RunnerMessage =
    serde_json::from_slice(&bytes).map_err(|error| RunnerInstallationError::Json(Box::new(error)))?;
  match message {
    RunnerMessage::Capabilities {
      octa_version,
      runner_protocols,
      event_schemas,
      plugin_protocols,
      octafile_versions,
      platform,
      features,
      build_commit,
    } => {
      let capabilities = RunnerCapabilities {
        octa_version,
        runner_protocols,
        event_schemas,
        plugin_protocols,
        octafile_versions,
        platform,
        features,
        build_commit,
      };
      validate_capabilities(&capabilities)?;
      Ok(capabilities)
    }
    _ => Err(RunnerInstallationError::Capabilities(
      "document is not a capabilities message".to_owned(),
    )),
  }
}

fn validate_capabilities(capabilities: &RunnerCapabilities) -> Result<(), RunnerInstallationError> {
  if capabilities.octa_version.trim().is_empty()
    || capabilities.platform.trim().is_empty()
    || capabilities.runner_protocols.is_empty()
    || capabilities.event_schemas.is_empty()
    || capabilities.plugin_protocols.is_empty()
    || capabilities.octafile_versions.is_empty()
  {
    return Err(RunnerInstallationError::Capabilities(
      "required capability values must not be empty".to_owned(),
    ));
  }
  for values in [
    &capabilities.runner_protocols,
    &capabilities.event_schemas,
    &capabilities.plugin_protocols,
  ] {
    if values.contains(&0) || has_duplicates(values) {
      return Err(RunnerInstallationError::Capabilities(
        "protocol versions must be non-zero and unique".to_owned(),
      ));
    }
  }
  if capabilities.octafile_versions.contains(&0) || has_duplicates(&capabilities.octafile_versions) {
    return Err(RunnerInstallationError::Capabilities(
      "Octafile versions must be non-zero and unique".to_owned(),
    ));
  }
  if capabilities.features.iter().any(|feature| feature.trim().is_empty()) || has_duplicates(&capabilities.features) {
    return Err(RunnerInstallationError::Capabilities(
      "features must be non-empty and unique".to_owned(),
    ));
  }
  Ok(())
}

fn has_duplicates<T: Ord + Clone>(values: &[T]) -> bool {
  let mut sorted = values.to_vec();
  sorted.sort();
  sorted.windows(2).any(|pair| pair[0] == pair[1])
}

fn canonical_directory(name: &str, path: &Path) -> Result<PathBuf, RunnerInstallationError> {
  let path = path
    .canonicalize()
    .map_err(|error| invalid(format!("{name} '{}': {error}", path.display())))?;
  if !path.is_dir() {
    return Err(invalid(format!("{name} '{}' is not a directory", path.display())));
  }
  validate_operator_permissions(
    name,
    &path,
    &fs::metadata(&path).map_err(|error| invalid(format!("{name}: {error}")))?,
  )?;
  Ok(path)
}

fn validate_regular_file(name: &str, path: &Path, executable: bool) -> Result<(), RunnerInstallationError> {
  let metadata =
    fs::symlink_metadata(path).map_err(|error| invalid(format!("{name} '{}': {error}", path.display())))?;
  if !metadata.file_type().is_file() {
    return Err(invalid(format!(
      "{name} '{}' must be a regular file, not a symlink",
      path.display()
    )));
  }
  validate_operator_permissions(name, path, &metadata)?;
  #[cfg(unix)]
  if executable {
    use std::os::unix::fs::PermissionsExt as _;
    if metadata.permissions().mode() & 0o111 == 0 {
      return Err(invalid(format!("{name} '{}' has no execute bit", path.display())));
    }
  }
  // Windows determines executability from the file type and association; it
  // has no Unix execute bit to validate for an installed runner or plugin.
  #[cfg(not(unix))]
  let _ = executable;
  Ok(())
}

fn validate_operator_permissions(
  name: &str,
  path: &Path,
  metadata: &fs::Metadata,
) -> Result<(), RunnerInstallationError> {
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    if metadata.permissions().mode() & 0o022 != 0 {
      return Err(invalid(format!(
        "{name} '{}' must not be writable by group or other users",
        path.display()
      )));
    }
  }
  #[cfg(not(unix))]
  let _ = (name, path, metadata);
  Ok(())
}

fn file_sha256(path: &Path) -> Result<String, RunnerInstallationError> {
  let mut file = fs::File::open(path).map_err(|source| RunnerInstallationError::Hash {
    path: path.to_owned(),
    source,
  })?;
  let mut hasher = Sha256::new();
  std::io::copy(&mut file, &mut hasher).map_err(|source| RunnerInstallationError::Hash {
    path: path.to_owned(),
    source,
  })?;
  Ok(format!("{:x}", hasher.finalize()))
}

fn runner_filename() -> &'static str {
  if cfg!(windows) {
    "octa-runner.exe"
  } else {
    "octa-runner"
  }
}

fn logical_name(value: &str) -> bool {
  !value.is_empty()
    && value
      .bytes()
      .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
}

fn relative_path(path: &Path) -> bool {
  use std::path::Component;

  !path.as_os_str().is_empty()
    && path
      .components()
      .all(|component| matches!(component, Component::Normal(_)))
    && path
      .to_str()
      .is_some_and(|value| !value.contains(['\\', ':']) && !value.chars().any(char::is_control))
}

fn sha256_digest(value: &str) -> bool {
  value.len() == 64
    && value
      .bytes()
      .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn invalid(message: impl Into<String>) -> RunnerInstallationError {
  RunnerInstallationError::Invalid(message.into())
}

fn requirement_error<T>(message: impl Into<String>) -> Result<T, RunnerInstallationError> {
  Err(RunnerInstallationError::Requirement(message.into()))
}

#[cfg(test)]
#[path = "installation_tests.rs"]
mod tests;
