//! Bounded exponential retry calculations for idempotent requests.

use std::time::Duration;

use crate::{CoordinatorError, invalid};

const MAX_BACKOFF_SHIFT: u32 = u32::BITS - 1;
const MIN_JITTER_PERCENT: u32 = 75;
const JITTER_PERCENT_BUCKETS: u64 = 51;
const PERCENT_SCALE: f64 = 100.0;

/// Local retry policy applied only to protocol-defined idempotent operations.
#[derive(Clone, Debug)]
pub struct RetryPolicy {
  /// Total request attempts, including the first attempt.
  pub max_attempts: usize,
  /// Base delay before the second attempt.
  pub initial_delay: Duration,
  /// Local upper bound for any retry delay.
  pub max_delay: Duration,
}

impl RetryPolicy {
  /// Validates finite non-zero retry limits.
  pub fn validate(&self) -> Result<(), CoordinatorError> {
    if self.max_attempts == 0 || self.initial_delay.is_zero() || self.max_delay.is_zero() {
      return Err(invalid("retry attempts and delays must be greater than zero"));
    }
    if self.initial_delay > self.max_delay {
      return Err(invalid("retry initial_delay must not exceed max_delay"));
    }
    Ok(())
  }

  pub(crate) fn delay(&self, request_id: &str, failed_attempt: usize, server_max: Duration) -> Duration {
    let exponent = u32::try_from(failed_attempt.saturating_sub(1))
      .unwrap_or(u32::MAX)
      .min(MAX_BACKOFF_SHIFT);
    let base = self.initial_delay.saturating_mul(1_u32 << exponent);
    let ceiling = self.max_delay.min(server_max);
    let base = base.min(ceiling);
    let hash = request_id.bytes().fold(failed_attempt as u64, |state, byte| {
      state.wrapping_mul(1_099_511_628_211).wrapping_add(u64::from(byte))
    });
    // A deterministic 75-125% spread avoids a process-global RNG while still
    // decorrelating agents because every logical call has a UUID request ID.
    let jitter_percent = MIN_JITTER_PERCENT + u32::try_from(hash % JITTER_PERCENT_BUCKETS).unwrap_or(0);
    base.saturating_mul(jitter_percent).div_f64(PERCENT_SCALE).min(ceiling)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn validates_and_caps_exponential_jitter() {
    let policy = RetryPolicy {
      max_attempts: 4,
      initial_delay: Duration::from_millis(100),
      max_delay: Duration::from_secs(10),
    };
    policy.validate().unwrap();
    for attempt in 1..10 {
      assert!(policy.delay("request-a", attempt, Duration::from_millis(250)) <= Duration::from_millis(250));
    }

    let mut invalid = policy;
    invalid.max_attempts = 0;
    assert!(invalid.validate().is_err());
  }
}
