use std::collections::{BTreeMap, BTreeSet};

use octacity_server_factory::{
  BudgetUsage, FactoryDigest, FactoryRun, StageAttempt, StageAttemptCompletion, StageAttemptId,
  validate_lifecycle_progress, validate_lifecycle_transition,
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
  /// Existing Stage Attempts keyed by immutable identity.
  pub stage_attempts: &'a BTreeMap<StageAttemptId, StageAttempt>,
  /// Existing Stage completions keyed by immutable identity.
  pub stage_completions: &'a BTreeMap<FactoryDigest, StageAttemptCompletion>,
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
  let appended_attempt_count = u32::try_from(request.append.stage_attempts.len()).ok();
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
      &request.append.stage_attempt_completions,
    )
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
    || !appended_completions_are_valid
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
  completions: &[StageAttemptCompletion],
) -> bool {
  let totals = completions.iter().try_fold(
    (
      current.elapsed_millis,
      current.tokens,
      current.cost_micro_units,
      current.output_bytes,
    ),
    |(elapsed, tokens, cost, output), completion| {
      Some((
        elapsed.checked_add(completion.usage().elapsed_millis)?,
        tokens.checked_add(completion.usage().tokens)?,
        cost.checked_add(completion.usage().cost_micro_units)?,
        output.checked_add(completion.usage().output_bytes)?,
      ))
    },
  );
  totals.is_some_and(|(elapsed, tokens, cost, output)| {
    next.elapsed_millis >= elapsed
      && next.tokens >= tokens
      && next.cost_micro_units >= cost
      && next.output_bytes >= output
  })
}
