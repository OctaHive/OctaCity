//! Allowlisted correlation prepared before, and emitted only after, an owner commit.
use octacity_server_factory::*;
use octacity_server_store::*;

pub(crate) struct FactoryTransitionObservation {
  run: FactoryRunId,
  configuration: FactoryConfigurationRef,
  version: FactoryRunVersion,
  audit: FactoryDigest,
  transition: FactoryDigest,
  claim: FactoryDigest,
  usage: BudgetUsage,
  state: FactoryRunState,
  progress: Option<FactoryLifecycleProgress>,
  inputs: Vec<InputObservation>,
  started: Vec<NodeAttempt>,
  cycles: Vec<WorkflowCycle>,
  completed: Vec<(NodeAttemptCompletion, u64)>,
  stopped: Vec<(FlowNodeStop, NodeAttempt, u64)>,
}
struct InputObservation {
  flow: FlowRunId,
  cycle: WorkflowCycleId,
  definition: FlowDefinitionRef,
  node: FactoryDigest,
  input: FactoryDigest,
  context: Option<FactoryDigest>,
  context_policy: Option<FactoryDigest>,
  context_entries: Option<usize>,
  permissions: FactoryDigest,
  generation: u32,
  execution: Option<FactoryDigest>,
  plugin: Option<FactoryDigest>,
  model: Option<FactoryDigest>,
  task: Option<FactoryDigest>,
}
impl FactoryTransitionObservation {
  pub(crate) fn new(snapshot: &FactoryRunSnapshot, request: &CommitFactoryRunTransition) -> Self {
    let inputs = request
      .append
      .flow
      .data
      .inputs
      .iter()
      .filter_map(|input| {
        let digest = input.digest().ok()?;
        let intent = request
          .append
          .flow
          .data
          .build_intents
          .iter()
          .chain(&snapshot.flow.data.build_intents)
          .find(|intent| intent.input().digest().ok() == Some(digest));
        let attempt = request
          .append
          .flow
          .attempts
          .iter()
          .chain(&snapshot.flow.attempts)
          .find(|attempt| attempt.input_digest() == digest);
        Some(InputObservation {
          flow: input.flow_run_id(),
          cycle: input.cycle_id(),
          definition: input.definition(),
          node: FactoryDigest::sha256("octacity.factory.trace-node.v1", &[input.node().as_str().as_bytes()]),
          input: digest,
          context: input.context().and_then(|context| context.digest().ok()),
          context_policy: input.context().map(ContextManifest::construction_policy_digest),
          context_entries: input.context().map(|context| context.entries().len()),
          permissions: snapshot
            .admitted_flow
            .closure()
            .definition(input.definition())?
            .node(input.node())?
            .permissions()
            .digest(),
          generation: input.generation(),
          execution: attempt.and_then(|attempt| match attempt.execution() {
            NodeExecutionIdentity::External(reference) => Some(reference.digest()),
            NodeExecutionIdentity::BuiltIn => None,
          }),
          plugin: intent.map(|intent| intent.profile().plugin.digest()),
          model: intent.map(|intent| intent.profile().model_or_tool.digest()),
          task: intent.map(|intent| intent.profile().task_digest),
        })
      })
      .collect();
    Self {
      run: request.run_id,
      configuration: snapshot.run.configuration().clone(),
      version: request.next_run.version(),
      audit: request.audit.id,
      transition: request.audit.request_identity_digest,
      claim: request.claim_id,
      usage: request.budget.usage,
      state: request.next_run.state(),
      progress: snapshot
        .lifecycle_checkpoints
        .iter()
        .find(|row| row.id == snapshot.current.lifecycle_checkpoint_id)
        .filter(|previous| previous.progress != request.lifecycle_checkpoint.progress)
        .map(|_| request.lifecycle_checkpoint.progress.clone()),
      inputs,
      started: request.append.flow.attempts.clone(),
      cycles: request.append.flow.cycles.clone(),
      stopped: request
        .append
        .flow
        .data
        .stops
        .iter()
        .filter_map(|stop| {
          let attempt = snapshot
            .flow
            .attempts
            .iter()
            .chain(&request.append.flow.attempts)
            .find(|attempt| attempt.id() == stop.node_attempt_id())?;
          let elapsed = stop
            .observed_at()
            .unix_millis()
            .checked_sub(attempt.started_at().unix_millis())?;
          Some((stop.clone(), attempt.clone(), u64::try_from(elapsed).ok()?))
        })
        .collect(),
      completed: request
        .append
        .flow
        .completions
        .iter()
        .filter_map(|completion| {
          let attempt = snapshot
            .flow
            .attempts
            .iter()
            .chain(&request.append.flow.attempts)
            .find(|attempt| attempt.id() == completion.node_attempt_id())?;
          let elapsed = completion
            .observed_at()
            .unix_millis()
            .checked_sub(attempt.started_at().unix_millis())?;
          Some((completion.clone(), u64::try_from(elapsed).ok()?))
        })
        .collect(),
    }
  }
  pub(crate) fn emit(self) {
    let span = tracing::info_span!("octacity.factory.transition",
      factory.run.id = %self.run,
      factory.run.version = self.version.get(),
      factory.configuration.id = %self.configuration.id(),
      factory.configuration.version = self.configuration.version().get(),
      factory.configuration.digest = %self.configuration.definition_digest(),
      factory.audit.id = %self.audit,
      factory.transition.digest = %self.transition,
      factory.claim.digest = %self.claim,
    );
    span.in_scope(|| {
      tracing::info!(
        event.name = "octacity.factory.transition.committed",
        factory.run.state = self.state.as_str(),
        factory.usage.attempts = self.usage.attempts,
        factory.usage.elapsed_ms = self.usage.elapsed_millis,
        factory.usage.tokens = self.usage.tokens,
        factory.usage.cost_micro_units = self.usage.cost_micro_units.is_known().then_some(self.usage.cost_micro_units.known_micro_units()),
        factory.usage.known_cost_micro_units = (!self.usage.cost_micro_units.is_known()).then_some(self.usage.cost_micro_units.known_micro_units()),
        factory.usage.cost_unknown = (!self.usage.cost_micro_units.is_known()).then_some(true),
        factory.usage.output_bytes = self.usage.output_bytes,
        "Factory transition committed"
      );
      match self.progress {
        Some(FactoryLifecycleProgress::FlowResolved { root, terminal }) => {
          let terminal = FactoryDigest::sha256("octacity.factory.trace-terminal.v1", &[terminal.as_str().as_bytes()]);
          tracing::info!(event.name = "octacity.factory.flow.resolved", factory.flow.id = %root,
            factory.flow.state = "resolved", factory.flow.terminal.digest = %terminal, "Factory root Flow resolved");
        }
        Some(FactoryLifecycleProgress::FlowRunning { root }) => {
          tracing::info!(event.name = "octacity.factory.flow.running", factory.flow.id = %root,
            factory.flow.state = "running", "Factory root Flow running");
        }
        Some(FactoryLifecycleProgress::FlowCancelled { root }) => {
          tracing::info!(event.name = "octacity.factory.flow.cancelled", factory.flow.id = %root,
            factory.flow.state = "cancelled", "Factory root Flow cancelled");
        }
        _ => {}
      }
      for cycle in self.cycles {
        tracing::info!(event.name = "octacity.factory.cycle.started", factory.cycle.state = "started",
          factory.flow.id = %cycle.flow_run_id(), factory.cycle.id = %cycle.id(), factory.cycle.number = cycle.number().get(),
          factory.cycle.predecessor.id = cycle.predecessor().map(|id| id.to_string()),
          factory.cycle.source.attempt.id = cycle.source_attempt_id().map(|id| id.to_string()), "Factory workflow cycle started");
      }
      for input in self.inputs {
        tracing::info!(
          event.name = "octacity.factory.input.accepted",
          factory.flow.id = %input.flow,
          factory.cycle.id = %input.cycle,
          factory.definition.id = %input.definition.id(),
          factory.definition.version = input.definition.version().get(),
          factory.definition.digest = %input.definition.digest(),
          factory.node.digest = %input.node,
          factory.input.digest = %input.input,
          factory.context.digest = input.context.as_ref().map(FactoryDigest::to_string),
          factory.context.policy.digest = input.context_policy.as_ref().map(FactoryDigest::to_string),
          factory.context.entries = input.context_entries,
          factory.permissions.digest = %input.permissions,
          factory.input.generation = input.generation,
          factory.execution.digest = input.execution.as_ref().map(FactoryDigest::to_string),
          factory.plugin.digest = input.plugin.as_ref().map(FactoryDigest::to_string),
          factory.model.digest = input.model.as_ref().map(FactoryDigest::to_string),
          factory.task.digest = input.task.as_ref().map(FactoryDigest::to_string),
          "Factory input frozen"
        );
      }
      for (completion, latency) in self.completed {
        let usage = completion.usage();
        let node = FactoryDigest::sha256(
          "octacity.factory.trace-node.v1",
          &[completion.node_key().as_str().as_bytes()],
        );
        let outcome = FactoryDigest::sha256(
          "octacity.factory.trace-outcome.v1",
          &[completion.outcome().as_str().as_bytes()],
        );
        tracing::info!(
          event.name = "octacity.factory.node.completed", factory.node.state = "completed",
          factory.node.attempt.id = %completion.node_attempt_id(), factory.node.digest = %node,
          factory.flow.id = %completion.flow_run_id(), factory.cycle.id = %completion.workflow_cycle_id(),
          factory.node.fence.digest = %completion.claim().fence().digest(), factory.node.outcome.digest = %outcome,
          factory.node.output.digest = %completion.output_digest(), factory.node.latency_ms = latency,
          factory.node.usage.attempts = usage.attempts, factory.node.usage.elapsed_ms = usage.elapsed_millis,
          factory.node.usage.tokens = usage.tokens,
          factory.node.usage.cost_micro_units = usage.cost_micro_units.is_known().then_some(usage.cost_micro_units.known_micro_units()),
          factory.node.usage.known_cost_micro_units = (!usage.cost_micro_units.is_known()).then_some(usage.cost_micro_units.known_micro_units()),
          factory.node.usage.cost_unknown = (!usage.cost_micro_units.is_known()).then_some(true),
          factory.node.usage.output_bytes = usage.output_bytes, "Factory node attempt completed"
        );
      }
      for (stop, attempt, latency) in self.stopped {
        let usage = stop.usage();
        let state = match stop.reason() {
          FlowNodeStopReason::ExecutionFailed => "execution_failed",
          FlowNodeStopReason::Cancelled { .. } => "cancelled",
        };
        let node = FactoryDigest::sha256(
          "octacity.factory.trace-node.v1",
          &[attempt.node_key().as_str().as_bytes()],
        );
        tracing::info!(
          event.name = "octacity.factory.node.stopped", factory.node.state = state,
          factory.node.attempt.id = %attempt.id(), factory.node.digest = %node,
          factory.flow.id = %attempt.flow_run_id(), factory.cycle.id = %attempt.workflow_cycle_id(),
          factory.node.input.digest = %attempt.input_digest(),
          factory.node.fence.digest = %stop.ownership().claim().fence().digest(),
          factory.node.execution.digest = stop.execution().as_ref().map(FactoryDigest::to_string),
          factory.node.latency_ms = latency,
          factory.node.usage.attempts = usage.attempts, factory.node.usage.elapsed_ms = usage.elapsed_millis,
          factory.node.usage.tokens = usage.tokens,
          factory.node.usage.cost_micro_units = usage.cost_micro_units.is_known().then_some(usage.cost_micro_units.known_micro_units()),
          factory.node.usage.known_cost_micro_units = (!usage.cost_micro_units.is_known()).then_some(usage.cost_micro_units.known_micro_units()),
          factory.node.usage.cost_unknown = (!usage.cost_micro_units.is_known()).then_some(true),
          factory.node.usage.output_bytes = usage.output_bytes, "Factory node attempt stopped"
        );
      }
      for attempt in self.started {
        let budget = attempt.budget();
        let node = FactoryDigest::sha256(
          "octacity.factory.trace-node.v1",
          &[attempt.node_key().as_str().as_bytes()],
        );
        tracing::info!(
          event.name = "octacity.factory.node.started",
          factory.node.state = "started",
          factory.node.attempt.id = %attempt.id(),
          factory.node.attempt.number = attempt.number().get(),
          factory.node.retry_of.id = attempt.retry_of().map(|id| id.to_string()),
          factory.node.kind = ?attempt.node_kind(),
          factory.node.digest = %node,
          factory.flow.id = %attempt.flow_run_id(),
          factory.cycle.id = %attempt.workflow_cycle_id(),
          factory.node.deadline_ms = attempt.deadline().unix_millis(),
          factory.node.fence.digest = %attempt.claim().fence().digest(),
          factory.budget.attempts = budget.max_attempts(),
          factory.budget.elapsed_ms = budget.max_elapsed_millis(),
          factory.budget.tokens = budget.max_tokens(),
          factory.budget.cost_micro_units = budget.max_cost_micro_units(),
          factory.budget.output_bytes = budget.max_output_bytes(),
          "Factory node attempt started"
        );
      }
    });
  }
}
