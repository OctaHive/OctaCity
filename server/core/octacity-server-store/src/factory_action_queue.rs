//! Authorized collection reads over the same derived action index as exact Run inspection.
use crate::{
  FactoryActionProjectionCoverage, FactoryNodeActionDescription, FactoryRunListVisibility, IssueListVisibility,
  IssueWorkAssociation, ProjectListVisibility, StoreError,
};
use octacity_server_domain::{IssueId, ProjectId, Timestamp};
use octacity_server_factory::{FactoryRunId, NodeAttemptId};
use serde::{Deserialize, Serialize};
/// Stable ordering for exact execution targets, independent of mutable Issue metadata.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(deny_unknown_fields)]
pub struct FactoryActionQueuePosition {
  /// Original owning execution.
  pub run_id: FactoryRunId,
  /// Exact response or retry target.
  pub node_attempt_id: NodeAttemptId,
}
/// Independent current visibility is applied before coverage, counts and pagination.
#[derive(Clone, Debug)]
pub struct ReadFactoryActionQueue {
  /// Optional owning Project; absent means the shared global collection.
  pub project_id: Option<ProjectId>,
  /// Optional Issue in the selected Project, across all associated executions.
  pub issue_id: Option<IssueId>,
  /// Exclusive last visible exact target.
  pub after: Option<FactoryActionQueuePosition>,
  /// Common diagnostic page bound.
  pub limit: u16,
  /// Trusted current time; cursor data cannot extend a response deadline.
  pub observed_at: Timestamp,
  /// Independent Run visibility, including unlinked Work.
  pub runs: FactoryRunListVisibility,
  /// Current Issue visibility; hidden linked executions are excluded entirely.
  pub issues: IssueListVisibility,
  /// Current owning Project visibility.
  pub projects: ProjectListVisibility,
}
impl ReadFactoryActionQueue {
  /// Revalidates adapter entry points, including manually constructed requests.
  pub fn validate(&self) -> Result<(), StoreError> {
    crate::validate_action_projection_limit(self.limit)?;
    if self.issue_id.is_some() && self.project_id.is_none() {
      return Err(crate::factory_action_projection::invalid_action_projection());
    }
    Ok(())
  }
}
/// Safe retained exact target with an optional verified local task association.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryActionQueueItem {
  /// Original Work owner, never inferred from the current Issue title.
  pub project_id: ProjectId,
  /// Original association; legacy unlinked Work remains usable.
  pub association: Option<IssueWorkAssociation>,
  /// Common current eligibility descriptor and strong Run precondition.
  pub action: FactoryNodeActionDescription,
}
impl FactoryActionQueueItem {
  /// Returns this immutable target's collection ordering position.
  pub fn position(&self) -> FactoryActionQueuePosition {
    FactoryActionQueuePosition {
      run_id: self.action.run_id,
      node_attempt_id: self.action.node_attempt_id,
    }
  }
}
/// Matching indexed actions; incomplete coverage never proves authoritative emptiness.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FactoryActionQueuePage {
  /// Indexed eligible visible count before pagination; coverage qualifies completeness.
  pub total: u64,
  /// Current source-version coverage over all matching visible executions.
  pub coverage: FactoryActionProjectionCoverage,
  /// At most the requested bound, without private input or response bodies.
  pub items: Vec<FactoryActionQueueItem>,
  /// Exclusive last visible target when more matching rows exist.
  pub next: Option<FactoryActionQueuePosition>,
}
/// Read-only shared collection; commands continue through the original control ports.
#[async_trait::async_trait]
pub trait FactoryActionQueueStore: Send + Sync {
  /// Joins current source versions and visibility before counts/order/continuation.
  async fn read_factory_action_queue(
    &self,
    request: ReadFactoryActionQueue,
  ) -> Result<FactoryActionQueuePage, StoreError>;
}
