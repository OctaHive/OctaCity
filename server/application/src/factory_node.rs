//! Fenced execution of configured nodes over one common append-only journal.
use crate::ApplicationError;
use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use octacity_server_store::*;
use std::sync::Arc;

/// Trusted owner time, read again after asynchronous capability calls.
/// Provider result timestamps never extend execution or observer ownership.
pub trait FactoryExecutionClock: Send + Sync {
  /// Reads authoritative wall time, failing closed if the clock is unavailable.
  fn now(&self) -> Result<Timestamp, ApplicationError>;
}
/// Production wall clock shared by configured Flow and node owners.
pub struct SystemFactoryExecutionClock;
impl FactoryExecutionClock for SystemFactoryExecutionClock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    let millis = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .ok()
      .and_then(|duration| i64::try_from(duration.as_millis()).ok())
      .ok_or_else(ApplicationError::unavailable)?;
    Timestamp::from_unix_millis(millis).map_err(|_| ApplicationError::unavailable())
  }
}

/// Exact owning Flow and cycle; node names carry no runtime semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryNodeTarget {
  /// Persisted Flow instance.
  pub flow_run_id: FlowRunId,
  /// Persisted workflow cycle.
  pub cycle_id: WorkflowCycleId,
  /// Operator-defined node key.
  pub node: FactoryKey,
}
/// Frozen request supplied only after its input and attempt have been committed.
pub struct FactoryNodeExecutionRequest<'a> {
  /// Exact owning Run, independent of capability lifecycle records.
  pub run_id: FactoryRunId,
  /// Immutable admitted graph and its selected contracts.
  pub admitted: &'a AdmittedFlow,
  /// Immutable source identity and subject, with no capability execution history.
  pub work: &'a WorkEnvelope,
  /// Frozen input, including selected context and source provenance.
  pub input: &'a FlowNodeInput,
  /// Persisted attempt owning this operation.
  pub attempt: &'a NodeAttempt,
  /// Persisted ordinary Build intent for command and reasoning execution.
  pub build: Option<&'a FlowBuildIntent>,
  /// Current successor or original observer ownership.
  pub ownership: FactoryClaimOwnership,
  /// Authoritative observation time.
  pub observed_at: Timestamp,
}
/// Observations from a trusted execution adapter; routing remains in the definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryNodeOutboxEffect {
  /// Stable logical side-effect identity, reused by the selected dispatcher.
  pub operation_id: FactoryDigest,
  /// Registered technical effect family, independent of Flow and node names.
  pub kind: FactoryKey,
}

