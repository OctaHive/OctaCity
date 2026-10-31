//! Process purpose is versioned Flow content over the existing immutable input owner.
use super::{FlowContextBinding, FlowContextSelection, FlowDataMapping, FlowNodeDefinition, FlowValueMapping};
use crate::{ContextSourceKind, FactoryArtifactReference, FactoryError, FactoryKey};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// An exact managed input and its explicitly reviewed recipients; no ticket acceptance replacement.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowProcessDocument {
  /// Existing Project input publication, retaining exact bytes/digest/size and read authorization.
  pub artifact: FactoryArtifactReference,
  /// Step keys receiving this Specification input. An empty set grants no context.
  pub recipients: BTreeSet<FactoryKey>,
}

/// Common process goals and criteria, separate from shared Project guidance and step result schemas.
/// An explicitly empty value detaches process-owned inputs; omission preserves legacy node semantics.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowProcessInputs {
  /// Purpose of this process, absent when not configured.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub goals: Option<FlowProcessDocument>,
  /// Completion of this process, without authorizing a fix, write or delivery.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub criteria: Option<FlowProcessDocument>,
}

impl FlowProcessInputs {
  /// Normalizes only the two process-owned Context entries; all other selections stay independent.
  pub(super) fn bind(&self, nodes: &[FlowNodeDefinition]) -> Result<Vec<FlowNodeDefinition>, FactoryError> {
    let keys = [FactoryKey::new("process.goals")?, FactoryKey::new("process.criteria")?];
    let documents = [self.goals.as_ref(), self.criteria.as_ref()];
    for document in documents.into_iter().flatten() {
      if document.artifact.encoded_size() == 0
        || document.artifact.encoded_size() > crate::MAX_CONTEXT_MANIFEST_BYTES
        || document.recipients.len() > super::MAX_FLOW_DEFINITION_NODES
        || document
          .recipients
          .iter()
          .any(|key| !nodes.iter().any(|node| node.key() == key))
      {
        return Err(invalid());
      }
    }
    nodes
      .iter()
      .map(|node| {
        let mut entries = BTreeMap::new();
        for (key, document) in keys.iter().zip(documents) {
          if let Some(document) = document.filter(|document| document.recipients.contains(node.key())) {
            entries.insert(
              key.clone(),
              FlowContextSelection {
                when_present: None,
                source_kind: ContextSourceKind::Specification,
                source: FlowDataMapping::new(FlowValueMapping::Constant {
                  value: serde_json::json!({"kind":"artifact", "value":document.artifact}),
                })?,
              },
            );
          }
        }
        node
          .clone()
          .with_optional_context_binding(FlowContextBinding::replace_owned_entries(
            node.context_binding(),
            &keys,
            entries,
          )?)
      })
      .collect()
  }
}

fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "Flow process inputs",
  }
}
