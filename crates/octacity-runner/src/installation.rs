//! Inventories an operator-installed Octa release before it can execute jobs.
//!
//! The inventory binds the runner binary, its advertised protocol support, and
//! every locked plugin to immutable digests. A signed job is accepted only if
//! its exact Octa requirement matches this local inventory.

use std::{
  collections::BTreeMap,
  fs,
  path::{Path, PathBuf},
  process::Stdio,
  time::Duration,
};

use octacity_execution::RunnerProgram;
use octacity_protocol::OctaSpec;
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use tokio::{
  io::{AsyncRead, AsyncReadExt as _},
  process::Command,
  time::timeout,
};
use tracing::{debug, info};

use crate::protocol::RunnerMessage;

const CAPABILITIES_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_CAPABILITIES_BYTES: usize = 1024 * 1024;
const MAX_CAPABILITIES_STDERR_BYTES: usize = 64 * 1024;
const MAX_PLUGIN_LOCK_BYTES: u64 = 1024 * 1024;
const PLUGIN_LOCK_VERSION: u8 = 1;

/// Capabilities reported by the installed `octa-runner` executable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunnerCapabilities {
  pub octa_version: String,
  pub runner_protocols: Vec<u16>,
  pub event_schemas: Vec<u16>,
  pub plugin_protocols: Vec<u16>,
  pub octafile_versions: Vec<u8>,
  pub platform: String,
  pub features: Vec<String>,
  pub build_commit: Option<String>,
}

/// Verified runner executable and plugin bundle available to the agent.
#[derive(Clone, Debug)]
pub struct RunnerInstallation {
  pub root: PathBuf,
  pub executable: PathBuf,
  pub plugins_dir: PathBuf,
  pub default_plugin_lock: PathBuf,
  pub sha256: String,
  pub capabilities: RunnerCapabilities,
  pub plugins: BTreeMap<String, RunnerPlugin>,
}

/// One plugin verified against the installed `Octa.lock`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunnerPlugin {
  pub version: String,
  pub protocol: u16,
  pub platforms: Vec<String>,
  pub executable: PathBuf,
  pub sha256: String,
  pub capabilities: Vec<String>,
}

