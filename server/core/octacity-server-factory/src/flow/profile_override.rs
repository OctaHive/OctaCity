//! Exact per-Work choices for declared stages, independent of execution occurrences.
use crate::{FactoryError, FactoryKey, FactoryRoleProfileRef, FlowDefinitionRef, PinnedFlowDefinitionClosure};
use octacity_server_domain::ProjectId;
use serde::{Deserialize, Serialize};

/// One declared stage, distinguishing repeated calls of the same nested definition.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowStageAddress {
  /// Exact admitted root, including version and content digest.
  pub root: FlowDefinitionRef,
  /// Ordered subflow-call node keys from the root; empty addresses a root stage.
  pub calls: Vec<FactoryKey>,
  /// Logical node key in the final reached definition.
  pub node: FactoryKey,
}
impl FlowStageAddress {
  pub(super) fn for_occurrence(
    admitted: &crate::AdmittedFlow,
    history: crate::FlowRuntimeHistory<'_>,
    flow_id: crate::FlowRunId,
    node: &FactoryKey,
  ) -> Result<Self, FactoryError> {
    let mut flow = history
      .flow_runs
      .iter()
      .find(|flow| flow.id() == flow_id)
      .ok_or_else(invalid)?;
    let definition = flow.definition();
    let mut calls = Vec::new();
    while let Some(parent) = flow.parent() {
      if calls.len() >= usize::from(crate::MAX_FLOW_NESTING_DEPTH) {
        return Err(invalid());
      }
      let call = history
        .attempts
        .iter()
        .find(|call| call.id() == parent.node_attempt_id() && call.flow_run_id() == parent.flow_run_id())
        .ok_or_else(invalid)?;
      let owner = history
        .flow_runs
        .iter()
        .find(|flow| flow.id() == parent.flow_run_id())
        .ok_or_else(invalid)?;
      if owner.factory_run_id() != admitted.root_run().factory_run_id()
        || admitted
          .closure()
          .definition(owner.definition())
          .and_then(|definition| definition.node(call.node_key()))
          .and_then(crate::FlowNodeDefinition::subflow_definition)
          != Some(flow.definition())
      {
        return Err(invalid());
      }
      calls.push(call.node_key().clone());
      flow = owner;
    }
    if flow != admitted.root_run() {
      return Err(invalid());
    }
    calls.reverse();
    let address = Self {
      root: admitted.closure().root(),
      calls,
      node: node.clone(),
    };
    if address.definition(admitted.closure())? != definition {
      return Err(invalid());
    }
    Ok(address)
  }
  pub(super) fn definition(&self, closure: &PinnedFlowDefinitionClosure) -> Result<FlowDefinitionRef, FactoryError> {
    if self.root != closure.root() || self.calls.len() > usize::from(crate::MAX_FLOW_NESTING_DEPTH) {
      return Err(invalid());
    }
    let mut definition = self.root;
    for call in &self.calls {
      definition = closure
        .definition(definition)
        .and_then(|definition| definition.node(call))
        .and_then(crate::FlowNodeDefinition::subflow_definition)
        .ok_or_else(invalid)?;
    }
    Ok(definition)
  }
}

/// Explicit admission choice; it applies to every bounded occurrence of this stage.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowProfileAdmissionOverride {
  /// Exact declared stage without wildcards or mutable aliases.
  pub stage: FlowStageAddress,
  /// Complete exact profile reference from the pinned candidate map.
  pub profile: FactoryRoleProfileRef,
}

/// Canonically ordered bounded admission intent; one choice per declared stage.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
  try_from = "Vec<FlowProfileAdmissionOverride>",
  into = "Vec<FlowProfileAdmissionOverride>"
)]
pub struct FlowProfileAdmissionOverrides(Vec<FlowProfileAdmissionOverride>);
impl TryFrom<Vec<FlowProfileAdmissionOverride>> for FlowProfileAdmissionOverrides {
  type Error = FactoryError;
  fn try_from(mut entries: Vec<FlowProfileAdmissionOverride>) -> Result<Self, Self::Error> {
    if entries.len() > crate::MAX_FLOW_DEFINITION_NODES
      || entries
        .iter()
        .any(|entry| entry.stage.calls.len() > usize::from(crate::MAX_FLOW_NESTING_DEPTH))
      || serde_json::to_vec(&entries).map_err(|_| invalid())?.len() > crate::MAX_FLOW_DATA_BYTES
    {
      return Err(invalid());
    }
    entries.sort_by(|left, right| left.stage.cmp(&right.stage));
    if entries.windows(2).any(|pair| pair[0].stage == pair[1].stage) {
      return Err(invalid());
    }
    Ok(Self(entries))
  }
}
impl From<FlowProfileAdmissionOverrides> for Vec<FlowProfileAdmissionOverride> {
  fn from(value: FlowProfileAdmissionOverrides) -> Self {
    value.0
  }
}
impl FlowProfileAdmissionOverrides {
  /// Exact choices retained at admission in canonical stage order.
  #[must_use]
  pub fn entries(&self) -> &[FlowProfileAdmissionOverride] {
    &self.0
  }
  /// Whether the original Work request had no explicit choices.
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.0.is_empty()
  }
  /// Checks exact stage reachability, Project ownership and declared profile membership.
  pub fn validate(&self, closure: &PinnedFlowDefinitionClosure, project: ProjectId) -> Result<(), FactoryError> {
    for entry in &self.0 {
      let definition = entry.stage.definition(closure)?;
      let binding = closure
        .definition(definition)
        .and_then(|definition| definition.node(&entry.stage.node))
        .and_then(crate::FlowNodeDefinition::profile_selection)
        .ok_or_else(invalid)?;
      if entry.profile.project_id != project
        || !binding
          .candidates
          .values()
          .any(|candidate| candidate.profile.reference() == &entry.profile)
      {
        return Err(invalid());
      }
    }
    Ok(())
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "admission profile override",
  }
}
