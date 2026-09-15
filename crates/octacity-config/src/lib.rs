//! Loads and validates operator-owned agent configuration.
//!
//! Validation happens once at startup and turns filesystem paths, signing
//! keys, and backend choices into trusted runtime inputs. It validates only
//! configuration-owned invariants; construction of source, runner, and
//! execution components belongs to the agent composition root.

use std::{
  collections::{BTreeMap, BTreeSet},
  fs,
  path::{Path, PathBuf},
};

use ed25519_dalek::VerifyingKey;
use octacity_protocol::{OutputLimits, RuntimeMode};
use serde::Deserialize;

pub use octa_cache_protocol::LocalCacheCapacity;
use thiserror::Error;
use tracing::{debug, info};

const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

mod validation;

use validation::*;

/// Configuration read from the agent's TOML file.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentConfig {
  /// Stable identity used for leases and owned resource names.
  pub agent_id: String,
  /// Coordinator HTTPS origin.
  pub server_url: String,
  /// Private agent-enrollment credential file.
  pub credential_file: PathBuf,
  /// Trusted server signature keys indexed by key identifier.
  pub server_signing_keys: BTreeMap<String, String>,
  /// Scheduler-visible operator labels.
  #[serde(default)]
  pub labels: BTreeMap<String, String>,
  /// Dedicated root for ephemeral job workspaces.
  pub work_root: PathBuf,
  /// Dedicated root for persistent agent and backend state.
  pub state_root: PathBuf,
  /// Operator-installed immutable Octa release root.
  pub octa_release_root: PathBuf,
  /// Operator-installed source-plugin registry root.
  pub source_plugins_dir: PathBuf,
  /// Workload identity profile names mapped to restricted rotating token files.
  #[serde(default)]
  pub workload_identity_profiles: BTreeMap<String, PathBuf>,
  /// Persistent local cache and optional remote-cache transport policy.
  pub cache: CacheConfig,
  /// Runtime modes this agent may advertise.
  pub enabled_runtime_modes: Vec<RuntimeMode>,
  /// Explicit opt-in for host-native execution.
  #[serde(default)]
  pub allow_native_execution: bool,
  /// Delegated cgroup-v2 root for Linux Native jobs.
  #[serde(default)]
  pub native_linux_cgroup_root: Option<PathBuf>,
  /// Exact Bubblewrap executable for Linux Native jobs.
  #[serde(default)]
  pub native_linux_bubblewrap_executable: Option<PathBuf>,
  /// Additional host paths exposed read-only to Native jobs.
  #[serde(default)]
  pub native_linux_readonly_paths: Vec<PathBuf>,
  /// Maximum descendant processes in one Native job.
  pub native_linux_pids_limit: u32,
  /// Explicit clean process environment for Native jobs.
  #[serde(default)]
  pub native_environment: BTreeMap<String, String>,
  /// Explicit OCI lifecycle engines and their policies.
  #[serde(default)]
  pub oci_engines: Vec<OciEngineConfig>,
  /// Allows a signed job to request unrestricted network access.
  #[serde(default)]
  pub allow_unrestricted_network: bool,
  /// Complete set of hosts that a restricted signed job may request.
  #[serde(default)]
  pub allowed_network_hosts: Vec<String>,
  /// HTTPS origins allowed for output uploads.
  pub allowed_upload_origins: Vec<String>,
  /// Maximum filesystem entries traversed in one directory artifact.
  pub max_archive_entries: usize,
  /// Wall-clock limit for one presigned object upload attempt.
  pub upload_timeout_seconds: u64,
  /// Agent-owned maxima applied in addition to every signed output quota.
  pub max_output_limits: OutputLimits,
  /// Maximum accepted workspace allocation.
  pub max_workspace_bytes: u64,
  /// Maximum local unacknowledged event-spool allocation.
  pub max_spool_bytes: u64,
  /// Maximum unacknowledged records retained for one attempt.
  pub max_spool_records: usize,
  /// Maximum encoded event bytes selected for one append call.
  pub event_batch_max_bytes: usize,
  /// Maximum events selected for one append call.
  pub event_batch_max_records: usize,
  /// In-memory runner-to-spool backpressure boundary.
  pub event_channel_capacity: usize,
  /// Coordinator long-poll duration.
  pub poll_timeout_seconds: u64,
  /// Timeout for coordinator requests other than the long-poll wait itself.
  pub coordinator_request_timeout_seconds: u64,
  /// Maximum serialized coordinator request or response body.
  pub coordinator_max_body_bytes: usize,
  /// Base delay for idempotent coordinator and object-upload retries.
  pub retry_initial_delay_milliseconds: u64,
  /// Local ceiling for coordinator and object-upload retry delays.
  pub retry_max_delay_seconds: u64,
  /// Total attempts for one idempotent outbound operation.
  pub retry_max_attempts: usize,
  /// Interval between heartbeats.
  pub heartbeat_interval_seconds: u64,
  /// Time reserved to stop before lease expiry.
  pub lease_safety_margin_seconds: u64,
  /// Grace between cooperative and forced cancellation.
  pub graceful_cancel_timeout_seconds: u64,
  /// Bound for backend cleanup operations.
  pub cleanup_timeout_seconds: u64,
  /// Bound for the runner protocol handshake.
  pub runner_hello_timeout_seconds: u64,
  /// Interval between resource-accounting samples.
  pub resource_sample_interval_seconds: u64,
  /// Bound for one resource-accounting call.
  pub resource_sample_timeout_seconds: u64,
  /// Consecutive accounting failures tolerated per job.
  pub max_accounting_failures: usize,
}

