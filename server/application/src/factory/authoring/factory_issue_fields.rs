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
  pub classification: Option<IssueFieldForm>,
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
  /// Writable identity when the shared form owner supports this field; absence grants no save scope.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub write_target: Option<IssueFieldTarget>,
  /// Original Project type, preserving date semantics and exact choice metadata.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub value_type: Option<IssuePropertyKind>,
  /// Server-derived value key, independent of the field label and local snapshot alias.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub value_key: Option<FactoryKey>,
  /// Whether the exact catalog supports required as well as optional selection.
  pub required_supported: bool,
  /// Project-derived type/choices/bounds, normalized by the shared contract owner.
  pub shape: FlowValueSchema,
  /// Optional display labels; allowed IDs and their order remain owned by the shape.
  #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
  pub choice_labels: std::collections::BTreeMap<String, String>,
}

pub(crate) fn contracts(catalog: &PublishedIssueProperties) -> FactoryIssueFieldChoices {
  let selection = crate::factory::reporting::factory_issue_classification_contract::selection(catalog);
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
      let target = match &source {
        FlowIssueFieldSource::Priority => Some(IssueFieldTarget::Priority),
        FlowIssueFieldSource::Component => Some(IssueFieldTarget::Component),
        FlowIssueFieldSource::Type => Some(IssueFieldTarget::Type),
        FlowIssueFieldSource::Size => Some(IssueFieldTarget::Size),
        FlowIssueFieldSource::Property { id } => Some(IssueFieldTarget::Property { id: id.clone() }),
        _ => None,
      };
      let display = target.clone().or_else(|| {
        property_id
          .as_ref()
          .map(|id| IssueFieldTarget::Property { id: id.clone() })
      });
      let choice_labels = match display.and_then(|target| target.kind(catalog)) {
        Some(IssuePropertyKind::Choice { choices }) => choices
          .into_iter()
          .filter(|choice| !choice.retired)
          .map(|choice| (choice.id, choice.name))
          .collect(),
        _ => Default::default(),
      };
      let write_target = target.filter(|target| {
        selection
          .as_ref()
          .is_some_and(|form| form.fields.iter().any(|selected| &selected.field == target))
      });
      let value_key = write_target.as_ref().and_then(|target| target.key().ok());
      let value_type = write_target.as_ref().and_then(|target| target.kind(catalog));
      Some(FactoryIssueFieldChoice {
        source,
        name,
        property_id,
        required_supported,
        write_target,
        value_key,
        value_type,
        shape,
        choice_labels,
      })
    })
    .collect();
  FactoryIssueFieldChoices {
    definition_version: catalog.version,
    classification: selection,
    fields,
  }
}

/// Compiles an inclusive threshold from one exact Project choice order into an ordinary predicate.
/// The local input path is supplied by the consumer; later catalog edits do not alter the result.
pub fn issue_choice_threshold(
  catalog: &PublishedIssueProperties,
  source: &FlowIssueFieldSource,
  path: &str,
  starting_at: &str,
) -> Result<FlowPredicate, crate::ApplicationError> {
  let Some(FlowValueSchema::Enum { values }) = crate::factory::issue_snapshot::field_shape(
    &FlowIssueFieldSelection {
      source: source.clone(),
      required: true,
    },
    catalog,
  ) else {
    return Err(crate::ApplicationError::invalid());
  };
  let start = values
    .iter()
    .position(|value| value == starting_at)
    .ok_or_else(crate::ApplicationError::invalid)?;
  Ok(FlowPredicate::Any {
    predicates: values[start..]
      .iter()
      .map(|value| FlowPredicate::Compare {
        left: FlowOperand::Path { path: path.into() },
        comparison: FlowComparison::Equal,
        right: FlowOperand::Literal { value: value.clone() },
      })
      .collect(),
  })
}