/// Observations from a trusted execution adapter; routing remains in the definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactoryNodeExecutionStep {
  /// No terminal execution observation yet.
  Waiting {
    /// Exact ordinary execution to retain, when execution uses a Build.
    execution: Option<Box<FlowBuildExecution>>,
  },
  /// Bounded independently verified result and measured consumption.
  Completed {
    /// Exact ordinary execution to retain with the result, when present.
    execution: Option<Box<FlowBuildExecution>>,
    /// Exact retained node result.
    record: Box<FlowNodeRecord>,
    /// Consumption of this execution; attempts are charged at creation.
    usage: BudgetUsage,
    /// Deferred effects accepted atomically with this exact retained result.
    outbox: Vec<FactoryNodeOutboxEffect>,
  },
  /// Execution failure stays distinct from a semantic configured outcome.
  ExecutionStopped {
    /// Exact stopped ordinary execution, when present.
    execution: Option<Box<FlowBuildExecution>>,
    /// Measured resources across the failed operation; unavailable usage cannot become zero.
    usage: BudgetUsage,
  },
}
/// Technical observation from an exact adapter, with no semantic outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactoryNodeCancellationStep {
  /// An accepted cancellation is still awaiting authoritative terminal facts.
  Waiting {
    /// Ordinary execution identities, when dispatched.
    execution: Option<Box<FlowBuildExecution>>,
  },
  /// The operation is absent or all its owned effects have stopped.
  Stopped {
    /// Actual ordinary execution, when dispatched.
    execution: Option<Box<FlowBuildExecution>>,
    /// Measured consumption across the operation's retries.
    usage: BudgetUsage,
  },
}
/// Installed execution capabilities for closed primitive kinds, independent of phase names.
#[async_trait]
pub trait FactoryNodeExecutor: Send + Sync {
  /// Stops or observes the existing operation without creating or replaying effects.
  /// The original accepted cancellation time is retained across owner replacement.
  async fn cancel(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
    requested_at: Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError>;
  /// Observes the exact persisted operation; adapters must use stable identities before side effects.
  async fn observe(
    &self,
    request: FactoryNodeExecutionRequest<'_>,
  ) -> Result<FactoryNodeExecutionStep, ApplicationError>;
}
/// One bounded owner reconciliation result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactoryNodeStep {
  /// Frozen input and execution intent were committed; dispatch occurs on a later pass.
  Advanced,
  /// External execution is still pending.
  Waiting,
  /// Trusted execution failed, without selecting any business outcome.
  ExecutionStopped,
  /// Exact accepted result; configured transitions can now consume it.
  Completed(Box<FlowNodeRecord>),
}
/// Prepares input only from immutable incoming data, Work or retained predecessor results.
pub fn prepare_factory_flow_input(
  snapshot: &FactoryRunSnapshot,
  target: &FactoryNodeTarget,
) -> Result<FlowNodeInput, ApplicationError> {
  let flow = snapshot
    .flow
    .runs
    .iter()
    .find(|row| row.id() == target.flow_run_id)
    .ok_or_else(ApplicationError::invalid)?;
  let cycle = snapshot
    .flow
    .cycles
    .iter()
    .find(|row| row.id() == target.cycle_id)
    .ok_or_else(ApplicationError::invalid)?;
  if !runtime(snapshot).is_current_cycle(cycle.id()) {
    return Err(ApplicationError::invalid());
  }
  let node = snapshot
    .admitted_flow
    .closure()
    .definition(flow.definition())
    .and_then(|definition| definition.node(&target.node))
    .ok_or_else(ApplicationError::invalid)?;
  let schema = node
    .input_schema()
    .and_then(|schema| snapshot.admitted_flow.data_schema(schema))
    .ok_or_else(ApplicationError::invalid)?;
  FlowNodeInput::prepare(FlowInputPreparation {
    work: &snapshot.work,
    admitted: &snapshot.admitted_flow,
    flow,
    cycle,
    node: target.node.clone(),
    schema,
    history: runtime(snapshot),
    records: &snapshot.flow.data.records,
    inputs: &snapshot.flow.data.inputs,
    incoming: snapshot.flow.data.incoming.first(),
  })
  .map_err(invalid)
}

