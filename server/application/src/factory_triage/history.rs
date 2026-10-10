use crate::ApplicationError;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use octacity_server_store::*;
pub(crate) struct TypedFlowOutput {
  pub outcome: FactoryKey,
  pub schema: ImmutableReference,
  pub digest: FactoryDigest,
  pub usage: BudgetUsage,
}
pub(super) struct TriageGate {
  pub node: TriageNode,
  pub input: FactoryDigest,
  pub output: TypedFlowOutput,
}

pub(super) fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).expect("static triage key")
}
pub(super) fn invalid(_: FactoryError) -> ApplicationError {
  ApplicationError::invalid()
}
pub(super) fn payload_digest(value: &TriageDisposition) -> Result<FactoryDigest, ApplicationError> {
  value.digest().map_err(invalid)
}

pub(super) fn current_usage(snapshot: &FactoryRunSnapshot) -> Result<BudgetUsage, ApplicationError> {
  snapshot
    .budgets
    .iter()
    .find(|row| row.id == snapshot.current.budget_id)
    .map(|row| row.usage)
    .ok_or_else(ApplicationError::invalid)
}
pub(super) fn cancellation(snapshot: &FactoryRunSnapshot) -> Result<bool, ApplicationError> {
  snapshot
    .lifecycle_checkpoints
    .iter()
    .find(|row| row.id == snapshot.current.lifecycle_checkpoint_id)
    .map(|row| row.cancellation_requested)
    .ok_or_else(ApplicationError::invalid)
}
pub(super) fn cycle_for(history: &FactoryFlowHistory, flow: &FlowRun) -> Result<WorkflowCycle, ApplicationError> {
  history
    .cycles
    .iter()
    .filter(|row| row.flow_run_id() == flow.id())
    .max_by_key(|row| row.number())
    .cloned()
    .ok_or_else(ApplicationError::invalid)
}
pub(super) fn triage_run(snapshot: &FactoryRunSnapshot) -> Result<FlowRun, ApplicationError> {
  let triage = snapshot.admitted_flow.triage().ok_or_else(ApplicationError::invalid)?;
  snapshot
    .flow
    .runs
    .iter()
    .find(|run| run.definition() == triage.definition)
    .cloned()
    .ok_or_else(ApplicationError::invalid)
}
pub(super) fn phase_run(snapshot: &FactoryRunSnapshot, role: TriageNode) -> Result<FlowRun, ApplicationError> {
  let triage = triage_run(snapshot)?;
  let attempt = snapshot
    .flow
    .attempts
    .iter()
    .find(|row| row.flow_run_id() == triage.id() && row.node_key().as_str() == role.as_str())
    .ok_or_else(ApplicationError::invalid)?;
  snapshot
    .flow
    .runs
    .iter()
    .find(|run| {
      run
        .parent()
        .is_some_and(|parent| parent.node_attempt_id() == attempt.id())
    })
    .cloned()
    .ok_or_else(ApplicationError::invalid)
}
pub(crate) fn difference(before: &FactoryFlowHistory, after: &FactoryFlowHistory) -> FactoryFlowHistory {
  FactoryFlowHistory {
    runs: after
      .runs
      .iter()
      .filter(|row| !before.runs.contains(row))
      .cloned()
      .collect(),
    cycles: after
      .cycles
      .iter()
      .filter(|row| !before.cycles.contains(row))
      .cloned()
      .collect(),
    attempts: after
      .attempts
      .iter()
      .filter(|row| !before.attempts.contains(row))
      .cloned()
      .collect(),
    completions: after
      .completions
      .iter()
      .filter(|row| !before.completions.contains(row))
      .cloned()
      .collect(),
    triage: Vec::new(),
  }
}

