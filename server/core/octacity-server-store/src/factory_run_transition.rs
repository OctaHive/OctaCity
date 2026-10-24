use std::collections::{BTreeMap, BTreeSet};

use octacity_server_factory::{
  AdmittedFlow, BudgetUsage, FactoryRun, FlowRun, FlowRunId, FlowRuntimeHistory, NodeAttempt, NodeAttemptCompletion,
  NodeAttemptId, WorkflowCycle, WorkflowCycleId, validate_flow_runtime_history, validate_lifecycle_progress,
};

use crate::{
  CommitFactoryRunTransition, FactoryAuditFact, FactoryBudgetRecord, FactoryLifecycleCheckpoint, FactoryOutboxState,
  FactoryRunClaimRecord, FactoryRunCurrentProjection, MAX_FACTORY_RUN_SNAPSHOT_RECORDS,
  MAX_FACTORY_TRANSITION_OUTBOX_RECORDS, MAX_FACTORY_TRANSITION_RECORDS, StoreError, StoreInputError, StoreOperation,
};

/// Existing authoritative rows required to validate one Factory transition.
///
/// Adapters build this bounded baseline under the same aggregate lock used to
/// commit the transition, so every backend applies exactly the same rules.
pub struct FactoryTransitionBaseline<'a> {
  /// Configuration-wide usage observed under the capacity lock protecting this append.
  pub configuration_wip_usage: octacity_server_factory::FactoryWipUsage,
  /// Exact retained queue selection granting the current claim, when present.
  pub pool_selection: Option<&'a crate::PhasePoolSelection>,
  /// Current aggregate projection.
  pub run: &'a FactoryRun,
  /// Current aggregate budget observation.
  pub budget: &'a FactoryBudgetRecord,
  /// Original accepted cancellation time, if cancellation was requested.
  pub cancellation_requested_at: Option<octacity_server_domain::Timestamp>,
  /// Canonical accepted retry intents keyed by the exact failed attempt.
  pub retry_requests: &'a BTreeMap<NodeAttemptId, octacity_server_domain::Timestamp>,
  /// Retained audited operator intents used to authorize manual selection consumption.
  pub controls: &'a [crate::FactoryRunControlRecord],
  /// Current lifecycle checkpoint.
  pub lifecycle: &'a FactoryLifecycleCheckpoint,
  /// Current immutable-row pointers.
  pub current: &'a FactoryRunCurrentProjection,
  /// Current fenced ownership row.
  pub claim: &'a FactoryRunClaimRecord,
  /// Exact immutable Flow closure admitted with the Run.
  pub admitted_flow: &'a AdmittedFlow,
  /// Immutable admitted Work for typed intake validation.
  pub work: &'a octacity_server_factory::WorkEnvelope,
  /// Existing common node input/result journal.
  pub data: &'a octacity_server_factory::FlowDataHistory,
  /// Existing generic Node Attempts keyed by immutable identity.
  pub node_attempts: &'a BTreeMap<NodeAttemptId, NodeAttempt>,
  /// Existing generic Node Attempt completions keyed by attempt identity.
  pub node_completions: &'a BTreeMap<NodeAttemptId, NodeAttemptCompletion>,
  /// Existing Flow Runs keyed by immutable identity.
  pub flow_runs: &'a BTreeMap<FlowRunId, FlowRun>,
  /// Existing Workflow Cycles keyed by immutable identity.
  pub workflow_cycles: &'a BTreeMap<WorkflowCycleId, WorkflowCycle>,
  /// Total immutable rows already retained in the bounded snapshot.
  pub record_count: usize,
}

