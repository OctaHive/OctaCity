use std::collections::{BTreeMap, BTreeSet};

use crate::{
  AdmittedFlow, BudgetUsage, FactoryError, FlowNodeKind, FlowRun, FlowRunId, NodeAttempt, NodeAttemptCompletion,
  NodeAttemptId, WorkflowCycle, WorkflowCycleId, WorkflowCycleNumber,
};

/// Borrowed authoritative Flow runtime rows validated as one aggregate.
#[derive(Clone, Copy)]
pub struct FlowRuntimeHistory<'a> {
  /// Confirmed technical stops independent of semantic completions.
  pub stops: &'a [crate::FlowNodeStop],
  /// Root and nested Flow Runs.
  pub flow_runs: &'a [FlowRun],
  /// Initial and subsequent append-only workflow cycles.
  pub cycles: &'a [WorkflowCycle],
  /// Append-only node attempts.
  pub attempts: &'a [NodeAttempt],
  /// At most one terminal observation for each attempt.
  pub completions: &'a [NodeAttemptCompletion],
}

impl<'a> FlowRuntimeHistory<'a> {
  /// Validates journal consumption, including verified overruns from stopped
  /// operations. This is retention authority only: all new dispatches must
  /// still reserve against the full, unmodified measured consumption.
  pub fn validate_budget_usage(self, admitted: &AdmittedFlow, usage: BudgetUsage) -> Result<(), FactoryError> {
    let overrun = sum_usage(self.stop_overruns_by_flow()?.values().copied())?;
    usage
      .checked_sub(overrun)
      .ok_or_else(|| invalid("stopped usage accounting"))?
      .validate(admitted.limits().budget())
      .map(|_| ())
  }

  fn stop_overruns_by_flow(self) -> Result<BTreeMap<FlowRunId, BudgetUsage>, FactoryError> {
    let mut overruns = BTreeMap::new();
    for receipt in self.stops {
      let attempt = self
        .attempts
        .iter()
        .find(|a| a.id() == receipt.node_attempt_id())
        .ok_or_else(|| invalid("Node stop attempt"))?;
      let spent = receipt.usage();
      let limit = attempt.budget();
      add_flow_usage(
        &mut overruns,
        attempt.flow_run_id(),
        BudgetUsage {
          tokens: spent.tokens.saturating_sub(limit.max_tokens()),
          elapsed_millis: spent.elapsed_millis.saturating_sub(limit.max_elapsed_millis()),
          cost_micro_units: spent
            .cost_micro_units
            .known_micro_units()
            .saturating_sub(limit.max_cost_micro_units())
            .into(),
          ..BudgetUsage::default()
        },
      )?;
    }
    Ok(overruns)
  }
  /// Tests active authority through the owning cycle and every enclosing call.
  /// Superseded rows remain readable for audit, but cannot dispatch or accept work.
  #[must_use]
  pub fn is_current_cycle(self, id: WorkflowCycleId) -> bool {
    let mut current = id;
    for _ in 0..=self.flow_runs.len() {
      let Some(cycle) = self.cycles.iter().find(|row| row.id() == current) else {
        return false;
      };
      if self
        .cycles
        .iter()
        .filter(|row| row.flow_run_id() == cycle.flow_run_id())
        .max_by_key(|row| row.number())
        != Some(cycle)
      {
        return false;
      }
      let Some(flow) = self.flow_runs.iter().find(|row| row.id() == cycle.flow_run_id()) else {
        return false;
      };
      let Some(parent) = flow.parent() else {
        return true;
      };
      let Some(call) = self
        .attempts
        .iter()
        .find(|row| row.id() == parent.node_attempt_id() && row.flow_run_id() == parent.flow_run_id())
      else {
        return false;
      };
      current = call.workflow_cycle_id();
    }
    false
  }
  /// Counts unfinished executable attempts. Suspended subflow call frames are
  /// control records; their child attempts own executable WIP.
  #[must_use]
  pub fn active_node_count(self) -> usize {
    self.active_nodes().count()
  }

  /// Iterates executable attempts that have neither a result nor a confirmed stop.
  pub fn active_nodes(self) -> impl Iterator<Item = &'a NodeAttempt> {
    let completed = self
      .completions
      .iter()
      .map(NodeAttemptCompletion::node_attempt_id)
      .chain(self.stops.iter().map(crate::FlowNodeStop::node_attempt_id))
      .collect::<BTreeSet<_>>();
    self
      .attempts
      .iter()
      .filter(move |attempt| attempt.node_kind() != FlowNodeKind::SubflowCall && !completed.contains(&attempt.id()))
  }

  /// Sums measured consumption once per retained result or technical stop.
  pub(super) fn observed_usage_by_flow(self) -> Result<BTreeMap<FlowRunId, BudgetUsage>, FactoryError> {
    let attempts = self
      .attempts
      .iter()
      .map(|row| (row.id(), row.flow_run_id()))
      .collect::<BTreeMap<_, _>>();
    let mut usage = BTreeMap::<FlowRunId, BudgetUsage>::new();
    for completion in self.completions {
      add_flow_usage(&mut usage, completion.flow_run_id(), completion.usage())?;
    }
    for stop in self.stops {
      let flow = attempts
        .get(&stop.node_attempt_id())
        .ok_or_else(|| invalid("Node stop attempt"))?;
      add_flow_usage(&mut usage, *flow, stop.usage())?;
    }
    Ok(usage)
  }
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
      if history
        .flow_runs
        .iter()
        .filter(|row| row.parent() == Some(parent))
        .count()
        != 1
      {
        return Err(invalid("unique nested Flow call"));
      }
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

  validate_cycles(admitted, &validated, &flow_runs, &cycles, history)?;
  validate_attempts(&validated, &flow_runs, &cycles, &attempts, history)?;
  validate_completions(&validated, &flow_runs, &attempts, history.completions)?;

  let mut stopped = BTreeSet::new();
  for receipt in history.stops {
    let attempt = attempts
      .get(&receipt.node_attempt_id())
      .ok_or_else(|| invalid("Node stop owner"))?;
    if !stopped.insert(attempt.id())
      || history
        .completions
        .iter()
        .any(|row| row.node_attempt_id() == attempt.id())
    {
      return Err(invalid("unique Node stop"));
    }
    attempt.verify_observer(receipt.ownership(), receipt.observed_at())?;
    if receipt.usage().attempts != 0 || receipt.usage().output_bytes > attempt.budget().max_output_bytes() {
      return Err(invalid("Node stop usage"));
    }
  }
  let active = history.active_node_count();
  if active > usize::from(limits.max_wip()) {
    return Err(invalid("Flow aggregate WIP"));
  }

  let usage = history.observed_usage_by_flow()?;
  validate_flow_budgets(admitted, history, &usage)
}

