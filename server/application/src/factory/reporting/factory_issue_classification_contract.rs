//! Project catalogs own classification vocabulary; Flow stages share one pinned schema.
use crate::*;
use octacity_server_factory::*;
use octacity_server_store::*;
use std::collections::BTreeMap;

/// Available targets stay usable even when their combined schema exceeds a Flow budget.
pub(crate) fn selection(catalog: &PublishedIssueProperties) -> Option<IssueClassificationFieldSelection> {
  if catalog.version == 0 {
    return None;
  }
  let available = |source| {
    crate::factory::issue_snapshot::field_shape(&FlowIssueFieldSelection { source, required: true }, catalog).is_some()
  };
  let active = |id: &Option<String>| {
    id.as_ref()
      .filter(|id| available(FlowIssueFieldSource::Property { id: (*id).clone() }))
      .cloned()
  };
  Some(IssueClassificationFieldSelection {
    definition_version: catalog.version,
    priority: available(FlowIssueFieldSource::Priority),
    component: available(FlowIssueFieldSource::Component),
    kind: available(FlowIssueFieldSource::Type),
    size: available(FlowIssueFieldSource::Size),
    properties: IssueClassificationBindings {
      severity: active(&catalog.definition.classification.severity),
      risk: active(&catalog.definition.classification.risk),
      recommended_route: active(&catalog.definition.classification.recommended_route),
      reproducibility: active(&catalog.definition.classification.reproducibility),
      ..Default::default()
    },
  })
}

pub(super) fn current(catalog: &PublishedIssueProperties) -> Option<FactoryIssueClassificationFieldContract> {
  let selection = selection(catalog)?;
  let schema = FlowDataSchema::new(
    FactoryKey::new("issue-classification.fields").ok()?,
    FactoryKey::new(catalog.version.to_string()).ok()?,
    selected(catalog, &selection)?,
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
