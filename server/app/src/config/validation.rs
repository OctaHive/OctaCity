use std::{collections::BTreeSet, net::SocketAddr, time::Duration};

use super::{ServerConfig, ServerConfigError, VcsIntegrationConfig};

const MAX_SHUTDOWN_GRACE_MILLISECONDS: u64 = 5 * 60 * 1000;
const MAX_READINESS_CHECK_MILLISECONDS: u64 = 5 * 60 * 1000;
const MAX_AGENT_REGISTRATION_LIFETIME_MILLISECONDS: u64 = 30 * 24 * 60 * 60 * 1000;
const MAX_AGENT_ENROLLMENT_LIFETIME_MILLISECONDS: u64 = 24 * 60 * 60 * 1000;
const MAX_AGENT_RETRY_DELAY_MILLISECONDS: u64 = 5 * 60 * 1000;
const MAX_AGENT_LEASE_LIFETIME_MILLISECONDS: u64 = 24 * 60 * 60 * 1000;
const MAX_AGENT_TELEMETRY_EXPORT_TIMEOUT_MILLISECONDS: u64 = 5 * 60 * 1000;
const MAX_AGENT_TELEMETRY_IN_FLIGHT_EXPORTS: u16 = 64;
const MAX_LEASE_EXPIRY_WORKER_MILLISECONDS: u64 = 5 * 60 * 1000;
const MAX_SCHEDULE_WORKER_MILLISECONDS: u64 = 5 * 60 * 1000;
const MAX_INTERNAL_TRIGGER_WORKER_MILLISECONDS: u64 = 5 * 60 * 1000;
const MAX_LOG_INDEX_WORKER_MILLISECONDS: u64 = 5 * 60 * 1000;
const MAX_LOG_INDEX_WORKER_ATTEMPTS: u16 = 100;
const MAX_RETENTION_WORKER_MILLISECONDS: u64 = 30 * 60 * 1000;
const MAX_RETENTION_WORKER_ATTEMPTS: u16 = 100;
const MAX_ORPHAN_LOG_CLEANUP_GRACE_MILLISECONDS: u64 = 24 * 60 * 60 * 1000;
const MAX_WEBHOOK_WORKER_MILLISECONDS: u64 = 5 * 60 * 1000;
const MAX_WEBHOOK_WORKER_ATTEMPTS: u16 = 100;
const MAX_TRIGGER_EVALUATION_WORKER_MILLISECONDS: u64 = 24 * 60 * 60 * 1000;
const MAX_TRIGGER_EVALUATION_ATTEMPTS: u16 = 100;
const MAX_READY_JOB_LISTENER_RECONNECT_MILLISECONDS: u64 = 60 * 1000;
const MAX_WEBHOOK_OPERATION_MILLISECONDS: u64 = 5 * 60 * 1000;
const WEBHOOK_CLAIM_COMPLETION_MARGIN_MILLISECONDS: u64 = 1_000;
const TRIGGER_EVALUATION_CLAIM_COMPLETION_MARGIN_MILLISECONDS: u64 = 1_000;
const LOG_INDEX_CLAIM_COMPLETION_MARGIN_MILLISECONDS: u64 = 1_000;
const RETENTION_CLAIM_COMPLETION_MARGIN_MILLISECONDS: u64 = 1_000;
const MAX_VCS_OPERATION_MILLISECONDS: u64 = 5 * 60 * 1000;

