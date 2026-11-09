//! Bounded configuration-driven graph ownership over the common node executor.
use crate::*;
use async_trait::async_trait;
use octacity_protocol::ScopedValidationAction;
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
pub struct FactoryFlowRunner<S, E, I: ?Sized> {
  store: Arc<S>,
  nodes: FactoryNodeRunner<S, E>,
  inputs: Arc<I>,
}

/// Resolves the Tester Job to its exact admitted Flow and invokes its bound runner.
pub struct FactoryScopedValidationService<S, E, I: ?Sized> {
  flow: Arc<FactoryFlowRunner<S, E, I>>,
  builds: Arc<dyn BuildQueryStore>,
  leases: Arc<dyn JobExecutionStore>,
}

impl<S, E, I: ?Sized> FactoryScopedValidationService<S, E, I> {
  /// Connects ordinary Build queries to the existing Flow owner.
  pub fn new(
    flow: Arc<FactoryFlowRunner<S, E, I>>,
    builds: Arc<dyn BuildQueryStore>,
    leases: Arc<dyn JobExecutionStore>,
  ) -> Self {
    Self { flow, builds, leases }
  }
}

#[async_trait]
impl<S, E, I> LeaseBoundScopedValidation for FactoryScopedValidationService<S, E, I>
where
  S: FactoryRunStore + FactoryPhasePoolStore,
  E: FactoryNodeExecutor,
  I: FactoryFlowInputPreparer + ?Sized,
{
  async fn run(
    &self,
    input: LeaseBoundScopedValidationInput,
  ) -> Result<FactoryScopedValidationVerdict, AgentToolActionError> {
    let job = self.builds.job(input.job_id).await?;
    let attempt = self.builds.attempt(job.attempt_id).await?;
    let build = self.builds.build(attempt.build_id).await?;
    let BuildOrigin::Factory(origin) = build.origin else {
      return Err(AgentToolActionError::InvalidRequest);
    };
    if attempt.number != input.attempt
      || job.terminal.is_some()
      || job
        .assignment
        .is_none_or(|assignment| assignment.agent_id != input.lease.agent_id)
      || self.builds.latest_attempt(attempt.build_id).await?.id != attempt.id
    {
      return Err(AgentToolActionError::Fenced);
    }
    self
      .flow
      .invoke_scoped_validation(&origin, &input, self.leases.as_ref())
      .await
  }
}