/// Common owner for any configured node. Each call performs at most one fenced append.
pub struct FactoryNodeRunner<S, E> {
  store: Arc<S>,
  executor: Arc<E>,
  clock: Arc<dyn FactoryExecutionClock>,
  profile_catalog: Option<Arc<dyn FactoryConfigurationStore>>,
}
impl<S: FactoryRunStore, E: FactoryNodeExecutor> FactoryNodeRunner<S, E> {
  /// Connects persistence and installed execution adapters.
  pub fn new(store: Arc<S>, executor: Arc<E>, clock: Arc<dyn FactoryExecutionClock>) -> Self {
    Self {
      store,
      executor,
      clock,
      profile_catalog: None,
    }
  }
  /// Qualifies reserve choices and revalidates retained profiles before execution.
  /// Static execution and unselected manual waits need no catalog; selected profiles fail closed without one.
  pub fn with_profile_catalog(mut self, catalog: Arc<dyn FactoryConfigurationStore>) -> Self {
    self.profile_catalog = Some(catalog);
    self
  }
  fn current_time(&self, at: Timestamp) -> Result<Timestamp, ApplicationError> {
    Ok(at.max(self.clock.now()?))
  }
  /// Prepares input only from immutable incoming data, Work or retained predecessor results.
  pub fn prepare_input(
    snapshot: &FactoryRunSnapshot,
    target: &FactoryNodeTarget,
  ) -> Result<FlowNodeInput, ApplicationError> {
    prepare_factory_flow_input(snapshot, target)
  }
  /// Starts or observes one ready node under the exact current Run claim.
  ///
  /// Initial context is supplied by the trusted context owner and freezes once.
  /// Restart reads the original intent instead of consulting current profiles.
  pub async fn run_once(
    &self,
    run: FactoryRunId,
    claim_id: FactoryDigest,
    target: FactoryNodeTarget,
    input: Option<FlowNodeInput>,
    at: Timestamp,
  ) -> Result<FactoryNodeStep, ApplicationError> {
    let snapshot = self.store.factory_run_snapshot(run).await?;
    let mut at = self.current_time(at)?;
    let claim = snapshot
      .current_claim
      .as_ref()
      .filter(|row| row.id == claim_id)
      .ok_or_else(ApplicationError::invalid)?;
    claim.claim.verify_fence(claim.claim.fence(), at).map_err(invalid)?;
    let checkpoint = snapshot
      .lifecycle_checkpoints
      .iter()
      .find(|row| row.id == snapshot.current.lifecycle_checkpoint_id)
      .ok_or_else(ApplicationError::invalid)?;
    if checkpoint.cancellation_requested || snapshot.run.state().is_terminal() {
      return Err(ApplicationError::invalid());
    }
    let flow = snapshot
      .flow
      .runs
      .iter()
      .find(|row| row.id() == target.flow_run_id)
      .ok_or_else(ApplicationError::invalid)?;
    let cycle = snapshot
      .flow
      .cycles
      .iter()
      .find(|row| row.id() == target.cycle_id && row.flow_run_id() == flow.id())
      .ok_or_else(ApplicationError::invalid)?;
    if !runtime(&snapshot).is_current_cycle(cycle.id()) {
      return Err(ApplicationError::invalid());
    }
    let definition = snapshot
      .admitted_flow
      .closure()
      .definition(flow.definition())
      .ok_or_else(ApplicationError::invalid)?;
    let node = definition.node(&target.node).ok_or_else(ApplicationError::invalid)?;
    let ownership = FactoryClaimOwnership::new(claim.owner.clone(), claim.claim);
    let attempt = snapshot
      .flow
      .attempts
      .iter()
      .filter(|row| {
        row.flow_run_id() == flow.id() && row.workflow_cycle_id() == cycle.id() && row.node_key() == &target.node
      })
      .max_by_key(|row| row.number());
    let retry_input = pending_node_retry(&snapshot, &target)?;
    let retry = retry_input.as_ref().map(|(attempt, _)| *attempt);
    let attempt = match attempt {
      Some(_) if retry.is_some() => None,
      Some(attempt) if ready_again(&snapshot, &target, attempt)? => None,
      other => other,
    };
    let mut input = input;
    let retry_build = retry.and_then(|previous| {
      snapshot
        .flow
        .data
        .build_intents
        .iter()
        .find(|intent| intent.node().id() == previous.id())
    });
    if let Some(profile) = retry_build.and_then(|intent| intent.selected_profile()) {
      let candidate = node
        .profile_selection()
        .and_then(|binding| {
          binding
            .candidates
            .values()
            .find(|candidate| candidate.profile.reference() == profile)
        })
        .ok_or_else(ApplicationError::invalid)?;
      crate::factory_profile_selection::validate_candidate(&snapshot, self.profile_catalog.as_deref(), candidate)
        .await?;
      at = self.current_time(at)?;
    }
    let selection = if attempt.is_none() && retry.is_none() && node.profile_selection().is_some() {
      if let Some(append) = crate::factory_profile_selection::prepare(
        &snapshot,
        &target,
        input.clone(),
        at,
        self.profile_catalog.as_deref(),
      )
      .await?
      {
        self.commit(&snapshot, append, at).await?;
        return Ok(FactoryNodeStep::Advanced);
      }
      let frozen =
        crate::factory_profile_selection::frozen_input(&snapshot, &target)?.ok_or_else(ApplicationError::invalid)?;
      let selection = snapshot
        .flow
        .data
        .current_profile_selection(frozen.digest().map_err(invalid)?)
        .ok_or_else(ApplicationError::invalid)?;
      if selection.candidate().is_none() {
        return Ok(FactoryNodeStep::Waiting);
      }
      input = Some(frozen.clone());
      Some(selection)
    } else {
      None
    };
    if let Some(attempt) = attempt {
      let frozen = snapshot
        .flow
        .data
        .inputs
        .iter()
        .find(|row| row.digest().ok() == Some(attempt.input_digest()))
        .ok_or_else(ApplicationError::invalid)?;
      if input.as_ref().is_some_and(|input| input != frozen) {
        return Err(ApplicationError::invalid());
      }
      if let Some(record) = snapshot
        .flow
        .data
        .records
        .iter()
        .find(|record| record.node_attempt_id() == attempt.id())
      {
        return Ok(FactoryNodeStep::Completed(Box::new(record.clone())));
      }
      if let Some(receipt) = snapshot
        .flow
        .data
        .stops
        .iter()
        .find(|row| row.node_attempt_id() == attempt.id())
      {
        return match receipt.reason() {
          FlowNodeStopReason::ExecutionFailed => Ok(FactoryNodeStep::ExecutionStopped),
          FlowNodeStopReason::Cancelled { .. } => Err(ApplicationError::invalid()),
        };
      }
      attempt.verify_observer(&ownership, at).map_err(invalid)?;
      let mut append = FactoryRunHistoryAppend::default();
      let mut effects = vec![];
      let (record, usage) = if node.kind() == FlowNodeKind::DeterministicGate {
        let program = FlowGateProgram::from_node(node).map_err(invalid)?;
        let outcome = program.select_at(frozen.payload(), at).map_err(invalid)?;
        let schema = node
          .outcome(&outcome)
          .and_then(|outcome| snapshot.admitted_flow.data_schema(outcome.schema()))
          .ok_or_else(ApplicationError::invalid)?;
        (
          Box::new(
            program
              .record(frozen, attempt, definition, schema, at)
              .map_err(invalid)?,
          ),
          BudgetUsage::default(),
        )
      } else if node.kind() == FlowNodeKind::HumanGate {
        let Some((response, outcome, payload)) = snapshot.controls.iter().find_map(|row| match &row.intent {
          FactoryRunControlIntent::ResolveNode {
            node_attempt_id,
            outcome,
            payload,
          } if *node_attempt_id == attempt.id() => Some((row, outcome, payload)),
          _ => None,
        }) else {
          return Ok(FactoryNodeStep::Waiting);
        };
        (
          Box::new(
            FlowNodeRecord::new(
              frozen,
              attempt,
              definition,
              FlowRecordObservation {
                outcome: outcome.clone(),
                payload: payload.clone(),
                producer: attempt.execution().clone(),
                observed_at: response.recorded_at,
              },
            )
            .map_err(invalid)?,
          ),
          BudgetUsage::default(),
        )
      } else if node.kind() == FlowNodeKind::SubflowCall {
        (
          Box::new(
            snapshot
              .flow
              .data
              .subflow_result(&snapshot.admitted_flow, runtime(&snapshot), attempt)
              .map_err(invalid)?
              .ok_or_else(ApplicationError::invalid)?,
          ),
          BudgetUsage::default(),
        )
      } else {
        let step = self
          .executor
          .observe(FactoryNodeExecutionRequest {
            run_id: snapshot.run.id(),
            admitted: &snapshot.admitted_flow,
            work: &snapshot.work,
            input: frozen,
            attempt,
            build: snapshot
              .flow
              .data
              .build_intents
              .iter()
              .find(|intent| intent.node().id() == attempt.id()),
            ownership: ownership.clone(),
            observed_at: at,
          })
          .await?;
        at = self.current_time(at)?;
        attempt.verify_observer(&ownership, at).map_err(invalid)?;
        match step {
          FactoryNodeExecutionStep::Waiting { execution } => {
            retain_execution(&snapshot, &mut append, execution);
            if append.record_count() != 0 {
              self.commit(&snapshot, append, at).await?;
            }
            return Ok(FactoryNodeStep::Waiting);
          }
          FactoryNodeExecutionStep::ExecutionStopped { execution, usage } => {
            append.flow.data.stops.push(
              FlowNodeStop::new(
                attempt,
                ownership,
                FlowNodeStopReason::ExecutionFailed,
                at,
                usage,
                execution.as_deref(),
              )
              .map_err(invalid)?,
            );
            retain_execution(&snapshot, &mut append, execution);
            self.commit(&snapshot, append, at).await?;
            return Ok(FactoryNodeStep::ExecutionStopped);
          }
          FactoryNodeExecutionStep::Completed {
            record,
            usage,
            execution,
            outbox,
          } => {
            retain_execution(&snapshot, &mut append, execution);
            effects = outbox;
            (record, usage)
          }
        }
      };
      let completion = record
        .completion(attempt, definition, ownership, BudgetUsage { attempts: 0, ..usage }, at)
        .map_err(invalid)?;
      append.flow.completions.push(completion);
      append.flow.data.records.push(*record.clone());
      if !effects.is_empty() && node.kind() != FlowNodeKind::TrustedAction || effects.len() > 16 {
        return Err(ApplicationError::invalid());
      }
      let mut unique = std::collections::BTreeSet::new();
      let input_digest = record.digest().map_err(invalid)?;
      let mut outbox = vec![];
      for effect in effects {
        if !unique.insert(effect.operation_id) {
          return Err(ApplicationError::invalid());
        }
        outbox.push(FactoryOutboxRecord::pending(
          effect.operation_id,
          snapshot.run.id(),
          effect.kind,
          input_digest,
          at,
          at,
        ));
      }
      self
        .commit_progress(&snapshot, append, at, GraphDisposition::Running, outbox)
        .await?;
      return Ok(FactoryNodeStep::Completed(record));
    }
    let input = if let Some((_, expected)) = retry_input {
      if input.as_ref().is_some_and(|row| row != &expected) {
        return Err(ApplicationError::invalid());
      }
      expected
    } else {
      input
        .map(Ok)
        .unwrap_or_else(|| Self::prepare_input(&snapshot, &target))?
    };
    if input.flow_run_id() != flow.id() || input.cycle_id() != cycle.id() || input.node() != &target.node {
      return Err(ApplicationError::invalid());
    }
    let number = snapshot
      .flow
      .attempts
      .iter()
      .filter(|row| row.flow_run_id() == flow.id())
      .map(|row| row.number().get())
      .max()
      .unwrap_or(0)
      .checked_add(1)
      .ok_or_else(ApplicationError::invalid)?;
    let deadline = if let Some(selection) = selection {
      selection.deadline()
    } else {
      Timestamp::from_unix_millis(
        at.unix_millis()
          .checked_add(i64::try_from(node.budget().max_elapsed_millis()).map_err(|_| ApplicationError::invalid())?)
          .ok_or_else(ApplicationError::invalid)?,
      )
      .map_err(|_| ApplicationError::invalid())?
    };
    let build = if let Some(previous) = retry_build {
      Some(previous.profile().binding(definition).map_err(invalid)?)
    } else {
      selection
        .and_then(|selection| node.profile_selection()?.candidates.get(selection.candidate()?))
        .map(|candidate| &candidate.build)
        .or_else(|| node.build())
    };
    let execution = if let Some(build) = build {
      NodeExecutionIdentity::External(build.tool.clone())
    } else if let Some(action) = node.action() {
      NodeExecutionIdentity::External(action.plugin().clone())
    } else {
      NodeExecutionIdentity::BuiltIn
    };
    let mut attempt = NodeAttempt::new(
      flow,
      cycle,
      definition,
      NodeAttemptInput {
        id: NodeAttemptId::generate(),
        node_key: target.node,
        node_kind: node.kind(),
        number: NodeAttemptNumber::new(number).map_err(invalid)?,
        input_digest: input.digest().map_err(invalid)?,
        budget: node.budget(),
        started_at: at,
        deadline,
        execution,
        ownership,
      },
    )
    .map_err(invalid)?;
    if let Some(previous) = retry {
      attempt = attempt.with_retry_of(previous).map_err(invalid)?;
    }
    let mut append = FactoryRunHistoryAppend::default();
    if let Some(child_definition) = node.subflow_definition() {
      let child = FlowRun::nested(
        FlowRunId::from_uuid(attempt.id().as_uuid()).map_err(invalid)?,
        run,
        child_definition,
        FlowRunParent::new(flow.id(), attempt.id()),
      );
      append
        .flow
        .cycles
        .push(WorkflowCycle::initial(&child).map_err(invalid)?);
      append.flow.runs.push(child);
    }
    if build.is_some() {
      let intent = if let Some(previous) = retry_build {
        previous.retry(
          input.clone(),
          &snapshot.admitted_flow,
          attempt.clone(),
          current_usage(&snapshot)?,
          i64::from(snapshot.work.priority().get()),
        )
      } else if let Some(selection) = selection {
        FlowBuildIntent::selected(
          input.clone(),
          &snapshot.admitted_flow,
          attempt.clone(),
          current_usage(&snapshot)?,
          i64::from(snapshot.work.priority().get()),
          selection,
        )
      } else {
        FlowBuildIntent::new(
          input.clone(),
          &snapshot.admitted_flow,
          attempt.clone(),
          current_usage(&snapshot)?,
          i64::from(snapshot.work.priority().get()),
        )
      }
      .map_err(invalid)?;
      intent
        .verify_reservation(&snapshot.admitted_flow, runtime(&snapshot))
        .map_err(invalid)?;
      append.flow.data.build_intents.push(intent);
    }
    if !snapshot.flow.data.inputs.contains(&input) {
      append.flow.data.inputs.push(input);
    }
    append.flow.attempts.push(attempt);
    self.commit(&snapshot, append, at).await?;
    Ok(FactoryNodeStep::Advanced)
  }
  pub(crate) async fn commit(
    &self,
    snapshot: &FactoryRunSnapshot,
    append: FactoryRunHistoryAppend,
    at: Timestamp,
  ) -> Result<(), ApplicationError> {
    self
      .commit_progress(snapshot, append, at, GraphDisposition::Running, vec![])
      .await
  }
  /// Completes only the root Flow; the store independently verifies its terminal proof.
  pub(crate) async fn resolve(
    &self,
    snapshot: &FactoryRunSnapshot,
    terminal: FactoryKey,
    at: Timestamp,
  ) -> Result<(), ApplicationError> {
    self
      .commit_progress(
        snapshot,
        FactoryRunHistoryAppend::default(),
        at,
        GraphDisposition::Resolved(terminal),
        vec![],
      )
      .await
  }
  /// Reconciles at most one effectful stop receipt before finalizing the root.
  /// Returns true only after every pending operation is independently stopped.
  pub(crate) async fn cancel(&self, snapshot: &FactoryRunSnapshot, at: Timestamp) -> Result<bool, ApplicationError> {
    let mut at = self.current_time(at)?;
    let claim = snapshot.current_claim.as_ref().ok_or_else(ApplicationError::invalid)?;
    claim.claim.verify_fence(claim.claim.fence(), at).map_err(invalid)?;
    let requested_at = snapshot
      .controls
      .iter()
      .filter(|row| row.intent == FactoryRunControlIntent::Cancel)
      .map(|row| row.recorded_at)
      .min()
      .ok_or_else(ApplicationError::invalid)?;
    let pending = snapshot.flow.attempts.iter().find(|attempt| {
      !matches!(
        attempt.node_kind(),
        FlowNodeKind::HumanGate | FlowNodeKind::SubflowCall | FlowNodeKind::DeterministicGate
      ) && !snapshot
        .flow
        .completions
        .iter()
        .any(|row| row.node_attempt_id() == attempt.id())
        && !snapshot
          .flow
          .data
          .stops
          .iter()
          .any(|row| row.node_attempt_id() == attempt.id())
    });
    if let Some(attempt) = pending {
      let input = snapshot
        .flow
        .data
        .inputs
        .iter()
        .find(|row| row.digest().ok() == Some(attempt.input_digest()))
        .ok_or_else(ApplicationError::invalid)?;
      let ownership = FactoryClaimOwnership::new(claim.owner.clone(), claim.claim);
      attempt.verify_observer(&ownership, at).map_err(invalid)?;
      let step = self
        .executor
        .cancel(
          FactoryNodeExecutionRequest {
            run_id: snapshot.run.id(),
            admitted: &snapshot.admitted_flow,
            work: &snapshot.work,
            input,
            attempt,
            build: snapshot
              .flow
              .data
              .build_intents
              .iter()
              .find(|row| row.node().id() == attempt.id()),
            ownership: ownership.clone(),
            observed_at: at,
          },
          requested_at,
        )
        .await?;
      at = self.current_time(at)?;
      attempt.verify_observer(&ownership, at).map_err(invalid)?;
      let mut append = FactoryRunHistoryAppend::default();
      match step {
        FactoryNodeCancellationStep::Waiting { execution } => retain_execution(snapshot, &mut append, execution),
        FactoryNodeCancellationStep::Stopped { execution, usage } => {
          append.flow.data.stops.push(
            FlowNodeStop::new(
              attempt,
              ownership,
              FlowNodeStopReason::Cancelled { requested_at },
              at,
              usage,
              execution.as_deref(),
            )
            .map_err(invalid)?,
          );
          retain_execution(snapshot, &mut append, execution);
        }
      }
      if append.record_count() != 0 {
        self.commit(snapshot, append, at).await?;
      }
      return Ok(false);
    }
    self
      .commit_progress(
        snapshot,
        FactoryRunHistoryAppend::default(),
        at,
        GraphDisposition::Cancelled,
        vec![],
      )
      .await?;
    Ok(true)
  }
  async fn commit_progress(
    &self,
    snapshot: &FactoryRunSnapshot,
    append: FactoryRunHistoryAppend,
    at: Timestamp,
    disposition: GraphDisposition,
    outbox: Vec<FactoryOutboxRecord>,
  ) -> Result<(), ApplicationError> {
    let claim = snapshot.current_claim.as_ref().ok_or_else(ApplicationError::invalid)?;
    let publication_time = self.current_time(at)?;
    claim
      .claim
      .verify_fence(claim.claim.fence(), publication_time)
      .map_err(invalid)?;
    // Catalog reads may outlive the selection window while the worker lease remains valid.
    if append
      .flow
      .data
      .profile_selections
      .iter()
      .any(|selection| publication_time >= selection.deadline())
      || append
        .flow
        .data
        .build_intents
        .iter()
        .any(|intent| intent.selected_profile().is_some() && publication_time >= intent.node().deadline())
    {
      return Err(ApplicationError::invalid());
    }
    let version = FactoryRunVersion::new(
      snapshot
        .run
        .version()
        .get()
        .checked_add(1)
        .ok_or_else(ApplicationError::invalid)?,
    )
    .map_err(invalid)?;
    let mut usage = current_usage(snapshot)?;
    usage.attempts = usage
      .attempts
      .checked_add(u32::try_from(append.flow.attempts.len()).map_err(|_| ApplicationError::invalid())?)
      .ok_or_else(ApplicationError::invalid)?;
    for completion in &append.flow.completions {
      usage = usage
        .checked_add(completion.usage())
        .ok_or_else(ApplicationError::invalid)?;
    }
    for receipt in &append.flow.data.stops {
      usage = usage
        .checked_add(receipt.usage())
        .ok_or_else(ApplicationError::invalid)?;
    }
    let budget = FactoryBudgetRecord::new(snapshot.run.id(), version, usage, at);
    let previous = snapshot
      .lifecycle_checkpoints
      .iter()
      .find(|row| row.id == snapshot.current.lifecycle_checkpoint_id)
      .ok_or_else(ApplicationError::invalid)?;
    let checkpoint = FactoryLifecycleCheckpoint::new(
      snapshot.run.id(),
      version,
      match &disposition {
        GraphDisposition::Running => FactoryLifecycleProgress::FlowRunning {
          root: snapshot.admitted_flow.root_run().id(),
        },
        GraphDisposition::Resolved(terminal) => FactoryLifecycleProgress::FlowResolved {
          root: snapshot.admitted_flow.root_run().id(),
          terminal: terminal.clone(),
        },
        GraphDisposition::Cancelled => FactoryLifecycleProgress::FlowCancelled {
          root: snapshot.admitted_flow.root_run().id(),
        },
      },
      previous.cancellation_requested,
      at,
    );
    let mut current = snapshot.current.clone();
    current.budget_id = budget.id;
    current.lifecycle_checkpoint_id = checkpoint.id;
    // Bind the audit to the complete accepted journal, including protected
    // context and execution provenance. Only its digest enters diagnostics.
    let flow = &append.flow;
    let transition_bytes = serde_json::to_vec(&(
      snapshot.run.id(),
      snapshot.run.version(),
      claim.id,
      (&flow.runs, &flow.cycles, &flow.attempts, &flow.completions),
      (
        &flow.data.stops,
        &flow.data.incoming,
        &flow.data.inputs,
        &flow.data.records,
        &flow.data.build_intents,
        &flow.data.build_executions,
      ),
      budget.usage,
      checkpoint.id,
      outbox.iter().map(|row| row.id).collect::<Vec<_>>(),
    ))
    .map_err(|_| ApplicationError::invalid())?;
    let audit = FactoryAuditFact::new(
      snapshot.run.id(),
      AuditActorKind::Worker,
      None,
      FactoryKey::new("factory.node.advance").map_err(invalid)?,
      FactoryDigest::sha256("octacity.factory.node-transition.v2", &[&transition_bytes]),
      FactoryKey::new("accepted").map_err(invalid)?,
      at,
    );
    let transition = CommitFactoryRunTransition {
      run_id: snapshot.run.id(),
      expected_version: snapshot.run.version(),
      claim_id: claim.id,
      owner: claim.owner.clone(),
      fence: claim.claim.fence(),
      committed_at: at,
      next_run: FactoryRun::restore(
        snapshot.run.id(),
        snapshot.run.configuration().clone(),
        &snapshot.work,
        snapshot.run.subject().clone(),
        match disposition {
          GraphDisposition::Resolved(_) => FactoryRunState::Completed,
          GraphDisposition::Running => FactoryRunState::Running,
          GraphDisposition::Cancelled => FactoryRunState::Cancelled,
        },
        version,
      )
      .map_err(invalid)?,
      budget,
      lifecycle_checkpoint: checkpoint,
      append,
      current,
      audit,
      outbox,
    };
    let observation = crate::factory_observability::FactoryTransitionObservation::new(snapshot, &transition);
    self.store.commit_factory_run_transition(transition).await?;
    observation.emit();
    Ok(())
  }
}
fn current_usage(snapshot: &FactoryRunSnapshot) -> Result<BudgetUsage, ApplicationError> {
  snapshot
    .budgets
    .iter()
    .find(|row| row.id == snapshot.current.budget_id)
    .map(|row| row.usage)
    .ok_or_else(ApplicationError::invalid)
}
fn runtime(snapshot: &FactoryRunSnapshot) -> FlowRuntimeHistory<'_> {
  FlowRuntimeHistory {
    stops: &snapshot.flow.data.stops,
    flow_runs: &snapshot.flow.runs,
    cycles: &snapshot.flow.cycles,
    attempts: &snapshot.flow.attempts,
    completions: &snapshot.flow.completions,
  }
}
fn invalid(_: FactoryError) -> ApplicationError {
  ApplicationError::invalid()
}

