use std::collections::BTreeSet;

use crate::{
  BudgetUsage, FactoryError, FactoryKey, FlowContextProjection, FlowDataProjection, FlowDefinition, FlowDefinitionRef,
  FlowNodeKind, FlowRun, NodeAttempt, NodeAttemptCompletion, ValidatedFlowDefinitionClosure, WorkflowCycle,
};

/// Explicit context and typed-data declarations selected for one successor.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FlowDirectiveInputs {
  /// Context-only selectors; these never transfer a typed result payload.
  pub context: Vec<FlowContextProjection>,
  /// Exact schema-bound data transfers.
  pub data: Vec<FlowDataProjection>,
}

/// One pure code-owned instruction derived from persisted Flow facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FlowDirective {
  /// Dispatch one closed primitive node from the current definition.
  ExecuteNode {
    /// Exact definition containing the node.
    definition: FlowDefinitionRef,
    /// Declared node key.
    node: FactoryKey,
    /// Closed provider-neutral primitive kind.
    kind: FlowNodeKind,
    /// Explicit inputs selected by the traversed edge.
    inputs: FlowDirectiveInputs,
  },
  /// Instantiate one exact pinned nested Flow Definition.
  EnterSubflow {
    /// Calling node key in the parent definition.
    node: FactoryKey,
    /// Exact nested definition selected before admission.
    definition: FlowDefinitionRef,
    /// Explicit inputs selected for the nested Flow entry.
    inputs: FlowDirectiveInputs,
  },
  /// Complete the current Flow Run with one declared terminal outcome.
  Complete {
    /// Finite terminal outcome key.
    terminal: FactoryKey,
    /// Digest of the typed terminal payload schema.
    schema_digest: crate::FactoryDigest,
  },
}

/// Stateless interpreter over one fully validated immutable closure.
pub struct FlowInterpreter<'a> {
  validated: &'a ValidatedFlowDefinitionClosure,
}

impl<'a> FlowInterpreter<'a> {
  /// Binds interpretation to one exact admitted closure.
  #[must_use]
  pub const fn new(validated: &'a ValidatedFlowDefinitionClosure) -> Self {
    Self { validated }
  }

  /// Selects the declared entry without consulting process-local state.
  pub fn start(&self, flow_run: &FlowRun) -> Result<FlowDirective, FactoryError> {
    let definition = self.definition(flow_run)?;
    directive_for_node(definition, definition.entry(), FlowDirectiveInputs::default())
  }

  /// Derives all next control actions from persisted attempts and completions.
  ///
  /// The completion supplies only a finite outcome; it cannot name or create an
  /// edge. Callers must commit the returned instruction and any external work
  /// to the authoritative fenced transaction/outbox before dispatch.
  pub fn advance(
    &self,
    flow_run: &FlowRun,
    cycle: &WorkflowCycle,
    attempt: &NodeAttempt,
    completion: &NodeAttemptCompletion,
    attempts: &[NodeAttempt],
    completions: &[NodeAttemptCompletion],
  ) -> Result<Vec<FlowDirective>, FactoryError> {
    let definition = self.definition(flow_run)?;
    if cycle.flow_run_id() != flow_run.id()
      || attempt.flow_run_id() != flow_run.id()
      || attempt.workflow_cycle_id() != cycle.id()
      || completion.node_attempt_id() != attempt.id()
      || completion.flow_run_id() != flow_run.id()
      || completion.workflow_cycle_id() != cycle.id()
      || completion.node_key() != attempt.node_key()
    {
      return Err(invalid("Flow interpretation snapshot"));
    }
    let node = definition
      .node(attempt.node_key())
      .ok_or_else(|| invalid("Flow interpretation node"))?;
    let declared = node
      .outcome(completion.outcome())
      .filter(|outcome| outcome.schema() == completion.output_schema())
      .ok_or_else(|| invalid("Flow interpretation outcome"))?;
    validate_runtime_limits(self.validated, definition, flow_run, completion, attempts, completions)?;
    let transitions = definition
      .transitions()
      .iter()
      .filter(|transition| {
        transition.predecessor() == attempt.node_key() && transition.outcome() == completion.outcome()
      })
      .collect::<Vec<_>>();
    if transitions.is_empty() {
      return Err(invalid("Flow interpretation transition"));
    }
    let traversals = completions
      .iter()
      .filter(|record| {
        record.flow_run_id() == flow_run.id()
          && record.node_key() == attempt.node_key()
          && record.outcome() == completion.outcome()
      })
      .count();
    let mut directives = Vec::with_capacity(transitions.len());
    for transition in transitions {
      if transition.max_repeats() > 0 && traversals > usize::from(transition.max_repeats()) {
        let terminal = transition
          .exhausted_terminal()
          .and_then(|key| definition.terminals().iter().find(|terminal| terminal.key() == key))
          .filter(|terminal| terminal.schema() == declared.schema())
          .ok_or_else(|| invalid("Flow repeat exhaustion route"))?;
        directives.push(FlowDirective::Complete {
          terminal: terminal.key().clone(),
          schema_digest: terminal.schema().digest(),
        });
        continue;
      }
      match transition.target() {
        crate::FlowTransitionTarget::Node(successor) => {
          directives.push(directive_for_node(
            definition,
            successor,
            projected_inputs(definition, attempt.node_key(), completion.outcome(), successor),
          )?);
        }
        crate::FlowTransitionTarget::Terminal(terminal_key) => {
          let terminal = definition
            .terminals()
            .iter()
            .find(|terminal| terminal.key() == terminal_key)
            .filter(|terminal| terminal.schema() == declared.schema())
            .ok_or_else(|| invalid("Flow terminal schema"))?;
          directives.push(FlowDirective::Complete {
            terminal: terminal.key().clone(),
            schema_digest: terminal.schema().digest(),
          });
        }
      }
    }
    let next_nodes = directives
      .iter()
      .filter(|directive| !matches!(directive, FlowDirective::Complete { .. }))
      .count();
    let completed = completions
      .iter()
      .map(NodeAttemptCompletion::node_attempt_id)
      .collect::<BTreeSet<_>>();
    let active = attempts
      .iter()
      .filter(|record| !completed.contains(&record.id()))
      .count();
    let max_wip = definition
      .execution()
      .max_active_nodes()
      .min(self.validated.limits().max_wip());
    if active.saturating_add(next_nodes) > usize::from(max_wip) {
      return Err(invalid("Flow WIP exhausted"));
    }
    Ok(directives)
  }

