//! Material selection uses the existing revision and scoped management receipt owners.
use crate::{
  PostgresStore,
  database::unavailable,
  mutation::{MutationIdentity, MutationKind, MutationStart, decode_outcome},
};
use octacity_server_domain::EntityKind;
use octacity_server_store::*;
use serde_json::Value;
use sqlx::types::Json;

#[async_trait::async_trait]
impl IssueMaterialStore for PostgresStore {
  async fn issue_material_edit(
    &self,
    scope: &ManagementSecurityScope,
    key: &IdempotencyKey,
  ) -> Result<Option<IssueMaterialEditReceipt>, StoreError> {
    let value: Option<Json<Value>> = sqlx::query_scalar(
      "SELECT outcome FROM idempotency_records WHERE scope=$1 AND security_scope=$2 AND idempotency_key=$3",
    )
    .bind(MutationKind::EditIssueMaterials.scope())
    .bind(scope.as_str())
    .bind(key.as_str())
    .fetch_optional(self.pool())
    .await
    .map_err(unavailable)?;
    value
      .map(|Json(value)| {
        let intent = serde_json::from_value(value.get("retained_command").cloned().ok_or(StoreError::Unavailable)?)
          .map_err(|_| StoreError::Unavailable)?;
        Ok(IssueMaterialEditReceipt {
          intent,
          outcome: decode_outcome(value)?,
        })
      })
      .transpose()
  }
  async fn edit_issue_materials(
    &self,
    request: ManagementMutation<EditIssueMaterials>,
  ) -> Result<IssueMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let intent = &request.intent;
    let identity = MutationIdentity::new_management(
      &audit,
      MutationKind::EditIssueMaterials,
      request.idempotency_key.to_string(),
      request.edited_at,
      EntityKind::Issue,
      intent,
    )?;
    // Keep the ordinary flat Issue outcome so existing retention redaction also
    // covers selection receipts. The sealed command contains references only.
    let command = serde_json::to_value(intent).map_err(|_| StoreError::Unavailable)?;
    let mut tx = match crate::mutation::begin_retained(self.pool(), &identity, command).await? {
      MutationStart::Replay(value) => {
        let mut outcome: IssueMutationOutcome = decode_outcome(value)?;
        outcome.disposition = MutationDisposition::Replayed;
        return Ok(outcome);
      }
      MutationStart::Fresh(tx) => tx,
    };
    super::issue_workflow::lock_project(&mut tx, intent.project_id).await?;
    let current = super::issue::lock(&mut tx, intent.project_id, intent.id).await?;
    let outcome = IssueMutationOutcome {
      disposition: MutationDisposition::Applied,
      issue: current.with_materials(&request, audit.actor())?,
    };
    validate_sources(&mut tx, &request).await?;
    super::issue::publish(tx, &identity, &audit, &outcome.issue, None, &outcome, None).await?;
    Ok(outcome)
  }
}

// The Issue head excludes discussion cleanup. Document publication/cleanup use
// their own journal rows; share-lock those sources through revision commit.
// Removed references are deliberately not reopened, and replay precedes this check.
async fn validate_sources(connection: &mut sqlx::PgConnection, request: &EditIssueMaterials) -> Result<(), StoreError> {
  let invalid = || StoreError::InvalidInput {
    operation: StoreOperation::EditIssueMaterials,
    source: StoreInputError::InvalidIssue,
  };
  for reference in &request.intent.references {
    let (owner, artifact, mapping) = match reference {
      IssueMaterialReference::Comment {
        comment_id,
        body_digest,
        ..
      } => {
        let metadata: Option<Json<IssueCommentMetadata>> = sqlx::query_scalar(
          "SELECT metadata FROM issue_comments WHERE project_id=$1 AND issue_id=$2 AND id=$3 AND body IS NOT NULL",
        )
        .bind(request.intent.project_id.as_uuid())
        .bind(request.intent.id.as_uuid())
        .bind(comment_id.as_uuid())
        .fetch_optional(&mut *connection)
        .await
        .map_err(unavailable)?;
        if metadata.is_none_or(|Json(comment)| comment.body_digest != *body_digest) {
          return Err(invalid());
        }
        continue;
      }
      IssueMaterialReference::IssueDocument { issue_id, artifact, .. } => {
        (Some(*issue_id), artifact, ISSUE_DOCUMENT_MAPPING)
      }
      IssueMaterialReference::ProjectDocument { artifact, .. } => (None, artifact, PROJECT_CONTEXT_DOCUMENT_MAPPING),
    };
    let retained: Option<Json<octacity_server_factory::FactoryArtifactReference>> = sqlx::query_scalar(
      "SELECT entry.object->'artifact' FROM project_text_preparations p \
       JOIN project_text_input_objects o ON o.preparation_id=p.id \
       JOIN LATERAL jsonb_array_elements(p.document->'objects') entry(object) \
         ON entry.object->'artifact'->>'artifact_id'=o.artifact_id::text \
       WHERE p.project_id=$1 AND p.issue_id IS NOT DISTINCT FROM $2::uuid \
         AND o.artifact_id=$3 AND o.role='document' AND o.deleted_at_millis IS NULL \
         AND p.document->'intent'->>'mapping'=$4 AND p.document->>'published_at' IS NOT NULL \
         AND (p.document->>'expires_at')::bigint>$5 FOR SHARE OF p,o",
    )
    .bind(request.intent.project_id.as_uuid())
    .bind(owner.map(|id| id.as_uuid()))
    .bind(artifact.artifact_id().as_uuid())
    .bind(mapping)
    .bind(request.edited_at.unix_millis())
    .fetch_optional(&mut *connection)
    .await
    .map_err(unavailable)?;
    if retained.is_none_or(|Json(retained)| retained != *artifact) {
      return Err(invalid());
    }
  }
  Ok(())
}