enum GraphDisposition {
  Running,
  Resolved(FactoryKey),
  Cancelled,
}

fn retain_execution(
  snapshot: &FactoryRunSnapshot,
  append: &mut FactoryRunHistoryAppend,
  execution: Option<Box<FlowBuildExecution>>,
) {
  if let Some(execution) = execution
    && !snapshot.flow.data.build_executions.contains(&execution)
  {
    append.flow.data.build_executions.push(*execution);
  }
}

/// One bounded observation of a configured graph by its owning application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactoryFlowStep {
  /// A fenced node or graph fact was appended.
  Advanced,
  /// External work or configured readiness is pending.
  Waiting,
  /// A configured node is waiting for durable queue selection.
  Ready(Box<PhasePoolEntry>),
  /// A finite configured terminal was reached.
  Resolved(FactoryKey),
  /// Technical execution was cancelled without selecting a logical terminal.
  Cancelled,
}
/// Composition boundary for configured graph execution, independent of business phases.
#[async_trait]
pub trait FactoryFlowCoordinator: Send + Sync {
  /// Reconciles at most one durable operation under the current stored claim.
  async fn reconcile_flow(
    &self,
    run: FactoryRunId,
    claim: FactoryDigest,
    at: Timestamp,
  ) -> Result<FactoryFlowStep, ApplicationError>;
}

