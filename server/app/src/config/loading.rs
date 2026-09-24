use std::{fs::File, io::Read as _, path::Path, path::PathBuf};

use thiserror::Error;

use super::ServerConfig;

const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

impl ServerConfig {
  /// Parses strict TOML and validates every process-level invariant.
  pub fn parse_toml(contents: &str) -> Result<Self, ServerConfigError> {
    if contents.len() as u64 > MAX_CONFIG_BYTES {
      return Err(ServerConfigError::TooLarge { path: None });
    }
    Self::parse_bounded(contents, None)
  }

  fn parse_bounded(contents: &str, path: Option<PathBuf>) -> Result<Self, ServerConfigError> {
    let config: Self = toml::from_str(contents).map_err(|source| ServerConfigError::Parse {
      path,
      source: Box::new(source),
    })?;
    config.validate()?;
    Ok(config)
  }

  /// Loads a size-bounded TOML file and validates it before any socket binds.
  pub fn load(path: &Path) -> Result<Self, ServerConfigError> {
    let file = File::open(path).map_err(|source| ServerConfigError::Read {
      path: path.to_owned(),
      source,
    })?;
    let mut contents = String::new();
    file
      .take(MAX_CONFIG_BYTES + 1)
      .read_to_string(&mut contents)
      .map_err(|source| ServerConfigError::Read {
        path: path.to_owned(),
        source,
      })?;
    if contents.len() as u64 > MAX_CONFIG_BYTES {
      return Err(ServerConfigError::TooLarge {
        path: Some(path.to_owned()),
      });
    }
    Self::parse_bounded(&contents, Some(path.to_owned()))
  }
}

/// Failure to read, decode, or validate server configuration.
#[derive(Debug, Error)]
pub enum ServerConfigError {
  /// The configuration file exceeds the bounded parser input size.
  #[error(
    "server configuration{} exceeds the {MAX_CONFIG_BYTES}-byte limit",
    .path.as_ref().map(|path| format!(" '{}'", path.display())).unwrap_or_default()
  )]
  TooLarge {
    /// Configuration path that exceeded the limit, when loaded from disk.
    path: Option<PathBuf>,
  },
  /// The configuration file contents could not be read.
  #[error("failed to read server configuration '{path}': {source}")]
  Read {
    /// Configuration path being read.
    path: PathBuf,
    /// Underlying filesystem error.
    source: std::io::Error,
  },
  /// TOML syntax or structure is invalid.
  #[error(
    "failed to parse server configuration{}: {source}",
    .path.as_ref().map(|path| format!(" '{}'", path.display())).unwrap_or_default()
  )]
  Parse {
    /// File path when parsing loaded input, or `None` for in-memory input.
    path: Option<PathBuf>,
    /// TOML decoder failure.
    source: Box<toml::de::Error>,
  },
  /// A decoded value violates a process-level invariant.
  #[error("invalid server configuration: {0}")]
  Invalid(String),
}
