use std::{collections::BTreeSet, fs::File, io::Read as _, net::SocketAddr, path::Path, path::PathBuf, time::Duration};

use octacity_server_domain::IntegrationId;
use serde::Deserialize;
use thiserror::Error;

mod dependencies;

pub(crate) use dependencies::{
  AgentCredentialConfig, CacheConfig, JobSpecConfig, ObjectStorageConfig, PostgresConfig, SigningConfig,
};

const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_SHUTDOWN_GRACE_MILLISECONDS: u64 = 5 * 60 * 1000;
const MAX_READINESS_CHECK_MILLISECONDS: u64 = 5 * 60 * 1000;
const MAX_AGENT_REGISTRATION_LIFETIME_MILLISECONDS: u64 = 30 * 24 * 60 * 60 * 1000;
const MAX_AGENT_ENROLLMENT_LIFETIME_MILLISECONDS: u64 = 24 * 60 * 60 * 1000;
const MAX_AGENT_RETRY_DELAY_MILLISECONDS: u64 = 5 * 60 * 1000;
const MAX_AGENT_LEASE_LIFETIME_MILLISECONDS: u64 = 24 * 60 * 60 * 1000;
const MAX_LEASE_EXPIRY_WORKER_MILLISECONDS: u64 = 5 * 60 * 1000;
const MAX_SCHEDULE_WORKER_MILLISECONDS: u64 = 5 * 60 * 1000;
const MAX_INTERNAL_TRIGGER_WORKER_MILLISECONDS: u64 = 5 * 60 * 1000;
const MAX_WEBHOOK_WORKER_MILLISECONDS: u64 = 5 * 60 * 1000;
const MAX_WEBHOOK_WORKER_ATTEMPTS: u16 = 100;
const MAX_TRIGGER_EVALUATION_WORKER_MILLISECONDS: u64 = 24 * 60 * 60 * 1000;
const MAX_TRIGGER_EVALUATION_ATTEMPTS: u16 = 100;
const MAX_READY_JOB_LISTENER_RECONNECT_MILLISECONDS: u64 = 60 * 1000;
const MAX_WEBHOOK_OPERATION_MILLISECONDS: u64 = 5 * 60 * 1000;
const WEBHOOK_CLAIM_COMPLETION_MARGIN_MILLISECONDS: u64 = 1_000;
const TRIGGER_EVALUATION_CLAIM_COMPLETION_MARGIN_MILLISECONDS: u64 = 1_000;
const MAX_VCS_OPERATION_MILLISECONDS: u64 = 5 * 60 * 1000;

#[derive(Clone, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct VcsIntegrationConfig {
  integration_id: IntegrationId,
  adapter_id: String,
  adapter_sha256: String,
  credential_handle: String,
}

impl std::fmt::Debug for VcsIntegrationConfig {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("VcsIntegrationConfig")
      .field("integration_id", &self.integration_id)
      .field("adapter_id", &self.adapter_id)
      .field("adapter_sha256", &self.adapter_sha256)
      .field("credential_handle", &"<redacted>")
      .finish()
  }
}

impl VcsIntegrationConfig {
  pub(crate) const fn integration_id(&self) -> IntegrationId {
    self.integration_id
  }

  pub(crate) fn adapter_id(&self) -> &str {
    &self.adapter_id
  }

  pub(crate) fn adapter_sha256(&self) -> &str {
    &self.adapter_sha256
  }

  pub(crate) fn credential_handle(&self) -> &str {
    &self.credential_handle
  }

