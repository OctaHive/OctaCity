//! Local classification shares the Issue revision and management receipt transaction.
use crate::{
  PostgresStore,
  database::unavailable,
  issue, issue_planning, issue_workflow,
  mutation::{self, MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};
use octacity_server_domain::{EntityKind, IssueId, IssueVersion, ProjectId, Timestamp};
use octacity_server_factory::{FactoryDigest, FactoryKey, FactoryRunId, WorkEnvelopeId};
use octacity_server_store::*;
use serde_json::json;
use sqlx::{PgConnection, types::Json};

#[async_trait::async_trait]
impl IssueClassificationStore for PostgresStore {
  async fn apply_issue_classification(
    &self,
    request: ManagementMutation<ApplyIssueClassification>,
  ) -> Result<IssueMutationOutcome, StoreError> {
    let (mut request, audit) = request.into_parts();
    let a = &request.association;
    let provenance = serde_json::to_value(ClassificationProvenance {
      project_id: a.project_id,
      issue_id: a.issue_id,
      work_id: a.work_id,
      run_id: a.run_id,
      record: request.record,
      operation_id: request.operation_id,
      policy: &request.policy,
      accepted_at: request.accepted_at,
    })
    .map_err(|_| StoreError::Unavailable)?;
    let intent = json!({
      "provenance": provenance, "association": a, "priority": request.priority,
      "size_label": request.size_label, "managed_labels": request.managed_labels,
    });
    let mut identity = MutationIdentity::new_management(
      &audit,
      MutationKind::ApplyIssueClassification,
      issue_status_operation_key(request.operation_id).to_string(),
      request.applied_at,
      EntityKind::Issue,
      &intent,
    )?;
    let mut tx = match mutation::begin(self.pool(), &identity).await? {
      MutationStart::Replay(value) => {
        if value["classification_rejected"] == true {
          return Err(conflict());
        }
        let mut outcome: IssueMutationOutcome = decode_outcome(value)?;
        outcome.disposition = MutationDisposition::Replayed;
        return Ok(outcome);
      }
      MutationStart::Fresh(tx) => tx,
    };
    let evaluated = evaluate(&mut tx, &mut request).await;
    let a = &request.association;
    if let Some(authority) = &request.authority {
      if std::time::Instant::now() >= authority.deadline {
        return Err(StoreError::Unavailable);
      }
      crate::factory_run::verify_issue_classification_claim(&mut tx, a.run_id, authority.claim).await?;
      identity.deadline = Some(authority.deadline);
    }
    let (current, next, admitted_at, owned) = match evaluated {
      Ok(value) => value,
      Err(error @ StoreError::Conflict { .. }) => {
        mutation::complete_record(
          tx,
          &identity,
          json!({
            "classification_rejected": true, "provenance": provenance,
          }),
        )
        .await?;
        return Err(error);
      }
      Err(error) => return Err(error),
    };
    if next.version != current.version {
      issue::persist_revision(&mut tx, &next, None).await?;
    }
    let outcome = IssueMutationOutcome {
      disposition: MutationDisposition::Applied,
      issue: next,
    };
    let mut receipt = encode_outcome(&outcome)?;
    receipt["provenance"] = provenance.clone();
    receipt["classification"] = serde_json::to_value(ClassificationReceipt {
      admitted_at,
      run_id: a.run_id,
      accepted_at: request.accepted_at.unix_millis(),
      record: request.record,
      priority: request.priority,
      size_label: request.size_label,
      owned_label: owned,
      version: outcome.issue.version,
      applicability: current.applicability_version,
      policy: request.policy,
    })
    .map_err(|_| StoreError::Unavailable)?;
    mutation::commit(
      tx,
      &identity,
      MutationFacts::management(&audit, a.issue_id.to_string(), provenance.clone(), provenance),
      receipt,
    )
    .await?;
    Ok(outcome)
  }
}

fn conflict() -> StoreError {
  StoreError::Conflict {
    entity: EntityKind::Issue,
  }
}

// All reads and the ensuing revision share the Project and Issue fences.
async fn evaluate(
  connection: &mut PgConnection,
  request: &mut ApplyIssueClassification,
) -> Result<(IssueRevision, IssueRevision, i64, Option<String>), StoreError> {
  let a = &request.association;
  issue_workflow::lock_project(connection, a.project_id).await?;
  let current = issue::lock(connection, a.project_id, a.issue_id).await?;
  let retained: Option<Json<IssueWorkAssociation>> = sqlx::query_scalar(
    "SELECT document FROM issue_work_associations WHERE project_id=$1 AND issue_id=$2 AND work_id=$3",
  )
  .bind(a.project_id.as_uuid())
  .bind(a.issue_id.as_uuid())
  .bind(a.work_id.as_uuid())
  .fetch_optional(&mut *connection)
  .await
  .map_err(unavailable)?;
  if retained.as_deref() != Some(a) || a.request_id.is_none() {
    return Err(conflict());
  }
  let mut baseline = issue::read(
    connection,
    a.project_id,
    a.issue_id,
    Some(a.requirements_version.ok_or_else(conflict)?),
  )
  .await?;
  if baseline.applicability_version != current.applicability_version {
    return Err(conflict());
  }
  let admitted_at: i64 = sqlx::query_scalar(
    "SELECT (EXTRACT(EPOCH FROM admitted_at) * 1000)::bigint FROM factory_runs WHERE id=$1 AND project_id=$2 AND work_envelope_id=$3",
  ).bind(a.run_id.as_uuid()).bind(a.project_id.as_uuid()).bind(a.work_id.as_uuid())
    .fetch_optional(&mut *connection).await.map_err(unavailable)?.ok_or_else(conflict)?;
  let order = (admitted_at, a.run_id, request.accepted_at.unix_millis(), request.record);
  if (request.priority.is_some() || request.size_label.is_some())
    && latest(connection, &current, Some(current.applicability_version), None, None)
      .await?
      .is_some_and(|prior| prior.order() >= order)
  {
    return Err(conflict());
  }

  if request.priority.is_some() {
    let prior = latest(
      connection,
      &current,
      Some(current.applicability_version),
      Some("priority"),
      None,
    )
    .await?;
    let from = prior.map_or(baseline.version, |receipt| receipt.version.max(baseline.version));
    baseline.content.priority = issue::read(connection, a.project_id, a.issue_id, Some(from))
      .await?
      .content
      .priority;
    let changed: bool = sqlx::query_scalar(
      "SELECT EXISTS(SELECT 1 FROM issue_revisions WHERE project_id=$1 AND issue_id=$2 AND version>$3
       AND document #>> '{content,priority}' IS DISTINCT FROM $4)",
    )
    .bind(a.project_id.as_uuid())
    .bind(a.issue_id.as_uuid())
    .bind(from.get() as i64)
    .bind(baseline.content.priority.map(|p| p.to_string()))
    .fetch_one(&mut *connection)
    .await
    .map_err(unavailable)?;
    if changed {
      return Err(conflict());
    }
  }

  let mut owned_label = None;
  if request.size_label.is_some() {
    let prior = latest(
      connection,
      &current,
      None,
      Some("size_label"),
      Some(request.policy.as_str()),
    )
    .await?;
    let from = prior
      .as_ref()
      .filter(|p| p.applicability == current.applicability_version)
      .map_or(baseline.version, |p| p.version.max(baseline.version));
    baseline.planning.labels = issue::read(connection, a.project_id, a.issue_id, Some(from))
      .await?
      .planning
      .labels;
    if let Some((previous, version)) = prior.and_then(|p| p.owned_label.map(|label| (label, p.version)))
      && labels_match_since(
        connection,
        &current,
        version,
        std::slice::from_ref(&previous),
        std::slice::from_ref(&previous),
      )
      .await?
    {
      request.managed_labels.insert(previous.clone());
      owned_label = Some(previous);
    }
    let managed: Vec<_> = request.managed_labels.iter().cloned().collect();
    let expected: Vec<_> = baseline
      .planning
      .labels
      .intersection(&request.managed_labels)
      .cloned()
      .collect();
    if !labels_match_since(connection, &current, from, &managed, &expected).await? {
      return Err(conflict());
    }
  }

  let catalog = issue_planning::read(connection, a.project_id, None).await?;
  let policy = issue::field_policy(connection, a.project_id, a.issue_id).await?;
  let next = current.classified(&baseline, request, owned_label.as_deref(), &catalog, &policy)?;
  let owned = request
    .size_label
    .as_ref()
    .filter(|label| !current.planning.labels.contains(*label) || owned_label.as_ref() == Some(*label))
    .cloned();
  Ok((current, next, admitted_at, owned))
}

// One scope for successful and conflicting receipts. Older successful receipts
// retain their scope in the encoded Issue mutation outcome.
#[derive(serde::Serialize)]
struct ClassificationProvenance<'a> {
  project_id: ProjectId,
  issue_id: IssueId,
  work_id: WorkEnvelopeId,
  run_id: FactoryRunId,
  record: FactoryDigest,
  operation_id: FactoryDigest,
  policy: &'a FactoryKey,
  accepted_at: Timestamp,
}

