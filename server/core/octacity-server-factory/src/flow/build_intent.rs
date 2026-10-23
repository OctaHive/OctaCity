use crate::{
  AdmittedFlow, BudgetLimit, BudgetUsage, FactoryDigest, FactoryError, FactoryKey, FactoryPermissionSet,
  FlowBuildProfile, FlowNodeInput, MAX_FLOW_DATA_BYTES, NodeAttempt, NodeExecutionIdentity,
};
use serde::{Deserialize, Serialize};

/// Immutable link to one selected generation; execution settings stay in the pinned candidate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct SelectedProfile {
  digest: FactoryDigest,
  profile: crate::FactoryRoleProfileRef,
  budget: BudgetLimit,
}

/// Frozen ordinary Build intent for any configured command or reasoning node.
///
/// The Flow owner persists this intent before dispatch and supplies cumulative
/// usage from authoritative history. Construction reserves one external execution
/// under the admitted aggregate budget. The owner and store separately check the
/// owning Flow reservation against authoritative history before publication.
/// Neither check grants readiness or current ownership.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FlowBuildIntent {
  input: FlowNodeInput,
  #[serde(skip_serializing_if = "Option::is_none")]
  selection: Option<SelectedProfile>,
  node: NodeAttempt,
  usage_before: BudgetUsage,
  priority: i64,
  profile: FlowBuildProfile,
  #[serde(skip_serializing_if = "Option::is_none")]
  action: Option<crate::FlowActionBinding>,
  result_outcome: FactoryKey,
  permissions: FactoryPermissionSet,
  #[serde(skip_serializing_if = "Option::is_none")]
  verification_check: Option<crate::VerificationCheckInput>,
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
    Self::with_selection(input, admitted, node, usage_before, priority, None)
  }
  /// Freezes the exact retained selection generation before dispatch commitment.
  /// The complete journal verifies that this is the current, authorized generation.
  pub fn selected(
    input: FlowNodeInput,
    admitted: &AdmittedFlow,
    node: NodeAttempt,
    usage_before: BudgetUsage,
    priority: i64,
    selection: &crate::FlowProfileSelectionRecord,
  ) -> Result<Self, FactoryError> {
    if selection.input_digest() != input.digest()?
      || node.started_at() < selection.recorded_at()
      || node.deadline() != selection.deadline()
    {
      return Err(invalid());
    }
    let candidate = admitted
      .closure()
      .definition(input.definition())
      .and_then(|definition| definition.node(input.node()))
      .and_then(|node| node.profile_selection())
      .and_then(|binding| binding.candidates.get(selection.candidate()?))
      .ok_or_else(invalid)?;
    let selected = SelectedProfile {
      digest: selection.digest()?,
      profile: candidate.profile.reference().clone(),
      budget: candidate.profile.definition().budget,
    };
    Self::with_selection(input, admitted, node, usage_before, priority, Some(selected))
  }

  /// Creates an explicitly authorized technical retry with the preceding execution binding.
  /// The journal separately verifies the failed predecessor and accepted retry control.
  pub fn retry(
    &self,
    input: FlowNodeInput,
    admitted: &AdmittedFlow,
    node: NodeAttempt,
    usage_before: BudgetUsage,
    priority: i64,
  ) -> Result<Self, FactoryError> {
    self.verify_admission(admitted)?;
    if node.retry_of() != Some(self.node.id()) || input != self.input.retry()? {
      return Err(invalid());
    }
    Self::with_selection(input, admitted, node, usage_before, priority, self.selection.clone())
  }

  fn with_selection(
    input: FlowNodeInput,
    admitted: &AdmittedFlow,
    node: NodeAttempt,
    usage_before: BudgetUsage,
    priority: i64,
    selection: Option<SelectedProfile>,
  ) -> Result<Self, FactoryError> {
    input.verify_admission(admitted)?;
    node.validate_execution_window()?;
    let definition = admitted.closure().definition(input.definition()).ok_or_else(invalid)?;
    let declared = definition.node(input.node()).ok_or_else(invalid)?;
    let candidate = selection.as_ref().and_then(|selected| {
      declared
        .profile_selection()?
        .candidates
        .values()
        .find(|candidate| candidate.profile.reference() == &selected.profile)
    });
    let binding = match &selection {
      None => declared.build(),
      Some(selected) => {
        let candidate = candidate.ok_or_else(invalid)?;
        if selected.budget != candidate.profile.definition().budget {
          return Err(invalid());
        }
        Some(&candidate.build)
      }
    }
    .ok_or_else(invalid)?;
    let permissions = candidate.map_or_else(
      || declared.permissions().clone(),
      |candidate| {
        candidate
          .profile
          .definition()
          .permissions
          .clone()
          .with_elapsed_limit(candidate.profile.definition().budget.max_elapsed_millis())
      },
    );
    if node.factory_run_id() != input.factory_run_id()
      || node.flow_run_id() != input.flow_run_id()
      || node.workflow_cycle_id() != input.cycle_id()
      || node.node_key() != input.node()
      || node.node_kind() != declared.kind()
      || node.input_digest() != input.digest()?
      || node.budget() != declared.budget()
      || node.execution() != &NodeExecutionIdentity::External(binding.tool.clone())
      || input.context().is_none()
      || binding.build_configuration.project_id() != input.subject().project_id()
    {
      return Err(invalid());
    }
    if let Some(deliverables) = &binding.capture_deliverables {
      deliverables.freeze(&input)?;
    }
    let budget = node.budget();
    usage_before
      .checked_add(reservation(budget))
      .ok_or_else(invalid)?
      .validate(admitted.limits().budget())?;
    let profile = FlowBuildProfile::from_binding(definition.reference(), declared.key().clone(), binding);
    let intent = Self {
      verification_check: if definition
        .execution()
        .verification()
        .is_some_and(|policy| policy.checks.contains_key(input.node()))
      {
        Some(crate::VerificationCheckInput::new(admitted, &input)?)
      } else {
        None
      },
      input,
      selection,
      node,
      usage_before,
      priority,
      profile,
      action: declared.action().cloned(),
      result_outcome: binding.result_outcome.clone(),
      permissions,
    };
    if intent.bytes()?.len() > MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    Ok(intent)
  }
  /// Checks the local ceiling at intent publication, using owner-retained history.
  /// The prospective attempt may already be present in the append; it is counted
  /// exactly once. This is not an observation/replay check against later history.
  pub fn verify_reservation(
    &self,
    admitted: &AdmittedFlow,
    history: crate::FlowRuntimeHistory<'_>,
  ) -> Result<(), FactoryError> {
    Self::verify_budget(&self.input, admitted, history, self.usage_before, Some(self.node.id()))
  }
  /// Checks the ordinary stage reservation before selection or intent publication.
  /// Waiting allocates no attempt and must leave capacity for the ordinary stage reservation.
  pub fn verify_available_budget(
    input: &FlowNodeInput,
    admitted: &AdmittedFlow,
    history: crate::FlowRuntimeHistory<'_>,
    usage_before: BudgetUsage,
  ) -> Result<(), FactoryError> {
    Self::verify_budget(input, admitted, history, usage_before, None)
  }

  fn verify_budget(
    input: &FlowNodeInput,
    admitted: &AdmittedFlow,
    history: crate::FlowRuntimeHistory<'_>,
    usage_before: BudgetUsage,
    prospective_attempt: Option<crate::NodeAttemptId>,
  ) -> Result<(), FactoryError> {
    input.verify_admission(admitted)?;
    let definition = admitted.closure().definition(input.definition()).ok_or_else(invalid)?;
    let budget = definition.node(input.node()).ok_or_else(invalid)?.budget();
    usage_before
      .checked_add(reservation(budget))
      .ok_or_else(invalid)?
      .validate(admitted.limits().budget())?;
    let mut usage = history
      .observed_usage_by_flow()?
      .get(&input.flow_run_id())
      .copied()
      .unwrap_or_default();
    usage.attempts = u32::try_from(
      history
        .attempts
        .iter()
        .filter(|attempt| attempt.flow_run_id() == input.flow_run_id() && Some(attempt.id()) != prospective_attempt)
        .count(),
    )
    .map_err(|_| invalid())?;
    usage
      .checked_add(reservation(budget))
      .ok_or_else(invalid)?
      .validate(definition.execution().budget())
      .map(|_| ())
  }
  /// Exact installed action parameters, absent for command/reasoning nodes.
  #[must_use]
  pub const fn action(&self) -> Option<&crate::FlowActionBinding> {
    self.action.as_ref()
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
    let selection = serde_json::from_value(wire.get("selection").cloned().unwrap_or(serde_json::Value::Null))
      .map_err(|_| invalid())?;
    let intent = Self::with_selection(input.clone(), admitted, node.clone(), usage, priority, selection)?;
    if wire != serde_json::to_value(&intent).map_err(|_| invalid())? || intent.operation_id()? != expected_digest {
      return Err(invalid());
    }
    Ok(intent)
  }
  /// Rechecks the complete immutable static or selected contract against its pinned admission.
  pub fn verify_admission(&self, admitted: &AdmittedFlow) -> Result<(), FactoryError> {
    if Self::with_selection(
      self.input.clone(),
      admitted,
      self.node.clone(),
      self.usage_before,
      self.priority,
      self.selection.clone(),
    )? != *self
    {
      return Err(invalid());
    }
    Ok(())
  }
  /// Exact chosen profile identity, available without a mutable catalog during package recovery.
  #[must_use]
  pub fn selected_profile(&self) -> Option<&crate::FactoryRoleProfileRef> {
    self.selection.as_ref().map(|selection| &selection.profile)
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
  /// Selected generation bound to this operation, absent for ordinary static nodes.
  #[must_use]
  pub fn selection_digest(&self) -> Option<FactoryDigest> {
    self.selection.as_ref().map(|s| s.digest)
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
  /// Returns the immutable execution ceiling, narrowed to the selected profile when present.
  /// The Node Attempt separately retains the enclosing stage reservation.
  #[must_use]
  pub const fn budget(&self) -> BudgetLimit {
    match &self.selection {
      Some(selected) => selected.budget,
      None => self.node.budget(),
    }
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
  /// Returns the exact configured verification snapshot retained before dispatch.
  #[must_use]
  pub fn verification_plan(&self) -> Option<&crate::VerificationPlan> {
    self
      .verification_check
      .as_ref()
      .map(crate::VerificationCheckInput::plan)
  }
  /// Returns the exact selected verification input and common plan, absent for unselected nodes.
  #[must_use]
  pub const fn verification_check(&self) -> Option<&crate::VerificationCheckInput> {
    self.verification_check.as_ref()
  }
}
fn reservation(budget: BudgetLimit) -> BudgetUsage {
  BudgetUsage {
    attempts: 1,
    elapsed_millis: budget.max_elapsed_millis(),
    tokens: budget.max_tokens(),
    cost_micro_units: budget.max_cost_micro_units(),
    output_bytes: budget.max_output_bytes(),
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Flow Build intent",
  }
}
