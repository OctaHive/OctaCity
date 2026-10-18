//! Project business vocabulary, with no execution, provider or scripting semantics.
use octacity_server_domain::{
  IssueStatusId, IssueTransitionId, IssueWorkflowId, IssueWorkflowSelectionVersion, IssueWorkflowVersion, ProjectId,
  Timestamp,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Maximum statuses in one immutable workflow, independently of display names.
pub const MAX_ISSUE_WORKFLOW_STATUSES: usize = 64;
/// Maximum explicit direct edges in one immutable workflow.
pub const MAX_ISSUE_WORKFLOW_TRANSITIONS: usize = 256;

/// Named organizational status; vector position supplies display order.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueWorkflowStatus {
  /// Stable identity, unaffected by renaming/reordering.
  pub id: IssueStatusId,
  /// Bounded human label with no built-in status semantics.
  pub name: String,
}

/// The only supported business-command origins; names do not select execution.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueTransitionOrigin {
  /// Explicit authorized operator command.
  Manual,
  /// Accepted configured pipeline action.
  Pipeline,
  /// Authorized source-owned mapped observation.
  Source,
}

/// Closed requirements interpreted by the shared transition boundary, never scripts.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueTransitionEvidence {
  /// An authoritative accepted Decision for the exact execution subject.
  AcceptedExecution,
  /// All required checks passed for the exact current execution subject.
  PassedRequiredChecks,
}

/// One direct organizational edge. Rights and exact evidence are checked at movement time.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueWorkflowTransition {
  /// Stable edge identity, independent of its display name.
  pub id: IssueTransitionId,
  /// Human-readable move label.
  pub name: String,
  /// Exact source status in this definition.
  pub from: IssueStatusId,
  /// Exact destination status, with no inferred intermediate moves.
  pub to: IssueStatusId,
  /// Nonempty subset of separately authorized origins.
  pub origins: Vec<IssueTransitionOrigin>,
  /// Finite exact-subject requirements; empty means no execution prerequisite.
  pub evidence: Vec<IssueTransitionEvidence>,
}

/// Immutable business-policy document, independent of Factory or integrations.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueWorkflowDefinition {
  /// Human-readable workflow label.
  pub name: String,
  /// Explicit display order; names are not stable identities.
  pub statuses: Vec<IssueWorkflowStatus>,
  /// Required initial status, independent of display order.
  pub initial_status_id: IssueStatusId,
  /// Organizational completion choices; empty is valid and none imply execution acceptance.
  pub terminal_status_ids: Vec<IssueStatusId>,
  /// Explicit allowed direct moves, not a Flow program.
  pub transitions: Vec<IssueWorkflowTransition>,
}
impl IssueWorkflowDefinition {
  /// Checks finite business policy only; never evaluates evidence or runs a script.
  pub fn validate(&self) -> Result<(), crate::StoreError> {
    if !valid_name(&self.name)
      || self.statuses.is_empty()
      || self.statuses.len() > MAX_ISSUE_WORKFLOW_STATUSES
      || self.terminal_status_ids.len() > MAX_ISSUE_WORKFLOW_STATUSES
      || self.transitions.len() > MAX_ISSUE_WORKFLOW_TRANSITIONS
    {
      return Err(invalid_workflow());
    }
    let mut statuses = BTreeSet::new();
    for status in &self.statuses {
      if !valid_name(&status.name) || !statuses.insert(status.id) {
        return Err(invalid_workflow());
      }
    }
    if !statuses.contains(&self.initial_status_id) {
      return Err(invalid_workflow());
    }
    let mut terminal_statuses = BTreeSet::new();
    if self
      .terminal_status_ids
      .iter()
      .any(|id| !statuses.contains(id) || !terminal_statuses.insert(*id))
    {
      return Err(invalid_workflow());
    }
    let mut ids = BTreeSet::new();
    let mut edges = BTreeSet::new();
    for edge in &self.transitions {
      if !valid_name(&edge.name)
        || !ids.insert(edge.id)
        || edge.from == edge.to
        || !statuses.contains(&edge.from)
        || !statuses.contains(&edge.to)
        || edge.origins.is_empty()
        || edge.origins.len() > 3
        || edge.evidence.len() > 2
        || edge.evidence.iter().collect::<BTreeSet<_>>().len() != edge.evidence.len()
      {
        return Err(invalid_workflow());
      }
      for origin in &edge.origins {
        if !edges.insert((edge.from, edge.to, *origin)) {
          return Err(invalid_workflow());
        }
      }
    }
    Ok(())
  }
}
fn valid_name(value: &str) -> bool {
  !value.is_empty()
    && value.len() <= octacity_server_domain::MAX_RESOURCE_NAME_BYTES
    && value.trim() == value
    && !value.chars().any(char::is_control)
}
pub(crate) fn invalid_workflow() -> crate::StoreError {
  crate::StoreError::invalid(
    crate::StoreOperation::PublishIssueWorkflow,
    crate::StoreInputError::InvalidIssueWorkflow,
  )
}

/// One retained immutable Project workflow version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedIssueWorkflow {
  /// Owning Project; no provider or execution prerequisites.
  pub project_id: ProjectId,
  /// Stable workflow identity within its Project.
  pub id: IssueWorkflowId,
  /// Immutable definition version.
  pub version: IssueWorkflowVersion,
  /// Original accepted business definition.
  pub definition: IssueWorkflowDefinition,
  /// Original publication time, retained by replay.
  pub published_at: Timestamp,
}

