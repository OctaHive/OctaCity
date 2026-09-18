use std::collections::{BTreeMap, BTreeSet};

use octacity_server_domain::JobId;
use octacity_server_job::{JobEvent, JobState};
use octacity_server_pipeline::{DependencyDecision, DependencyOutcome, DependencyPolicy};

use crate::{AttemptState, BuildState};

/// One Job and its immutable fan-in captured from authoritative storage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobGraphNode {
  id: JobId,
  state: JobState,
  dependencies: Vec<JobId>,
  dependency_policy: DependencyPolicy,
}

impl JobGraphNode {
  /// Creates one graph node and canonicalizes its direct dependencies.
  pub fn new(
    id: JobId,
    state: JobState,
    mut dependencies: Vec<JobId>,
    dependency_policy: DependencyPolicy,
  ) -> Result<Self, OrchestrationError> {
    dependencies.sort_unstable();
    if dependencies.binary_search(&id).is_ok() {
      return Err(OrchestrationError::SelfDependency { job_id: id });
    }
    if dependencies.windows(2).any(|pair| pair[0] == pair[1]) {
      return Err(OrchestrationError::DuplicateDependency { job_id: id });
    }
    Ok(Self {
      id,
      state,
      dependencies,
      dependency_policy,
    })
  }

  /// Returns the stable Job identity.
  #[must_use]
  pub const fn id(&self) -> JobId {
    self.id
  }

  /// Returns the currently persisted Job state.
  #[must_use]
  pub const fn state(&self) -> JobState {
    self.state
  }

  /// Borrows the canonical direct dependency list.
  #[must_use]
  pub fn dependencies(&self) -> &[JobId] {
    &self.dependencies
  }

  /// Returns the immutable fan-in policy.
  #[must_use]
  pub const fn dependency_policy(&self) -> DependencyPolicy {
    self.dependency_policy
  }
}

/// One deterministic state change selected for a dependency-blocked Job.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobGraphTransition {
  /// Job whose persisted state must change.
  job_id: JobId,
  /// New state selected through the Job state machine.
  state: JobState,
}

impl JobGraphTransition {
  /// Returns the Job selected for transition.
  #[must_use]
  pub const fn job_id(self) -> JobId {
    self.job_id
  }

  /// Returns the new Job state.
  #[must_use]
  pub const fn state(self) -> JobState {
    self.state
  }
}

/// Complete deterministic result of reconciling one persisted Attempt graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrchestrationDecision {
  /// Stably ordered dependency-driven Job transitions.
  job_transitions: Vec<JobGraphTransition>,
  /// State derived from the complete post-transition Job graph.
  attempt_state: AttemptState,
  /// Build state derived from the current Attempt state.
  build_state: BuildState,
}

impl OrchestrationDecision {
  /// Borrows the stably ordered dependency-driven Job transitions.
  #[must_use]
  pub fn job_transitions(&self) -> &[JobGraphTransition] {
    &self.job_transitions
  }

  /// Returns the state derived for the Attempt.
  #[must_use]
  pub const fn attempt_state(&self) -> AttemptState {
    self.attempt_state
  }

  /// Returns the state derived for the Build.
  #[must_use]
  pub const fn build_state(&self) -> BuildState {
    self.build_state
  }
}

/// Corrupt or inconsistent persisted facts that prevent safe reconciliation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrchestrationError {
  /// An Attempt graph contains no Jobs.
  EmptyGraph,
  /// The same Job identity occurs more than once.
  DuplicateJob {
    /// Repeated Job identity.
    job_id: JobId,
  },
  /// A Job directly depends on itself.
  SelfDependency {
    /// Invalid Job identity.
    job_id: JobId,
  },
  /// A Job repeats one direct dependency.
  DuplicateDependency {
    /// Invalid Job identity.
    job_id: JobId,
  },
  /// A dependency is absent from the same Attempt graph.
  UnknownDependency {
    /// Job containing the invalid edge.
    job_id: JobId,
    /// Missing predecessor identity.
    dependency_id: JobId,
  },
  /// The persisted dependency graph contains a cycle.
  CyclicGraph,
  /// A root Job is incorrectly persisted as dependency-blocked.
  BlockedRoot {
    /// Invalid root Job identity.
    job_id: JobId,
  },
  /// A selected transition was rejected by the Job state machine.
  InvalidJobTransition {
    /// Job whose stored state is inconsistent with the decision.
    job_id: JobId,
  },
  /// Durable cancellation exists but an active Job was not targeted by it.
  CancellationNotPropagated {
    /// Job whose persisted state contradicts the Build cancellation intent.
    job_id: JobId,
  },
}

/// Reconciles one complete persisted Attempt graph to a deterministic fixed point.
///
/// Only dependency-blocked Jobs are changed. A failure may therefore skip an
/// arbitrary descendant chain in one decision, while Jobs allowed by their
/// immutable policy become ready. Attempt and Build state are derived after all
/// skip propagation has settled, so replaying the same facts is a no-op with the
/// same aggregate result.
pub fn reconcile_job_graph(
  nodes: impl IntoIterator<Item = JobGraphNode>,
) -> Result<OrchestrationDecision, OrchestrationError> {
  reconcile(nodes, false)
}

/// Reconciles a graph after durable Build cancellation was requested.
///
/// Once every Job is terminal, cancellation intent dominates individual Job
/// outcomes so a late failure or success cannot revive or reclassify the Build.
pub fn reconcile_cancelled_job_graph(
  nodes: impl IntoIterator<Item = JobGraphNode>,
) -> Result<OrchestrationDecision, OrchestrationError> {
  reconcile(nodes, true)
}

