use crate::{
  BudgetUsage, FactoryClaimOwnership, FactoryDigest, FactoryError, FlowBuildExecution, MAX_FLOW_DATA_BYTES,
  NodeAttempt, NodeAttemptId,
};
use octacity_server_domain::Timestamp;
use serde::{Deserialize, Serialize};

/// Technical reason for a stopped operation; it never selects a business outcome.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum FlowNodeStopReason {
  /// Original accepted operator intent, retained through observer replacement.
  Cancelled {
    /// Authoritative acceptance time of the original cancellation intent.
    requested_at: Timestamp,
  },
  /// Independently confirmed execution failure, with diagnostics in the execution journal.
  ExecutionFailed,
}

/// Retained technical stop receipt. It has no semantic outcome or routing authority.
/// The trusted executor confirms that the exact operation has no remaining effects.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowNodeStop {
  node_attempt_id: NodeAttemptId,
  input_digest: FactoryDigest,
  ownership: FactoryClaimOwnership,
  reason: FlowNodeStopReason,
  observed_at: Timestamp,
  usage: BudgetUsage,
  execution: Option<FactoryDigest>,
}
impl FlowNodeStop {
  /// Binds a confirmed stop and its measured consumption to the current fenced observer.
  pub fn new(
    attempt: &NodeAttempt,
    ownership: FactoryClaimOwnership,
    reason: FlowNodeStopReason,
    observed_at: Timestamp,
    usage: BudgetUsage,
    execution: Option<&FlowBuildExecution>,
  ) -> Result<Self, FactoryError> {
    attempt.verify_observer(&ownership, observed_at)?;
    if observed_at < attempt.started_at()
      || matches!(reason, FlowNodeStopReason::Cancelled { requested_at } if requested_at < attempt.started_at() || requested_at > observed_at)
      || usage.attempts != 0
      || execution.is_some_and(|row| row.node_attempt_id() != attempt.id())
    {
      return Err(invalid());
    }
    usage.validate(attempt.budget())?;
    Ok(Self {
      node_attempt_id: attempt.id(),
      input_digest: attempt.input_digest(),
      ownership,
      reason,
      observed_at,
      usage,
      execution: execution.map(FlowBuildExecution::digest).transpose()?,
    })
  }
  /// Restores canonical retained bytes against their exact attempt and execution link.
  pub fn restore(
    bytes: &[u8],
    attempt: &NodeAttempt,
    execution: Option<&FlowBuildExecution>,
    expected: FactoryDigest,
  ) -> Result<Self, FactoryError> {
    if bytes.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let wire: Self = serde_json::from_value(value.clone()).map_err(|_| invalid())?;
    let restored = Self::new(
      attempt,
      wire.ownership.clone(),
      wire.reason,
      wire.observed_at,
      wire.usage,
      execution,
    )?;
    if value != serde_json::to_value(&restored).map_err(|_| invalid())? || restored.digest()? != expected {
      return Err(invalid());
    }
    Ok(restored)
  }
  /// Complete identity including ownership, observation time and measured consumption.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    Ok(FactoryDigest::sha256(
      "octacity.factory.node-stop.v1",
      &[&serde_json::to_vec(self).map_err(|_| invalid())?],
    ))
  }
  /// Exact stopped Node Attempt.
  #[must_use]
  pub const fn node_attempt_id(&self) -> NodeAttemptId {
    self.node_attempt_id
  }
  /// Confirmed technical disposition, independent of graph outcomes.
  #[must_use]
  pub const fn reason(&self) -> FlowNodeStopReason {
    self.reason
  }
  /// Authoritative time at which stopping was independently confirmed.
  #[must_use]
  pub const fn observed_at(&self) -> Timestamp {
    self.observed_at
  }
  /// Measured resources charged once by the owning transaction.
  #[must_use]
  pub const fn usage(&self) -> BudgetUsage {
    self.usage
  }
  /// Exact observed ordinary execution, or an operation with no ordinary Build.
  #[must_use]
  pub const fn execution(&self) -> Option<FactoryDigest> {
    self.execution
  }
  /// Fenced observer which accepted the receipt.
  #[must_use]
  pub const fn ownership(&self) -> &FactoryClaimOwnership {
    &self.ownership
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Node stop receipt",
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::flow_test_support::execution_fixture;
  fn time(value: i64) -> Timestamp {
    Timestamp::from_unix_millis(value).unwrap()
  }
  #[test]
  fn a_failed_operation_releases_execution_capacity_without_selecting_a_business_outcome() {
    let (_, admitted, _, _, attempt) = execution_fixture(crate::FlowNodeKind::Reasoning);
    let receipt = crate::FlowNodeStop::new(
      &attempt,
      FactoryClaimOwnership::new(attempt.owner().clone(), attempt.claim()),
      crate::FlowNodeStopReason::ExecutionFailed,
      time(15),
      BudgetUsage {
        tokens: 23,
        elapsed_millis: 2,
        ..Default::default()
      },
      None,
    )
    .unwrap();
    let encoded = serde_json::to_vec(&receipt).unwrap();
    assert_eq!(
      crate::FlowNodeStop::restore(&encoded, &attempt, None, receipt.digest().unwrap()).unwrap(),
      receipt
    );
    assert_eq!(receipt.usage().tokens, 23);
    let history = crate::FlowRuntimeHistory {
      stops: std::slice::from_ref(&receipt),
      flow_runs: std::slice::from_ref(admitted.root_run()),
      cycles: std::slice::from_ref(admitted.initial_cycle()),
      attempts: std::slice::from_ref(&attempt),
      completions: &[],
    };
    crate::validate_flow_runtime_history(&admitted, history).unwrap();
    assert_eq!(history.active_node_count(), 0);
    let validated = admitted.validated().unwrap();
    assert_eq!(
      crate::FlowInterpreter::new(&validated)
        .progress(admitted.root_run(), admitted.initial_cycle(), history)
        .unwrap(),
      crate::FlowProgress::Node(attempt.node_key().clone())
    );
    let mut forged = serde_json::to_value(&receipt).unwrap();
    forged["outcome"] = serde_json::json!("success");
    assert!(
      crate::FlowNodeStop::restore(
        &serde_json::to_vec(&forged).unwrap(),
        &attempt,
        None,
        receipt.digest().unwrap()
      )
      .is_err()
    );
  }
  #[test]
  fn cancellation_restoration_rejects_extra_authority_and_preserves_its_exact_owner() {
    let (_, _, _, _, attempt) = execution_fixture(crate::FlowNodeKind::Reasoning);
    let receipt = FlowNodeStop::new(
      &attempt,
      FactoryClaimOwnership::new(attempt.owner().clone(), attempt.claim()),
      FlowNodeStopReason::Cancelled { requested_at: time(11) },
      time(15),
      BudgetUsage::default(),
      None,
    )
    .unwrap();
    let encoded = serde_json::to_vec(&receipt).unwrap();
    assert_eq!(
      FlowNodeStop::restore(&encoded, &attempt, None, receipt.digest().unwrap()).unwrap(),
      receipt
    );
    let mut wire = serde_json::to_value(&receipt).unwrap();
    wire["ownership"]["hidden_authority"] = serde_json::json!(true);
    assert!(
      FlowNodeStop::restore(
        &serde_json::to_vec(&wire).unwrap(),
        &attempt,
        None,
        receipt.digest().unwrap()
      )
      .is_err(),
      "restoration must check all retained bytes, including nested authority"
    );
  }
}
