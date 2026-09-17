use std::{fs::File, io::Read as _, net::SocketAddr, path::Path, path::PathBuf, time::Duration};

use serde::Deserialize;
use thiserror::Error;

mod dependencies;

pub(crate) use dependencies::{ObjectStorageConfig, PostgresConfig, SigningConfig};

const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_SHUTDOWN_GRACE_MILLISECONDS: u64 = 5 * 60 * 1000;
const MAX_READINESS_CHECK_MILLISECONDS: u64 = 5 * 60 * 1000;

/// Validated operator configuration for the server process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerConfig {
  management_bind: SocketAddr,
  shutdown_grace_milliseconds: u64,
  readiness_check_interval_milliseconds: u64,
  readiness_check_timeout_milliseconds: u64,
  postgres: PostgresConfig,
  object_storage: ObjectStorageConfig,
  signing: SigningConfig,
}

impl ServerConfig {
  /// Parses strict TOML and validates every process-level invariant.
  pub fn parse_toml(contents: &str) -> Result<Self, ServerConfigError> {
    if contents.len() as u64 > MAX_CONFIG_BYTES {
      return Err(ServerConfigError::TooLarge { path: None });
    }
    Self::parse_bounded(contents, None)
  }

  fn parse_bounded(contents: &str, path: Option<PathBuf>) -> Result<Self, ServerConfigError> {
    let config: UnvalidatedServerConfig = toml::from_str(contents).map_err(|source| ServerConfigError::Parse {
      path,
      source: Box::new(source),
    })?;
    let config = Self {
      management_bind: config.management_bind,
      shutdown_grace_milliseconds: config.shutdown_grace_milliseconds,
      readiness_check_interval_milliseconds: config.readiness_check_interval_milliseconds,
      readiness_check_timeout_milliseconds: config.readiness_check_timeout_milliseconds,
      postgres: config.postgres,
      object_storage: config.object_storage,
      signing: config.signing,
    };
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

  /// Address on which the management and health listener is bound.
  pub const fn management_bind(&self) -> SocketAddr {
    self.management_bind
  }

  /// Maximum time allowed for in-flight requests to finish during shutdown.
  pub const fn shutdown_grace(&self) -> Duration {
    Duration::from_millis(self.shutdown_grace_milliseconds)
  }

  /// Interval between refreshes of the dependency readiness snapshot.
  pub const fn readiness_check_interval(&self) -> Duration {
    Duration::from_millis(self.readiness_check_interval_milliseconds)
  }

  /// Aggregate deadline for one refresh of every readiness dependency.
  pub const fn readiness_check_timeout(&self) -> Duration {
    Duration::from_millis(self.readiness_check_timeout_milliseconds)
  }

  pub(crate) const fn postgres(&self) -> &PostgresConfig {
    &self.postgres
  }

  pub(crate) const fn object_storage(&self) -> &ObjectStorageConfig {
    &self.object_storage
  }

  pub(crate) const fn signing(&self) -> &SigningConfig {
    &self.signing
  }

  fn validate(&self) -> Result<(), ServerConfigError> {
    if self.shutdown_grace_milliseconds == 0 || self.shutdown_grace_milliseconds > MAX_SHUTDOWN_GRACE_MILLISECONDS {
      return Err(ServerConfigError::Invalid(format!(
        "shutdown_grace_milliseconds must be between 1 and {MAX_SHUTDOWN_GRACE_MILLISECONDS}"
      )));
    }
    if self.readiness_check_interval_milliseconds == 0
      || self.readiness_check_interval_milliseconds > MAX_READINESS_CHECK_MILLISECONDS
    {
      return Err(ServerConfigError::Invalid(format!(
        "readiness_check_interval_milliseconds must be between 1 and {MAX_READINESS_CHECK_MILLISECONDS}"
      )));
    }
    if self.readiness_check_timeout_milliseconds == 0
      || self.readiness_check_timeout_milliseconds > MAX_READINESS_CHECK_MILLISECONDS
    {
      return Err(ServerConfigError::Invalid(format!(
        "readiness_check_timeout_milliseconds must be between 1 and {MAX_READINESS_CHECK_MILLISECONDS}"
      )));
    }
    self.postgres.validate().map_err(ServerConfigError::Invalid)?;
    self.object_storage.validate().map_err(ServerConfigError::Invalid)?;
    self.signing.validate().map_err(ServerConfigError::Invalid)?;
    Ok(())
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UnvalidatedServerConfig {
  #[serde(default = "default_management_bind")]
  management_bind: SocketAddr,
  #[serde(default = "default_shutdown_grace_milliseconds")]
  shutdown_grace_milliseconds: u64,
  #[serde(default = "default_readiness_check_interval_milliseconds")]
  readiness_check_interval_milliseconds: u64,
  #[serde(default = "default_readiness_check_timeout_milliseconds")]
  readiness_check_timeout_milliseconds: u64,
  postgres: PostgresConfig,
  object_storage: ObjectStorageConfig,
  signing: SigningConfig,
}

fn default_management_bind() -> SocketAddr {
  SocketAddr::from(([127, 0, 0, 1], 8080))
}

const fn default_shutdown_grace_milliseconds() -> u64 {
  10_000
}

const fn default_readiness_check_interval_milliseconds() -> u64 {
  5_000
}

const fn default_readiness_check_timeout_milliseconds() -> u64 {
  2_000
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
