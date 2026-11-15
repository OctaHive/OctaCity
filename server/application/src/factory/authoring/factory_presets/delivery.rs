//! Compiles declared example dependencies using ordinary Flow inputs, commands and actions.
use super::*;
use std::collections::BTreeMap;

pub(super) fn without_command(template: &mut FlowDefinitionTemplate, optional: &release::OptionalCommand) {
  template.nodes.retain(|node| !optional.nodes.contains(&node.key));
  template
    .transitions
    .retain(|route| !optional.nodes.contains(route.predecessor()));
  for route in &mut template.transitions {
    if route.predecessor() == optional.route.predecessor() && route.outcome() == optional.route.outcome() {
      *route = optional.route.clone();
    }
  }
  template.terminals.remove(&optional.terminal);
}

/// Exact external inputs use declared projections, not a producer name or result vocabulary.
pub(super) fn bind_predecessor(
  template: &mut FlowDefinitionTemplate,
  predecessor: &FlowDataSchema,
  parameters: &release::Incoming,
) -> Result<(), FactoryError> {
  let invalid = || FactoryError::InvalidConfiguration {
    field: "preset incoming contract",
  };
  let schemas = template.data_schemas()?;
  for (name, mapping) in &parameters.inputs {
    let node = template
      .nodes
      .iter_mut()
      .find(|node| &node.key == name)
      .ok_or_else(invalid)?;
    let target = schemas.get(&node.input_schema).ok_or_else(invalid)?;
    let binding = FlowInputBinding::new(
      [(
        parameters.source_alias.clone(),
        FlowInputSource::Incoming {
          schema: predecessor.reference().clone(),
          path: String::new(),
        },
      )]
      .into(),
      mapping.clone(),
    )?;
    if !binding.fits_incoming(predecessor.reference(), predecessor.shape(), target) {
      return Err(invalid());
    }
    node.input_binding = binding;
  }
  for (name, predicate) in &parameters.preconditions {
    let node = template
      .nodes
      .iter_mut()
      .find(|node| &node.key == name)
      .ok_or_else(invalid)?;
    node.gate = Some(
      node
        .gate
        .take()
        .ok_or_else(invalid)?
        .with_precondition(predicate.clone())?,
    );
  }
  template.schemas.insert(
    parameters.schema_alias.clone(),
    FlowTemplateSchema {
      identity: predecessor.reference().identity().clone(),
      version: predecessor.reference().version().clone(),
      shape: predecessor.shape().clone(),
    },
  );
  Ok(())
}

/// Resolves release-local aliases; the resulting graph uses only ordinary node contracts.
pub(super) fn bind_installed_steps(
  template: &mut FlowDefinitionTemplate,
  commands: &BTreeMap<FactoryKey, FactoryPresetCommandSelection>,
  publication: &FactoryPresetPublicationSelection,
  publisher: &FactoryCandidatePublicationChoice,
  publication_node: &FactoryKey,
) -> Result<BTreeMap<FactoryKey, FlowBuildBinding>, String> {
  let schemas = template.data_schemas().map_err(|_| "/commands".to_owned())?;
  let mut builds = BTreeMap::new();
  for node in template
    .nodes
    .iter_mut()
    .filter(|node| node.kind == FlowNodeKind::BuildCommand)
  {
    let alias = node.build_profile.as_ref().ok_or_else(|| "/commands".to_owned())?;
    let path = || format!("/commands/{alias}");
    let command = commands.get(alias).ok_or_else(path)?;
    let [outcome] = node.outcomes.as_slice() else {
      return Err(path());
    };
    if command.build.result_outcome != outcome.key || command.build.result_output_kind != EvidenceOutputKind::Report {
      return Err(path());
    }
    match schemas.get(&outcome.schema) {
      Some(schema) if schema != &command.result => return Err(format!("{}/result", path())),
      Some(_) => (),
      None => {
        let reference = command.result.reference();
        template.schemas.insert(
          outcome.schema.clone(),
          FlowTemplateSchema {
            identity: reference.identity().clone(),
            version: reference.version().clone(),
            shape: command.result.shape().clone(),
          },
        );
      }
    }
    node.permissions = command.permissions.clone();
    // The installed compiler's resource request must fit the authored node's
    // budget; retain any larger limits already reviewed in the template.
    let output_bytes = command
      .permissions
      .outputs()
      .max_artifact_bytes()
      .checked_add(command.permissions.outputs().max_report_bytes())
      .ok_or_else(path)?;
    node.budget = BudgetLimit::new(
      node.budget.max_attempts(),
      node
        .budget
        .max_elapsed_millis()
        .max(command.permissions.resources().elapsed_millis()),
      node.budget.max_tokens(),
      node.budget.max_cost_micro_units(),
      node.budget.max_output_bytes().max(output_bytes),
    )
    .map_err(|_| path())?;
    builds.insert(alias.clone(), command.build.clone());
  }
  let node = template
    .nodes
    .iter_mut()
    .find(|node| &node.key == publication_node)
    .ok_or_else(|| "/publication".to_owned())?;
  let original = node.action.as_ref().ok_or_else(|| "/publication".to_owned())?;
  let result = node
    .outcomes
    .iter()
    .find(|outcome| outcome.kind == FlowOutcomeKind::Success)
    .ok_or_else(|| "/publication".to_owned())?;
  if publisher.outcome.key() != &result.key
    || publisher.outcome.kind() != result.kind
    || !publisher.receipt_schema.as_ref().is_some_and(|receipt| {
      schemas.get(&result.schema) == Some(receipt) && publisher.outcome.schema() == receipt.reference()
    })
  {
    return Err("/publication/result".to_owned());
  }
  if !publication
    .permissions
    .plugins()
    .any(|plugin| plugin == &publication.plugin)
    || !publisher.permissions.is_no_broader_than(&publication.permissions)
  {
    return Err("/publication/permissions".to_owned());
  }
  let mut parameters = original.parameters().clone();
  parameters["target"] = serde_json::to_value(&publication.target).map_err(|_| "/publication/target".to_owned())?;
  node.action = Some(
    FlowActionBinding::new(publication.plugin.clone(), original.action().clone(), parameters)
      .map_err(|_| "/publication".to_owned())?,
  );
  node.permissions = publication.permissions.clone();
  Ok(builds)
}
