//! Template defaults select fields; the Project catalog remains their only type/schema owner.
use super::*;
use octacity_server_store::{IssueFieldForm, IssueFieldFormContract};
use std::collections::BTreeMap;

pub(super) fn bind(
  template: &mut FlowDefinitionTemplate,
  defaults: BTreeMap<FactoryKey, Vec<IssueFormField>>,
  overrides: BTreeMap<FactoryKey, IssueFieldForm>,
  catalog: &octacity_server_store::PublishedIssueProperties,
  writer: &ImmutableReference,
  policy: &FactoryKey,
) -> Result<(), String> {
  bind_forms(template, defaults, overrides, catalog, writer, policy, false)
}

/// Keep an unavailable selected field visible for repair in the ordinary result editor.
pub(super) fn draft(
  template: &mut FlowDefinitionTemplate,
  defaults: BTreeMap<FactoryKey, Vec<IssueFormField>>,
  catalog: &octacity_server_store::PublishedIssueProperties,
  writer: &ImmutableReference,
  policy: &FactoryKey,
) -> Result<(), String> {
  bind_forms(template, defaults, BTreeMap::new(), catalog, writer, policy, true)
}

fn bind_forms(
  template: &mut FlowDefinitionTemplate,
  defaults: BTreeMap<FactoryKey, Vec<IssueFormField>>,
  overrides: BTreeMap<FactoryKey, IssueFieldForm>,
  catalog: &octacity_server_store::PublishedIssueProperties,
  writer: &ImmutableReference,
  policy: &FactoryKey,
  allow_incomplete: bool,
) -> Result<(), String> {
  let mut forms = defaults
    .into_iter()
    .map(|(node, fields)| {
      (
        node,
        IssueFieldForm {
          project_id: catalog.project_id,
          definition_version: catalog.version,
          fields,
        },
      )
    })
    .collect::<BTreeMap<_, _>>();
  forms.extend(overrides);
  for (name, form) in forms {
    let invalid = || format!("/issue_fields/{name}");
    let shape = form.form_schema(catalog);
    if shape.is_err() && !allow_incomplete {
      return Err(invalid());
    }
    let node = template
      .nodes
      .iter_mut()
      .find(|node| node.key == name)
      .ok_or_else(invalid)?;
    let [outcome] = node.outcomes.as_mut_slice() else {
      return Err(invalid());
    };
    if node.kind != FlowNodeKind::Reasoning || outcome.kind != FlowOutcomeKind::Success {
      return Err(invalid());
    }
    let schema = template.schemas.get_mut(&outcome.schema).ok_or_else(invalid)?;
    if let Ok(shape) = shape {
      schema.shape = shape;
    }
    schema.version = FactoryKey::new(catalog.version.to_string()).map_err(|_| invalid())?;
    outcome.issue_fields = Some(FlowIssueFieldWrite {
      writer: writer.clone(),
      policy: policy.clone(),
      form,
    });
    node.permissions = node
      .permissions
      .clone()
      .with_plugin(writer.clone())
      .map_err(|_| invalid())?;
  }
  Ok(())
}
