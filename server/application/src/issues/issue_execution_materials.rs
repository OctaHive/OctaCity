//! Composes existing authorized content readers; Issue execution grants no body-read shortcut.
use crate::*;
use octacity_server_factory::FactoryDigest;
use octacity_server_store::{IssueMaterialReference, IssueSnapshotMaterialSource, ProjectDocumentQueryStore};
use std::sync::Arc;

/// Installed independent body readers used only for explicitly selected Issue input.
pub struct IssueSnapshotMaterialReaders {
  comments: Arc<dyn AuthorizedManagementQueryHandler<GetIssueCommentBodyQuery, Error = ApplicationError>>,
  documents: Option<DocumentReaders>,
}
struct DocumentReaders {
  owned: Arc<dyn AuthorizedManagementQueryHandler<ReadIssueDocumentQuery, Error = ApplicationError>>,
  shared: Arc<dyn AuthorizedManagementQueryHandler<ReadProjectInputDocumentQuery, Error = ApplicationError>>,
  metadata: Arc<dyn ProjectDocumentQueryStore>,
}
impl IssueSnapshotMaterialReaders {
  /// Uses the ordinary authorized discussion query, preserving its owner/retention checks.
  pub fn new(
    comments: Arc<dyn AuthorizedManagementQueryHandler<GetIssueCommentBodyQuery, Error = ApplicationError>>,
  ) -> Self {
    Self {
      comments,
      documents: None,
    }
  }
  /// Uses exact owner-aware document queries; metadata is read only after authorized body access.
  pub fn with_documents(
    mut self,
    owned: Arc<dyn AuthorizedManagementQueryHandler<ReadIssueDocumentQuery, Error = ApplicationError>>,
    shared: Arc<dyn AuthorizedManagementQueryHandler<ReadProjectInputDocumentQuery, Error = ApplicationError>>,
    metadata: Arc<dyn ProjectDocumentQueryStore>,
  ) -> Self {
    self.documents = Some(DocumentReaders {
      owned,
      shared,
      metadata,
    });
    self
  }
  pub(crate) async fn read(
    &self,
    context: &ManagementRequestContext,
    reference: &IssueMaterialReference,
  ) -> Result<(String, IssueSnapshotMaterialSource), ApplicationError> {
    match reference {
      IssueMaterialReference::Comment {
        project_id,
        issue_id,
        comment_id,
        body_digest,
      } => {
        let content = self
          .comments
          .handle_authorized_query(
            context,
            GetIssueCommentBodyQuery {
              project_id: *project_id,
              issue_id: *issue_id,
              comment_id: *comment_id,
            },
          )
          .await
          .map_err(|_| ApplicationError::unavailable())?;
        let body = content.body.as_str();
        if content.comment.project_id != *project_id
          || content.comment.issue_id != *issue_id
          || content.comment.id != *comment_id
          || content.comment.body_digest != *body_digest
          || FactoryDigest::content_sha256(body.as_bytes()) != *body_digest
          || body.len() as u64 != u64::from(content.comment.body_bytes)
        {
          return Err(ApplicationError::invalid());
        }
        Ok((
          body.to_owned(),
          IssueSnapshotMaterialSource::Comment {
            comment: content.comment,
          },
        ))
      }
      IssueMaterialReference::IssueDocument {
        project_id,
        issue_id,
        artifact,
      } => {
        let readers = self.documents.as_ref().ok_or_else(ApplicationError::unavailable)?;
        let content = readers
          .owned
          .handle_authorized_query(
            context,
            ReadIssueDocumentQuery {
              project_id: *project_id,
              issue_id: *issue_id,
              artifact_id: artifact.artifact_id(),
            },
          )
          .await
          .map_err(|_| ApplicationError::unavailable())?;
        if content.document.project_id != *project_id
          || content.document.issue_id != *issue_id
          || content.document.artifact != *artifact
        {
          return Err(ApplicationError::invalid());
        }
        verify_text(artifact, &content.content)?;
        Ok((
          content.content,
          IssueSnapshotMaterialSource::IssueDocument {
            document: content.document,
          },
        ))
      }
      IssueMaterialReference::ProjectDocument { project_id, artifact } => {
        let readers = self.documents.as_ref().ok_or_else(ApplicationError::unavailable)?;
        let content = readers
          .shared
          .handle_authorized_query(
            context,
            ReadProjectInputDocumentQuery {
              project_id: *project_id,
              artifact_id: artifact.artifact_id(),
            },
          )
          .await
          .map_err(|_| ApplicationError::unavailable())?;
        let document = readers
          .metadata
          .project_document_metadata(*project_id, artifact.artifact_id())
          .await?
          .ok_or_else(ApplicationError::unavailable)?;
        if content.project_id != *project_id
          || content.artifact != *artifact
          || document.project_id != *project_id
          || document.artifact != *artifact
        {
          return Err(ApplicationError::invalid());
        }
        verify_text(artifact, &content.content)?;
        Ok((
          content.content,
          IssueSnapshotMaterialSource::ProjectDocument { document },
        ))
      }
    }
  }
}

fn verify_text(
  artifact: &octacity_server_factory::FactoryArtifactReference,
  text: &str,
) -> Result<(), ApplicationError> {
  if FactoryDigest::content_sha256(text.as_bytes()) != artifact.content_digest()
    || text.len() as u64 != artifact.encoded_size()
  {
    return Err(ApplicationError::invalid());
  }
  Ok(())
}