/// Operator-owned task-result cache policy shared by all jobs on this agent.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheConfig {
  /// Dedicated persistent root for Octa's verified local L1 stores.
  pub root: PathBuf,
  /// Capacity and collection watermarks for each authorized local scope.
  pub capacity: LocalCacheCapacity,
  /// Maximum persistent trust scopes; aggregate cache data is bounded by this
  /// value multiplied by `max_bytes`.
  pub max_scopes: usize,
  /// Whether signed jobs may read cached results.
  pub allow_read: bool,
  /// Whether signed jobs may publish cache results.
  pub allow_write: bool,
  /// Remote cache origins the coordinator may select.
  #[serde(default)]
  pub allowed_remote_origins: Vec<String>,
  /// Optional private CA passed to Octa for cache HTTPS connections.
  #[serde(default)]
  pub ca_certificate_file: Option<PathBuf>,
  /// Stable environment identity strings keyed by `os-architecture` for Native jobs.
  #[serde(default)]
  pub native_environment_identities: BTreeMap<String, String>,
  /// Whole-operation deadline applied by Octa's remote client.
  pub request_timeout_seconds: u64,
  /// Maximum simultaneous cache blob transfers within one job.
  pub max_parallel_transfers: usize,
}

/// One explicitly configured OCI lifecycle implementation.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "engine", rename_all = "snake_case", deny_unknown_fields)]
pub enum OciEngineConfig {
  /// Hypervisor-isolated Linux guest supplied by Microsandbox.
  Microsandbox {
    /// Exact `msb` executable.
    executable: PathBuf,
    /// Exact libkrun firmware library.
    libkrunfw: PathBuf,
    /// Interval at which the microVM backend collects accounting samples.
    metrics_sample_interval_seconds: u64,
  },
  /// Linux OCI process isolation supplied by containerd.
  Containerd {
    /// Absolute containerd Unix socket.
    endpoint: PathBuf,
    /// Dedicated containerd namespace.
    namespace: String,
    /// Snapshotter for job root filesystems.
    snapshotter: String,
    /// OCI runtime-v2 implementation.
    runtime: String,
    /// Optional registry-host configuration root.
    #[serde(default)]
    registry_config_dir: Option<PathBuf>,
    /// Maximum descendant processes in a container.
    pids_limit: u32,
    /// Per-process `RLIMIT_NOFILE` inside the container.
    open_files_limit: u64,
  },
}

