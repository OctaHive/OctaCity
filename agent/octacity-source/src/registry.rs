//! Builds the agent's trusted inventory of operator-installed source plugins.
//!
//! This module owns the static side of source acquisition. At agent startup it
//! scans `source_plugins_dir`, validates directory permissions and manifests,
//! confines each entrypoint to its plugin directory, verifies its executable
//! digest, and records the resulting [`InstalledSourcePlugin`] values in a
//! [`SourcePluginRegistry`]. For a job, the registry resolves a signed
//! [`SourceSpec`] only when its logical name, version, and digest exactly match
//! the installed plugin.
//!
//! This module never starts a plugin or materializes a workspace. That dynamic
//! lifecycle is exposed through [`InstalledSourcePlugin::materialize`].
//! Repository content can select a verified logical plugin through signed job
//! data, but cannot register a binary, supply its host path, or change
//! operator-owned settings.

use std::{
  collections::BTreeMap,
  fs::{self, File},
  io::{self, Read as _},
  path::{Path, PathBuf},
};

use octacity_protocol::SourceSpec;
use octacity_source_plugin::{SOURCE_PLUGIN_MANIFEST_VERSION, SOURCE_PLUGIN_PROTOCOL_VERSION, SourcePluginManifest};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use tracing::{debug, info};

const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

/// Immutable source-plugin inventory constructed during agent startup.
#[derive(Debug)]
pub struct SourcePluginRegistry {
  plugins: BTreeMap<String, InstalledSourcePlugin>,
}

/// Manifest and canonical executable for one verified plugin.
#[derive(Debug)]
pub struct InstalledSourcePlugin {
  /// Manifest whose identity and executable digest were verified at discovery.
  pub manifest: SourcePluginManifest,
  /// Canonical executable confined to the plugin directory.
  pub executable: PathBuf,
}

#[derive(Debug, Error)]
/// Failure while building or querying the trusted plugin inventory.
pub enum RegistryError {
  /// The registry root could not be enumerated.
  #[error("failed to read source plugin registry '{path}': {source}")]
  ReadDirectory {
    /// Registry root that could not be read.
    path: PathBuf,
    /// Underlying filesystem error.
    source: io::Error,
  },
  /// A registry entry violates the trusted layout or permission rules.
  #[error("invalid source plugin registry entry '{path}': {message}")]
  InvalidEntry {
    /// Invalid registry entry or file.
    path: PathBuf,
    /// Violated registry invariant.
    message: String,
  },
  /// An installed plugin manifest could not be read.
  #[error("failed to read source plugin manifest '{path}': {source}")]
  ReadManifest {
    /// Manifest path that could not be read.
    path: PathBuf,
    /// Underlying filesystem error.
    source: io::Error,
  },
  /// An installed plugin manifest is not valid TOML.
  #[error("failed to parse source plugin manifest '{path}': {source}")]
  ParseManifest {
    /// Manifest path being parsed.
    path: PathBuf,
    /// TOML syntax or deserialization error.
    source: Box<toml::de::Error>,
  },
  /// An installed plugin executable could not be hashed.
  #[error("failed to hash source plugin executable '{path}': {source}")]
  HashExecutable {
    /// Executable whose digest could not be computed.
    path: PathBuf,
    /// Underlying filesystem error.
    source: io::Error,
  },
  /// A signed job requires a plugin absent from the inventory.
  #[error("source plugin '{name}' is not installed")]
  NotInstalled {
    /// Required logical plugin name.
    name: String,
  },
  /// The installed plugin does not match the signed version or digest.
  #[error("source plugin '{name}' does not match the signed job requirement: {message}")]
  Requirement {
    /// Required logical plugin name.
    name: String,
    /// Version or digest mismatch.
    message: String,
  },
}

