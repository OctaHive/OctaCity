use std::{net::SocketAddr, path::PathBuf};

use serde::Deserialize;

mod accessors;
mod defaults;
mod infrastructure;
mod integrations;
mod loading;
mod validation;
mod workers;

use defaults::*;
pub(crate) use infrastructure::{
  AgentCredentialConfig, CacheConfig, JobSpecConfig, ObjectStorageConfig, PostgresConfig, SigningConfig,
};
pub(crate) use integrations::VcsIntegrationConfig;
pub use loading::ServerConfigError;
pub(crate) use workers::{RetentionWorkerPolicy, RetryingWorkerPolicy, WebhookWorkerPolicy, WorkerPolicy};

/// Validated operator configuration for the server process.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
  #[serde(default = "default_management_bind")]
  management_bind: SocketAddr,
  #[serde(default)]
  agent_bind: Option<SocketAddr>,
  #[serde(default)]
  cache_bind: Option<SocketAddr>,
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
  #[serde(default = "default_log_index_poll_interval_milliseconds")]
  log_index_poll_interval_milliseconds: u64,
  #[serde(default = "default_log_index_claim_lifetime_milliseconds")]
  log_index_claim_lifetime_milliseconds: u64,
  #[serde(default = "default_log_index_batch_size")]
  log_index_batch_size: u16,
  #[serde(default = "default_log_index_max_attempts")]
  log_index_max_attempts: u16,
  #[serde(default = "default_log_index_initial_retry_milliseconds")]
  log_index_initial_retry_milliseconds: u64,
  #[serde(default = "default_log_index_maximum_retry_milliseconds")]
  log_index_maximum_retry_milliseconds: u64,
  #[serde(default = "default_retention_poll_interval_milliseconds")]
  retention_poll_interval_milliseconds: u64,
  #[serde(default = "default_retention_claim_lifetime_milliseconds")]
  retention_claim_lifetime_milliseconds: u64,
  #[serde(default = "default_retention_work_batch_size")]
  retention_work_batch_size: u16,
  #[serde(default = "default_retention_object_batch_size")]
  retention_object_batch_size: u16,
  #[serde(default = "default_retention_max_attempts")]
  retention_max_attempts: u16,
  #[serde(default = "default_retention_initial_retry_milliseconds")]
  retention_initial_retry_milliseconds: u64,
  #[serde(default = "default_retention_maximum_retry_milliseconds")]
  retention_maximum_retry_milliseconds: u64,
  #[serde(default = "default_orphan_log_cleanup_grace_milliseconds")]
  orphan_log_cleanup_grace_milliseconds: u64,
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
