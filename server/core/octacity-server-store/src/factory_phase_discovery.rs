use crate::{FactoryRunListVisibility, PhasePoolEntry, StoreError};
use octacity_server_domain::ProjectId;
use octacity_server_factory::{
  FactoryDigest, FactoryKey, FactoryRunId, FlowRunId, PhasePoolOrder, PhasePoolPolicy, WorkEnvelopeId, WorkflowCycleId,
};
use std::{fmt, str::FromStr};

/// Self-contained exclusive phase ordering position. It remains usable after selection.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PhaseReadyPagePosition {
  /// Exact immutable pool policy.
  pub policy: FactoryDigest,
  /// Frozen operator policy order, with descending dimensions negated.
  pub rank: [i64; 3],
  /// Immutable Work tie breaker.
  pub work_id: WorkEnvelopeId,
  /// Run tie breaker.
  pub run_id: FactoryRunId,
  /// Nested Flow tie breaker.
  pub flow_run_id: FlowRunId,
  /// Workflow Cycle tie breaker.
  pub cycle_id: WorkflowCycleId,
  /// Logical node tie breaker.
  pub node: FactoryKey,
  /// Exact ready entry tie breaker.
  pub entry_id: FactoryDigest,
}
impl PhaseReadyPagePosition {
  /// Derives the complete stable ordering tuple without loading a cursor anchor.
  #[must_use]
  pub fn of(policy: &PhasePoolPolicy, entry: &PhasePoolEntry) -> Self {
    Self {
      policy: policy.digest(),
      rank: std::array::from_fn(|i| match policy.order[i] {
        PhasePoolOrder::Severity => -(entry.input.severity as i64),
        PhasePoolOrder::ProjectPriority => -i64::from(entry.input.project_priority),
        PhasePoolOrder::Age => entry.admitted_at.unix_millis(),
      }),
      work_id: entry.work_id,
      run_id: entry.input.run_id,
      flow_run_id: entry.input.flow_run_id,
      cycle_id: entry.input.cycle_id,
      node: entry.input.node.clone(),
      entry_id: entry.id,
    }
  }
}
impl fmt::Display for PhaseReadyPagePosition {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      f,
      "1:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
      self.policy,
      self.rank[0],
      self.rank[1],
      self.rank[2],
      self.work_id,
      self.run_id,
      self.flow_run_id,
      self.cycle_id,
      self.node,
      self.entry_id
    )
  }
}
impl FromStr for PhaseReadyPagePosition {
  type Err = StoreError;
  fn from_str(value: &str) -> Result<Self, Self::Err> {
    let invalid = crate::factory_phase_pool::pool_invalid;
    if value.len() > 512 {
      return Err(invalid());
    }
    let parts = value.split(':').collect::<Vec<_>>();
    if parts.len() != 11 || parts[0] != "1" {
      return Err(invalid());
    }
    let position = Self {
      policy: parts[1].parse().map_err(|_| invalid())?,
      rank: [
        parts[2].parse().map_err(|_| invalid())?,
        parts[3].parse().map_err(|_| invalid())?,
        parts[4].parse().map_err(|_| invalid())?,
      ],
      work_id: parts[5].parse().map_err(|_| invalid())?,
      run_id: parts[6].parse().map_err(|_| invalid())?,
      flow_run_id: parts[7].parse().map_err(|_| invalid())?,
      cycle_id: parts[8].parse().map_err(|_| invalid())?,
      node: parts[9].parse().map_err(|_| invalid())?,
      entry_id: parts[10].parse().map_err(|_| invalid())?,
    };
    if position.to_string() != value {
      return Err(invalid());
    }
    Ok(position)
  }
}
/// Bounded Project-scoped or global current phase pool discovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListPhaseReadyEntries {
  project_id: Option<ProjectId>,
  policy: FactoryDigest,
  after: Option<PhaseReadyPagePosition>,
  limit: u16,
  visibility: FactoryRunListVisibility,
}
impl ListPhaseReadyEntries {
  /// Validates immutable policy scope, canonical continuation and page bounds.
  pub fn new(
    project_id: Option<ProjectId>,
    policy: FactoryDigest,
    after: Option<PhaseReadyPagePosition>,
    limit: u16,
    visibility: FactoryRunListVisibility,
  ) -> Result<Self, StoreError> {
    if limit == 0 || limit > crate::MAX_FACTORY_RUN_PAGE_SIZE || after.as_ref().is_some_and(|p| p.policy != policy) {
      return Err(crate::factory_phase_pool::pool_invalid());
    }
    Ok(Self {
      project_id,
      policy,
      after,
      limit,
      visibility,
    })
  }
  /// Returns the optional Project scope.
  #[must_use]
  pub const fn project_id(&self) -> Option<ProjectId> {
    self.project_id
  }
  /// Returns the exact pool policy.
  #[must_use]
  pub const fn policy(&self) -> FactoryDigest {
    self.policy
  }
  /// Borrows the exclusive order position.
  #[must_use]
  pub const fn after(&self) -> Option<&PhaseReadyPagePosition> {
    self.after.as_ref()
  }
  /// Returns the bounded positive page size.
  #[must_use]
  pub const fn limit(&self) -> u16 {
    self.limit
  }
  /// Returns visibility that must precede filtering, ordering and counts.
  #[must_use]
  pub const fn visibility(&self) -> &FactoryRunListVisibility {
    &self.visibility
  }
}
/// Visible current phase-ready entries in immutable policy order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PhaseReadyPage {
  /// Matching visible count before continuation.
  pub total: u64,
  /// Ready entries in deterministic policy order.
  pub items: Vec<PhasePoolEntry>,
  /// Self-contained exclusive continuation position.
  pub next_cursor: Option<PhaseReadyPagePosition>,
}
