use std::collections::{BTreeMap, BTreeSet};

use octacity_server_factory::{
  AdmittedFlow, BudgetUsage, ContextManifest, ContextManifestId, FactoryDigest, FactoryRun, FlowRun, FlowRunId,
  FlowRuntimeHistory, MacroCall, MacroCallCompletion, MacroCallId, NodeAttempt, NodeAttemptCompletion, NodeAttemptId,
  StageAttempt, StageAttemptCompletion, StageAttemptId, StageHandoff, StageHandoffId, WorkflowCycle, WorkflowCycleId,
  validate_flow_runtime_history, validate_lifecycle_progress, validate_lifecycle_transition,
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
  /// Exact retained queue selection granting the current claim, when present.
  pub pool_selection: Option<&'a crate::PhasePoolSelection>,
  /// Current aggregate projection.
  pub run: &'a FactoryRun,
  /// Current aggregate budget observation.
  pub budget: &'a FactoryBudgetRecord,
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
  /// Existing Stage Attempts keyed by immutable identity.
  pub stage_attempts: &'a BTreeMap<StageAttemptId, StageAttempt>,
  /// Existing generic Node Attempts keyed by immutable identity.
  pub node_attempts: &'a BTreeMap<NodeAttemptId, NodeAttempt>,
  /// Existing generic Node Attempt completions keyed by attempt identity.
  pub node_completions: &'a BTreeMap<NodeAttemptId, NodeAttemptCompletion>,
  /// Existing Flow Runs keyed by immutable identity.
  pub flow_runs: &'a BTreeMap<FlowRunId, FlowRun>,
  /// Existing Workflow Cycles keyed by immutable identity.
  pub workflow_cycles: &'a BTreeMap<WorkflowCycleId, WorkflowCycle>,
  /// Existing Stage completions keyed by immutable identity.
  pub stage_completions: &'a BTreeMap<FactoryDigest, StageAttemptCompletion>,
  /// Existing Stage Handoffs keyed by immutable identity.
  pub stage_handoffs: &'a BTreeMap<StageHandoffId, StageHandoff>,
  /// Existing Context Manifests keyed by immutable identity.
  pub context_manifests: &'a BTreeMap<ContextManifestId, ContextManifest>,
  /// Existing macro-call nodes keyed by immutable identity.
  pub macro_calls: &'a BTreeMap<MacroCallId, MacroCall>,
  /// Existing terminal macro-call observations keyed by call identity.
  pub macro_call_completions: &'a BTreeMap<MacroCallId, MacroCallCompletion>,
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
  let mut expected_attempt_number = u64::try_from(baseline.stage_attempts.len())
    .ok()
    .and_then(|value| value.checked_add(1))
    .ok_or(StoreError::Unavailable)?;
  let mut appended_stage_ids = BTreeSet::new();
  let appended_attempts_are_valid = request.append.stage_attempts.iter().all(|stage| {
    let valid = !baseline.stage_attempts.contains_key(&stage.id())
      && appended_stage_ids.insert(stage.id())
      && stage.run_id() == request.run_id
      && stage.subject() == baseline.run.subject()
      && stage.owner() == &request.owner
      && stage.claim() == baseline.claim.claim
      && stage.number().get() == expected_attempt_number;
    expected_attempt_number = expected_attempt_number.saturating_add(1);
    valid
  });
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
  if !baseline.admitted_flow.data_schemas().is_empty() {
    let data = octacity_server_factory::FlowDataHistory {
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
      let node = baseline
        .admitted_flow
        .closure()
        .definition(
          flow_runs
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
        if selection.run_claim() != *baseline.claim
          || selected.run_id != request.run_id
          || selected.flow_run_id != attempt.flow_run_id()
          || selected.cycle_id != attempt.workflow_cycle_id()
          || &selected.node != attempt.node_key()
          || selected.phase_input_digest != attempt.input_digest()
          || !data.inputs.iter().any(|input| {
            input.digest().ok() == Some(attempt.input_digest()) && input.generation() == selected.generation
          })
        {
          return Err(invalid());
        }
      }
    }
    for intent in &request.append.flow.data.build_intents {
      if intent.priority() != i64::from(baseline.work.priority().get())
        || intent.usage_before() != baseline.budget.usage
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
      let flow = flow_runs
        .iter()
        .find(|flow| flow.id() == input.flow_run_id())
        .ok_or_else(invalid)?;
      let cycle = workflow_cycles
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
            history: FlowRuntimeHistory {
              flow_runs: &flow_runs,
              cycles: &workflow_cycles,
              attempts: &node_attempts,
              completions: &node_completions,
            },
            records: &data.records,
            inputs: &data.inputs,
            incoming: data.incoming.first(),
          })
          .map_err(|_| invalid())?;
      }
    }
    data
      .validate(
        baseline.work,
        baseline.admitted_flow,
        FlowRuntimeHistory {
          flow_runs: &flow_runs,
          cycles: &workflow_cycles,
          attempts: &node_attempts,
          completions: &node_completions,
        },
      )
      .map_err(|_| invalid())?;
  } else if request.append.flow.data.record_count() != 0 {
    return Err(invalid());
  }
  let flow_runtime_is_valid = validate_flow_runtime_history(
    baseline.admitted_flow,
    FlowRuntimeHistory {
      flow_runs: &flow_runs,
      cycles: &workflow_cycles,
      attempts: &node_attempts,
      completions: &node_completions,
    },
  )
  .is_ok();
  let appended_nodes_have_current_owner = request.append.flow.attempts.iter().all(|attempt| {
    attempt.owner() == &request.owner
      && attempt.claim() == baseline.claim.claim
      && attempt.deadline() <= baseline.claim.claim.expires_at()
  });
  let appended_node_completions_have_current_owner = request.append.flow.completions.iter().all(|completion| {
    completion.owner() == &request.owner
      && completion.claim() == baseline.claim.claim
      && completion
        .claim()
        .verify_fence(request.fence, completion.observed_at())
        .is_ok()
  });
  let stage_projection_is_complete = request.append.stage_attempts.iter().all(|stage| {
    request
      .append
      .flow
      .attempts
      .iter()
      .any(|node| baseline.admitted_flow.matches_stage_projection(stage, node))
  }) && request.append.flow.attempts.iter().all(|node| {
    node.stage_projection_id().is_none_or(|stage_id| {
      request
        .append
        .stage_attempts
        .iter()
        .find(|stage| stage.id() == stage_id)
        .is_some_and(|stage| baseline.admitted_flow.matches_stage_projection(stage, node))
    })
  });
  let completed_stage_ids = baseline
    .stage_completions
    .values()
    .map(StageAttemptCompletion::stage_attempt_id)
    .collect::<BTreeSet<_>>();
  let mut appended_completion_ids = BTreeSet::new();
  let mut appended_completed_stages = BTreeSet::new();
  let appended_completions_are_valid = request.append.stage_attempt_completions.iter().all(|completion| {
    let stage = baseline.stage_attempts.get(&completion.stage_attempt_id()).or_else(|| {
      request
        .append
        .stage_attempts
        .iter()
        .find(|stage| stage.id() == completion.stage_attempt_id())
    });
    !baseline.stage_completions.contains_key(&completion.id())
      && appended_completion_ids.insert(completion.id())
      && !completed_stage_ids.contains(&completion.stage_attempt_id())
      && appended_completed_stages.insert(completion.stage_attempt_id())
      && completion.run_id() == request.run_id
      && completion.owner() == &request.owner
      && completion.claim() == baseline.claim.claim
      && completion
        .claim()
        .verify_fence(request.fence, completion.observed_at())
        .is_ok()
      && stage.is_some_and(|stage| completion.usage().validate(stage.budget()).is_ok())
  });
  let call_context_is_valid = call_context_is_valid(baseline, request);
  let canonical_budget =
    FactoryBudgetRecord::new(request.run_id, next_version, request.budget.usage, request.committed_at);
  let canonical_lifecycle = FactoryLifecycleCheckpoint::new(
    request.run_id,
    next_version,
    request.lifecycle_checkpoint.progress.clone(),
    request.lifecycle_checkpoint.signal,
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
  let projection_is_valid = projection_is_valid(baseline.current, request);
  let outbox_is_valid = request.outbox.iter().all(|record| {
    record.run_id == request.run_id
      && record.recorded_at == request.committed_at
      && record.available_at >= request.committed_at
      && record.state == FactoryOutboxState::Pending
      && record.attempt == 0
      && record.is_canonical()
  });
  let configured_flow = !baseline.admitted_flow.data_schemas().is_empty();
  let appended_attempt_count = u32::try_from(if configured_flow {
    request.append.flow.attempts.len()
  } else {
    request.append.stage_attempts.len()
  })
  .ok();
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
        .stage_attempt_completions
        .iter()
        .filter(|_| !configured_flow)
        .map(StageAttemptCompletion::usage)
        .chain(
          request
            .append
            .flow
            .completions
            .iter()
            .filter(|_| configured_flow)
            .map(NodeAttemptCompletion::usage),
        ),
    )
    || configured_flow
      && request
        .budget
        .usage
        .validate(baseline.admitted_flow.limits().budget())
        .is_err()
    || request.current.budget_id != request.budget.id
    || request.lifecycle_checkpoint != canonical_lifecycle
    || validate_lifecycle_progress(request.next_run.state(), &request.lifecycle_checkpoint.progress).is_err()
    || validate_lifecycle_transition(
      baseline.run.state(),
      &baseline.lifecycle.progress,
      request.next_run.state(),
      &request.lifecycle_checkpoint.progress,
    )
    .is_err()
    || !appended_attempts_are_valid
    || !flow_runtime_is_valid
    || !appended_nodes_have_current_owner
    || !appended_node_completions_have_current_owner
    || !stage_projection_is_complete
    || !appended_completions_are_valid
    || !call_context_is_valid
    || request.current.lifecycle_checkpoint_id != request.lifecycle_checkpoint.id
    || !projection_is_valid
    || request.audit.run_id != request.run_id
    || request.audit.recorded_at != request.committed_at
    || request.audit != canonical_audit
    || !outbox_is_valid
  {
    return Err(invalid());
  }
  Ok(())
}

