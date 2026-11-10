//! Issue ownership over immutable documents in the shared private input journal.
use crate::*;
use octacity_server_domain::{ArtifactId, ArtifactName, IssueId, ProjectId, Timestamp};
use octacity_server_factory::{FactoryArtifactReference, FactoryDigest};
use serde::{Deserialize, Serialize};

/// Distinguishes task documents from discoverable shared Project guidance.
pub const ISSUE_DOCUMENT_MAPPING: &str = "issue-document.v1";

/// Original owner and actual accepted management author of a task document.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueDocumentOwnership {
  /// Exact parent within the publication's Project.
  pub issue_id: IssueId,
  /// Original management author; replay never replaces this attribution.
  pub actor: AuditActor,
}

/// Reviewed text; no owner override, claimed actor or execution input selection.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueDocumentSubmission {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Exact owning Issue.
  pub issue_id: IssueId,
  /// Display name; editing publishes another exact immutable Artifact.
  pub name: ArtifactName,
  /// Private bounded UTF-8 text, omitted from diagnostics.
  pub content: String,
  /// Caller identity scoped to this Issue and management security partition.
  pub idempotency_key: IdempotencyKey,
}
impl std::fmt::Debug for IssueDocumentSubmission {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("IssueDocumentSubmission")
      .field("project_id", &self.project_id)
      .field("issue_id", &self.issue_id)
      .finish_non_exhaustive()
  }
}
impl IssueDocumentSubmission {
  /// Reuses existing byte validation, identities, lease and cleanup under an Issue-scoped replay key.
  pub fn into_reservation(self, now: Timestamp) -> Result<ReserveProjectDocument, StoreError> {
    let key = FactoryDigest::sha256(
      "octacity.issue-document-operation.v1",
      &[
        self.issue_id.as_uuid().as_bytes(),
        self.idempotency_key.as_str().as_bytes(),
      ],
    );
    let submission = ProjectDocumentSubmission {
      project_id: self.project_id,
      name: self.name,
      content: self.content,
      purpose: ProjectDocumentPurpose::Context,
      idempotency_key: IdempotencyKey::new(key.to_string())
        .map_err(|_| crate::projects::project_input_document::document_conflict())?,
    };
    submission.validate()?;
    Ok(ReserveProjectDocument {
      submission,
      issue_id: Some(self.issue_id),
      selection_digest: None,
      now,
    })
  }
}

/// Safe metadata of an exact immutable Issue document publication.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct IssueDocumentResource {
  /// Exact owning Project.
  pub project_id: ProjectId,
  /// Exact parent Issue, never inferred from Artifact visibility.
  pub issue_id: IssueId,
  /// Reviewed display name, without instruction authority.
  pub name: ArtifactName,
  /// Immutable bytes/digest/length; a new version has a new Artifact identity.
  pub artifact: FactoryArtifactReference,
  /// Original accepted author.
  pub actor: AuditActor,
  /// Original publication visibility point.
  pub published_at: Timestamp,
  /// Original exclusive byte retention deadline, never renewed by replay.
  pub expires_at: Timestamp,
}

/// Owner-scoped, content-free discovery with Artifact visibility applied before pagination.
#[derive(Clone, Debug)]
pub struct ListIssueDocuments {
  /// Shared bounded document page policy and Artifact visibility.
  pub page: ListProjectDocuments,
  /// Exact parent; another Issue cannot enumerate these documents.
  pub issue_id: IssueId,
}
/// A bounded page of exact Issue document versions, including no private bodies.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct IssueDocumentPage {
  /// Authorized publication metadata.
  pub items: Vec<IssueDocumentResource>,
  /// Exclusive Artifact cursor, absent at the end.
  pub next_cursor: Option<ArtifactId>,
}
/// Owner-aware metadata reads over the shared document publication journal.
#[async_trait::async_trait]
pub trait IssueDocumentInputStore: Send + Sync {
  /// Resolves private input metadata only after matching the exact Project/Issue owner.
  /// Generic Project input readers deliberately exclude these publications.
  async fn issue_document_input(
    &self,
    project: ProjectId,
    issue: IssueId,
    artifact: ArtifactId,
  ) -> Result<Option<PublishedProjectInput>, StoreError>;
  /// Returns exact metadata only for a publication owned by this Project/Issue.
  async fn issue_document_metadata(
    &self,
    project: ProjectId,
    issue: IssueId,
    artifact: ArtifactId,
  ) -> Result<Option<IssueDocumentResource>, StoreError>;
  /// Filters owner, retention and visibility before applying page boundaries.
  async fn list_issue_documents(&self, request: ListIssueDocuments) -> Result<IssueDocumentPage, StoreError>;
}
