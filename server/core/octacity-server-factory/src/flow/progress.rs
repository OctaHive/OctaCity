use crate::{
  FactoryError, FactoryKey, FlowDirective, FlowInterpreter, FlowRun, FlowRuntimeHistory, NodeAttemptId, WorkflowCycle,
};
use std::collections::BTreeSet;

/// Current configured control position derived only from retained runtime history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FlowProgress {
  /// Accepted feedback requires a new fenced cycle before any successor dispatch.
  RestartCycle {
    /// Explicit correction entry.
    node: FactoryKey,
    /// Exact accepted source result.
    source: NodeAttemptId,
  },
  /// Ready or unfinished node; nested calls are interpreted by the owning Flow.
  Node(FactoryKey),
  /// Exact result that selected the declared terminal, after all ready work is consumed.
  Terminal {
    /// Declared terminal outcome in the exact definition.
    key: FactoryKey,
    /// Retained attempt whose result selected this terminal.
    source: NodeAttemptId,
  },
  /// No executable control directive is currently available.
  Waiting,
}
impl FlowInterpreter<'_> {
  /// Reconstructs one Flow's current position without re-executing historical edges.
  pub fn progress(
    &self,
    flow: &FlowRun,
    cycle: &WorkflowCycle,
    history: FlowRuntimeHistory<'_>,
  ) -> Result<FlowProgress, FactoryError> {
    self.position(flow, cycle, history).map(|(position, _)| position)
  }

  /// Returns all eligible nodes, including unfinished attempts, without selecting
  /// one sibling or reserving dispatch capacity. A pending restart suppresses
  /// unstarted old-cycle work; only unfinished attempts may drain before it.
  pub fn ready_nodes(
    &self,
    flow: &FlowRun,
    cycle: &WorkflowCycle,
    history: FlowRuntimeHistory<'_>,
  ) -> Result<BTreeSet<FactoryKey>, FactoryError> {
    self.position(flow, cycle, history).map(|(_, ready)| ready)
  }

  fn position(
    &self,
    flow: &FlowRun,
    cycle: &WorkflowCycle,
    history: FlowRuntimeHistory<'_>,
  ) -> Result<(FlowProgress, BTreeSet<FactoryKey>), FactoryError> {
    let mut attempts = history
      .attempts
      .iter()
      .filter(|row| row.flow_run_id() == flow.id() && row.workflow_cycle_id() == cycle.id())
      .collect::<Vec<_>>();
    attempts.sort_by_key(|row| row.number());
    if attempts.is_empty() {
      let entry = self.cycle_entry(flow, cycle, history)?;
      return Ok((FlowProgress::Node(entry.clone()), BTreeSet::from([entry])));
    }
    let pending = attempts
      .iter()
      .filter(|attempt| {
        !attempts.iter().any(|next| next.retry_of() == Some(attempt.id()))
          && !history
            .completions
            .iter()
            .any(|row| row.node_attempt_id() == attempt.id())
      })
      .copied()
      .collect::<Vec<_>>();
    let mut ready = pending
      .iter()
      .map(|row| row.node_key().clone())
      .collect::<BTreeSet<_>>();
    let mut restart = None;
    let mut terminal = None;
    for attempt in &attempts {
      let Some(completion) = history
        .completions
        .iter()
        .find(|row| row.node_attempt_id() == attempt.id())
      else {
        continue;
      };
      for directive in self.control_directives(flow, cycle, attempt, completion, history)? {
        match directive {
          FlowDirective::RestartCycle { node, source } => {
            if restart.is_none() {
              restart = Some(FlowProgress::RestartCycle { node, source });
            }
          }
          FlowDirective::ExecuteNode { node, .. } | FlowDirective::EnterSubflow { node, .. } => {
            if !attempts
              .iter()
              .any(|successor| successor.node_key() == &node && successor.number() > attempt.number())
            {
              ready.insert(node);
            }
          }
          FlowDirective::Complete { terminal: key, .. } => {
            if terminal
              .as_ref()
              .is_none_or(|(number, _, _)| *number < attempt.number())
            {
              terminal = Some((attempt.number(), key, attempt.id()));
            }
          }
        }
      }
    }
    if let Some(restart) = restart {
      ready = pending.iter().map(|row| row.node_key().clone()).collect();
      return Ok((
        pending
          .first()
          .map_or(restart, |row| FlowProgress::Node(row.node_key().clone())),
        ready,
      ));
    }
    let position = if let Some(pending) = pending.first() {
      FlowProgress::Node(pending.node_key().clone())
    } else if let Some(node) = ready.iter().next() {
      FlowProgress::Node(node.clone())
    } else if let Some((_, key, source)) = terminal {
      FlowProgress::Terminal { key, source }
    } else {
      FlowProgress::Waiting
    };
    Ok((position, ready))
  }
}