impl ServerConfig {
  pub(super) fn validate(&self) -> Result<(), ServerConfigError> {
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
      ("cache_bind", self.cache_bind),
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
    if self.agent_telemetry_export_timeout().is_zero() || self.agent_telemetry_max_in_flight_exports() == 0 {
      return Err(ServerConfigError::Invalid(format!(
        "Agent telemetry export must be bounded by {MAX_AGENT_TELEMETRY_EXPORT_TIMEOUT_MILLISECONDS} ms / {MAX_AGENT_TELEMETRY_IN_FLIGHT_EXPORTS} in-flight batches"
      )));
    }
    if self.agent_telemetry_export_timeout() > Duration::from_millis(MAX_AGENT_TELEMETRY_EXPORT_TIMEOUT_MILLISECONDS)
      || self.agent_telemetry_max_in_flight_exports() > usize::from(MAX_AGENT_TELEMETRY_IN_FLIGHT_EXPORTS)
    {
      return Err(ServerConfigError::Invalid(format!(
        "Agent telemetry export must be bounded by {MAX_AGENT_TELEMETRY_EXPORT_TIMEOUT_MILLISECONDS} ms / {MAX_AGENT_TELEMETRY_IN_FLIGHT_EXPORTS} in-flight batches"
      )));
    }
    if !self.lease_expiry_worker().is_bounded(
      Duration::from_millis(MAX_LEASE_EXPIRY_WORKER_MILLISECONDS),
      octacity_server_store::MAX_LEASE_EXPIRY_BATCH_SIZE,
    ) {
      return Err(ServerConfigError::Invalid(format!(
        "lease expiry worker intervals and batch must be positive and bounded by {MAX_LEASE_EXPIRY_WORKER_MILLISECONDS} ms / {} items",
        octacity_server_store::MAX_LEASE_EXPIRY_BATCH_SIZE
      )));
    }
    if !self.schedule_worker().is_bounded(
      Duration::from_millis(MAX_SCHEDULE_WORKER_MILLISECONDS),
      octacity_server_store::MAX_SCHEDULE_CLAIM_BATCH_SIZE,
    ) {
      return Err(ServerConfigError::Invalid(format!(
        "schedule worker intervals and batch must be positive and bounded by {MAX_SCHEDULE_WORKER_MILLISECONDS} ms / {} items",
        octacity_server_store::MAX_SCHEDULE_CLAIM_BATCH_SIZE
      )));
    }
    if !self.internal_trigger_worker().is_bounded(
      Duration::from_millis(MAX_INTERNAL_TRIGGER_WORKER_MILLISECONDS),
      octacity_server_store::MAX_INTERNAL_TRIGGER_EVENT_BATCH_SIZE,
    ) {
      return Err(ServerConfigError::Invalid(format!(
        "internal Trigger worker intervals and batch must be positive and bounded by {MAX_INTERNAL_TRIGGER_WORKER_MILLISECONDS} ms / {} items",
        octacity_server_store::MAX_INTERNAL_TRIGGER_EVENT_BATCH_SIZE
      )));
    }
    if !self.log_index_worker().is_bounded(
      Duration::from_millis(MAX_LOG_INDEX_WORKER_MILLISECONDS),
      octacity_server_store::MAX_LOG_INDEX_WORK_BATCH_SIZE,
      MAX_LOG_INDEX_WORKER_ATTEMPTS,
    ) {
      return Err(ServerConfigError::Invalid(format!(
        "log-index worker policy must be positive and bounded by {MAX_LOG_INDEX_WORKER_MILLISECONDS} ms / {} items / {MAX_LOG_INDEX_WORKER_ATTEMPTS} attempts",
        octacity_server_store::MAX_LOG_INDEX_WORK_BATCH_SIZE
      )));
    }
    if !self.retention_worker().is_bounded(
      Duration::from_millis(MAX_RETENTION_WORKER_MILLISECONDS),
      octacity_server_store::MAX_RETENTION_WORK_BATCH_SIZE,
      octacity_server_store::MAX_RETENTION_OBJECT_BATCH_SIZE,
      MAX_RETENTION_WORKER_ATTEMPTS,
    ) {
      return Err(ServerConfigError::Invalid(format!(
        "retention worker policy must be positive and bounded by {MAX_RETENTION_WORKER_MILLISECONDS} ms / {} work items / {} objects / {MAX_RETENTION_WORKER_ATTEMPTS} attempts",
        octacity_server_store::MAX_RETENTION_WORK_BATCH_SIZE,
        octacity_server_store::MAX_RETENTION_OBJECT_BATCH_SIZE,
      )));
    }
    if self.orphan_log_cleanup_grace_milliseconds == 0
      || self.orphan_log_cleanup_grace_milliseconds > MAX_ORPHAN_LOG_CLEANUP_GRACE_MILLISECONDS
      || self.orphan_log_cleanup_grace() <= self.object_storage.operation_timeout()
    {
      return Err(ServerConfigError::Invalid(format!(
        "orphan_log_cleanup_grace_milliseconds must exceed the object-storage operation timeout and be at most {MAX_ORPHAN_LOG_CLEANUP_GRACE_MILLISECONDS}"
      )));
    }
    if !self.webhook_worker().is_bounded(
      Duration::from_millis(MAX_WEBHOOK_WORKER_MILLISECONDS),
      octacity_server_store::MAX_WEBHOOK_DELIVERY_BATCH_SIZE,
      octacity_server_store::MAX_MANAGED_WEBHOOK_OPERATION_BATCH_SIZE,
      MAX_WEBHOOK_WORKER_ATTEMPTS,
    ) {
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
    if !self.trigger_evaluation_worker().is_bounded(
      Duration::from_millis(MAX_TRIGGER_EVALUATION_WORKER_MILLISECONDS),
      octacity_server_store::MAX_TRIGGER_EVALUATION_BATCH_SIZE,
      MAX_TRIGGER_EVALUATION_ATTEMPTS,
    ) {
      return Err(ServerConfigError::Invalid(format!(
        "manual Trigger retry policy must be positive and bounded by {MAX_TRIGGER_EVALUATION_WORKER_MILLISECONDS} ms / {} items / {MAX_TRIGGER_EVALUATION_ATTEMPTS} attempts",
        octacity_server_store::MAX_TRIGGER_EVALUATION_BATCH_SIZE,
      )));
    }
    let trigger_evaluation_policy = self.trigger_evaluation_worker().worker();
    let trigger_evaluation_batch_budget = self
      .vcs_operation_timeout_milliseconds
      .checked_add(self.vcs_cancellation_grace_milliseconds)
      .and_then(|operation| operation.checked_mul(u64::from(trigger_evaluation_policy.batch_size())))
      .and_then(|batch| batch.checked_add(TRIGGER_EVALUATION_CLAIM_COMPLETION_MARGIN_MILLISECONDS));
    if trigger_evaluation_batch_budget
      .is_none_or(|budget| budget >= trigger_evaluation_policy.claim_lifetime().as_millis() as u64)
    {
      return Err(ServerConfigError::Invalid(format!(
        "manual Trigger claim lifetime must exceed the sequential VCS budget plus the {TRIGGER_EVALUATION_CLAIM_COMPLETION_MARGIN_MILLISECONDS} ms completion margin"
      )));
    }
    let webhook_policy = self.webhook_worker();
    let webhook_batch_budget = self
      .webhook_operation_timeout_milliseconds
      .checked_add(self.webhook_cancellation_grace_milliseconds)
      .and_then(|operation| {
        operation.checked_mul(u64::from(
          webhook_policy
            .delivery()
            .worker()
            .batch_size()
            .max(webhook_policy.managed_batch_size()),
        ))
      })
      .and_then(|batch| batch.checked_add(WEBHOOK_CLAIM_COMPLETION_MARGIN_MILLISECONDS));
    if webhook_batch_budget
      .is_none_or(|budget| budget >= webhook_policy.delivery().worker().claim_lifetime().as_millis() as u64)
    {
      return Err(ServerConfigError::Invalid(format!(
        "webhook worker claim lifetime must exceed the sequential adapter budget plus the {WEBHOOK_CLAIM_COMPLETION_MARGIN_MILLISECONDS} ms completion margin"
      )));
    }
    let log_index_policy = self.log_index_worker().worker();
    let log_index_batch_budget = self
      .object_storage
      .operation_timeout()
      .as_millis()
      .try_into()
      .ok()
      .and_then(|operation: u64| operation.checked_mul(u64::from(log_index_policy.batch_size())))
      .and_then(|batch| batch.checked_add(LOG_INDEX_CLAIM_COMPLETION_MARGIN_MILLISECONDS));
    if log_index_batch_budget.is_none_or(|budget| budget >= log_index_policy.claim_lifetime().as_millis() as u64) {
      return Err(ServerConfigError::Invalid(format!(
        "log-index claim lifetime must exceed the sequential object-read budget plus the {LOG_INDEX_CLAIM_COMPLETION_MARGIN_MILLISECONDS} ms completion margin"
      )));
    }
    let retention_policy = self.retention_worker();
    let retention_worker = retention_policy.retrying().worker();
    let retention_batch_budget = self
      .object_storage
      .operation_timeout()
      .as_millis()
      .try_into()
      .ok()
      .and_then(|operation: u64| operation.checked_mul(u64::from(retention_policy.object_batch_size())))
      .and_then(|per_claim| per_claim.checked_mul(u64::from(retention_worker.batch_size())))
      .and_then(|batch| batch.checked_add(RETENTION_CLAIM_COMPLETION_MARGIN_MILLISECONDS));
    if retention_batch_budget.is_none_or(|budget| budget >= retention_worker.claim_lifetime().as_millis() as u64) {
      return Err(ServerConfigError::Invalid(format!(
        "retention claim lifetime must exceed the sequential object-delete budget plus the {RETENTION_CLAIM_COMPLETION_MARGIN_MILLISECONDS} ms completion margin"
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

fn listener_addresses_overlap(left: SocketAddr, right: SocketAddr) -> bool {
  if left.port() == 0 || right.port() == 0 || left.port() != right.port() {
    return false;
  }
  left.ip() == right.ip()
    || (left.is_ipv4() == right.is_ipv4() && (left.ip().is_unspecified() || right.ip().is_unspecified()))
}
