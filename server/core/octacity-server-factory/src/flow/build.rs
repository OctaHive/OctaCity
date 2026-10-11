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
  /// Optional independent verifier for the primary retained output.
  /// Absence uses the executed tool; this selection grants no execution authority.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub result_tool: Option<ImmutableReference>,
  /// Optional disjoint directory scope, frozen as signed capture and source write paths.
  /// Selection narrows source authority; absence retains the ordinary mount ceiling.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub capture_write_paths: Option<Vec<crate::FactoryRepositoryPath>>,
  /// Optional immutable public-deliverable contract enforced by trusted capture.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub capture_deliverables: Option<crate::CandidateDeliverableBinding>,
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
  /// Publication category of the exact JSON output selected for this result.
  pub result_output_kind: crate::EvidenceOutputKind,
  /// Finite observation outcome selected by the operator, never by provider JSON.
  pub result_outcome: FactoryKey,
  /// Additional exact outputs independently checked by their configured tools.
  #[serde(default)]
  pub evidence: Vec<crate::EvidenceRequirement>,
}

impl FlowBuildBinding {
  /// Returns the exact tool whose verified output can supply this node result.
  #[must_use]
  pub fn result_tool(&self) -> &ImmutableReference {
    self.result_tool.as_ref().unwrap_or(&self.tool)
  }
  pub(super) fn validate_node(&self, node: &crate::FlowNodeDefinition) -> Result<(), crate::FactoryError> {
    if !matches!(
      node.kind(),
      crate::FlowNodeKind::BuildCommand | crate::FlowNodeKind::Reasoning
    ) && !(node.kind() == crate::FlowNodeKind::TrustedAction
      && node.action().is_some_and(|action| action.plugin() == &self.tool))
      || node.input_schema().is_none()
      || node.outcome(&self.result_outcome).is_none()
      || self
        .capture_write_paths
        .as_ref()
        .is_some_and(|paths| !valid_capture_paths(paths))
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
  /// Optional independent verifier for the primary retained output.
  /// Absence uses the executed tool; this selection grants no execution authority.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub result_tool: Option<ImmutableReference>,
  /// Optional disjoint directory scope, frozen as signed capture and source write paths.
  /// Selection narrows source authority; absence retains the ordinary mount ceiling.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub capture_write_paths: Option<Vec<crate::FactoryRepositoryPath>>,
  /// Frozen public repository deliverable parameters for this exact execution.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub capture_deliverables: Option<crate::CandidateDeliverableBinding>,
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
  /// Publication category of the exact JSON output selected for this result.
  pub result_output_kind: crate::EvidenceOutputKind,
}

impl FlowBuildProfile {
  /// Returns the frozen verifier, independently of the executed harness.
  #[must_use]
  pub fn result_tool(&self) -> &ImmutableReference {
    self.result_tool.as_ref().unwrap_or(&self.tool)
  }
}

pub(super) fn valid_capture_paths(paths: &[crate::FactoryRepositoryPath]) -> bool {
  !paths.is_empty()
    && paths.len() <= crate::MAX_PERMISSION_ENTRIES_PER_CATEGORY
    && paths.windows(2).all(|pair| pair[0] < pair[1])
    && !paths
      .iter()
      .enumerate()
      .any(|(index, path)| paths.iter().take(index).any(|parent| path.is_within(parent)))
}