impl<S: FactoryRunStore + FactoryPhasePoolStore, E: FactoryNodeExecutor, I: FactoryFlowInputPreparer + ?Sized>
  FactoryFlowRunner<S, E, I>
{
  /// Connects graph persistence, trusted execution capabilities and input preparation.
  pub fn new(store: Arc<S>, executor: Arc<E>, inputs: Arc<I>, clock: Arc<dyn FactoryExecutionClock>) -> Self {
    Self {
      nodes: FactoryNodeRunner::new(store.clone(), executor, clock),
      store,
      inputs,
    }
  }
  /// Supplies the existing Project catalog without adding dependencies to static stages.
  pub fn with_profile_catalog(mut self, catalog: Arc<dyn FactoryConfigurationStore>) -> Self {
    self.nodes = self.nodes.with_profile_catalog(catalog);
    self
  }
  /// Supplies the optional separate provider consumer; manual and static stages remain independent.
  pub fn with_profile_selectors(mut self, selectors: Arc<dyn FactoryProfileSelectionDriver>) -> Self {
    self.nodes = self.nodes.with_profile_selectors(selectors);
    self
  }

  async fn invoke_scoped_validation(
    &self,
    origin: &FactoryBuildOrigin,
    caller: &LeaseBoundScopedValidationInput,
    leases: &dyn JobExecutionStore,
  ) -> Result<FactoryScopedValidationVerdict, AgentToolActionError> {
    use AgentToolActionError::{Expired, InvalidRequest, Unavailable};
    let at = self.nodes.current_time(caller.observed_at).map_err(|_| Unavailable)?;
    let mut snapshot = self.store.factory_run_snapshot(origin.run_id).await?;
    let target = scoped_validation_target(&snapshot, origin, at)?;
    let find_runner = |snapshot: &FactoryRunSnapshot| {
      snapshot
        .flow
        .attempts
        .iter()
        .filter(|node| {
          node.flow_run_id() == target.flow_run_id
            && node.workflow_cycle_id() == target.cycle_id
            && node.node_key() == &target.node
        })
        .max_by_key(|node| node.number())
        .map(NodeAttempt::id)
    };
    if find_runner(&snapshot).is_none() {
      if caller.action == ScopedValidationAction::Status {
        return Ok(FactoryScopedValidationVerdict::Waiting);
      }
      let claim = snapshot.current_claim.as_ref().ok_or(Unavailable)?.id;
      let selected = self.inputs.prepare(&snapshot, &target).await.map_err(|_| Unavailable)?;
      let at = self.nodes.current_time(at).map_err(|_| Unavailable)?;
      leases
        .authorize_tool_action_lease(caller.lease, caller.job_id, caller.attempt, at)
        .await?;
      self
        .nodes
        .run_once(origin.run_id, claim, target.clone(), Some(selected), at)
        .await
        .map_err(|_| Unavailable)?;
      snapshot = self.store.factory_run_snapshot(origin.run_id).await?;
    }
    let at = self.nodes.current_time(at).map_err(|_| Unavailable)?;
    if scoped_validation_target(&snapshot, origin, at)? != target {
      return Err(InvalidRequest);
    }
    let id = find_runner(&snapshot).ok_or(Unavailable)?;
    if snapshot.flow.data.stops.iter().any(|row| row.node_attempt_id() == id) {
      return Ok(FactoryScopedValidationVerdict::Stopped);
    }
    let intent = snapshot
      .flow
      .data
      .build_intents
      .iter()
      .find(|row| row.node().id() == id)
      .ok_or(Unavailable)?;
    let claim = snapshot.current_claim.as_ref().ok_or(Unavailable)?;
    claim.claim.verify_fence(claim.claim.fence(), at).map_err(|_| Expired)?;
    leases
      .authorize_tool_action_lease(caller.lease, caller.job_id, caller.attempt, at)
      .await?;
    self
      .nodes
      .scoped_validation(
        match caller.action {
          ScopedValidationAction::Start => FactoryScopedValidationCall::Start(FactoryScopedBuildCaller {
            lease: caller.lease,
            job_id: caller.job_id,
            attempt: caller.attempt,
          }),
          ScopedValidationAction::Status => FactoryScopedValidationCall::Status,
        },
        FactoryNodeExecutionRequest {
          run_id: origin.run_id,
          admitted: &snapshot.admitted_flow,
          work: &snapshot.work,
          input: intent.input(),
          attempt: intent.node(),
          build: Some(intent),
          ownership: FactoryClaimOwnership::new(claim.owner.clone(), claim.claim),
          observed_at: at,
        },
      )
      .await
      .map_err(|_| Unavailable)
  }
}

