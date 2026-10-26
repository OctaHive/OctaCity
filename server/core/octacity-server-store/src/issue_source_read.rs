//! Execution-neutral targets sharing the existing Source lease, page and scheduling model.
use crate::*;
use octacity_server_domain::{IssueId, ProjectId, Timestamp};
use octacity_server_factory::{FactoryDigest, WorkSourceBinding};

/// Exact installed read selection, independent of execution input enablement.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueSourceReadTarget {
  /// Server-captured pull generation; callers pass zero and acquisition resolves current authority.
  pub pull_generation: u64,
  /// Exact reviewed import revision; absent for legacy discovery and retained-link refresh.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub import: Option<PublishedIssueSourceImport>,
  /// Authorized installed profile; a refresh profile can use a broader selector.
  pub source: WorkSourceBinding,
  /// A retained link to refresh, or a bounded discovery/import pass.
  pub issue_id: Option<IssueId>,
}
impl IssueSourceReadTarget {
  /// Original partitioned read-journal identity; no Factory input or target is fabricated.
  pub fn identity(&self, project: ProjectId, scope: &ManagementSecurityScope) -> Result<FactoryDigest, StoreError> {
    if let Some(import) = &self.import {
      return Ok(FactoryDigest::sha256(
        "octacity.issue.managed-source-read.v1",
        &[
          project.to_string().as_bytes(),
          scope.as_str().as_bytes(),
          &serde_json::to_vec(&(&self.source, self.issue_id, import.id)).map_err(|_| StoreError::Unavailable)?,
        ],
      ));
    }
    Ok(FactoryDigest::sha256(
      "octacity.issue.source-read.v1",
      &[
        project.to_string().as_bytes(),
        scope.as_str().as_bytes(),
        &serde_json::to_vec(&(&self.source, self.issue_id)).map_err(|_| StoreError::Unavailable)?,
      ],
    ))
  }
}
/// Exact reusable scan proof for an execution-neutral read.
pub type IssueSourceReadClaim = WorkSourceScanClaim<IssueSourceReadTarget>;
/// Original durable progress and retry state.
pub type IssueSourceReadScan = WorkSourceScan<IssueSourceReadTarget>;
/// Optional read progress; independent of all Factory admission ports.
#[async_trait::async_trait]
pub trait IssueSourceReadStore: IssueSourceRefreshStore {
  /// Acquires the shared fenced scan model for a visible Project/native read target.
  async fn claim_issue_source_read(
    &self,
    scope: &ManagementSecurityScope,
    request: ClaimWorkSourceScan<IssueSourceReadTarget>,
  ) -> Result<Option<IssueSourceReadScan>, StoreError>;
  /// Checks the exact owner and advances only a covered position.
  async fn checkpoint_issue_source_read(
    &self,
    scope: &ManagementSecurityScope,
    claim: IssueSourceReadClaim,
    at: Timestamp,
    expected: Option<WorkSourceProgress>,
    progress: WorkSourceProgress,
  ) -> Result<(), StoreError>;
  /// Releases the current read owner with safe retained retry/cadence facts.
  async fn finish_issue_source_read(
    &self,
    scope: &ManagementSecurityScope,
    claim: IssueSourceReadClaim,
    at: Timestamp,
    schedule: WorkSourceSchedule,
  ) -> Result<(), StoreError>;
}
