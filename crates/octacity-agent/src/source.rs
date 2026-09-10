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

#[derive(Debug)]
pub struct SourcePluginRegistry {
  plugins: BTreeMap<String, InstalledSourcePlugin>,
}

#[derive(Debug)]
pub struct InstalledSourcePlugin {
  pub manifest: SourcePluginManifest,
  pub executable: PathBuf,
}

#[derive(Debug, Error)]
pub enum RegistryError {
  #[error("failed to read source plugin registry '{path}': {source}")]
  ReadDirectory { path: PathBuf, source: io::Error },
  #[error("invalid source plugin registry entry '{path}': {message}")]
  InvalidEntry { path: PathBuf, message: String },
  #[error("failed to read source plugin manifest '{path}': {source}")]
  ReadManifest { path: PathBuf, source: io::Error },
  #[error("failed to parse source plugin manifest '{path}': {source}")]
  ParseManifest {
    path: PathBuf,
    source: Box<toml::de::Error>,
  },
  #[error("failed to hash source plugin executable '{path}': {source}")]
  HashExecutable { path: PathBuf, source: io::Error },
  #[error("source plugin '{name}' is not installed")]
  NotInstalled { name: String },
  #[error("source plugin '{name}' does not match the signed job requirement: {message}")]
  Requirement { name: String, message: String },
}

impl SourcePluginRegistry {
  pub fn discover(root: &Path) -> Result<Self, RegistryError> {
    debug!(registry = %root.display(), "discovering source plugins");
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

  pub fn len(&self) -> usize {
    self.plugins.len()
  }

  pub fn is_empty(&self) -> bool {
    self.plugins.is_empty()
  }

  pub fn get(&self, name: &str) -> Option<&InstalledSourcePlugin> {
    self.plugins.get(name)
  }

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
mod tests {
  use std::io::Write as _;

  use super::*;

  fn plugin_fixture() -> (tempfile::TempDir, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("git");
    fs::create_dir(&directory).unwrap();
    let executable = directory.join(if cfg!(windows) { "git.exe" } else { "git" });
    File::create(&executable).unwrap().write_all(b"plugin").unwrap();
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt as _;
      fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let digest = file_sha256(&executable).unwrap();
    let manifest = format!(
      r#"manifest_version = 1
name = "git"
version = "0.1.0"
protocol_min = 1
protocol_max = 1
executable = "{}"
sha256 = "{digest}"
platforms = ["{}-{}"]
"#,
      executable.file_name().unwrap().to_string_lossy(),
      std::env::consts::OS,
      std::env::consts::ARCH
    );
    fs::write(directory.join("plugin.toml"), manifest).unwrap();
    (root, executable)
  }

  #[test]
  fn discovers_a_verified_plugin() {
    let (root, executable) = plugin_fixture();
    let registry = SourcePluginRegistry::discover(root.path()).unwrap();
    assert_eq!(registry.len(), 1);
    assert_eq!(
      registry.get("git").unwrap().executable,
      executable.canonicalize().unwrap()
    );
  }

  #[test]
  fn rejects_a_digest_mismatch() {
    let (root, executable) = plugin_fixture();
    File::options()
      .append(true)
      .open(executable)
      .unwrap()
      .write_all(b"changed")
      .unwrap();
    assert!(
      SourcePluginRegistry::discover(root.path())
        .unwrap_err()
        .to_string()
        .contains("SHA-256")
    );
  }

  #[test]
  fn rejects_unexpected_registry_files() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("README"), "not a plugin").unwrap();
    assert!(
      SourcePluginRegistry::discover(root.path())
        .unwrap_err()
        .to_string()
        .contains("real directories")
    );
  }

  #[test]
  fn resolves_only_the_exact_signed_plugin_requirement() {
    let (root, _) = plugin_fixture();
    let registry = SourcePluginRegistry::discover(root.path()).unwrap();
    let installed = registry.get("git").unwrap();
    let mut requirement = SourceSpec {
      provider: "git".to_owned(),
      plugin_version: installed.manifest.version.clone(),
      plugin_sha256: installed.manifest.sha256.clone(),
      revision: "revision".to_owned(),
      reference: None,
      parameters: BTreeMap::new(),
    };
    assert!(registry.resolve(&requirement).is_ok());

    requirement.plugin_sha256 = "0".repeat(64);
    assert!(
      registry
        .resolve(&requirement)
        .unwrap_err()
        .to_string()
        .contains("required digest")
    );

    requirement.plugin_sha256 = installed.manifest.sha256.clone();
    requirement.plugin_version = "9.9.9".to_owned();
    assert!(
      registry
        .resolve(&requirement)
        .unwrap_err()
        .to_string()
        .contains("required version")
    );

    requirement.provider = "mercurial".to_owned();
    assert!(matches!(
      registry.resolve(&requirement),
      Err(RegistryError::NotInstalled { .. })
    ));
  }

  #[test]
  fn accepts_an_empty_registry_for_an_unschedulable_agent() {
    let root = tempfile::tempdir().unwrap();
    let registry = SourcePluginRegistry::discover(root.path()).unwrap();
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
  }

  #[test]
  fn rejects_a_plugin_for_another_platform() {
    let (root, _) = plugin_fixture();
    let manifest = root.path().join("git/plugin.toml");
    let contents = fs::read_to_string(&manifest).unwrap();
    fs::write(
      &manifest,
      contents.replace(
        &format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        "unsupported-platform",
      ),
    )
    .unwrap();
    assert!(
      SourcePluginRegistry::discover(root.path())
        .unwrap_err()
        .to_string()
        .contains("does not support platform")
    );
  }
}
