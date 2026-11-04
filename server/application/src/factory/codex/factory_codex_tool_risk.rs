//! Provider-neutral immutable tool-risk allocation for the Codex compiler.
use crate::{ApplicationError, FactoryToolRiskBinding, factory_tool_risk_task_digest};
use octacity_server_factory::{BudgetLimit, FactoryDigest};
use serde::{Deserialize, Serialize};

/// Frozen references and resource allocation without server provider credentials
/// or API configuration. Installation must match these facts to the exact binding.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryCodexToolRiskPolicy {
  policy_digest: FactoryDigest,
  signal_allowance: BudgetLimit,
}
impl FactoryCodexToolRiskPolicy {
  /// Derives the pin and finite allocation from the complete installed policy.
  pub fn from_binding(binding: &FactoryToolRiskBinding) -> Result<Self, ApplicationError> {
    Ok(Self {
      policy_digest: binding.policy_digest()?,
      signal_allowance: binding.signal_allowance()?,
    })
  }
  /// Exact policy selected by the immutable Build task definition.
  pub const fn policy_digest(self) -> FactoryDigest {
    self.policy_digest
  }
  /// Provider calls cannot consume the ordinary worker's separate resource share.
  pub const fn signal_allowance(self) -> BudgetLimit {
    self.signal_allowance
  }
  /// Binds the opaque compiler contract to this complete provider policy.
  pub fn task_digest(self, base: FactoryDigest) -> FactoryDigest {
    factory_tool_risk_task_digest(base, self.policy_digest)
  }
  /// Reserves independent resources before dispatch. Blocking provider latency
  /// is already included in the workload's original wall window, which remains
  /// unchanged; tokens, charges, output and call slots cannot be shared twice.
  pub fn model_budget(self, node: BudgetLimit) -> Result<BudgetLimit, ApplicationError> {
    let signal = self.signal_allowance;
    if !signal.fits_within(node) {
      return Err(ApplicationError::invalid());
    }
    BudgetLimit::new(
      node
        .max_attempts()
        .checked_sub(signal.max_attempts())
        .ok_or_else(ApplicationError::invalid)?,
      node.max_elapsed_millis(),
      node
        .max_tokens()
        .checked_sub(signal.max_tokens())
        .ok_or_else(ApplicationError::invalid)?,
      node
        .max_cost_micro_units()
        .checked_sub(signal.max_cost_micro_units())
        .ok_or_else(ApplicationError::invalid)?,
      node
        .max_output_bytes()
        .checked_sub(signal.max_output_bytes())
        .ok_or_else(ApplicationError::invalid)?,
    )
    .map_err(|_| ApplicationError::invalid())
  }
}