/// Applies the canonical adapter-neutral validation for one fenced transition.
pub fn validate_factory_transition(
  baseline: &FactoryTransitionBaseline<'_>,
  request: &CommitFactoryRunTransition,
) -> Result<(), StoreError> {
  let invalid = || {
    StoreError::invalid(
      StoreOperation::CommitFactoryRunTransition,
      StoreInputError::InvalidFactoryRunTransition,
    )
  };

  if request.append.record_count() > MAX_FACTORY_TRANSITION_RECORDS
    || request.outbox.len() > MAX_FACTORY_TRANSITION_OUTBOX_RECORDS
    || request
      .outbox
      .iter()
      .map(|record| record.id)
      .collect::<BTreeSet<_>>()
      .len()
      != request.outbox.len()
  {
    return Err(invalid());
  }
  let signal_usage = request
    .append
    .flow
    .data
    .profile_selections
    .iter()
    .map(|selection| {
      selection
        .additional_signal_usage(baseline.data.current_profile_selection(selection.input_digest()))
        .map_err(|_| invalid())
    })
    .collect::<Result<Vec<_>, _>>()?;
  let added = request
    .append
    .record_count()
    .checked_add(request.outbox.len())
    .and_then(|count| count.checked_add(3))
    .ok_or_else(invalid)?;
  if baseline
    .record_count
    .checked_add(added)
    .is_none_or(|count| count > MAX_FACTORY_RUN_SNAPSHOT_RECORDS)
  {
    return Err(invalid());
  }
  if request.expected_version != baseline.run.version() || request.claim_id != baseline.claim.id {
    return Err(StoreError::Conflict {
      entity: octacity_server_domain::EntityKind::FactoryRun,
    });
  }
  if request.owner != baseline.claim.owner
    || baseline
      .claim
      .claim
      .verify_fence(request.fence, request.committed_at)
      .is_err()
  {
    return Err(StoreError::Conflict {
      entity: octacity_server_domain::EntityKind::FactoryRun,
    });
  }
  let next_version = baseline
    .run
    .version()
    .get()
    .checked_add(1)
    .and_then(|value| octacity_server_factory::FactoryRunVersion::new(value).ok())
    .ok_or(StoreError::Unavailable)?;
  let flow_runs = baseline
    .flow_runs
    .values()
    .cloned()
    .chain(request.append.flow.runs.iter().cloned())
    .collect::<Vec<_>>();
  let workflow_cycles = baseline
    .workflow_cycles
    .values()
    .cloned()
    .chain(request.append.flow.cycles.iter().cloned())
    .collect::<Vec<_>>();
  let node_attempts = baseline
    .node_attempts
    .values()
    .cloned()
    .chain(request.append.flow.attempts.iter().cloned())
    .collect::<Vec<_>>();
  let node_completions = baseline
    .node_completions
    .values()
    .cloned()
    .chain(request.append.flow.completions.iter().cloned())
    .collect::<Vec<_>>();
  let stops = baseline
    .data
    .stops
    .iter()
    .cloned()
    .chain(request.append.flow.data.stops.iter().cloned())
    .collect::<Vec<_>>();
  let history = FlowRuntimeHistory {
    stops: &stops,
    flow_runs: &flow_runs,
    cycles: &workflow_cycles,
    attempts: &node_attempts,
    completions: &node_completions,
  };
  validate_node_journal(baseline, request, history)?;
  let flow_runtime_is_valid = validate_flow_runtime_history(baseline.admitted_flow, history).is_ok();
  let before_active = FlowRuntimeHistory {
    stops: &stops[..baseline.data.stops.len()],
    attempts: &node_attempts[..baseline.node_attempts.len()],
    completions: &node_completions[..baseline.node_completions.len()],
    ..history
  }
  .active_node_count();
  let after_active = history.active_node_count();
  if after_active > before_active {
    let additional = u32::try_from(after_active - before_active).map_err(|_| invalid())?;
    if baseline
      .configuration_wip_usage
      .active_nodes()
      .checked_add(additional)
      .is_none_or(|total| total > baseline.admitted_flow.wip_limits().max_active_nodes())
    {
      return Err(invalid());
    }
  }
  let appended_nodes_have_current_owner = request.append.flow.attempts.iter().all(|attempt| {
    attempt.owner() == &request.owner
      && attempt.claim() == baseline.claim.claim
      && attempt.started_at() <= request.committed_at
  });
  let appended_node_completions_have_current_owner = request.append.flow.completions.iter().all(|completion| {
    completion.owner() == &request.owner
      && completion.claim() == baseline.claim.claim
      && completion
        .claim()
        .verify_fence(request.fence, completion.observed_at())
        .is_ok()
  });
  let canonical_budget =
    FactoryBudgetRecord::new(request.run_id, next_version, request.budget.usage, request.committed_at);
  let canonical_lifecycle = FactoryLifecycleCheckpoint::new(
    request.run_id,
    next_version,
    request.lifecycle_checkpoint.progress.clone(),
    request.lifecycle_checkpoint.cancellation_requested,
    request.committed_at,
  );
  let canonical_audit = FactoryAuditFact::new(
    request.audit.run_id,
    request.audit.actor_kind,
    request.audit.actor_identity_digest,
    request.audit.operation.clone(),
    request.audit.request_identity_digest,
    request.audit.outcome.clone(),
    request.audit.recorded_at,
  );
  let outbox_is_valid = request.outbox.iter().all(|record| {
    record.subject == crate::FactoryOutboxSubject::Run(request.run_id)
      && record.recorded_at == request.committed_at
      && record.available_at >= request.committed_at
      && record.state == FactoryOutboxState::Pending
      && record.attempt == 0
      && record.is_canonical()
  });
  let appended_attempt_count = u32::try_from(request.append.flow.attempts.len()).ok();
  let attempts_are_covered = appended_attempt_count.is_some_and(|count| {
    baseline
      .budget
      .usage
      .attempts
      .checked_add(count)
      .is_some_and(|minimum| request.budget.usage.attempts >= minimum)
  });
  if request.next_run.id() != baseline.run.id()
    || request.next_run.work_id() != baseline.run.work_id()
    || request.next_run.configuration() != baseline.run.configuration()
    || request.next_run.subject() != baseline.run.subject()
    || request.next_run.version() != next_version
    || request.budget != canonical_budget
    || !budget_is_monotonic(baseline.budget.usage, request.budget.usage)
    || !attempts_are_covered
    || !completion_budget_is_covered(
      baseline.budget.usage,
      request.budget.usage,
      request
        .append
        .flow
        .completions
        .iter()
        .map(NodeAttemptCompletion::usage)
        .chain(signal_usage)
        .chain(
          request
            .append
            .flow
            .data
            .stops
            .iter()
            .map(octacity_server_factory::FlowNodeStop::usage),
        ),
    )
    || request
      .budget
      .usage
      .validate(baseline.admitted_flow.limits().budget())
      .is_err()
    || request.current.budget_id != request.budget.id
    || (baseline.lifecycle.cancellation_requested
      && (!request.outbox.is_empty()
        || request.append.record_count()
          != request.append.flow.data.stops.len() + request.append.flow.data.build_executions.len()))
    || request.lifecycle_checkpoint.cancellation_requested != baseline.lifecycle.cancellation_requested
    || request.lifecycle_checkpoint != canonical_lifecycle
    || validate_lifecycle_progress(request.next_run.state(), &request.lifecycle_checkpoint.progress).is_err()
    || !lifecycle_transition_is_valid(
      baseline,
      request,
      FlowRuntimeHistory {
        stops: &stops,
        flow_runs: &flow_runs,
        cycles: &workflow_cycles,
        attempts: &node_attempts,
        completions: &node_completions,
      },
    )
    || !flow_runtime_is_valid
    || !appended_nodes_have_current_owner
    || !appended_node_completions_have_current_owner
    || request.current.lifecycle_checkpoint_id != request.lifecycle_checkpoint.id
    || request.audit.run_id != request.run_id
    || request.audit.recorded_at != request.committed_at
    || request.audit != canonical_audit
    || !outbox_is_valid
  {
    return Err(invalid());
  }
  Ok(())
}

