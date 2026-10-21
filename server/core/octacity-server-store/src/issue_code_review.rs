//! Optional read-only review associations, separate from primary tickets and execution authority.
use crate::*;
use octacity_server_domain::{IssueId, IssueVersion, ProjectId, Timestamp};
use octacity_server_factory::{FactoryDigest, FactorySafeText, WorkSourceBinding};
use serde::{Deserialize, Serialize};
/// An exact installed code-repository reader and canonical native review identity; URLs are display only.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueCodeReviewSelection {
  /// Independently installed review reader, not a task or Delivery grant.
  pub source: WorkSourceBinding,
  /// Stable native code repository and PR/MR identity.
  pub item: WorkSourceItem,
}
impl IssueCodeReviewSelection {
  /// Stable installed/native code repository identity for current metadata visibility.
  pub fn repository_identity(&self) -> FactoryDigest {
    Self::repository_identity_for(self.source.installation.identity(), &self.item.resource)
  }
  /// Computes the same metadata identity for an installed repository before selecting a review.
  pub fn repository_identity_for(
    installation: &octacity_server_factory::FactoryKey,
    repository: &octacity_server_factory::ExternalWorkIdentity,
  ) -> FactoryDigest {
    FactoryDigest::sha256(
      "octacity.issue-code-review-repository.v1",
      &[installation.as_str().as_bytes(), repository.as_str().as_bytes()],
    )
  }
  /// Canonical identity ignores display text, credentials and mutable reader configuration.
  pub fn identity(&self, project: ProjectId, issue: IssueId) -> FactoryDigest {
    FactoryDigest::sha256(
      "octacity.issue-code-review.v1",
      &[
        project.as_uuid().as_bytes(),
        issue.as_uuid().as_bytes(),
        self.source.installation.identity().as_str().as_bytes(),
        self.item.resource.as_str().as_bytes(),
        self.item.item.as_str().as_bytes(),
      ],
    )
  }
}
/// Observational forge state, never a Factory Decision or Issue transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueCodeReviewState {
  /// Review remains open.
  Open,
  /// Closed without an asserted merge.
  Closed,
  /// Merged externally; confers no acceptance.
  Merged,
}
/// Bounded normalized safe facts. Missing fields mean unavailable, never successful/zero.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueCodeReviewObservation {
  /// Exact originally authorized repository/review.
  pub selection: IssueCodeReviewSelection,
  /// Provider observation time.
  pub observed_at: Timestamp,
  /// Visible title only, without body/transcript/private metadata.
  pub title: Option<FactorySafeText>,
  /// Display number, never canonical identity.
  pub number: Option<FactorySafeText>,
  /// Trusted HTTPS navigation link; never a fetch or permission target.
  pub url: Option<FactorySafeText>,
  /// Visible author; redacted/unsupported authors stay absent.
  pub author: Option<FactorySafeText>,
  /// Observed external state only.
  pub state: Option<IssueCodeReviewState>,
  /// Draft status if reported.
  pub draft: Option<bool>,
  /// Original immutable observed head.
  pub head: Option<octacity_server_domain::ImmutableRevision>,
  /// Original immutable observed base.
  pub base: Option<octacity_server_domain::ImmutableRevision>,
  /// Available changed-file count.
  pub changed_files: Option<u64>,
  /// Available added-line count.
  pub additions: Option<u64>,
  /// Available deleted-line count.
  pub deletions: Option<u64>,
  /// Safe external review summary; never accepted Factory evidence.
  pub review: Option<FactorySafeText>,
  /// Safe external CI summary; never a Factory Decision.
  pub checks: Option<FactorySafeText>,
  /// Bounded typed external CI for this original head. Absent legacy/unsupported detail stays unknown.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub check_details: Option<IssueExternalChecks>,
}
impl IssueCodeReviewObservation {
  /// Rejects unusable navigation without following it or acquiring provider authority.
  pub fn validate(&self) -> Result<(), StoreError> {
    if let Some(checks) = &self.check_details {
      checks.validate()?;
      if self.head.as_ref() != Some(&checks.head) || self.observed_at != checks.observed_at {
        return Err(invalid_issue_code_review());
      }
    }
    if let Some(value) = &self.url {
      let url = url::Url::parse(value.as_str()).map_err(|_| invalid_issue_code_review())?;
      if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
      {
        return Err(invalid_issue_code_review());
      }
    }
    Ok(())
  }
}
/// Optional lineage is verified by the existing execution reader before association.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueCodeReviewBinding {
  /// Exact retained Work/Run association.
  pub association: IssueWorkAssociation,
  /// Verified producing common record.
  pub record: FactoryDigest,
  /// Original candidate; later remote heads cannot retarget it.
  pub candidate: octacity_server_factory::CandidateSubject,
}
/// Origin is retained separately from external state and optional execution lineage.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IssueCodeReviewProvenance {
  /// Explicit operator association, attributed by management ingress.
  Explicit {
    /// Actual accepted actor.
    actor: AuditActor,
  },
  /// Verified existing Delivery publication receipt.
  Delivery {
    /// Original immutable receipt identity.
    receipt: FactoryDigest,
  },
}
/// Immutable task/review association; does not replace the Issue's primary source link.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueCodeReviewAssociation {
  /// Canonical association identity.
  pub id: FactoryDigest,
  /// Monotonic Issue-local association order.
  pub sequence: u64,
  /// Owning Project.
  pub project_id: ProjectId,
  /// Visible local task.
  pub issue_id: IssueId,
  /// Original independent reader/native selection.
  pub selection: IssueCodeReviewSelection,
  /// Verified immutable lineage, if explicitly selected.
  pub binding: Option<IssueCodeReviewBinding>,
  /// Explicit versus trusted Delivery origin.
  pub provenance: IssueCodeReviewProvenance,
  /// Original trusted association time.
  pub attached_at: Timestamp,
}
/// Reviewed explicit association. Display strings and task keys cannot supply identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachIssueCodeReviewIntent {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Current visible Issue.
  pub issue_id: IssueId,
  /// Reviewed current Issue revision.
  pub expected_version: IssueVersion,
  /// Independently authorized native review.
  pub selection: IssueCodeReviewSelection,
  /// Optional independently verified exact candidate/record.
  pub binding: Option<IssueCodeReviewBinding>,
}
/// Trusted store request after read and optional lineage verification.
#[derive(Clone, Debug)]
pub struct AttachIssueCodeReview {
  /// Original reviewed intent.
  pub intent: AttachIssueCodeReviewIntent,
  /// Verified safe provider observation; cannot change Issue fields.
  pub observation: IssueCodeReviewObservation,
  /// Trusted original provenance.
  pub provenance: IssueCodeReviewProvenance,
  /// Scoped original retry key.
  pub idempotency_key: IdempotencyKey,
  /// Trusted accepted association time.
  pub attached_at: Timestamp,
}
/// Atomic original receipt, preserved after later observations and restarts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IssueCodeReviewOutcome {
  /// Applied or exact original replay.
  pub disposition: MutationDisposition,
  /// Immutable original association.
  pub association: IssueCodeReviewAssociation,
}
/// Retained row; the application separately rechecks current reader availability.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IssueCodeReviewRow {
  /// Original immutable association.
  pub association: IssueCodeReviewAssociation,
  /// Latest retained safe observation, possibly stale or blocked.
  pub observation: Option<IssueCodeReviewObservation>,
}
/// Internal bounded observation cursor, wrapped by the common application opaque codec.
#[derive(Clone, Debug)]
pub struct ReadIssueCodeReviews {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Visible local task.
  pub issue_id: IssueId,
  /// Original accepted management partition.
  pub scope: ManagementSecurityScope,
  /// Current native repository metadata visibility, checked before membership/order/cursors.
  pub visibility: IssueCodeReviewRepositoryVisibility,
  /// Original observed membership; absent starts a fresh observation.
  pub boundary: Option<u64>,
  /// Original local association sequence.
  pub after: Option<u64>,
  /// Shared bounded page size.
  pub limit: u16,
}
/// Bounded original membership; no total count or implicit acceptance.
#[derive(Clone, Debug)]
pub struct IssueCodeReviewStorePage<T = IssueCodeReviewRow> {
  /// Safe retained rows.
  pub items: Vec<T>,
  /// Frozen association watermark.
  pub boundary: u64,
  /// Next local position, if more original members remain.
  pub next: Option<u64>,
}
/// Narrow metadata persistence; no provider, Factory mutation, merge or task transition port.
#[async_trait::async_trait]
pub trait IssueCodeReviewStore: IssueStore {
  /// Exact original scoped receipt before another protected read.
  async fn issue_code_review_replay(
    &self,
    scope: &ManagementSecurityScope,
    intent: &AttachIssueCodeReviewIntent,
    key: &IdempotencyKey,
  ) -> Result<Option<IssueCodeReviewOutcome>, StoreError>;
  /// Atomic canonical attach, safe audit and scoped replay.
  async fn attach_issue_code_review(
    &self,
    request: ManagementMutation<AttachIssueCodeReview>,
  ) -> Result<IssueCodeReviewOutcome, StoreError>;
  /// Reads one original association only within its owning Issue/security partition.
  async fn issue_code_review(
    &self,
    project: ProjectId,
    issue: IssueId,
    scope: &ManagementSecurityScope,
    id: FactoryDigest,
  ) -> Result<IssueCodeReviewAssociation, StoreError>;
  /// Original refresh acknowledgement, before dispatching another installed read.
  async fn issue_code_review_observation_replay(
    &self,
    scope: &ManagementSecurityScope,
    intent: &ObserveIssueCodeReviewIntent,
    key: &IdempotencyKey,
  ) -> Result<Option<IssueCodeReviewObservationOutcome>, StoreError>;
  /// Append-only safe metadata observation; exact replay never moves an accepted subject.
  async fn observe_issue_code_review(
    &self,
    request: ManagementMutation<ObserveIssueCodeReview>,
  ) -> Result<IssueCodeReviewObservationOutcome, StoreError>;
  /// Per-association retained head history in original sequence order.
  async fn issue_code_review_observations(
    &self,
    request: ReadIssueCodeReviews,
    association: FactoryDigest,
  ) -> Result<IssueCodeReviewStorePage<IssueCodeReviewObservationRecord>, StoreError>;
  /// Retained safe metadata only; never contacts a provider.
  async fn issue_code_reviews(&self, request: ReadIssueCodeReviews) -> Result<IssueCodeReviewStorePage, StoreError>;
}
pub(crate) fn invalid_issue_code_review() -> StoreError {
  StoreError::invalid(
    StoreOperation::AttachIssueCodeReview,
    StoreInputError::InvalidIssueCodeReview,
  )
}