  fn definition(&self, flow_run: &FlowRun) -> Result<&FlowDefinition, FactoryError> {
    self
      .validated
      .closure()
      .definition(flow_run.definition())
      .ok_or_else(|| invalid("Flow Run definition"))
  }

  /// Returns the finite code-owned node routes for one declared outcome.
  ///
  /// Runtime-history validation uses the same selector as forward
  /// interpretation, so a restored store snapshot cannot admit a route that
  /// the live interpreter would reject.
  pub(crate) fn declared_successor_keys(
    definition: &FlowDefinition,
    predecessor: &FactoryKey,
    outcome: &FactoryKey,
  ) -> BTreeSet<FactoryKey> {
    definition
      .transitions()
      .iter()
      .filter(|transition| transition.predecessor() == predecessor && transition.outcome() == outcome)
      .filter_map(|transition| match transition.target() {
        crate::FlowTransitionTarget::Node(successor) => Some(successor.clone()),
        crate::FlowTransitionTarget::Terminal(_) => None,
      })
      .collect()
  }
}

fn directive_for_node(
  definition: &FlowDefinition,
  key: &FactoryKey,
  inputs: FlowDirectiveInputs,
) -> Result<FlowDirective, FactoryError> {
  let node = definition.node(key).ok_or_else(|| invalid("Flow directive node"))?;
  Ok(match node.subflow_definition() {
    Some(nested) => FlowDirective::EnterSubflow {
      node: key.clone(),
      definition: nested,
      inputs,
    },
    None => FlowDirective::ExecuteNode {
      definition: definition.reference(),
      node: key.clone(),
      kind: node.kind(),
      inputs,
    },
  })
}

fn projected_inputs(
  definition: &FlowDefinition,
  predecessor: &FactoryKey,
  outcome: &FactoryKey,
  successor: &FactoryKey,
) -> FlowDirectiveInputs {
  FlowDirectiveInputs {
    context: definition
      .context_projections()
      .iter()
      .filter(|projection| {
        projection.predecessor() == predecessor
          && projection.outcome() == outcome
          && projection.successor() == successor
      })
      .cloned()
      .collect(),
    data: definition
      .data_projections()
      .iter()
      .filter(|projection| {
        projection.predecessor() == predecessor
          && projection.outcome() == outcome
          && projection.successor() == successor
      })
      .cloned()
      .collect(),
  }
}

fn validate_runtime_limits(
  validated: &ValidatedFlowDefinitionClosure,
  definition: &FlowDefinition,
  flow_run: &FlowRun,
  completion: &NodeAttemptCompletion,
  attempts: &[NodeAttempt],
  completions: &[NodeAttemptCompletion],
) -> Result<(), FactoryError> {
  if attempts
    .iter()
    .filter(|record| record.id() == completion.node_attempt_id())
    .count()
    != 1
    || completions
      .iter()
      .filter(|record| record.node_attempt_id() == completion.node_attempt_id())
      .count()
      != 1
  {
    return Err(invalid("Flow append-only attempt history"));
  }
  let usage = completions
    .iter()
    .filter(|record| record.flow_run_id() == flow_run.id())
    .try_fold(BudgetUsage::default(), |total, record| add_usage(total, record.usage()))?;
  usage.validate(definition.execution().budget())?;
  let aggregate = completions
    .iter()
    .try_fold(BudgetUsage::default(), |total, record| add_usage(total, record.usage()))?;
  aggregate.validate(validated.limits().budget())?;
  Ok(())
}

fn add_usage(left: BudgetUsage, right: BudgetUsage) -> Result<BudgetUsage, FactoryError> {
  left.checked_add(right).ok_or_else(|| invalid("Flow budget usage"))
}

fn invalid(field: &'static str) -> FactoryError {
  FactoryError::InvalidConfiguration { field }
}
