//! Operator-selected Codex cost accounting; no built-in model prices.
use crate::ApplicationError;
use octacity_server_factory::ImmutableReference;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Exact cost basis frozen in the protected invocation documents. Rates use
/// the same currency micro-units as the admission budget and are supplied by
/// the deployment owner; model names never select a price table.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum FactoryCodexUsagePolicy {
  /// Require a cost counter cross-checked in the result and provenance.
  ReportedCost,
  /// Price independently reported token counters for one exact model.
  TokenRates {
    /// Full immutable model identity, including version and digest.
    model: ImmutableReference,
    /// Cost of one million uncached input tokens.
    input_micro_units_per_million: u64,
    /// Cost of one million cached input tokens.
    cached_input_micro_units_per_million: u64,
    /// Cost of one million output tokens.
    output_micro_units_per_million: u64,
  },
}
impl FactoryCodexUsagePolicy {
  pub(crate) fn matches_model(&self, model: &ImmutableReference) -> bool {
    match self {
      Self::ReportedCost => true,
      Self::TokenRates { model: priced, .. } => priced == model,
    }
  }
  pub(crate) fn cost(
    &self,
    executed_model: &ImmutableReference,
    counters: &BTreeMap<String, u64>,
  ) -> Result<u64, ApplicationError> {
    let invalid = ApplicationError::invalid;
    match self {
      Self::ReportedCost => counters
        .get("cost_micro_units")
        .copied()
        .ok_or_else(ApplicationError::unavailable),
      Self::TokenRates {
        model,
        input_micro_units_per_million: input_rate,
        cached_input_micro_units_per_million: cached_rate,
        output_micro_units_per_million: output_rate,
      } => {
        if model != executed_model {
          return Err(invalid());
        }
        let input = counters.get("input_tokens").copied().ok_or_else(invalid)?;
        let output = counters.get("output_tokens").copied().ok_or_else(invalid)?;
        let cached = match counters.get("cached_input_tokens") {
          Some(value) => *value,
          None if input_rate == cached_rate => 0,
          None => return Err(ApplicationError::unavailable()),
        };
        let uncached = input.checked_sub(cached).ok_or_else(invalid)?;
        let total = [(uncached, *input_rate), (cached, *cached_rate), (output, *output_rate)]
          .into_iter()
          .try_fold(0_u128, |sum, (tokens, rate)| {
            sum.checked_add(u128::from(tokens) * u128::from(rate))
          })
          .ok_or_else(invalid)?;
        let cost = u64::try_from(total.div_ceil(1_000_000)).map_err(|_| invalid())?;
        if counters
          .get("cost_micro_units")
          .is_some_and(|reported| *reported != cost)
        {
          return Err(invalid());
        }
        Ok(cost)
      }
    }
  }
}