#[derive(Debug, Error)]
pub enum RunnerInstallationError {
  #[error("invalid Octa installation: {0}")]
  Invalid(String),
  #[error("failed to hash octa-runner '{path}': {source}")]
  Hash { path: PathBuf, source: std::io::Error },
  #[error("failed to start octa-runner capabilities: {0}")]
  Spawn(#[source] std::io::Error),
  #[error("octa-runner capabilities timed out")]
  TimedOut,
  #[error("octa-runner capabilities I/O failed: {0}")]
  Io(#[source] std::io::Error),
  #[error("octa-runner capabilities returned invalid JSON: {0}")]
  Json(#[source] Box<serde_json::Error>),
  #[error("failed to parse the installed Octa.lock: {0}")]
  PluginLock(#[source] Box<serde_yml::Error>),
  #[error("octa-runner capabilities failed: {0}")]
  Capabilities(String),
  #[error("installed Octa does not satisfy the signed job: {0}")]
  Requirement(String),
}

impl RunnerInstallation {
  /// Builds a trusted inventory from an operator-controlled release directory.
  pub async fn load(root: &Path) -> Result<Self, RunnerInstallationError> {
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
    debug!(runner = %executable.display(), sha256 = %sha256, "inspecting Octa runner capabilities");
    let capabilities = inspect_capabilities(&executable, &root).await?;
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
      executable: self.executable.clone(),
      plugins_dir: self.plugins_dir.clone(),
      plugin_lock: self.default_plugin_lock.clone(),
    }
  }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginLock {
  version: u8,
  plugins: BTreeMap<String, LockedPlugin>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LockedPlugin {
  version: String,
  protocol: u16,
  platforms: Vec<String>,
  entrypoint: PathBuf,
  sha256: String,
  #[serde(default)]
  capabilities: Vec<String>,
  source: String,
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
    serde_yml::from_str(&contents).map_err(|error| RunnerInstallationError::PluginLock(Box::new(error)))?;
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

async fn inspect_capabilities(executable: &Path, root: &Path) -> Result<RunnerCapabilities, RunnerInstallationError> {
  let mut command = Command::new(executable);
  command
    .arg("capabilities")
    .env_clear()
    .current_dir(root)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);
  #[cfg(unix)]
  command.process_group(0);
  let mut child = command.spawn().map_err(RunnerInstallationError::Spawn)?;
  let stdout = child
    .stdout
    .take()
    .ok_or_else(|| invalid("capabilities stdout was not piped"))?;
  let stderr = child
    .stderr
    .take()
    .ok_or_else(|| invalid("capabilities stderr was not piped"))?;
  let stdout_task = tokio::spawn(read_bounded(stdout, MAX_CAPABILITIES_BYTES));
  let stderr_task = tokio::spawn(read_bounded(stderr, MAX_CAPABILITIES_STDERR_BYTES));

  let status = match timeout(CAPABILITIES_TIMEOUT, child.wait()).await {
    Ok(status) => status.map_err(RunnerInstallationError::Io)?,
    Err(_) => {
      kill_process_group(&mut child);
      let _ = child.wait().await;
      return Err(RunnerInstallationError::TimedOut);
    }
  };
  let stdout = join_output(stdout_task).await?;
  let stderr = join_output(stderr_task).await?;
  if stdout.truncated || stderr.truncated {
    return Err(RunnerInstallationError::Capabilities(
      "output exceeded its bounded limit".to_owned(),
    ));
  }
  if !status.success() {
    return Err(RunnerInstallationError::Capabilities(format!(
      "process exited with {status}: {}",
      sanitize(&stderr.bytes)
    )));
  }

  let mut frames = stdout
    .bytes
    .split(|byte| *byte == b'\n')
    .filter(|frame| !frame.is_empty());
  let first = frames
    .next()
    .ok_or_else(|| RunnerInstallationError::Capabilities("no protocol message was emitted".to_owned()))?;
  if frames.next().is_some() {
    return Err(RunnerInstallationError::Capabilities(
      "more than one protocol message was emitted".to_owned(),
    ));
  }
  let message: RunnerMessage =
    serde_json::from_slice(first).map_err(|error| RunnerInstallationError::Json(Box::new(error)))?;
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
      "first message was not capabilities".to_owned(),
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

struct BoundedOutput {
  bytes: Vec<u8>,
  truncated: bool,
}

async fn read_bounded<R: AsyncRead + Unpin>(mut reader: R, limit: usize) -> Result<BoundedOutput, std::io::Error> {
  let mut bytes = Vec::with_capacity(limit.min(8192));
  let mut buffer = [0_u8; 8192];
  let mut truncated = false;
  loop {
    let read = reader.read(&mut buffer).await?;
    if read == 0 {
      return Ok(BoundedOutput { bytes, truncated });
    }
    let remaining = limit.saturating_sub(bytes.len());
    bytes.extend_from_slice(&buffer[..read.min(remaining)]);
    truncated |= read > remaining;
  }
}

async fn join_output(
  task: tokio::task::JoinHandle<Result<BoundedOutput, std::io::Error>>,
) -> Result<BoundedOutput, RunnerInstallationError> {
  task
    .await
    .map_err(|error| RunnerInstallationError::Io(std::io::Error::other(error)))?
    .map_err(RunnerInstallationError::Io)
}

fn sanitize(bytes: &[u8]) -> String {
  String::from_utf8_lossy(bytes)
    .chars()
    .filter(|character| *character == '\n' || *character == '\t' || !character.is_control())
    .take(MAX_CAPABILITIES_STDERR_BYTES)
    .collect::<String>()
    .trim_end()
    .to_owned()
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

fn kill_process_group(child: &mut tokio::process::Child) {
  #[cfg(unix)]
  if let Some(id) = child.id().and_then(|id| i32::try_from(id).ok()) {
    // SAFETY: capabilities starts as leader of a new process group.
    unsafe {
      libc::kill(-id, libc::SIGKILL);
    }
  }
  let _ = child.start_kill();
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::collections::BTreeMap;

  fn capabilities() -> RunnerCapabilities {
    RunnerCapabilities {
      octa_version: "0.3.0".to_owned(),
      runner_protocols: vec![1],
      event_schemas: vec![3],
      plugin_protocols: vec![1],
      octafile_versions: vec![1],
      platform: "linux-x86_64".to_owned(),
      features: vec!["versioned-events".to_owned()],
      build_commit: None,
    }
  }

  #[test]
  fn validates_capability_sets() {
    assert!(validate_capabilities(&capabilities()).is_ok());
    let mut invalid = capabilities();
    invalid.runner_protocols = vec![1, 1];
    assert!(validate_capabilities(&invalid).is_err());
    invalid = capabilities();
    invalid.features = vec![String::new()];
    assert!(validate_capabilities(&invalid).is_err());
    invalid = capabilities();
    invalid.octafile_versions = vec![0];
    assert!(validate_capabilities(&invalid).is_err());
  }

  #[test]
  fn validates_distribution_identifiers_and_paths() {
    assert!(logical_name("shell_2"));
    assert!(!logical_name("Shell"));
    assert!(relative_path(Path::new("bin/shell")));
    assert!(!relative_path(Path::new("../shell")));
    assert!(!relative_path(Path::new("bin\\shell")));
    assert!(sha256_digest(&"a".repeat(64)));
    assert!(!sha256_digest(&"A".repeat(64)));
  }

  #[cfg(unix)]
  #[test]
  fn rejects_operator_files_writable_by_other_users() {
    use std::os::unix::fs::PermissionsExt as _;

    let temporary = tempfile::tempdir().unwrap();
    let file = temporary.path().join("runner");
    fs::write(&file, "runner").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o777)).unwrap();
    assert!(
      validate_regular_file("runner", &file, true)
        .unwrap_err()
        .to_string()
        .contains("writable by group")
    );
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn inventories_and_matches_a_release_without_importing_octa() {
    use std::os::unix::fs::PermissionsExt as _;

    let release = tempfile::tempdir().unwrap();
    fs::create_dir(release.path().join("plugins")).unwrap();
    let plugin = release.path().join("plugins/shell");
    fs::write(&plugin, "fixture plugin").unwrap();
    fs::set_permissions(&plugin, fs::Permissions::from_mode(0o755)).unwrap();
    let plugin_digest = file_sha256(&plugin).unwrap();
    fs::write(
      release.path().join("Octa.lock"),
      format!(
        "version: 1\nplugins:\n  shell:\n    version: '0.3.0'\n    protocol: 1\n    platforms: [linux-x86_64]\n    entrypoint: shell\n    sha256: {plugin_digest}\n    capabilities: [shell]\n    source: shell.plugin.yml\n"
      ),
    )
    .unwrap();
    let runner = release.path().join(runner_filename());
    fs::write(
      &runner,
      "#!/bin/sh\nprintf '%s\\n' '{\"type\":\"capabilities\",\"octa_version\":\"0.3.0\",\"runner_protocols\":[1],\"event_schemas\":[3],\"plugin_protocols\":[1],\"octafile_versions\":[1],\"platform\":\"linux-x86_64\",\"features\":[\"versioned-events\"]}'\n",
    )
    .unwrap();
    fs::set_permissions(&runner, fs::Permissions::from_mode(0o755)).unwrap();

    let installation = RunnerInstallation::load(release.path()).await.unwrap();
    let requirement = OctaSpec {
      version: "0.3.0".to_owned(),
      runner_sha256: installation.sha256.clone(),
      runner_protocol: 1,
      event_schema: 3,
      plugin_protocol: 1,
      plugin_digests: BTreeMap::from([("shell".to_owned(), plugin_digest.clone())]),
    };
    installation.verify(&requirement).unwrap();
    assert_eq!(installation.plugins.len(), 1);
    assert_eq!(
      installation.program(),
      RunnerProgram {
        executable: installation.executable.clone(),
        plugins_dir: installation.plugins_dir.clone(),
        plugin_lock: installation.default_plugin_lock.clone(),
      }
    );

    let mut wrong = requirement;
    wrong.event_schema = 2;
    assert!(installation.verify(&wrong).is_err());
    wrong = OctaSpec {
      version: "0.2.0".to_owned(),
      runner_sha256: installation.sha256.clone(),
      runner_protocol: 1,
      event_schema: 3,
      plugin_protocol: 1,
      plugin_digests: BTreeMap::from([("shell".to_owned(), plugin_digest.clone())]),
    };
    assert!(installation.verify(&wrong).is_err());
    wrong.version = "0.3.0".to_owned();
    wrong.runner_sha256 = "f".repeat(64);
    assert!(installation.verify(&wrong).is_err());
    wrong.runner_sha256 = installation.sha256.clone();
    wrong.plugin_digests.clear();
    assert!(installation.verify(&wrong).is_err());
    wrong.plugin_digests.insert("shell".to_owned(), "f".repeat(64));
    assert!(installation.verify(&wrong).is_err());
    wrong.plugin_digests.insert("shell".to_owned(), plugin_digest);
    wrong.plugin_protocol = 2;
    assert!(installation.verify(&wrong).is_err());
  }
}
