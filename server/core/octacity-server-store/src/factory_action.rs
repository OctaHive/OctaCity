//! Pure descriptions over retained execution authority; no queue, provider or control owner.
use crate::FactoryRunSnapshot;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use serde::{Deserialize, Serialize};

/// Existing control family and its exact admitted responses, never a lifecycle setter.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FactoryNodeActionKind {
  /// A generic response only when binding is absent; a bound operation must use its typed control.
  Respond {
    /// Declared outcome keys and immutable response schema references, without input bodies.
    outcomes: Vec<FlowOutcomeDefinition>,
    /// Exact installed operation/disposition mapping when this gate is purpose-bound.
    binding: Option<FlowHumanGateBinding>,
    /// Exact admitted response shapes only; no input schema, Context or response values.
    schemas: Vec<FlowDataSchema>,
  },
  /// One explicit technical retry permitted by the original node policy and remaining budget.
  Retry,
}

/// One currently eligible target at a trusted observation time; it grants no permission.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryNodeActionDescription {
  /// Original immutable Work; legacy unlinked executions remain valid.
  pub work_id: WorkEnvelopeId,
  /// Owning aggregate, never a latest-run hint.
  pub run_id: FactoryRunId,
  /// Required Run precondition; an Issue revision cannot substitute it.
  pub run_version: FactoryRunVersion,
  /// Exact root or nested Flow execution.
  pub flow_run_id: FlowRunId,
  /// Original requirements cycle.
  pub cycle_id: WorkflowCycleId,
  /// Exact execution/control target, independent of node names.
  pub node_attempt_id: NodeAttemptId,
  /// Immutable definition containing this node contract.
  pub contract: FlowDefinitionRef,
  /// Caller-configured display key, never interpreted as dispatch authority.
  pub node_key: FactoryKey,
  /// Exact frozen input identity, without private payload or Context.
  pub input_digest: FactoryDigest,
  /// Candidate from the attempt's frozen typed Context, if one was selected.
  pub candidate: Option<CandidateSubject>,
  /// Original time at which this action became available.
  pub actionable_at: Timestamp,
  /// Inclusive response deadline; absent for retries without an admitted response window.
  pub deadline: Option<Timestamp>,
  /// Exact admitted command/response family.
  pub kind: FactoryNodeActionKind,
}
impl FactoryNodeActionDescription {
  /// Describes an already authorized retained snapshot without reads or side effects.
  /// Consumers must revalidate current visibility/time/version; controls remain independently authoritative.
  #[must_use]
  pub fn from_snapshot(snapshot: &FactoryRunSnapshot, attempt_id: NodeAttemptId, at: Timestamp) -> Option<Self> {
    let lifecycle = snapshot
      .lifecycle_checkpoints
      .iter()
      .find(|row| row.id == snapshot.current.lifecycle_checkpoint_id)?;
    if at < lifecycle.recorded_at {
      return None;
    }
    let attempt = snapshot.flow.attempts.iter().find(|row| row.id() == attempt_id)?;
    let input = snapshot
      .flow
      .data
      .inputs
      .iter()
      .find(|row| row.digest().ok() == Some(attempt.input_digest()))?;
    let (kind, actionable_at, deadline) =
      if let Ok(node) = crate::factory_node_response_contract(snapshot, attempt_id, at) {
        let outcomes = node
          .outcomes()
          .iter()
          .filter(|outcome| {
            node.operator_requests().is_none_or(|binding| {
              binding
                .dispositions()
                .any(|choice| binding.outcome(choice) == Some(outcome.key()))
            })
          })
          .cloned()
          .collect::<Vec<_>>();
        let schemas = snapshot
          .admitted_flow
          .data_schemas()
          .iter()
          .filter(|schema| outcomes.iter().any(|outcome| outcome.schema() == schema.reference()))
          .cloned()
          .collect();
        (
          FactoryNodeActionKind::Respond {
            outcomes,
            schemas,
            binding: node.operator_requests().cloned(),
          },
          attempt.started_at(),
          Some(attempt.deadline()),
        )
      } else {
        crate::validate_factory_node_retry(snapshot, attempt_id, at).ok()?;
        let stopped = snapshot
          .flow
          .data
          .stops
          .iter()
          .find(|row| row.node_attempt_id() == attempt_id)?;
        (FactoryNodeActionKind::Retry, stopped.observed_at(), None)
      };
    Some(Self {
      work_id: snapshot.work.id(),
      run_id: snapshot.run.id(),
      run_version: snapshot.run.version(),
      flow_run_id: attempt.flow_run_id(),
      cycle_id: attempt.workflow_cycle_id(),
      node_attempt_id: attempt.id(),
      contract: input.definition(),
      node_key: attempt.node_key().clone(),
      input_digest: attempt.input_digest(),
      candidate: input
        .context()
        .and_then(|context| context.subject().candidate())
        .cloned(),
      actionable_at,
      deadline,
      kind,
    })
  }
}