/// Exact retained review target; refresh cannot select a different repository or review.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObserveIssueCodeReviewIntent {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Owning Issue.
  pub issue_id: IssueId,
  /// Immutable original association.
  pub association_id: FactoryDigest,
}
/// Trusted bounded read observation with common management replay/audit.
#[derive(Clone, Debug)]
pub struct ObserveIssueCodeReview {
  /// Original target, excluding mutable provider facts from replay equivalence.
  pub intent: ObserveIssueCodeReviewIntent,
  /// Verified safe reader output.
  pub observation: IssueCodeReviewObservation,
  /// Original scoped retry identity.
  pub idempotency_key: IdempotencyKey,
  /// Trusted acceptance time; does not rewrite provider observation time.
  pub recorded_at: Timestamp,
}
/// Immutable observation retained across moving heads and provider outages.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueCodeReviewObservationRecord {
  /// Content identity of the original normalized observation.
  pub id: FactoryDigest,
  /// Monotonic association-local history sequence.
  pub sequence: u64,
  /// Exact immutable original association.
  pub association_id: FactoryDigest,
  /// Safe original provider facts.
  pub observation: IssueCodeReviewObservation,
}
impl IssueCodeReviewObservationRecord {
  /// Canonical original bytes, independent of acknowledgement time or replay key.
  pub fn new(
    association_id: FactoryDigest,
    sequence: u64,
    observation: IssueCodeReviewObservation,
  ) -> Result<Self, StoreError> {
    observation.validate()?;
    let bytes = serde_json::to_vec(&observation).map_err(|_| StoreError::Unavailable)?;
    Ok(Self {
      id: FactoryDigest::sha256(
        "octacity.issue-code-review-observation.v1",
        &[&association_id.as_bytes(), &bytes],
      ),
      sequence,
      association_id,
      observation,
    })
  }
}
/// Original append-only observation receipt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IssueCodeReviewObservationOutcome {
  /// Applied or exact replay.
  pub disposition: MutationDisposition,
  /// Original sequence, head and observation time.
  pub observation: IssueCodeReviewObservationRecord,
}