/// Durable committed classification shared by ordering and Issue card reads.
#[derive(serde::Deserialize, serde::Serialize)]
pub(super) struct ClassificationReceipt {
  admitted_at: i64,
  run_id: FactoryRunId,
  accepted_at: i64,
  record: FactoryDigest,
  version: IssueVersion,
  applicability: IssueVersion,
  owned_label: Option<String>,
  policy: FactoryKey,
  pub(super) priority: Option<u8>,
  pub(super) size_label: Option<String>,
}

impl ClassificationReceipt {
  fn order(&self) -> (i64, FactoryRunId, i64, FactoryDigest) {
    (self.admitted_at, self.run_id, self.accepted_at, self.record)
  }
}

// Read the original durable management receipts using the Issue revision index.
async fn latest(
  connection: &mut PgConnection,
  issue: &IssueRevision,
  applicability: Option<IssueVersion>,
  field: Option<&str>,
  policy: Option<&str>,
) -> Result<Option<ClassificationReceipt>, StoreError> {
  let row: Option<Json<ClassificationReceipt>> = sqlx::query_scalar(
    "SELECT outcome #> '{classification}' AS receipt FROM idempotency_records
     WHERE scope=$1 AND outcome #>> '{value,issue,project_id}'=$2 AND outcome #>> '{value,issue,id}'=$3
       AND ($4::bigint IS NULL OR (outcome #>> '{classification,applicability}')::bigint=$4)
       AND ($5::text IS NULL OR (outcome #> '{classification}')->>$5 IS NOT NULL)
       AND ($6::text IS NULL OR outcome #>> '{classification,policy}'=$6)
       AND (outcome #>> '{classification,priority}' IS NOT NULL OR outcome #>> '{classification,size_label}' IS NOT NULL)
     ORDER BY (outcome #>> '{classification,admitted_at}')::bigint DESC,
       outcome #>> '{classification,run_id}' DESC, (outcome #>> '{classification,accepted_at}')::bigint DESC,
       outcome #>> '{classification,record}' DESC LIMIT 1",
  ).bind(MutationKind::ApplyIssueClassification.scope())
    .bind(issue.project_id.to_string()).bind(issue.id.to_string())
    .bind(applicability.map(|v| v.get() as i64)).bind(field).bind(policy)
    .fetch_optional(connection).await.map_err(unavailable)?;
  Ok(row.map(|Json(receipt)| receipt))
}

async fn labels_match_since(
  connection: &mut PgConnection,
  issue: &IssueRevision,
  version: IssueVersion,
  managed: &[String],
  expected: &[String],
) -> Result<bool, StoreError> {
  sqlx::query_scalar(
    "SELECT NOT EXISTS(
      SELECT 1 FROM issue_revisions
      WHERE project_id=$1 AND issue_id=$2 AND version>$3
      AND EXISTS(SELECT 1 FROM unnest($4::text[]) AS managed(label)
        WHERE COALESCE((document #> '{planning,labels}') ? label, FALSE) <> (label=ANY($5::text[])))
    )",
  )
  .bind(issue.project_id.as_uuid())
  .bind(issue.id.as_uuid())
  .bind(version.get() as i64)
  .bind(managed)
  .bind(expected)
  .fetch_one(connection)
  .await
  .map_err(unavailable)
}
