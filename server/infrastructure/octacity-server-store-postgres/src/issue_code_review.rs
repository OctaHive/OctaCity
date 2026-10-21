//! Optional review metadata uses the common scoped mutation transaction and existing Issue lock.
use crate::{
  PostgresStore,
  database::{number, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};
use octacity_server_domain::{EntityKind, IssueId, ProjectId, Timestamp};
use octacity_server_factory::FactoryDigest;
use octacity_server_store::*;
use sqlx::{PgConnection, types::Json};
#[async_trait::async_trait]
impl IssueCodeReviewStore for PostgresStore {
  async fn issue_code_review_replay(
    &self,
    scope: &ManagementSecurityScope,
    intent: &AttachIssueCodeReviewIntent,
    key: &IdempotencyKey,
  ) -> Result<Option<IssueCodeReviewOutcome>, StoreError> {
    self.issue(intent.project_id, intent.issue_id, None).await?;
    let identity = MutationIdentity::new_management_replay(
      scope,
      MutationKind::AttachIssueCodeReview,
      key.to_string(),
      Timestamp::from_unix_millis(0).map_err(|_| StoreError::Unavailable)?,
      EntityKind::Issue,
      intent,
    )?;
    crate::mutation::probe(self.pool(), &identity)
      .await?
      .map(attach_replay)
      .transpose()
  }
  async fn attach_issue_code_review(
    &self,
    request: ManagementMutation<AttachIssueCodeReview>,
  ) -> Result<IssueCodeReviewOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let intent = &request.intent;
    self.issue(intent.project_id, intent.issue_id, None).await?;
    let identity = MutationIdentity::new_management(
      &audit,
      MutationKind::AttachIssueCodeReview,
      request.idempotency_key.to_string(),
      request.attached_at,
      EntityKind::Issue,
      intent,
    )?;
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => return attach_replay(value),
      MutationStart::Fresh(tx) => tx,
    };
    let issue = crate::issue::lock(&mut tx, intent.project_id, intent.issue_id).await?;
    request.validate(&issue)?;
    let id = intent.selection.identity(intent.project_id, intent.issue_id);
    let existing = association(&mut tx, intent.project_id, intent.issue_id, audit.security_scope(), id).await?;
    let outcome = if let Some(association) = existing {
      if association.binding != intent.binding {
        return Err(conflict());
      }
      IssueCodeReviewOutcome {
        disposition: MutationDisposition::Replayed,
        association,
      }
    } else {
      let previous: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(sequence),0) FROM issue_code_review_associations WHERE project_id=$1 AND issue_id=$2 AND security_scope=$3").bind(intent.project_id.as_uuid()).bind(intent.issue_id.as_uuid()).bind(audit.security_scope().as_str()).fetch_one(&mut *tx).await.map_err(unavailable)?;
      let sequence = previous.checked_add(1).ok_or(StoreError::Unavailable)?;
      let association = IssueCodeReviewAssociation {
        id,
        sequence: sequence as u64,
        project_id: intent.project_id,
        issue_id: intent.issue_id,
        selection: intent.selection.clone(),
        binding: intent.binding.clone(),
        provenance: request.provenance,
        attached_at: request.attached_at,
      };
      sqlx::query("INSERT INTO issue_code_review_associations (project_id,issue_id,security_scope,id,sequence,document,repository_identity) VALUES ($1,$2,$3,$4,$5,$6,$7)").bind(intent.project_id.as_uuid()).bind(intent.issue_id.as_uuid()).bind(audit.security_scope().as_str()).bind(id.as_bytes().as_slice()).bind(sequence).bind(Json(&association)).bind(association.selection.repository_identity().as_bytes().as_slice()).execute(&mut *tx).await.map_err(unavailable)?;
      insert_observation(
        &mut tx,
        audit.security_scope(),
        &association,
        &IssueCodeReviewObservationRecord::new(id, 1, request.observation)?,
      )
      .await?;
      IssueCodeReviewOutcome {
        disposition: MutationDisposition::Applied,
        association,
      }
    };
    // Safe reference only: original provider title/author/URLs stay out of generic audit/outbox.
    let facts = serde_json::json!({"association":outcome.association.id});
    crate::mutation::commit(
      tx,
      &identity,
      MutationFacts::management(&audit, intent.issue_id.to_string(), facts.clone(), facts),
      encode_outcome(&outcome)?,
    )
    .await?;
    Ok(outcome)
  }
  async fn issue_code_review(
    &self,
    project: ProjectId,
    issue: IssueId,
    scope: &ManagementSecurityScope,
    id: FactoryDigest,
  ) -> Result<IssueCodeReviewAssociation, StoreError> {
    self.issue(project, issue, None).await?;
    association(
      &mut *self.pool().acquire().await.map_err(unavailable)?,
      project,
      issue,
      scope,
      id,
    )
    .await?
    .ok_or(StoreError::NotFound {
      entity: EntityKind::Issue,
    })
  }
  async fn issue_code_review_observation_replay(
    &self,
    scope: &ManagementSecurityScope,
    intent: &ObserveIssueCodeReviewIntent,
    key: &IdempotencyKey,
  ) -> Result<Option<IssueCodeReviewObservationOutcome>, StoreError> {
    self
      .issue_code_review(intent.project_id, intent.issue_id, scope, intent.association_id)
      .await?;
    let identity = MutationIdentity::new_management_replay(
      scope,
      MutationKind::ObserveIssueCodeReview,
      key.to_string(),
      Timestamp::from_unix_millis(0).map_err(|_| StoreError::Unavailable)?,
      EntityKind::Issue,
      intent,
    )?;
    crate::mutation::probe(self.pool(), &identity)
      .await?
      .map(observe_replay)
      .transpose()
  }
  async fn observe_issue_code_review(
    &self,
    request: ManagementMutation<ObserveIssueCodeReview>,
  ) -> Result<IssueCodeReviewObservationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let intent = &request.intent;
    self
      .issue_code_review(
        intent.project_id,
        intent.issue_id,
        audit.security_scope(),
        intent.association_id,
      )
      .await?;
    let identity = MutationIdentity::new_management(
      &audit,
      MutationKind::ObserveIssueCodeReview,
      request.idempotency_key.to_string(),
      request.recorded_at,
      EntityKind::Issue,
      intent,
    )?;
    let mut tx = match crate::mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => return observe_replay(value),
      MutationStart::Fresh(tx) => tx,
    };
    crate::issue::lock(&mut tx, intent.project_id, intent.issue_id).await?;
    let association = association(
      &mut tx,
      intent.project_id,
      intent.issue_id,
      audit.security_scope(),
      intent.association_id,
    )
    .await?
    .ok_or_else(conflict)?;
    request.validate(&association)?;
    let previous:i64=sqlx::query_scalar("SELECT COALESCE(MAX(sequence),0) FROM issue_code_review_observations WHERE project_id=$1 AND issue_id=$2 AND security_scope=$3 AND association_id=$4").bind(intent.project_id.as_uuid()).bind(intent.issue_id.as_uuid()).bind(audit.security_scope().as_str()).bind(intent.association_id.as_bytes().as_slice()).fetch_one(&mut *tx).await.map_err(unavailable)?;
    let record = IssueCodeReviewObservationRecord::new(
      intent.association_id,
      previous.checked_add(1).ok_or(StoreError::Unavailable)? as u64,
      request.observation,
    )?;
    let existing:Option<Json<IssueCodeReviewObservationRecord>>=sqlx::query_scalar("SELECT document FROM issue_code_review_observations WHERE project_id=$1 AND issue_id=$2 AND security_scope=$3 AND association_id=$4 AND id=$5").bind(intent.project_id.as_uuid()).bind(intent.issue_id.as_uuid()).bind(audit.security_scope().as_str()).bind(intent.association_id.as_bytes().as_slice()).bind(record.id.as_bytes().as_slice()).fetch_optional(&mut *tx).await.map_err(unavailable)?;
    let outcome = if let Some(Json(observation)) = existing {
      IssueCodeReviewObservationOutcome {
        disposition: MutationDisposition::Replayed,
        observation,
      }
    } else {
      insert_observation(&mut tx, audit.security_scope(), &association, &record).await?;
      IssueCodeReviewObservationOutcome {
        disposition: MutationDisposition::Applied,
        observation: record,
      }
    };
    let facts = serde_json::json!({"association":intent.association_id,"observation":outcome.observation.id});
    crate::mutation::commit(
      tx,
      &identity,
      MutationFacts::management(&audit, intent.issue_id.to_string(), facts.clone(), facts),
      encode_outcome(&outcome)?,
    )
    .await?;
    Ok(outcome)
  }
  async fn issue_code_reviews(&self, request: ReadIssueCodeReviews) -> Result<IssueCodeReviewStorePage, StoreError> {
    request.validate()?;
    self.issue(request.project_id, request.issue_id, None).await?;
    let repositories = repository_visibility(&request.visibility);
    let maximum = sqlx::query_scalar::<_,i64>("SELECT COALESCE(MAX(sequence),0) FROM issue_code_review_associations WHERE project_id=$1 AND issue_id=$2 AND security_scope=$3 AND ($4::bytea[] IS NULL OR repository_identity=ANY($4))").bind(request.project_id.as_uuid()).bind(request.issue_id.as_uuid()).bind(request.scope.as_str()).bind(&repositories).fetch_one(self.pool()).await.map_err(unavailable)? as u64;
    let boundary = request.retained_boundary(maximum)?;
    let rows:Vec<(Json<IssueCodeReviewAssociation>,Option<Json<IssueCodeReviewObservationRecord>>)>=sqlx::query_as("SELECT a.document,o.document FROM issue_code_review_associations a LEFT JOIN LATERAL (SELECT document FROM issue_code_review_observations WHERE project_id=a.project_id AND issue_id=a.issue_id AND security_scope=a.security_scope AND association_id=a.id ORDER BY observed_at DESC,sequence DESC LIMIT 1) o ON TRUE WHERE a.project_id=$1 AND a.issue_id=$2 AND a.security_scope=$3 AND ($7::bytea[] IS NULL OR a.repository_identity=ANY($7)) AND a.sequence<=$4 AND ($5::bigint IS NULL OR a.sequence>$5) ORDER BY a.sequence LIMIT $6").bind(request.project_id.as_uuid()).bind(request.issue_id.as_uuid()).bind(request.scope.as_str()).bind(page_number(boundary)?).bind(request.after.map(page_number).transpose()?).bind(i64::from(request.limit)+1).bind(&repositories).fetch_all(self.pool()).await.map_err(unavailable)?;
    let items = rows
      .into_iter()
      .map(|(Json(association), observation)| IssueCodeReviewRow {
        association,
        observation: observation.map(|Json(o)| o.observation),
      })
      .collect();
    Ok(finish_page(
      items,
      boundary,
      request.limit,
      |row: &IssueCodeReviewRow| row.association.sequence,
    ))
  }
  async fn issue_code_review_observations(
    &self,
    request: ReadIssueCodeReviews,
    id: FactoryDigest,
  ) -> Result<IssueCodeReviewStorePage<IssueCodeReviewObservationRecord>, StoreError> {
    request.validate()?;
    let selected = self
      .issue_code_review(request.project_id, request.issue_id, &request.scope, id)
      .await?;
    if !request.visibility.allows(&selected.selection.repository_identity()) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Issue,
      });
    }
    let maximum = sqlx::query_scalar::<_,i64>("SELECT COALESCE(MAX(sequence),0) FROM issue_code_review_observations WHERE project_id=$1 AND issue_id=$2 AND security_scope=$3 AND association_id=$4").bind(request.project_id.as_uuid()).bind(request.issue_id.as_uuid()).bind(request.scope.as_str()).bind(id.as_bytes().as_slice()).fetch_one(self.pool()).await.map_err(unavailable)? as u64;
    let boundary = request.retained_boundary(maximum)?;
    let rows:Vec<Json<IssueCodeReviewObservationRecord>>=sqlx::query_scalar("SELECT document FROM issue_code_review_observations WHERE project_id=$1 AND issue_id=$2 AND security_scope=$3 AND association_id=$4 AND sequence<=$5 AND ($6::bigint IS NULL OR sequence>$6) ORDER BY sequence LIMIT $7").bind(request.project_id.as_uuid()).bind(request.issue_id.as_uuid()).bind(request.scope.as_str()).bind(id.as_bytes().as_slice()).bind(page_number(boundary)?).bind(request.after.map(page_number).transpose()?).bind(i64::from(request.limit)+1).fetch_all(self.pool()).await.map_err(unavailable)?;
    Ok(finish_page(
      rows.into_iter().map(|Json(row)| row).collect(),
      boundary,
      request.limit,
      |row: &IssueCodeReviewObservationRecord| row.sequence,
    ))
  }
}
async fn association(
  connection: &mut PgConnection,
  project: ProjectId,
  issue: IssueId,
  scope: &ManagementSecurityScope,
  id: FactoryDigest,
) -> Result<Option<IssueCodeReviewAssociation>, StoreError> {
  let row:Option<Json<IssueCodeReviewAssociation>>=sqlx::query_scalar("SELECT document FROM issue_code_review_associations WHERE project_id=$1 AND issue_id=$2 AND security_scope=$3 AND id=$4").bind(project.as_uuid()).bind(issue.as_uuid()).bind(scope.as_str()).bind(id.as_bytes().as_slice()).fetch_optional(connection).await.map_err(unavailable)?;
  Ok(row.map(|Json(row)| row))
}
async fn insert_observation(
  connection: &mut PgConnection,
  scope: &ManagementSecurityScope,
  association: &IssueCodeReviewAssociation,
  record: &IssueCodeReviewObservationRecord,
) -> Result<(), StoreError> {
  sqlx::query("INSERT INTO issue_code_review_observations (project_id,issue_id,security_scope,association_id,id,sequence,observed_at,document) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)").bind(association.project_id.as_uuid()).bind(association.issue_id.as_uuid()).bind(scope.as_str()).bind(association.id.as_bytes().as_slice()).bind(record.id.as_bytes().as_slice()).bind(page_number(record.sequence)?).bind(record.observation.observed_at.unix_millis()).bind(Json(record)).execute(connection).await.map_err(unavailable)?;
  Ok(())
}
fn attach_replay(value: serde_json::Value) -> Result<IssueCodeReviewOutcome, StoreError> {
  let mut outcome: IssueCodeReviewOutcome = decode_outcome(value)?;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(outcome)
}
fn observe_replay(value: serde_json::Value) -> Result<IssueCodeReviewObservationOutcome, StoreError> {
  let mut outcome: IssueCodeReviewObservationOutcome = decode_outcome(value)?;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(outcome)
}
fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Issue,
  }
}
fn page_number(value: u64) -> Result<i64, StoreError> {
  number(value, StoreOperation::AttachIssueCodeReview)
}
fn finish_page<T>(
  mut items: Vec<T>,
  boundary: u64,
  limit: u16,
  sequence: impl Fn(&T) -> u64,
) -> IssueCodeReviewStorePage<T> {
  let more = items.len() > usize::from(limit);
  items.truncate(usize::from(limit));
  let next = more.then(|| sequence(items.last().unwrap()));
  IssueCodeReviewStorePage { items, boundary, next }
}

fn repository_visibility(visibility: &IssueCodeReviewRepositoryVisibility) -> Option<Vec<Vec<u8>>> {
  match visibility.view() {
    ReadVisibilityView::All => None,
    ReadVisibilityView::None => Some(vec![]),
    ReadVisibilityView::Restricted(ids) => Some(ids.iter().map(|id| id.as_bytes().to_vec()).collect()),
  }
}
