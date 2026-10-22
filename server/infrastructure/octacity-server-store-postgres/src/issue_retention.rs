//! Body tombstones and receipt redaction commit together under the ordinary Issue fence.
use crate::{PostgresStore, database::unavailable, mutation::*};
use octacity_server_domain::{EntityKind, Timestamp};
use octacity_server_store::*;
use serde_json::json;
use sqlx::types::Json;

fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Issue,
  }
}

#[async_trait::async_trait]
impl IssueContentRetentionStore for PostgresStore {
  async fn issue_content_retention(
    &self,
    project: octacity_server_domain::ProjectId,
    issue: octacity_server_domain::IssueId,
    kind: IssueContentRetentionKind,
  ) -> Result<Option<IssueContentRetentionState>, StoreError> {
    let row:Option<(Option<Json<IssueContentRetentionState>>,)> = sqlx::query_as("SELECT r.document FROM issues i LEFT JOIN issue_content_retention r ON r.issue_id=i.id AND r.kind=$3 WHERE i.project_id=$1 AND i.id=$2")
      .bind(project.as_uuid()).bind(issue.as_uuid()).bind(kind.as_str()).fetch_optional(self.pool()).await.map_err(unavailable)?;
    row
      .map(|(value,)| value.map(|Json(state)| state))
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Issue,
      })
  }
  async fn set_issue_content_retention(
    &self,
    request: ManagementMutation<SetIssueContentRetention>,
  ) -> Result<IssueContentRetentionState, StoreError> {
    let (request, audit) = request.into_parts();
    request.plan.validate()?;
    let fingerprint = json!({"project_id":request.project_id,"issue_id":request.issue_id,"expected_version":request.expected_version,"plan":request.plan});
    let identity = MutationIdentity::new_management(
      &audit,
      MutationKind::SetIssueContentRetention,
      request.idempotency_key.to_string(),
      request.configured_at,
      EntityKind::Issue,
      &fingerprint,
    )?;
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => return decode_outcome(value),
      MutationStart::Fresh(tx) => tx,
    };
    let issue = crate::issue::lock(&mut tx, request.project_id, request.issue_id).await?;
    let (version, count): (i64, i64) = sqlx::query_as("SELECT COALESCE((SELECT version FROM issue_content_retention WHERE issue_id=$2 AND kind=$3),0),discussion_count FROM issues WHERE project_id=$1 AND id=$2")
      .bind(request.project_id.as_uuid()).bind(request.issue_id.as_uuid()).bind(request.plan.scope.kind().as_str()).fetch_one(&mut *tx).await.map_err(unavailable)?;
    if u64::try_from(version).ok() != Some(request.expected_version)
      || match request.plan.scope {
        IssueContentRetentionScope::Requirements { through } => through > issue.version,
        IssueContentRetentionScope::Discussion { through } => through > u64::try_from(count).map_err(|_| conflict())?,
      }
    {
      return Err(conflict());
    }
    let state = IssueContentRetentionState {
      project_id: request.project_id,
      issue_id: request.issue_id,
      version: request.expected_version.checked_add(1).ok_or_else(conflict)?,
      plan: request.plan,
    };
    sqlx::query("INSERT INTO issue_content_retention(project_id,issue_id,version,document,kind) VALUES($1,$2,$3,$4,$5) ON CONFLICT(issue_id,kind) DO UPDATE SET version=EXCLUDED.version,document=EXCLUDED.document,completed=FALSE")
      .bind(state.project_id.as_uuid()).bind(state.issue_id.as_uuid()).bind(i64::try_from(state.version).map_err(|_|conflict())?).bind(Json(&state)).bind(state.plan.scope.kind().as_str()).execute(&mut *tx).await.map_err(unavailable)?;
    let metadata = json!({"project_id":state.project_id,"issue_id":state.issue_id,"retention_version":state.version});
    crate::mutation::commit(
      tx,
      &identity,
      MutationFacts::management(&audit, state.issue_id.to_string(), metadata.clone(), metadata),
      encode_outcome(&state)?,
    )
    .await?;
    Ok(state)
  }
  async fn expire_issue_content(&self, at: Timestamp, limit: u16) -> Result<u16, StoreError> {
    if limit == 0 || limit > MAX_ISSUE_HISTORY_PAGE_SIZE {
      return Err(StoreError::InvalidInput {
        operation: StoreOperation::EditIssue,
        source: StoreInputError::InvalidIssue,
      });
    }
    let candidate: Option<Json<IssueContentRetentionState>> = sqlx::query_scalar("SELECT document FROM issue_content_retention WHERE NOT completed AND expires_at_millis<=$1 AND (document->'plan'->'hold'='null'::jsonb OR (document->'plan'->'hold'->>'expires_at')::bigint<=$1) ORDER BY expires_at_millis,issue_id LIMIT 1")
      .bind(at.unix_millis()).fetch_optional(self.pool()).await.map_err(unavailable)?;
    let Some(Json(candidate)) = candidate else {
      return Ok(0);
    };
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    crate::issue::lock(&mut tx, candidate.project_id, candidate.issue_id).await?;
    let Json(current): Json<IssueContentRetentionState> =
      sqlx::query_scalar("SELECT document FROM issue_content_retention WHERE issue_id=$1 AND kind=$2 FOR UPDATE")
        .bind(candidate.issue_id.as_uuid())
        .bind(candidate.plan.scope.kind().as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(unavailable)?;
    if current != candidate {
      return Ok(0);
    }
    let changed = match current.plan.scope {
      IssueContentRetentionScope::Requirements { through } => {
        let unavailable_content =
          json!({"title":"Task content unavailable","description":"","acceptance":null,"priority":null});
        let revisions: Vec<i64> = sqlx::query_scalar("WITH selected AS (SELECT project_id,issue_id,version FROM issue_revisions WHERE project_id=$1 AND issue_id=$2 AND version<=$3 AND COALESCE((document->>'content_available')::boolean,TRUE) ORDER BY version LIMIT $4 FOR UPDATE) UPDATE issue_revisions r SET document=jsonb_set(jsonb_set(r.document,'{content}',$5),'{content_available}','false') FROM selected s WHERE (r.project_id,r.issue_id,r.version)=(s.project_id,s.issue_id,s.version) RETURNING r.version")
          .bind(current.project_id.as_uuid()).bind(current.issue_id.as_uuid()).bind(i64::try_from(through.get()).map_err(|_|conflict())?).bind(i64::from(limit)).bind(Json(&unavailable_content)).fetch_all(&mut *tx).await.map_err(unavailable)?;
        // Original receipt identity/version survives, but never resurrects an expired body.
        if !revisions.is_empty() {
          sqlx::query("UPDATE idempotency_records SET outcome=jsonb_set(jsonb_set(outcome,'{value,issue,content}',$4),'{value,issue,content_available}','false') WHERE outcome#>>'{value,issue,project_id}'=$1 AND outcome#>>'{value,issue,id}'=$2 AND (outcome#>>'{value,issue,version}')::bigint=ANY($3)")
            .bind(current.project_id.to_string()).bind(current.issue_id.to_string()).bind(&revisions).bind(Json(&unavailable_content)).execute(&mut *tx).await.map_err(unavailable)?;
        }
        revisions.len()
      }
      IssueContentRetentionScope::Discussion { through } => {
        let changed=sqlx::query("WITH selected AS (SELECT id FROM issue_comments WHERE project_id=$1 AND issue_id=$2 AND sequence<=$3 AND body IS NOT NULL ORDER BY sequence LIMIT $4 FOR UPDATE) UPDATE issue_comments SET body=NULL WHERE id IN (SELECT id FROM selected)")
          .bind(current.project_id.as_uuid()).bind(current.issue_id.as_uuid()).bind(i64::try_from(through).map_err(|_|conflict())?).bind(i64::from(limit)).execute(&mut *tx).await.map_err(unavailable)?.rows_affected();
        usize::try_from(changed).map_err(|_| conflict())?
      }
    };
    // Finish in the same pass, so a completed lifetime cannot starve another due plan.
    let (query, through) = match current.plan.scope {
      IssueContentRetentionScope::Requirements { through } => (
        "SELECT EXISTS(SELECT 1 FROM issue_revisions WHERE project_id=$1 AND issue_id=$2 AND version<=$3 AND COALESCE((document->>'content_available')::boolean,TRUE))",
        through.get(),
      ),
      IssueContentRetentionScope::Discussion { through } => (
        "SELECT EXISTS(SELECT 1 FROM issue_comments WHERE project_id=$1 AND issue_id=$2 AND sequence<=$3 AND body IS NOT NULL)",
        through,
      ),
    };
    let pending: bool = sqlx::query_scalar(query)
      .bind(current.project_id.as_uuid())
      .bind(current.issue_id.as_uuid())
      .bind(i64::try_from(through).map_err(|_| conflict())?)
      .fetch_one(&mut *tx)
      .await
      .map_err(unavailable)?;
    if !pending {
      sqlx::query("UPDATE issue_content_retention SET completed=TRUE WHERE issue_id=$1 AND kind=$2")
        .bind(current.issue_id.as_uuid())
        .bind(current.plan.scope.kind().as_str())
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?;
    }
    tx.commit().await.map_err(unavailable)?;
    u16::try_from(changed).map_err(|_| conflict())
  }
}