/// Selected default for new Issues only; existing Issues keep their own exact pin.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueWorkflowDefault {
  /// Optimistic selection version, distinct from the definition version.
  pub version: IssueWorkflowSelectionVersion,
  /// Exact selected definition, available even before first administrative publication.
  pub workflow: PublishedIssueWorkflow,
}

/// Minimal workflow authority; does not admit Work or call a provider.
#[async_trait::async_trait]
pub trait IssueWorkflowStore: Send + Sync {
  /// Lists visible current policies, including the implicit initial template, before pagination.
  async fn list_issue_workflows(
    &self,
    request: crate::ListProjectIssueWorkflows,
  ) -> Result<IssueWorkflowPage, crate::StoreError>;
  /// Returns the selected immutable version, or the declarative initial default, without writes.
  async fn default_issue_workflow(&self, project_id: ProjectId) -> Result<IssueWorkflowDefault, crate::StoreError>;
  /// Reads a retained version; absence selects current publication, not Project default.
  async fn issue_workflow(
    &self,
    project_id: ProjectId,
    id: IssueWorkflowId,
    version: Option<IssueWorkflowVersion>,
  ) -> Result<PublishedIssueWorkflow, crate::StoreError>;
  /// Atomically publishes policy, scoped replay and safe audit without changing default.
  async fn publish_issue_workflow(
    &self,
    request: crate::ManagementMutation<PublishIssueWorkflow>,
  ) -> Result<IssueWorkflowPublicationOutcome, crate::StoreError>;
  /// Selects an exact same-Project default for new Issues without migrating any existing pin.
  async fn select_default_issue_workflow(
    &self,
    request: crate::ManagementMutation<SelectDefaultIssueWorkflow>,
  ) -> Result<IssueWorkflowSelectionOutcome, crate::StoreError>;
}

/// Reviewed exact default selection; independent of the current workflow publication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SelectDefaultIssueWorkflowIntent {
  /// Owning Project of both selection and selected workflow.
  pub project_id: ProjectId,
  /// Exact current selection revision.
  pub expected_version: IssueWorkflowSelectionVersion,
  /// Same-Project workflow identity.
  pub workflow_id: IssueWorkflowId,
  /// Exact immutable definition, never implicitly latest.
  pub workflow_version: IssueWorkflowVersion,
}
/// Atomic authorized default selection.
#[derive(Clone, Debug)]
pub struct SelectDefaultIssueWorkflow {
  /// Original reviewed intent.
  pub intent: SelectDefaultIssueWorkflowIntent,
  /// Scoped replay identity.
  pub idempotency_key: crate::IdempotencyKey,
}
/// Original default-selection receipt; not a migration or execution request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IssueWorkflowSelectionOutcome {
  /// Applied or exact historical replay.
  pub disposition: crate::MutationDisposition,
  /// Original selected version and definition.
  pub default: IssueWorkflowDefault,
}

/// Exact caller intent, excluding publication time and server metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishIssueWorkflowIntent {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Stable workflow identity.
  pub id: IssueWorkflowId,
  /// None creates; Some requires exactly this current version.
  pub expected_current_version: Option<IssueWorkflowVersion>,
  /// Immutable business policy.
  pub definition: IssueWorkflowDefinition,
}
/// Authorized atomic publication, with no execution or provider effects.
#[derive(Clone, Debug)]
pub struct PublishIssueWorkflow {
  /// Original caller intent.
  pub intent: PublishIssueWorkflowIntent,
  /// Scoped replay key.
  pub idempotency_key: crate::IdempotencyKey,
  /// Original accepted time, excluded from intent comparison.
  pub published_at: Timestamp,
}
/// Original immutable receipt, retained after later versions/default changes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IssueWorkflowPublicationOutcome {
  /// Newly applied or original exact replay.
  pub disposition: crate::MutationDisposition,
  /// Original exact published version.
  pub workflow: PublishedIssueWorkflow,
}

/// Resolves the editable template without a write or first-creator administration.
/// Its typed identity is scoped by Project and stable before any policy publication.
/// The v1 template is retained data: future defaults must not overwrite its meaning.
pub fn initial_issue_workflow(
  project_id: ProjectId,
  created_at: Timestamp,
) -> Result<PublishedIssueWorkflow, crate::StoreError> {
  let definition: IssueWorkflowDefinition =
    serde_json::from_str(include_str!("issue_workflow_default.v1.json")).map_err(|_| crate::StoreError::Unavailable)?;
  definition.validate()?;
  Ok(PublishedIssueWorkflow {
    project_id,
    id: IssueWorkflowId::from_uuid(project_id.as_uuid()).expect("non-nil Project identity"),
    version: IssueWorkflowVersion::INITIAL,
    definition,
    published_at: created_at,
  })
}

/// Safe current policy facts without a full definition on every discovery row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssueWorkflowSummary {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Stable policy identity.
  pub id: IssueWorkflowId,
  /// Current publication version, independent of selected default.
  pub version: IssueWorkflowVersion,
  /// Bounded human label.
  pub name: String,
  /// Original publication time.
  pub published_at: Timestamp,
}
impl From<PublishedIssueWorkflow> for IssueWorkflowSummary {
  fn from(value: PublishedIssueWorkflow) -> Self {
    Self {
      project_id: value.project_id,
      id: value.id,
      version: value.version,
      name: value.definition.name,
      published_at: value.published_at,
    }
  }
}
/// One bounded visible page of current policies in stable identity order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssueWorkflowPage {
  /// Current visible summaries.
  pub items: Vec<IssueWorkflowSummary>,
  /// Exclusive last visible identity when more results exist.
  pub next_cursor: Option<IssueWorkflowId>,
}
