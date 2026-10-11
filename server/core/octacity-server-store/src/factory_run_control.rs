use octacity_server_domain::Timestamp;
use octacity_server_factory::{FactoryDigest, FactoryRunId, FactoryRunState, FactoryRunVersion};
use serde::{Deserialize, Serialize};

use crate::{IdempotencyKey, MutationDisposition};

/// One closed, code-owned Factory Run control intent.
///
/// The variants contain only identities used as strong confirmations. There
/// is intentionally no caller-provided lifecycle state, candidate digest,
/// Assessment, Decision outcome, or success value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum FactoryRunControlIntent {
  /// Request cancellation through the normal reconciler path.
  Cancel,
  /// Confirm one new execution of the exact independently failed configured node.
  RetryNode {
    /// Failed attempt, used as a stale target precondition.
    node_attempt_id: octacity_server_factory::NodeAttemptId,
  },
  /// Respond to the exact pending HumanGate; the owner later commits its result.
  ResolveNode {
    /// Exact immutable attempt, independent of the logical node name.
    node_attempt_id: octacity_server_factory::NodeAttemptId,
    /// One outcome from its pinned declaration.
    outcome: octacity_server_factory::FactoryKey,
    /// Exact schema-validated operator response.
    payload: octacity_server_factory::FlowPayload,
  },
}

impl FactoryRunControlIntent {
  /// Stable management operation used for audit and scoped idempotency.
  pub const fn operation(&self) -> &'static str {
    match self {
      Self::Cancel => "cancel-factory-run",
      Self::RetryNode { .. } => "retry-factory-node",
      Self::ResolveNode { .. } => "resolve-factory-node",
    }
  }

  fn digest(&self) -> FactoryDigest {
    match self {
      Self::ResolveNode {
        node_attempt_id,
        outcome,
        payload,
      } => FactoryDigest::sha256(
        "octacity.factory.control-intent.v1",
        &[
          b"node-response",
          node_attempt_id.as_uuid().as_bytes(),
          outcome.as_str().as_bytes(),
          payload.schema().identity().as_str().as_bytes(),
          payload.schema().version().as_str().as_bytes(),
          &payload.schema().digest().as_bytes(),
          payload.value().to_string().as_bytes(),
        ],
      ),
      Self::Cancel => FactoryDigest::sha256("octacity.factory.control-intent.v1", &[b"cancel"]),
      Self::RetryNode { node_attempt_id } => FactoryDigest::sha256(
        "octacity.factory.control-intent.v1",
        &[b"node-retry", node_attempt_id.as_uuid().as_bytes()],
      ),
    }
  }
}

/// Immutable accepted operator intent appended to Factory Run history.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FactoryRunControlRecord {
  /// Content-derived record identity.
  pub id: FactoryDigest,
  /// Controlled Run identity.
  pub run_id: FactoryRunId,
  /// Aggregate version produced by this intent.
  pub run_version: FactoryRunVersion,
  /// Closed accepted intent, including its bounded human reason when required.
  pub intent: FactoryRunControlIntent,
  /// Authoritative acceptance time.
  pub recorded_at: Timestamp,
}

impl FactoryRunControlRecord {
  /// Constructs one canonical immutable control record.
  #[must_use]
  pub fn new(
    run_id: FactoryRunId,
    run_version: FactoryRunVersion,
    intent: FactoryRunControlIntent,
    recorded_at: Timestamp,
  ) -> Self {
    let id = FactoryDigest::sha256(
      "octacity.factory.control-record.v1",
      &[
        run_id.as_uuid().as_bytes(),
        &run_version.get().to_be_bytes(),
        &intent.digest().as_bytes(),
        &recorded_at.unix_millis().to_be_bytes(),
      ],
    );
    Self {
      id,
      run_id,
      run_version,
      intent,
      recorded_at,
    }
  }
}

/// Idempotent, preconditioned mutation of one Factory Run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ApplyFactoryRunControl {
  /// Run whose current state is being controlled.
  pub run_id: FactoryRunId,
  /// Exact aggregate version observed by the caller.
  pub expected_version: FactoryRunVersion,
  /// Stable transport replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Closed command intent.
  pub intent: FactoryRunControlIntent,
  /// Authoritative request time.
  pub requested_at: Timestamp,
}