/// Startup configuration after cryptographic and filesystem validation.
#[derive(Debug)]
pub struct ValidatedConfig {
  /// Canonicalized general configuration.
  pub config: AgentConfig,
  /// Decoded server signature-verification keys.
  pub signing_keys: BTreeMap<String, VerifyingKey>,
  /// Mode-specific configuration with required values present.
  pub runtimes: Vec<ValidatedRuntimeConfig>,
}

/// Runtime configuration whose required paths and mode-specific invariants
/// have already been validated.
#[derive(Clone, Debug)]
pub enum ValidatedRuntimeConfig {
  /// Complete Linux Native configuration.
  Native {
    /// Canonical delegated cgroup root.
    cgroup_root: PathBuf,
    /// Canonical Bubblewrap executable.
    bubblewrap: PathBuf,
    /// Canonical extra read-only host paths.
    readonly_paths: Vec<PathBuf>,
    /// Enforced descendant-process ceiling.
    pids_limit: u32,
    /// Explicit clean process environment.
    environment: BTreeMap<String, String>,
  },
  /// Validated OCI engine configurations.
  Oci {
    /// Engines in deterministic operator order.
    engines: Vec<OciEngineConfig>,
  },
}

#[derive(Debug, Error)]
/// Failure to load or validate the agent configuration.
pub enum ConfigError {
  /// The configuration file metadata could not be inspected.
  #[error("failed to inspect configuration '{path}': {source}")]
  Inspect {
    /// Configuration path being inspected.
    path: PathBuf,
    /// Underlying filesystem error.
    source: std::io::Error,
  },
  /// The configuration file exceeds the bounded parser input size.
  #[error("configuration '{path}' exceeds the {MAX_CONFIG_BYTES}-byte limit")]
  TooLarge {
    /// Configuration path that exceeded the input limit.
    path: PathBuf,
  },
  /// The configuration file contents could not be read.
  #[error("failed to read configuration '{path}': {source}")]
  Read {
    /// Configuration path being read.
    path: PathBuf,
    /// Underlying filesystem error.
    source: std::io::Error,
  },
  /// The configuration file is not valid TOML for [`AgentConfig`].
  #[error("failed to parse configuration '{path}': {source}")]
  Parse {
    /// Configuration path being parsed.
    path: PathBuf,
    /// TOML syntax or deserialization error.
    source: Box<toml::de::Error>,
  },
  #[error("invalid agent configuration: {0}")]
  /// Configuration values violate an agent invariant.
  Invalid(String),
}

impl CacheConfig {
  /// Canonicalizes cache-owned paths and validates limits and trust inputs.
  fn validate(&mut self) -> Result<(), ConfigError> {
    self
      .capacity
      .validate()
      .map_err(|error| ConfigError::Invalid(error.to_string()))?;
    if self.max_scopes == 0 {
      return invalid("cache max_scopes must be greater than zero");
    }
    if !self.allow_read && !self.allow_write {
      return invalid("cache policy must allow reading, writing, or both");
    }
    if u64::try_from(self.max_scopes)
      .ok()
      .and_then(|scopes| self.capacity.max_bytes.checked_mul(scopes))
      .is_none()
    {
      return invalid("aggregate cache capacity is not representable");
    }
    if self.request_timeout_seconds == 0
      || self.request_timeout_seconds > octa_cache_protocol::MAX_REMOTE_CACHE_REQUEST_TIMEOUT_SECONDS
    {
      return invalid("cache request timeout is outside Octa's supported bounds");
    }
    if self.max_parallel_transfers == 0
      || self.max_parallel_transfers > octa_cache_protocol::MAX_REMOTE_CACHE_PARALLEL_TRANSFERS
    {
      return invalid("cache transfer concurrency is outside Octa's supported bounds");
    }

    let mut origins = BTreeSet::new();
    for origin in &mut self.allowed_remote_origins {
      *origin = canonical_cache_origin(origin)?;
      if !origins.insert(origin.clone()) {
        return invalid("cache remote origins must not contain duplicates or equivalent origins");
      }
    }
    if let Some(certificate) = &mut self.ca_certificate_file {
      *certificate = canonical_regular_file("cache.ca_certificate_file", certificate)?;
      validate_trusted_owner("cache.ca_certificate_file", certificate)?;
      validate_private_file_permissions("cache.ca_certificate_file", certificate)?;
      let parent = certificate
        .parent()
        .ok_or_else(|| ConfigError::Invalid("cache.ca_certificate_file has no parent".to_owned()))?;
      validate_trusted_directory_chain("cache.ca_certificate_file parent", parent)?;
    }
    for (platform, identity) in &self.native_environment_identities {
      if !matches!(
        platform.as_str(),
        "linux-amd64" | "linux-arm64" | "windows-amd64" | "windows-arm64" | "macos-amd64" | "macos-arm64"
      ) {
        return invalid(format!("unsupported Native cache platform identity '{platform}'"));
      }
      // The string validation is platform-independent. The runtime module
      // supplies the selected platform to the same shared constructor.
      octa_cache_protocol::RuntimeIdentity::native(
        octa_cache_protocol::PlatformOs::Linux,
        octa_cache_protocol::PlatformArchitecture::Amd64,
        identity,
      )
      .map_err(|error| ConfigError::Invalid(format!("Native cache environment identity '{platform}': {error}")))?;
    }
    Ok(())
  }
}

