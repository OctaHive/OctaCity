//! Selected declarations retain their complete publication pin, not an edited catalog.
use super::invalid_snapshot;
use crate::*;
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::{FactoryDigest, FactoryKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Exact selected property declaration and its meaning in the original publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueSnapshotPropertyDefinition {
  /// Original stable ID, display name, kind, bounds and complete choice vocabulary.
  pub definition: IssuePropertyDefinition,
  /// Original classification role; absence denotes an ordinary property.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub classification_slot: Option<FactoryKey>,
}

/// Versioned selected declarations; legacy full publications preserve their original wire.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum IssueSnapshotDefinitions {
  /// Explicit fragment of one exact publication, never a replacement catalog.
  SelectedV1 {
    /// Owning Project of the original publication.
    project_id: ProjectId,
    /// Original immutable catalog version, including the default publication zero.
    version: u64,
    /// SHA-256 of the complete original PublishedIssueProperties encoding.
    publication_digest: FactoryDigest,
    /// Original publication time, never the time of snapshot preparation.
    published_at: Timestamp,
    /// Present only when this publication supplies a selected built-in priority.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    priorities: Option<Vec<IssuePropertyChoice>>,
    /// Present only when this publication supplies a selected built-in component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    components: Option<Vec<IssuePropertyChoice>>,
    /// Present only when this publication supplies a selected built-in type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    types: Option<Vec<IssuePropertyChoice>>,
    /// Only selected property IDs, sorted and deduplicated within this publication.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    properties: Vec<IssueSnapshotPropertyDefinition>,
  },
  /// Historical complete catalog bytes. New reservations use SelectedV1.
  #[serde(untagged)]
  Legacy(PublishedIssueProperties),
}
impl IssueSnapshotDefinitions {
  /// Copies declarations referenced under this publication and seals its complete identity.
  pub fn selected<'a>(
    catalog: &PublishedIssueProperties,
    fields: impl IntoIterator<Item = &'a IssueClassificationFieldReference>,
  ) -> Result<Self, StoreError> {
    let mut priorities = None;
    let mut components = None;
    let mut types = None;
    let mut properties = BTreeMap::new();
    for field in fields
      .into_iter()
      .filter(|field| field.definition_version() == Some(catalog.version))
    {
      match field {
        IssueClassificationFieldReference::Priority { .. } => {
          priorities.get_or_insert_with(|| catalog.definition.priorities.clone());
        }
        IssueClassificationFieldReference::Component { .. } => {
          components.get_or_insert_with(|| catalog.definition.components.clone());
        }
        IssueClassificationFieldReference::Type { .. } => {
          types.get_or_insert_with(|| catalog.definition.types.clone());
        }
        IssueClassificationFieldReference::Property { id, .. } => {
          let definition = catalog
            .definition
            .properties
            .iter()
            .find(|property| &property.id == id)
            .ok_or_else(invalid_snapshot)?;
          properties
            .entry(id.clone())
            .or_insert_with(|| IssueSnapshotPropertyDefinition {
              definition: definition.clone(),
              classification_slot: catalog
                .definition
                .classification
                .fields()
                .find(|(_, bound)| *bound == id)
                .map(|(slot, _)| FactoryKey::new(slot).expect("canonical classification slot")),
            });
        }
        _ => {}
      }
    }
    Ok(Self::SelectedV1 {
      project_id: catalog.project_id,
      version: catalog.version,
      publication_digest: FactoryDigest::content_sha256(&serde_json::to_vec(catalog).map_err(|_| invalid_snapshot())?),
      published_at: catalog.published_at,
      priorities,
      components,
      types,
      properties: properties.into_values().collect(),
    })
  }
}
