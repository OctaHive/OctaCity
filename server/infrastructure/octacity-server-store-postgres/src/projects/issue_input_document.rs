//! Exact task document reads share publication metadata and immutable objects with Project inputs.
use crate::{PostgresStore, database::unavailable};
use octacity_server_domain::{ArtifactId, IssueId, ProjectId};
use octacity_server_store::*;
use serde_json::Value;
use sqlx::types::Json;

#[async_trait::async_trait]
impl IssueDocumentInputStore for PostgresStore {
  async fn issue_document_metadata(
    &self,
    project: ProjectId,
    issue: IssueId,
    artifact: ArtifactId,
  ) -> Result<Option<IssueDocumentResource>, StoreError> {
    let row: Option<Json<Value>> = sqlx::query_scalar(
      "SELECT p.document FROM project_text_preparations p \
       JOIN project_text_input_objects o ON o.preparation_id=p.id \
       WHERE p.project_id=$1 AND p.issue_id=$2 \
         AND o.artifact_id=$3 AND o.role='document' \
         AND p.document->'intent'->>'mapping'=$4 AND p.document->>'published_at' IS NOT NULL",
    )
    .bind(project.as_uuid())
    .bind(issue.as_uuid())
    .bind(artifact.as_uuid())
    .bind(ISSUE_DOCUMENT_MAPPING)
    .fetch_optional(self.pool())
    .await
    .map_err(unavailable)?;
    row
      .map(|row| {
        let document = super::project_input_document::decode(row)?.issue_resource()?;
        if document.project_id != project || document.issue_id != issue || document.artifact.artifact_id() != artifact {
          return Err(super::project_input_document::conflict());
        }
        Ok(document)
      })
      .transpose()
  }
  async fn issue_document_input(
    &self,
    project: ProjectId,
    issue: IssueId,
    artifact: ArtifactId,
  ) -> Result<Option<PublishedProjectInput>, StoreError> {
    if self.issue_document_metadata(project, issue, artifact).await?.is_none() {
      return Ok(None);
    }
    // Ownership is immutable. The shared reader also rejects deleted/unpublished objects.
    super::project_text_input::published_input(self.pool(), artifact, Some(project), Some(issue)).await
  }
  async fn list_issue_documents(&self, request: ListIssueDocuments) -> Result<IssueDocumentPage, StoreError> {
    let page = request.page;
    page.validate()?;
    crate::discovery::require_project(self.pool(), page.project_id).await?;
    let (all, visible): (bool, Vec<uuid::Uuid>) = match page.visibility.view() {
      ReadVisibilityView::All => (true, vec![]),
      ReadVisibilityView::None => (false, vec![]),
      ReadVisibilityView::Restricted(ids) => (false, ids.iter().map(|id| id.as_uuid()).collect()),
    };
    let rows: Vec<Json<Value>> = sqlx::query_scalar(
      "SELECT p.document FROM project_text_preparations p \
       JOIN project_text_input_objects o ON o.preparation_id=p.id \
       WHERE p.project_id=$1 AND p.issue_id=$2 \
         AND p.document->'intent'->>'mapping'=$3 AND o.role='document' \
         AND p.document->>'published_at' IS NOT NULL AND (p.document->>'expires_at')::bigint>$4 \
         AND o.deleted_at_millis IS NULL AND ($5::uuid IS NULL OR o.artifact_id>$5) \
         AND ($6 OR o.artifact_id=ANY($7::uuid[])) ORDER BY o.artifact_id LIMIT $8",
    )
    .bind(page.project_id.as_uuid())
    .bind(request.issue_id.as_uuid())
    .bind(ISSUE_DOCUMENT_MAPPING)
    .bind(page.observed_at.unix_millis())
    .bind(page.after.map(|id| id.as_uuid()))
    .bind(all)
    .bind(visible)
    .bind(crate::discovery::requested_row_limit(page.limit))
    .fetch_all(self.pool())
    .await
    .map_err(unavailable)?;
    let (items, next_cursor) = crate::discovery::decode_page(
      rows,
      page.limit,
      |row| super::project_input_document::decode(row)?.issue_resource(),
      |resource| resource.artifact.artifact_id(),
    )?;
    Ok(IssueDocumentPage { items, next_cursor })
  }
}
