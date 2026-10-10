use std::{cmp::Ordering, collections::BTreeSet};

use async_trait::async_trait;
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::{
  BudgetUsage, FactoryClaim, FactoryClaimFence, FactoryDigest, FactoryKey, FactoryRunId, FactoryRunVersion,
  FindingSeverity, FlowDirective, FlowInterpreter, FlowRunId, ImmutableReference, MAX_PHASE_POOL_BATCH,
  MAX_PHASE_POOL_WIP, PhasePoolOrder, PhasePoolPolicy, WorkEnvelopeId, WorkflowCycleId, reserve_phase_budget,
};
use serde::{Deserialize, Serialize};

use crate::{FactoryRunClaimRecord, FactoryRunSnapshot, StoreError, StoreInputError, StoreOperation};

/// Exact predecessor Work and declared root-flow resolution required by a pool entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PhasePoolDependency {
  /// Exact Factory Run containing the predecessor Work.
  pub run_id: FactoryRunId,
  /// Exact predecessor envelope identity.
  pub work_id: WorkEnvelopeId,
  /// Content identity of the immutable predecessor envelope.
  pub work_digest: FactoryDigest,
  /// Declared root Flow terminal required before selection.
  pub terminal: FactoryKey,
}

/// Trusted immutable inputs attached to an authoritative ready Flow node.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PhasePoolInput {
  /// Enclosing Work execution.
  pub run_id: FactoryRunId,
  /// Optimistic snapshot precondition.
  pub run_version: FactoryRunVersion,
  /// Exact nested or root Flow Run owning the ready node.
  pub flow_run_id: FlowRunId,
  /// Exact current workflow cycle.
  pub cycle_id: WorkflowCycleId,
  /// Declared node made ready by persisted control outcomes.
  pub node: FactoryKey,
  /// Accepted severity observation, never a model-selected queue position.
  pub severity: FindingSeverity,
  /// Frozen operator-owned Project priority.
  pub project_priority: i32,
  /// Exact retained triage, handoff, or phase-policy input digest.
  pub phase_input_digest: FactoryDigest,
  /// Exact capability identities required by this phase.
  pub capabilities: BTreeSet<ImmutableReference>,
  /// Canonically ordered exact predecessor requirements.
  pub dependencies: Vec<PhasePoolDependency>,
  /// Hard resource reservation needed before dispatch.
  pub reservation: BudgetUsage,
}

/// Immutable ready projection; stores reconstruct it before accepting publication or selection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PhasePoolEntry {
  /// Content-addressed entry identity, including exact snapshot and policy.
  pub id: FactoryDigest,
  /// Exact immutable pool policy identity.
  pub policy_digest: FactoryDigest,
  /// Work identity used as an immutable ordering tie breaker.
  pub work_id: WorkEnvelopeId,
  /// Project owning the exact subject.
  pub project_id: ProjectId,
  /// Original admission time; refresh never changes age.
  pub admitted_at: Timestamp,
  /// Complete authoritative Flow snapshot digest.
  pub snapshot_digest: FactoryDigest,
  /// Validated frozen selection inputs.
  pub input: PhasePoolInput,
}

/// One immutable fenced selection and its frozen input evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PhasePoolSelection {
  /// Entry selected at most once.
  pub entry: PhasePoolEntry,
  /// Exact selection-pass inputs, including ordered candidate and capacity observations.
  pub input_digest: FactoryDigest,
  /// Stable client/worker request identity retained for lost-response replay.
  pub request_id: FactoryDigest,
  /// Exclusive owner.
  pub owner: FactoryKey,
  /// Fenced window used by ordinary Factory transitions.
  pub claim: FactoryClaim,
}

impl PhasePoolSelection {
  /// Returns the ordinary Factory claim atomically committed with this selection.
  #[must_use]
  pub fn run_claim(&self) -> FactoryRunClaimRecord {
    FactoryRunClaimRecord::new(self.entry.input.run_id, self.owner.clone(), self.claim)
  }
}