pub(super) fn validate_flow_budgets(
  admitted: &AdmittedFlow,
  history: FlowRuntimeHistory<'_>,
  usage: &BTreeMap<FlowRunId, BudgetUsage>,
) -> Result<(), FactoryError> {
  history.validate_budget_usage(admitted, sum_usage(usage.values().copied())?)?;
  let overruns = history.stop_overruns_by_flow()?;
  for flow_run in history.flow_runs {
    let definition = admitted
      .closure()
      .definition(flow_run.definition())
      .ok_or_else(|| invalid("Flow Run definition"))?;
    usage
      .get(&flow_run.id())
      .copied()
      .unwrap_or_default()
      .checked_sub(overruns.get(&flow_run.id()).copied().unwrap_or_default())
      .ok_or_else(|| invalid("stopped Flow usage accounting"))?
      .validate(definition.execution().budget())?;
  }
  Ok(())
}

fn validate_cycles(
  admitted: &AdmittedFlow,
  validated: &crate::ValidatedFlowDefinitionClosure,
  flow_runs: &BTreeMap<FlowRunId, &FlowRun>,
  cycles: &BTreeMap<WorkflowCycleId, &WorkflowCycle>,
  history: FlowRuntimeHistory<'_>,
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
      || initial[0].source_attempt_id().is_some()
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
          || cycles
            .values()
            .filter(|row| row.predecessor() == Some(predecessor))
            .count()
            != 1
        {
          return Err(invalid("workflow cycle sequence"));
        }
        crate::FlowInterpreter::new(validated).cycle_entry(flow_run, cycle, history)?;
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
  history: FlowRuntimeHistory<'_>,
) -> Result<(), FactoryError> {
  let ordered_attempts = history.attempts;
  let completions = history.completions;
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
    attempt.validate_execution_window()?;
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
      if let Some(previous_id) = attempt.retry_of() {
        let previous = attempts
          .get(&previous_id)
          .ok_or_else(|| invalid("Node retry predecessor"))?;
        let stopped = history
          .stops
          .iter()
          .find(|row| row.node_attempt_id() == previous_id)
          .filter(|row| row.reason() == crate::FlowNodeStopReason::ExecutionFailed)
          .ok_or_else(|| invalid("Node retry confirmed failure"))?;
        if (**attempt).clone().with_retry_of(previous)? != **attempt
          || attempt.started_at() < stopped.observed_at()
          || ordered_attempts
            .iter()
            .filter(|row| row.retry_of() == Some(previous_id))
            .count()
            != 1
        {
          return Err(invalid("Node retry lineage"));
        }
        let maximum = definition
          .node(attempt.node_key())
          .ok_or_else(|| invalid("Node retry declaration"))?
          .max_execution_retries();
        let mut current = *attempt;
        let mut count = 0_u16;
        while let Some(parent) = current.retry_of() {
          count = count.checked_add(1).ok_or_else(|| invalid("Node retry bound"))?;
          if count > maximum {
            return Err(invalid("Node retry bound"));
          }
          current = attempts.get(&parent).ok_or_else(|| invalid("Node retry predecessor"))?;
        }
        continue;
      }
      let cycle = cycles
        .get(&attempt.workflow_cycle_id())
        .ok_or_else(|| invalid("Node cycle"))?;
      if !run_attempts[..index]
        .iter()
        .any(|previous| previous.workflow_cycle_id() == cycle.id())
      {
        let entry = crate::FlowInterpreter::new(validated).cycle_entry(flow_run, cycle, history)?;
        if attempt.node_key() != &entry {
          return Err(invalid("Flow entry attempt"));
        }
        continue;
      }
      let declared = run_attempts[..index].iter().any(|predecessor| {
        if predecessor.workflow_cycle_id() != cycle.id() {
          return false;
        }
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
      || !attempt.accepts_completion_owner(&crate::FactoryClaimOwnership::new(
        completion.owner().clone(),
        completion.claim(),
      ))
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

pub(super) fn sum_usage(mut usage: impl Iterator<Item = BudgetUsage>) -> Result<BudgetUsage, FactoryError> {
  usage.try_fold(BudgetUsage::default(), |left, right| {
    left.checked_add(right).ok_or_else(|| invalid("Flow budget usage"))
  })
}

fn invalid(field: &'static str) -> FactoryError {
  FactoryError::InvalidConfiguration { field }
}

fn add_flow_usage(
  usage: &mut BTreeMap<FlowRunId, BudgetUsage>,
  flow: FlowRunId,
  observed: BudgetUsage,
) -> Result<(), FactoryError> {
  let total = usage.entry(flow).or_default();
  *total = total
    .checked_add(observed)
    .ok_or_else(|| invalid("Flow budget usage"))?;
  Ok(())
}
