use std::{fs::File, io::Read as _, net::SocketAddr, path::Path, path::PathBuf, time::Duration};

use serde::Deserialize;
use thiserror::Error;

mod dependencies;

pub(crate) use dependencies::{
  AgentCredentialConfig, JobSpecConfig, ObjectStorageConfig, PostgresConfig, SigningConfig,
};

const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_SHUTDOWN_GRACE_MILLISECONDS: u64 = 5 * 60 * 1000;
const MAX_READINESS_CHECK_MILLISECONDS: u64 = 5 * 60 * 1000;
const MAX_AGENT_REGISTRATION_LIFETIME_MILLISECONDS: u64 = 30 * 24 * 60 * 60 * 1000;
const MAX_AGENT_ENROLLMENT_LIFETIME_MILLISECONDS: u64 = 24 * 60 * 60 * 1000;
const MAX_AGENT_RETRY_DELAY_MILLISECONDS: u64 = 5 * 60 * 1000;
const MAX_AGENT_LEASE_LIFETIME_MILLISECONDS: u64 = 24 * 60 * 60 * 1000;
const MAX_LEASE_EXPIRY_WORKER_MILLISECONDS: u64 = 5 * 60 * 1000;
const MAX_READY_JOB_LISTENER_RECONNECT_MILLISECONDS: u64 = 60 * 1000;

