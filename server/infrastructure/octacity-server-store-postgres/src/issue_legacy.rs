//! Reentrant migration uses ordinary Issue creation and the original association table.
use crate::{PostgresStore, database::unavailable, mutation::*};
use octacity_server_domain::{EntityKind, IssueId, ProjectId, Timestamp};
use octacity_server_factory::{FactoryDigest, FactoryRunId, WorkEnvelopeId};
use octacity_server_store::*;
use serde_json::json;
use sqlx::types::Json;

#[async_trait::async_trait]
impl IssueLegacyStore for PostgresStore {
  async fn legacy_issue_work(
    &self,
    project: ProjectId,
    issue: IssueId,
    after: Option<WorkEnvelopeId>,
    observation: Option<FactoryDigest>,
    limit: u16,
  ) -> Result<IssueLegacyWorkPage, StoreError> {
    let invalid = || StoreError::InvalidInput {
      operation: StoreOperation::ListIssueExecutions,
      source: StoreInputError::InvalidIssueExecutionPage,
    };
    if limit == 0 || limit > MAX_ISSUE_HISTORY_PAGE_SIZE || after.is_some() != observation.is_some() {
      return Err(invalid());
    }
    let mut tx = self.pool().begin().await.map_err(unavailable)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
      .execute(&mut *tx)
      .await
      .map_err(unavailable)?;
    let (version,count,last): (i64,i64,Option<String>)=sqlx::query_as("SELECT i.current_version,COUNT(a.work_id),MAX(a.work_id::text) FROM issues i LEFT JOIN issue_work_associations a ON a.project_id=i.project_id AND a.issue_id=i.id AND a.request_id IS NULL WHERE i.project_id=$1 AND i.id=$2 GROUP BY i.current_version")
      .bind(project.as_uuid()).bind(issue.as_uuid()).fetch_optional(&mut *tx).await.map_err(unavailable)?.ok_or(StoreError::NotFound { entity:EntityKind::Issue })?;
    let current = FactoryDigest::sha256(
      "octacity.issue.legacy-work-observation.v1",
      &[
        project.as_uuid().as_bytes(),
        issue.as_uuid().as_bytes(),
        &version.to_le_bytes(),
        &count.to_le_bytes(),
        last.as_deref().unwrap_or_default().as_bytes(),
      ],
    );
    if observation.is_some_and(|o| o != current) {
      return Err(StoreError::Conflict {
        entity: EntityKind::Issue,
      });
    }
    if let Some(after) = after {
      let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM issue_work_associations WHERE project_id=$1 AND issue_id=$2 AND work_id=$3 AND request_id IS NULL)")
        .bind(project.as_uuid()).bind(issue.as_uuid()).bind(after.as_uuid()).fetch_one(&mut *tx).await.map_err(unavailable)?;
      if !exists {
        return Err(invalid());
      }
    }
    let rows:Vec<Json<IssueWorkAssociation>>=sqlx::query_scalar("SELECT document FROM issue_work_associations WHERE project_id=$1 AND issue_id=$2 AND request_id IS NULL AND ($3::uuid IS NULL OR work_id>$3) ORDER BY work_id LIMIT $4")
      .bind(project.as_uuid()).bind(issue.as_uuid()).bind(after.map(|id|id.as_uuid())).bind(i64::from(limit)+1).fetch_all(&mut *tx).await.map_err(unavailable)?;
    let mut items: Vec<_> = rows.into_iter().map(|Json(row)| row).collect();
    let more = items.len() > usize::from(limit);
    items.truncate(usize::from(limit));
    let next = more.then(|| items.last().unwrap().work_id);
    tx.commit().await.map_err(unavailable)?;
    Ok(IssueLegacyWorkPage {
      items,
      observation: current,
      next,
    })
  }
  async fn backfill_legacy_issue_work(
    &self,
    limit: u16,
    at: Timestamp,
  ) -> Result<Vec<IssueWorkAssociation>, StoreError> {
    if limit == 0 || limit > MAX_ISSUE_HISTORY_PAGE_SIZE {
      return Err(StoreError::InvalidInput {
        operation: StoreOperation::CreateIssue,
        source: StoreInputError::InvalidIssue,
      });
    }
    let rows: Vec<(uuid::Uuid, uuid::Uuid, uuid::Uuid)> = sqlx::query_as(
      "SELECT w.project_id,w.id,r.id FROM issue_legacy_backfill b JOIN factory_work_envelopes w ON w.id=b.work_id JOIN factory_runs r ON r.work_envelope_id=w.id WHERE r.visible AND NOT EXISTS(SELECT 1 FROM issue_work_associations a WHERE a.work_id=w.id) ORDER BY b.work_id LIMIT $1")
      .bind(i64::from(limit)).fetch_all(self.pool()).await.map_err(unavailable)?;
    let mut associated = Vec::with_capacity(rows.len());
    for (project, work, run) in rows {
      let project = ProjectId::from_uuid(project).map_err(|_| StoreError::Unavailable)?;
      let work = WorkEnvelopeId::from_uuid(work).map_err(|_| StoreError::Unavailable)?;
      let run = FactoryRunId::from_uuid(run).map_err(|_| StoreError::Unavailable)?;
      let identity = MutationIdentity::new_non_management(
        MutationKind::BackfillIssueWork,
        work.to_string(),
        at,
        EntityKind::Issue,
        &work,
      )?;
      let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
        MutationStart::Replay(value) => {
          associated.push(decode_outcome(value)?);
          continue;
        }
        MutationStart::Fresh(tx) => tx,
      };
      // Same Project fence as manual/import creation; no cross-Project authority.
      crate::issue_workflow::lock_project(&mut tx, project).await?;
      let actor = AuditActor {
        kind: AuditActorKind::Worker,
        identity: Some("issue-legacy-backfill".into()),
      };
      let native: Option<(Vec<u8>, Vec<u8>)> = sqlx::query_as("SELECT p.issue,w.security_scope_digest FROM factory_source_preparations p JOIN factory_work_envelopes w ON w.project_id=p.project_id AND w.id=p.work_id JOIN factory_runs r ON r.id=p.run_id AND r.work_envelope_id=w.id WHERE p.project_id=$1 AND p.work_id=$2 AND p.settled")
        .bind(project.as_uuid()).bind(work.as_uuid()).fetch_optional(&mut *tx).await.map_err(unavailable)?;
      let existing = match native {
        Some((native, partition)) => {
          crate::issue_source::native_issue(&mut tx, project, digest(native)?, digest(partition)?).await?
        }
        None => None,
      };
      let issue_id = if let Some(issue) = existing {
        crate::issue::lock(&mut tx, project, issue).await?;
        issue
      } else {
        let issue = crate::issue::create_initial_issue(
          &mut tx,
          &CreateIssue {
            id: IssueId::generate(),
            assignees: Default::default(),
            intent: CreateIssueIntent {
              project_id: project,
              planning: Default::default(),
              content: IssueContent {
                title: "Legacy task (content unavailable)".into(),
                description: String::new(),
                acceptance: None,
                priority: None,
              },
            },
            idempotency_key: format!("legacy.{work}").parse().map_err(|_| StoreError::Unavailable)?,
            created_at: at,
          },
          &actor,
          IssueTransitionOrigin::Manual,
          false,
        )
        .await?;
        issue.id
      };
      // Match the ordinary association lock order: Issue before the original Run.
      let visible: bool = sqlx::query_scalar("SELECT visible FROM factory_runs WHERE id=$1 FOR UPDATE")
        .bind(run.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(unavailable)?;
      if !visible
        || crate::issue_execution::read_association(&mut tx, project, work)
          .await?
          .is_some()
      {
        continue;
      }
      let association = IssueWorkAssociation {
        project_id: project,
        issue_id,
        request_id: None,
        requirements_version: None,
        work_id: work,
        run_id: run,
      };
      crate::issue_execution::insert_association(&mut tx, &association).await?;
      sqlx::query("DELETE FROM issue_legacy_backfill WHERE work_id=$1")
        .bind(work.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?;
      let metadata = json!({"project_id":project,"issue_id":issue_id,"work_id":work,"run_id":run});
      crate::mutation::commit(
        tx,
        &identity,
        MutationFacts::non_management(
          NonManagementActor::Worker("issue-legacy-backfill".into()),
          issue_id.to_string(),
          metadata.clone(),
          metadata,
        ),
        encode_outcome(&association)?,
      )
      .await?;
      associated.push(association);
    }
    Ok(associated)
  }
}

fn digest(bytes: Vec<u8>) -> Result<FactoryDigest, StoreError> {
  Ok(FactoryDigest::from_bytes(
    bytes.try_into().map_err(|_| StoreError::Unavailable)?,
  ))
}