fn call_context_is_valid(baseline: &FactoryTransitionBaseline<'_>, request: &CommitFactoryRunTransition) -> bool {
  let mut stages = baseline.stage_attempts.clone();
  if request
    .append
    .stage_attempts
    .iter()
    .any(|record| stages.insert(record.id(), record.clone()).is_some())
  {
    return false;
  }

  let mut handoffs = baseline.stage_handoffs.clone();
  let mut handoff_stages = handoffs
    .values()
    .map(StageHandoff::stage_attempt_id)
    .collect::<BTreeSet<_>>();
  for handoff in &request.append.stage_handoffs {
    let Some(stage) = stages.get(&handoff.stage_attempt_id()) else {
      return false;
    };
    if handoffs.insert(handoff.id(), handoff.clone()).is_some()
      || !handoff_stages.insert(handoff.stage_attempt_id())
      || handoff.subject().exact() != stage.subject()
    {
      return false;
    }
  }

  let mut manifests = baseline.context_manifests.clone();
  if request
    .append
    .context_manifests
    .iter()
    .any(|record| manifests.insert(record.id(), record.clone()).is_some())
  {
    return false;
  }

  let mut calls = baseline.macro_calls.clone();
  if request
    .append
    .macro_calls
    .iter()
    .any(|record| calls.insert(record.id(), record.clone()).is_some())
  {
    return false;
  }

  if request.append.context_manifests.iter().any(|manifest| {
    request
      .append
      .macro_calls
      .iter()
      .all(|call| call.context_manifest_id() != manifest.id())
  }) {
    return false;
  }

  let calls_are_valid = request.append.macro_calls.iter().all(|call| {
    let stage = stages.get(&call.stage_attempt_id());
    let manifest = manifests.get(&call.context_manifest_id());
    let expected_depth = call
      .call_dependencies()
      .iter()
      .filter_map(|id| calls.get(id))
      .map(MacroCall::depth)
      .max()
      .unwrap_or(0)
      .checked_add(1);
    call.run_id() == request.run_id
      && call.subject().exact() == baseline.run.subject()
      && stage.is_some_and(|stage| stage.subject() == baseline.run.subject())
      && manifest.is_some_and(|manifest| {
        manifest.subject() == call.subject() && manifest.digest().ok() == Some(call.context_digest())
      })
      && call.stage_dependencies().iter().all(|id| {
        stages.get(id).is_some_and(|dependency_stage| {
          dependency_stage.run_id() == request.run_id
            && dependency_stage.subject() == baseline.run.subject()
            && handoffs
              .values()
              .any(|handoff| handoff.stage_attempt_id() == *id && handoff.subject().exact() == baseline.run.subject())
        })
      })
      && call.call_dependencies().iter().all(|id| {
        calls.get(id).is_some_and(|dependency| {
          dependency.run_id() == request.run_id
            && dependency.subject() == call.subject()
            && dependency.depth() < call.depth()
        })
      })
      && expected_depth == Some(call.depth())
      && call.parent_id().is_none_or(|parent| {
        call.call_dependencies().contains(&parent)
          && calls.get(&parent).is_some_and(|value| {
            value.run_id() == request.run_id
              && value.stage_attempt_id() == call.stage_attempt_id()
              && value.subject() == call.subject()
          })
      })
  });
  let mut completed_calls = BTreeSet::new();
  calls_are_valid
    && request.append.macro_call_completions.iter().all(|completion| {
      !baseline.macro_call_completions.contains_key(&completion.call_id())
        && completed_calls.insert(completion.call_id())
        && calls.get(&completion.call_id()).is_some_and(|call| {
          call.subject() == completion.subject()
            && stages
              .get(&call.stage_attempt_id())
              .is_some_and(|stage| completion.task_envelope_digest() == stage.input_digest())
            && completion.usage_is_valid_for(call.budget())
        })
    })
}

