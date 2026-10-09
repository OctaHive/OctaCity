use std::collections::{BTreeMap, BTreeSet};

use crate::{
  AdmittedFlow, BudgetUsage, FactoryError, FlowNodeKind, FlowRun, FlowRunId, NodeAttempt, NodeAttemptCompletion,
  NodeAttemptId, WorkflowCycle, WorkflowCycleId, WorkflowCycleNumber,
};

/// Borrowed authoritative Flow runtime rows validated as one aggregate.
#[derive(Clone, Copy)]
pub struct FlowRuntimeHistory<'a> {
  /// Root and nested Flow Runs.
  pub flow_runs: &'a [FlowRun],
  /// Initial and subsequent append-only workflow cycles.
  pub cycles: &'a [WorkflowCycle],
  /// Append-only node attempts.
  pub attempts: &'a [NodeAttempt],
  /// At most one terminal observation for each attempt.
  pub completions: &'a [NodeAttemptCompletion],
}

/// Validates the complete Flow runtime aggregate against one admitted closure.
///
/// This is the single structural validation seam used before a transition is
/// committed and after either store backend reconstructs a snapshot. Current
/// claim/fence checks remain transition concerns; immutable historical claims
/// are verified at their recorded observation times here.
pub fn validate_flow_runtime_history(
  admitted: &AdmittedFlow,
  history: FlowRuntimeHistory<'_>,
) -> Result<(), FactoryError> {
  let validated = admitted.validated()?;
  let limits = validated.limits();
  let flow_runs = unique_by(history.flow_runs, FlowRun::id, "Flow Run identity")?;
  let cycles = unique_by(history.cycles, WorkflowCycle::id, "workflow cycle identity")?;
  let attempts = unique_by(history.attempts, NodeAttempt::id, "Node Attempt identity")?;

  if flow_runs.get(&admitted.root_run().id()) != Some(&admitted.root_run())
    || flow_runs
      .values()
      .filter(|flow_run| flow_run.parent().is_none())
      .count()
      != 1
  {
    return Err(invalid("Flow runtime root"));
  }

  for flow_run in flow_runs.values() {
    let definition = validated
      .closure()
      .definition(flow_run.definition())
      .ok_or_else(|| invalid("Flow Run definition"))?;
    if flow_run.factory_run_id() != admitted.root_run().factory_run_id() {
      return Err(invalid("Flow Run Factory Run"));
    }
    if let Some(parent) = flow_run.parent() {
      let parent_run = flow_runs
        .get(&parent.flow_run_id())
        .ok_or_else(|| invalid("nested Flow parent Run"))?;
      let parent_attempt = attempts
        .get(&parent.node_attempt_id())
        .ok_or_else(|| invalid("nested Flow parent attempt"))?;
      let nested = validated
        .closure()
        .definition(parent_run.definition())
        .and_then(|parent_definition| parent_definition.node(parent_attempt.node_key()))
        .filter(|node| node.kind() == FlowNodeKind::SubflowCall)
        .and_then(crate::FlowNodeDefinition::subflow_definition);
      if parent_attempt.flow_run_id() != parent_run.id() || nested != Some(definition.reference()) {
        return Err(invalid("nested Flow definition"));
      }
    }
  }

  validate_cycles(admitted, &flow_runs, &cycles)?;
  validate_attempts(
    &validated,
    &flow_runs,
    &cycles,
    &attempts,
    history.attempts,
    history.completions,
  )?;
  validate_completions(&validated, &flow_runs, &attempts, history.completions)?;

  let completed = history
    .completions
    .iter()
    .map(NodeAttemptCompletion::node_attempt_id)
    .collect::<BTreeSet<_>>();
  let active = history
    .attempts
    .iter()
    .filter(|attempt| attempt.stage_projection_id().is_none() && !completed.contains(&attempt.id()))
    .count();
  if active > usize::from(limits.max_wip()) {
    return Err(invalid("Flow aggregate WIP"));
  }

  let aggregate_usage = sum_usage(history.completions.iter().map(NodeAttemptCompletion::usage))?;
  aggregate_usage.validate(limits.budget())?;
  for flow_run in flow_runs.values() {
    let definition = validated
      .closure()
      .definition(flow_run.definition())
      .ok_or_else(|| invalid("Flow Run definition"))?;
    let usage = sum_usage(
      history
        .completions
        .iter()
        .filter(|completion| completion.flow_run_id() == flow_run.id())
        .map(NodeAttemptCompletion::usage),
    )?;
    usage.validate(definition.execution().budget())?;
  }
  Ok(())
}