/// Bounded selection pass; replay returns the original batch, including an empty batch.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SelectPhasePool {
  /// Exact immutable pool policy.
  pub policy_digest: FactoryDigest,
  /// Stable operation identity.
  pub request_id: FactoryDigest,
  /// Bounded owner identity.
  pub owner: FactoryKey,
  /// Authoritative selection time.
  pub observed_at: Timestamp,
  /// Exclusive ownership deadline.
  pub expires_at: Timestamp,
  /// Exact available capabilities; selection cannot substitute a version or digest.
  pub capabilities: BTreeSet<ImmutableReference>,
  /// Positive bounded selected count.
  pub limit: u16,
}

impl SelectPhasePool {
  /// Revalidates mutable requests at the store boundary.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.limit == 0
      || self.limit > MAX_PHASE_POOL_BATCH
      || self.expires_at <= self.observed_at
      || self.capabilities.len() > 64
    {
      return Err(pool_invalid());
    }
    Ok(())
  }
}

/// Operation-shaped durable pool contract, separate from Agent placement and run reconciliation.
#[async_trait]
pub trait FactoryPhasePoolStore: Send + Sync {
  /// Publishes a current ready projection after reconstructing authoritative Flow readiness.
  async fn publish_phase_ready(
    &self,
    policy: PhasePoolPolicy,
    input: PhasePoolInput,
  ) -> Result<PhasePoolEntry, StoreError>;
  /// Returns one bounded deterministic current projection, excluding selected and stale entries.
  async fn phase_ready_entries(
    &self,
    policy: FactoryDigest,
    after: Option<FactoryDigest>,
    limit: u16,
  ) -> Result<Vec<PhasePoolEntry>, StoreError>;
  /// Atomically checks readiness, reserves capacity, appends selection/audit, and acquires run claims.
  async fn select_phase_ready(&self, request: SelectPhasePool) -> Result<Vec<PhasePoolSelection>, StoreError>;
}

