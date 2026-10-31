//! Exact reviewed task execution intent, independent of input publication and Run control.
use crate::{IdempotencyKey, ManagementMutation, MutationDisposition, RepositorySourceSelection, StoreError};
use octacity_server_domain::{IssueId, IssueVersion, ProjectId, RepositoryId, RepositoryVersion, Timestamp};
use octacity_server_factory::{
  FactoryConfigurationId, FactoryConfigurationVersion, FactoryDigest, FactoryRunId, WorkEnvelopeId,
};
use serde::{Deserialize, Serialize};

/// Installed finite text mapping; no caller scripting or implicit planning selection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueExecutionMapping {
  /// Version one selects title, description and required acceptance text.
  TitleDescriptionAcceptanceV1,
}
/// Exact immutable target and reviewed base policy for later ordinary admission.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueExecutionTarget {
  /// Factory target identity.
  pub configuration_id: FactoryConfigurationId,
  /// Exact immutable target publication.
  pub configuration_version: FactoryConfigurationVersion,
  /// Repository in the same Project.
  pub repository_id: RepositoryId,
  /// Exact immutable repository policy.
  pub repository_version: RepositoryVersion,
  /// Reviewed permitted source expression; resolution belongs to ordinary admission.
  pub source: RepositorySourceSelection,
  /// Explicit finite versioned requirements selection.
  pub mapping: IssueExecutionMapping,
}
/// Caller review; contains neither bodies nor execution-state setters.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RequestIssueExecutionIntent {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Reviewed task identity.
  pub issue_id: IssueId,
  /// Exact reviewed current task revision.
  pub expected_version: IssueVersion,
  /// Frozen target, source policy and text selection.
  pub target: IssueExecutionTarget,
  /// Reviewed ordinary admission classification; absent only in legacy reservations.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub classification: Option<octacity_server_factory::WorkClassification>,
}
/// Trusted request reservation with scoped replay and original time.
#[derive(Clone, Debug)]
pub struct RequestIssueExecution {
  /// Generated logical identity, excluded from replay equivalence.
  pub id: octacity_server_domain::IssueExecutionRequestId,
  /// Original reviewed intent.
  pub intent: RequestIssueExecutionIntent,
  /// Caller replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Trusted first-request time.
  pub requested_at: Timestamp,
}
/// Immutable association to existing execution authority; never a second Run state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueWorkAssociation {
  /// Owning Project of both Issue and Work.
  pub project_id: ProjectId,
  /// Stable owning Issue.
  pub issue_id: IssueId,
  /// Explicit reviewed request, absent for retained Artifact-only lineage.
  pub request_id: Option<octacity_server_domain::IssueExecutionRequestId>,
  /// Exact reviewed revision; legacy Artifact lineage never invents one.
  pub requirements_version: Option<IssueVersion>,
  /// Original immutable Work.
  pub work_id: WorkEnvelopeId,
  /// The Work's original Factory Run, including all its retries.
  pub run_id: FactoryRunId,
}
/// Retained execution intent; a missing association means preparation is pending.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueExecutionRequest {
  /// Stable server-generated request identity.
  pub id: octacity_server_domain::IssueExecutionRequestId,
  /// Exact Issue revision and reviewed target; never implicitly latest.
  pub intent: RequestIssueExecutionIntent,
  /// Exact mapped task document digest, shared with prepared input publication.
  pub task_digest: FactoryDigest,
  /// Exact mapped task UTF-8 size.
  pub task_size: u64,
  /// Exact selected acceptance UTF-8 size.
  pub acceptance_size: u64,
  /// Original selected title bytes.
  pub title_digest: FactoryDigest,
  /// Original selected description bytes.
  pub description_digest: FactoryDigest,
  /// Original required acceptance bytes.
  pub acceptance_digest: FactoryDigest,
  /// Trusted original request time.
  pub requested_at: Timestamp,
  /// Durable link, present only after association commits.
  pub association: Option<IssueWorkAssociation>,
}
/// Original reservation receipt; current recovery uses the dedicated request read.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IssueExecutionOutcome {
  /// Applied or exact replay.
  pub disposition: MutationDisposition,
  /// Original reservation snapshot.
  pub request: IssueExecutionRequest,
}
/// Narrow transactional reservation/read port; no provider, publisher or executor.
#[async_trait::async_trait]
pub trait IssueExecutionStore: Send + Sync {
  /// Resolves an original request only in its retained admission security partition.
  async fn issue_execution_for_admission(
    &self,
    project_id: ProjectId,
    issue_id: IssueId,
    request_id: octacity_server_domain::IssueExecutionRequestId,
    scope: &crate::ManagementSecurityScope,
  ) -> Result<IssueExecutionRequest, StoreError>;
  /// Links existing Work once; atomically marks an exact request associated when present.
  async fn associate_issue_work(
    &self,
    request: ManagementMutation<AssociateIssueWork>,
  ) -> Result<IssueWorkAssociationOutcome, StoreError>;
  /// Same-Project reverse lookup, with no association/backfill on reads.
  async fn issue_work_association(
    &self,
    project_id: ProjectId,
    work_id: WorkEnvelopeId,
  ) -> Result<Option<IssueWorkAssociation>, StoreError>;
  /// Scoped original replay, checked before current text or mutable target lookup.
  async fn replay_issue_execution(
    &self,
    request: &ManagementMutation<RequestIssueExecution>,
  ) -> Result<Option<IssueExecutionOutcome>, StoreError>;
  /// Atomically checks the current reviewed revision and retains one frozen request.
  async fn request_issue_execution(
    &self,
    request: ManagementMutation<RequestIssueExecution>,
  ) -> Result<IssueExecutionOutcome, StoreError>;
  /// Enumerates retained membership under an Issue-local observation, without publication.
  async fn issue_executions(
    &self,
    project_id: ProjectId,
    issue_id: IssueId,
    after: Option<IssueExecutionPosition>,
    observation: Option<IssueExecutionObservation>,
    limit: u16,
  ) -> Result<IssueExecutionReadPage, StoreError>;
  /// Reads one retained request without reserving or resuming execution.
  async fn issue_execution(
    &self,
    project_id: ProjectId,
    issue_id: IssueId,
    request_id: octacity_server_domain::IssueExecutionRequestId,
  ) -> Result<IssueExecutionRequest, StoreError>;
}

