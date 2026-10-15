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
  #[serde(default)]
  retry: Option<FactoryRetryPolicy>,
}

/// Bounded retry and circuit cooldown for one independently supervised Factory worker.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FactoryRetryPolicy {
  max_backoff_milliseconds: u64,
  consecutive_failure_limit: u16,
  circuit_open_milliseconds: u64,
}
impl FactoryRetryPolicy {
  /// Maximum exponential delay before the circuit opens.
  pub const fn max_backoff(self) -> Duration {
    Duration::from_millis(self.max_backoff_milliseconds)
  }
  /// Failed passes required before a cooldown and one recovery probe.
  pub const fn consecutive_failure_limit(self) -> u16 {
    self.consecutive_failure_limit
  }
  /// Time during which the worker makes no new dependency calls.
  pub const fn circuit_open(self) -> Duration {
    Duration::from_millis(self.circuit_open_milliseconds)
  }
}

impl FactoryRuntimeConfig {
  /// Retry settings; older enabled configurations default to a bounded claim-window cooldown.
  pub fn retry_policy(self) -> FactoryRetryPolicy {
    self.retry.unwrap_or_else(|| {
      let window = self.claim_lifetime_milliseconds.max(self.poll_interval_milliseconds);
      FactoryRetryPolicy {
        max_backoff_milliseconds: window,
        consecutive_failure_limit: 3,
        circuit_open_milliseconds: window,
      }
    })
  }
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
    let retry = self.retry_policy();
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
      || retry.max_backoff() < self.poll_interval()
      || retry.max_backoff() > MAX_FACTORY_WORKER_WINDOW
      || retry.consecutive_failure_limit() == 0
      || retry.consecutive_failure_limit() > 100
      || retry.circuit_open() < self.poll_interval()
      || retry.circuit_open() > MAX_FACTORY_WORKER_WINDOW
    {
      return Err(super::ServerConfigError::Invalid(
        "Factory worker bounds are invalid".to_owned(),
      ));
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn factory_retry_policy_is_optional_and_rejects_unbounded_or_tight_loop_settings() {
    let base = "poll_interval_milliseconds=10\nclaim_lifetime_milliseconds=1000\nbatch_size=4\nconcurrency=2";
    let default: FactoryRuntimeConfig = toml::from_str(base).unwrap();
    default.validate().unwrap();
    assert!(default.retry_policy().max_backoff() >= default.poll_interval());
    let source = format!(
      "{base}\n[retry]\nmax_backoff_milliseconds=40\nconsecutive_failure_limit=3\ncircuit_open_milliseconds=200"
    );
    let configured: FactoryRuntimeConfig = toml::from_str(&source).unwrap();
    configured.validate().unwrap();
    assert_eq!(configured.retry_policy().max_backoff(), Duration::from_millis(40));
    assert_eq!(configured.retry_policy().consecutive_failure_limit(), 3);
    assert_eq!(configured.retry_policy().circuit_open(), Duration::from_millis(200));
    for invalid in [
      source.replace("max_backoff_milliseconds=40", "max_backoff_milliseconds=0"),
      source.replace("max_backoff_milliseconds=40", "max_backoff_milliseconds=9"),
      source.replace("max_backoff_milliseconds=40", "max_backoff_milliseconds=300001"),
      source.replace("consecutive_failure_limit=3", "consecutive_failure_limit=0"),
      source.replace("consecutive_failure_limit=3", "consecutive_failure_limit=101"),
      source.replace("circuit_open_milliseconds=200", "circuit_open_milliseconds=9"),
      source.replace("circuit_open_milliseconds=200", "circuit_open_milliseconds=300001"),
    ] {
      assert!(
        toml::from_str::<FactoryRuntimeConfig>(&invalid)
          .unwrap()
          .validate()
          .is_err()
      );
    }
  }
}
