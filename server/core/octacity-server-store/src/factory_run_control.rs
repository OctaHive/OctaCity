use octacity_server_domain::Timestamp;
use octacity_server_factory::{FactoryDigest, FactoryRunId, FactoryRunState, FactoryRunVersion};
use serde::{Deserialize, Serialize};

use crate::{IdempotencyKey, MutationDisposition};

/// One closed, code-owned Factory Run control intent.
///
/// Variants bind confirmations or operator responses to exact retained targets.
/// They accept no caller-authored lifecycle state, Assessment, Decision outcome
/// or execution success; a profile reference grants no new capability authority.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum FactoryRunControlIntent {
  /// Request cancellation through the normal reconciler path.
  Cancel,
  /// Choose an exact admitted profile for one frozen, pending node occurrence.
  SelectProfile {
    /// Frozen input binds the Run, nested Flow, cycle, node and attempt generation.
    input_digest: FactoryDigest,
    /// Exact admitted Project/profile version; mutable aliases are not accepted.
    profile: octacity_server_factory::FactoryRoleProfileRef,
  },
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
      Self::SelectProfile { .. } => "select-factory-profile",
      Self::RetryNode { .. } => "retry-factory-node",
      Self::ResolveNode { .. } => "resolve-factory-node",
    }
  }

  fn digest(&self) -> FactoryDigest {
    match self {
      Self::SelectProfile { input_digest, profile } => FactoryDigest::sha256(
        "octacity.factory.control-intent.v1",
        &[
          b"select-profile",
          &input_digest.as_bytes(),
          profile.project_id.as_uuid().as_bytes(),
          profile.id.as_uuid().as_bytes(),
          &profile.version.get().to_be_bytes(),
          &profile.digest.as_bytes(),
        ],
      ),
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

impl ApplyFactoryRunControl {
  /// Stable user intent within the scoped operation/key. Server request time
  /// and correlation can change after a lost response; accepted audit facts
  /// and the original receipt remain immutable.
  #[must_use]
  pub fn replay_intent(&self) -> (FactoryRunId, FactoryRunVersion, &FactoryRunControlIntent) {
    (self.run_id, self.expected_version, &self.intent)
  }
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

fn node_contract(
  snapshot: &crate::FactoryRunSnapshot,
  attempt_id: octacity_server_factory::NodeAttemptId,
) -> Result<
  (
    &octacity_server_factory::NodeAttempt,
    &octacity_server_factory::FlowNodeDefinition,
  ),
  crate::StoreError,
> {
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
  Ok((attempt, node))
}

/// Pure current HumanGate eligibility shared by descriptions and atomic control validation.
/// This exposes the pinned response contract, never a reservation or mutation authority.
pub fn factory_node_response_contract(
  snapshot: &crate::FactoryRunSnapshot,
  attempt_id: octacity_server_factory::NodeAttemptId,
  at: Timestamp,
) -> Result<&octacity_server_factory::FlowNodeDefinition, crate::StoreError> {
  let invalid = || {
    crate::StoreError::invalid(
      crate::StoreOperation::ControlFactoryRun,
      crate::StoreInputError::InvalidFactoryRunControl,
    )
  };
  let (attempt, node) = node_contract(snapshot, attempt_id)?;
  if node.kind() != octacity_server_factory::FlowNodeKind::HumanGate || at < attempt.claim().claimed_at() {
    return Err(invalid());
  }
  if !snapshot.flow.runtime().is_current_cycle(attempt.workflow_cycle_id())
    || snapshot.run.state().is_terminal()
    || at > attempt.deadline()
    || snapshot.lifecycle_checkpoints.iter().find(|row| row.id == snapshot.current.lifecycle_checkpoint_id).is_none_or(|row| row.cancellation_requested)
    || snapshot.flow.completions.iter().any(|row| row.node_attempt_id() == attempt_id)
    || snapshot.flow.attempts.iter().any(|row| row.flow_run_id() == attempt.flow_run_id() && row.workflow_cycle_id() == attempt.workflow_cycle_id() && row.node_key() == attempt.node_key() && row.number() > attempt.number())
    || snapshot.controls.iter().any(|row| matches!(row.intent, FactoryRunControlIntent::ResolveNode { node_attempt_id, .. } if node_attempt_id == attempt_id))
  { return Err(crate::StoreError::Conflict { entity: octacity_server_domain::EntityKind::FactoryRun }); }
  Ok(node)
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
  use octacity_server_factory::FlowPayload;
  let invalid = || {
    crate::StoreError::invalid(
      crate::StoreOperation::ControlFactoryRun,
      crate::StoreInputError::InvalidFactoryRunControl,
    )
  };
  let (_, node) = node_contract(snapshot, attempt_id)?;
  let schema = node
    .outcome(outcome)
    .and_then(|outcome| snapshot.admitted_flow.data_schema(outcome.schema()))
    .ok_or_else(invalid)?;
  factory_node_response_contract(snapshot, attempt_id, at)?;
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
  let (attempt, node) = node_contract(snapshot, attempt_id)?;
  if !snapshot.flow.runtime().is_current_cycle(attempt.workflow_cycle_id()) {
    return Err(crate::StoreError::Conflict {
      entity: octacity_server_domain::EntityKind::FactoryRun,
    });
  }
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

/// Latest accepted choice for one occurrence; history remains append-only.
#[must_use]
pub fn latest_factory_profile_control(
  controls: &[FactoryRunControlRecord],
  input: FactoryDigest,
) -> Option<&FactoryRunControlRecord> {
  controls
    .iter()
    .filter(|control| {
      matches!(control.intent,
    FactoryRunControlIntent::SelectProfile { input_digest, .. } if input_digest == input)
    })
    .max_by_key(|control| control.run_version)
}

/// Common pre-dispatch time and budget bounds; this grants no choice or dispatch authority.
/// Initial preparation has no selection yet. A retained wait keeps its original exclusive deadline.
pub fn factory_profile_wait_is_open(
  snapshot: &crate::FactoryRunSnapshot,
  input: &octacity_server_factory::FlowNodeInput,
  at: Timestamp,
) -> bool {
  let Ok(digest) = input.digest() else {
    return false;
  };
  let Some(budget) = snapshot.budgets.iter().find(|row| row.id == snapshot.current.budget_id) else {
    return false;
  };
  let selection = snapshot.flow.data.current_profile_selection(digest);
  let Some(unspent_usage) = budget
    .usage
    .checked_sub(selection.map_or(Default::default(), |selection| selection.signal_usage()))
  else {
    return false;
  };
  snapshot
    .flow
    .data
    .current_profile_selection(digest)
    .is_none_or(|selection| at >= selection.recorded_at() && at < selection.deadline())
    && octacity_server_factory::FlowBuildIntent::verify_available_budget(
      input,
      &snapshot.admitted_flow,
      snapshot.flow.runtime(),
      &snapshot.flow.data,
      unspent_usage,
    )
    .is_ok()
}

/// Checks a live choice against the exact retained wait under the aggregate lock.
/// The input digest is the occurrence precondition; acceptance never publishes a Build.
fn validate_factory_profile_choice(
  snapshot: &crate::FactoryRunSnapshot,
  input_digest: FactoryDigest,
  profile: &octacity_server_factory::FactoryRoleProfileRef,
  at: Timestamp,
) -> Result<(), crate::StoreError> {
  let conflict = || crate::StoreError::Conflict {
    entity: octacity_server_domain::EntityKind::FactoryRun,
  };
  let input = snapshot
    .flow
    .data
    .inputs
    .iter()
    .find(|input| input.digest().ok() == Some(input_digest))
    .ok_or_else(conflict)?;
  snapshot
    .flow
    .data
    .current_profile_selection(input_digest)
    .ok_or_else(conflict)?;
  let binding = snapshot
    .admitted_flow
    .closure()
    .definition(input.definition())
    .and_then(|d| d.node(input.node()))
    .and_then(|n| n.profile_selection())
    .ok_or_else(conflict)?;
  if snapshot.run.state().is_terminal()
    || snapshot
      .lifecycle_checkpoints
      .iter()
      .find(|row| row.id == snapshot.current.lifecycle_checkpoint_id)
      .is_none_or(|row| row.cancellation_requested)
    || !snapshot.flow.runtime().is_current_cycle(input.cycle_id())
    || !factory_profile_wait_is_open(snapshot, input, at)
    || !binding.candidates.values().any(|c| c.profile.reference() == profile)
    || profile.project_id != snapshot.work.subject().project_id()
    || snapshot.flow.attempts.iter().any(|a| {
      a.flow_run_id() == input.flow_run_id()
        && a.workflow_cycle_id() == input.cycle_id()
        && a.node_key() == input.node()
    })
    || snapshot
      .flow
      .data
      .build_intents
      .iter()
      .any(|intent| intent.input().digest().ok() == Some(input_digest))
    || latest_factory_profile_control(&snapshot.controls, input_digest).is_some_and(
      |c| matches!(&c.intent, FactoryRunControlIntent::SelectProfile { profile: current, .. } if current == profile),
    )
  {
    return Err(conflict());
  }
  Ok(())
}

/// Read-only eligibility using the same checks as atomic manual acceptance.
/// The caller must authorize the owning Run/Project and qualify installed capabilities separately.
#[must_use]
pub fn factory_profile_choice_is_open(
  snapshot: &crate::FactoryRunSnapshot,
  input_digest: FactoryDigest,
  profile: &octacity_server_factory::FactoryRoleProfileRef,
  at: Timestamp,
) -> bool {
  validate_factory_profile_choice(snapshot, input_digest, profile, at).is_ok()
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
    FactoryRunControlIntent::SelectProfile { input_digest, profile } => {
      validate_factory_profile_choice(snapshot, *input_digest, profile, request.requested_at)?;
      (false, "profile-choice-recorded")
    }
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

/// Verifies manual consumption against retained accepted intent, for both adapters.
pub(crate) fn validate_factory_profile_control_history(
  admitted: &octacity_server_factory::AdmittedFlow,
  data: &octacity_server_factory::FlowDataHistory,
  controls: &[FactoryRunControlRecord],
) -> Result<(), crate::StoreError> {
  let mut targets = std::collections::BTreeSet::new();
  for control in controls {
    if let FactoryRunControlIntent::SelectProfile { input_digest, profile } = &control.intent {
      let input = data
        .inputs
        .iter()
        .find(|i| i.digest().ok() == Some(*input_digest))
        .ok_or(crate::StoreError::Unavailable)?;
      let pending = data
        .profile_selections
        .iter()
        .find(|s| s.input_digest() == *input_digest && s.generation() == 0)
        .ok_or(crate::StoreError::Unavailable)?;
      let candidate = admitted
        .closure()
        .definition(input.definition())
        .and_then(|d| d.node(input.node()))
        .and_then(|n| n.profile_selection())
        .is_some_and(|s| s.candidates.values().any(|c| c.profile.reference() == profile));
      if !candidate
        || !targets.insert((*input_digest, control.run_version))
        || control.recorded_at < pending.started_at()
        || control.recorded_at > pending.deadline()
      {
        return Err(crate::StoreError::Unavailable);
      }
    }
  }
  let mut consumed = std::collections::BTreeSet::new();
  for choice in &data.profile_selections {
    if let Some(id) = choice.manual_control() {
      let control = controls
        .iter()
        .find(|c| c.id == id)
        .ok_or(crate::StoreError::Unavailable)?;
      let input = data
        .inputs
        .iter()
        .find(|i| i.digest().ok() == Some(choice.input_digest()))
        .ok_or(crate::StoreError::Unavailable)?;
      let profile = admitted
        .closure()
        .definition(input.definition())
        .and_then(|d| d.node(input.node()))
        .and_then(|n| n.profile_selection())
        .and_then(|s| choice.candidate().and_then(|k| s.candidates.get(k)))
        .map(|c| c.profile.reference())
        .ok_or(crate::StoreError::Unavailable)?;
      let predecessor = data
        .profile_selections
        .iter()
        .find(|s| s.digest().ok() == choice.pending_digest())
        .ok_or(crate::StoreError::Unavailable)?;
      let earlier = predecessor
        .manual_control()
        .and_then(|id| controls.iter().find(|c| c.id == id));
      if !consumed.insert(id)
        || earlier.is_some_and(|earlier| earlier.run_version >= control.run_version)
        || control.recorded_at > choice.recorded_at()
        || control.intent
          != (FactoryRunControlIntent::SelectProfile {
            input_digest: choice.input_digest(),
            profile: profile.clone(),
          })
      {
        return Err(crate::StoreError::Unavailable);
      }
    }
  }
  Ok(())
}

/// Revalidates retained technical receipts against their exact accepted operator intent.
/// A retry lineage cannot survive restoration after its authorization row disappears.
pub fn validate_factory_node_control_history(snapshot: &crate::FactoryRunSnapshot) -> Result<(), crate::StoreError> {
  use octacity_server_factory::FlowNodeStopReason;
  validate_factory_profile_control_history(&snapshot.admitted_flow, &snapshot.flow.data, &snapshot.controls)?;
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