impl SourcePluginRegistry {
  /// Scans and verifies every entry below the operator-owned registry root.
  pub fn discover(root: &Path) -> Result<Self, RegistryError> {
    debug!(registry = %root.display(), "discovering source plugins");
    validate_directory(root)?;
    validate_permissions(root, false)?;
    let entries = fs::read_dir(root).map_err(|source| RegistryError::ReadDirectory {
      path: root.to_owned(),
      source,
    })?;
    let mut paths = entries
      .collect::<Result<Vec<_>, _>>()
      .map_err(|source| RegistryError::ReadDirectory {
        path: root.to_owned(),
        source,
      })?;
    paths.sort_by_key(std::fs::DirEntry::file_name);

    let mut plugins = BTreeMap::new();
    for entry in paths {
      let path = entry.path();
      let file_type = entry
        .file_type()
        .map_err(|error| invalid_entry(&path, error.to_string()))?;
      if !file_type.is_dir() || file_type.is_symlink() {
        return Err(invalid_entry(&path, "registry entries must be real directories"));
      }
      validate_permissions(&path, false)?;
      let installed = load_plugin(&path)?;
      let name = installed.manifest.name.clone();
      debug!(
        plugin = %name,
        version = %installed.manifest.version,
        executable = %installed.executable.display(),
        "validated source plugin"
      );
      plugins.insert(name, installed);
    }
    info!(registry = %root.display(), plugins = plugins.len(), "loaded source plugin registry");
    Ok(Self { plugins })
  }

  /// Returns the number of verified installed plugins.
  pub fn len(&self) -> usize {
    self.plugins.len()
  }

  /// Returns whether the verified inventory is empty.
  pub fn is_empty(&self) -> bool {
    self.plugins.is_empty()
  }

  /// Looks up a verified plugin by its logical name.
  pub fn get(&self, name: &str) -> Option<&InstalledSourcePlugin> {
    self.plugins.get(name)
  }

  /// Iterates over verified plugins in deterministic logical-name order.
  pub fn iter(&self) -> impl Iterator<Item = (&str, &InstalledSourcePlugin)> {
    self.plugins.iter().map(|(name, plugin)| (name.as_str(), plugin))
  }

  /// Resolves the exact plugin version and digest required by a signed job.
  pub fn resolve(&self, requirement: &SourceSpec) -> Result<&InstalledSourcePlugin, RegistryError> {
    let plugin = self
      .get(&requirement.provider)
      .ok_or_else(|| RegistryError::NotInstalled {
        name: requirement.provider.clone(),
      })?;
    if plugin.manifest.version != requirement.plugin_version {
      return Err(RegistryError::Requirement {
        name: requirement.provider.clone(),
        message: format!(
          "installed version '{}' differs from required version '{}'",
          plugin.manifest.version, requirement.plugin_version
        ),
      });
    }
    if plugin.manifest.sha256 != requirement.plugin_sha256 {
      return Err(RegistryError::Requirement {
        name: requirement.provider.clone(),
        message: "installed executable digest differs from the required digest".to_owned(),
      });
    }
    Ok(plugin)
  }
}

/// Requires a real directory independently of platform permission support.
fn validate_directory(path: &Path) -> Result<(), RegistryError> {
  let metadata = fs::symlink_metadata(path).map_err(|error| invalid_entry(path, error.to_string()))?;
  if !metadata.file_type().is_dir() {
    return Err(invalid_entry(
      path,
      "expected a real directory, not a symlink or special file",
    ));
  }
  Ok(())
}

fn load_plugin(directory: &Path) -> Result<InstalledSourcePlugin, RegistryError> {
  let manifest_path = directory.join("plugin.toml");
  validate_regular_file(&manifest_path, false)?;
  let metadata = fs::metadata(&manifest_path).map_err(|source| RegistryError::ReadManifest {
    path: manifest_path.clone(),
    source,
  })?;
  if metadata.len() > MAX_MANIFEST_BYTES {
    return Err(invalid_entry(
      &manifest_path,
      format!("manifest exceeds the {MAX_MANIFEST_BYTES}-byte limit"),
    ));
  }
  let contents = fs::read_to_string(&manifest_path).map_err(|source| RegistryError::ReadManifest {
    path: manifest_path.clone(),
    source,
  })?;
  let manifest = SourcePluginManifest::from_toml(&contents).map_err(|source| RegistryError::ParseManifest {
    path: manifest_path,
    source: Box::new(source),
  })?;
  validate_manifest(directory, &manifest)?;

  // Canonicalization plus the prefix check prevents a manifest symlink from
  // selecting an executable outside its operator-reviewed plugin directory.
  let executable = directory.join(&manifest.executable);
  validate_regular_file(&executable, true)?;
  let canonical_directory = directory
    .canonicalize()
    .map_err(|error| invalid_entry(directory, error.to_string()))?;
  let executable = executable
    .canonicalize()
    .map_err(|error| invalid_entry(&executable, error.to_string()))?;
  if !executable.starts_with(&canonical_directory) {
    return Err(invalid_entry(&executable, "executable escapes its plugin directory"));
  }
  let digest = file_sha256(&executable)?;
  if digest != manifest.sha256 {
    return Err(invalid_entry(
      &executable,
      "executable SHA-256 does not match plugin.toml",
    ));
  }
  Ok(InstalledSourcePlugin { manifest, executable })
}

