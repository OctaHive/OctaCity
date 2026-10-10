use crate::{
  FactoryError, FactoryKey, FlowDirective, FlowInterpreter, FlowRun, FlowRuntimeHistory, NodeAttemptId, WorkflowCycle,
};
use std::collections::BTreeSet;

/// Current configured control position derived only from retained runtime history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FlowProgress {
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
    let attempts = history
      .attempts
      .iter()
      .filter(|row| row.flow_run_id() == flow.id() && row.workflow_cycle_id() == cycle.id())
      .collect::<Vec<_>>();
    if let Some(pending) = attempts
      .iter()
      .filter(|attempt| {
        !history
          .completions
          .iter()
          .any(|row| row.node_attempt_id() == attempt.id())
      })
      .min_by_key(|row| row.number())
    {
      return Ok(FlowProgress::Node(pending.node_key().clone()));
    }
    if attempts.is_empty() {
      return Ok(match self.start(flow)? {
        FlowDirective::ExecuteNode { node, .. } | FlowDirective::EnterSubflow { node, .. } => FlowProgress::Node(node),
        FlowDirective::Complete { .. } => FlowProgress::Waiting,
      });
    }
    let mut ready = BTreeSet::new();
    let mut terminal = None;
    for attempt in &attempts {
      let Some(completion) = history
        .completions
        .iter()
        .find(|row| row.node_attempt_id() == attempt.id())
      else {
        continue;
      };
      for directive in self.advance(flow, cycle, attempt, completion, history.attempts, history.completions)? {
        match directive {
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
    Ok(if let Some(node) = ready.into_iter().next() {
      FlowProgress::Node(node)
    } else if let Some((_, key, source)) = terminal {
      FlowProgress::Terminal { key, source }
    } else {
      FlowProgress::Waiting
    })
  }
}
