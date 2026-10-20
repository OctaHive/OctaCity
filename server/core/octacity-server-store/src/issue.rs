//! Persistent task intent; no execution or provider dependency is required.
use crate::{IdempotencyKey, ManagementMutation, MutationDisposition, StoreError};
use octacity_server_domain::{
  IssueId, IssueStatusId, IssueTransitionId, IssueVersion, IssueWorkflowId, IssueWorkflowVersion, ProjectId, Timestamp,
};
use serde::{Deserialize, Serialize};

/// Maximum status-history records in one request.
pub const MAX_ISSUE_HISTORY_PAGE_SIZE: u16 = 200;
/// Maximum UTF-8 bytes in a task title.
pub const MAX_ISSUE_TITLE_BYTES: usize = octacity_server_domain::MAX_RESOURCE_NAME_BYTES;
/// Maximum UTF-8 bytes in each requirements document.
pub const MAX_ISSUE_DOCUMENT_BYTES: usize = 65_536;
/// Editable requirements only; status, workflow and execution setters are excluded.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueContent {
  /// Nonblank canonical single-line title.
  pub title: String,
  /// Plain-text description; empty drafts are valid.
  pub description: String,
  /// Optional incomplete acceptance text, validated again at execution review.
  pub acceptance: Option<String>,
  /// Optional bounded planning priority, independent of admitted scheduling.
  pub priority: Option<u8>,
}
impl std::fmt::Debug for IssueContent {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("IssueContent").finish_non_exhaustive()
  }
}
impl IssueContent {
  /// Validates stored text bounds; never interprets prose or runs an action.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.title.is_empty()
      || self.title.trim() != self.title
      || self.title.len() > MAX_ISSUE_TITLE_BYTES
      || self.title.chars().any(char::is_control)
      || self.description.len() > MAX_ISSUE_DOCUMENT_BYTES
      || self
        .acceptance
        .as_ref()
        .is_some_and(|s| s.len() > MAX_ISSUE_DOCUMENT_BYTES)
      || self.description.contains('\0')
      || self.acceptance.as_ref().is_some_and(|s| s.contains('\0'))
    {
      return Err(invalid_issue(crate::StoreOperation::CreateIssue));
    }
    Ok(())
  }
}
/// One immutable task revision; business status is independent of execution state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueRevision {
  /// Stable internal identity.
  pub id: IssueId,
  /// Owning Project, never changed by an ordinary edit.
  pub project_id: ProjectId,
  /// Positive Project-local sequence allocated atomically with creation.
  pub number: u64,
  /// Exact current revision for future preconditions.
  pub version: IssueVersion,
  /// Last requirements/status/workflow invalidation; unrelated planning edits preserve it.
  pub applicability_version: IssueVersion,
  /// Editable requirements snapshot.
  pub content: IssueContent,
  /// Optional organizational snapshot; absent in prior retained revisions.
  #[serde(default, skip_serializing_if = "crate::IssuePlanning::is_empty")]
  pub planning: crate::IssuePlanning,
  /// Exact retained business workflow identity.
  pub workflow_id: IssueWorkflowId,
  /// Pinned workflow publication, unaffected by default changes.
  pub workflow_version: IssueWorkflowVersion,
  /// Current organizational status; no execution acceptance claim.
  pub status_id: IssueStatusId,
  /// Original creation time.
  pub created_at: Timestamp,
  /// Time of this revision.
  pub updated_at: Timestamp,
}
impl IssueRevision {
  /// Human-readable Project-local key; not an authorization identity.
  pub fn display_key(&self) -> String {
    format!("ISS-{}", self.number)
  }
}
/// Frozen creator intent; identity and default are resolved only once on acceptance.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateIssueIntent {
  /// Optional local planning; ordinary tasks require no catalog or directory.
  #[serde(default, skip_serializing_if = "crate::IssuePlanning::is_empty")]
  pub planning: crate::IssuePlanning,
  /// Visible owning Project.
  pub project_id: ProjectId,
  /// Draft requirements requiring no acceptance document or provider.
  pub content: IssueContent,
}
/// Authorized store input; generated identity is excluded from replay equivalence.
#[derive(Clone, Debug)]
pub struct CreateIssue {
  /// Independently server-resolved selectable identities, excluded from replay intent.
  pub assignees: std::collections::BTreeSet<String>,
  /// Proposed internal identity, used only for the first application.
  pub id: IssueId,
  /// Frozen user intent.
  pub intent: CreateIssueIntent,
  /// Caller-selected scoped replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative time for the first application.
  pub created_at: Timestamp,
}
/// Original revision receipt returned unchanged by exact replay.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IssueMutationOutcome {
  /// Applied versus replayed, without repeating any side effect.
  pub disposition: MutationDisposition,
  /// Original resulting revision, even after later edits or moves.
  pub issue: IssueRevision,
}
/// Transactional task authority independent of Factory and native systems.
#[async_trait::async_trait]
pub trait IssueStore: Send + Sync {
  /// Reads one original pipeline command from the existing scoped replay record; reserves nothing.
  async fn issue_status_command(
    &self,
    project_id: ProjectId,
    id: IssueId,
    scope: &crate::ManagementSecurityScope,
    operation_id: octacity_server_factory::FactoryDigest,
  ) -> Result<Option<crate::RetainedIssueStatusCommand>, StoreError>;
  /// Reads retained ownership/mapping preconditions; grants no authority to change them.
  async fn issue_field_policy(&self, project_id: ProjectId, id: IssueId) -> Result<IssueFieldPolicy, StoreError>;
  /// Reads bounded revision headers; bodies remain dedicated exact reads.
  async fn issue_revisions(
    &self,
    project_id: ProjectId,
    id: IssueId,
    after: Option<IssueVersion>,
    limit: u16,
  ) -> Result<IssueRevisionPage, StoreError>;
  /// Filters visible current tasks before ordering and pagination.
  async fn list_issues(&self, request: ListProjectIssues) -> Result<IssuePage, StoreError>;
  /// Creates a draft with an atomically selected default and unique Project-local key.
  async fn create_issue(&self, request: ManagementMutation<CreateIssue>) -> Result<IssueMutationOutcome, StoreError>;
  /// Replaces editable requirements under the exact observed revision.
  async fn edit_issue(&self, request: ManagementMutation<EditIssue>) -> Result<IssueMutationOutcome, StoreError>;
  /// Applies one explicit direct business move, atomically with its history and receipt.
  async fn transition_issue(
    &self,
    request: ManagementMutation<TransitionIssue>,
  ) -> Result<IssueTransitionOutcome, StoreError>;
  /// Explicitly adopts a reviewed workflow/status correspondence without execution or provider effects.
  async fn adopt_issue_workflow(
    &self,
    request: ManagementMutation<AdoptIssueWorkflow>,
  ) -> Result<IssueTransitionOutcome, StoreError>;
  /// Returns bounded organizational history without consulting execution or a provider.
  async fn issue_status_history(
    &self,
    project_id: ProjectId,
    id: IssueId,
    after: Option<IssueVersion>,
    limit: u16,
  ) -> Result<IssueStatusHistoryPage, StoreError>;
  /// Reads current or exact retained revision, confined to its owning Project.
  async fn issue(
    &self,
    project_id: ProjectId,
    id: IssueId,
    version: Option<IssueVersion>,
  ) -> Result<IssueRevision, StoreError>;
}
pub(crate) fn invalid_issue(operation: crate::StoreOperation) -> StoreError {
  StoreError::invalid(operation, crate::StoreInputError::InvalidIssue)
}

