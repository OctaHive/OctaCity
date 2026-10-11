//! Explicit bounds for the optional configurable Flow owner.
use serde::Deserialize;
use std::time::Duration;

const MAX_FACTORY_WORKER_WINDOW: Duration = Duration::from_secs(5 * 60);

/// Presence of this configuration enables Factory; absence leaves ordinary CI/CD independent.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FactoryRuntimeConfig {
  poll_interval_milliseconds: u64,
  claim_lifetime_milliseconds: u64,
  batch_size: u16,
  concurrency: u16,
}
impl FactoryRuntimeConfig {
  /// Interval between bounded owner passes.
  pub const fn poll_interval(self) -> Duration {
    Duration::from_millis(self.poll_interval_milliseconds)
  }
  /// Exclusive durable ownership window for one pass.
  pub const fn claim_lifetime(self) -> Duration {
    Duration::from_millis(self.claim_lifetime_milliseconds)
  }
  /// Maximum Runs claimed by one pass.
  pub const fn batch_size(self) -> u16 {
    self.batch_size
  }
  /// Maximum concurrently reconciled Runs within a pass.
  pub const fn concurrency(self) -> u16 {
    self.concurrency
  }
  pub(super) fn validate(self) -> Result<(), super::ServerConfigError> {
    if !super::WorkerPolicy::new(
      self.poll_interval_milliseconds,
      self.claim_lifetime_milliseconds,
      self.batch_size,
    )
    .is_bounded(
      MAX_FACTORY_WORKER_WINDOW,
      octacity_server_store::MAX_FACTORY_RECONCILIATION_BATCH_SIZE,
    ) || self.concurrency == 0
      || self.concurrency > self.batch_size
    {
      return Err(super::ServerConfigError::Invalid(
        "Factory worker bounds are invalid".to_owned(),
      ));
    }
    Ok(())
  }
}