fn projection_is_valid(current: &FactoryRunCurrentProjection, request: &CommitFactoryRunTransition) -> bool {
  request.current.stage_attempt_id.is_none_or(|id| {
    Some(id) == current.stage_attempt_id || request.append.stage_attempts.iter().any(|record| record.id() == id)
  }) && request.current.macro_call_id.is_none_or(|id| {
    Some(id) == current.macro_call_id || request.append.macro_calls.iter().any(|record| record.id() == id)
  }) && request.current.signal_request_id.is_none_or(|id| {
    Some(id) == current.signal_request_id || request.append.signal_requests.iter().any(|record| record.id() == id)
  }) && request.current.signal_receipt_id.is_none_or(|id| {
    Some(id) == current.signal_receipt_id || request.append.signal_receipts.iter().any(|record| record.id() == id)
  }) && request.current.build_id.is_none_or(|id| {
    Some(id) == current.build_id || request.append.linked_builds.iter().any(|record| record.build_id == id)
  }) && request.current.candidate_id.is_none_or(|id| {
    Some(id) == current.candidate_id || request.append.candidates.iter().any(|record| record.id() == id)
  }) && request
    .current
    .evidence_id
    .is_none_or(|id| Some(id) == current.evidence_id || request.append.evidence.iter().any(|record| record.id() == id))
    && request.current.evaluation_plan_id.is_none_or(|id| {
      Some(id) == current.evaluation_plan_id || request.append.evaluation_plans.iter().any(|record| record.id() == id)
    })
    && request.current.decision_id.is_none_or(|id| {
      Some(id) == current.decision_id || request.append.decisions.iter().any(|record| record.id() == id)
    })
    && request.current.escalation_id.is_none_or(|id| {
      Some(id) == current.escalation_id || request.append.escalations.iter().any(|record| record.id() == id)
    })
    && request.current.delivery_attempt_id.is_none_or(|id| {
      Some(id) == current.delivery_attempt_id || request.append.delivery_attempts.iter().any(|record| record.id() == id)
    })
    && request.current.reporting_attempt_id.is_none_or(|id| {
      Some(id) == current.reporting_attempt_id
        || request.append.reporting_attempts.iter().any(|record| record.id() == id)
    })
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
