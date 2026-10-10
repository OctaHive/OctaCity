use crate::{FactoryDigest, FactoryKey, FlowDefinitionRef, ImmutableReference};
use serde::{Deserialize, Serialize};

/// Immutable execution selection embedded in a node before its definition is hashed.
///
/// Definition and node identities are supplied by the containing declaration,
/// avoiding a circular content reference. Only the configured observation outcome
/// may be emitted; semantic routing belongs to later deterministic nodes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowBuildBinding {
  /// Exact selected harness or deterministic tool.
  pub tool: ImmutableReference,
  /// Exact runner plugin.
  pub plugin: ImmutableReference,
  /// Exact model or command configuration.
  pub model_or_tool: ImmutableReference,
  /// Exact task/prompt contract.
  pub task_digest: FactoryDigest,
  /// Ordinary Build configuration that materializes the Job DAG.
  pub build_configuration: crate::BuildConfigurationRef,
  /// Declared logical output containing schema-validated observations.
  pub result_output: FactoryKey,
  /// Finite observation outcome selected by the operator, never by provider JSON.
  pub result_outcome: FactoryKey,
  /// Additional exact outputs independently checked by their configured tools.
  #[serde(default)]
  pub evidence: Vec<crate::EvidenceRequirement>,
}

impl FlowBuildBinding {
  pub(super) fn validate_node(&self, node: &crate::FlowNodeDefinition) -> Result<(), crate::FactoryError> {
    if !matches!(
      node.kind(),
      crate::FlowNodeKind::BuildCommand | crate::FlowNodeKind::Reasoning
    ) || node.input_schema().is_none()
      || node.outcome(&self.result_outcome).is_none()
      || node.stage_projection().is_some()
      || self.evidence.len() >= crate::MAX_FLOW_SCHEMA_ENTRIES
      || self
        .evidence
        .iter()
        .any(|requirement| requirement.kind() == &self.result_output)
      || self
        .evidence
        .iter()
        .map(|requirement| requirement.kind())
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != self.evidence.len()
    {
      return Err(crate::FactoryError::InvalidConfiguration {
        field: "Flow Build binding",
      });
    }
    Ok(())
  }
}

/// Exact tool/model/task selection for one externally executed Flow node.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowBuildProfile {
  /// Exact definition containing the node.
  pub definition: FlowDefinitionRef,
  /// Stable node key in that definition.
  pub node: FactoryKey,
  /// Exact selected harness or deterministic tool.
  pub tool: ImmutableReference,
  /// Exact runner plugin.
  pub plugin: ImmutableReference,
  /// Exact model or command configuration.
  pub model_or_tool: ImmutableReference,
  /// Exact task/prompt contract.
  pub task_digest: FactoryDigest,
  /// Exact ordinary Build configuration that materializes this node's Job DAG.
  pub build_configuration: crate::BuildConfigurationRef,
  /// Declared logical output containing typed, non-authoritative observations.
  pub result_output: FactoryKey,
}
