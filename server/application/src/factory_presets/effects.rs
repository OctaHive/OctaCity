//! Reporting uses the existing finite compiler and ordinary exact child Flow calls.
use super::*;

pub(super) fn compose(
  mut template: FlowDefinitionTemplate,
  selections: std::collections::BTreeMap<FactoryKey, FactoryPresetEffectSelection>,
  root_id: FlowDefinitionId,
) -> Result<(FlowDefinitionTemplate, Vec<PublishedFlowTemplate>), ApplicationError> {
  // Every prospective v1 publication must have one unambiguous content identity.
  let mut identities = std::collections::BTreeSet::from([root_id]);
  if selections.values().any(|selection| !identities.insert(selection.id)) {
    return Err(FactoryValidationDiagnostics::invalid("/effects"));
  }
  let mut children = Vec::new();
  for (gate, selected) in selections {
    let node = template
      .nodes
      .iter()
      .find(|n| n.key == gate && n.kind == FlowNodeKind::DeterministicGate)
      .ok_or_else(|| FactoryValidationDiagnostics::invalid("/effects"))?;
    let schema = template
      .data_schemas()
      .map_err(|_| ApplicationError::invalid())?
      .remove(&node.outcomes[0].schema)
      .ok_or_else(ApplicationError::invalid)?;
    let outcomes = node.outcomes.iter().map(|o| o.key.clone()).collect();
    let child = selected
      .definition
      .compile(&schema, &outcomes, selected.id, FlowDefinitionVersion::INITIAL)?;
    template = selected.definition.bind(
      &template,
      &gate,
      &child,
      &FactoryKey::new(format!("report-{gate}")).map_err(|_| ApplicationError::invalid())?,
    )?;
    children.push(child);
  }
  Ok((template, children))
}

pub(super) fn ordinary_draft(
  root: &FlowDefinition,
  limits: FlowAdmissionLimits,
  data_schemas: Vec<FlowDataSchema>,
) -> FactoryFlowDefinitionDraft {
  FactoryFlowDefinitionDraft {
    root: FactoryFlowRootDraft {
      input_schema: root.input_schema().cloned(),
      entry: root.entry().clone(),
      nodes: root.nodes().to_vec(),
      transitions: root.transitions().to_vec(),
      terminals: root.terminals().to_vec(),
      context_projections: root.context_projections().to_vec(),
      data_projections: root.data_projections().to_vec(),
      execution: root.execution().clone(),
    },
    limits,
    data_schemas,
    pools: Default::default(),
  }
}
