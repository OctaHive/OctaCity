use std::net::SocketAddr;

pub(super) fn default_management_bind() -> SocketAddr {
  SocketAddr::from(([127, 0, 0, 1], 8080))
}

pub(super) const fn default_shutdown_grace_milliseconds() -> u64 {
  10_000
}

pub(super) const fn default_readiness_check_interval_milliseconds() -> u64 {
  5_000
}

pub(super) const fn default_readiness_check_timeout_milliseconds() -> u64 {
  2_000
}

pub(super) const fn default_agent_registration_lifetime_milliseconds() -> u64 {
  24 * 60 * 60 * 1000
}

pub(super) const fn default_agent_enrollment_lifetime_milliseconds() -> u64 {
  15 * 60 * 1000
}

pub(super) const fn default_agent_max_retry_delay_milliseconds() -> u64 {
  30_000
}

pub(super) const fn default_agent_lease_lifetime_milliseconds() -> u64 {
  5 * 60 * 1000
}

pub(super) const fn default_lease_expiry_poll_interval_milliseconds() -> u64 {
  1_000
}

pub(super) const fn default_lease_expiry_claim_lifetime_milliseconds() -> u64 {
  30_000
}

pub(super) const fn default_lease_expiry_batch_size() -> u16 {
  32
}

pub(super) const fn default_schedule_poll_interval_milliseconds() -> u64 {
  1_000
}

pub(super) const fn default_schedule_claim_lifetime_milliseconds() -> u64 {
  30_000
}

pub(super) const fn default_schedule_batch_size() -> u16 {
  32
}

pub(super) const fn default_internal_trigger_poll_interval_milliseconds() -> u64 {
  1_000
}

pub(super) const fn default_internal_trigger_claim_lifetime_milliseconds() -> u64 {
  30_000
}

pub(super) const fn default_internal_trigger_batch_size() -> u16 {
  32
}

pub(super) const fn default_log_index_poll_interval_milliseconds() -> u64 {
  1_000
}

pub(super) const fn default_log_index_claim_lifetime_milliseconds() -> u64 {
  120_000
}

pub(super) const fn default_log_index_batch_size() -> u16 {
  16
}

pub(super) const fn default_log_index_max_attempts() -> u16 {
  10
}

pub(super) const fn default_log_index_initial_retry_milliseconds() -> u64 {
  1_000
}

pub(super) const fn default_log_index_maximum_retry_milliseconds() -> u64 {
  60_000
}

pub(super) const fn default_retention_poll_interval_milliseconds() -> u64 {
  5_000
}

pub(super) const fn default_retention_claim_lifetime_milliseconds() -> u64 {
  15 * 60 * 1_000
}

pub(super) const fn default_retention_work_batch_size() -> u16 {
  8
}

pub(super) const fn default_retention_object_batch_size() -> u16 {
  16
}

pub(super) const fn default_retention_max_attempts() -> u16 {
  20
}

pub(super) const fn default_retention_initial_retry_milliseconds() -> u64 {
  1_000
}

pub(super) const fn default_retention_maximum_retry_milliseconds() -> u64 {
  5 * 60 * 1_000
}

pub(super) const fn default_orphan_log_cleanup_grace_milliseconds() -> u64 {
  15 * 60 * 1_000
}

pub(super) const fn default_webhook_worker_poll_interval_milliseconds() -> u64 {
  1_000
}

pub(super) const fn default_webhook_worker_claim_lifetime_milliseconds() -> u64 {
  30_000
}

pub(super) const fn default_webhook_delivery_batch_size() -> u16 {
  4
}

pub(super) const fn default_managed_webhook_batch_size() -> u16 {
  4
}

pub(super) const fn default_webhook_worker_max_attempts() -> u16 {
  5
}

pub(super) const fn default_webhook_worker_initial_retry_milliseconds() -> u64 {
  1_000
}

pub(super) const fn default_webhook_worker_maximum_retry_milliseconds() -> u64 {
  60_000
}

pub(super) const fn default_ready_job_listener_reconnect_milliseconds() -> u64 {
  1_000
}

pub(super) const fn default_webhook_operation_timeout_milliseconds() -> u64 {
  5_000
}

pub(super) const fn default_webhook_cancellation_grace_milliseconds() -> u64 {
  1_000
}

pub(super) const fn default_vcs_operation_timeout_milliseconds() -> u64 {
  30_000
}

pub(super) const fn default_vcs_cancellation_grace_milliseconds() -> u64 {
  1_000
}

pub(super) const fn default_trigger_evaluation_poll_interval_milliseconds() -> u64 {
  1_000
}

pub(super) const fn default_trigger_evaluation_claim_lifetime_milliseconds() -> u64 {
  180_000
}

pub(super) const fn default_trigger_evaluation_batch_size() -> u16 {
  4
}

pub(super) const fn default_vcs_retry_max_attempts() -> u16 {
  5
}

pub(super) const fn default_vcs_retry_initial_milliseconds() -> u64 {
  1_000
}

pub(super) const fn default_vcs_retry_maximum_milliseconds() -> u64 {
  60_000
}