fn validate_manifest(directory: &Path, manifest: &SourcePluginManifest) -> Result<(), RegistryError> {
  if manifest.manifest_version != SOURCE_PLUGIN_MANIFEST_VERSION {
    return Err(invalid_entry(directory, "unsupported manifest version"));
  }
  let directory_name = directory.file_name().and_then(|name| name.to_str());
  if !logical_name(&manifest.name) || directory_name != Some(manifest.name.as_str()) {
    return Err(invalid_entry(
      directory,
      "manifest name must be a logical name matching its directory",
    ));
  }
  if manifest.version.trim().is_empty() {
    return Err(invalid_entry(directory, "plugin version must not be empty"));
  }
  if manifest.protocol_min == 0
    || manifest.protocol_min > SOURCE_PLUGIN_PROTOCOL_VERSION
    || manifest.protocol_max < SOURCE_PLUGIN_PROTOCOL_VERSION
    || manifest.protocol_min > manifest.protocol_max
  {
    return Err(invalid_entry(directory, "plugin does not support source protocol v1"));
  }
  if !relative_wire_path(&manifest.executable) {
    return Err(invalid_entry(
      directory,
      "plugin executable must be a normalized relative path",
    ));
  }
  if !sha256_digest(&manifest.sha256) {
    return Err(invalid_entry(directory, "plugin SHA-256 must be lowercase hexadecimal"));
  }
  let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
  if !manifest.platforms.iter().any(|candidate| candidate == &platform) {
    return Err(invalid_entry(
      directory,
      format!("plugin does not support platform '{platform}'"),
    ));
  }
  Ok(())
}

fn validate_regular_file(path: &Path, executable: bool) -> Result<(), RegistryError> {
  let metadata = fs::symlink_metadata(path).map_err(|error| invalid_entry(path, error.to_string()))?;
  if !metadata.file_type().is_file() {
    return Err(invalid_entry(
      path,
      "expected a regular file, not a symlink or special file",
    ));
  }
  validate_permissions(path, executable)
}

#[cfg(unix)]
fn validate_permissions(path: &Path, executable: bool) -> Result<(), RegistryError> {
  use std::os::unix::fs::PermissionsExt as _;

  let mode = fs::metadata(path)
    .map_err(|error| invalid_entry(path, error.to_string()))?
    .permissions()
    .mode();
  if mode & 0o022 != 0 {
    return Err(invalid_entry(path, "must not be writable by group or others"));
  }
  if executable && mode & 0o111 == 0 {
    return Err(invalid_entry(path, "plugin executable has no execute bit"));
  }
  Ok(())
}

#[cfg(not(unix))]
fn validate_permissions(_path: &Path, _executable: bool) -> Result<(), RegistryError> {
  Ok(())
}

fn file_sha256(path: &Path) -> Result<String, RegistryError> {
  let mut file = File::open(path).map_err(|source| RegistryError::HashExecutable {
    path: path.to_owned(),
    source,
  })?;
  let mut hasher = Sha256::new();
  let mut buffer = [0_u8; 64 * 1024];
  loop {
    let read = file.read(&mut buffer).map_err(|source| RegistryError::HashExecutable {
      path: path.to_owned(),
      source,
    })?;
    if read == 0 {
      break;
    }
    hasher.update(&buffer[..read]);
  }
  Ok(format!("{:x}", hasher.finalize()))
}

fn logical_name(value: &str) -> bool {
  !value.is_empty()
    && value
      .bytes()
      .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
}

fn relative_wire_path(value: &str) -> bool {
  !value.is_empty()
    && !value.starts_with('/')
    && !value.contains(['\\', ':'])
    && !value.chars().any(char::is_control)
    && !value
      .split('/')
      .any(|part| part.is_empty() || part == "." || part == "..")
}

fn sha256_digest(value: &str) -> bool {
  value.len() == 64
    && value
      .bytes()
      .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn invalid_entry(path: &Path, message: impl Into<String>) -> RegistryError {
  RegistryError::InvalidEntry {
    path: path.to_owned(),
    message: message.into(),
  }
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