/// Explicit editable-field replacement; cannot assign status/workflow or execution state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EditIssueIntent {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Existing task identity.
  pub id: IssueId,
  /// Exact reviewed current revision.
  pub expected_version: IssueVersion,
  /// Complete editable text snapshot.
  pub content: IssueContent,
}
/// Original preconditioned edit with trusted time and replay scope.
#[derive(Clone, Debug)]
pub struct EditIssue {
  /// Reviewed immutable edit intent.
  pub intent: EditIssueIntent,
  /// Scoped caller replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Server time retained only on first application.
  pub edited_at: Timestamp,
}

/// Exact reviewed business move; separate from editable text and Run controls.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionIssueIntent {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Stable task identity.
  pub id: IssueId,
  /// Exact current task revision.
  pub expected_version: IssueVersion,
  /// Exact reviewed workflow.
  pub workflow_id: IssueWorkflowId,
  /// Retained workflow publication, never implicitly latest.
  pub workflow_version: IssueWorkflowVersion,
  /// Exact reviewed starting status.
  pub expected_status_id: IssueStatusId,
  /// One declared direct edge, never an inferred multi-edge path.
  pub transition_id: IssueTransitionId,
  /// Optional bounded organizational explanation, absent from generic audit/outbox.
  pub reason: Option<String>,
}
/// Shared trusted business-command envelope for manual and accepted pipeline moves.
#[derive(Clone, Debug)]
pub struct TransitionIssue {
  pub(crate) source: Option<crate::IssueSourceStatusProvenance>,
  pub(crate) pipeline: Option<crate::AcceptedIssueStatusAction>,
  /// Frozen edge and preconditions.
  pub intent: TransitionIssueIntent,
  /// Original scoped replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Server mutation time.
  pub moved_at: Timestamp,
}
/// Append-only organizational event, independent of execution history.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
pub struct IssueStatusHistoryEntry {
  /// Exact mapped observation; absent for manual/pipeline moves and legacy history.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub source: Option<crate::IssueSourceStatusProvenance>,
  /// Stable Issue identity.
  pub issue_id: IssueId,
  /// Revision committed with this event.
  pub version: IssueVersion,
  /// Exact business workflow.
  pub workflow_id: IssueWorkflowId,
  /// Exact retained workflow policy.
  pub workflow_version: IssueWorkflowVersion,
  /// Absent only for initial placement at creation.
  pub from: Option<IssueStatusId>,
  /// Resulting organizational status.
  pub to: IssueStatusId,
  /// Absent for initial placement or explicit workflow adoption.
  pub transition_id: Option<IssueTransitionId>,
  /// Original workflow when this record is an explicit adoption.
  pub previous_workflow: Option<IssueWorkflowPin>,
  /// Trusted command producer kind, never derived from text.
  pub origin: crate::IssueTransitionOrigin,
  /// Exact accepted action metadata, absent for manual placement/moves.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub pipeline: Option<crate::IssuePipelineActionProvenance>,
  /// Optional organizational explanation; no raw execution/provider output.
  pub reason: Option<String>,
  /// Actual accepted management actor, never a fabricated human.
  pub actor: crate::AuditActor,
  /// Original commit time.
  pub occurred_at: Timestamp,
}
/// Exact original move receipt retained after later edits and moves.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IssueTransitionOutcome {
  /// Whether this intent was applied or replayed.
  pub disposition: MutationDisposition,
  /// Original resulting task revision.
  pub issue: IssueRevision,
  /// Original organizational history record.
  pub transition: IssueStatusHistoryEntry,
}
/// Bounded status-history page ordered by immutable Issue revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssueStatusHistoryPage {
  /// Visible records in revision order.
  pub items: Vec<IssueStatusHistoryEntry>,
  /// Continue strictly after this revision, absent at the current end.
  pub next_cursor: Option<IssueVersion>,
}

