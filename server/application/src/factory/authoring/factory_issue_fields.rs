//! Server-owned Project field shapes shared by visual classification and snapshot authoring.
use octacity_server_factory::*;
use octacity_server_store::*;

const BUILTINS: [(FlowIssueFieldSource, &str); 8] = [
  (FlowIssueFieldSource::Title, "Title"),
  (FlowIssueFieldSource::Description, "Description"),
  (FlowIssueFieldSource::Acceptance, "Acceptance"),
  (FlowIssueFieldSource::Priority, "Priority"),
  (FlowIssueFieldSource::Component, "Component"),
  (FlowIssueFieldSource::Type, "Type"),
  (FlowIssueFieldSource::Size, "Size"),
  (FlowIssueFieldSource::Dependencies, "Dependencies"),
];
/// Ordinary and semantically bound properties share the same bounded Project catalog.
pub const MAX_FACTORY_ISSUE_FIELD_CHOICES: usize = BUILTINS.len() + 2 * MAX_ISSUE_PROPERTIES;

/// Exact catalog pin and bounded individual shapes, including catalogs too large for one Flow schema.
#[derive(Clone, Debug, serde::Serialize)]
pub struct FactoryIssueFieldChoices {
  /// Immutable Project definition publication.
  pub definition_version: u64,
  /// Supported local-classification targets; no writer or automatic effect is implied.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub classification: Option<IssueClassificationFieldSelection>,
  /// Available fields; source identity, never its display label, selects semantics.
  pub fields: Vec<FactoryIssueFieldChoice>,
}
/// One ordinary/classification source; no Issue value or content is read during authoring.
#[derive(Clone, Debug, serde::Serialize)]
pub struct FactoryIssueFieldChoice {
  /// Canonical built-in, property ID or semantic binding.
  pub source: FlowIssueFieldSource,
  /// Current Project display name; built-in labels can be localized by source kind.
  pub name: String,
  /// Stable property ID for ordinary/semantic Project sources; absent for built-ins.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub property_id: Option<String>,
  /// Whether the exact catalog supports required as well as optional selection.
  pub required_supported: bool,
  /// Project-derived type/choices/bounds, normalized by the shared contract owner.
  pub shape: FlowValueSchema,
}

pub(crate) fn contracts(catalog: &PublishedIssueProperties) -> FactoryIssueFieldChoices {
  let ordinary = catalog.definition.properties.iter().map(|p| {
    (
      FlowIssueFieldSource::Property { id: p.id.clone() },
      p.name.to_string(),
      Some(p.id.clone()),
    )
  });
  let semantic = catalog.definition.classification.fields().filter_map(|(slot, id)| {
    let property = catalog.definition.properties.iter().find(|p| p.id == id)?;
    Some((
      FlowIssueFieldSource::Classification {
        slot: FactoryKey::new(slot).ok()?,
      },
      property.name.to_string(),
      Some(id.to_owned()),
    ))
  });
  let fields = BUILTINS
    .into_iter()
    .map(|(source, name)| (source, name.to_owned(), None))
    .chain(ordinary)
    .chain(semantic)
    .filter_map(|(source, name, property_id)| {
      let shape = crate::factory::issue_snapshot::field_shape(
        &FlowIssueFieldSelection {
          source: source.clone(),
          required: false,
        },
        catalog,
      )?;
      let required_supported = crate::factory::issue_snapshot::field_shape(
        &FlowIssueFieldSelection {
          source: source.clone(),
          required: true,
        },
        catalog,
      )
      .is_some();
      Some(FactoryIssueFieldChoice {
        source,
        name,
        property_id,
        required_supported,
        shape,
      })
    })
    .collect();
  FactoryIssueFieldChoices {
    definition_version: catalog.version,
    classification: crate::factory::reporting::factory_issue_classification_contract::selection(catalog),
    fields,
  }
}