/// Safe result of applying or replaying one Factory Run control intent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FactoryRunControlOutcome {
  /// Whether the original mutation was applied or replayed.
  pub disposition: MutationDisposition,
  /// Controlled Run identity.
  pub run_id: FactoryRunId,
  /// Aggregate version produced by the original mutation.
  pub version: FactoryRunVersion,
  /// Code-owned state after the mutation.
  pub state: FactoryRunState,
  /// Whether cancellation is durably requested for reconciliation.
  pub cancellation_requested: bool,
}

/// Revalidates a response against an exact active HumanGate and its admitted schema.
/// Both backends use this seam under the aggregate lock before recording intent.
pub fn validate_factory_node_response(
  snapshot: &crate::FactoryRunSnapshot,
  attempt_id: octacity_server_factory::NodeAttemptId,
  outcome: &octacity_server_factory::FactoryKey,
  payload: &octacity_server_factory::FlowPayload,
  at: Timestamp,
) -> Result<(), crate::StoreError> {
  use octacity_server_factory::{FlowNodeKind, FlowPayload};
  let invalid = || {
    crate::StoreError::invalid(
      crate::StoreOperation::ControlFactoryRun,
      crate::StoreInputError::InvalidFactoryRunControl,
    )
  };
  let attempt = snapshot
    .flow
    .attempts
    .iter()
    .find(|row| row.id() == attempt_id)
    .ok_or_else(invalid)?;
  let node = snapshot
    .flow
    .runs
    .iter()
    .find(|row| row.id() == attempt.flow_run_id())
    .and_then(|run| snapshot.admitted_flow.closure().definition(run.definition()))
    .and_then(|definition| definition.node(attempt.node_key()))
    .ok_or_else(invalid)?;
  let schema = node
    .outcome(outcome)
    .and_then(|outcome| snapshot.admitted_flow.data_schema(outcome.schema()))
    .ok_or_else(invalid)?;
  if snapshot.run.state().is_terminal() || node.kind() != FlowNodeKind::HumanGate
    || at < attempt.claim().claimed_at() || at > attempt.deadline()
    || snapshot.lifecycle_checkpoints.iter().find(|row| row.id == snapshot.current.lifecycle_checkpoint_id).is_none_or(|row| row.cancellation_requested)
    || snapshot.flow.completions.iter().any(|row| row.node_attempt_id() == attempt_id)
    || snapshot.flow.attempts.iter().any(|row| row.flow_run_id() == attempt.flow_run_id() && row.workflow_cycle_id() == attempt.workflow_cycle_id() && row.node_key() == attempt.node_key() && row.number() > attempt.number())
    || snapshot.controls.iter().any(|row| matches!(row.intent, FactoryRunControlIntent::ResolveNode { node_attempt_id, .. } if node_attempt_id == attempt_id))
  { return Err(invalid()); }
  FlowPayload::restore(&serde_json::to_vec(payload).map_err(|_| invalid())?, schema).map_err(|_| invalid())?;
  Ok(())
}

