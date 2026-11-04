//! Ordered occurrence metadata and exact accepted result reads.
use crate::{PostgresStore, database::unavailable};
use octacity_server_domain::Timestamp;
use octacity_server_factory::{
  FactoryDigest, FactoryRunId, FlowDefinitionId, FlowIssuePresentation, NodeAttemptId, WorkArtifacts, WorkEnvelopeId,
};
use octacity_server_store::{
  FactoryRunListVisibility, FactoryRunPagePosition, IssueOutcomeAssignedLabel, IssueOutcomeAssignedPriority,
  IssueOutcomeCandidate, IssueOutcomeClassification, IssueOutcomePosition, IssueOutcomeRecord, IssueOutcomeStore,
  IssuePropertyStore, IssueRevision, MAX_ISSUE_OUTCOME_CARDS, ReadVisibilityView, StoreError,
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

#[derive(sqlx::FromRow)]
struct RecordRow {
  value: Json<serde_json::Value>,
  artifacts: Json<WorkArtifacts>,
  classification: Option<Json<crate::issues::issue_classification::ClassificationReceipt>>,
  rejected: Option<bool>,
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
    let row = sqlx::query_as::<_, RecordRow>(include_str!("issue_outcome_record.sql"))
      .bind(issue.project_id.as_uuid())
      .bind(issue.id.as_uuid())
      .bind(i64::try_from(issue.version.get()).map_err(|_| StoreError::Unavailable)?)
      .bind(candidate.position.run.run_id.as_uuid())
      .bind(candidate.work_id.as_uuid())
      .bind(candidate.position.attempt.as_uuid())
      .bind(matches!(candidate.presentation, FlowIssuePresentation::TriageV1 { .. }))
      .fetch_optional(self.pool())
      .await
      .map_err(unavailable)?;
    let Some(row) = row else { return Ok(None) };
    let classification = match row.classification {
      Some(Json(applied)) => {
        let size_label = if let Some(label) = applied.size_label {
          let catalog = self.issue_properties(issue.project_id, None).await?;
          let current = issue.planning.labels.contains(&label);
          let name = catalog
            .definition
            .labels
            .into_iter()
            .find(|choice| choice.id == label)
            .map_or(label, |choice| choice.name);
          Some(IssueOutcomeAssignedLabel { name, current })
        } else {
          None
        };
        IssueOutcomeClassification::Applied {
          priority: applied.priority.map(|value| IssueOutcomeAssignedPriority {
            value,
            current: issue.content.priority == Some(value),
          }),
          size_label,
        }
      }
      None if row.rejected == Some(true) => IssueOutcomeClassification::Conflict,
      None => IssueOutcomeClassification::Recommended,
    };
    Ok(Some(IssueOutcomeRecord {
      value: row.value.0,
      inputs: row.artifacts.0,
      classification,
    }))
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