/// Reconstructs one pool entry from immutable policy and current authoritative Flow facts.
pub fn derive_phase_pool_entry(
  policy: &PhasePoolPolicy,
  input: PhasePoolInput,
  snapshot: &FactoryRunSnapshot,
) -> Result<PhasePoolEntry, StoreError> {
  policy.validate().map_err(|_| pool_invalid())?;
  let mut require_projected_input = false;
  if let Some(triage) = snapshot.admitted_flow.triage() {
    let Some(octacity_server_factory::TriageJournalRecord::Classification(row)) = snapshot.flow.triage.last() else {
      return Err(pool_invalid());
    };
    let route = row.decision.route;
    let (&pool_route, _) = triage
      .pools
      .iter()
      .find(|(_, declared)| *declared == policy)
      .ok_or_else(pool_invalid)?;
    let root = snapshot
      .admitted_flow
      .closure()
      .definition(snapshot.admitted_flow.closure().root())
      .ok_or_else(pool_invalid)?;
    let target = root
      .transitions()
      .iter()
      .find(|edge| edge.predecessor() == root.entry() && edge.outcome().as_str() == route.as_str())
      .map(|edge| edge.target());
    let mut observed_dependencies = row
      .result
      .classification()
      .dependencies
      .iter()
      .map(|row| (row.value().work_id, row.value().work_digest))
      .collect::<Vec<_>>();
    let mut ready_dependencies = input
      .dependencies
      .iter()
      .map(|row| (row.work_id, row.work_digest))
      .collect::<Vec<_>>();
    observed_dependencies.sort();
    ready_dependencies.sort();
    // Intake's first successor consumes the accepted triage disposition. Later
    // phases consume an exact completion through a declared typed projection.
    let first_successor = target == Some(&octacity_server_factory::FlowTransitionTarget::Node(input.node.clone()));
    if input.flow_run_id != snapshot.admitted_flow.root_run().id()
      || first_successor && pool_route != route
      || first_successor
        && input.phase_input_digest
          != octacity_server_factory::TriageDisposition::Classification(row.decision.clone())
            .digest()
            .map_err(|_| pool_invalid())?
      || input.severity != *row.result.classification().severity.value()
      || input.project_priority != triage.project_priority
      || input.capabilities != triage.capabilities.get(&pool_route).cloned().unwrap_or_default()
      || observed_dependencies != ready_dependencies
    {
      return Err(pool_invalid());
    }
    require_projected_input = !first_successor;
  }
  let checkpoint = snapshot
    .lifecycle_checkpoints
    .iter()
    .find(|row| row.id == snapshot.current.lifecycle_checkpoint_id)
    .ok_or(StoreError::Unavailable)?;
  if input.run_id != snapshot.run.id()
    || input.run_version != snapshot.run.version()
    || snapshot.run.state().is_terminal()
    || checkpoint.cancellation_requested
    || input.capabilities.len() > 16
    || input.dependencies.len() > 64
    || input.reservation.attempts == 0
    || input
      .dependencies
      .windows(2)
      .any(|pair| pair[0].run_id >= pair[1].run_id)
    || input
      .dependencies
      .iter()
      .any(|dep| dep.run_id == input.run_id || dep.work_id == snapshot.work.id())
  {
    return Err(pool_invalid());
  }
  let flow = snapshot
    .flow
    .runs
    .iter()
    .find(|run| run.id() == input.flow_run_id)
    .ok_or_else(pool_invalid)?;
  let cycle = snapshot
    .flow
    .cycles
    .iter()
    .filter(|cycle| cycle.flow_run_id() == flow.id())
    .max_by_key(|cycle| cycle.number())
    .ok_or_else(pool_invalid)?;
  if cycle.id() != input.cycle_id
    || snapshot
      .flow
      .attempts
      .iter()
      .any(|attempt| attempt.workflow_cycle_id() == cycle.id() && attempt.node_key() == &input.node)
  {
    return Err(pool_invalid());
  }
  let validated = snapshot.admitted_flow.validated().map_err(|_| pool_invalid())?;
  let definition = validated
    .closure()
    .definition(flow.definition())
    .ok_or_else(pool_invalid)?;
  let node = definition.node(&input.node).ok_or_else(pool_invalid)?;
  if node.phase_pool().is_some_and(|phase| phase != &policy.phase)
    || require_projected_input && node.phase_pool().is_none()
  {
    return Err(pool_invalid());
  }
  input.reservation.validate(node.budget()).map_err(|_| pool_invalid())?;
  let interpreter = FlowInterpreter::new(&validated);
  let matches = |directive: &FlowDirective| match directive {
    FlowDirective::ExecuteNode { node, .. } | FlowDirective::EnterSubflow { node, .. } => node == &input.node,
    FlowDirective::Complete { .. } => false,
  };
  let ready = if &input.node == definition.entry() {
    matches(&interpreter.start(flow).map_err(|_| pool_invalid())?)
  } else {
    let mut ready = false;
    for completion in snapshot
      .flow
      .completions
      .iter()
      .filter(|completion| completion.workflow_cycle_id() == cycle.id())
    {
      let attempt = snapshot
        .flow
        .attempts
        .iter()
        .find(|attempt| attempt.id() == completion.node_attempt_id())
        .ok_or_else(pool_invalid)?;
      let directives = interpreter
        .advance(
          flow,
          cycle,
          attempt,
          completion,
          &snapshot.flow.attempts,
          &snapshot.flow.completions,
        )
        .map_err(|_| pool_invalid())?;
      ready |= directives.iter().any(|directive| {
        if !matches(directive) {
          return false;
        }
        if !require_projected_input {
          return true;
        }
        let inputs = match directive {
          FlowDirective::ExecuteNode { inputs, .. } | FlowDirective::EnterSubflow { inputs, .. } => inputs,
          FlowDirective::Complete { .. } => return false,
        };
        completion.output_digest() == input.phase_input_digest
          && inputs.data.iter().any(|projection| {
            projection.predecessor() == attempt.node_key()
              && projection.outcome() == completion.outcome()
              && projection.successor() == &input.node
              && projection.schema() == completion.output_schema()
          })
      });
    }
    ready
  };
  if !ready {
    return Err(pool_invalid());
  }
  let admitted_at = snapshot
    .lifecycle_checkpoints
    .iter()
    .min_by_key(|row| row.run_version)
    .ok_or(StoreError::Unavailable)?
    .recorded_at;
  let snapshot_digest = digest(
    "phase-pool-snapshot",
    &(
      &snapshot.work,
      &snapshot.run,
      &snapshot.admitted_flow,
      &snapshot.flow.runs,
      &snapshot.flow.cycles,
      &snapshot.flow.attempts,
      &snapshot.flow.completions,
      snapshot.current.candidate_id,
    ),
  );
  let mut entry = PhasePoolEntry {
    id: FactoryDigest::from_bytes([0; 32]),
    policy_digest: policy.digest(),
    work_id: snapshot.work.id(),
    project_id: snapshot.work.subject().project_id(),
    admitted_at,
    snapshot_digest,
    input,
  };
  entry.id = digest("phase-pool-entry", &entry);
  Ok(entry)
}