/// Checks exact retry eligibility against the pinned node policy and retained failure.
/// Historical retries do not grant permission for a newer execution or cycle.
pub fn validate_factory_node_retry(
  snapshot: &crate::FactoryRunSnapshot,
  attempt_id: octacity_server_factory::NodeAttemptId,
  at: Timestamp,
) -> Result<(), crate::StoreError> {
  let invalid = || {
    crate::StoreError::invalid(
      crate::StoreOperation::ControlFactoryRun,
      crate::StoreInputError::InvalidFactoryRunControl,
    )
  };
  let attempt = snapshot
    .flow
    .attempts
    .iter()
    .find(|row| row.id() == attempt_id)
    .ok_or_else(invalid)?;
  let node = snapshot
    .flow
    .runs
    .iter()
    .find(|row| row.id() == attempt.flow_run_id())
    .and_then(|run| snapshot.admitted_flow.closure().definition(run.definition()))
    .and_then(|definition| definition.node(attempt.node_key()))
    .ok_or_else(invalid)?;
  if snapshot.run.state().is_terminal()
    || snapshot.lifecycle_checkpoints.iter().find(|row| row.id == snapshot.current.lifecycle_checkpoint_id).is_none_or(|row| row.cancellation_requested)
    || snapshot.flow.attempts.iter().any(|row| row.flow_run_id() == attempt.flow_run_id() && row.workflow_cycle_id() == attempt.workflow_cycle_id() && row.node_key() == attempt.node_key() && row.number() > attempt.number())
    || snapshot.controls.iter().any(|row| matches!(row.intent, FactoryRunControlIntent::RetryNode { node_attempt_id } if node_attempt_id == attempt_id))
    || !snapshot.flow.data.stops.iter().any(|row| row.node_attempt_id() == attempt_id && row.reason() == octacity_server_factory::FlowNodeStopReason::ExecutionFailed && row.observed_at() <= at)
  { return Err(invalid()); }
  let mut ancestor = attempt;
  let mut retried = 0_u16;
  while let Some(parent) = ancestor.retry_of() {
    retried = retried.checked_add(1).ok_or_else(invalid)?;
    if retried >= node.max_execution_retries() {
      return Err(invalid());
    }
    ancestor = snapshot
      .flow
      .attempts
      .iter()
      .find(|row| row.id() == parent && row.number() < ancestor.number())
      .ok_or_else(invalid)?;
  }
  if retried >= node.max_execution_retries() {
    return Err(invalid());
  }
  let budget = snapshot
    .budgets
    .iter()
    .find(|row| row.id == snapshot.current.budget_id)
    .ok_or_else(invalid)?;
  budget
    .usage
    .checked_add(octacity_server_factory::BudgetUsage {
      attempts: 1,
      ..Default::default()
    })
    .ok_or_else(invalid)?
    .validate(snapshot.admitted_flow.limits().budget())
    .map_err(|_| invalid())?;
  Ok(())
}