/// Closed fields participating in local edit/transition ownership, never provider wire names.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueField {
  /// Organizational assignee.
  Assignee,
  /// Project labels.
  Labels,
  /// Date-only planning target.
  DueDate,
  /// Configured simple planning values.
  Properties,
  /// Organizational parent reference.
  Parent,
  /// Task title.
  Title,
  /// Task requirements description.
  Description,
  /// Optional acceptance text.
  Acceptance,
  /// Planning priority.
  Priority,
  /// Organizational status, writable only through the transition command.
  Status,
}
/// Retained trusted association policy; ordinary edit commands cannot replace ownership.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueFieldPolicy {
  /// Fields managed only by an authorized source observation.
  pub source_owned: std::collections::BTreeSet<IssueField>,
  /// Exact compatible business workflow for a managed status mapping, when present.
  pub status_mapping_workflow: Option<IssueWorkflowPin>,
  /// Explicit approved immutable push mapping; absent for ordinary manual tasks.
  pub push: Option<IssuePushPolicy>,
}
/// Exact authorized mapping reference and selected local push fields.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssuePushPolicy {
  /// Retained content-addressed mapping, including its source target; never a mutable latest lookup.
  pub mapping: octacity_server_factory::ImmutableReference,
  /// Locally owned fields approved for projection by that mapping.
  pub fields: std::collections::BTreeSet<IssueField>,
}
impl IssueContent {
  pub(crate) fn changed_fields(&self, previous: &Self) -> std::collections::BTreeSet<IssueField> {
    let mut fields = std::collections::BTreeSet::new();
    if self.title != previous.title {
      fields.insert(IssueField::Title);
    }
    if self.description != previous.description {
      fields.insert(IssueField::Description);
    }
    if self.acceptance != previous.acceptance {
      fields.insert(IssueField::Acceptance);
    }
    if self.priority != previous.priority {
      fields.insert(IssueField::Priority);
    }
    fields
  }
}