const fn budget_is_monotonic(current: BudgetUsage, next: BudgetUsage) -> bool {
  next.attempts >= current.attempts
    && next.elapsed_millis >= current.elapsed_millis
    && next.tokens >= current.tokens
    && next.cost_micro_units >= current.cost_micro_units
    && next.output_bytes >= current.output_bytes
}

fn completion_budget_is_covered(
  current: BudgetUsage,
  next: BudgetUsage,
  mut completions: impl Iterator<Item = BudgetUsage>,
) -> bool {
  // Attempt creation is covered separately; completions contribute measured resources.
  let totals = completions.try_fold(current, |usage, completion| {
    usage.checked_add(BudgetUsage {
      attempts: 0,
      ..completion
    })
  });
  totals.is_some_and(|total| {
    next.elapsed_millis >= total.elapsed_millis
      && next.tokens >= total.tokens
      && next.cost_micro_units >= total.cost_micro_units
      && next.output_bytes >= total.output_bytes
  })
}

// A terminal name never selects behavior in code: the pinned root graph supplies it.
fn lifecycle_transition_is_valid(
  baseline: &FactoryTransitionBaseline<'_>,
  request: &CommitFactoryRunTransition,
  history: FlowRuntimeHistory<'_>,
) -> bool {
  use octacity_server_factory::{FactoryLifecycleProgress, FactoryRunState, FlowInterpreter, FlowProgress};
  if let FactoryLifecycleProgress::FlowCancelled { root } = &request.lifecycle_checkpoint.progress {
    return !baseline.run.state().is_terminal()
      && *root == baseline.admitted_flow.root_run().id()
      && baseline.lifecycle.cancellation_requested
      && request.lifecycle_checkpoint.cancellation_requested
      && request.next_run.state() == FactoryRunState::Cancelled
      && request.append.record_count() == 0
      && request.outbox.is_empty()
      && history.attempts.iter().all(|attempt| {
        history
          .completions
          .iter()
          .any(|row| row.node_attempt_id() == attempt.id())
          || baseline
            .data
            .stops
            .iter()
            .any(|row| row.node_attempt_id() == attempt.id())
          || matches!(
            attempt.node_kind(),
            octacity_server_factory::FlowNodeKind::HumanGate
              | octacity_server_factory::FlowNodeKind::SubflowCall
              | octacity_server_factory::FlowNodeKind::DeterministicGate
          )
      });
  }
  if let FactoryLifecycleProgress::FlowRunning { root } = &request.lifecycle_checkpoint.progress {
    return !baseline.run.state().is_terminal()
      && *root == baseline.admitted_flow.root_run().id()
      && request.next_run.state() == FactoryRunState::Running;
  }
  if let FactoryLifecycleProgress::FlowResolved { root, terminal } = &request.lifecycle_checkpoint.progress {
    if baseline.run.state().is_terminal()
      || *root != baseline.admitted_flow.root_run().id()
      || baseline.lifecycle.cancellation_requested
      || request.next_run.state() != FactoryRunState::Completed
    {
      return false;
    }
    let Some(cycle) = history
      .cycles
      .iter()
      .filter(|row| row.flow_run_id() == *root)
      .max_by_key(|row| row.number())
    else {
      return false;
    };
    let Ok(validated) = baseline.admitted_flow.validated() else {
      return false;
    };
    return matches!(FlowInterpreter::new(&validated).progress(baseline.admitted_flow.root_run(), cycle, history),
      Ok(FlowProgress::Terminal { key, .. }) if &key == terminal);
  }
  false
}

