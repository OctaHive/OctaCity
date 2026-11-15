//! Project-derived field forms are shared by discovery, publication and the Issue writer.
use crate::*;
use octacity_server_factory::*;
use octacity_server_store::*;

/// Individual writable fields stay discoverable even if their combined schema exceeds a Flow budget.
pub(crate) fn selection(catalog: &PublishedIssueProperties) -> Option<IssueFieldForm> {
  let fields = [
    IssueFieldTarget::Priority,
    IssueFieldTarget::Component,
    IssueFieldTarget::Type,
    IssueFieldTarget::Size,
  ]
  .into_iter()
  .chain(
    catalog
      .definition
      .properties
      .iter()
      .filter(|p| !p.retired)
      .map(|p| IssueFieldTarget::Property { id: p.id.clone() }),
  )
  .filter_map(|field| {
    let selected = IssueFormField { field, required: true };
    let single = IssueFieldForm {
      project_id: catalog.project_id,
      definition_version: catalog.version,
      fields: vec![selected.clone()],
    };
    single.form_schema(catalog).ok().map(|_| selected)
  })
  .collect::<Vec<_>>();
  (!fields.is_empty()).then_some(IssueFieldForm {
    project_id: catalog.project_id,
    definition_version: catalog.version,
    fields,
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

/// One owner derives the selected form, with no copied vocabulary or result aliases.
pub(super) fn selected(catalog: &PublishedIssueProperties, selection: &IssueFieldForm) -> Option<FlowValueSchema> {
  selection.form_schema(catalog).ok()
}
