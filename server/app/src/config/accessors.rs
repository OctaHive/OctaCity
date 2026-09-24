use std::{net::SocketAddr, path::Path, time::Duration};

use super::{
  AgentCredentialConfig, CacheConfig, JobSpecConfig, ObjectStorageConfig, PostgresConfig, RetryingWorkerPolicy,
  ServerConfig, SigningConfig, VcsIntegrationConfig, WebhookWorkerPolicy, WorkerPolicy,
};

impl ServerConfig {
  /// Address on which the management and health listener is bound.
  pub const fn management_bind(&self) -> SocketAddr {
    self.management_bind
  }

  /// Optional address for authenticated Agent protocol ingress.
  pub const fn agent_bind(&self) -> Option<SocketAddr> {
    self.agent_bind
  }

  /// Optional address for authenticated Octa HTTP cache ingress.
  pub const fn cache_bind(&self) -> Option<SocketAddr> {
    self.cache_bind
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

  pub(crate) const fn trigger_evaluation_worker(&self) -> RetryingWorkerPolicy {
    RetryingWorkerPolicy::new(
      WorkerPolicy::new(
        self.trigger_evaluation_poll_interval_milliseconds,
        self.trigger_evaluation_claim_lifetime_milliseconds,
        self.trigger_evaluation_batch_size,
      ),
      self.vcs_retry_max_attempts,
      self.vcs_retry_initial_milliseconds,
      self.vcs_retry_maximum_milliseconds,
    )
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

  pub(crate) const fn lease_expiry_worker(&self) -> WorkerPolicy {
    WorkerPolicy::new(
      self.lease_expiry_poll_interval_milliseconds,
      self.lease_expiry_claim_lifetime_milliseconds,
      self.lease_expiry_batch_size,
    )
  }

  pub(crate) const fn schedule_worker(&self) -> WorkerPolicy {
    WorkerPolicy::new(
      self.schedule_poll_interval_milliseconds,
      self.schedule_claim_lifetime_milliseconds,
      self.schedule_batch_size,
    )
  }

  pub(crate) const fn internal_trigger_worker(&self) -> WorkerPolicy {
    WorkerPolicy::new(
      self.internal_trigger_poll_interval_milliseconds,
      self.internal_trigger_claim_lifetime_milliseconds,
      self.internal_trigger_batch_size,
    )
  }

  pub(crate) const fn log_index_worker(&self) -> RetryingWorkerPolicy {
    RetryingWorkerPolicy::new(
      WorkerPolicy::new(
        self.log_index_poll_interval_milliseconds,
        self.log_index_claim_lifetime_milliseconds,
        self.log_index_batch_size,
      ),
      self.log_index_max_attempts,
      self.log_index_initial_retry_milliseconds,
      self.log_index_maximum_retry_milliseconds,
    )
  }

  pub(crate) const fn webhook_worker(&self) -> WebhookWorkerPolicy {
    WebhookWorkerPolicy::new(
      RetryingWorkerPolicy::new(
        WorkerPolicy::new(
          self.webhook_worker_poll_interval_milliseconds,
          self.webhook_worker_claim_lifetime_milliseconds,
          self.webhook_delivery_batch_size,
        ),
        self.webhook_worker_max_attempts,
        self.webhook_worker_initial_retry_milliseconds,
        self.webhook_worker_maximum_retry_milliseconds,
      ),
      self.managed_webhook_batch_size,
    )
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
}