/// Shared ordering used by both persistence adapters; refresh never supplies a queue position.
#[must_use]
pub fn compare_phase_pool_entries(policy: &PhasePoolPolicy, left: &PhasePoolEntry, right: &PhasePoolEntry) -> Ordering {
  for dimension in &policy.order {
    let order = match dimension {
      PhasePoolOrder::Severity => right.input.severity.cmp(&left.input.severity),
      PhasePoolOrder::ProjectPriority => right.input.project_priority.cmp(&left.input.project_priority),
      PhasePoolOrder::Age => left.admitted_at.cmp(&right.admitted_at),
    };
    if order != Ordering::Equal {
      return order;
    }
  }
  (
    left.work_id,
    left.input.run_id,
    left.input.flow_run_id,
    left.input.cycle_id,
    &left.input.node,
    left.id,
  )
    .cmp(&(
      right.work_id,
      right.input.run_id,
      right.input.flow_run_id,
      right.input.cycle_id,
      &right.input.node,
      right.id,
    ))
}

/// Evaluates exact dependency resolution from authoritative retained root Flow history.
#[must_use]
pub fn phase_pool_dependency_ready(
  dependency: &PhasePoolDependency,
  project: ProjectId,
  snapshot: &FactoryRunSnapshot,
) -> bool {
  if dependency.run_id != snapshot.run.id()
    || dependency.work_id != snapshot.work.id()
    || snapshot.work.subject().project_id() != project
    || dependency.work_digest != phase_pool_work_digest(snapshot)
    || snapshot.run.state() == octacity_server_factory::FactoryRunState::Cancelled
    || snapshot
      .lifecycle_checkpoints
      .iter()
      .find(|row| row.id == snapshot.current.lifecycle_checkpoint_id)
      .is_none_or(|row| row.cancellation_requested)
  {
    return false;
  }
  let Ok(validated) = snapshot.admitted_flow.validated() else {
    return false;
  };
  let root = snapshot.admitted_flow.root_run();
  let Some(cycle) = snapshot
    .flow
    .cycles
    .iter()
    .filter(|cycle| cycle.flow_run_id() == root.id())
    .max_by_key(|cycle| cycle.number())
  else {
    return false;
  };
  snapshot.flow.completions.iter().filter(|row| row.workflow_cycle_id() == cycle.id()).any(|completion| {
    snapshot.flow.attempts.iter().find(|attempt| attempt.id() == completion.node_attempt_id()).is_some_and(|attempt| {
      FlowInterpreter::new(&validated).advance(root, cycle, attempt, completion, &snapshot.flow.attempts, &snapshot.flow.completions)
        .is_ok_and(|directives| directives.iter().any(|directive| matches!(directive, FlowDirective::Complete { terminal, .. } if terminal == &dependency.terminal)))
    })
  })
}

/// Canonical exact predecessor envelope identity.
#[must_use]
pub fn phase_pool_work_digest(snapshot: &FactoryRunSnapshot) -> FactoryDigest {
  digest("phase-pool-work", &snapshot.work)
}

/// Tests run-wide remaining hard budget before any capacity is reserved.
#[must_use]
pub fn phase_pool_run_budget_ready(entry: &PhasePoolEntry, snapshot: &FactoryRunSnapshot) -> bool {
  let Some(usage) = snapshot
    .budgets
    .iter()
    .find(|row| row.id == snapshot.current.budget_id)
    .map(|row| row.usage)
  else {
    return false;
  };
  let limit = snapshot.admitted_flow.limits().budget();
  usage.attempts < limit.max_attempts()
    && usage.elapsed_millis < limit.max_elapsed_millis()
    && usage.tokens < limit.max_tokens()
    && usage.cost_micro_units < limit.max_cost_micro_units()
    && usage.output_bytes < limit.max_output_bytes()
    && reserve_phase_budget(usage, entry.input.reservation, limit).is_some()
}

