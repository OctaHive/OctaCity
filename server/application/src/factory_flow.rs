//! Bounded configuration-driven graph ownership over the common node executor.
use crate::*;
use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use octacity_server_store::*;
use std::sync::Arc;

/// Trusted input owner for explicitly selected Context and immutable incoming data.
/// It may resolve protected bytes, but cannot change the admitted schema or graph.
#[async_trait]
pub trait FactoryFlowInputPreparer: Send + Sync {
  /// Prepares the configured node projection and its explicitly selected Context.
  async fn prepare(
    &self,
    snapshot: &FactoryRunSnapshot,
    target: &FactoryNodeTarget,
  ) -> Result<FlowNodeInput, ApplicationError>;
}
/// One graph reconciler shared by supplied templates and operator-authored Flows.
/// Each pass starts, observes or publishes readiness for at most one node.
pub struct FactoryFlowRunner<S, E, I> {
  store: Arc<S>,
  nodes: FactoryNodeRunner<S, E>,
  inputs: Arc<I>,
}
impl<S: FactoryRunStore + FactoryPhasePoolStore, E: FactoryNodeExecutor, I: FactoryFlowInputPreparer>
  FactoryFlowRunner<S, E, I>
{
  /// Connects graph persistence, trusted execution capabilities and input preparation.
  pub fn new(store: Arc<S>, executor: Arc<E>, inputs: Arc<I>) -> Self {
    Self {
      nodes: FactoryNodeRunner::new(store.clone(), executor),
      store,
      inputs,
    }
  }
}
#[async_trait]
impl<S: FactoryRunStore + FactoryPhasePoolStore, E: FactoryNodeExecutor, I: FactoryFlowInputPreparer>
  FactoryFlowCoordinator for FactoryFlowRunner<S, E, I>
{
  async fn reconcile_flow(
    &self,
    run: FactoryRunId,
    claim: FactoryDigest,
    at: Timestamp,
  ) -> Result<FactoryFlowStep, ApplicationError> {
    let snapshot = self.store.factory_run_snapshot(run).await?;
    let ownership = snapshot
      .current_claim
      .as_ref()
      .filter(|row| row.id == claim)
      .ok_or_else(ApplicationError::invalid)?;
    ownership
      .claim
      .verify_fence(ownership.claim.fence(), at)
      .map_err(invalid)?;
    if snapshot.run.state().is_terminal()
      || snapshot
        .lifecycle_checkpoints
        .iter()
        .find(|row| row.id == snapshot.current.lifecycle_checkpoint_id)
        .is_none_or(|row| row.cancellation_requested)
    {
      return Err(ApplicationError::invalid());
    }
    let target = match next(&snapshot)? {
      Next::Node(target) => target,
      Next::Terminal(terminal) => return Ok(FactoryFlowStep::Resolved(terminal)),
      Next::Waiting => return Ok(FactoryFlowStep::Waiting),
    };
    let definition = snapshot
      .flow
      .runs
      .iter()
      .find(|flow| flow.id() == target.flow_run_id)
      .and_then(|flow| snapshot.admitted_flow.closure().definition(flow.definition()))
      .ok_or_else(ApplicationError::invalid)?;
    let node = definition.node(&target.node).ok_or_else(ApplicationError::invalid)?;
    let pending = snapshot.flow.attempts.iter().any(|attempt| {
      attempt.flow_run_id() == target.flow_run_id
        && attempt.workflow_cycle_id() == target.cycle_id
        && attempt.node_key() == &target.node
        && !snapshot
          .flow
          .completions
          .iter()
          .any(|row| row.node_attempt_id() == attempt.id())
    });
    let mut input = None;
    if !pending {
      if let Some(pool) = node.phase_pool() {
        let generation = u32::try_from(
          snapshot
            .flow
            .attempts
            .iter()
            .filter(|row| {
              row.flow_run_id() == target.flow_run_id
                && row.workflow_cycle_id() == target.cycle_id
                && row.node_key() == &target.node
            })
            .count(),
        )
        .map_err(|_| ApplicationError::invalid())?;
        let frozen = snapshot.flow.data.inputs.iter().find(|input| {
          input.flow_run_id() == target.flow_run_id
            && input.cycle_id() == target.cycle_id
            && input.node() == &target.node
            && input.generation() == generation
        });
        let Some(frozen) = frozen else {
          let input = self.inputs.prepare(&snapshot, &target).await?;
          let mut append = FactoryRunHistoryAppend::default();
          append.flow.data.inputs.push(input);
          self.nodes.commit(&snapshot, append, at).await?;
          return Ok(FactoryFlowStep::Advanced);
        };
        let selected = self.store.phase_pool_selection_for_claim(run, claim).await?;
        let input_digest = frozen.digest().map_err(invalid)?;
        if selected.as_ref().is_none_or(|selection| {
          selection.entry.input.flow_run_id != target.flow_run_id
            || selection.entry.input.cycle_id != target.cycle_id
            || selection.entry.input.node != target.node
            || selection.entry.input.phase_input_digest != input_digest
            || selection.entry.input.generation != generation
        }) {
          let settings = snapshot
            .admitted_flow
            .pool_settings()
            .get(pool)
            .ok_or_else(ApplicationError::invalid)?;
          let entry = self
            .store
            .publish_phase_ready(settings.policy.clone(), configured_phase_pool_input(&snapshot, frozen)?)
            .await?;
          return Ok(FactoryFlowStep::Ready(Box::new(entry)));
        }
        input = Some(frozen.clone());
      } else if node.kind() != FlowNodeKind::DeterministicGate {
        input = Some(self.inputs.prepare(&snapshot, &target).await?);
      }
    }
    Ok(match self.nodes.run_once(run, claim, target, input, at).await? {
      FactoryNodeStep::Advanced | FactoryNodeStep::Completed(_) => FactoryFlowStep::Advanced,
      FactoryNodeStep::Waiting | FactoryNodeStep::ExecutionStopped => FactoryFlowStep::Waiting,
    })
  }
}
enum Next {
  Node(FactoryNodeTarget),
  Terminal(FactoryKey),
  Waiting,
}
fn next(snapshot: &FactoryRunSnapshot) -> Result<Next, ApplicationError> {
  next_in_flow(snapshot, snapshot.admitted_flow.root_run())
}
fn next_in_flow(snapshot: &FactoryRunSnapshot, flow: &FlowRun) -> Result<Next, ApplicationError> {
  let validated = snapshot.admitted_flow.validated().map_err(invalid)?;
  let cycle = snapshot
    .flow
    .cycles
    .iter()
    .filter(|row| row.flow_run_id() == flow.id())
    .max_by_key(|row| row.number())
    .ok_or_else(ApplicationError::invalid)?;
  let history = FlowRuntimeHistory {
    flow_runs: &snapshot.flow.runs,
    cycles: &snapshot.flow.cycles,
    attempts: &snapshot.flow.attempts,
    completions: &snapshot.flow.completions,
  };
  match FlowInterpreter::new(&validated)
    .progress(flow, cycle, history)
    .map_err(invalid)?
  {
    FlowProgress::Node(node) => {
      let target = FactoryNodeTarget {
        flow_run_id: flow.id(),
        cycle_id: cycle.id(),
        node: node.clone(),
      };
      let pending = snapshot.flow.attempts.iter().find(|row| {
        row.flow_run_id() == flow.id()
          && row.workflow_cycle_id() == cycle.id()
          && row.node_key() == &node
          && row.node_kind() == FlowNodeKind::SubflowCall
          && !snapshot
            .flow
            .completions
            .iter()
            .any(|done| done.node_attempt_id() == row.id())
      });
      if let Some(call) = pending {
        let child = snapshot
          .flow
          .runs
          .iter()
          .find(|row| row.parent() == Some(FlowRunParent::new(flow.id(), call.id())))
          .ok_or_else(ApplicationError::invalid)?;
        return Ok(match next_in_flow(snapshot, child)? {
          Next::Terminal(_) => Next::Node(target),
          other => other,
        });
      }
      Ok(Next::Node(target))
    }
    FlowProgress::Terminal { key, .. } => Ok(Next::Terminal(key)),
    FlowProgress::Waiting => Ok(Next::Waiting),
  }
}
fn invalid(_: FactoryError) -> ApplicationError {
  ApplicationError::invalid()
}