/// Validated operator configuration for the server process.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
  #[serde(default = "default_management_bind")]
  management_bind: SocketAddr,
  #[serde(default)]
  agent_bind: Option<SocketAddr>,
  #[serde(default)]
  webhook_bind: Option<SocketAddr>,
  #[serde(default)]
  acknowledge_unauthenticated_management: bool,
  #[serde(default = "default_shutdown_grace_milliseconds")]
  shutdown_grace_milliseconds: u64,
  #[serde(default = "default_readiness_check_interval_milliseconds")]
  readiness_check_interval_milliseconds: u64,
  #[serde(default = "default_readiness_check_timeout_milliseconds")]
  readiness_check_timeout_milliseconds: u64,
  #[serde(default = "default_agent_registration_lifetime_milliseconds")]
  agent_registration_lifetime_milliseconds: u64,
  #[serde(default = "default_agent_enrollment_lifetime_milliseconds")]
  agent_enrollment_lifetime_milliseconds: u64,
  #[serde(default = "default_agent_max_retry_delay_milliseconds")]
  agent_max_retry_delay_milliseconds: u64,
  #[serde(default = "default_agent_lease_lifetime_milliseconds")]
  agent_lease_lifetime_milliseconds: u64,
  #[serde(default = "default_lease_expiry_poll_interval_milliseconds")]
  lease_expiry_poll_interval_milliseconds: u64,
  #[serde(default = "default_lease_expiry_claim_lifetime_milliseconds")]
  lease_expiry_claim_lifetime_milliseconds: u64,
  #[serde(default = "default_lease_expiry_batch_size")]
  lease_expiry_batch_size: u16,
  #[serde(default = "default_ready_job_listener_reconnect_milliseconds")]
  ready_job_listener_reconnect_milliseconds: u64,
  supported_pipeline_capabilities: Vec<String>,
  postgres: PostgresConfig,
  object_storage: ObjectStorageConfig,
  signing: SigningConfig,
  agent_credentials: AgentCredentialConfig,
  job_spec: JobSpecConfig,
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

  /// Address on which the management and health listener is bound.
  pub const fn management_bind(&self) -> SocketAddr {
    self.management_bind
  }

  /// Optional address for authenticated Agent protocol ingress.
  pub const fn agent_bind(&self) -> Option<SocketAddr> {
    self.agent_bind
  }

  /// Whether external unauthenticated management access was explicitly acknowledged.
  pub const fn unauthenticated_management_acknowledged(&self) -> bool {
    self.acknowledge_unauthenticated_management
  }

  /// Whether management is configured beyond an IP loopback interface.
  pub const fn management_externally_reachable(&self) -> bool {
    !self.management_bind.ip().is_loopback()
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

  /// Lifetime of one process registration before the Agent must register again.
  pub const fn agent_registration_lifetime(&self) -> Duration {
    Duration::from_millis(self.agent_registration_lifetime_milliseconds)
  }

  /// Lifetime of one single-use Agent enrollment credential.
  pub const fn agent_enrollment_lifetime(&self) -> Duration {
    Duration::from_millis(self.agent_enrollment_lifetime_milliseconds)
  }

  /// Retry-delay ceiling advertised to registered Agents.
  pub const fn agent_max_retry_delay_milliseconds(&self) -> u64 {
    self.agent_max_retry_delay_milliseconds
  }

  /// Initial lifetime of a newly committed Job Lease.
  pub const fn agent_lease_lifetime(&self) -> Duration {
    Duration::from_millis(self.agent_lease_lifetime_milliseconds)
  }

  /// Interval between authoritative scans for expired Leases.
  pub const fn lease_expiry_poll_interval(&self) -> Duration {
    Duration::from_millis(self.lease_expiry_poll_interval_milliseconds)
  }

  /// Exclusive ownership window for one expired-Lease worker claim.
  pub const fn lease_expiry_claim_lifetime(&self) -> Duration {
    Duration::from_millis(self.lease_expiry_claim_lifetime_milliseconds)
  }

  /// Maximum expired Leases recovered in one worker pass.
  pub const fn lease_expiry_batch_size(&self) -> u16 {
    self.lease_expiry_batch_size
  }

  /// Delay before reconnecting a failed PostgreSQL ready-Job notification listener.
  pub const fn ready_job_listener_reconnect_delay(&self) -> Duration {
    Duration::from_millis(self.ready_job_listener_reconnect_milliseconds)
  }

  pub(crate) fn supported_pipeline_capabilities(&self) -> &[String] {
    &self.supported_pipeline_capabilities
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

  pub(crate) const fn agent_credentials(&self) -> &AgentCredentialConfig {
    &self.agent_credentials
  }

  pub(crate) const fn job_spec(&self) -> &JobSpecConfig {
    &self.job_spec
  }

  fn validate(&self) -> Result<(), ServerConfigError> {
    if self.management_externally_reachable() && !self.acknowledge_unauthenticated_management {
      return Err(ServerConfigError::Invalid(
        "acknowledge_unauthenticated_management must be true when management_bind is not loopback".to_owned(),
      ));
    }
    if self.webhook_bind.is_some() {
      return Err(ServerConfigError::Invalid(
        "webhook_bind is unavailable until authenticated webhook ingress is implemented".to_owned(),
      ));
    }
    let listeners = [
      ("management_bind", Some(self.management_bind)),
      ("agent_bind", self.agent_bind),
    ];
    for (index, (left_name, left)) in listeners.iter().enumerate() {
      for (right_name, right) in &listeners[index + 1..] {
        if left
          .zip(*right)
          .is_some_and(|(left, right)| listener_addresses_overlap(left, right))
        {
          return Err(ServerConfigError::Invalid(format!(
            "{left_name} and {right_name} must use independently bindable addresses"
          )));
        }
      }
    }
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
    if self.agent_registration_lifetime_milliseconds == 0
      || self.agent_registration_lifetime_milliseconds > MAX_AGENT_REGISTRATION_LIFETIME_MILLISECONDS
    {
      return Err(ServerConfigError::Invalid(format!(
        "agent_registration_lifetime_milliseconds must be between 1 and {MAX_AGENT_REGISTRATION_LIFETIME_MILLISECONDS}"
      )));
    }
    if self.agent_enrollment_lifetime_milliseconds == 0
      || self.agent_enrollment_lifetime_milliseconds > MAX_AGENT_ENROLLMENT_LIFETIME_MILLISECONDS
    {
      return Err(ServerConfigError::Invalid(format!(
        "agent_enrollment_lifetime_milliseconds must be between 1 and {MAX_AGENT_ENROLLMENT_LIFETIME_MILLISECONDS}"
      )));
    }
    if self.agent_max_retry_delay_milliseconds == 0
      || self.agent_max_retry_delay_milliseconds > MAX_AGENT_RETRY_DELAY_MILLISECONDS
    {
      return Err(ServerConfigError::Invalid(format!(
        "agent_max_retry_delay_milliseconds must be between 1 and {MAX_AGENT_RETRY_DELAY_MILLISECONDS}"
      )));
    }
    if self.agent_lease_lifetime_milliseconds == 0
      || self.agent_lease_lifetime_milliseconds > MAX_AGENT_LEASE_LIFETIME_MILLISECONDS
    {
      return Err(ServerConfigError::Invalid(format!(
        "agent_lease_lifetime_milliseconds must be between 1 and {MAX_AGENT_LEASE_LIFETIME_MILLISECONDS}"
      )));
    }
    if self.lease_expiry_poll_interval_milliseconds == 0
      || self.lease_expiry_poll_interval_milliseconds > MAX_LEASE_EXPIRY_WORKER_MILLISECONDS
      || self.lease_expiry_claim_lifetime_milliseconds == 0
      || self.lease_expiry_claim_lifetime_milliseconds > MAX_LEASE_EXPIRY_WORKER_MILLISECONDS
      || self.lease_expiry_batch_size == 0
      || self.lease_expiry_batch_size > octacity_server_store::MAX_LEASE_EXPIRY_BATCH_SIZE
    {
      return Err(ServerConfigError::Invalid(format!(
        "lease expiry worker intervals and batch must be positive and bounded by {MAX_LEASE_EXPIRY_WORKER_MILLISECONDS} ms / {} items",
        octacity_server_store::MAX_LEASE_EXPIRY_BATCH_SIZE
      )));
    }
    if self.ready_job_listener_reconnect_milliseconds == 0
      || self.ready_job_listener_reconnect_milliseconds > MAX_READY_JOB_LISTENER_RECONNECT_MILLISECONDS
    {
      return Err(ServerConfigError::Invalid(format!(
        "ready_job_listener_reconnect_milliseconds must be between 1 and {MAX_READY_JOB_LISTENER_RECONNECT_MILLISECONDS}"
      )));
    }
    self.postgres.validate().map_err(ServerConfigError::Invalid)?;
    self.object_storage.validate().map_err(ServerConfigError::Invalid)?;
    self.signing.validate().map_err(ServerConfigError::Invalid)?;
    self.agent_credentials.validate().map_err(ServerConfigError::Invalid)?;
    self.job_spec.validate().map_err(ServerConfigError::Invalid)?;
    octacity_server_application::ManagementInputFactory::validate_policy(
      &self.supported_pipeline_capabilities,
      self.agent_enrollment_lifetime(),
    )
    .map_err(|_| ServerConfigError::Invalid("supported_pipeline_capabilities is invalid".to_owned()))?;
    Ok(())
  }
}

fn default_management_bind() -> SocketAddr {
  SocketAddr::from(([127, 0, 0, 1], 8080))
}

fn listener_addresses_overlap(left: SocketAddr, right: SocketAddr) -> bool {
  if left.port() == 0 || right.port() == 0 || left.port() != right.port() {
    return false;
  }
  left.ip() == right.ip()
    || (left.is_ipv4() == right.is_ipv4() && (left.ip().is_unspecified() || right.ip().is_unspecified()))
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

const fn default_agent_registration_lifetime_milliseconds() -> u64 {
  24 * 60 * 60 * 1000
}

const fn default_agent_enrollment_lifetime_milliseconds() -> u64 {
  15 * 60 * 1000
}

const fn default_agent_max_retry_delay_milliseconds() -> u64 {
  30_000
}

const fn default_agent_lease_lifetime_milliseconds() -> u64 {
  5 * 60 * 1000
}

const fn default_lease_expiry_poll_interval_milliseconds() -> u64 {
  1_000
}

const fn default_lease_expiry_claim_lifetime_milliseconds() -> u64 {
  30_000
}

const fn default_lease_expiry_batch_size() -> u16 {
  32
}

const fn default_ready_job_listener_reconnect_milliseconds() -> u64 {
  1_000
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