/// Constructs a stable fenced selection from frozen pass evidence.
pub fn phase_pool_selection(
  entry: PhasePoolEntry,
  request: &SelectPhasePool,
  input_digest: FactoryDigest,
) -> Result<PhasePoolSelection, StoreError> {
  let fence = FactoryClaimFence::new(digest("phase-pool-fence", &(entry.id, request, input_digest)));
  let claim = FactoryClaim::new(fence, request.observed_at, request.expires_at).map_err(|_| pool_invalid())?;
  Ok(PhasePoolSelection {
    entry,
    input_digest,
    request_id: request.request_id,
    owner: request.owner.clone(),
    claim,
  })
}

pub(crate) fn digest<T: Serialize>(domain: &str, value: &T) -> FactoryDigest {
  FactoryDigest::sha256(
    domain,
    &[&serde_json::to_vec(value).expect("typed pool input serializes")],
  )
}

pub(crate) fn pool_invalid() -> StoreError {
  StoreError::invalid(
    StoreOperation::SelectFactoryPhasePool,
    StoreInputError::InvalidFactoryPhasePool,
  )
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) fn pool_conflict() -> StoreError {
  StoreError::Conflict {
    entity: octacity_server_domain::EntityKind::FactoryRun,
  }
}

/// Frozen current observation used by deterministic selection, including skipped entries.
#[derive(Clone, Debug, Serialize)]
pub struct PhasePoolCandidate {
  /// Retained immutable ready entry.
  pub entry: PhasePoolEntry,
  /// Exact current observations, including dependency snapshots and budgets.
  pub observation_digest: FactoryDigest,
  /// Whether all current safety and readiness checks succeeded.
  pub ready: bool,
}

/// Revalidates a retained entry against current Flow, claim, budget, and dependency facts.
#[must_use]
pub fn observe_phase_pool_candidate(
  policy: &PhasePoolPolicy,
  entry: &PhasePoolEntry,
  snapshot: &FactoryRunSnapshot,
  dependencies: &[Option<FactoryRunSnapshot>],
  at: Timestamp,
) -> PhasePoolCandidate {
  let dependencies_ready = entry.input.dependencies.len() == dependencies.len()
    && entry
      .input
      .dependencies
      .iter()
      .zip(dependencies)
      .all(|(dep, snapshot)| {
        snapshot
          .as_ref()
          .is_some_and(|snapshot| phase_pool_dependency_ready(dep, entry.project_id, snapshot))
      });
  let ready = derive_phase_pool_entry(policy, entry.input.clone(), snapshot).is_ok_and(|current| current == *entry)
    && snapshot
      .current_claim
      .as_ref()
      .is_none_or(|claim| claim.claim.expires_at() <= at)
    && at >= entry.admitted_at
    && phase_pool_run_budget_ready(entry, snapshot)
    && phase_pool_run_wip_ready(entry, snapshot)
    && dependencies_ready;
  let dependency_observations = dependencies
    .iter()
    .map(|snapshot| {
      snapshot.as_ref().map(|snapshot| {
        (
          &snapshot.run,
          &snapshot.flow.completions,
          snapshot.current.lifecycle_checkpoint_id,
        )
      })
    })
    .collect::<Vec<_>>();
  PhasePoolCandidate {
    entry: entry.clone(),
    ready,
    observation_digest: digest(
      "phase-pool-observation",
      &(
        &snapshot.run,
        &snapshot.flow.attempts,
        &snapshot.flow.completions,
        snapshot.current_claim.as_ref().map(|row| (&row.owner, row.claim)),
        snapshot
          .budgets
          .iter()
          .find(|row| row.id == snapshot.current.budget_id)
          .map(|row| row.usage),
        dependency_observations,
      ),
    ),
  }
}

fn phase_pool_run_wip_ready(entry: &PhasePoolEntry, snapshot: &FactoryRunSnapshot) -> bool {
  let active = snapshot
    .flow
    .attempts
    .iter()
    .filter(|attempt| {
      !snapshot
        .flow
        .completions
        .iter()
        .any(|row| row.node_attempt_id() == attempt.id())
    })
    .collect::<Vec<_>>();
  let Some(flow) = snapshot
    .flow
    .runs
    .iter()
    .find(|run| run.id() == entry.input.flow_run_id)
  else {
    return false;
  };
  let Some(definition) = snapshot.admitted_flow.closure().definition(flow.definition()) else {
    return false;
  };
  active.len() < usize::from(snapshot.admitted_flow.limits().max_wip())
    && active
      .iter()
      .filter(|attempt| attempt.flow_run_id() == flow.id())
      .count()
      < usize::from(definition.execution().max_active_nodes())
}