/// Retained projection intent, referencing original text/history through the exact Issue revision.
/// No provider request or credentials are embedded; delivery belongs to existing Reporter recovery.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IssuePushIntent {
  /// Owning Project under which the mapping was authorized.
  pub project_id: ProjectId,
  /// Stable task identity.
  pub issue_id: IssueId,
  /// Original committed revision, never replaced with a later head.
  pub version: IssueVersion,
  /// Exact approved mapping/target pin, unaffected by later policy replacement.
  pub mapping: octacity_server_factory::ImmutableReference,
  /// Only changed locally owned fields approved by the retained mapping.
  pub fields: std::collections::BTreeSet<IssueField>,
}

impl std::fmt::Debug for TransitionIssueIntent {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("TransitionIssueIntent")
      .field("id", &self.id)
      .field("expected_version", &self.expected_version)
      .finish_non_exhaustive()
  }
}
impl std::fmt::Debug for IssueStatusHistoryEntry {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("IssueStatusHistoryEntry")
      .field("issue_id", &self.issue_id)
      .field("version", &self.version)
      .finish_non_exhaustive()
  }
}

/// Exact retained business-policy reference, independent of provider vocabulary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueWorkflowPin {
  /// Stable workflow identity.
  pub id: IssueWorkflowId,
  /// Immutable publication version.
  pub version: IssueWorkflowVersion,
}
/// Reviewed migration of one current Issue; ordinary edits cannot submit this intent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdoptIssueWorkflowIntent {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Current Issue identity.
  pub id: IssueId,
  /// Exact reviewed revision.
  pub expected_version: IssueVersion,
  /// Exact original workflow identity.
  pub expected_workflow_id: IssueWorkflowId,
  /// Exact original workflow version.
  pub expected_workflow_version: IssueWorkflowVersion,
  /// Exact original status.
  pub expected_status_id: IssueStatusId,
  /// Explicit target workflow in the same Project.
  pub target_workflow_id: IssueWorkflowId,
  /// Reviewed immutable target version; never latest.
  pub target_workflow_version: IssueWorkflowVersion,
  /// Bounded explicit old-to-new status correspondence, including the current status.
  pub status_correspondence: std::collections::BTreeMap<IssueStatusId, IssueStatusId>,
  /// Reviewed trusted ownership/mapping state; compared, never installed by this command.
  pub expected_field_policy: IssueFieldPolicy,
}
/// Trusted workflow-adoption envelope with original replay and mutation time.
#[derive(Clone, Debug)]
pub struct AdoptIssueWorkflow {
  /// Frozen policy/status review.
  pub intent: AdoptIssueWorkflowIntent,
  /// Scoped replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Server mutation time.
  pub adopted_at: Timestamp,
}