  fn validate(&self) -> bool {
    let valid_field = |value: &str| {
      !value.is_empty()
        && value.len() <= octacity_vcs_protocol::MAX_FIELD_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
    };
    valid_field(&self.adapter_id)
      && valid_field(&self.credential_handle)
      && self.adapter_sha256.len() == 64
      && self
        .adapter_sha256
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
  }
}

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
  webhook_public_base_url: Option<String>,
  #[serde(default)]
  webhook_adapter_registry: Option<PathBuf>,
  #[serde(default = "default_webhook_operation_timeout_milliseconds")]
  webhook_operation_timeout_milliseconds: u64,
  #[serde(default = "default_webhook_cancellation_grace_milliseconds")]
  webhook_cancellation_grace_milliseconds: u64,
  #[serde(default)]
  vcs_adapter_registry: Option<PathBuf>,
  #[serde(default)]
  vcs_integrations: Vec<VcsIntegrationConfig>,
  #[serde(default = "default_vcs_operation_timeout_milliseconds")]
  vcs_operation_timeout_milliseconds: u64,
  #[serde(default = "default_vcs_cancellation_grace_milliseconds")]
  vcs_cancellation_grace_milliseconds: u64,
  #[serde(default = "default_trigger_evaluation_poll_interval_milliseconds")]
  trigger_evaluation_poll_interval_milliseconds: u64,
  #[serde(default = "default_trigger_evaluation_claim_lifetime_milliseconds")]
  trigger_evaluation_claim_lifetime_milliseconds: u64,
  #[serde(default = "default_trigger_evaluation_batch_size")]
  trigger_evaluation_batch_size: u16,
  #[serde(default = "default_vcs_retry_max_attempts")]
  vcs_retry_max_attempts: u16,
  #[serde(default = "default_vcs_retry_initial_milliseconds")]
  vcs_retry_initial_milliseconds: u64,
  #[serde(default = "default_vcs_retry_maximum_milliseconds")]
  vcs_retry_maximum_milliseconds: u64,
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
  #[serde(default = "default_schedule_poll_interval_milliseconds")]
  schedule_poll_interval_milliseconds: u64,
  #[serde(default = "default_schedule_claim_lifetime_milliseconds")]
  schedule_claim_lifetime_milliseconds: u64,
  #[serde(default = "default_schedule_batch_size")]
  schedule_batch_size: u16,
  #[serde(default = "default_internal_trigger_poll_interval_milliseconds")]
  internal_trigger_poll_interval_milliseconds: u64,
  #[serde(default = "default_internal_trigger_claim_lifetime_milliseconds")]
  internal_trigger_claim_lifetime_milliseconds: u64,
  #[serde(default = "default_internal_trigger_batch_size")]
  internal_trigger_batch_size: u16,
  #[serde(
    default = "default_webhook_worker_poll_interval_milliseconds",
    alias = "webhook_delivery_poll_interval_milliseconds"
  )]
  webhook_worker_poll_interval_milliseconds: u64,
  #[serde(
    default = "default_webhook_worker_claim_lifetime_milliseconds",
    alias = "webhook_delivery_claim_lifetime_milliseconds"
  )]
  webhook_worker_claim_lifetime_milliseconds: u64,
  #[serde(default = "default_webhook_delivery_batch_size")]
  webhook_delivery_batch_size: u16,
  #[serde(default = "default_managed_webhook_batch_size")]
  managed_webhook_batch_size: u16,
  #[serde(
    default = "default_webhook_worker_max_attempts",
    alias = "webhook_delivery_max_attempts"
  )]
  webhook_worker_max_attempts: u16,
  #[serde(
    default = "default_webhook_worker_initial_retry_milliseconds",
    alias = "webhook_delivery_initial_retry_milliseconds"
  )]
  webhook_worker_initial_retry_milliseconds: u64,
  #[serde(
    default = "default_webhook_worker_maximum_retry_milliseconds",
    alias = "webhook_delivery_maximum_retry_milliseconds"
  )]
  webhook_worker_maximum_retry_milliseconds: u64,
  #[serde(default = "default_ready_job_listener_reconnect_milliseconds")]
  ready_job_listener_reconnect_milliseconds: u64,
  supported_pipeline_capabilities: Vec<String>,
  postgres: PostgresConfig,
  object_storage: ObjectStorageConfig,
  signing: SigningConfig,
  agent_credentials: AgentCredentialConfig,
  cache: CacheConfig,
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

  /// Optional address for authenticated webhook ingress.
  pub const fn webhook_bind(&self) -> Option<SocketAddr> {
    self.webhook_bind
  }

  /// Validated public origin used to construct operator-facing callback URLs.
  pub fn webhook_callback_origin(&self) -> Option<octacity_server_application::WebhookCallbackOrigin> {
    self
      .webhook_public_base_url
      .as_ref()
      .map(|value| octacity_server_application::WebhookCallbackOrigin::new(value.clone()).expect("config is validated"))
  }

  /// Operator-owned directory containing verified webhook adapters.
  pub fn webhook_adapter_registry(&self) -> Option<&Path> {
    self.webhook_adapter_registry.as_deref()
  }

  /// Absolute deadline for one provider verification operation.
  pub const fn webhook_operation_timeout(&self) -> Duration {
    Duration::from_millis(self.webhook_operation_timeout_milliseconds)
  }

  /// Grace allowed for cooperative adapter cancellation before force-stop.
  pub const fn webhook_cancellation_grace(&self) -> Duration {
    Duration::from_millis(self.webhook_cancellation_grace_milliseconds)
  }

  /// Operator-owned directory containing verified VCS adapters.
  pub fn vcs_adapter_registry(&self) -> Option<&Path> {
    self.vcs_adapter_registry.as_deref()
  }

  /// Configured provider-neutral VCS integrations.
  pub(crate) fn vcs_integrations(&self) -> &[VcsIntegrationConfig] {
    &self.vcs_integrations
  }

  /// Absolute deadline for one VCS adapter operation.
  pub const fn vcs_operation_timeout(&self) -> Duration {
    Duration::from_millis(self.vcs_operation_timeout_milliseconds)
  }

  /// Grace allowed for cooperative VCS cancellation before force-stop.
  pub const fn vcs_cancellation_grace(&self) -> Duration {
    Duration::from_millis(self.vcs_cancellation_grace_milliseconds)
  }

  /// Interval between scans for manual Trigger evaluations awaiting a VCS retry.
  pub const fn trigger_evaluation_poll_interval(&self) -> Duration {
    Duration::from_millis(self.trigger_evaluation_poll_interval_milliseconds)
  }

  /// Exclusive ownership window for a bounded batch of manual Trigger evaluations.
  pub const fn trigger_evaluation_claim_lifetime(&self) -> Duration {
    Duration::from_millis(self.trigger_evaluation_claim_lifetime_milliseconds)
  }

  /// Maximum manual Trigger evaluations advanced in one worker pass.
  pub const fn trigger_evaluation_batch_size(&self) -> u16 {
    self.trigger_evaluation_batch_size
  }

  /// Maximum VCS evaluation attempts before retaining a dead letter.
  pub const fn vcs_retry_max_attempts(&self) -> u16 {
    self.vcs_retry_max_attempts
  }

  /// Initial exponential delay after a transient VCS failure.
  pub const fn vcs_retry_initial_milliseconds(&self) -> u64 {
    self.vcs_retry_initial_milliseconds
  }

  /// Ceiling for exponential VCS retry delays.
  pub const fn vcs_retry_maximum_milliseconds(&self) -> u64 {
    self.vcs_retry_maximum_milliseconds
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

  /// Interval between authoritative scans for due schedules.
  pub const fn schedule_poll_interval(&self) -> Duration {
    Duration::from_millis(self.schedule_poll_interval_milliseconds)
  }

  /// Exclusive ownership window for one due-schedule worker claim.
  pub const fn schedule_claim_lifetime(&self) -> Duration {
    Duration::from_millis(self.schedule_claim_lifetime_milliseconds)
  }

  /// Maximum due schedules evaluated in one worker pass.
  pub const fn schedule_batch_size(&self) -> u16 {
    self.schedule_batch_size
  }

  /// Interval between authoritative scans for terminal Build outbox events.
  pub const fn internal_trigger_poll_interval(&self) -> Duration {
    Duration::from_millis(self.internal_trigger_poll_interval_milliseconds)
  }

  /// Exclusive ownership window for one internal-Trigger outbox claim.
  pub const fn internal_trigger_claim_lifetime(&self) -> Duration {
    Duration::from_millis(self.internal_trigger_claim_lifetime_milliseconds)
  }

  /// Maximum terminal Build events delivered in one worker pass.
  pub const fn internal_trigger_batch_size(&self) -> u16 {
    self.internal_trigger_batch_size
  }

  /// Shared poll interval for delivery verification and managed webhook work.
  pub const fn webhook_worker_poll_interval(&self) -> Duration {
    Duration::from_millis(self.webhook_worker_poll_interval_milliseconds)
  }

  /// Shared exclusive ownership window for one webhook worker claim.
  pub const fn webhook_worker_claim_lifetime(&self) -> Duration {
    Duration::from_millis(self.webhook_worker_claim_lifetime_milliseconds)
  }

  /// Maximum webhook receipts advanced in one worker pass.
  pub const fn webhook_delivery_batch_size(&self) -> u16 {
    self.webhook_delivery_batch_size
  }

  /// Maximum managed provider operations advanced in one worker pass.
  pub const fn managed_webhook_batch_size(&self) -> u16 {
    self.managed_webhook_batch_size
  }

  /// Maximum webhook adapter attempts before dead-lettering.
  pub const fn webhook_worker_max_attempts(&self) -> u16 {
    self.webhook_worker_max_attempts
  }

  /// Initial exponential retry delay for transient adapter failures.
  pub const fn webhook_worker_initial_retry_milliseconds(&self) -> u64 {
    self.webhook_worker_initial_retry_milliseconds
  }

  /// Ceiling for exponential webhook adapter retry delays.
  pub const fn webhook_worker_maximum_retry_milliseconds(&self) -> u64 {
    self.webhook_worker_maximum_retry_milliseconds
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

  pub(crate) const fn cache(&self) -> &CacheConfig {
    &self.cache
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
    let webhook_companions = self.webhook_public_base_url.is_some() || self.webhook_adapter_registry.is_some();
    if self.webhook_bind.is_some()
      != (self.webhook_public_base_url.is_some() && self.webhook_adapter_registry.is_some())
      || self.webhook_bind.is_none() && webhook_companions
    {
      return Err(ServerConfigError::Invalid(
        "webhook_bind, webhook_public_base_url, and webhook_adapter_registry must be configured together".to_owned(),
      ));
    }
    if let Some(base) = &self.webhook_public_base_url
      && octacity_server_application::WebhookCallbackOrigin::new(base.clone()).is_err()
    {
      return Err(ServerConfigError::Invalid(
        "webhook_public_base_url must be a credential-free HTTP(S) origin without a path, query, fragment, or trailing slash"
          .to_owned(),
      ));
    }
    if self
      .webhook_adapter_registry
      .as_ref()
      .is_some_and(|path| path.as_os_str().is_empty())
    {
      return Err(ServerConfigError::Invalid(
        "webhook_adapter_registry must not be empty".to_owned(),
      ));
    }
    if self.vcs_adapter_registry.is_some() != !self.vcs_integrations.is_empty() {
      return Err(ServerConfigError::Invalid(
        "vcs_adapter_registry and at least one vcs_integrations entry must be configured together".to_owned(),
      ));
    }
    if self
      .vcs_adapter_registry
      .as_ref()
      .is_some_and(|path| path.as_os_str().is_empty())
      || self.vcs_integrations.iter().any(|integration| !integration.validate())
      || self
        .vcs_integrations
        .iter()
        .map(VcsIntegrationConfig::integration_id)
        .collect::<BTreeSet<_>>()
        .len()
        != self.vcs_integrations.len()
    {
      return Err(ServerConfigError::Invalid(
        "VCS integration identities, adapter pins, and credential handles must be non-empty, bounded, and unique"
          .to_owned(),
      ));
    }
    let listeners = [
      ("management_bind", Some(self.management_bind)),
      ("agent_bind", self.agent_bind),
      ("webhook_bind", self.webhook_bind),
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
    if self.schedule_poll_interval_milliseconds == 0
      || self.schedule_poll_interval_milliseconds > MAX_SCHEDULE_WORKER_MILLISECONDS
      || self.schedule_claim_lifetime_milliseconds == 0
      || self.schedule_claim_lifetime_milliseconds > MAX_SCHEDULE_WORKER_MILLISECONDS
      || self.schedule_batch_size == 0
      || self.schedule_batch_size > octacity_server_store::MAX_SCHEDULE_CLAIM_BATCH_SIZE
    {
      return Err(ServerConfigError::Invalid(format!(
        "schedule worker intervals and batch must be positive and bounded by {MAX_SCHEDULE_WORKER_MILLISECONDS} ms / {} items",
        octacity_server_store::MAX_SCHEDULE_CLAIM_BATCH_SIZE
      )));
    }
    if self.internal_trigger_poll_interval_milliseconds == 0
      || self.internal_trigger_poll_interval_milliseconds > MAX_INTERNAL_TRIGGER_WORKER_MILLISECONDS
      || self.internal_trigger_claim_lifetime_milliseconds == 0
      || self.internal_trigger_claim_lifetime_milliseconds > MAX_INTERNAL_TRIGGER_WORKER_MILLISECONDS
      || self.internal_trigger_batch_size == 0
      || self.internal_trigger_batch_size > octacity_server_store::MAX_INTERNAL_TRIGGER_EVENT_BATCH_SIZE
    {
      return Err(ServerConfigError::Invalid(format!(
        "internal Trigger worker intervals and batch must be positive and bounded by {MAX_INTERNAL_TRIGGER_WORKER_MILLISECONDS} ms / {} items",
        octacity_server_store::MAX_INTERNAL_TRIGGER_EVENT_BATCH_SIZE
      )));
    }
    if self.webhook_worker_poll_interval_milliseconds == 0
      || self.webhook_worker_poll_interval_milliseconds > MAX_WEBHOOK_WORKER_MILLISECONDS
      || self.webhook_worker_claim_lifetime_milliseconds == 0
      || self.webhook_worker_claim_lifetime_milliseconds > MAX_WEBHOOK_WORKER_MILLISECONDS
      || self.webhook_delivery_batch_size == 0
      || self.webhook_delivery_batch_size > octacity_server_store::MAX_WEBHOOK_DELIVERY_BATCH_SIZE
      || self.managed_webhook_batch_size == 0
      || self.managed_webhook_batch_size > octacity_server_store::MAX_MANAGED_WEBHOOK_OPERATION_BATCH_SIZE
      || self.webhook_worker_max_attempts == 0
      || self.webhook_worker_max_attempts > MAX_WEBHOOK_WORKER_ATTEMPTS
      || self.webhook_worker_initial_retry_milliseconds == 0
      || self.webhook_worker_maximum_retry_milliseconds < self.webhook_worker_initial_retry_milliseconds
      || self.webhook_worker_maximum_retry_milliseconds > MAX_WEBHOOK_WORKER_MILLISECONDS
    {
      return Err(ServerConfigError::Invalid(format!(
        "webhook worker policy must be positive and bounded by {MAX_WEBHOOK_WORKER_MILLISECONDS} ms / {} delivery items / {} managed items / {MAX_WEBHOOK_WORKER_ATTEMPTS} attempts",
        octacity_server_store::MAX_WEBHOOK_DELIVERY_BATCH_SIZE,
        octacity_server_store::MAX_MANAGED_WEBHOOK_OPERATION_BATCH_SIZE,
      )));
    }
    if self.ready_job_listener_reconnect_milliseconds == 0
      || self.ready_job_listener_reconnect_milliseconds > MAX_READY_JOB_LISTENER_RECONNECT_MILLISECONDS
    {
      return Err(ServerConfigError::Invalid(format!(
        "ready_job_listener_reconnect_milliseconds must be between 1 and {MAX_READY_JOB_LISTENER_RECONNECT_MILLISECONDS}"
      )));
    }
    if self.webhook_operation_timeout_milliseconds == 0
      || self.webhook_operation_timeout_milliseconds > MAX_WEBHOOK_OPERATION_MILLISECONDS
      || self.webhook_cancellation_grace_milliseconds == 0
      || self.webhook_cancellation_grace_milliseconds > MAX_WEBHOOK_OPERATION_MILLISECONDS
    {
      return Err(ServerConfigError::Invalid(format!(
        "webhook operation timeouts must be between 1 and {MAX_WEBHOOK_OPERATION_MILLISECONDS} milliseconds"
      )));
    }
    if self.vcs_operation_timeout_milliseconds == 0
      || self.vcs_operation_timeout_milliseconds > MAX_VCS_OPERATION_MILLISECONDS
      || self.vcs_cancellation_grace_milliseconds == 0
      || self.vcs_cancellation_grace_milliseconds > MAX_VCS_OPERATION_MILLISECONDS
    {
      return Err(ServerConfigError::Invalid(format!(
        "VCS operation timeouts must be between 1 and {MAX_VCS_OPERATION_MILLISECONDS} milliseconds"
      )));
    }
    if self.trigger_evaluation_poll_interval_milliseconds == 0
      || self.trigger_evaluation_poll_interval_milliseconds > MAX_TRIGGER_EVALUATION_WORKER_MILLISECONDS
      || self.trigger_evaluation_claim_lifetime_milliseconds == 0
      || self.trigger_evaluation_claim_lifetime_milliseconds > MAX_TRIGGER_EVALUATION_WORKER_MILLISECONDS
      || self.trigger_evaluation_batch_size == 0
      || self.trigger_evaluation_batch_size > octacity_server_store::MAX_TRIGGER_EVALUATION_BATCH_SIZE
      || self.vcs_retry_max_attempts == 0
      || self.vcs_retry_max_attempts > MAX_TRIGGER_EVALUATION_ATTEMPTS
      || self.vcs_retry_initial_milliseconds == 0
      || self.vcs_retry_maximum_milliseconds < self.vcs_retry_initial_milliseconds
      || self.vcs_retry_maximum_milliseconds > MAX_TRIGGER_EVALUATION_WORKER_MILLISECONDS
    {
      return Err(ServerConfigError::Invalid(format!(
        "manual Trigger retry policy must be positive and bounded by {MAX_TRIGGER_EVALUATION_WORKER_MILLISECONDS} ms / {} items / {MAX_TRIGGER_EVALUATION_ATTEMPTS} attempts",
        octacity_server_store::MAX_TRIGGER_EVALUATION_BATCH_SIZE,
      )));
    }
    let trigger_evaluation_batch_budget = self
      .vcs_operation_timeout_milliseconds
      .checked_add(self.vcs_cancellation_grace_milliseconds)
      .and_then(|operation| operation.checked_mul(u64::from(self.trigger_evaluation_batch_size)))
      .and_then(|batch| batch.checked_add(TRIGGER_EVALUATION_CLAIM_COMPLETION_MARGIN_MILLISECONDS));
    if trigger_evaluation_batch_budget
      .is_none_or(|budget| budget >= self.trigger_evaluation_claim_lifetime_milliseconds)
    {
      return Err(ServerConfigError::Invalid(format!(
        "manual Trigger claim lifetime must exceed the sequential VCS budget plus the {TRIGGER_EVALUATION_CLAIM_COMPLETION_MARGIN_MILLISECONDS} ms completion margin"
      )));
    }
    let webhook_batch_budget = self
      .webhook_operation_timeout_milliseconds
      .checked_add(self.webhook_cancellation_grace_milliseconds)
      .and_then(|operation| {
        operation.checked_mul(u64::from(
          self.webhook_delivery_batch_size.max(self.managed_webhook_batch_size),
        ))
      })
      .and_then(|batch| batch.checked_add(WEBHOOK_CLAIM_COMPLETION_MARGIN_MILLISECONDS));
    if webhook_batch_budget.is_none_or(|budget| budget >= self.webhook_worker_claim_lifetime_milliseconds) {
      return Err(ServerConfigError::Invalid(format!(
        "webhook worker claim lifetime must exceed the sequential adapter budget plus the {WEBHOOK_CLAIM_COMPLETION_MARGIN_MILLISECONDS} ms completion margin"
      )));
    }
    self.postgres.validate().map_err(ServerConfigError::Invalid)?;
    self.object_storage.validate().map_err(ServerConfigError::Invalid)?;
    self.signing.validate().map_err(ServerConfigError::Invalid)?;
    self.agent_credentials.validate().map_err(ServerConfigError::Invalid)?;
    self.cache.validate().map_err(ServerConfigError::Invalid)?;
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

const fn default_schedule_poll_interval_milliseconds() -> u64 {
  1_000
}

const fn default_schedule_claim_lifetime_milliseconds() -> u64 {
  30_000
}

const fn default_schedule_batch_size() -> u16 {
  32
}

const fn default_internal_trigger_poll_interval_milliseconds() -> u64 {
  1_000
}

const fn default_internal_trigger_claim_lifetime_milliseconds() -> u64 {
  30_000
}

const fn default_internal_trigger_batch_size() -> u16 {
  32
}

const fn default_webhook_worker_poll_interval_milliseconds() -> u64 {
  1_000
}

const fn default_webhook_worker_claim_lifetime_milliseconds() -> u64 {
  30_000
}

const fn default_webhook_delivery_batch_size() -> u16 {
  4
}

const fn default_managed_webhook_batch_size() -> u16 {
  4
}

const fn default_webhook_worker_max_attempts() -> u16 {
  5
}

const fn default_webhook_worker_initial_retry_milliseconds() -> u64 {
  1_000
}

const fn default_webhook_worker_maximum_retry_milliseconds() -> u64 {
  60_000
}

const fn default_ready_job_listener_reconnect_milliseconds() -> u64 {
  1_000
}

const fn default_webhook_operation_timeout_milliseconds() -> u64 {
  5_000
}

const fn default_webhook_cancellation_grace_milliseconds() -> u64 {
  1_000
}

const fn default_vcs_operation_timeout_milliseconds() -> u64 {
  30_000
}

const fn default_vcs_cancellation_grace_milliseconds() -> u64 {
  1_000
}

const fn default_trigger_evaluation_poll_interval_milliseconds() -> u64 {
  1_000
}

const fn default_trigger_evaluation_claim_lifetime_milliseconds() -> u64 {
  180_000
}

const fn default_trigger_evaluation_batch_size() -> u16 {
  4
}

const fn default_vcs_retry_max_attempts() -> u16 {
  5
}

const fn default_vcs_retry_initial_milliseconds() -> u64 {
  1_000
}

const fn default_vcs_retry_maximum_milliseconds() -> u64 {
  60_000
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
