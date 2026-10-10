use serde::{Deserialize, Serialize};

use crate::{BudgetLimit, BudgetUsage, FactoryDigest, FactoryError, FactoryKey};

/// Maximum items inspected or selected by a single phase-pool pass.
pub const MAX_PHASE_POOL_BATCH: u16 = 100;

/// Maximum concurrent selections retained by one phase pool.
pub const MAX_PHASE_POOL_WIP: u16 = 1_000;

/// Operator-selected priority dimensions, followed by exact identity tie breakers.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PhasePoolOrder {
  /// Highest accepted severity first.
  Severity,
  /// Highest immutable Project priority first.
  ProjectPriority,
  /// Oldest admitted Work first.
  Age,
}

/// Immutable deterministic selection policy for one durable phase-ready pool.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PhasePoolPolicy {
  /// Stable logical phase, independent of lifecycle states and Agent Pools.
  pub phase: FactoryKey,
  /// A permutation of the three supported priority dimensions.
  pub order: Vec<PhasePoolOrder>,
  /// Concurrent selected Work ceiling across this pool.
  pub max_wip: u16,
  /// Concurrent selected Work ceiling for one Project in this pool.
  pub max_project_wip: u16,
  /// Aggregate reservation ceiling across selected Work in this pool.
  pub budget: BudgetLimit,
}

impl PhasePoolPolicy {
  /// Revalidates even values restored from serialized history.
  pub fn validate(&self) -> Result<(), FactoryError> {
    if self.order.len() != 3
      || [
        PhasePoolOrder::Severity,
        PhasePoolOrder::ProjectPriority,
        PhasePoolOrder::Age,
      ]
      .iter()
      .any(|dimension| self.order.iter().filter(|item| *item == dimension).count() != 1)
      || self.max_wip == 0
      || self.max_wip > MAX_PHASE_POOL_WIP
      || self.max_project_wip == 0
      || self.max_project_wip > self.max_wip
    {
      return Err(FactoryError::InvalidReference {
        relationship: "phase pool policy",
      });
    }
    BudgetLimit::new(
      self.budget.max_attempts(),
      self.budget.max_elapsed_millis(),
      self.budget.max_tokens(),
      self.budget.max_cost_micro_units(),
      self.budget.max_output_bytes(),
    )?;
    Ok(())
  }

  /// Exact immutable policy identity retained with every selection.
  #[must_use]
  pub fn digest(&self) -> FactoryDigest {
    FactoryDigest::sha256(
      "octacity.factory.phase-pool-policy.v1",
      &[&serde_json::to_vec(self).expect("typed policy serializes")],
    )
  }
}

/// Adds resource reservations without permitting overflow or widening a hard budget.
#[must_use]
pub fn reserve_phase_budget(used: BudgetUsage, reserve: BudgetUsage, limit: BudgetLimit) -> Option<BudgetUsage> {
  used.checked_add(reserve)?.validate(limit).ok()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn pool_policy_rejects_ambiguous_order_and_budget_reservations_fail_closed() {
    let budget = BudgetLimit::new(2, 10, 10, 10, 10).unwrap();
    let mut policy = PhasePoolPolicy {
      phase: FactoryKey::new("research").unwrap(),
      order: vec![
        PhasePoolOrder::Severity,
        PhasePoolOrder::ProjectPriority,
        PhasePoolOrder::Age,
      ],
      max_wip: 2,
      max_project_wip: 1,
      budget,
    };
    assert!(policy.validate().is_ok());
    policy.order[2] = PhasePoolOrder::Severity;
    assert!(policy.validate().is_err());
    let reserve = BudgetUsage {
      attempts: 1,
      tokens: 5,
      ..BudgetUsage::default()
    };
    assert!(reserve_phase_budget(reserve, reserve, budget).is_some());
    assert!(reserve_phase_budget(BudgetUsage { tokens: 6, ..reserve }, reserve, budget).is_none());
    assert!(
      reserve_phase_budget(
        BudgetUsage {
          tokens: u64::MAX,
          ..reserve
        },
        reserve,
        budget
      )
      .is_none()
    );
  }
}
