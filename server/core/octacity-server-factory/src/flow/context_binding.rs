use crate::{
  ContextSourceKind, FactoryContextReference, FactoryError, FactoryKey, FlowDataMapping, FlowNodeInput, FlowPayload,
  MAX_CONTEXT_MANIFEST_ENTRIES, MAX_FLOW_INPUT_BINDING_BYTES,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One required immutable source in a node's closed context contract.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowContextSelection {
  /// Declared semantic category, independent of node names.
  pub source_kind: ContextSourceKind,
  /// Explicit mapping from frozen input to an exact context reference.
  pub source: FlowDataMapping,
}

/// Closed context selection for any configured node; control edges grant no data.
/// Artifact references include content identity and size; repository references
/// pin their revision and range. Matching a logical name cannot substitute a source.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "BindingWire")]
pub struct FlowContextBinding {
  candidate_revision: Option<FlowDataMapping>,
  entries: BTreeMap<FactoryKey, FlowContextSelection>,
  #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
  artifact_collections: BTreeMap<FactoryKey, FlowContextSelection>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BindingWire {
  #[serde(default)]
  candidate_revision: Option<FlowDataMapping>,
  entries: BTreeMap<FactoryKey, FlowContextSelection>,
  #[serde(default)]
  artifact_collections: BTreeMap<FactoryKey, FlowContextSelection>,
}
impl TryFrom<BindingWire> for FlowContextBinding {
  type Error = FactoryError;
  fn try_from(wire: BindingWire) -> Result<Self, Self::Error> {
    let binding = Self {
      candidate_revision: wire.candidate_revision,
      entries: wire.entries,
      artifact_collections: wire.artifact_collections,
    };
    binding.validate()?;
    Ok(binding)
  }
}
impl FlowContextBinding {
  /// Freezes required context sources and a selected candidate revision.
  /// Absence of a candidate mapping requires the original exact Work subject.
  pub fn new(
    candidate_revision: Option<FlowDataMapping>,
    entries: BTreeMap<FactoryKey, FlowContextSelection>,
  ) -> Result<Self, FactoryError> {
    let binding = Self {
      candidate_revision,
      entries,
      artifact_collections: BTreeMap::new(),
    };
    binding.validate()?;
    Ok(binding)
  }
  fn validate(&self) -> Result<(), FactoryError> {
    if (self.entries.is_empty() && self.artifact_collections.is_empty())
      || self.entries.len() + self.artifact_collections.len() > MAX_CONTEXT_MANIFEST_ENTRIES
      || serde_json::to_vec(self).map_err(|_| invalid())?.len() > MAX_FLOW_INPUT_BINDING_BYTES
    {
      return Err(invalid());
    }
    Ok(())
  }
  /// Resolves only the declared immutable sources for a trusted context collector.
  /// The collector still verifies bytes, subject, size and any retrieval receipt;
  /// this pure projection performs no discovery or artifact access grant.
  pub fn selected_sources(
    &self,
    input: &FlowPayload,
  ) -> Result<BTreeMap<FactoryKey, (ContextSourceKind, FactoryContextReference)>, FactoryError> {
    let mut selected = self
      .entries
      .iter()
      .map(|(name, selection)| {
        let source = serde_json::from_value(selection.source.project_raw(input.value())?).map_err(|_| invalid())?;
        Ok((name.clone(), (selection.source_kind, source)))
      })
      .collect::<Result<BTreeMap<_, _>, FactoryError>>()?;
    for (prefix, collection) in &self.artifact_collections {
      // Collections select only immutable Artifact references. Repository access
      // and discovery still require separately declared receipt-bound selections.
      let artifacts: Vec<crate::FactoryArtifactReference> =
        serde_json::from_value(collection.source.project_raw(input.value())?).map_err(|_| invalid())?;
      if selected.len() + artifacts.len() > MAX_CONTEXT_MANIFEST_ENTRIES {
        return Err(invalid());
      }
      for (index, artifact) in artifacts.into_iter().enumerate() {
        let key = FactoryKey::new(format!("{prefix}.{index}")).map_err(|_| invalid())?;
        if selected
          .insert(
            key,
            (collection.source_kind, FactoryContextReference::Artifact(artifact)),
          )
          .is_some()
        {
          return Err(invalid());
        }
      }
    }
    if selected.is_empty() {
      return Err(invalid());
    }
    Ok(selected)
  }
  /// Resolves the explicitly selected candidate; absence selects the original Work base.
  pub fn candidate_revision(
    &self,
    input: &FlowPayload,
  ) -> Result<Option<octacity_server_domain::ImmutableRevision>, FactoryError> {
    self
      .candidate_revision
      .as_ref()
      .map(|mapping| {
        let value = mapping.project_raw(input.value())?;
        octacity_server_domain::ImmutableRevision::new(value.as_str().ok_or_else(invalid)?).map_err(|_| invalid())
      })
      .transpose()
  }
  pub(super) fn verify(&self, input: &FlowNodeInput) -> Result<(), FactoryError> {
    let context = input.context().ok_or_else(invalid)?;
    if self.candidate_revision(input.payload())?.as_ref()
      != context
        .subject()
        .candidate()
        .map(|candidate| candidate.candidate_revision())
    {
      return Err(invalid());
    }
    let selected = self.selected_sources(input.payload())?;
    if context.entries().len() != selected.len() {
      return Err(invalid());
    }
    for entry in context.entries() {
      let (kind, source) = selected.get(entry.logical_identity()).ok_or_else(invalid)?;
      if entry.source_kind() != *kind || entry.source() != source {
        return Err(invalid());
      }
    }
    Ok(())
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Flow context binding",
  }
}
