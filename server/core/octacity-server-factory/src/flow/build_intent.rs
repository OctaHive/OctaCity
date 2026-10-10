use crate::{
  AdmittedFlow, BudgetLimit, BudgetUsage, FactoryDigest, FactoryError, FactoryKey, FactoryPermissionSet,
  FlowBuildProfile, FlowNodeInput, MAX_FLOW_DATA_BYTES, NodeAttempt, NodeExecutionIdentity,
};
use serde::Serialize;

/// Frozen ordinary Build intent for any configured command or reasoning node.
///
/// The Flow owner persists this intent before dispatch and supplies cumulative
/// usage from authoritative history. Construction reserves one external execution
/// under the owning Flow budget; it never grants readiness or current ownership.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FlowBuildIntent {
  input: FlowNodeInput,
  node: NodeAttempt,
  usage_before: BudgetUsage,
  priority: i64,
  profile: FlowBuildProfile,
  result_outcome: FactoryKey,
  permissions: FactoryPermissionSet,
}
impl FlowBuildIntent {
  /// Freezes the containing node's exact profile, bounded reservation and Build priority.
  pub fn new(
    input: FlowNodeInput,
    admitted: &AdmittedFlow,
    node: NodeAttempt,
    usage_before: BudgetUsage,
    priority: i64,
  ) -> Result<Self, FactoryError> {
    input.verify_admission(admitted)?;
    let definition = admitted.closure().definition(input.definition()).ok_or_else(invalid)?;
    let declared = definition.node(input.node()).ok_or_else(invalid)?;
    let binding = declared.build().ok_or_else(invalid)?;
    if node.factory_run_id() != input.factory_run_id()
      || node.flow_run_id() != input.flow_run_id()
      || node.workflow_cycle_id() != input.cycle_id()
      || node.node_key() != input.node()
      || node.node_kind() != declared.kind()
      || node.input_digest() != input.digest()?
      || node.budget() != declared.budget()
      || node.execution() != &NodeExecutionIdentity::External(binding.tool.clone())
      || node.stage_projection_id().is_some()
      || node.deadline() <= node.claim().claimed_at()
      || node.deadline() > node.claim().expires_at()
      || input.context().is_none()
      || binding.build_configuration.project_id() != input.subject().project_id()
    {
      return Err(invalid());
    }
    let budget = node.budget();
    usage_before
      .checked_add(BudgetUsage {
        attempts: 1,
        elapsed_millis: budget.max_elapsed_millis(),
        tokens: budget.max_tokens(),
        cost_micro_units: budget.max_cost_micro_units(),
        output_bytes: budget.max_output_bytes(),
      })
      .ok_or_else(invalid)?
      .validate(definition.execution().budget())?;
    let profile = FlowBuildProfile {
      definition: definition.reference(),
      node: declared.key().clone(),
      tool: binding.tool.clone(),
      plugin: binding.plugin.clone(),
      model_or_tool: binding.model_or_tool.clone(),
      task_digest: binding.task_digest,
      build_configuration: binding.build_configuration.clone(),
      result_output: binding.result_output.clone(),
    };
    let intent = Self {
      input,
      node,
      usage_before,
      priority,
      profile,
      result_outcome: binding.result_outcome.clone(),
      permissions: declared.permissions().clone(),
    };
    if intent.bytes()?.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    Ok(intent)
  }
  /// Restores against exact trusted input, attempt, admission and retained operation identity.
  pub fn restore(
    bytes: &[u8],
    input: &FlowNodeInput,
    admitted: &AdmittedFlow,
    node: &NodeAttempt,
    expected_digest: FactoryDigest,
  ) -> Result<Self, FactoryError> {
    if bytes.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    let wire: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let field = |name| wire.get(name).cloned().ok_or_else(invalid);
    let usage = serde_json::from_value(field("usage_before")?).map_err(|_| invalid())?;
    let priority = serde_json::from_value(field("priority")?).map_err(|_| invalid())?;
    let intent = Self::new(input.clone(), admitted, node.clone(), usage, priority)?;
    if wire != serde_json::to_value(&intent).map_err(|_| invalid())? || intent.operation_id()? != expected_digest {
      return Err(invalid());
    }
    Ok(intent)
  }
  /// Stable identity including frozen inputs, selected profile, priority and resource reservation.
  pub fn operation_id(&self) -> Result<FactoryDigest, FactoryError> {
    Ok(FactoryDigest::sha256(
      "octacity.factory.flow-build-intent.v1",
      &[&self.bytes()?],
    ))
  }
  fn bytes(&self) -> Result<Vec<u8>, FactoryError> {
    serde_json::to_vec(self).map_err(|_| invalid())
  }
  /// Returns the exact common node input and selected context.
  #[must_use]
  pub const fn input(&self) -> &FlowNodeInput {
    &self.input
  }
  /// Returns the persisted Node Attempt owning this external operation.
  #[must_use]
  pub const fn node(&self) -> &NodeAttempt {
    &self.node
  }
  /// Returns the immutable containing definition/node and materialized execution selection.
  #[must_use]
  pub const fn profile(&self) -> &FlowBuildProfile {
    &self.profile
  }
  /// Returns the operator-selected observation outcome, independent of provider fields.
  #[must_use]
  pub const fn result_outcome(&self) -> &FactoryKey {
    &self.result_outcome
  }
  /// Returns the narrowed authority already admitted for this node.
  #[must_use]
  pub const fn permissions(&self) -> &FactoryPermissionSet {
    &self.permissions
  }
  /// Returns the immutable per-node hard ceiling.
  #[must_use]
  pub const fn budget(&self) -> BudgetLimit {
    self.node.budget()
  }
  /// Returns cumulative consumption preceding this external execution.
  #[must_use]
  pub const fn usage_before(&self) -> BudgetUsage {
    self.usage_before
  }
  /// Returns the immutable ordinary Build queue priority.
  #[must_use]
  pub const fn priority(&self) -> i64 {
    self.priority
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Flow Build intent",
  }
}