impl AttachIssueCodeReview {
  /// Shared backend-neutral preconditions for one newly accepted attachment.
  pub fn validate(&self, issue: &IssueRevision) -> Result<(), StoreError> {
    if issue.project_id != self.intent.project_id
      || issue.id != self.intent.issue_id
      || issue.version != self.intent.expected_version
      || self.observation.selection != self.intent.selection
      || self.observation.observed_at > self.attached_at
    {
      return Err(StoreError::Conflict {
        entity: octacity_server_domain::EntityKind::Issue,
      });
    }
    self.observation.validate()
  }
}
impl ObserveIssueCodeReview {
  /// Shared exact native-target/time checks; refresh never replaces accepted lineage.
  pub fn validate(&self, association: &IssueCodeReviewAssociation) -> Result<(), StoreError> {
    if association.project_id != self.intent.project_id
      || association.issue_id != self.intent.issue_id
      || association.id != self.intent.association_id
      || self.observation.selection != association.selection
      || self.observation.observed_at > self.recorded_at
    {
      return Err(StoreError::Conflict {
        entity: octacity_server_domain::EntityKind::Issue,
      });
    }
    self.observation.validate()
  }
}

impl ReadIssueCodeReviews {
  /// Shared bounds checked before Issue/native metadata reads by either backend.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.limit == 0 || self.limit > MAX_ISSUE_HISTORY_PAGE_SIZE {
      return Err(invalid_issue_code_review());
    }
    Ok(())
  }
  /// Rejects positions and membership claims beyond this currently visible retained stream.
  pub fn retained_boundary(&self, maximum: u64) -> Result<u64, StoreError> {
    self.validate()?;
    let boundary = self.boundary.unwrap_or(maximum);
    if boundary > maximum || self.after.is_some_and(|after| after > boundary) {
      return Err(invalid_issue_code_review());
    }
    Ok(boundary)
  }
}
