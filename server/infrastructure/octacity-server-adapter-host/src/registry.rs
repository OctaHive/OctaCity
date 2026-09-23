use std::{
  collections::BTreeMap,
  fs::{self, File},
  io::{self, Read as _},
  path::{Path, PathBuf},
};

use sha2::{Digest as _, Sha256};
use thiserror::Error;

const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
/// Maximum verified adapter executable size.
pub const MAX_EXECUTABLE_BYTES: u64 = 256 * 1024 * 1024;

/// Canonical immutable executable selected from a verified registry entry.
#[derive(Clone, Debug)]
pub struct VerifiedExecutable {
  adapter_id: String,
  path: PathBuf,
  sha256: String,
}

impl VerifiedExecutable {
  /// Returns the logical adapter identity bound to this executable.
  #[must_use]
  pub fn adapter_id(&self) -> &str {
    &self.adapter_id
  }

  /// Returns the canonical executable path confined to its registry entry.
  #[must_use]
  pub fn path(&self) -> &Path {
    &self.path
  }

  /// Returns the operator-pinned lowercase SHA-256 digest.
  #[must_use]
  pub fn sha256(&self) -> &str {
    &self.sha256
  }
}

/// Protocol-specific installed adapter that can be loaded into a verified registry.
pub trait RegistryAdapter: Sized {
  /// Loads and validates one real registry directory.
  fn load(directory: &Path) -> Result<Self, RegistryError>;

  /// Returns the logical identity used as the registry key.
  fn adapter_id(&self) -> &str;

  /// Returns the operator-pinned executable digest used for exact resolution.
  fn executable_sha256(&self) -> &str;

  /// Human-readable protocol family used only in secret-free diagnostics.
  fn protocol_family() -> &'static str;
}

/// Immutable, deterministically ordered inventory of verified adapters.
#[derive(Debug)]
pub struct AdapterRegistry<T> {
  adapters: BTreeMap<String, T>,
}

impl<T: RegistryAdapter> AdapterRegistry<T> {
  /// Discovers and verifies every real adapter directory below `root`.
  pub fn discover(root: &Path) -> Result<Self, RegistryError> {
    let adapters = discover_adapters(root, |directory| {
      let adapter = T::load(directory)?;
      Ok((adapter.adapter_id().to_owned(), adapter))
    })?;
    for adapter_id in adapters.keys() {
      tracing::debug!(adapter = %adapter_id, protocol = T::protocol_family(), "validated adapter");
    }
    tracing::info!(
      registry = %root.display(),
      adapters = adapters.len(),
      protocol = T::protocol_family(),
      "loaded adapter registry"
    );
    Ok(Self { adapters })
  }

  /// Returns the number of verified adapters.
  #[must_use]
  pub fn len(&self) -> usize {
    self.adapters.len()
  }

  /// Returns whether no adapters are installed.
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.adapters.is_empty()
  }

  /// Iterates over adapters in deterministic identity order.
  pub fn iter(&self) -> impl Iterator<Item = (&str, &T)> {
    self.adapters.iter().map(|(id, adapter)| (id.as_str(), adapter))
  }

  /// Resolves an adapter only when its operator-pinned digest matches exactly.
  pub fn resolve(&self, adapter_id: &str, executable_sha256: &str) -> Result<&T, RegistryError> {
    let adapter = self
      .adapters
      .get(adapter_id)
      .ok_or_else(|| RegistryError::NotInstalled {
        adapter_id: adapter_id.to_owned(),
      })?;
    if adapter.executable_sha256() != executable_sha256 {
      return Err(RegistryError::DigestMismatch {
        adapter_id: adapter_id.to_owned(),
      });
    }
    Ok(adapter)
  }
}

/// Failure while discovering or resolving installed adapters.
#[derive(Debug, Error)]
pub enum RegistryError {
  /// The registry root could not be enumerated.
  #[error("failed to read adapter registry '{path}': {source}")]
  ReadDirectory {
    /// Registry root being read.
    path: PathBuf,
    /// Underlying filesystem error.
    source: io::Error,
  },
  /// An entry violates the trusted registry layout.
  #[error("invalid adapter registry entry '{path}': {message}")]
  InvalidEntry {
    /// Invalid entry or file.
    path: PathBuf,
    /// Violated invariant.
    message: String,
  },
  /// An adapter manifest could not be read.
  #[error("failed to read adapter manifest '{path}': {source}")]
  ReadManifest {
    /// Manifest path.
    path: PathBuf,
    /// Underlying filesystem error.
    source: io::Error,
  },
  /// An adapter manifest is not valid strict TOML.
  #[error("failed to parse adapter manifest '{path}': {message}")]
  ParseManifest {
    /// Manifest path.
    path: PathBuf,
    /// Parser diagnostic.
    message: String,
  },
  /// An executable could not be hashed.
  #[error("failed to hash adapter executable '{path}': {source}")]
  HashExecutable {
    /// Executable path.
    path: PathBuf,
    /// Underlying filesystem error.
    source: io::Error,
  },
  /// The requested adapter is absent.
  #[error("adapter '{adapter_id}' is not installed")]
  NotInstalled {
    /// Required logical adapter identity.
    adapter_id: String,
  },
  /// The installed digest differs from the configured immutable requirement.
  #[error("adapter '{adapter_id}' does not match the required executable digest")]
  DigestMismatch {
    /// Required logical adapter identity.
    adapter_id: String,
  },
}

