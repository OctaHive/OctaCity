//! Freeze pre-dispatch selection through the ordinary fenced node journal.
use crate::{ApplicationError, FactoryNodeTarget, prepare_factory_flow_input};
use octacity_server_domain::Timestamp;
use octacity_server_factory::{FlowNodeInput, FlowProfileSelectionRecord};
use octacity_server_store::{FactoryRunHistoryAppend, FactoryRunSnapshot};

pub(crate) async fn prepare(
  snapshot: &FactoryRunSnapshot,
  target: &FactoryNodeTarget,
  supplied: Option<FlowNodeInput>,
  at: Timestamp,
  catalog: Option<&dyn octacity_server_store::FactoryConfigurationStore>,
) -> Result<Option<FactoryRunHistoryAppend>, ApplicationError> {
  let generation = next_generation(snapshot, target)?;
  let frozen = frozen_input(snapshot, target)?;
  if frozen.is_some_and(|input| supplied.as_ref().is_some_and(|supplied| supplied != input)) {
    return Err(ApplicationError::invalid());
  }
  let input = match frozen.cloned().or(supplied) {
    Some(input) => input,
    None => prepare_factory_flow_input(snapshot, target)?,
  };
  if input.flow_run_id() != target.flow_run_id
    || input.cycle_id() != target.cycle_id
    || input.node() != &target.node
    || input.generation() != generation
  {
    return Err(ApplicationError::invalid());
  }
  let binding = snapshot
    .admitted_flow
    .closure()
    .definition(input.definition())
    .and_then(|definition| definition.node(input.node()))
    .and_then(|node| node.profile_selection())
    .ok_or_else(ApplicationError::invalid)?;
  let digest = input.digest().map_err(|_| ApplicationError::invalid())?;
  if snapshot
    .flow
    .data
    .profile_selections
    .iter()
    .any(|selection| selection.input_digest() == digest)
  {
    return Ok(None);
  }
  let selection =
    FlowProfileSelectionRecord::select(&input, &snapshot.admitted_flow, at).map_err(|_| ApplicationError::invalid())?;
  let needs_catalog = selection.needs_recovery()
    && matches!(
      binding.failure,
      octacity_server_factory::FlowProfileSelectionFailure::ReserveProfile { .. }
    );
  let choices = if needs_catalog {
    current_choices(snapshot, catalog).await?
  } else {
    None
  };
  let selection = selection
    .recover(&input, &snapshot.admitted_flow, choices.as_ref())
    .map_err(|_| ApplicationError::invalid())?;
  let mut append = FactoryRunHistoryAppend::default();
  if frozen.is_none() {
    append.flow.data.inputs.push(input);
  }
  append.flow.data.profile_selections.push(selection);
  Ok(Some(append))
}

async fn current_choices(
  snapshot: &FactoryRunSnapshot,
  catalog: Option<&dyn octacity_server_store::FactoryConfigurationStore>,
) -> Result<Option<octacity_server_factory::FactoryConfigurationChoices>, ApplicationError> {
  let Some(catalog) = catalog else {
    return Ok(None);
  };
  Ok(
    match catalog
      .factory_configuration_availability(snapshot.work.subject().project_id())
      .await?
    {
      octacity_server_store::FactoryConfigurationAvailability::Available(choices) => Some(choices),
      octacity_server_store::FactoryConfigurationAvailability::Disabled => None,
    },
  )
}

fn next_generation(snapshot: &FactoryRunSnapshot, target: &FactoryNodeTarget) -> Result<u32, ApplicationError> {
  u32::try_from(
    snapshot
      .flow
      .attempts
      .iter()
      .filter(|attempt| {
        attempt.flow_run_id() == target.flow_run_id
          && attempt.workflow_cycle_id() == target.cycle_id
          && attempt.node_key() == &target.node
      })
      .count(),
  )
  .map_err(|_| ApplicationError::invalid())
}
pub(crate) fn frozen_input<'a>(
  snapshot: &'a FactoryRunSnapshot,
  target: &FactoryNodeTarget,
) -> Result<Option<&'a FlowNodeInput>, ApplicationError> {
  let generation = next_generation(snapshot, target)?;
  Ok(snapshot.flow.data.inputs.iter().find(|input| {
    input.flow_run_id() == target.flow_run_id
      && input.cycle_id() == target.cycle_id
      && input.node() == &target.node
      && input.generation() == generation
  }))
}
