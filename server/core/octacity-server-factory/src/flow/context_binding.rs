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
  /// RFC 6901 pointer whose presence requires this source. Absent guards omit
  /// the selection; present but invalid source values still reject the input.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub when_present: Option<String>,
  /// Declared semantic category, independent of node names.
  pub source_kind: ContextSourceKind,
  /// Explicit mapping from frozen input to an exact context reference.
  pub source: FlowDataMapping,
}

/// Input-independent metadata for one declared Context dependency.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowContextSourceObservation {
  /// Unguarded selections (including the root pointer) always contribute to readiness.
  pub required: bool,
  /// A literal exact reference, when the declaration does not require Work input.
  pub source: Option<FactoryContextReference>,
}

/// Closed context selection for any configured node; control edges grant no data.
/// Artifact references include content identity and size; repository references
/// pin their revision and range. Matching a logical name cannot substitute a source.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "BindingWire")]
pub struct FlowContextBinding {
  candidate_revision: Option<FlowDataMapping>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  candidate_when_present: Option<String>,
  entries: BTreeMap<FactoryKey, FlowContextSelection>,
  #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
  artifact_collections: BTreeMap<FactoryKey, FlowContextSelection>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BindingWire {
  #[serde(default)]
  candidate_revision: Option<FlowDataMapping>,
  #[serde(default)]
  candidate_when_present: Option<String>,
  entries: BTreeMap<FactoryKey, FlowContextSelection>,
  #[serde(default)]
  artifact_collections: BTreeMap<FactoryKey, FlowContextSelection>,
}
impl TryFrom<BindingWire> for FlowContextBinding {
  type Error = FactoryError;
  fn try_from(wire: BindingWire) -> Result<Self, Self::Error> {
    let binding = Self {
      candidate_revision: wire.candidate_revision,
      candidate_when_present: wire.candidate_when_present,
      entries: wire.entries,
      artifact_collections: wire.artifact_collections,
    };
    binding.validate()?;
    Ok(binding)
  }
}
impl FlowContextBinding {
  pub(super) fn replace_owned_entries(
    original: Option<&Self>,
    owned: &[FactoryKey],
    entries: BTreeMap<FactoryKey, FlowContextSelection>,
  ) -> Result<Option<Self>, FactoryError> {
    let mut binding = original.cloned().unwrap_or(Self {
      candidate_revision: None,
      candidate_when_present: None,
      entries: BTreeMap::new(),
      artifact_collections: BTreeMap::new(),
    });
    binding.entries.retain(|key, _| !owned.contains(key));
    binding.entries.extend(entries);
    if binding.entries.is_empty()
      && binding.artifact_collections.is_empty()
      && binding.candidate_revision.is_none()
      && binding.candidate_when_present.is_none()
    {
      return Ok(None);
    }
    binding.validate()?;
    Ok(Some(binding))
  }
  /// Freezes required context sources and a selected candidate revision.
  /// Absence of a candidate mapping requires the original exact Work subject.
  pub fn new(
    candidate_revision: Option<FlowDataMapping>,
    entries: BTreeMap<FactoryKey, FlowContextSelection>,
  ) -> Result<Self, FactoryError> {
    let binding = Self {
      candidate_revision,
      candidate_when_present: None,
      entries,
      artifact_collections: BTreeMap::new(),
    };
    binding.validate()?;
    Ok(binding)
  }
  pub(super) fn guarantees(&self, name: &FactoryKey) -> bool {
    self
      .entries
      .get(name)
      .is_some_and(|selection| selection.when_present.is_none())
  }
  fn validate(&self) -> Result<(), FactoryError> {
    if self.candidate_when_present.is_some() && self.candidate_revision.is_none() {
      return Err(invalid());
    }
    for path in self.candidate_when_present.iter().chain(
      self
        .entries
        .values()
        .chain(self.artifact_collections.values())
        .filter_map(|selection| selection.when_present.as_ref()),
    ) {
      super::predicate::validate_pointer(path).map_err(|_| invalid())?;
    }
    if (self.entries.is_empty() && self.artifact_collections.is_empty())
      || self.entries.len() + self.artifact_collections.len() > MAX_CONTEXT_MANIFEST_ENTRIES
      || serde_json::to_vec(self).map_err(|_| invalid())?.len() > MAX_FLOW_INPUT_BINDING_BYTES
    {
      return Err(invalid());
    }
    Ok(())
  }
  /// Observes declarations without reading Context bytes or inventing input.
  /// Required dynamic and collection sources remain required but unobserved.
  pub fn static_sources(&self) -> Vec<(FactoryKey, FlowContextSourceObservation)> {
    self
      .entries
      .iter()
      .map(|(name, selection)| (name, selection, true))
      .chain(
        self
          .artifact_collections
          .iter()
          .map(|(name, selection)| (name, selection, false)),
      )
      .map(|(name, selection, literal)| {
        let required = selection.when_present.as_ref().is_none_or(String::is_empty);
        let source = if required && literal {
          selection
            .source
            .project_raw(&serde_json::Value::Null)
            .ok()
            .and_then(|value| serde_json::from_value(value).ok())
        } else {
          None
        };
        (name.clone(), FlowContextSourceObservation { required, source })
      })
      .collect()
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
      .filter(|(_, selection)| applies(selection.when_present.as_deref(), input))
      .map(|(name, selection)| {
        let source = serde_json::from_value(selection.source.project_raw(input.value())?).map_err(|_| invalid())?;
        Ok((name.clone(), (selection.source_kind, source)))
      })
      .collect::<Result<BTreeMap<_, _>, FactoryError>>()?;
    for (prefix, collection) in &self.artifact_collections {
      if !applies(collection.when_present.as_deref(), input) {
        continue;
      }
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
    if !applies(self.candidate_when_present.as_deref(), input) {
      return Ok(None);
    }
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
    let requested = self.candidate_revision(input.payload())?;
    let actual = context
      .subject()
      .candidate()
      .map(|candidate| candidate.candidate_revision());
    let matches = if input.code().is_some() {
      context.subject() == &input.task_subject()? && requested.as_ref().is_none_or(|revision| Some(revision) == actual)
    } else {
      requested.as_ref() == actual
    };
    if !matches {
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
fn applies(when_present: Option<&str>, input: &FlowPayload) -> bool {
  when_present.is_none_or(|path| input.value().pointer(path).is_some())
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Flow context binding",
  }
}