/// Validates one complete materialized Job dependency graph.
///
/// Store adapters and read projections use the same structural decision as
/// reconciliation so duplicate edges, missing predecessors, and cycles cannot
/// acquire layer-specific interpretations.
pub fn validate_job_graph(nodes: impl IntoIterator<Item = JobGraphNode>) -> Result<(), OrchestrationError> {
  let graph = collect_graph(nodes)?;
  validate_graph(&graph)
}

fn reconcile(
  nodes: impl IntoIterator<Item = JobGraphNode>,
  cancellation_requested: bool,
) -> Result<OrchestrationDecision, OrchestrationError> {
  let mut graph = collect_graph(nodes)?;
  validate_graph(&graph)?;
  if cancellation_requested
    && let Some(job_id) = graph.iter().find_map(|(job_id, node)| {
      matches!(
        node.state,
        JobState::Blocked | JobState::Ready | JobState::Leased | JobState::Running
      )
      .then_some(*job_id)
    })
  {
    return Err(OrchestrationError::CancellationNotPropagated { job_id });
  }
  let mut job_transitions = Vec::new();

  loop {
    let blocked: Vec<_> = graph
      .iter()
      .filter_map(|(job_id, node)| (node.state == JobState::Blocked).then_some(*job_id))
      .collect();
    let mut changed = false;
    for job_id in blocked {
      let node = graph
        .get(&job_id)
        .ok_or(OrchestrationError::InvalidJobTransition { job_id })?;
      let outcomes = node
        .dependencies
        .iter()
        .map(|dependency| {
          graph
            .get(dependency)
            .map(|dependency| dependency_outcome(dependency.state))
            .ok_or(OrchestrationError::UnknownDependency {
              job_id,
              dependency_id: *dependency,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
      let decision = node.dependency_policy.decide(outcomes);
      let event = match decision {
        DependencyDecision::Blocked => continue,
        DependencyDecision::Ready => JobEvent::DependenciesSatisfied,
        DependencyDecision::Skipped => JobEvent::DependencyFailed,
      };
      let state = node
        .state
        .transition(event)
        .map_err(|_| OrchestrationError::InvalidJobTransition { job_id })?;
      graph
        .get_mut(&job_id)
        .ok_or(OrchestrationError::InvalidJobTransition { job_id })?
        .state = state;
      job_transitions.push(JobGraphTransition { job_id, state });
      changed = true;
    }
    if !changed {
      break;
    }
  }

  job_transitions.sort_unstable_by_key(|transition| transition.job_id);
  let attempt_state = derive_attempt_state(graph.values().map(|node| node.state), cancellation_requested);
  let build_state = match attempt_state {
    AttemptState::Succeeded => BuildState::Succeeded,
    AttemptState::Failed => BuildState::Failed,
    AttemptState::Cancelled => BuildState::Cancelled,
    AttemptState::Created | AttemptState::Running => BuildState::Running,
  };
  Ok(OrchestrationDecision {
    job_transitions,
    attempt_state,
    build_state,
  })
}

fn collect_graph(
  nodes: impl IntoIterator<Item = JobGraphNode>,
) -> Result<BTreeMap<JobId, JobGraphNode>, OrchestrationError> {
  let mut graph = BTreeMap::new();
  for node in nodes {
    let job_id = node.id;
    if graph.insert(job_id, node).is_some() {
      return Err(OrchestrationError::DuplicateJob { job_id });
    }
  }
  if graph.is_empty() {
    return Err(OrchestrationError::EmptyGraph);
  }
  Ok(graph)
}

fn validate_graph(graph: &BTreeMap<JobId, JobGraphNode>) -> Result<(), OrchestrationError> {
  for node in graph.values() {
    if node.dependencies.is_empty() && node.state == JobState::Blocked {
      return Err(OrchestrationError::BlockedRoot { job_id: node.id });
    }
    if let Some(dependency_id) = node
      .dependencies
      .iter()
      .find(|dependency| !graph.contains_key(dependency))
    {
      return Err(OrchestrationError::UnknownDependency {
        job_id: node.id,
        dependency_id: *dependency_id,
      });
    }
  }

  let mut visited = BTreeSet::new();
  loop {
    let before = visited.len();
    for node in graph.values() {
      if !visited.contains(&node.id) && node.dependencies.iter().all(|dependency| visited.contains(dependency)) {
        visited.insert(node.id);
      }
    }
    if visited.len() == graph.len() {
      return Ok(());
    }
    if visited.len() == before {
      return Err(OrchestrationError::CyclicGraph);
    }
  }
}

const fn dependency_outcome(state: JobState) -> DependencyOutcome {
  match state {
    JobState::Succeeded => DependencyOutcome::Succeeded,
    JobState::Failed => DependencyOutcome::Failed,
    JobState::Cancelled => DependencyOutcome::Cancelled,
    JobState::Skipped => DependencyOutcome::Skipped,
    JobState::Blocked | JobState::Ready | JobState::Leased | JobState::Running | JobState::Cancelling => {
      DependencyOutcome::Pending
    }
  }
}

fn derive_attempt_state(states: impl IntoIterator<Item = JobState>, cancellation_requested: bool) -> AttemptState {
  let mut all_terminal = true;
  let mut failed = false;
  let mut cancelled = false;
  for state in states {
    all_terminal &= state.is_terminal();
    failed |= state == JobState::Failed;
    cancelled |= state == JobState::Cancelled;
  }
  if !all_terminal {
    AttemptState::Running
  } else if cancellation_requested {
    AttemptState::Cancelled
  } else if failed {
    AttemptState::Failed
  } else if cancelled {
    AttemptState::Cancelled
  } else {
    AttemptState::Succeeded
  }
}
