//! Strict operator configuration and credential-handle resolution.

use std::{
  fs,
  path::{Component, Path, PathBuf},
};

use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use thiserror::Error;

const CONFIG_FILE: &str = "git-adapter.toml";
const CONFIG_VERSION: u16 = 1;
const MAX_CONFIG_BYTES: u64 = 64 * 1024;
const DEFAULT_MAX_GIT_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const DEFAULT_MAX_REPOSITORY_BYTES: u64 = 512 * 1024 * 1024;
const DEFAULT_MAX_BLOB_BYTES: usize = 64 * 1024 * 1024;
const MAX_GIT_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
const MAX_REPOSITORY_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_BLOB_BYTES: usize = 256 * 1024 * 1024;

/// Validated settings for one installed Git adapter process.
#[derive(Clone, Debug)]
pub struct GitAdapterConfig {
  pub(crate) git_path: PathBuf,
  pub(crate) credential_directory: PathBuf,
  pub(crate) allow_file: bool,
  pub(crate) max_git_output_bytes: usize,
  pub(crate) max_repository_bytes: u64,
  pub(crate) max_blob_bytes: usize,
}

/// Failure while loading trusted adapter configuration.
#[derive(Debug, Error)]
pub enum GitAdapterConfigError {
  /// Configuration is absent, malformed, unsafe, or outside its documented bounds.
  #[error("invalid Git VCS adapter configuration: {0}")]
  Invalid(&'static str),
  /// A configuration filesystem operation failed.
  #[error("failed to read Git VCS adapter configuration: {0}")]
  Io(#[from] std::io::Error),
  /// The strict TOML document could not be decoded.
  #[error("failed to decode Git VCS adapter configuration: {0}")]
  Toml(#[from] toml::de::Error),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
  config_version: u16,
  git_path: PathBuf,
  credential_directory: PathBuf,
  #[serde(default)]
  allow_file: bool,
  #[serde(default = "default_max_git_output_bytes")]
  max_git_output_bytes: usize,
  #[serde(default = "default_max_repository_bytes")]
  max_repository_bytes: u64,
  #[serde(default = "default_max_blob_bytes")]
  max_blob_bytes: usize,
}

impl GitAdapterConfig {
  /// Loads `git-adapter.toml` from the adapter executable directory.
  pub fn load(adapter_directory: &Path) -> Result<Self, GitAdapterConfigError> {
    validate_real_directory(adapter_directory, false)?;
    let config_path = adapter_directory.join(CONFIG_FILE);
    validate_regular_file(&config_path, false)?;
    let metadata = fs::metadata(&config_path)?;
    if metadata.len() > MAX_CONFIG_BYTES {
      return Err(GitAdapterConfigError::Invalid("configuration exceeds 64 KiB"));
    }
    let file: ConfigFile = toml::from_str(&fs::read_to_string(config_path)?)?;
    if file.config_version != CONFIG_VERSION {
      return Err(GitAdapterConfigError::Invalid("unsupported configuration version"));
    }
    if !file.git_path.is_absolute() {
      return Err(GitAdapterConfigError::Invalid("git_path must be absolute"));
    }
    let git_path = file.git_path.canonicalize()?;
    validate_regular_file(&git_path, true)?;

    if !normalized_relative_path(&file.credential_directory) {
      return Err(GitAdapterConfigError::Invalid(
        "credential_directory must be a normalized relative path",
      ));
    }
    let credential_directory = adapter_directory.join(file.credential_directory);
    validate_real_directory(&credential_directory, true)?;
    let canonical_adapter_directory = adapter_directory.canonicalize()?;
    let credential_directory = credential_directory.canonicalize()?;
    if !credential_directory.starts_with(&canonical_adapter_directory) {
      return Err(GitAdapterConfigError::Invalid(
        "credential_directory escapes the adapter directory",
      ));
    }
    if file.max_git_output_bytes == 0 || file.max_git_output_bytes > MAX_GIT_OUTPUT_BYTES {
      return Err(GitAdapterConfigError::Invalid(
        "max_git_output_bytes is outside the supported range",
      ));
    }
    if file.max_repository_bytes == 0 || file.max_repository_bytes > MAX_REPOSITORY_BYTES {
      return Err(GitAdapterConfigError::Invalid(
        "max_repository_bytes is outside the supported range",
      ));
    }
    if file.max_blob_bytes == 0 || file.max_blob_bytes > MAX_BLOB_BYTES {
      return Err(GitAdapterConfigError::Invalid(
        "max_blob_bytes is outside the supported range",
      ));
    }
    Ok(Self {
      git_path,
      credential_directory,
      allow_file: file.allow_file,
      max_git_output_bytes: file.max_git_output_bytes,
      max_repository_bytes: file.max_repository_bytes,
      max_blob_bytes: file.max_blob_bytes,
    })
  }

  pub(crate) fn credential_config(&self, handle: &str) -> Result<Option<PathBuf>, GitAdapterConfigError> {
    if handle == "anonymous" {
      return Ok(None);
    }
    let digest = format!("{:x}", Sha256::digest(handle.as_bytes()));
    let path = self.credential_directory.join(format!("{digest}.gitconfig"));
    validate_regular_file(&path, false)?;
    let metadata = fs::metadata(&path)?;
    if metadata.len() > MAX_CONFIG_BYTES {
      return Err(GitAdapterConfigError::Invalid(
        "credential configuration exceeds 64 KiB",
      ));
    }
    path.canonicalize().map(Some).map_err(Into::into)
  }
}

fn normalized_relative_path(path: &Path) -> bool {
  !path.as_os_str().is_empty()
    && !path.is_absolute()
    && path
      .components()
      .all(|component| matches!(component, Component::Normal(_)))
}

fn validate_real_directory(path: &Path, private: bool) -> Result<(), GitAdapterConfigError> {
  let metadata = fs::symlink_metadata(path)?;
  if !metadata.file_type().is_dir() {
    return Err(GitAdapterConfigError::Invalid("expected a real directory"));
  }
  validate_permissions(&metadata, private)
}

fn validate_regular_file(path: &Path, executable: bool) -> Result<(), GitAdapterConfigError> {
  let metadata = fs::symlink_metadata(path)?;
  if !metadata.file_type().is_file() {
    return Err(GitAdapterConfigError::Invalid("expected a regular file, not a symlink"));
  }
  validate_file_permissions(&metadata, executable)
}

#[cfg(unix)]
fn validate_file_permissions(metadata: &fs::Metadata, executable: bool) -> Result<(), GitAdapterConfigError> {
  use std::os::unix::fs::PermissionsExt as _;

  let mode = metadata.permissions().mode();
  if executable && mode & 0o111 == 0 {
    return Err(GitAdapterConfigError::Invalid("git_path is not executable"));
  }
  if mode & 0o022 != 0 || (!executable && mode & 0o077 != 0) {
    return Err(GitAdapterConfigError::Invalid("unsafe filesystem permissions"));
  }
  Ok(())
}

#[cfg(not(unix))]
fn validate_file_permissions(_: &fs::Metadata, _: bool) -> Result<(), GitAdapterConfigError> {
  Ok(())
}

#[cfg(unix)]
fn validate_permissions(metadata: &fs::Metadata, private: bool) -> Result<(), GitAdapterConfigError> {
  use std::os::unix::fs::PermissionsExt as _;

  let forbidden = if private { 0o077 } else { 0o022 };
  if metadata.permissions().mode() & forbidden != 0 {
    return Err(GitAdapterConfigError::Invalid("unsafe directory permissions"));
  }
  Ok(())
}

#[cfg(not(unix))]
fn validate_permissions(_: &fs::Metadata, _: bool) -> Result<(), GitAdapterConfigError> {
  Ok(())
}

const fn default_max_git_output_bytes() -> usize {
  DEFAULT_MAX_GIT_OUTPUT_BYTES
}

const fn default_max_repository_bytes() -> u64 {
  DEFAULT_MAX_REPOSITORY_BYTES
}

const fn default_max_blob_bytes() -> usize {
  DEFAULT_MAX_BLOB_BYTES
}

#[cfg(unix)]
pub(crate) const fn null_device() -> &'static str {
  "/dev/null"
}

#[cfg(windows)]
pub(crate) const fn null_device() -> &'static str {
  "NUL"
}