impl IssueExecutionMapping {
  /// Deterministic version-one task bytes; publication must reuse this mapping.
  pub fn task_bytes(&self, content: &crate::IssueContent) -> Vec<u8> {
    match self {
      Self::TitleDescriptionAcceptanceV1 => format!("{}\n\n{}", content.title, content.description).into_bytes(),
    }
  }
}
impl IssueExecutionRequest {
  /// Stable ordinary manual Work identity, distinct for each intentional request.
  pub fn external_identity(&self) -> octacity_server_factory::ExternalWorkIdentity {
    octacity_server_factory::ExternalWorkIdentity::new(format!("issue-execution/{}", self.id))
      .expect("bounded generated request identity")
  }
  /// Stable admission replay identity independent of the caller's transport key.
  pub fn admission_key(&self) -> IdempotencyKey {
    IdempotencyKey::new(format!("issue-execution.{}", self.id)).expect("bounded generated request identity")
  }
}
/// Trusted association of an already published admission; no Work is created here.
#[derive(Clone, Debug)]
pub struct AssociateIssueWork {
  /// Owning Project; all retained subjects must agree.
  pub project_id: ProjectId,
  /// Existing Issue; no implicit task creation.
  pub issue_id: IssueId,
  /// Exact request, or absent for legacy Artifact provenance.
  pub request_id: Option<octacity_server_domain::IssueExecutionRequestId>,
  /// Original authoritative ordinary admission receipt, never client-supplied IDs.
  pub admission: crate::PublishedFactoryAdmission,
}
/// Original Work-to-Issue association outcome.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IssueWorkAssociationOutcome {
  /// Applied or unchanged exact association replay.
  pub disposition: MutationDisposition,
  /// Retained exact association.
  pub association: IssueWorkAssociation,
}

impl AssociateIssueWork {
  /// Validates immutable admission lineage against the exact reviewed request.
  pub fn association(&self, reviewed: Option<&IssueExecutionRequest>) -> Result<IssueWorkAssociation, StoreError> {
    let conflict = || StoreError::Conflict {
      entity: octacity_server_domain::EntityKind::Issue,
    };
    let admission = &self.admission;
    admission.validate()?;
    if admission.work.subject().project_id() != self.project_id
      || admission.work.configuration().project_id() != self.project_id
      || self.request_id != reviewed.map(|r| r.id)
    {
      return Err(conflict());
    }
    if let Some(r) = reviewed {
      let t = &r.intent.target;
      if r.intent.project_id != self.project_id
        || r.intent.issue_id != self.issue_id
        || admission.work.external_identity() != &r.external_identity()
        || r.intent.classification.as_ref().is_some_and(|reviewed| {
          reviewed.priority() != admission.work.priority()
            || reviewed.risk() != admission.work.risk()
            || reviewed.metadata() != admission.work.metadata()
        })
        || admission.work.configuration().id() != t.configuration_id
        || admission.work.configuration().version() != t.configuration_version
        || !admission
          .work
          .subject()
          .repository()
          .is_ok_and(|subject| subject.repository_id() == t.repository_id)
        || admission.work.artifacts().task().content_digest() != r.task_digest
        || admission.work.artifacts().task().encoded_size() != r.task_size
        || admission.work.artifacts().acceptance().content_digest() != r.acceptance_digest
        || admission.work.artifacts().acceptance().encoded_size() != r.acceptance_size
      {
        return Err(conflict());
      }
    }
    let association = IssueWorkAssociation {
      project_id: self.project_id,
      issue_id: self.issue_id,
      request_id: self.request_id,
      requirements_version: reviewed.map(|r| r.intent.expected_version),
      work_id: admission.work.id(),
      run_id: admission.run.id(),
    };
    if reviewed
      .and_then(|r| r.association.as_ref())
      .is_some_and(|a| a != &association)
    {
      return Err(conflict());
    }
    Ok(association)
  }
}