/// Canonical common control facts prepared before either backend mutates authority.
pub struct FactoryRunControlChange {
  /// Successor technical Run version.
  pub run: octacity_server_factory::FactoryRun,
  /// Unchanged measured consumption under the new aggregate version.
  pub budget: crate::FactoryBudgetRecord,
  /// Current progress with accepted cancellation intent, when applicable.
  pub lifecycle: crate::FactoryLifecycleCheckpoint,
  /// Exact immutable operator intent.
  pub control: FactoryRunControlRecord,
  /// Secret-safe actor/request correlation.
  pub audit: crate::FactoryAuditFact,
  /// Safe original mutation outcome used for durable replay.
  pub outcome: FactoryRunControlOutcome,
}
/// Prepares graph-neutral operator intent under the current aggregate precondition.
/// Logical outcome authority remains in the configured HumanGate and the Flow owner.
pub fn prepare_factory_run_control(
  snapshot: &crate::FactoryRunSnapshot,
  request: &ApplyFactoryRunControl,
  context: &crate::MutationAuditContext,
  record_count: usize,
) -> Result<FactoryRunControlChange, crate::StoreError> {
  let invalid = || {
    crate::StoreError::invalid(
      crate::StoreOperation::ControlFactoryRun,
      crate::StoreInputError::InvalidFactoryRunControl,
    )
  };
  let conflict = || crate::StoreError::Conflict {
    entity: octacity_server_domain::EntityKind::FactoryRun,
  };
  if snapshot.run.id() != request.run_id || snapshot.run.version() != request.expected_version {
    return Err(conflict());
  }
  let current = snapshot
    .lifecycle_checkpoints
    .iter()
    .find(|row| row.id == snapshot.current.lifecycle_checkpoint_id)
    .ok_or_else(invalid)?;
  let budget = snapshot
    .budgets
    .iter()
    .find(|row| row.id == snapshot.current.budget_id)
    .ok_or_else(invalid)?;
  if request.requested_at < current.recorded_at
    || record_count
      .checked_add(4)
      .is_none_or(|count| count > crate::MAX_FACTORY_RUN_SNAPSHOT_RECORDS)
  {
    return Err(invalid());
  }
  let (cancellation, result) = match &request.intent {
    FactoryRunControlIntent::Cancel => {
      if snapshot.run.state().is_terminal() || current.cancellation_requested {
        return Err(conflict());
      }
      (true, "cancel-requested")
    }
    FactoryRunControlIntent::ResolveNode {
      node_attempt_id,
      outcome,
      payload,
    } => {
      validate_factory_node_response(snapshot, *node_attempt_id, outcome, payload, request.requested_at)?;
      (false, "node-response-recorded")
    }
    FactoryRunControlIntent::RetryNode { node_attempt_id } => {
      validate_factory_node_retry(snapshot, *node_attempt_id, request.requested_at)?;
      (false, "node-retry-requested")
    }
  };
  let version =
    FactoryRunVersion::new(snapshot.run.version().get().checked_add(1).ok_or_else(invalid)?).map_err(|_| invalid())?;
  let run = octacity_server_factory::FactoryRun::restore(
    request.run_id,
    snapshot.run.configuration().clone(),
    &snapshot.work,
    snapshot.run.subject().clone(),
    snapshot.run.state(),
    version,
  )
  .map_err(|_| invalid())?;
  let lifecycle = crate::FactoryLifecycleCheckpoint::new(
    request.run_id,
    version,
    current.progress.clone(),
    cancellation,
    request.requested_at,
  );
  let control = FactoryRunControlRecord::new(request.run_id, version, request.intent.clone(), request.requested_at);
  let key = |value| octacity_server_factory::FactoryKey::new(value).map_err(|_| invalid());
  let audit = crate::FactoryAuditFact::new(
    request.run_id,
    context.actor().kind,
    context
      .actor()
      .identity
      .as_deref()
      .map(|identity| FactoryDigest::sha256("octacity.factory.audit-actor.v1", &[identity.as_bytes()])),
    key(request.intent.operation())?,
    FactoryDigest::sha256(
      "octacity.factory.audit-request.v1",
      &[context.request_identity().as_bytes()],
    ),
    key(result)?,
    request.requested_at,
  );
  Ok(FactoryRunControlChange {
    run,
    budget: crate::FactoryBudgetRecord::new(request.run_id, version, budget.usage, request.requested_at),
    lifecycle,
    control,
    audit,
    outcome: FactoryRunControlOutcome {
      disposition: MutationDisposition::Applied,
      run_id: request.run_id,
      version,
      state: snapshot.run.state(),
      cancellation_requested: cancellation,
    },
  })
}

/// Revalidates retained technical receipts against their exact accepted operator intent.
/// A retry lineage cannot survive restoration after its authorization row disappears.
pub fn validate_factory_node_control_history(snapshot: &crate::FactoryRunSnapshot) -> Result<(), crate::StoreError> {
  use octacity_server_factory::FlowNodeStopReason;
  let cancellation_at = snapshot
    .controls
    .iter()
    .filter(|row| row.intent == FactoryRunControlIntent::Cancel)
    .map(|row| row.recorded_at)
    .min();
  if snapshot.flow.data.stops.iter().any(|row| matches!(row.reason(), FlowNodeStopReason::Cancelled { requested_at } if Some(requested_at) != cancellation_at)) {
    return Err(crate::StoreError::Unavailable);
  }
  let mut retries = std::collections::BTreeMap::new();
  for control in &snapshot.controls {
    if let FactoryRunControlIntent::RetryNode { node_attempt_id } = control.intent
      && (retries.insert(node_attempt_id, control.recorded_at).is_some()
        || !snapshot.flow.data.stops.iter().any(|row| {
          row.node_attempt_id() == node_attempt_id
            && row.reason() == FlowNodeStopReason::ExecutionFailed
            && row.observed_at() <= control.recorded_at
        }))
    {
      return Err(crate::StoreError::Unavailable);
    }
  }
  if snapshot.flow.attempts.iter().any(|attempt| {
    attempt.retry_of().is_some_and(|parent| {
      retries
        .get(&parent)
        .is_none_or(|requested_at| *requested_at > attempt.started_at())
    })
  }) {
    return Err(crate::StoreError::Unavailable);
  }
  Ok(())
}
