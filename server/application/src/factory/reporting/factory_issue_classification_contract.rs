//! Project catalogs own classification vocabulary; Flow stages share one pinned schema.
use crate::*;
use octacity_server_factory::*;
use octacity_server_store::*;
use std::collections::BTreeMap;

pub(super) fn current(catalog: &PublishedIssueProperties) -> Option<FactoryIssueClassificationFieldContract> {
  if catalog.version == 0 {
    return None;
  }
  let shape =
    |source| crate::factory::issue_snapshot::field_shape(&FlowIssueFieldSelection { source, required: true }, catalog);
  let mut fields = BTreeMap::new();
  for (name, source) in [
    ("priority", FlowIssueFieldSource::Priority),
    ("component", FlowIssueFieldSource::Component),
    ("kind", FlowIssueFieldSource::Type),
    ("size", FlowIssueFieldSource::Size),
  ] {
    if let Some(value) = shape(source) {
      fields.insert(FactoryKey::new(name).ok()?, FlowFieldSchema::optional(value));
    }
  }
  let active = |id: &Option<String>| {
    id.as_ref()
      .filter(|id| shape(FlowIssueFieldSource::Property { id: (*id).clone() }).is_some())
      .cloned()
  };
  let properties = IssueClassificationBindings {
    severity: active(&catalog.definition.classification.severity),
    risk: active(&catalog.definition.classification.risk),
    recommended_route: active(&catalog.definition.classification.recommended_route),
    reproducibility: active(&catalog.definition.classification.reproducibility),
    ..Default::default()
  };
  for (name, id) in properties.fields() {
    fields.insert(
      FactoryKey::new(name).ok()?,
      FlowFieldSchema::optional(shape(FlowIssueFieldSource::Property { id: id.to_owned() })?),
    );
  }
  let selected = |name| fields.contains_key(&FactoryKey::new(name).expect("static classification field"));
  let selection = IssueClassificationFieldSelection {
    definition_version: catalog.version,
    priority: selected("priority"),
    component: selected("component"),
    kind: selected("kind"),
    size: selected("size"),
    properties,
  };
  // Capabilities must not fail as a whole when a valid Project vocabulary cannot
  // fit one Flow schema. Existing authoring bounds still apply to selected schemas.
  let schema = FlowDataSchema::new(
    FactoryKey::new("issue-classification.fields").ok()?,
    FactoryKey::new(catalog.version.to_string()).ok()?,
    FlowValueSchema::Object { fields },
  )
  .ok()?;
  Some(FactoryIssueClassificationFieldContract { selection, schema })
}

/// Compiles only the reviewed selection; an oversized all-fields preview is irrelevant.
pub(super) fn selected(
  catalog: &PublishedIssueProperties,
  selection: &IssueClassificationFieldSelection,
) -> Option<FlowValueSchema> {
  selection.validate(catalog).ok()?;
  let shape =
    |source| crate::factory::issue_snapshot::field_shape(&FlowIssueFieldSelection { source, required: true }, catalog);
  let mut fields = BTreeMap::new();
  for (name, enabled, source) in [
    ("priority", selection.priority, FlowIssueFieldSource::Priority),
    ("component", selection.component, FlowIssueFieldSource::Component),
    ("kind", selection.kind, FlowIssueFieldSource::Type),
    ("size", selection.size, FlowIssueFieldSource::Size),
  ] {
    if enabled {
      fields.insert(FactoryKey::new(name).ok()?, FlowFieldSchema::optional(shape(source)?));
    }
  }
  for (name, id) in selection.properties.fields() {
    fields.insert(
      FactoryKey::new(name).ok()?,
      FlowFieldSchema::optional(shape(FlowIssueFieldSource::Property { id: id.to_owned() })?),
    );
  }
  Some(FlowValueSchema::Object { fields })
}
