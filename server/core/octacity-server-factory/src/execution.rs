use crate::{
  BudgetLimit, DecisionSignalPurpose, DecisionSignalRequestId, ExactSubject, FactoryClaim, FactoryDigest, FactoryError,
  FactoryKey, FactoryRunId,
};
use serde::{Deserialize, Serialize};

/// Fenced worker ownership attached to immutable Factory execution records.
///
/// Capabilities address common nodes; the parallel stage/call graph is retired.
/// ```compile_fail
/// use octacity_server_factory::{StageAttempt, MacroCall, StageHandoff};
/// ```
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FactoryClaimOwnership {
  owner: FactoryKey,
  claim: FactoryClaim,
}

impl FactoryClaimOwnership {
  /// Binds one bounded worker identity to its exclusive claim window.
  #[must_use]
  pub const fn new(owner: FactoryKey, claim: FactoryClaim) -> Self {
    Self { owner, claim }
  }

  /// Returns the bounded worker identity.
  #[must_use]
  pub const fn owner(&self) -> &FactoryKey {
    &self.owner
  }

  /// Returns the exclusive fenced claim window.
  #[must_use]
  pub const fn claim(&self) -> FactoryClaim {
    self.claim
  }
}

/// Canonical input and deterministic policy digests for one Decision Signal request.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DecisionSignalDigests {
  input: FactoryDigest,
  policy: FactoryDigest,
}

impl DecisionSignalDigests {
  /// Constructs the exact immutable request digests.
  #[must_use]
  pub const fn new(input: FactoryDigest, policy: FactoryDigest) -> Self {
    Self { input, policy }
  }

  /// Returns the canonical redacted input digest.
  #[must_use]
  pub const fn input(self) -> FactoryDigest {
    self.input
  }

  /// Returns the deterministic policy digest.
  #[must_use]
  pub const fn policy(self) -> FactoryDigest {
    self.policy
  }
}

/// Provider-neutral request for one bounded, non-authoritative Decision Signal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionSignalRequest {
  id: DecisionSignalRequestId,
  node_attempt_id: crate::NodeAttemptId,
  frozen_input_digest: FactoryDigest,
  run_id: FactoryRunId,
  subject: ExactSubject,
  purpose: DecisionSignalPurpose,
  input_digest: FactoryDigest,
  policy_digest: FactoryDigest,
  budget: BudgetLimit,
}

impl DecisionSignalRequest {
  /// Binds redacted signal data to the exact common node and its frozen input.
  pub fn new(
    id: DecisionSignalRequestId,
    input: &crate::FlowNodeInput,
    node: &crate::NodeAttempt,
    purpose: DecisionSignalPurpose,
    digests: DecisionSignalDigests,
    budget: BudgetLimit,
  ) -> Result<Self, FactoryError> {
    input.verify_attempt(node)?;
    Ok(Self {
      id,
      node_attempt_id: node.id(),
      frozen_input_digest: input.digest()?,
      run_id: input.factory_run_id(),
      subject: input.subject().clone(),
      purpose,
      input_digest: digests.input,
      policy_digest: digests.policy,
      budget,
    })
  }

  /// Returns the stable logical request identity.
  #[must_use]
  pub const fn id(&self) -> DecisionSignalRequestId {
    self.id
  }

  /// Returns the common execution owner.
  #[must_use]
  pub const fn node_attempt_id(&self) -> crate::NodeAttemptId {
    self.node_attempt_id
  }

  /// Returns the complete owner input digest, distinct from the redacted signal data.
  #[must_use]
  pub const fn frozen_input_digest(&self) -> FactoryDigest {
    self.frozen_input_digest
  }

  /// Returns the owning Run.
  #[must_use]
  pub const fn run_id(&self) -> FactoryRunId {
    self.run_id
  }

  /// Returns the exact subject.
  #[must_use]
  pub const fn subject(&self) -> &ExactSubject {
    &self.subject
  }

  /// Returns the only seam in which this signal may be consumed.
  #[must_use]
  pub const fn purpose(&self) -> DecisionSignalPurpose {
    self.purpose
  }

  /// Returns the canonical redacted input digest.
  #[must_use]
  pub const fn input_digest(&self) -> FactoryDigest {
    self.input_digest
  }

  /// Returns the immutable deterministic policy digest.
  #[must_use]
  pub const fn policy_digest(&self) -> FactoryDigest {
    self.policy_digest
  }

  /// Returns the immutable call budget.
  #[must_use]
  pub const fn budget(&self) -> BudgetLimit {
    self.budget
  }
}