// A completed attempt is replayed until a later accepted control edge (or an
// unexhausted self-repeat) makes another attempt ready. Its new input is frozen
// independently, including its generation even when the selected payload repeats.
fn ready_again(
  snapshot: &FactoryRunSnapshot,
  target: &FactoryNodeTarget,
  previous: &NodeAttempt,
) -> Result<bool, ApplicationError> {
  if !snapshot
    .flow
    .completions
    .iter()
    .any(|row| row.node_attempt_id() == previous.id())
  {
    return Ok(false);
  }
  let flow = snapshot
    .flow
    .runs
    .iter()
    .find(|flow| flow.id() == target.flow_run_id)
    .ok_or_else(ApplicationError::invalid)?;
  let cycle = snapshot
    .flow
    .cycles
    .iter()
    .find(|cycle| cycle.id() == target.cycle_id)
    .ok_or_else(ApplicationError::invalid)?;
  let validated = snapshot.admitted_flow.validated().map_err(invalid)?;
  let interpreter = FlowInterpreter::new(&validated);
  for predecessor in snapshot.flow.attempts.iter().filter(|attempt| {
    attempt.flow_run_id() == flow.id()
      && attempt.workflow_cycle_id() == cycle.id()
      && attempt.number() >= previous.number()
  }) {
    if let Some(completion)=snapshot.flow.completions.iter().find(|row|row.node_attempt_id()==predecessor.id())
      && interpreter.advance(flow,cycle,predecessor,completion,snapshot.flow.runtime()).map_err(invalid)?.iter().any(|directive|matches!(directive,FlowDirective::ExecuteNode { node,.. }|FlowDirective::EnterSubflow { node,.. } if node==&target.node)) { return Ok(true); }
  }
  Ok(false)
}