fn validate_cycles(
  admitted: &AdmittedFlow,
  flow_runs: &BTreeMap<FlowRunId, &FlowRun>,
  cycles: &BTreeMap<WorkflowCycleId, &WorkflowCycle>,
) -> Result<(), FactoryError> {
  if cycles.get(&admitted.initial_cycle().id()) != Some(&admitted.initial_cycle()) {
    return Err(invalid("initial workflow cycle"));
  }
  for flow_run in flow_runs.values() {
    let run_cycles = cycles
      .values()
      .filter(|cycle| cycle.flow_run_id() == flow_run.id())
      .copied()
      .collect::<Vec<_>>();
    let initial = run_cycles
      .iter()
      .filter(|cycle| cycle.predecessor().is_none())
      .copied()
      .collect::<Vec<_>>();
    if initial.len() != 1
      || initial[0].number() != WorkflowCycleNumber::INITIAL
      || initial[0].id().as_uuid() != flow_run.id().as_uuid()
    {
      return Err(invalid("Flow Run initial cycle"));
    }
    for cycle in run_cycles {
      if let Some(predecessor) = cycle.predecessor() {
        let previous = cycles
          .get(&predecessor)
          .ok_or_else(|| invalid("workflow cycle predecessor"))?;
        if previous.flow_run_id() != flow_run.id()
          || previous.number().get().checked_add(1) != Some(cycle.number().get())
        {
          return Err(invalid("workflow cycle sequence"));
        }
      }
    }
  }
  Ok(())
}

fn validate_attempts(
  validated: &crate::ValidatedFlowDefinitionClosure,
  flow_runs: &BTreeMap<FlowRunId, &FlowRun>,
  cycles: &BTreeMap<WorkflowCycleId, &WorkflowCycle>,
  attempts: &BTreeMap<NodeAttemptId, &NodeAttempt>,
  ordered_attempts: &[NodeAttempt],
  completions: &[NodeAttemptCompletion],
) -> Result<(), FactoryError> {
  for attempt in attempts.values() {
    let flow_run = flow_runs
      .get(&attempt.flow_run_id())
      .ok_or_else(|| invalid("Node Attempt Flow Run"))?;
    let cycle = cycles
      .get(&attempt.workflow_cycle_id())
      .ok_or_else(|| invalid("Node Attempt workflow cycle"))?;
    let node = validated
      .closure()
      .definition(flow_run.definition())
      .and_then(|definition| definition.node(attempt.node_key()))
      .ok_or_else(|| invalid("Node Attempt definition"))?;
    if attempt.factory_run_id() != flow_run.factory_run_id() || cycle.flow_run_id() != flow_run.id() {
      return Err(invalid("Node Attempt ownership"));
    }
    if attempt.node_kind() != node.kind() || attempt.budget() != node.budget() {
      return Err(invalid("Node Attempt declaration"));
    }
    if !attempt.execution().is_compatible(attempt.node_kind()) {
      return Err(invalid("Node Attempt executor"));
    }
    if attempt.deadline() <= attempt.claim().claimed_at() || attempt.deadline() > attempt.claim().expires_at() {
      return Err(invalid("Node Attempt deadline"));
    }
  }

  let mut numbers = BTreeMap::<FlowRunId, Vec<u64>>::new();
  for attempt in ordered_attempts {
    numbers
      .entry(attempt.flow_run_id())
      .or_default()
      .push(attempt.number().get());
  }
  for values in numbers.values_mut() {
    values.sort_unstable();
    if values
      .iter()
      .copied()
      .enumerate()
      .any(|(index, actual)| u64::try_from(index).ok().and_then(|value| value.checked_add(1)) != Some(actual))
    {
      return Err(invalid("Node Attempt sequence"));
    }
  }
  for flow_run in flow_runs.values() {
    let definition = validated
      .closure()
      .definition(flow_run.definition())
      .ok_or_else(|| invalid("Flow Run definition"))?;
    let mut run_attempts = ordered_attempts
      .iter()
      .filter(|attempt| attempt.flow_run_id() == flow_run.id())
      .collect::<Vec<_>>();
    run_attempts.sort_by_key(|attempt| attempt.number());
    for (index, attempt) in run_attempts.iter().enumerate() {
      if index == 0 {
        if attempt.node_key() != definition.entry() {
          return Err(invalid("Flow entry attempt"));
        }
        continue;
      }
      // Fixed-stage attempts are a lossless compatibility projection. Their
      // readiness and WIP are validated by the existing lifecycle aggregate;
      // that model did not persist a generic typed outcome for every stage.
      if attempt.stage_projection_id().is_some() {
        continue;
      }
      let declared = run_attempts[..index].iter().any(|predecessor| {
        completions
          .iter()
          .find(|completion| completion.node_attempt_id() == predecessor.id())
          .is_some_and(|completion| {
            crate::FlowInterpreter::declared_successor_keys(definition, predecessor.node_key(), completion.outcome())
              .contains(attempt.node_key())
          })
      });
      if !declared {
        return Err(invalid("Flow Node Attempt route"));
      }
    }
  }
  Ok(())
}

