//! Discussion uses the common scoped mutation transaction; bodies stay outside its receipt/evidence.
use crate::{
  PostgresStore,
  database::unavailable,
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};
use octacity_server_domain::{EntityKind, IssueCommentId, IssueId, ProjectId};
use octacity_server_factory::FactoryDigest;
use octacity_server_store::*;
use sqlx::types::Json;

#[async_trait::async_trait]
impl IssueDiscussionStore for PostgresStore {
  async fn append_issue_comment(
    &self,
    request: ManagementMutation<AppendIssueComment>,
  ) -> Result<IssueCommentOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let intent = &request.intent;
    // Existence is checked before replay as well as inside the atomic append.
    self.issue(intent.project_id, intent.issue_id, None).await?;
    let identity = MutationIdentity::new_management(
      &audit,
      MutationKind::AppendIssueComment,
      request.idempotency_key.to_string(),
      request.created_at,
      EntityKind::Issue,
      intent,
    )?;
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => {
        let mut outcome: IssueCommentOutcome = decode_outcome(value)?;
        outcome.disposition = MutationDisposition::Replayed;
        return Ok(outcome);
      }
      MutationStart::Fresh(tx) => tx,
    };
    crate::issue::lock(&mut tx, intent.project_id, intent.issue_id).await?;
    if intent.body.as_str().len() > request.limits.body_bytes() {
      return Err(invalid());
    }
    // Serialize append allocation on the existing Issue lock. This counter survives body cleanup.
    let sequence: i64 = sqlx::query_scalar("UPDATE issues SET discussion_count=discussion_count+1 WHERE project_id=$1 AND id=$2 AND discussion_count<$3 RETURNING discussion_count")
      .bind(intent.project_id.as_uuid()).bind(intent.issue_id.as_uuid()).bind(i64::from(request.limits.lifetime_comments()))
      .fetch_optional(&mut *tx).await.map_err(unavailable)?.ok_or(StoreError::InvalidInput {
        operation: StoreOperation::AppendIssueComment, source: StoreInputError::IssueDiscussionQuotaReached,
      })?;
    if let Some(parent) = intent.parent_id {
      let root: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM issue_comments WHERE project_id=$1 AND issue_id=$2 AND id=$3 AND parent_id IS NULL)")
        .bind(intent.project_id.as_uuid()).bind(intent.issue_id.as_uuid()).bind(parent.as_uuid())
        .fetch_one(&mut *tx).await.map_err(unavailable)?;
      if !root {
        return Err(invalid());
      }
    }
    let comment = IssueCommentMetadata {
      id: request.id,
      project_id: intent.project_id,
      issue_id: intent.issue_id,
      sequence: sequence as u64,
      parent_id: intent.parent_id,
      actor: audit.actor().clone(),
      created_at: request.created_at,
      body_digest: FactoryDigest::content_sha256(intent.body.as_str().as_bytes()),
      body_bytes: intent.body.as_str().len() as u32,
    };
    sqlx::query("INSERT INTO issue_comments (project_id,issue_id,id,sequence,parent_id,metadata,body) VALUES ($1,$2,$3,$4,$5,$6,$7)")
      .bind(intent.project_id.as_uuid()).bind(intent.issue_id.as_uuid()).bind(request.id.as_uuid()).bind(sequence)
      .bind(intent.parent_id.map(|id| id.as_uuid())).bind(Json(&comment)).bind(intent.body.as_str())
      .execute(&mut *tx).await.map_err(unavailable)?;
    let outcome = IssueCommentOutcome {
      disposition: MutationDisposition::Applied,
      comment,
    };
    let metadata = serde_json::to_value(&outcome.comment).map_err(|_| StoreError::Unavailable)?;
    crate::mutation::commit(
      tx,
      &identity,
      MutationFacts::management(&audit, intent.issue_id.to_string(), metadata.clone(), metadata),
      encode_outcome(&outcome)?,
    )
    .await?;
    Ok(outcome)
  }
  async fn issue_comment_body(
    &self,
    project: ProjectId,
    issue: IssueId,
    comment: IssueCommentId,
  ) -> Result<IssueCommentContent, StoreError> {
    self.issue(project, issue, None).await?;
    let (Json(comment), body): (Json<IssueCommentMetadata>, String) = sqlx::query_as(
      "SELECT metadata,body FROM issue_comments WHERE project_id=$1 AND issue_id=$2 AND id=$3 AND body IS NOT NULL",
    )
    .bind(project.as_uuid())
    .bind(issue.as_uuid())
    .bind(comment.as_uuid())
    .fetch_optional(self.pool())
    .await
    .map_err(unavailable)?
    .ok_or(StoreError::NotFound {
      entity: EntityKind::Issue,
    })?;
    Ok(IssueCommentContent {
      comment,
      body: IssueCommentBody::try_new(body)?,
    })
  }
  async fn issue_comments(
    &self,
    project: ProjectId,
    issue: IssueId,
    after: Option<u64>,
    limit: u16,
  ) -> Result<IssueCommentPage, StoreError> {
    if limit == 0 || limit > MAX_ISSUE_HISTORY_PAGE_SIZE {
      return Err(invalid());
    }
    // Every sequence is below the lifetime ceiling; larger continuations mean an empty page.
    let after = after.map(|s| s.min(u64::from(MAX_ISSUE_DISCUSSION_COMMENTS)) as i64);
    self.issue(project, issue, None).await?;
    let rows: Vec<(Json<IssueCommentMetadata>,)> = sqlx::query_as("SELECT metadata FROM issue_comments WHERE project_id=$1 AND issue_id=$2 AND ($3::bigint IS NULL OR sequence>$3) ORDER BY sequence LIMIT $4")
      .bind(project.as_uuid()).bind(issue.as_uuid()).bind(after).bind(i64::from(limit)+1).fetch_all(self.pool()).await.map_err(unavailable)?;
    let mut items: Vec<_> = rows.into_iter().map(|(Json(m),)| m).collect();
    let more = items.len() > usize::from(limit);
    if more {
      items.pop();
    }
    let next_sequence = more.then(|| items.last().unwrap().sequence);
    Ok(IssueCommentPage { items, next_sequence })
  }
}
fn invalid() -> StoreError {
  StoreError::InvalidInput {
    operation: StoreOperation::AppendIssueComment,
    source: StoreInputError::InvalidIssueDiscussion,
  }
}
