use crate::{FactoryDigest, FactoryError, FlowBuildIntent, MAX_FLOW_DATA_BYTES, NodeAttemptId};
use octacity_server_domain::{AttemptId, BuildId, JobId};
use serde::Serialize;

/// Retained ordinary Build/Attempt/Job identities for one frozen node operation.
/// A new ordinary retry appends another link; it cannot replace the original request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FlowBuildExecution {
  operation_id: FactoryDigest,
  node_attempt_id: NodeAttemptId,
  build_id: BuildId,
  attempt_id: AttemptId,
  jobs: Vec<JobId>,
}
impl FlowBuildExecution {
  /// Binds trusted ordinary execution identities to the exact durable intent.
  pub fn new(
    intent: &FlowBuildIntent,
    build_id: BuildId,
    attempt_id: AttemptId,
    mut jobs: Vec<JobId>,
  ) -> Result<Self, FactoryError> {
    jobs.sort_unstable();
    if jobs.is_empty() || jobs.len() > 64 || jobs.windows(2).any(|pair| pair[0] == pair[1]) {
      return Err(invalid());
    }
    Ok(Self {
      operation_id: intent.operation_id()?,
      node_attempt_id: intent.node().id(),
      build_id,
      attempt_id,
      jobs,
    })
  }
  /// Restores exact retained identities using the owner-persisted digest and intent.
  pub fn restore(bytes: &[u8], intent: &FlowBuildIntent, expected: FactoryDigest) -> Result<Self, FactoryError> {
    if bytes.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    let wire: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let field = |name| wire.get(name).cloned().ok_or_else(invalid);
    let restored = Self::new(
      intent,
      serde_json::from_value(field("build_id")?).map_err(|_| invalid())?,
      serde_json::from_value(field("attempt_id")?).map_err(|_| invalid())?,
      serde_json::from_value(field("jobs")?).map_err(|_| invalid())?,
    )?;
    if wire != serde_json::to_value(&restored).map_err(|_| invalid())? || restored.digest()? != expected {
      return Err(invalid());
    }
    Ok(restored)
  }
  /// Complete content identity, including the complete canonical Job set.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    Ok(FactoryDigest::sha256(
      "octacity.factory.flow-build-execution.v1",
      &[&serde_json::to_vec(self).map_err(|_| invalid())?],
    ))
  }
  /// Stable external operation identity.
  #[must_use]
  pub const fn operation_id(&self) -> FactoryDigest {
    self.operation_id
  }
  /// Persisted owning Node Attempt.
  #[must_use]
  pub const fn node_attempt_id(&self) -> NodeAttemptId {
    self.node_attempt_id
  }
  /// Actual ordinary Build.
  #[must_use]
  pub const fn build_id(&self) -> BuildId {
    self.build_id
  }
  /// Actual ordinary Attempt, including retries.
  #[must_use]
  pub const fn attempt_id(&self) -> AttemptId {
    self.attempt_id
  }
  /// Complete sorted immutable Job identities.
  #[must_use]
  pub fn jobs(&self) -> &[JobId] {
    &self.jobs
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Flow Build execution",
  }
}
