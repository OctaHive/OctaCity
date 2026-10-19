//! Durable reviewed requests; execution remains ordinary Factory admission.
use crate::{
  PostgresStore,
  database::unavailable,
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};
use octacity_server_domain::{EntityKind, IssueExecutionRequestId, IssueId, ProjectId};
use octacity_server_factory::WorkEnvelopeId;
use octacity_server_store::*;
use serde_json::json;
use sqlx::types::Json;

fn identity(request: &ManagementMutation<RequestIssueExecution>) -> Result<MutationIdentity, StoreError> {
  let r = request.mutation();
  MutationIdentity::new_management(
    request.audit(),
    MutationKind::RequestIssueExecution,
    r.idempotency_key.to_string(),
    r.requested_at,
    EntityKind::Issue,
    &r.intent,
  )
}
#[async_trait::async_trait]
impl IssueExecutionStore for PostgresStore {
  async fn replay_issue_execution(
    &self,
    request: &ManagementMutation<RequestIssueExecution>,
  ) -> Result<Option<IssueExecutionOutcome>, StoreError> {
    let identity = identity(request)?;
    // Read-only replay uses the same canonical digest as the transactional reservation.
    let row: Option<(Vec<u8>, Json<serde_json::Value>)> = sqlx::query_as("SELECT request_digest,outcome FROM idempotency_records WHERE scope=$1 AND security_scope=$2 AND idempotency_key=$3")
      .bind(identity.kind.scope()).bind(request.audit().security_scope().as_str()).bind(&identity.key).fetch_optional(self.pool()).await.map_err(unavailable)?;
    row
      .map(|(digest, Json(value))| {
        if digest != identity.request_digest {
          return Err(conflict());
        }
        let mut outcome: IssueExecutionOutcome = decode_outcome(value)?;
        outcome.disposition = MutationDisposition::Replayed;
        Ok(outcome)
      })
      .transpose()
  }
  async fn request_issue_execution(
    &self,
    request: ManagementMutation<RequestIssueExecution>,
  ) -> Result<IssueExecutionOutcome, StoreError> {
    let identity = identity(&request)?;
    let (request, audit) = request.into_parts();
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => {
        let mut outcome: IssueExecutionOutcome = decode_outcome(value)?;
        outcome.disposition = MutationDisposition::Replayed;
        return Ok(outcome);
      }
      MutationStart::Fresh(tx) => tx,
    };
    let issue = crate::issue::lock(&mut tx, request.intent.project_id, request.intent.issue_id).await?;
    let outcome = request.reserve(&issue)?;
    sqlx::query("INSERT INTO issue_execution_requests(id,project_id,issue_id,requirements_version,security_scope,document) VALUES ($1,$2,$3,$4,$5,$6)")
      .bind(request.id.as_uuid()).bind(issue.project_id.as_uuid()).bind(issue.id.as_uuid()).bind(i64::try_from(issue.version.get()).map_err(|_| conflict())?)
      .bind(audit.security_scope().as_str()).bind(Json(&outcome.request)).execute(&mut *tx).await.map_err(unavailable)?;
    let metadata = json!({"project_id": issue.project_id, "issue_id": issue.id, "request_id": request.id, "requirements_version": issue.version});
    crate::mutation::commit(
      tx,
      &identity,
      MutationFacts::management(&audit, issue.id.to_string(), metadata.clone(), metadata),
      encode_outcome(&outcome)?,
    )
    .await?;
    Ok(outcome)
  }
  async fn issue_execution(
    &self,
    project: ProjectId,
    issue: IssueId,
    id: IssueExecutionRequestId,
  ) -> Result<IssueExecutionRequest, StoreError> {
    read(self.pool(), project, issue, id, None).await
  }
  async fn issue_execution_for_admission(
    &self,
    project: ProjectId,
    issue: IssueId,
    id: IssueExecutionRequestId,
    scope: &ManagementSecurityScope,
  ) -> Result<IssueExecutionRequest, StoreError> {
    read(self.pool(), project, issue, id, Some(scope)).await
  }
  async fn associate_issue_work(
    &self,
    request: ManagementMutation<AssociateIssueWork>,
  ) -> Result<IssueWorkAssociationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let reviewed = match request.request_id {
      Some(id) => Some(
        self
          .issue_execution_for_admission(request.project_id, request.issue_id, id, audit.security_scope())
          .await?,
      ),
      None => None,
    };
    let association = request.association(reviewed.as_ref())?;
    let identity = MutationIdentity::new_management(
      &audit,
      MutationKind::AssociateIssueWork,
      format!("issue-work.{}", association.work_id),
      request.admission.admitted_at,
      EntityKind::Issue,
      &association,
    )?;
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => {
        let mut o: IssueWorkAssociationOutcome = decode_outcome(value)?;
        o.disposition = MutationDisposition::Replayed;
        return Ok(o);
      }
      MutationStart::Fresh(tx) => tx,
    };
    crate::issue::lock(&mut tx, request.project_id, request.issue_id).await?;
    let authoritative: Option<(Json<octacity_server_factory::WorkEnvelope>,)> = sqlx::query_as("SELECT w.envelope FROM factory_work_envelopes w JOIN factory_runs r ON r.work_envelope_id=w.id WHERE w.project_id=$1 AND w.id=$2 AND r.id=$3 FOR KEY SHARE OF w,r")
      .bind(request.project_id.as_uuid()).bind(association.work_id.as_uuid()).bind(association.run_id.as_uuid()).fetch_optional(&mut *tx).await.map_err(unavailable)?;
    if authoritative.map(|v| v.0.0).as_ref() != Some(&request.admission.work) {
      return Err(conflict());
    }
    if let Some(id) = request.request_id {
      let Json(current): Json<IssueExecutionRequest> =
        sqlx::query_scalar("SELECT document FROM issue_execution_requests WHERE id=$1 FOR UPDATE")
          .bind(id.as_uuid())
          .fetch_one(&mut *tx)
          .await
          .map_err(unavailable)?;
      request.association(Some(&current))?;
    }
    let previous: Option<Json<IssueWorkAssociation>> =
      sqlx::query_scalar("SELECT document FROM issue_work_associations WHERE work_id=$1")
        .bind(association.work_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(unavailable)?;
    if let Some(Json(previous)) = previous {
      if previous != association {
        return Err(conflict());
      }
      return Ok(IssueWorkAssociationOutcome {
        disposition: MutationDisposition::Replayed,
        association: previous,
      });
    }
    sqlx::query("INSERT INTO issue_work_associations(work_id,run_id,project_id,issue_id,request_id,document) VALUES ($1,$2,$3,$4,$5,$6)")
      .bind(association.work_id.as_uuid()).bind(association.run_id.as_uuid()).bind(association.project_id.as_uuid()).bind(association.issue_id.as_uuid()).bind(association.request_id.map(|id|id.as_uuid())).bind(Json(&association)).execute(&mut *tx).await.map_err(|e| {
        if e.as_database_error().is_some_and(|e| e.is_unique_violation()) { conflict() } else { unavailable(e) }
      })?;
    if let Some(mut reviewed) = reviewed {
      reviewed.association = Some(association.clone());
      sqlx::query("UPDATE issue_execution_requests SET document=$2 WHERE id=$1")
        .bind(reviewed.id.as_uuid())
        .bind(Json(reviewed))
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?;
    }
    let metadata = json!({"project_id":request.project_id,"issue_id":request.issue_id,"work_id":association.work_id,"run_id":association.run_id,"request_id":association.request_id});
    let outcome = IssueWorkAssociationOutcome {
      disposition: MutationDisposition::Applied,
      association,
    };
    crate::mutation::commit(
      tx,
      &identity,
      MutationFacts::management(&audit, request.issue_id.to_string(), metadata.clone(), metadata),
      encode_outcome(&outcome)?,
    )
    .await?;
    Ok(outcome)
  }
  async fn issue_work_association(
    &self,
    project: ProjectId,
    work: WorkEnvelopeId,
  ) -> Result<Option<IssueWorkAssociation>, StoreError> {
    let document: Option<Json<IssueWorkAssociation>> =
      sqlx::query_scalar("SELECT document FROM issue_work_associations WHERE project_id=$1 AND work_id=$2")
        .bind(project.as_uuid())
        .bind(work.as_uuid())
        .fetch_optional(self.pool())
        .await
        .map_err(unavailable)?;
    Ok(document.map(|v| v.0))
  }
}
async fn read(
  pool: &sqlx::PgPool,
  project: ProjectId,
  issue: IssueId,
  id: IssueExecutionRequestId,
  scope: Option<&ManagementSecurityScope>,
) -> Result<IssueExecutionRequest, StoreError> {
  let document: Option<Json<IssueExecutionRequest>> = sqlx::query_scalar("SELECT document FROM issue_execution_requests WHERE project_id=$1 AND issue_id=$2 AND id=$3 AND ($4::text IS NULL OR security_scope=$4)")
    .bind(project.as_uuid()).bind(issue.as_uuid()).bind(id.as_uuid()).bind(scope.map(|s| s.as_str())).fetch_optional(pool).await.map_err(unavailable)?;
  document.map(|v| v.0).ok_or(StoreError::NotFound {
    entity: EntityKind::Issue,
  })
}
fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Issue,
  }
}
