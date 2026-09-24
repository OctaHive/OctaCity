use std::{net::SocketAddr, path::Path, time::Duration};

use super::{
  AgentCredentialConfig, CacheConfig, JobSpecConfig, ObjectStorageConfig, PostgresConfig, ServerConfig, SigningConfig,
  VcsIntegrationConfig,
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
}