impl AgentConfig {
  /// Reads a size-bounded TOML file without applying environmental defaults.
  pub fn load(path: &Path) -> Result<Self, ConfigError> {
    debug!(config = %path.display(), "loading agent configuration");
    let metadata = fs::metadata(path).map_err(|source| ConfigError::Inspect {
      path: path.to_owned(),
      source,
    })?;
    if metadata.len() > MAX_CONFIG_BYTES {
      return Err(ConfigError::TooLarge { path: path.to_owned() });
    }
    let contents = fs::read_to_string(path).map_err(|source| ConfigError::Read {
      path: path.to_owned(),
      source,
    })?;
    debug!(config = %path.display(), bytes = contents.len(), "read agent configuration");
    toml::from_str(&contents).map_err(|source| ConfigError::Parse {
      path: path.to_owned(),
      source: Box::new(source),
    })
  }

  /// Resolves and validates every operator-controlled trust boundary.
  pub fn validate(mut self) -> Result<ValidatedConfig, ConfigError> {
    non_empty("agent_id", &self.agent_id)?;
    validate_server_url(&self.server_url)?;
    validate_regular_file("credential_file", &self.credential_file)?;
    validate_private_file_permissions("credential_file", &self.credential_file)?;
    self.credential_file = self
      .credential_file
      .canonicalize()
      .map_err(|error| ConfigError::Invalid(format!("credential_file: {error}")))?;
    validate_trusted_owner("credential_file", &self.credential_file)?;
    let credential_parent = self
      .credential_file
      .parent()
      .ok_or_else(|| ConfigError::Invalid("credential_file has no parent".to_owned()))?;
    validate_private_directory_permissions("credential_file parent", credential_parent)?;
    validate_trusted_directory_chain("credential_file parent", credential_parent)?;

    if self.server_signing_keys.is_empty() {
      return invalid("server_signing_keys must contain at least one key");
    }
    let signing_keys = self
      .server_signing_keys
      .iter()
      .map(|(id, encoded)| decode_signing_key(id, encoded))
      .collect::<Result<_, _>>()?;

    // Overlapping roots would let job cleanup or workspace writes reach agent
    // state, installed Octa binaries, or source-plugin executables.
    let roots = [
      ("work_root", canonical_directory("work_root", &self.work_root)?),
      ("state_root", canonical_directory("state_root", &self.state_root)?),
      (
        "octa_release_root",
        canonical_directory("octa_release_root", &self.octa_release_root)?,
      ),
      (
        "source_plugins_dir",
        canonical_directory("source_plugins_dir", &self.source_plugins_dir)?,
      ),
      ("cache.root", canonical_directory("cache.root", &self.cache.root)?),
    ];
    validate_distinct_roots(&roots)?;
    self.work_root = roots[0].1.clone();
    self.state_root = roots[1].1.clone();
    self.octa_release_root = roots[2].1.clone();
    self.source_plugins_dir = roots[3].1.clone();
    self.cache.root = roots[4].1.clone();

    validate_private_directory_permissions("cache.root", &self.cache.root)?;
    validate_trusted_directory_chain("cache.root", &self.cache.root)?;
    self.cache.validate()?;

    for (profile, source) in &mut self.workload_identity_profiles {
      non_empty("workload identity profile", profile)?;
      if profile.chars().any(char::is_control) {
        return invalid("workload identity profile names must not contain control characters");
      }
      *source = canonical_regular_file("workload identity source", source)?;
      validate_private_file_permissions("workload identity source", source)?;
      validate_trusted_owner("workload identity source", source)?;
      let parent = source
        .parent()
        .ok_or_else(|| ConfigError::Invalid("workload identity source has no parent".to_owned()))?;
      canonical_directory("workload identity source parent", parent)?;
      validate_private_directory_permissions("workload identity source parent", parent)?;
      validate_trusted_directory_chain("workload identity source parent", parent)?;
      if source.starts_with(&self.work_root) {
        return invalid("workload identity sources must be outside work_root");
      }
    }

    for (name, value) in &self.labels {
      non_empty("label name", name)?;
      non_empty("label value", value)?;
    }
    if self.enabled_runtime_modes.is_empty() {
      return invalid("enabled_runtime_modes must contain at least one mode");
    }
    let modes: BTreeSet<_> = self.enabled_runtime_modes.iter().copied().collect();
    if modes.len() != self.enabled_runtime_modes.len() {
      return invalid("enabled_runtime_modes must not contain duplicates");
    }
    if modes.contains(&RuntimeMode::Native) && !self.allow_native_execution {
      return invalid("NativeBackend requires allow_native_execution = true");
    }
    let native_runtime = if modes.contains(&RuntimeMode::Native) {
      let configured_root = self
        .native_linux_cgroup_root
        .as_deref()
        .ok_or_else(|| ConfigError::Invalid("NativeBackend requires native_linux_cgroup_root".to_owned()))?;
      let cgroup_root = canonical_directory("native_linux_cgroup_root", configured_root)?;
      self.native_linux_cgroup_root = Some(cgroup_root.clone());
      let configured_bubblewrap = self
        .native_linux_bubblewrap_executable
        .as_deref()
        .ok_or_else(|| ConfigError::Invalid("NativeBackend requires native_linux_bubblewrap_executable".to_owned()))?;
      let bubblewrap = canonical_regular_file("native_linux_bubblewrap_executable", configured_bubblewrap)?;
      self.native_linux_bubblewrap_executable = Some(bubblewrap.clone());
      self.native_linux_readonly_paths = self
        .native_linux_readonly_paths
        .iter()
        .map(|path| canonical_path("native_linux_readonly_paths", path))
        .collect::<Result<_, _>>()?;
      if self.native_linux_pids_limit == 0 {
        return invalid("native_linux_pids_limit must be greater than zero");
      }
      validate_native_environment(&self.native_environment)?;
      Some(ValidatedRuntimeConfig::Native {
        cgroup_root,
        bubblewrap,
        readonly_paths: self.native_linux_readonly_paths.clone(),
        pids_limit: self.native_linux_pids_limit,
        environment: self.native_environment.clone(),
      })
    } else if self.native_linux_cgroup_root.is_some()
      || self.native_linux_bubblewrap_executable.is_some()
      || !self.native_linux_readonly_paths.is_empty()
      || self.native_linux_pids_limit != 0
    {
      return invalid("native Linux settings are only valid when NativeBackend is enabled");
    } else if !self.native_environment.is_empty() {
      return invalid("native_environment is only valid when NativeBackend is enabled");
    } else {
      None
    };
    validate_oci_engines(&mut self.oci_engines, modes.contains(&RuntimeMode::Oci))?;

    let mut network_hosts = BTreeSet::new();
    for host in &self.allowed_network_hosts {
      non_empty("allowed network host", host)?;
      if host.trim() != host || host.chars().any(char::is_control) || !network_hosts.insert(host) {
        return invalid("allowed network hosts must be unique trimmed values without control characters");
      }
    }

    if self.allowed_upload_origins.is_empty() {
      return invalid("allowed_upload_origins must contain at least one origin");
    }
    let mut unique_origins = BTreeSet::new();
    for origin in &mut self.allowed_upload_origins {
      *origin = canonical_upload_origin(origin)?;
      if !unique_origins.insert(origin.clone()) {
        return invalid("allowed_upload_origins must not contain duplicates or equivalent origins");
      }
    }
    self.max_output_limits.validate().map_err(ConfigError::Invalid)?;

    if self.max_archive_entries == 0
      || self.upload_timeout_seconds == 0
      || self.max_workspace_bytes == 0
      || self.max_spool_bytes == 0
      || self.max_spool_records == 0
      || self.event_batch_max_bytes == 0
      || self.event_batch_max_records == 0
      || self.event_channel_capacity == 0
    {
      return invalid("output, workspace, spool, event batch, and channel limits must be greater than zero");
    }
    if self.coordinator_max_body_bytes == 0 || self.retry_max_attempts == 0 {
      return invalid("coordinator body and retry-attempt limits must be greater than zero");
    }
    if self.event_batch_max_records > self.max_spool_records
      || self.event_batch_max_records > octacity_protocol::MAX_EVENT_BATCH_RECORDS
      || self.event_batch_max_bytes as u64 > self.max_spool_bytes
      || self
        .event_batch_max_bytes
        .checked_add(octacity_protocol::MAX_APPEND_REQUEST_OVERHEAD_BYTES)
        .is_none_or(|maximum| maximum > self.coordinator_max_body_bytes)
    {
      return invalid("event batch limits plus protocol metadata must fit the spool and coordinator bounds");
    }
    for (name, value) in [
      ("poll_timeout_seconds", self.poll_timeout_seconds),
      (
        "coordinator_request_timeout_seconds",
        self.coordinator_request_timeout_seconds,
      ),
      (
        "retry_initial_delay_milliseconds",
        self.retry_initial_delay_milliseconds,
      ),
      ("retry_max_delay_seconds", self.retry_max_delay_seconds),
      ("heartbeat_interval_seconds", self.heartbeat_interval_seconds),
      ("lease_safety_margin_seconds", self.lease_safety_margin_seconds),
      ("graceful_cancel_timeout_seconds", self.graceful_cancel_timeout_seconds),
      ("cleanup_timeout_seconds", self.cleanup_timeout_seconds),
      ("runner_hello_timeout_seconds", self.runner_hello_timeout_seconds),
      (
        "resource_sample_interval_seconds",
        self.resource_sample_interval_seconds,
      ),
      ("resource_sample_timeout_seconds", self.resource_sample_timeout_seconds),
    ] {
      if value == 0 {
        return invalid(format!("{name} must be greater than zero"));
      }
    }
    if self.max_accounting_failures == 0 {
      return invalid("max_accounting_failures must be greater than zero");
    }
    if self.retry_initial_delay_milliseconds > self.retry_max_delay_seconds.saturating_mul(1000) {
      return invalid("retry_initial_delay_milliseconds must not exceed retry_max_delay_seconds");
    }
    if self.heartbeat_interval_seconds >= self.lease_safety_margin_seconds {
      return invalid("heartbeat_interval_seconds must be shorter than lease_safety_margin_seconds");
    }

    info!(
      agent_id = %self.agent_id,
      runtime_modes = self.enabled_runtime_modes.len(),
      "validated agent configuration"
    );
    let mut runtimes = Vec::with_capacity(self.enabled_runtime_modes.len());
    if let Some(native_runtime) = native_runtime {
      runtimes.push(native_runtime);
    }
    if modes.contains(&RuntimeMode::Oci) {
      runtimes.push(ValidatedRuntimeConfig::Oci {
        engines: self.oci_engines.clone(),
      });
    }
    Ok(ValidatedConfig {
      config: self,
      signing_keys,
      runtimes,
    })
  }
}

#[cfg(test)]
mod tests;
