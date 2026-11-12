//! Local classification shares the Issue revision and management receipt transaction.
use crate::{
  PostgresStore,
  database::unavailable,
  issues::issue,
  issues::issue_planning,
  issues::issue_workflow,
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
    let mut intent = json!({
      "provenance": provenance, "association": a, "priority": request.priority,
      "size_label": request.size_label, "managed_labels": request.managed_labels,
    });
    // Legacy command fingerprints/receipts retain their original bytes.
    if let Some(fields) = &request.fields {
      intent["fields"] = serde_json::to_value(fields).map_err(|_| StoreError::Unavailable)?;
    }
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
    let evaluated = evaluate(&mut tx, &mut request, audit.actor()).await;
    let a = &request.association;
    if let Some(authority) = &request.authority {
      if std::time::Instant::now() >= authority.deadline {
        return Err(StoreError::Unavailable);
      }
      crate::factory::execution::factory_run::verify_issue_classification_claim(&mut tx, a.run_id, authority.claim)
        .await?;
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
      fields: request
        .fields
        .as_ref()
        .map(|fields| fields.field_references())
        .unwrap_or_default(),
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
  actor: &AuditActor,
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
  // Native intake has no manual request. The retained association and exact
  // requirements baseline below provide the same local Issue ownership fence.
  if retained.as_deref() != Some(a) {
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
  if (request.priority.is_some()
    || request.size_label.is_some()
    || request
      .fields
      .as_ref()
      .is_some_and(|fields| !fields.field_references().is_empty()))
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
      Some(ReceiptField::Canonical(IssueClassificationFieldIdentity::Priority)),
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
       AND COALESCE(document #> '{content,priority}', 'null'::jsonb) IS DISTINCT FROM $4::jsonb)",
    )
    .bind(a.project_id.as_uuid())
    .bind(a.issue_id.as_uuid())
    .bind(from.get() as i64)
    .bind(Json(&baseline.content.priority))
    .fetch_one(&mut *connection)
    .await
    .map_err(unavailable)?;
    if changed {
      return Err(conflict());
    }
  }

  if let Some(update) = &request.fields {
    for field in update.field_references() {
      let prior = latest(
        connection,
        &current,
        Some(current.applicability_version),
        Some(ReceiptField::Canonical(field.identity())),
        None,
      )
      .await?;
      let from = prior.map_or(baseline.version, |receipt| receipt.version.max(baseline.version));
      let reviewed = issue::read(connection, a.project_id, a.issue_id, Some(from)).await?;
      let (path, expected) = classification_field_baseline(&mut baseline, &reviewed, &field)?;
      let changed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM issue_revisions WHERE project_id=$1 AND issue_id=$2 AND version>$3
         AND COALESCE(document #> $4::text[], 'null'::jsonb) IS DISTINCT FROM $5::jsonb)",
      )
      .bind(a.project_id.as_uuid())
      .bind(a.issue_id.as_uuid())
      .bind(from.get() as i64)
      .bind(&path)
      .bind(Json(expected))
      .fetch_one(&mut *connection)
      .await
      .map_err(unavailable)?;
      if changed {
        return Err(conflict());
      }
    }
  }

  let mut owned_label = None;
  if request.size_label.is_some() {
    let prior = latest(
      connection,
      &current,
      None,
      Some(ReceiptField::SizeLabel),
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
  let next = current.classified(&baseline, request, owned_label.as_deref(), &catalog, &policy, actor)?;
  let owned = request
    .size_label
    .as_ref()
    .filter(|label| !current.planning.labels.contains(*label) || owned_label.as_ref() == Some(*label))
    .cloned();
  Ok((current, next, admitted_at, owned))
}

// Rebase only the selected field on its last applicable applied assessment. The
// original reviewed Issue/applicability versions remain intact for attribution.
// Property identities are SQL parameters, never interpolated JSON paths.
fn classification_field_baseline<'a>(
  baseline: &mut IssueRevision,
  reviewed: &IssueRevision,
  field: &'a IssueClassificationFieldReference,
) -> Result<(Vec<&'a str>, serde_json::Value), StoreError> {
  Ok(match field {
    IssueClassificationFieldReference::Priority { .. } => {
      baseline.content.priority.clone_from(&reviewed.content.priority);
      (vec!["content", "priority"], json!(reviewed.content.priority))
    }
    IssueClassificationFieldReference::Component { .. } => {
      baseline.planning.component.clone_from(&reviewed.planning.component);
      (vec!["planning", "component"], json!(reviewed.planning.component))
    }
    IssueClassificationFieldReference::Type { .. } => {
      baseline.planning.issue_type.clone_from(&reviewed.planning.issue_type);
      (vec!["planning", "type"], json!(reviewed.planning.issue_type))
    }
    IssueClassificationFieldReference::Size => {
      baseline.planning.size = reviewed.planning.size;
      (vec!["planning", "size"], json!(reviewed.planning.size))
    }
    IssueClassificationFieldReference::Property { id, .. } => {
      match reviewed.planning.values.get(id) {
        Some(value) => {
          baseline.planning.values.insert(id.clone(), value.clone());
        }
        None => {
          baseline.planning.values.remove(id);
        }
      }
      (vec!["planning", "values", id], json!(reviewed.planning.values.get(id)))
    }
    IssueClassificationFieldReference::Dependencies => return Err(conflict()),
  })
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
pub(crate) struct ClassificationReceipt {
  admitted_at: i64,
  run_id: FactoryRunId,
  accepted_at: i64,
  record: FactoryDigest,
  version: IssueVersion,
  applicability: IssueVersion,
  owned_label: Option<String>,
  policy: FactoryKey,
  pub(crate) priority: Option<u8>,
  pub(crate) size_label: Option<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  fields: Vec<IssueClassificationFieldReference>,
}

impl ClassificationReceipt {
  fn order(&self) -> (i64, FactoryRunId, i64, FactoryDigest) {
    (self.admitted_at, self.run_id, self.accepted_at, self.record)
  }
}

enum ReceiptField<'a> {
  SizeLabel,
  Canonical(IssueClassificationFieldIdentity<'a>),
}

// Read original durable receipts. Canonical field identity excludes the catalog
// version: republishing definitions must not hide an earlier applied assessment.
// Legacy and typed priority effects address the same canonical field.
async fn latest(
  connection: &mut PgConnection,
  issue: &IssueRevision,
  applicability: Option<IssueVersion>,
  field: Option<ReceiptField<'_>>,
  policy: Option<&str>,
) -> Result<Option<ClassificationReceipt>, StoreError> {
  let (legacy, selected) = match field {
    None => (None, None),
    Some(ReceiptField::SizeLabel) => (Some("size_label"), None),
    Some(ReceiptField::Canonical(identity)) => {
      let legacy = (identity == IssueClassificationFieldIdentity::Priority).then_some("priority");
      (legacy, Some(Json(json!([identity]))))
    }
  };
  let row: Option<Json<ClassificationReceipt>> = sqlx::query_scalar(
    "SELECT outcome #> '{classification}' AS receipt FROM idempotency_records
     WHERE scope=$1 AND outcome #>> '{value,issue,project_id}'=$2 AND outcome #>> '{value,issue,id}'=$3
       AND ($4::bigint IS NULL OR (outcome #>> '{classification,applicability}')::bigint=$4)
       AND (($5::text IS NULL AND $7::jsonb IS NULL)
         OR ($5::text IS NOT NULL AND (outcome #> '{classification}')->>$5 IS NOT NULL)
         OR ($7::jsonb IS NOT NULL AND outcome #> '{classification,fields}' @> $7::jsonb))
       AND ($6::text IS NULL OR outcome #>> '{classification,policy}'=$6)
       AND (outcome #>> '{classification,priority}' IS NOT NULL OR outcome #>> '{classification,size_label}' IS NOT NULL
         OR COALESCE(jsonb_array_length(outcome #> '{classification,fields}'),0)>0)
     ORDER BY (outcome #>> '{classification,admitted_at}')::bigint DESC,
       outcome #>> '{classification,run_id}' DESC, (outcome #>> '{classification,accepted_at}')::bigint DESC,
       outcome #>> '{classification,record}' DESC LIMIT 1",
  )
  .bind(MutationKind::ApplyIssueClassification.scope())
  .bind(issue.project_id.to_string())
  .bind(issue.id.to_string())
  .bind(applicability.map(|v| v.get() as i64))
  .bind(legacy)
  .bind(policy)
  .bind(selected)
  .fetch_optional(connection)
  .await
  .map_err(unavailable)?;
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