/// Validates the common journal for every admitted Flow under the current fence.
fn validate_node_journal(
  baseline: &FactoryTransitionBaseline<'_>,
  request: &CommitFactoryRunTransition,
  history: FlowRuntimeHistory<'_>,
) -> Result<(), StoreError> {
  let invalid = || {
    StoreError::invalid(
      StoreOperation::CommitFactoryRunTransition,
      StoreInputError::InvalidFactoryRunTransition,
    )
  };
  let appended = &request.append.flow;
  if appended
    .attempts
    .iter()
    .any(|row| !history.is_current_cycle(row.workflow_cycle_id()))
    || appended
      .completions
      .iter()
      .any(|row| !history.is_current_cycle(row.workflow_cycle_id()))
    || appended
      .data
      .inputs
      .iter()
      .any(|row| !history.is_current_cycle(row.cycle_id()))
    || appended
      .data
      .records
      .iter()
      .any(|row| !history.is_current_cycle(row.input().cycle_id()))
    || appended
      .data
      .build_intents
      .iter()
      .any(|row| !history.is_current_cycle(row.node().workflow_cycle_id()))
    || appended
      .data
      .stops
      .iter()
      .map(|row| row.node_attempt_id())
      .chain(appended.data.build_executions.iter().map(|row| row.node_attempt_id()))
      .any(|id| {
        history
          .attempts
          .iter()
          .find(|row| row.id() == id)
          .is_none_or(|row| !history.is_current_cycle(row.workflow_cycle_id()))
      })
  {
    return Err(invalid());
  }
  let data = octacity_server_factory::FlowDataHistory {
    profile_selections: baseline
      .data
      .profile_selections
      .iter()
      .chain(&request.append.flow.data.profile_selections)
      .cloned()
      .collect(),
    stops: baseline
      .data
      .stops
      .iter()
      .chain(&request.append.flow.data.stops)
      .cloned()
      .collect(),
    build_executions: baseline
      .data
      .build_executions
      .iter()
      .chain(&request.append.flow.data.build_executions)
      .cloned()
      .collect(),
    build_intents: baseline
      .data
      .build_intents
      .iter()
      .chain(&request.append.flow.data.build_intents)
      .cloned()
      .collect(),
    incoming: baseline
      .data
      .incoming
      .iter()
      .chain(&request.append.flow.data.incoming)
      .cloned()
      .collect(),
    inputs: baseline
      .data
      .inputs
      .iter()
      .chain(&request.append.flow.data.inputs)
      .cloned()
      .collect(),
    records: baseline
      .data
      .records
      .iter()
      .chain(&request.append.flow.data.records)
      .cloned()
      .collect(),
  };
  for attempt in &request.append.flow.attempts {
    if let Some(previous) = attempt.retry_of()
      && baseline
        .retry_requests
        .get(&previous)
        .is_none_or(|at| *at > attempt.started_at())
    {
      return Err(invalid());
    }
    let node = baseline
      .admitted_flow
      .closure()
      .definition(
        history
          .flow_runs
          .iter()
          .find(|flow| flow.id() == attempt.flow_run_id())
          .ok_or_else(invalid)?
          .definition(),
      )
      .and_then(|definition| definition.node(attempt.node_key()))
      .ok_or_else(invalid)?;
    if let Some(pool) = node.phase_pool() {
      let settings = baseline.admitted_flow.pool_settings().get(pool).ok_or_else(invalid)?;
      let selection = baseline.pool_selection.ok_or_else(invalid)?;
      if selection.entry.policy_digest != settings.policy.digest() {
        return Err(invalid());
      }
      let selected = &selection.entry.input;
      // A current store-validated owner may resume an expired original lease,
      // retaining the same immutable selection and capacity reservation.
      if (selection.run_claim() != *baseline.claim && baseline.claim.claim.claimed_at() < selection.claim.expires_at())
        || selected.run_id != request.run_id
        || selected.flow_run_id != attempt.flow_run_id()
        || selected.cycle_id != attempt.workflow_cycle_id()
        || &selected.node != attempt.node_key()
        || selected.phase_input_digest != attempt.input_digest()
        || !data
          .inputs
          .iter()
          .any(|input| input.digest().ok() == Some(attempt.input_digest()) && input.generation() == selected.generation)
      {
        return Err(invalid());
      }
    }
  }
  for receipt in &request.append.flow.data.stops {
    let valid_reason = match receipt.reason() {
      octacity_server_factory::FlowNodeStopReason::Cancelled { requested_at } => {
        baseline.lifecycle.cancellation_requested
          && request.lifecycle_checkpoint.cancellation_requested
          && Some(requested_at) == baseline.cancellation_requested_at
      }
      octacity_server_factory::FlowNodeStopReason::ExecutionFailed => !baseline.lifecycle.cancellation_requested,
    };
    if !valid_reason
      || receipt.observed_at() != request.committed_at
      || receipt.ownership().owner() != &request.owner
      || receipt.ownership().claim() != baseline.claim.claim
    {
      return Err(invalid());
    }
  }
  if request
    .append
    .flow
    .data
    .profile_selections
    .iter()
    .any(|selection| selection.recorded_at() != request.committed_at)
  {
    return Err(invalid());
  }
  for selection in &request.append.flow.data.profile_selections {
    let input = data
      .inputs
      .iter()
      .find(|input| input.digest().ok() == Some(selection.input_digest()))
      .ok_or_else(invalid)?;
    let before = baseline.data.current_profile_selection(selection.input_digest());
    let usage = baseline
      .budget
      .usage
      .checked_sub(before.map_or(BudgetUsage::default(), |s| s.signal_usage()))
      .ok_or_else(invalid)?;
    octacity_server_factory::FlowBuildIntent::verify_available_budget(
      input,
      baseline.admitted_flow,
      history,
      &data,
      usage,
    )
    .map_err(|_| invalid())?;
    let latest = crate::latest_factory_profile_control(baseline.controls, selection.input_digest());
    if latest.is_some_and(|c| selection.manual_control() != Some(c.id))
      || baseline
        .data
        .build_intents
        .iter()
        .any(|intent| intent.input().digest().ok() == Some(selection.input_digest()))
    {
      return Err(invalid());
    }
  }
  crate::factory_run_control::validate_factory_profile_control_history(
    baseline.admitted_flow,
    &data,
    baseline.controls,
  )
  .map_err(|_| invalid())?;
  let pending_signal_usage = data.pending_profile_signal_usage().map_err(|_| invalid())?;
  request
    .budget
    .usage
    .checked_add(pending_signal_usage)
    .ok_or_else(invalid)?
    .validate(baseline.admitted_flow.limits().budget())
    .map_err(|_| invalid())?;
  for intent in &request.append.flow.data.build_intents {
    if let Some(control) =
      crate::latest_factory_profile_control(baseline.controls, intent.input().digest().map_err(|_| invalid())?)
      && data
        .current_profile_selection(intent.input().digest().map_err(|_| invalid())?)
        .is_none_or(|s| s.manual_control() != Some(control.id))
    {
      return Err(invalid());
    }

    intent
      .verify_reservation(baseline.admitted_flow, history, &data)
      .map_err(|_| invalid())?;
    let stage_reservation = intent.stage_reservation().map_err(|_| invalid())?;
    intent
      .usage_before()
      .checked_add(pending_signal_usage)
      .and_then(|usage| usage.checked_add(stage_reservation))
      .ok_or_else(invalid)?
      .validate(baseline.admitted_flow.limits().budget())
      .map_err(|_| invalid())?;
    if intent.priority() != i64::from(baseline.work.priority().get()) || intent.usage_before() != baseline.budget.usage
    {
      return Err(invalid());
    }
  }
  for input in &request.append.flow.data.inputs {
    if input.generation() as usize
      != baseline
        .node_attempts
        .values()
        .filter(|row| {
          row.flow_run_id() == input.flow_run_id()
            && row.workflow_cycle_id() == input.cycle_id()
            && row.node_key() == input.node()
        })
        .count()
    {
      return Err(invalid());
    }
    let flow = history
      .flow_runs
      .iter()
      .find(|flow| flow.id() == input.flow_run_id())
      .ok_or_else(invalid)?;
    let cycle = history
      .cycles
      .iter()
      .find(|cycle| cycle.id() == input.cycle_id())
      .ok_or_else(invalid)?;
    if baseline
      .admitted_flow
      .closure()
      .definition(input.definition())
      .and_then(|definition| definition.node(input.node()))
      .and_then(octacity_server_factory::FlowNodeDefinition::input_binding)
      .is_some()
    {
      input
        .verify_preparation(octacity_server_factory::FlowInputPreparation {
          work: baseline.work,
          admitted: baseline.admitted_flow,
          flow,
          cycle,
          node: input.node().clone(),
          schema: baseline
            .admitted_flow
            .data_schema(input.payload().schema())
            .ok_or_else(invalid)?,
          history,
          records: &data.records,
          inputs: &data.inputs,
          incoming: data.incoming.first(),
        })
        .map_err(|_| invalid())?;
    }
  }
  data
    .validate(baseline.work, baseline.admitted_flow, history)
    .map_err(|_| invalid())?;
  Ok(())
}

/// Resolves only new selected profiles from the validated journal to their admitted roles.
/// Adapters recheck those roles under the same fence as deletion before committing history.
pub fn factory_profile_selection_roles(
  admitted: &AdmittedFlow,
  retained_inputs: &[octacity_server_factory::FlowNodeInput],
  append: &crate::FactoryRunHistoryAppend,
) -> Result<BTreeSet<octacity_server_factory::FactoryRoleId>, StoreError> {
  append
    .flow
    .data
    .profile_selections
    .iter()
    .filter(|selection| selection.candidate().is_some())
    .map(|selection| {
      let input = retained_inputs
        .iter()
        .chain(&append.flow.data.inputs)
        .find(|input| input.digest().ok() == Some(selection.input_digest()));
      input
        .and_then(|input| {
          admitted
            .closure()
            .definition(input.definition())
            .and_then(|d| d.node(input.node()))
        })
        .and_then(|node| node.profile_selection())
        .map(|binding| binding.role.id)
        .ok_or(StoreError::Conflict {
          entity: octacity_server_domain::EntityKind::FactoryRun,
        })
    })
    .collect()
}
