use serde::{Deserialize, Serialize};

use crate::FactoryError;

/// Maximum Stage Attempts allowed by one immutable hard budget.
pub const MAX_FACTORY_ATTEMPTS: u32 = 1_000;
/// Maximum elapsed duration allowed by one immutable hard budget.
pub const MAX_FACTORY_ELAPSED_MILLIS: u64 = 365 * 24 * 60 * 60 * 1_000;
/// Maximum model tokens allowed by one immutable hard budget.
pub const MAX_FACTORY_TOKENS: u64 = 1_000_000_000;
/// Maximum cost allowed by one immutable hard budget, in millionths of the configured currency unit.
pub const MAX_FACTORY_COST_MICRO_UNITS: u64 = 1_000_000_000_000;
/// Maximum output bytes allowed by one immutable hard budget.
pub const MAX_FACTORY_OUTPUT_BYTES: u64 = 1024 * 1024 * 1024 * 1024;

/// Independently enforced category in a Factory hard budget.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BudgetResource {
  /// Stage or call attempts.
  Attempts,
  /// Elapsed wall-clock milliseconds.
  ElapsedTime,
  /// Model tokens.
  Tokens,
  /// Provider cost in millionths of an operator-defined currency unit.
  Cost,
  /// Published output bytes.
  OutputBytes,
}

impl BudgetResource {
  /// Returns the canonical stable representation used by durable action identities.
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Attempts => "attempts",
      Self::ElapsedTime => "elapsed_time",
      Self::Tokens => "tokens",
      Self::Cost => "cost",
      Self::OutputBytes => "output_bytes",
    }
  }
}

/// Immutable hard limits for one bounded Factory scope.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetLimit {
  max_attempts: u32,
  max_elapsed_millis: u64,
  max_tokens: u64,
  max_cost_micro_units: u64,
  max_output_bytes: u64,
}

impl BudgetLimit {
  /// Constructs hard non-zero bounds within product safety ceilings.
  pub fn new(
    max_attempts: u32,
    max_elapsed_millis: u64,
    max_tokens: u64,
    max_cost_micro_units: u64,
    max_output_bytes: u64,
  ) -> Result<Self, FactoryError> {
    validate_limit(
      max_attempts.into(),
      MAX_FACTORY_ATTEMPTS.into(),
      BudgetResource::Attempts,
    )?;
    validate_limit(
      max_elapsed_millis,
      MAX_FACTORY_ELAPSED_MILLIS,
      BudgetResource::ElapsedTime,
    )?;
    validate_limit(max_tokens, MAX_FACTORY_TOKENS, BudgetResource::Tokens)?;
    validate_limit(max_cost_micro_units, MAX_FACTORY_COST_MICRO_UNITS, BudgetResource::Cost)?;
    validate_limit(max_output_bytes, MAX_FACTORY_OUTPUT_BYTES, BudgetResource::OutputBytes)?;
    Ok(Self {
      max_attempts,
      max_elapsed_millis,
      max_tokens,
      max_cost_micro_units,
      max_output_bytes,
    })
  }

  /// Maximum attempts.
  #[must_use]
  pub const fn max_attempts(self) -> u32 {
    self.max_attempts
  }

  /// Maximum elapsed milliseconds.
  #[must_use]
  pub const fn max_elapsed_millis(self) -> u64 {
    self.max_elapsed_millis
  }

  /// Maximum model tokens.
  #[must_use]
  pub const fn max_tokens(self) -> u64 {
    self.max_tokens
  }

  /// Maximum provider cost in millionths of the configured currency unit.
  #[must_use]
  pub const fn max_cost_micro_units(self) -> u64 {
    self.max_cost_micro_units
  }

  /// Maximum output bytes.
  #[must_use]
  pub const fn max_output_bytes(self) -> u64 {
    self.max_output_bytes
  }

  /// Reports whether every limit fits within an enclosing hard budget.
  #[must_use]
  pub const fn fits_within(self, enclosing: Self) -> bool {
    self.max_attempts <= enclosing.max_attempts
      && self.max_elapsed_millis <= enclosing.max_elapsed_millis
      && self.max_tokens <= enclosing.max_tokens
      && self.max_cost_micro_units <= enclosing.max_cost_micro_units
      && self.max_output_bytes <= enclosing.max_output_bytes
  }
}

/// Monotonic usage measured against an immutable [`BudgetLimit`].
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetUsage {
  /// Attempts already consumed.
  pub attempts: u32,
  /// Elapsed milliseconds already consumed.
  pub elapsed_millis: u64,
  /// Model tokens already consumed.
  pub tokens: u64,
  /// Provider cost already consumed in millionths of the configured currency unit.
  pub cost_micro_units: u64,
  /// Output bytes already published.
  pub output_bytes: u64,
}

impl BudgetUsage {
  /// Validates usage against every hard limit.
  pub fn validate(self, limit: BudgetLimit) -> Result<Self, FactoryError> {
    for (exceeded, resource) in [
      (self.attempts > limit.max_attempts, BudgetResource::Attempts),
      (
        self.elapsed_millis > limit.max_elapsed_millis,
        BudgetResource::ElapsedTime,
      ),
      (self.tokens > limit.max_tokens, BudgetResource::Tokens),
      (self.cost_micro_units > limit.max_cost_micro_units, BudgetResource::Cost),
      (self.output_bytes > limit.max_output_bytes, BudgetResource::OutputBytes),
    ] {
      if exceeded {
        return Err(FactoryError::BudgetExceeded { resource });
      }
    }
    Ok(self)
  }

  pub(crate) fn is_exhausted(self, limit: BudgetLimit, resource: BudgetResource) -> bool {
    match resource {
      BudgetResource::Attempts => self.attempts >= limit.max_attempts,
      BudgetResource::ElapsedTime => self.elapsed_millis >= limit.max_elapsed_millis,
      BudgetResource::Tokens => self.tokens >= limit.max_tokens,
      BudgetResource::Cost => self.cost_micro_units >= limit.max_cost_micro_units,
      BudgetResource::OutputBytes => self.output_bytes >= limit.max_output_bytes,
    }
  }
}

fn validate_limit(value: u64, ceiling: u64, resource: BudgetResource) -> Result<(), FactoryError> {
  if value == 0 || value > ceiling {
    return Err(FactoryError::InvalidBudget { resource });
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn limit() -> BudgetLimit {
    BudgetLimit::new(2, 1_000, 10, 20, 30).expect("fixture budget is valid")
  }

  #[test]
  fn rejects_zero_and_excessive_hard_limits() {
    assert_eq!(
      BudgetLimit::new(0, 1, 1, 1, 1),
      Err(FactoryError::InvalidBudget {
        resource: BudgetResource::Attempts,
      })
    );
    assert_eq!(
      BudgetLimit::new(1, 1, MAX_FACTORY_TOKENS + 1, 1, 1),
      Err(FactoryError::InvalidBudget {
        resource: BudgetResource::Tokens,
      })
    );
  }

  #[test]
  fn rejects_usage_above_any_hard_limit() {
    assert_eq!(
      BudgetUsage {
        attempts: 3,
        ..BudgetUsage::default()
      }
      .validate(limit()),
      Err(FactoryError::BudgetExceeded {
        resource: BudgetResource::Attempts,
      })
    );
  }

  #[test]
  fn nested_budgets_cannot_exceed_their_enclosing_limit() {
    let enclosing = BudgetLimit::new(2, 2_000, 20, 40, 60).expect("fixture budget is valid");
    assert!(limit().fits_within(enclosing));
    assert!(!enclosing.fits_within(limit()));
  }
}