fn scoped_validation_target(
  snapshot: &FactoryRunSnapshot,
  origin: &FactoryBuildOrigin,
  at: Timestamp,
) -> Result<FactoryNodeTarget, AgentToolActionError> {
  use AgentToolActionError::{Expired, InvalidRequest};
  let checkpoint = snapshot
    .lifecycle_checkpoints
    .iter()
    .find(|row| row.id == snapshot.current.lifecycle_checkpoint_id)
    .ok_or(InvalidRequest)?;
  let caller = snapshot
    .flow
    .data
    .build_intents
    .iter()
    .find(|intent| intent.node().id() == origin.node_attempt_id)
    .filter(|intent| FactoryBuildOrigin::from_intent(intent).ok().as_ref() == Some(origin))
    .ok_or(InvalidRequest)?;
  let attempt = caller.node();
  if checkpoint.cancellation_requested
    || snapshot.run.state().is_terminal()
    || !snapshot
      .flow
      .runtime()
      .active_nodes()
      .any(|row| row.id() == attempt.id())
  {
    return Err(InvalidRequest);
  }
  if at >= attempt.deadline() {
    return Err(Expired);
  }
  let definition = snapshot
    .admitted_flow
    .closure()
    .definition(caller.input().definition())
    .ok_or(InvalidRequest)?;
  let node = definition
    .node(caller.input().node())
    .and_then(FlowNodeDefinition::scoped_validation_runner)
    .filter(|node| {
      definition
        .node(node)
        .is_some_and(|runner| runner.phase_pool().is_none())
    })
    .ok_or(InvalidRequest)?;
  let flow = snapshot
    .flow
    .runs
    .iter()
    .find(|row| row.id() == attempt.flow_run_id())
    .ok_or(InvalidRequest)?;
  let cycle = snapshot
    .flow
    .cycles
    .iter()
    .find(|row| row.id() == attempt.workflow_cycle_id() && row.flow_run_id() == flow.id())
    .ok_or(InvalidRequest)?;
  let history = snapshot.flow.runtime();
  if !history.is_current_cycle(cycle.id()) {
    return Err(InvalidRequest);
  }
  let validated = snapshot.admitted_flow.validated().map_err(|_| InvalidRequest)?;
  let ready = FlowInterpreter::new(&validated)
    .ready_nodes(flow, cycle, history)
    .map_err(|_| InvalidRequest)?;
  if !ready.contains(node)
    && !snapshot
      .flow
      .attempts
      .iter()
      .any(|row| row.flow_run_id() == flow.id() && row.workflow_cycle_id() == cycle.id() && row.node_key() == node)
  {
    return Err(InvalidRequest);
  }
  Ok(FactoryNodeTarget {
    flow_run_id: flow.id(),
    cycle_id: cycle.id(),
    node: node.clone(),
  })
}
#[async_trait]
impl<S: FactoryRunStore + FactoryPhasePoolStore, E: FactoryNodeExecutor, I: FactoryFlowInputPreparer + ?Sized>
  FactoryFlowCoordinator for FactoryFlowRunner<S, E, I>
{
  async fn reconcile_flow(
    &self,
    run: FactoryRunId,
    claim: FactoryDigest,
    at: Timestamp,
  ) -> Result<FactoryFlowStep, ApplicationError> {
    let snapshot = self.store.factory_run_snapshot(run).await?;
    if snapshot.run.state() == FactoryRunState::Cancelled {
      return Ok(FactoryFlowStep::Cancelled);
    }
    if let Some(FactoryLifecycleProgress::FlowResolved { terminal, .. }) = snapshot
      .lifecycle_checkpoints
      .iter()
      .find(|row| row.id == snapshot.current.lifecycle_checkpoint_id)
      .map(|row| &row.progress)
    {
      return Ok(FactoryFlowStep::Resolved(terminal.clone()));
    }
    let ownership = snapshot
      .current_claim
      .as_ref()
      .filter(|row| row.id == claim)
      .ok_or_else(ApplicationError::invalid)?;
    ownership
      .claim
      .verify_fence(ownership.claim.fence(), at)
      .map_err(invalid)?;
    if snapshot.run.state().is_terminal() {
      return Err(ApplicationError::invalid());
    }
    let checkpoint = snapshot
      .lifecycle_checkpoints
      .iter()
      .find(|row| row.id == snapshot.current.lifecycle_checkpoint_id)
      .ok_or_else(ApplicationError::invalid)?;
    if checkpoint.cancellation_requested {
      return Ok(if self.nodes.cancel(&snapshot, at).await? {
        FactoryFlowStep::Cancelled
      } else {
        FactoryFlowStep::Waiting
      });
    }
    // Older servers retained confirmed failures as waiting. Reconcile their
    // existing journal without re-running a Build or charging its usage again.
    if snapshot.run.state() != FactoryRunState::Stopped
      && snapshot.flow.runtime().failed_node_attempts().next().is_some()
    {
      self
        .nodes
        .commit(&snapshot, FactoryRunHistoryAppend::default(), at)
        .await?;
      return Ok(FactoryFlowStep::Advanced);
    }
    let target = match next(&snapshot)? {
      Next::Node(target) => target,
      Next::Terminal(terminal) => {
        self.nodes.resolve(&snapshot, terminal.clone(), at).await?;
        return Ok(FactoryFlowStep::Resolved(terminal));
      }
      Next::Waiting => return Ok(FactoryFlowStep::Waiting),
      Next::Restart { cycle, source } => {
        let previous = snapshot
          .flow
          .cycles
          .iter()
          .find(|row| row.id() == cycle)
          .ok_or_else(ApplicationError::invalid)?;
        let trigger = snapshot
          .flow
          .attempts
          .iter()
          .find(|row| row.id() == source)
          .ok_or_else(ApplicationError::invalid)?;
        let mut append = FactoryRunHistoryAppend::default();
        append.flow.cycles.push(
          WorkflowCycle::restart(
            WorkflowCycleId::from_uuid(source.as_uuid()).map_err(invalid)?,
            previous,
            trigger,
          )
          .map_err(invalid)?,
        );
        self.nodes.commit(&snapshot, append, at).await?;
        return Ok(FactoryFlowStep::Advanced);
      }
    };
    let definition = snapshot
      .flow
      .runs
      .iter()
      .find(|flow| flow.id() == target.flow_run_id)
      .and_then(|flow| snapshot.admitted_flow.closure().definition(flow.definition()))
      .ok_or_else(ApplicationError::invalid)?;
    let node = definition.node(&target.node).ok_or_else(ApplicationError::invalid)?;
    let retry_input =
      crate::factory::execution::factory_node::pending_node_retry(&snapshot, &target)?.map(|(_, input)| input);
    let pending = retry_input.is_none()
      && snapshot.flow.attempts.iter().any(|attempt| {
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
      if retry_input.is_none() && node.profile_selection().is_some() {
        input = Some(
          match crate::factory::profiles::factory_profile_selection::frozen_input(&snapshot, &target)? {
            Some(frozen) => frozen.clone(),
            None => match retry_input.clone() {
              Some(input) => input,
              None => self.inputs.prepare(&snapshot, &target).await?,
            },
          },
        );
      }
      if node.profile_selection().is_some()
        && input
          .as_ref()
          .is_some_and(|input| !factory_profile_wait_is_open(&snapshot, input, at))
      {
        return Err(ApplicationError::invalid());
      }
      let selection_pending = retry_input.is_none()
        && node.profile_selection().is_some()
        && input.as_ref().is_none_or(|input| {
          input
            .digest()
            .ok()
            .and_then(|digest| snapshot.flow.data.current_profile_selection(digest))
            .is_none_or(|selection| selection.candidate().is_none())
        });
      if !selection_pending && let Some(pool) = node.phase_pool() {
        let frozen = crate::factory::profiles::factory_profile_selection::frozen_input(&snapshot, &target)?;
        let Some(frozen) = frozen else {
          let input = match retry_input {
            Some(input) => input,
            None => self.inputs.prepare(&snapshot, &target).await?,
          };
          let mut append = FactoryRunHistoryAppend::default();
          append.flow.data.inputs.push(input);
          self.nodes.commit(&snapshot, append, at).await?;
          return Ok(FactoryFlowStep::Advanced);
        };
        let selected = self.store.phase_pool_selection_for_claim(run, claim).await?;
        let input_digest = frozen.digest().map_err(invalid)?;
        let generation = frozen.generation();
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
      } else if input.is_none() && node.kind() != FlowNodeKind::DeterministicGate {
        input = Some(match retry_input {
          Some(input) => input,
          None => self.inputs.prepare(&snapshot, &target).await?,
        });
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
  Restart {
    cycle: WorkflowCycleId,
    source: NodeAttemptId,
  },
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
    stops: &snapshot.flow.data.stops,
    flow_runs: &snapshot.flow.runs,
    cycles: &snapshot.flow.cycles,
    attempts: &snapshot.flow.attempts,
    completions: &snapshot.flow.completions,
  };
  match FlowInterpreter::new(&validated)
    .progress(flow, cycle, history)
    .map_err(invalid)?
  {
    FlowProgress::RestartCycle { source, .. } => Ok(Next::Restart {
      cycle: cycle.id(),
      source,
    }),
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
