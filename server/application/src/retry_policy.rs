use octacity_server_domain::Timestamp;
use thiserror::Error;

/// Validated exponential retry policy shared by durable external work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DurableRetryPolicy {
  max_attempts: u16,
  initial_delay_milliseconds: u64,
  maximum_delay_milliseconds: u64,
}

impl DurableRetryPolicy {
  /// Creates a positive bounded policy with an explicit delay ceiling.
  pub fn new(
    max_attempts: u16,
    initial_delay_milliseconds: u64,
    maximum_delay_milliseconds: u64,
  ) -> Result<Self, DurableRetryPolicyError> {
    if max_attempts == 0
      || initial_delay_milliseconds == 0
      || maximum_delay_milliseconds < initial_delay_milliseconds
      || maximum_delay_milliseconds > i64::MAX as u64
    {
      return Err(DurableRetryPolicyError);
    }
    Ok(Self {
      max_attempts,
      initial_delay_milliseconds,
      maximum_delay_milliseconds,
    })
  }

  pub(crate) fn retry_at(self, attempt: u16, failed_at: Timestamp) -> Option<Timestamp> {
    self.retry_at_with_hint(attempt, failed_at, None)
  }

  pub(crate) fn retry_at_with_hint(
    self,
    attempt: u16,
    failed_at: Timestamp,
    suggested_delay_milliseconds: Option<u64>,
  ) -> Option<Timestamp> {
    if attempt >= self.max_attempts {
      return None;
    }
    let shift = u32::from(attempt.saturating_sub(1)).min(63);
    let multiplier = 1_u64.checked_shl(shift).unwrap_or(u64::MAX);
    let exponential_delay = self
      .initial_delay_milliseconds
      .saturating_mul(multiplier)
      .min(self.maximum_delay_milliseconds);
    let delay = suggested_delay_milliseconds
      .unwrap_or_default()
      .max(exponential_delay)
      .min(self.maximum_delay_milliseconds);
    let next = failed_at.unix_millis().checked_add(i64::try_from(delay).ok()?)?;
    Timestamp::from_unix_millis(next).ok()
  }
}

/// Invalid durable retry policy.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("durable retry policy is invalid")]
pub struct DurableRetryPolicyError;

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn provider_hint_is_clamped_to_the_configured_retry_window() {
    let policy = DurableRetryPolicy::new(3, 10, 100).unwrap();
    let failed_at = Timestamp::from_unix_millis(1_000).unwrap();

    assert_eq!(
      policy.retry_at_with_hint(1, failed_at, Some(1)).unwrap().unix_millis(),
      1_010
    );
    assert_eq!(
      policy
        .retry_at_with_hint(1, failed_at, Some(1_000))
        .unwrap()
        .unix_millis(),
      1_100
    );
    assert!(policy.retry_at_with_hint(3, failed_at, Some(20)).is_none());
  }

  #[test]
  fn provider_hint_cannot_shorten_the_local_exponential_backoff() {
    let policy = DurableRetryPolicy::new(5, 10, 100).unwrap();
    let failed_at = Timestamp::from_unix_millis(1_000).unwrap();

    assert_eq!(
      policy.retry_at_with_hint(4, failed_at, Some(10)).unwrap().unix_millis(),
      1_080
    );
  }
}
