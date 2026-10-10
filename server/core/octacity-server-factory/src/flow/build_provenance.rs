use crate::{
  EvidenceOutputKind, EvidenceProducer, FactoryArtifactReference, FactoryError, FactoryKey, FlowBuildProfile,
  FlowDefinition, FlowNodeInput, FlowNodeKind, FlowPayload, ImmutableReference,
};
use octacity_server_domain::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Trusted retained-output metadata verified by the ordinary Build output source.
///
/// These are owner facts, never accepted from provider JSON. Payload parsing and
/// publication/content checks occur before these references are constructed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowVerifiedBuildOutput {
  /// Exact configured logical output name.
  pub name: FactoryKey,
  /// Trusted publication category.
  pub output_kind: EvidenceOutputKind,
  /// Exact schema checked by the selected trusted verifier.
  pub schema: ImmutableReference,
  /// Retained exact bytes, including digest and size.
  pub artifact: FactoryArtifactReference,
  /// Actual immutable Build, Attempt, Job, tool and plugin.
  pub producer: EvidenceProducer,
  /// Trusted verification time.
  pub verified_at: Timestamp,
  /// Exclusive accepted freshness deadline.
  pub fresh_until: Timestamp,
  /// Schema-validated observations from an independently verified report.
  /// Presence alone is not a pass; configured gates decide from these typed facts.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub data: Option<FlowVerifiedBuildData>,
}

/// Bounded report facts with their complete content-addressed data contract.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "VerifiedDataWire")]
pub struct FlowVerifiedBuildData {
  contract: crate::FlowDataSchema,
  payload: FlowPayload,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VerifiedDataWire {
  contract: crate::FlowDataSchema,
  payload: serde_json::Value,
}
impl TryFrom<VerifiedDataWire> for FlowVerifiedBuildData {
  type Error = FactoryError;
  fn try_from(wire: VerifiedDataWire) -> Result<Self, Self::Error> {
    let payload = FlowPayload::restore(
      &serde_json::to_vec(&wire.payload).map_err(|_| invalid())?,
      &wire.contract,
    )?;
    Self::new(wire.contract, payload.value().clone())
  }
}
impl FlowVerifiedBuildData {
  /// Validates exact bytes' decoded facts before retaining them in the owner journal.
  pub fn new(contract: crate::FlowDataSchema, value: serde_json::Value) -> Result<Self, FactoryError> {
    let payload = FlowPayload::new(&contract, value)?;
    let data = Self { contract, payload };
    if serde_json::to_vec(&data).map_err(|_| invalid())?.len() > crate::MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    Ok(data)
  }
  /// Returns typed verified observations, without any inferred acceptance decision.
  #[must_use]
  pub const fn payload(&self) -> &FlowPayload {
    &self.payload
  }
}

/// Common Build provenance retained in the immutable node result and its digest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowBuildProvenance {
  profile: FlowBuildProfile,
  outputs: Vec<FlowVerifiedBuildOutput>,
}
impl FlowBuildProvenance {
  /// Returns the exact executed profile bound to the containing node.
  #[must_use]
  pub const fn profile(&self) -> &FlowBuildProfile {
    &self.profile
  }
  /// Retains trusted verified references; the containing record validates its exact node binding.
  pub fn new(profile: FlowBuildProfile, mut outputs: Vec<FlowVerifiedBuildOutput>) -> Result<Self, FactoryError> {
    outputs.sort_by(|a, b| a.name.cmp(&b.name));
    let mut artifacts = BTreeSet::new();
    if outputs.is_empty()
      || outputs.len() > 64
      || outputs.windows(2).any(|pair| pair[0].name == pair[1].name)
      || outputs.iter().any(|output| {
        !artifacts.insert(output.artifact.artifact_id())
          || output.verified_at >= output.fresh_until
          || output.data.as_ref().is_some_and(|data| {
            data.payload().schema() != &output.schema || output.output_kind != EvidenceOutputKind::Report
          })
      })
    {
      return Err(invalid());
    }
    Ok(Self { profile, outputs })
  }
  /// Returns exact trusted output references, without provider-selected acceptance flags.
  #[must_use]
  pub fn outputs(&self) -> &[FlowVerifiedBuildOutput] {
    &self.outputs
  }
  pub(super) fn validate(
    &self,
    input: &FlowNodeInput,
    payload: &FlowPayload,
    definition: &FlowDefinition,
  ) -> Result<(), FactoryError> {
    let node = definition.node(input.node()).ok_or_else(invalid)?;
    let binding = node.build().ok_or_else(invalid)?;
    let raw = self
      .outputs
      .iter()
      .find(|output| output.name == binding.result_output)
      .ok_or_else(invalid)?;
    if !matches!(node.kind(), FlowNodeKind::BuildCommand | FlowNodeKind::Reasoning)
      || Self::new(self.profile.clone(), self.outputs.clone())? != *self
      || self.profile.definition != definition.reference()
      || &self.profile.node != input.node()
      || self.profile.tool != binding.tool
      || self.profile.plugin != binding.plugin
      || self.profile.model_or_tool != binding.model_or_tool
      || self.profile.task_digest != binding.task_digest
      || self.profile.build_configuration != binding.build_configuration
      || self.profile.result_output != binding.result_output
      || &raw.schema != payload.schema()
      || raw.output_kind != EvidenceOutputKind::Report
      || raw.producer.tool() != &binding.tool
      || raw.producer.plugin() != &binding.plugin
      || self.outputs.len() != binding.evidence.len() + 1
      || binding.evidence.iter().any(|requirement| {
        self
          .outputs
          .iter()
          .find(|output| &output.name == requirement.kind())
          .is_none_or(|output| {
            output.output_kind != requirement.output_kind()
              || requirement.output_kind() == EvidenceOutputKind::Report && output.data.is_none()
              || &output.schema != requirement.schema()
              || output.producer.tool() != requirement.tool()
              || output.producer.plugin() != requirement.plugin()
          })
      })
      || self.outputs.iter().any(|output| {
        output.producer.build_id() != raw.producer.build_id()
          || output.producer.attempt_id() != raw.producer.attempt_id()
      })
    {
      return Err(invalid());
    }
    Ok(())
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Flow Build provenance",
  }
}
