//! Bounded offline association of retained execution history; never called by a GET.
use crate::{IssueWorkAssociation, StoreError};
use octacity_server_domain::{IssueId, ProjectId, Timestamp};
use octacity_server_factory::{FactoryDigest, WorkEnvelopeId};

/// Read-only original Artifact lineage, independent of reviewed execution requests.
#[derive(Clone, Debug)]
pub struct IssueLegacyWorkPage {
  /// Exact associations; requirements/request identities remain absent.
  pub items: Vec<IssueWorkAssociation>,
  /// Membership and Issue revision fence, containing no task body.
  pub observation: FactoryDigest,
  /// Exclusive immutable Work position in deterministic identity order.
  pub next: Option<WorkEnvelopeId>,
}

/// Maintenance only: no admission, Source observation, Reporter or Run mutation.
#[async_trait::async_trait]
pub trait IssueLegacyStore: Send + Sync {
  /// Enumerates only original legacy associations; current callers authorize the Issue first.
  /// Membership or Issue revision changes reject stale continuation rather than mix observations.
  async fn legacy_issue_work(
    &self,
    project: ProjectId,
    issue: IssueId,
    after: Option<WorkEnvelopeId>,
    observation: Option<FactoryDigest>,
    limit: u16,
  ) -> Result<IssueLegacyWorkPage, StoreError>;
  /// Atomically creates and associates at most `limit` retained, unassociated Work.
  /// Membership itself is the restart checkpoint: upgrade Work and newly settled
  /// Source Work share recovery; original input authority stays with Work.
  async fn backfill_legacy_issue_work(
    &self,
    limit: u16,
    at: Timestamp,
  ) -> Result<Vec<IssueWorkAssociation>, StoreError>;
}