impl RequestIssueExecution {
  /// Checks the reviewed current revision and freezes its exact mapped content.
  pub fn reserve(&self, issue: &crate::IssueRevision) -> Result<IssueExecutionOutcome, StoreError> {
    if issue.version != self.intent.expected_version {
      return Err(StoreError::Conflict {
        entity: octacity_server_domain::EntityKind::Issue,
      });
    }
    let acceptance = issue
      .content
      .acceptance
      .as_deref()
      .filter(|s| !s.trim().is_empty())
      .ok_or_else(|| {
        StoreError::invalid(
          crate::StoreOperation::RequestIssueExecution,
          crate::StoreInputError::IssueAcceptanceRequired,
        )
      })?;
    let task = self.intent.target.mapping.task_bytes(&issue.content);
    Ok(IssueExecutionOutcome {
      disposition: MutationDisposition::Applied,
      request: IssueExecutionRequest {
        id: self.id,
        task_digest: octacity_server_factory::FactoryDigest::content_sha256(&task),
        task_size: task.len() as u64,
        acceptance_size: acceptance.len() as u64,
        title_digest: octacity_server_factory::FactoryDigest::content_sha256(issue.content.title.as_bytes()),
        description_digest: octacity_server_factory::FactoryDigest::content_sha256(
          issue.content.description.as_bytes(),
        ),
        acceptance_digest: octacity_server_factory::FactoryDigest::content_sha256(acceptance.as_bytes()),
        intent: self.intent.clone(),
        requested_at: self.requested_at,
        association: None,
      },
    })
  }
}

/// Stable chronological continuation; timestamp ties use the original request identity.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueExecutionPosition {
  /// Original trusted reservation time.
  pub requested_at: Timestamp,
  /// Original request, never a current Run pointer.
  pub id: octacity_server_domain::IssueExecutionRequestId,
}
impl IssueExecutionRequest {
  /// Immutable ordering key, independent of mutable execution progress.
  pub fn position(&self) -> IssueExecutionPosition {
    IssueExecutionPosition {
      requested_at: self.requested_at,
      id: self.id,
    }
  }
}
/// Retained membership boundary; associations remain live observations on each page.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueExecutionObservation {
  /// Maximum committed reservation sequence at the start of this traversal.
  pub watermark: u64,
  /// Issue revision observed with that membership boundary.
  pub issue_version: IssueVersion,
}
/// Bounded metadata page with no inferred totals or private documents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssueExecutionReadPage {
  /// Retained requests in chronological order.
  pub items: Vec<IssueExecutionRequest>,
  /// Strict continuation key, absent at the end of observed membership.
  pub next: Option<IssueExecutionPosition>,
  /// Original membership observation.
  pub observation: IssueExecutionObservation,
}
impl IssueExecutionReadPage {
  /// Shapes at most limit plus one already ordered observations from either backend.
  pub fn bounded(
    mut items: Vec<IssueExecutionRequest>,
    observation: IssueExecutionObservation,
    limit: u16,
  ) -> Result<Self, StoreError> {
    if limit == 0 || limit > crate::MAX_ISSUE_HISTORY_PAGE_SIZE || items.len() > usize::from(limit) + 1 {
      return Err(StoreError::invalid(
        crate::StoreOperation::ListIssueExecutions,
        crate::StoreInputError::InvalidIssueExecutionPage,
      ));
    }
    let more = items.len() > usize::from(limit);
    if more {
      items.pop();
    }
    let next = if more {
      items.last().map(IssueExecutionRequest::position)
    } else {
      None
    };
    Ok(Self {
      items,
      next,
      observation,
    })
  }
}