fn validate_completions(
  validated: &crate::ValidatedFlowDefinitionClosure,
  flow_runs: &BTreeMap<FlowRunId, &FlowRun>,
  attempts: &BTreeMap<NodeAttemptId, &NodeAttempt>,
  completions: &[NodeAttemptCompletion],
) -> Result<(), FactoryError> {
  let mut completed = BTreeSet::new();
  for completion in completions {
    let attempt = attempts
      .get(&completion.node_attempt_id())
      .ok_or_else(|| invalid("Node Attempt completion attempt"))?;
    let definition = flow_runs
      .get(&attempt.flow_run_id())
      .and_then(|flow_run| validated.closure().definition(flow_run.definition()))
      .ok_or_else(|| invalid("Node Attempt completion definition"))?;
    if !completed.insert(completion.node_attempt_id())
      || attempt.flow_run_id() != completion.flow_run_id()
      || attempt.workflow_cycle_id() != completion.workflow_cycle_id()
      || attempt.node_key() != completion.node_key()
      || attempt.owner() != completion.owner()
      || attempt.claim() != completion.claim()
      || completion
        .claim()
        .verify_fence(completion.claim().fence(), completion.observed_at())
        .is_err()
      || completion.usage().validate(attempt.budget()).is_err()
      || definition
        .node(attempt.node_key())
        .and_then(|node| node.outcome(completion.outcome()))
        .is_none_or(|outcome| outcome.schema() != completion.output_schema())
    {
      return Err(invalid("Node Attempt completion"));
    }
  }
  Ok(())
}

fn unique_by<'a, T, K: Ord + Copy>(
  records: &'a [T],
  key: impl Fn(&T) -> K,
  field: &'static str,
) -> Result<BTreeMap<K, &'a T>, FactoryError> {
  let mut values = BTreeMap::new();
  for record in records {
    if values.insert(key(record), record).is_some() {
      return Err(invalid(field));
    }
  }
  Ok(values)
}

fn sum_usage(mut usage: impl Iterator<Item = BudgetUsage>) -> Result<BudgetUsage, FactoryError> {
  usage.try_fold(BudgetUsage::default(), |left, right| {
    Ok(BudgetUsage {
      attempts: left
        .attempts
        .checked_add(right.attempts)
        .ok_or_else(|| invalid("Flow budget usage"))?,
      elapsed_millis: left
        .elapsed_millis
        .checked_add(right.elapsed_millis)
        .ok_or_else(|| invalid("Flow budget usage"))?,
      tokens: left
        .tokens
        .checked_add(right.tokens)
        .ok_or_else(|| invalid("Flow budget usage"))?,
      cost_micro_units: left
        .cost_micro_units
        .checked_add(right.cost_micro_units)
        .ok_or_else(|| invalid("Flow budget usage"))?,
      output_bytes: left
        .output_bytes
        .checked_add(right.output_bytes)
        .ok_or_else(|| invalid("Flow budget usage"))?,
    })
  })
}

fn invalid(field: &'static str) -> FactoryError {
  FactoryError::InvalidConfiguration { field }
}
