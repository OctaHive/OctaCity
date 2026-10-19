//! Reconciled read freshness; opaque provider update IDs never establish ordering.
use crate::*;
use octacity_server_domain::{IssueId, Timestamp};
use serde::{Deserialize, Serialize};

/// Safe last accepted observation and an unconfirmed changed snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueSourceRefreshState {
  /// Explicit pull access for this retained link, independent of execution inputs.
  pub enabled: bool,
  /// Monotonic reviewed pull policy generation; invalidates old read owners.
  pub generation: u64,
  /// Last reconciled provenance; unchanged rereads advance the watermark only.
  pub accepted: IssueSourceObservation,
  /// First independent read awaiting confirmation of the same complete snapshot.
  pub pending: Option<IssueSourceObservation>,
}
/// Classified observation or pull availability, independently of Issue status and execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueSourceRefreshDisposition {
  /// Pull was disabled at publication; retain a replayable skip without advancing freshness.
  PullDisabled,
  /// Changed snapshot confirmed by an independent later read.
  Fresh,
  /// Same snapshot with a later trusted read time.
  Unchanged,
  /// Original receipt was already accepted.
  Duplicate,
  /// Observation precedes the retained watermark.
  Stale,
  /// Changed opaque version is not yet independently confirmed.
  Uncertain,
}
/// Exact safe result; task content is read separately.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IssueSourceRefreshOutcome {
  /// Retained local task.
  pub issue_id: IssueId,
  /// Observation classification, never lifecycle authority.
  pub disposition: IssueSourceRefreshDisposition,
}
/// Trusted installed read validated by the application, with no execution target.
#[derive(Clone)]
pub struct RefreshIssueSource {
  /// Optional original bounded read fence; checked atomically before new publication.
  pub read_claim: Option<crate::IssueSourceReadClaim>,
  /// Exact retained native identity with the authorized current read binding.
  pub selection: IssueSourceSelection,
  /// Current visible task.
  pub issue_id: IssueId,
  /// Safe original provenance.
  pub observation: IssueSourceObservation,
  /// Bounded normalized initial fields; only existing source ownership permits replacement.
  pub content: IssueContent,
  /// Original command key.
  pub idempotency_key: IdempotencyKey,
  /// Trusted post-read time.
  pub refreshed_at: Timestamp,
}
impl IssueSourceRefreshState {
  /// Initializes retained provenance; reads still require an explicit authorized invocation.
  pub fn new(accepted: IssueSourceObservation) -> Self {
    Self {
      enabled: true,
      generation: 1,
      accepted,
      pending: None,
    }
  }
  /// Advances provenance conservatively; arrival time and opaque IDs are not provider versions.
  pub fn observe(&mut self, observation: &IssueSourceObservation) -> IssueSourceRefreshDisposition {
    use IssueSourceRefreshDisposition::*;
    if !self.enabled {
      return PullDisabled;
    }
    if observation == &self.accepted {
      return Duplicate;
    }
    if observation.observed_at <= self.accepted.observed_at
      || self
        .pending
        .as_ref()
        .is_some_and(|pending| observation.observed_at <= pending.observed_at)
    {
      return Stale;
    }
    let disposition = if observation.snapshot_digest == self.accepted.snapshot_digest {
      Unchanged
    } else if self.pending.as_ref().is_some_and(|pending| {
      pending.snapshot_digest == observation.snapshot_digest && pending.source == observation.source
    }) {
      Fresh
    } else {
      self.pending = Some(observation.clone());
      return Uncertain;
    };
    self.accepted = observation.clone();
    self.pending = None;
    disposition
  }
}
impl IssueRevision {
  /// Refreshes supported already source-owned text, preserving all local fields and business status.
  pub fn refreshed_source(
    &self,
    request: &RefreshIssueSource,
    policy: &IssueFieldPolicy,
  ) -> Result<Option<Self>, StoreError> {
    let mut content = self.content.clone();
    if policy.source_owned.contains(&IssueField::Title) {
      content.title = request.content.title.clone();
    }
    if policy.source_owned.contains(&IssueField::Description) {
      content.description = request.content.description.clone();
    }
    if content == self.content {
      return Ok(None);
    }
    // Reuse ordinary requirements revision and applicability policy after narrowing permitted fields.
    self
      .edited(
        &EditIssue {
          intent: EditIssueIntent {
            project_id: self.project_id,
            id: self.id,
            expected_version: self.version,
            content,
          },
          idempotency_key: request.idempotency_key.clone(),
          edited_at: request.refreshed_at,
        },
        &IssueFieldPolicy::default(),
      )
      .map(Some)
  }
}
/// Optional persistence capability; manual Issue handlers have no dependency on it.
#[async_trait::async_trait]
pub trait IssueSourceRefreshStore: IssueSourceStore {
  /// Reads safe retained freshness/control metadata in its original native partition.
  async fn issue_source_refresh_state(
    &self,
    project: octacity_server_domain::ProjectId,
    issue: IssueId,
    scope: &ManagementSecurityScope,
  ) -> Result<Option<IssueSourceRefreshState>, StoreError>;
  /// Changes only pull enablement under an exact reviewed generation and shared publication.
  async fn set_issue_source_pull(
    &self,
    request: ManagementMutation<SetIssueSourcePull>,
  ) -> Result<IssueSourcePullOutcome, StoreError>;
  /// Atomically classifies, retains provenance and, only after reconciliation, updates owned text.
  /// Disabled pull retains a replayable skip without advancing accepted or pending freshness.
  async fn refresh_issue_source(
    &self,
    request: ManagementMutation<RefreshIssueSource>,
  ) -> Result<IssueSourceRefreshOutcome, StoreError>;
}

/// Reviewed link pull control; it changes no ownership, source identity or execution setting.
#[derive(Clone)]
pub struct SetIssueSourcePull {
  /// Owning Project.
  pub project_id: octacity_server_domain::ProjectId,
  /// Visible source-linked task.
  pub issue_id: IssueId,
  /// Exact current control generation.
  pub expected_generation: u64,
  /// Whether explicit link refresh is allowed.
  pub enabled: bool,
  /// Original scoped command key.
  pub idempotency_key: IdempotencyKey,
  /// Trusted publication time.
  pub changed_at: Timestamp,
}
/// Safe original pull control receipt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IssueSourcePullOutcome {
  /// Applied explicit pull permission.
  pub enabled: bool,
  /// New monotonic control generation.
  pub generation: u64,
}