fn discover_adapters<T, Load>(root: &Path, load: Load) -> Result<BTreeMap<String, T>, RegistryError>
where
  Load: Fn(&Path) -> Result<(String, T), RegistryError>,
{
  validate_directory(root)?;
  validate_permissions(root, false)?;
  let entries = fs::read_dir(root).map_err(|source| RegistryError::ReadDirectory {
    path: root.to_owned(),
    source,
  })?;
  let mut entries = entries
    .collect::<Result<Vec<_>, _>>()
    .map_err(|source| RegistryError::ReadDirectory {
      path: root.to_owned(),
      source,
    })?;
  entries.sort_by_key(std::fs::DirEntry::file_name);

  let mut adapters = BTreeMap::new();
  for entry in entries {
    let directory = entry.path();
    let file_type = entry
      .file_type()
      .map_err(|error| invalid_entry(&directory, error.to_string()))?;
    if !file_type.is_dir() || file_type.is_symlink() {
      return Err(invalid_entry(&directory, "registry entries must be real directories"));
    }
    validate_permissions(&directory, false)?;
    let (adapter_id, adapter) = load(&directory)?;
    if adapters.insert(adapter_id.clone(), adapter).is_some() {
      return Err(invalid_entry(
        &directory,
        format!("duplicate adapter identity '{adapter_id}'"),
      ));
    }
  }
  Ok(adapters)
}

/// Reads one bounded regular `adapter.toml` file.
pub fn read_manifest(directory: &Path) -> Result<(PathBuf, String), RegistryError> {
  let path = directory.join("adapter.toml");
  validate_regular_file(&path, false)?;
  let metadata = fs::metadata(&path).map_err(|source| RegistryError::ReadManifest {
    path: path.clone(),
    source,
  })?;
  if metadata.len() > MAX_MANIFEST_BYTES {
    return Err(invalid_entry(
      &path,
      format!("manifest exceeds the {MAX_MANIFEST_BYTES}-byte limit"),
    ));
  }
  let contents = fs::read_to_string(&path).map_err(|source| RegistryError::ReadManifest {
    path: path.clone(),
    source,
  })?;
  Ok((path, contents))
}

/// Verifies one normalized relative executable path, confinement, size, and digest.
pub fn verify_executable(
  directory: &Path,
  adapter_id: String,
  relative_path: &str,
  sha256: String,
) -> Result<VerifiedExecutable, RegistryError> {
  if directory.file_name().and_then(|name| name.to_str()) != Some(adapter_id.as_str()) {
    return Err(invalid_entry(
      directory,
      "adapter identity must match its directory name",
    ));
  }
  if !relative_wire_path(relative_path) {
    return Err(invalid_entry(
      directory,
      "adapter executable must be a normalized relative path",
    ));
  }
  let executable = directory.join(relative_path);
  validate_regular_file(&executable, true)?;
  let canonical_directory = directory
    .canonicalize()
    .map_err(|error| invalid_entry(directory, error.to_string()))?;
  let executable = executable
    .canonicalize()
    .map_err(|error| invalid_entry(&executable, error.to_string()))?;
  if !executable.starts_with(&canonical_directory) {
    return Err(invalid_entry(&executable, "adapter executable escapes its directory"));
  }
  let metadata = fs::metadata(&executable).map_err(|error| invalid_entry(&executable, error.to_string()))?;
  if metadata.len() > MAX_EXECUTABLE_BYTES {
    return Err(invalid_entry(
      &executable,
      format!("adapter executable exceeds the {MAX_EXECUTABLE_BYTES}-byte limit"),
    ));
  }
  if file_sha256(&executable)? != sha256 {
    return Err(invalid_entry(
      &executable,
      "adapter executable SHA-256 does not match adapter.toml",
    ));
  }
  Ok(VerifiedExecutable {
    adapter_id,
    path: executable,
    sha256,
  })
}

pub(crate) fn validate_regular_file(path: &Path, executable: bool) -> Result<(), RegistryError> {
  let metadata = fs::symlink_metadata(path).map_err(|error| invalid_entry(path, error.to_string()))?;
  if !metadata.file_type().is_file() {
    return Err(invalid_entry(
      path,
      "expected a regular file, not a symlink or special file",
    ));
  }
  validate_permissions(path, executable)
}

fn validate_directory(path: &Path) -> Result<(), RegistryError> {
  let metadata = fs::symlink_metadata(path).map_err(|error| invalid_entry(path, error.to_string()))?;
  if metadata.file_type().is_dir() {
    Ok(())
  } else {
    Err(invalid_entry(path, "expected a real directory"))
  }
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
    return Err(invalid_entry(path, "adapter executable has no execute bit"));
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

fn relative_wire_path(value: &str) -> bool {
  !value.is_empty()
    && !value.starts_with('/')
    && !value.contains(['\\', ':'])
    && !value.chars().any(char::is_control)
    && !value
      .split('/')
      .any(|part| part.is_empty() || part == "." || part == "..")
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