pub(crate) fn make_attempt(
  snapshot: &FactoryRunSnapshot,
  history: &mut FactoryFlowHistory,
  flow: &FlowRun,
  cycle: &WorkflowCycle,
  node: &FactoryKey,
  input: FactoryDigest,
  executor: NodeExecutionIdentity,
) -> Result<NodeAttempt, ApplicationError> {
  let definition = snapshot
    .admitted_flow
    .closure()
    .definition(flow.definition())
    .ok_or_else(ApplicationError::invalid)?;
  let declaration = definition.node(node).ok_or_else(ApplicationError::invalid)?;
  let claim = snapshot.current_claim.as_ref().ok_or_else(ApplicationError::invalid)?;
  let number = NodeAttemptNumber::new(
    u64::try_from(
      history
        .attempts
        .iter()
        .filter(|row| row.flow_run_id() == flow.id())
        .count(),
    )
    .map_err(|_| ApplicationError::invalid())?
      + 1,
  )
  .map_err(invalid)?;
  let attempt = NodeAttempt::new(
    flow,
    cycle,
    definition,
    NodeAttemptInput {
      id: NodeAttemptId::generate(),
      node_key: node.clone(),
      node_kind: declaration.kind(),
      number,
      input_digest: input,
      budget: declaration.budget(),
      deadline: claim.claim.expires_at(),
      execution: executor,
      ownership: FactoryClaimOwnership::new(claim.owner.clone(), claim.claim),
    },
  )
  .map_err(invalid)?;
  history.attempts.push(attempt.clone());
  Ok(attempt)
}
pub(crate) fn enter(
  snapshot: &FactoryRunSnapshot,
  history: &mut FactoryFlowHistory,
  parent: &FlowRun,
  cycle: &WorkflowCycle,
  node: &FactoryKey,
  input: FactoryDigest,
) -> Result<FlowRun, ApplicationError> {
  let attempt = make_attempt(
    snapshot,
    history,
    parent,
    cycle,
    node,
    input,
    NodeExecutionIdentity::BuiltIn,
  )?;
  let definition = snapshot
    .admitted_flow
    .closure()
    .definition(parent.definition())
    .and_then(|def| def.node(node))
    .and_then(FlowNodeDefinition::subflow_definition)
    .ok_or_else(ApplicationError::invalid)?;
  let child = FlowRun::nested(
    FlowRunId::generate(),
    snapshot.run.id(),
    definition,
    FlowRunParent::new(parent.id(), attempt.id()),
  );
  history.cycles.push(WorkflowCycle::initial(&child).map_err(invalid)?);
  history.runs.push(child.clone());
  Ok(child)
}
pub(crate) fn complete(
  snapshot: &FactoryRunSnapshot,
  history: &mut FactoryFlowHistory,
  flow: &FlowRun,
  attempt: &NodeAttempt,
  output: TypedFlowOutput,
  at: Timestamp,
) -> Result<(), ApplicationError> {
  let definition = snapshot
    .admitted_flow
    .closure()
    .definition(flow.definition())
    .ok_or_else(ApplicationError::invalid)?;
  let claim = snapshot.current_claim.as_ref().ok_or_else(ApplicationError::invalid)?;
  history.completions.push(
    NodeAttemptCompletion::new(
      attempt,
      definition,
      NodeAttemptCompletionInput {
        outcome: output.outcome,
        output_schema: output.schema,
        output_digest: output.digest,
        ownership: FactoryClaimOwnership::new(claim.owner.clone(), claim.claim),
        usage: output.usage,
        observed_at: at,
      },
    )
    .map_err(invalid)?,
  );
  Ok(())
}
pub(super) fn gate(
  snapshot: &FactoryRunSnapshot,
  history: &mut FactoryFlowHistory,
  flow: &FlowRun,
  cycle: &WorkflowCycle,
  gate: TriageGate,
  at: Timestamp,
) -> Result<(), ApplicationError> {
  let attempt = make_attempt(
    snapshot,
    history,
    flow,
    cycle,
    &gate.node.key(),
    gate.input,
    NodeExecutionIdentity::BuiltIn,
  )?;
  complete(snapshot, history, flow, &attempt, gate.output, at)
}
pub(crate) fn complete_parent(
  snapshot: &FactoryRunSnapshot,
  history: &mut FactoryFlowHistory,
  child: &FlowRun,
  terminal: &FactoryKey,
  output: FactoryDigest,
  at: Timestamp,
) -> Result<(), ApplicationError> {
  let parent = child.parent().ok_or_else(ApplicationError::invalid)?;
  let flow = history
    .runs
    .iter()
    .find(|row| row.id() == parent.flow_run_id())
    .cloned()
    .ok_or_else(ApplicationError::invalid)?;
  let attempt = history
    .attempts
    .iter()
    .find(|row| row.id() == parent.node_attempt_id())
    .cloned()
    .ok_or_else(ApplicationError::invalid)?;
  let definition = snapshot
    .admitted_flow
    .closure()
    .definition(flow.definition())
    .ok_or_else(ApplicationError::invalid)?;
  let schema = definition
    .node(attempt.node_key())
    .and_then(|node| node.outcome(terminal))
    .ok_or_else(ApplicationError::invalid)?
    .schema()
    .clone();
  complete(
    snapshot,
    history,
    &flow,
    &attempt,
    TypedFlowOutput {
      outcome: terminal.clone(),
      schema,
      digest: output,
      usage: BudgetUsage::default(),
    },
    at,
  )
}
pub(super) fn verify_observation(
  snapshot: &FactoryRunSnapshot,
  phase: &FlowRun,
  provenance: &TriageProvenance,
  digest: FactoryDigest,
) -> Result<(), ApplicationError> {
  validate_triage_phase_observation(
    &snapshot.admitted_flow,
    phase,
    provenance,
    digest,
    FlowRuntimeHistory {
      flow_runs: &snapshot.flow.runs,
      cycles: &snapshot.flow.cycles,
      attempts: &snapshot.flow.attempts,
      completions: &snapshot.flow.completions,
    },
  )
  .map_err(invalid)
}