fn retry_requested(snapshot: &FactoryRunSnapshot, attempt: &NodeAttempt) -> bool {
  snapshot.flow.data.stops.iter().any(|row| row.node_attempt_id() == attempt.id() && row.reason() == FlowNodeStopReason::ExecutionFailed)
    && snapshot.controls.iter().any(|row| matches!(row.intent, FactoryRunControlIntent::RetryNode { node_attempt_id } if node_attempt_id == attempt.id()))
}

/// Returns the exact next execution input only for the latest accepted technical retry.
pub(crate) fn pending_node_retry<'a>(
  snapshot: &'a FactoryRunSnapshot,
  target: &FactoryNodeTarget,
) -> Result<Option<(&'a NodeAttempt, FlowNodeInput)>, ApplicationError> {
  let previous = snapshot
    .flow
    .attempts
    .iter()
    .filter(|row| {
      row.flow_run_id() == target.flow_run_id
        && row.workflow_cycle_id() == target.cycle_id
        && row.node_key() == &target.node
    })
    .max_by_key(|row| row.number())
    .filter(|row| retry_requested(snapshot, row));
  previous
    .map(|attempt| {
      let input = snapshot
        .flow
        .data
        .inputs
        .iter()
        .find(|row| row.digest().ok() == Some(attempt.input_digest()))
        .ok_or_else(ApplicationError::invalid)?
        .retry()
        .map_err(invalid)?;
      Ok((attempt, input))
    })
    .transpose()
}