/// Determines whether a selected Work still consumes pool WIP and reservation budget.
///
/// A selected target continues to reserve capacity after a worker claim expires.
/// Recovery takes over ordinary run ownership without selecting Work again or
/// opening a second capacity slot. Completion, cancellation or a new cycle releases it.
#[must_use]
pub fn phase_pool_selection_active(
  selection: &PhasePoolSelection,
  snapshot: &FactoryRunSnapshot,
  _at: Timestamp,
) -> bool {
  if snapshot.run.state().is_terminal()
    || snapshot
      .lifecycle_checkpoints
      .iter()
      .find(|row| row.id == snapshot.current.lifecycle_checkpoint_id)
      .is_none_or(|row| row.cancellation_requested)
  {
    return false;
  }
  let targets = snapshot
    .flow
    .attempts
    .iter()
    .filter(|attempt| {
      attempt.workflow_cycle_id() == selection.entry.input.cycle_id && attempt.node_key() == &selection.entry.input.node
    })
    .collect::<Vec<_>>();
  if targets.is_empty() {
    return snapshot
      .flow
      .cycles
      .iter()
      .filter(|cycle| cycle.flow_run_id() == selection.entry.input.flow_run_id)
      .max_by_key(|cycle| cycle.number())
      .is_some_and(|cycle| cycle.id() == selection.entry.input.cycle_id);
  }
  targets.iter().any(|attempt| {
    !snapshot
      .flow
      .completions
      .iter()
      .any(|completion| completion.node_attempt_id() == attempt.id())
  })
}

/// Pure bounded ordering and capacity intersection over frozen authoritative observations.
pub fn plan_phase_pool_selection(
  policy: &PhasePoolPolicy,
  request: &SelectPhasePool,
  candidates: &[PhasePoolCandidate],
  active: &[PhasePoolSelection],
) -> Result<Vec<PhasePoolSelection>, StoreError> {
  request.validate()?;
  policy.validate().map_err(|_| pool_invalid())?;
  if request.policy_digest != policy.digest()
    || candidates.len() > usize::from(MAX_PHASE_POOL_BATCH)
    || active.len() > usize::from(MAX_PHASE_POOL_WIP)
    || candidates.iter().any(|row| row.entry.policy_digest != policy.digest())
    || active.iter().any(|row| row.entry.policy_digest != policy.digest())
  {
    return Err(pool_invalid());
  }
  let mut ordered = candidates.to_vec();
  ordered.sort_by(|left, right| compare_phase_pool_entries(policy, &left.entry, &right.entry));
  let pass_digest = digest("phase-pool-selection-input", &(policy, request, &ordered, active));
  let mut usage = BudgetUsage::default();
  let mut projects = std::collections::BTreeMap::<ProjectId, usize>::new();
  for selection in active {
    usage = reserve_phase_budget(usage, selection.entry.input.reservation, policy.budget).ok_or_else(pool_invalid)?;
    *projects.entry(selection.entry.project_id).or_default() += 1;
  }
  let mut selected = Vec::new();
  let mut runs = BTreeSet::new();
  for candidate in ordered {
    if selected.len() >= usize::from(request.limit) || active.len() + selected.len() >= usize::from(policy.max_wip) {
      break;
    }
    let entry = candidate.entry;
    if !candidate.ready
      || !entry.input.capabilities.is_subset(&request.capabilities)
      || projects.get(&entry.project_id).copied().unwrap_or_default() >= usize::from(policy.max_project_wip)
      || runs.contains(&entry.input.run_id)
    {
      continue;
    }
    let Some(next_usage) = reserve_phase_budget(usage, entry.input.reservation, policy.budget) else {
      continue;
    };
    usage = next_usage;
    runs.insert(entry.input.run_id);
    *projects.entry(entry.project_id).or_default() += 1;
    selected.push(phase_pool_selection(entry, request, pass_digest)?);
  }
  Ok(selected)
}