/// Bounded current task page; no requirements bodies or execution aggregates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssuePage {
  /// Visible task metadata in ascending identity order.
  pub items: Vec<IssueSummary>,
  /// Last visible identity when another visible record exists.
  pub next_cursor: Option<IssueId>,
}
/// Safe bounded list metadata. Dedicated revision reads own requirement documents.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
pub struct IssueSummary {
  /// Stable task identity.
  pub id: IssueId,
  /// Exact owning Project.
  pub project_id: ProjectId,
  /// Human-readable Project-local key.
  pub display_key: String,
  /// Current revision.
  pub version: IssueVersion,
  /// Canonical title.
  pub title: String,
  /// Pinned policy.
  pub workflow_id: IssueWorkflowId,
  /// Exact policy publication.
  pub workflow_version: IssueWorkflowVersion,
  /// Business status only.
  pub status_id: IssueStatusId,
  /// Original creation time.
  #[serde(rename = "created_at_unix_ms")]
  pub created_at: Timestamp,
  /// Current revision time.
  #[serde(rename = "updated_at_unix_ms")]
  pub updated_at: Timestamp,
}
impl std::fmt::Debug for IssueSummary {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("IssueSummary")
      .field("id", &self.id)
      .field("version", &self.version)
      .finish_non_exhaustive()
  }
}
impl From<&IssueRevision> for IssueSummary {
  fn from(issue: &IssueRevision) -> Self {
    Self {
      id: issue.id,
      project_id: issue.project_id,
      display_key: issue.display_key(),
      version: issue.version,
      title: issue.content.title.clone(),
      workflow_id: issue.workflow_id,
      workflow_version: issue.workflow_version,
      status_id: issue.status_id,
      created_at: issue.created_at,
      updated_at: issue.updated_at,
    }
  }
}
/// Validated Project list request with visibility applied before pagination.
#[derive(Clone, Debug)]
pub struct ListProjectIssues {
  project_id: ProjectId,
  after: Option<IssueId>,
  limit: u16,
  visibility: crate::IssueListVisibility,
}
impl ListProjectIssues {
  /// Validates bounds and refuses a cursor outside the current security scope.
  pub fn new(
    project_id: ProjectId,
    after: Option<IssueId>,
    limit: u16,
    visibility: crate::IssueListVisibility,
  ) -> Result<Self, StoreError> {
    if limit == 0 || limit > MAX_ISSUE_HISTORY_PAGE_SIZE || after.is_some_and(|id| !visibility.allows(&id)) {
      return Err(invalid_issue(crate::StoreOperation::ListProjectIssues));
    }
    Ok(Self {
      project_id,
      after,
      limit,
      visibility,
    })
  }
  /// Exact Project scope.
  pub fn project_id(&self) -> ProjectId {
    self.project_id
  }
  /// Exclusive visible identity continuation.
  pub fn after(&self) -> Option<IssueId> {
    self.after
  }
  /// Maximum returned records.
  pub fn limit(&self) -> u16 {
    self.limit
  }
  /// Current authorized identities.
  pub fn visibility(&self) -> &crate::IssueListVisibility {
    &self.visibility
  }
}

/// Bounded immutable revision headers, excluding requirements documents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssueRevisionPage {
  /// Retained headers in increasing revision order.
  pub items: Vec<IssueRevisionHeader>,
  /// Last visible revision when another exists.
  pub next_cursor: Option<IssueVersion>,
}
/// Minimal discovery metadata for one dedicated requirements read.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct IssueRevisionHeader {
  /// Exact revision identity.
  pub version: IssueVersion,
  /// Requirements/status applicability marker.
  pub applicability_version: IssueVersion,
  /// Original revision time.
  pub updated_at_unix_ms: i64,
}
impl From<&IssueRevision> for IssueRevisionHeader {
  fn from(issue: &IssueRevision) -> Self {
    Self {
      version: issue.version,
      applicability_version: issue.applicability_version,
      updated_at_unix_ms: issue.updated_at.unix_millis(),
    }
  }
}

/// Safe reason why a displayed manual edge cannot currently be offered.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueMoveBlockReason {
  /// Independently authorized transition right is missing.
  PermissionRequired,
  /// Status is managed by its source association.
  SourceOwned,
  /// The declared edge excludes manual origin.
  OriginNotAllowed,
  /// Organizational input cannot assert trusted execution evidence.
  EvidenceRequired,
}
