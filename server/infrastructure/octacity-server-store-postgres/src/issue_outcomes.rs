//! Ordered occurrence metadata and exact accepted result reads.
use crate::{PostgresStore, database::unavailable};
use octacity_server_domain::Timestamp;
use octacity_server_factory::{
  FactoryDigest, FactoryRunId, FlowDefinitionId, FlowIssuePresentation, NodeAttemptId, WorkArtifacts, WorkEnvelopeId,
};
use octacity_server_store::{
  FactoryRunListVisibility, FactoryRunPagePosition, IssueOutcomeCandidate, IssueOutcomePosition, IssueOutcomeRecord,
  IssueOutcomeStore, IssueRevision, MAX_ISSUE_OUTCOME_CARDS, ReadVisibilityView, StoreError,
};
use sqlx::types::Json;
use uuid::Uuid;

#[derive(sqlx::FromRow)]
struct OutcomeRow {
  work_id: Uuid,
  run_id: Uuid,
  admitted_at: i64,
  node_attempt_id: Uuid,
  completed_at: i64,
  flow_id: Uuid,
  node_key: String,
  outcome: String,
  presentation: Json<FlowIssuePresentation>,
}

#[async_trait::async_trait]
impl IssueOutcomeStore for PostgresStore {
  async fn issue_outcome_candidates(
    &self,
    issue: &IssueRevision,
    visibility: &FactoryRunListVisibility,
    after: Option<IssueOutcomePosition>,
  ) -> Result<Vec<IssueOutcomeCandidate>, StoreError> {
    if !issue.content_available {
      return Ok(Vec::new());
    }
    let runs = match visibility.view() {
      ReadVisibilityView::None => return Ok(Vec::new()),
      ReadVisibilityView::All => None,
      ReadVisibilityView::Restricted(ids) => Some(ids.iter().map(|id| id.as_uuid()).collect::<Vec<_>>()),
    };
    let digest = |text: &str| FactoryDigest::content_sha256(text.as_bytes()).to_string();
    sqlx::query_as::<_, OutcomeRow>(include_str!("issue_outcomes.sql"))
      .bind(issue.project_id.as_uuid())
      .bind(issue.id.as_uuid())
      .bind(i64::try_from(issue.version.get()).map_err(|_| StoreError::Unavailable)?)
      .bind(digest(&issue.content.title))
      .bind(digest(&issue.content.description))
      .bind(digest(issue.content.acceptance.as_deref().unwrap_or_default()))
      .bind(runs)
      .bind(after.map(|p| p.run.admitted_at.unix_millis()))
      .bind(after.map(|p| p.run.run_id.as_uuid()))
      .bind(after.map(|p| p.completed_at.unix_millis()))
      .bind(after.map(|p| p.attempt.as_uuid()))
      .bind(i64::from(MAX_ISSUE_OUTCOME_CARDS))
      .fetch_all(self.pool())
      .await
      .map_err(unavailable)?
      .into_iter()
      .map(OutcomeRow::candidate)
      .collect()
  }

  async fn issue_outcome_record(
    &self,
    issue: &IssueRevision,
    candidate: &IssueOutcomeCandidate,
  ) -> Result<Option<IssueOutcomeRecord>, StoreError> {
    let row: Option<(Json<serde_json::Value>, Json<WorkArtifacts>)> = sqlx::query_as(
      "SELECT convert_from(record.document, 'UTF8')::jsonb #> '{payload,value}', work.envelope->'artifacts' \
       FROM issue_work_associations a JOIN factory_runs run ON run.id=a.run_id AND run.project_id=a.project_id \
       JOIN factory_work_envelopes work ON work.id=run.work_envelope_id AND work.id=a.work_id \
       JOIN factory_flow_records record ON record.run_id=run.id \
       JOIN issues issue ON issue.project_id=a.project_id AND issue.id=a.issue_id \
       WHERE issue.project_id=$1 AND issue.id=$2 AND issue.current_version=$3 AND run.visible \
         AND run.id=$4 AND work.id=$5 AND record.node_attempt_id=$6",
    )
    .bind(issue.project_id.as_uuid())
    .bind(issue.id.as_uuid())
    .bind(i64::try_from(issue.version.get()).map_err(|_| StoreError::Unavailable)?)
    .bind(candidate.position.run.run_id.as_uuid())
    .bind(candidate.work_id.as_uuid())
    .bind(candidate.position.attempt.as_uuid())
    .fetch_optional(self.pool())
    .await
    .map_err(unavailable)?;
    Ok(row.map(|(Json(value), Json(inputs))| IssueOutcomeRecord { value, inputs }))
  }
}

impl OutcomeRow {
  fn candidate(self) -> Result<IssueOutcomeCandidate, StoreError> {
    let presentation = self.presentation.0;
    let slot = presentation.slot(
      FlowDefinitionId::from_uuid(self.flow_id).map_err(|_| StoreError::Unavailable)?,
      &self.node_key.parse().map_err(|_| StoreError::Unavailable)?,
      &self.outcome.parse().map_err(|_| StoreError::Unavailable)?,
    );
    Ok(IssueOutcomeCandidate {
      slot,
      presentation,
      work_id: WorkEnvelopeId::from_uuid(self.work_id).map_err(|_| StoreError::Unavailable)?,
      position: IssueOutcomePosition {
        run: FactoryRunPagePosition {
          admitted_at: Timestamp::from_unix_millis(self.admitted_at).map_err(|_| StoreError::Unavailable)?,
          run_id: FactoryRunId::from_uuid(self.run_id).map_err(|_| StoreError::Unavailable)?,
        },
        completed_at: Timestamp::from_unix_millis(self.completed_at).map_err(|_| StoreError::Unavailable)?,
        attempt: NodeAttemptId::from_uuid(self.node_attempt_id).map_err(|_| StoreError::Unavailable)?,
      },
    })
  }
}
