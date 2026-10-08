use std::collections::BTreeMap;

use octacity_server_domain::Timestamp;
use octacity_server_factory::{
  Assessment, ChangeSet, ContextManifest, Decision, DecisionSignalReceipt, DecisionSignalRequest, DeliveryAttempt,
  Escalation, EvaluationPlan, EvidenceManifest, FactoryContextReference, FactoryDigest, FactoryRunId, MacroCall,
  MacroCallCompletion, ReportingAttempt, StageAttempt, StageAttemptCompletion, StageHandoff,
};
use octacity_server_store::{FactoryBuildLink, FactoryBuildObservationRecord, FactoryRunHistoryAppend, StoreError};
use serde::Serialize;
use sqlx::{Postgres, Transaction, types::Json};

use crate::{database::unavailable, factory_run::insert_artifact_reference};

pub(crate) async fn append_history(
  transaction: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
  recorded_at: Timestamp,
  append: &FactoryRunHistoryAppend,
) -> Result<(), StoreError> {
  if append.stage_attempts.iter().any(|record| record.run_id() != run_id)
    || append
      .stage_attempt_completions
      .iter()
      .any(|record| record.run_id() != run_id)
    || append.macro_calls.iter().any(|record| record.run_id() != run_id)
    || append.signal_requests.iter().any(|record| record.run_id() != run_id)
    || append.signal_receipts.iter().any(|record| record.run_id() != run_id)
    || append.linked_builds.iter().any(|record| record.run_id != run_id)
    || append.build_observations.iter().any(|record| record.run_id != run_id)
    || append.escalations.iter().any(|record| record.run_id() != run_id)
    || append.reporting_attempts.iter().any(|record| record.run_id() != run_id)
  {
    return Err(StoreError::InvalidInput {
      operation: octacity_server_store::StoreOperation::CommitFactoryRunTransition,
      source: octacity_server_store::StoreInputError::InvalidFactoryRunTransition,
    });
  }
  for record in &append.stage_attempts {
    insert_stage_attempt(transaction, recorded_at, record).await?;
  }
  for record in &append.stage_attempt_completions {
    insert_stage_completion(transaction, recorded_at, record).await?;
  }
  for record in &append.stage_handoffs {
    insert_stage_handoff(transaction, run_id, recorded_at, record).await?;
  }
  for record in &append.context_manifests {
    let stage_attempt_id = append
      .macro_calls
      .iter()
      .find(|call| call.context_manifest_id() == record.id())
      .map(MacroCall::stage_attempt_id)
      .ok_or(StoreError::Unavailable)?;
    insert_context_manifest(transaction, run_id, stage_attempt_id, recorded_at, record).await?;
  }
  let mut calls = append.macro_calls.iter().collect::<Vec<_>>();
  calls.sort_by_key(|record| (record.depth(), record.id()));
  for record in calls {
    insert_call(transaction, recorded_at, record).await?;
  }
  for record in &append.macro_call_completions {
    insert_call_completion(transaction, run_id, recorded_at, record).await?;
  }
  for record in &append.signal_requests {
    insert_signal_request(transaction, recorded_at, record).await?;
  }
  for record in &append.signal_receipts {
    insert_signal_receipt(transaction, recorded_at, record).await?;
  }
  for record in &append.linked_builds {
    insert_build_link(transaction, recorded_at, record).await?;
  }
  for record in &append.build_observations {
    insert_build_observation(transaction, record).await?;
  }
  for record in &append.candidates {
    insert_changeset(transaction, run_id, recorded_at, record).await?;
  }
  for record in &append.evidence {
    insert_evidence(transaction, run_id, recorded_at, record).await?;
  }
  for record in &append.evaluation_plans {
    insert_plan(transaction, run_id, recorded_at, record).await?;
  }
  for record in &append.assessments {
    insert_assessment(transaction, run_id, recorded_at, record).await?;
  }
  for record in &append.decisions {
    insert_decision(transaction, run_id, recorded_at, record).await?;
  }
  for record in &append.escalations {
    insert_escalation(transaction, recorded_at, record).await?;
  }
  for record in &append.delivery_attempts {
    insert_delivery(transaction, run_id, recorded_at, record).await?;
  }
  for record in &append.reporting_attempts {
    insert_reporting(transaction, recorded_at, record).await?;
  }
  Ok(())
}

async fn insert_stage_attempt(
  tx: &mut Transaction<'_, Postgres>,
  recorded_at: Timestamp,
  record: &StageAttempt,
) -> Result<(), StoreError> {
  let target_digest = FactoryDigest::sha256(
    "octacity.factory.stage-target.v1",
    &[record.target().canonical_key().as_bytes()],
  );
  sqlx::query("INSERT INTO factory_stage_attempts (id, run_id, attempt_number, stage_kind, target_digest, input_digest, stage_attempt, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, to_timestamp($8::double precision / 1000.0))")
    .bind(record.id().as_uuid()).bind(record.run_id().as_uuid()).bind(number(record.number().get())?)
    .bind(record.kind().as_str()).bind(target_digest.as_bytes().as_slice()).bind(record.input_digest().as_bytes().as_slice())
    .bind(Json(record)).bind(recorded_at.unix_millis()).execute(&mut **tx).await.map_err(unavailable)?;
  Ok(())
}

async fn insert_stage_completion(
  tx: &mut Transaction<'_, Postgres>,
  recorded_at: Timestamp,
  record: &StageAttemptCompletion,
) -> Result<(), StoreError> {
  sqlx::query("INSERT INTO factory_stage_attempt_completions (id, stage_attempt_id, run_id, outcome, output_digest, completion, completed_at) VALUES ($1, $2, $3, $4, $5, $6, to_timestamp($7::double precision / 1000.0))")
    .bind(record.id().as_bytes().as_slice()).bind(record.stage_attempt_id().as_uuid()).bind(record.run_id().as_uuid())
    .bind(stage_outcome(record.outcome())).bind(document_digest("octacity.factory.stage-completion-document.v1", record)?.as_bytes().as_slice())
    .bind(Json(record)).bind(recorded_at.unix_millis()).execute(&mut **tx).await.map_err(unavailable)?;
  Ok(())
}

async fn insert_stage_handoff(
  tx: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
  recorded_at: Timestamp,
  record: &StageHandoff,
) -> Result<(), StoreError> {
  let digest = FactoryDigest::content_sha256(&record.canonical_bytes().map_err(|_| StoreError::Unavailable)?);
  let inserted = sqlx::query(
    "INSERT INTO factory_stage_handoffs (id, run_id, stage_attempt_id, content_digest, handoff, created_at) \
     SELECT $1, $2, $3, $4, $5, to_timestamp($6::double precision / 1000.0) \
     FROM factory_stage_attempts WHERE id = $3 AND run_id = $2",
  )
  .bind(record.id().as_uuid())
  .bind(run_id.as_uuid())
  .bind(record.stage_attempt_id().as_uuid())
  .bind(digest.as_bytes().as_slice())
  .bind(Json(record))
  .bind(recorded_at.unix_millis())
  .execute(&mut **tx)
  .await
  .map_err(unavailable)?;
  if inserted.rows_affected() != 1 {
    return Err(StoreError::Unavailable);
  }
  for artifact in record.artifacts() {
    insert_shared_artifact_reference(
      tx,
      run_id,
      artifact.artifact_id(),
      octacity_server_store::FactoryArtifactRole::StageHandoff,
      artifact.content_digest(),
      recorded_at,
    )
    .await?;
  }
  Ok(())
}

async fn insert_context_manifest(
  tx: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
  stage_attempt_id: octacity_server_factory::StageAttemptId,
  recorded_at: Timestamp,
  record: &ContextManifest,
) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO factory_context_manifests \
     (id, run_id, stage_attempt_id, content_digest, policy_digest, manifest, created_at) \
     VALUES ($1, $2, $3, $4, $5, $6, to_timestamp($7::double precision / 1000.0))",
  )
  .bind(record.id().as_uuid())
  .bind(run_id.as_uuid())
  .bind(stage_attempt_id.as_uuid())
  .bind(
    record
      .digest()
      .map_err(|_| StoreError::Unavailable)?
      .as_bytes()
      .as_slice(),
  )
  .bind(record.construction_policy_digest().as_bytes().as_slice())
  .bind(Json(record))
  .bind(recorded_at.unix_millis())
  .execute(&mut **tx)
  .await
  .map_err(unavailable)?;
  for entry in record.entries() {
    let artifact = match entry.source() {
      FactoryContextReference::Artifact(artifact) => Some(artifact),
      FactoryContextReference::RepositoryFragment(fragment) => Some(fragment.artifact()),
      FactoryContextReference::RepositoryRange(_) => None,
    };
    if let Some(artifact) = artifact {
      insert_shared_artifact_reference(
        tx,
        run_id,
        artifact.artifact_id(),
        octacity_server_store::FactoryArtifactRole::CallContext,
        artifact.content_digest(),
        recorded_at,
      )
      .await?;
    }
  }
  Ok(())
}

/// Retains a shared immutable input once per Run and semantic role.
///
/// Multiple handoffs or calls may legitimately select the same bytes at
/// different times. The first reference owns `created_at`; later references
/// must prove the exact same digest without rewriting that retention fact.
async fn insert_shared_artifact_reference(
  tx: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
  artifact_id: octacity_server_domain::ArtifactId,
  role: octacity_server_store::FactoryArtifactRole,
  digest: FactoryDigest,
  recorded_at: Timestamp,
) -> Result<(), StoreError> {
  let result = sqlx::query(
    "INSERT INTO factory_artifact_references \
       (run_id, artifact_id, role, expected_sha256, created_at) \
     VALUES ($1, $2, $3, $4, to_timestamp($5::double precision / 1000.0)) \
     ON CONFLICT (run_id, artifact_id, role) DO UPDATE SET artifact_id = EXCLUDED.artifact_id \
     WHERE factory_artifact_references.expected_sha256 = EXCLUDED.expected_sha256 \
       AND factory_artifact_references.released_at IS NULL",
  )
  .bind(run_id.as_uuid())
  .bind(artifact_id.as_uuid())
  .bind(role.as_str())
  .bind(digest.as_bytes().as_slice())
  .bind(recorded_at.unix_millis())
  .execute(&mut **tx)
  .await
  .map_err(unavailable)?;
  if result.rows_affected() == 1 {
    Ok(())
  } else {
    Err(StoreError::Conflict {
      entity: octacity_server_domain::EntityKind::Artifact,
    })
  }
}

async fn insert_call(
  tx: &mut Transaction<'_, Postgres>,
  recorded_at: Timestamp,
  record: &MacroCall,
) -> Result<(), StoreError> {
  sqlx::query("INSERT INTO factory_call_nodes (id, run_id, stage_attempt_id, parent_call_id, call_kind, context_manifest_id, context_digest, depth, call_node, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, to_timestamp($10::double precision / 1000.0))")
    .bind(record.id().as_uuid()).bind(record.run_id().as_uuid()).bind(record.stage_attempt_id().as_uuid())
    .bind(record.parent_id().map(|id| id.as_uuid())).bind(record.kind().as_str()).bind(record.context_manifest_id().as_uuid())
    .bind(record.context_digest().as_bytes().as_slice()).bind(i16::try_from(record.depth()).map_err(|_| StoreError::Unavailable)?)
    .bind(Json(record)).bind(recorded_at.unix_millis()).execute(&mut **tx).await.map_err(unavailable)?;
  for dependency in record.stage_dependencies() {
    sqlx::query("INSERT INTO factory_call_stage_dependencies (run_id, call_id, stage_attempt_id) VALUES ($1, $2, $3)")
      .bind(record.run_id().as_uuid())
      .bind(record.id().as_uuid())
      .bind(dependency.as_uuid())
      .execute(&mut **tx)
      .await
      .map_err(unavailable)?;
  }
  for dependency in record.call_dependencies() {
    sqlx::query("INSERT INTO factory_call_dependencies (run_id, call_id, dependency_call_id) VALUES ($1, $2, $3)")
      .bind(record.run_id().as_uuid())
      .bind(record.id().as_uuid())
      .bind(dependency.as_uuid())
      .execute(&mut **tx)
      .await
      .map_err(unavailable)?;
  }
  Ok(())
}

async fn insert_call_completion(
  tx: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
  recorded_at: Timestamp,
  record: &MacroCallCompletion,
) -> Result<(), StoreError> {
  let digest = record.digest().map_err(|_| StoreError::Unavailable)?;
  sqlx::query(
    "INSERT INTO factory_call_completions \
     (call_id, run_id, terminal, content_digest, completion, completed_at) \
     VALUES ($1, $2, $3, $4, $5, to_timestamp($6::double precision / 1000.0))",
  )
  .bind(record.call_id().as_uuid())
  .bind(run_id.as_uuid())
  .bind(record.terminal().as_str())
  .bind(digest.as_bytes().as_slice())
  .bind(Json(record))
  .bind(record.completed_at().unix_millis())
  .execute(&mut **tx)
  .await
  .map_err(unavailable)?;
  for artifact in [
    record.result(),
    record.summary_artifact(),
    record.trace(),
    record.provenance(),
  ]
  .into_iter()
  .flatten()
  {
    insert_shared_artifact_reference(
      tx,
      run_id,
      artifact.artifact_id(),
      octacity_server_store::FactoryArtifactRole::CallOutput,
      artifact.content_digest(),
      recorded_at,
    )
    .await?;
  }
  Ok(())
}

async fn insert_signal_request(
  tx: &mut Transaction<'_, Postgres>,
  recorded_at: Timestamp,
  record: &DecisionSignalRequest,
) -> Result<(), StoreError> {
  sqlx::query("INSERT INTO factory_decision_signal_requests (id, run_id, stage_attempt_id, call_node_id, purpose, policy_digest, input_digest, request, requested_at) VALUES ($1, $2, $3, NULL, $4, $5, $6, $7, to_timestamp($8::double precision / 1000.0))")
    .bind(record.id().as_uuid()).bind(record.run_id().as_uuid()).bind(record.stage_attempt_id().as_uuid()).bind(record.purpose().as_str())
    .bind(record.policy_digest().as_bytes().as_slice()).bind(record.input_digest().as_bytes().as_slice()).bind(Json(record))
    .bind(recorded_at.unix_millis()).execute(&mut **tx).await.map_err(unavailable)?;
  Ok(())
}

async fn insert_signal_receipt(
  tx: &mut Transaction<'_, Postgres>,
  recorded_at: Timestamp,
  record: &DecisionSignalReceipt,
) -> Result<(), StoreError> {
  let request = record.request();
  sqlx::query("INSERT INTO factory_decision_signal_receipts (id, run_id, request_id, provider_digest, model_digest, policy_digest, input_digest, receipt_digest, receipt, received_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, to_timestamp($10::double precision / 1000.0))")
    .bind(record.id().as_uuid()).bind(record.run_id().as_uuid()).bind(record.request_id().as_uuid())
    .bind(record.provider().digest().as_bytes().as_slice()).bind(record.model().digest().as_bytes().as_slice())
    .bind(record.policy().digest().as_bytes().as_slice()).bind(request.input().digest().as_bytes().as_slice())
    .bind(record.receipt_digest().as_bytes().as_slice()).bind(Json(record)).bind(recorded_at.unix_millis())
    .execute(&mut **tx).await.map_err(unavailable)?;
  Ok(())
}

async fn insert_build_link(
  tx: &mut Transaction<'_, Postgres>,
  recorded_at: Timestamp,
  record: &FactoryBuildLink,
) -> Result<(), StoreError> {
  sqlx::query("INSERT INTO factory_build_links (build_id, run_id, stage_attempt_id, attempt_id, factory_configuration_id, factory_configuration_version, build_configuration_id, build_configuration_version, task_envelope_digest, effective_policy_digest, input_digest, exact_revision, link, linked_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,to_timestamp($14::double precision / 1000.0))")
    .bind(record.build_id.as_uuid()).bind(record.run_id.as_uuid()).bind(record.stage_attempt_id.as_uuid()).bind(record.attempt_id.as_uuid())
    .bind(record.factory_configuration.id().as_uuid()).bind(number(record.factory_configuration.version().get())?)
    .bind(record.build_configuration.id().as_uuid()).bind(number(record.build_configuration.version().get())?)
    .bind(record.task_envelope_digest.as_bytes().as_slice()).bind(record.effective_policy_digest.as_bytes().as_slice())
    .bind(record.input_digest.as_bytes().as_slice()).bind(record.exact_revision.as_str()).bind(Json(record))
    .bind(recorded_at.unix_millis()).execute(&mut **tx).await.map_err(unavailable)?;
  for (ordinal, job_id) in record.job_ids.iter().enumerate() {
    sqlx::query("INSERT INTO factory_build_link_jobs (build_id, job_id, ordinal) VALUES ($1, $2, $3)")
      .bind(record.build_id.as_uuid())
      .bind(job_id.as_uuid())
      .bind(i16::try_from(ordinal).map_err(|_| StoreError::Unavailable)?)
      .execute(&mut **tx)
      .await
      .map_err(unavailable)?;
  }
  Ok(())
}

async fn insert_build_observation(
  tx: &mut Transaction<'_, Postgres>,
  record: &FactoryBuildObservationRecord,
) -> Result<(), StoreError> {
  sqlx::query("INSERT INTO factory_build_observations (id, run_id, build_id, attempt_id, build_version, attempt_version, state, observation, observed_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,to_timestamp($9::double precision / 1000.0))")
    .bind(record.id.as_bytes().as_slice()).bind(record.run_id.as_uuid()).bind(record.build_id.as_uuid()).bind(record.attempt_id.as_uuid())
    .bind(number(record.build_version.get())?).bind(number(record.attempt_version.get())?).bind(build_state(record.state))
    .bind(Json(record)).bind(record.observed_at.unix_millis()).execute(&mut **tx).await.map_err(unavailable)?;
  for output in &record.outputs {
    insert_artifact_reference(
      tx,
      record.run_id,
      output.artifact_id,
      octacity_server_store::FactoryArtifactRole::BuildOutput,
      Some(output.digest.as_bytes()),
      record.observed_at,
    )
    .await?;
  }
  Ok(())
}

async fn insert_changeset(
  tx: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
  recorded_at: Timestamp,
  record: &ChangeSet,
) -> Result<(), StoreError> {
  sqlx::query("INSERT INTO factory_changesets (id, run_id, stage_attempt_id, base_revision, candidate_revision, changeset_digest, changeset, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,to_timestamp($8::double precision / 1000.0))")
    .bind(record.id().as_uuid()).bind(run_id.as_uuid()).bind(record.stage_attempt_id().as_uuid()).bind(record.subject().exact().base_revision().as_str())
    .bind(record.subject().candidate_revision().as_str()).bind(record.subject().changeset_digest().as_bytes().as_slice()).bind(Json(record))
    .bind(recorded_at.unix_millis()).execute(&mut **tx).await.map_err(unavailable)?;
  insert_artifact_reference(
    tx,
    run_id,
    record.bundle_artifact(),
    octacity_server_store::FactoryArtifactRole::ChangeSetBundle,
    None,
    recorded_at,
  )
  .await?;
  insert_artifact_reference(
    tx,
    run_id,
    record.manifest_artifact(),
    octacity_server_store::FactoryArtifactRole::ChangeSetManifest,
    None,
    recorded_at,
  )
  .await?;
  Ok(())
}

async fn insert_evidence(
  tx: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
  recorded_at: Timestamp,
  record: &EvidenceManifest,
) -> Result<(), StoreError> {
  sqlx::query("INSERT INTO factory_evidence_manifests (id, run_id, changeset_id, manifest_digest, manifest, created_at) VALUES ($1,$2,$3,$4,$5,to_timestamp($6::double precision / 1000.0))")
    .bind(record.id().as_uuid()).bind(run_id.as_uuid()).bind(record.changeset_id().as_uuid())
    .bind(document_digest("octacity.factory.evidence-manifest.v1", record)?.as_bytes().as_slice()).bind(Json(record))
    .bind(recorded_at.unix_millis()).execute(&mut **tx).await.map_err(unavailable)?;
  for item in record.items() {
    insert_artifact_reference(
      tx,
      run_id,
      item.artifact_id(),
      octacity_server_store::FactoryArtifactRole::Evidence,
      Some(item.digest().as_bytes()),
      recorded_at,
    )
    .await?;
  }
  Ok(())
}

async fn insert_plan(
  tx: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
  recorded_at: Timestamp,
  record: &EvaluationPlan,
) -> Result<(), StoreError> {
  let result = sqlx::query("INSERT INTO factory_evaluation_plans (id, run_id, changeset_id, evidence_manifest_id, plan_digest, plan, created_at) SELECT $1,$2,e.changeset_id,$3,$4,$5,to_timestamp($6::double precision / 1000.0) FROM factory_evidence_manifests e WHERE e.id = $3 AND e.run_id = $2")
    .bind(record.id().as_uuid()).bind(run_id.as_uuid()).bind(record.evidence_id().as_uuid())
    .bind(document_digest("octacity.factory.evaluation-plan.v1", record)?.as_bytes().as_slice()).bind(Json(record))
    .bind(recorded_at.unix_millis()).execute(&mut **tx).await.map_err(unavailable)?;
  exactly_one(result.rows_affected())
}

async fn insert_assessment(
  tx: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
  recorded_at: Timestamp,
  record: &Assessment,
) -> Result<(), StoreError> {
  sqlx::query("INSERT INTO factory_assessments (id, run_id, evaluation_plan_id, evaluator, assessment_digest, assessment, created_at) VALUES ($1,$2,$3,$4,$5,$6,to_timestamp($7::double precision / 1000.0))")
    .bind(record.id().as_uuid()).bind(run_id.as_uuid()).bind(record.plan_id().as_uuid()).bind(record.evaluator().as_str())
    .bind(document_digest("octacity.factory.assessment.v1", record)?.as_bytes().as_slice()).bind(Json(record))
    .bind(recorded_at.unix_millis()).execute(&mut **tx).await.map_err(unavailable)?;
  Ok(())
}

async fn insert_decision(
  tx: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
  recorded_at: Timestamp,
  record: &Decision,
) -> Result<(), StoreError> {
  sqlx::query("INSERT INTO factory_decisions (id, run_id, evaluation_plan_id, outcome, input_digest, policy_digest, decision_digest, decision, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,to_timestamp($9::double precision / 1000.0))")
    .bind(record.id().as_uuid()).bind(run_id.as_uuid()).bind(record.plan_id().as_uuid()).bind(record.outcome().as_str())
    .bind(record.input_digest().as_bytes().as_slice()).bind(record.policy_digest().as_bytes().as_slice())
    .bind(document_digest("octacity.factory.decision.v1", record)?.as_bytes().as_slice()).bind(Json(record))
    .bind(recorded_at.unix_millis()).execute(&mut **tx).await.map_err(unavailable)?;
  for (ordinal, assessment_id) in record.assessment_ids().iter().enumerate() {
    sqlx::query("INSERT INTO factory_decision_assessments (decision_id, assessment_id, ordinal) VALUES ($1,$2,$3)")
      .bind(record.id().as_uuid())
      .bind(assessment_id.as_uuid())
      .bind(i16::try_from(ordinal).map_err(|_| StoreError::Unavailable)?)
      .execute(&mut **tx)
      .await
      .map_err(unavailable)?;
  }
  Ok(())
}

async fn insert_escalation(
  tx: &mut Transaction<'_, Postgres>,
  recorded_at: Timestamp,
  record: &Escalation,
) -> Result<(), StoreError> {
  sqlx::query("INSERT INTO factory_escalations (id, run_id, decision_id, reason, escalation, created_at) VALUES ($1,$2,$3,$4,$5,to_timestamp($6::double precision / 1000.0))")
    .bind(record.id().as_uuid()).bind(record.run_id().as_uuid()).bind(record.decision_id().map(|id| id.as_uuid()))
    .bind(record.reason().as_str()).bind(Json(record)).bind(recorded_at.unix_millis()).execute(&mut **tx).await.map_err(unavailable)?;
  Ok(())
}

async fn insert_delivery(
  tx: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
  recorded_at: Timestamp,
  record: &DeliveryAttempt,
) -> Result<(), StoreError> {
  let operation = document_digest("octacity.factory.delivery-attempt.v1", record)?;
  let result = sqlx::query("INSERT INTO factory_delivery_attempts (id, run_id, changeset_id, decision_id, attempt_number, state, operation_digest, attempt, recorded_at) SELECT $1,$2,p.changeset_id,$3,$4,$5,$6,$7,to_timestamp($8::double precision / 1000.0) FROM factory_decisions d JOIN factory_evaluation_plans p ON p.id = d.evaluation_plan_id WHERE d.id = $3 AND d.run_id = $2")
    .bind(record.id().as_uuid()).bind(run_id.as_uuid()).bind(record.decision_id().as_uuid()).bind(number(record.number().get())?)
    .bind(delivery_state(record.state())).bind(operation.as_bytes().as_slice()).bind(Json(record)).bind(recorded_at.unix_millis())
    .execute(&mut **tx).await.map_err(unavailable)?;
  exactly_one(result.rows_affected())
}

async fn insert_reporting(
  tx: &mut Transaction<'_, Postgres>,
  recorded_at: Timestamp,
  record: &ReportingAttempt,
) -> Result<(), StoreError> {
  let operation = document_digest("octacity.factory.reporting-attempt.v1", record)?;
  sqlx::query("INSERT INTO factory_reporting_attempts (id, run_id, attempt_number, state, operation_digest, attempt, recorded_at) VALUES ($1,$2,$3,$4,$5,$6,to_timestamp($7::double precision / 1000.0))")
    .bind(record.id().as_uuid()).bind(record.run_id().as_uuid()).bind(number(record.number().get())?).bind(reporting_state(record.state()))
    .bind(operation.as_bytes().as_slice()).bind(Json(record)).bind(recorded_at.unix_millis()).execute(&mut **tx).await.map_err(unavailable)?;
  Ok(())
}

pub(crate) async fn history_rows(
  tx: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
) -> Result<FactoryRunHistoryAppend, StoreError> {
  Ok(FactoryRunHistoryAppend {
    stage_attempts: json_rows(
      tx,
      "SELECT stage_attempt FROM factory_stage_attempts WHERE run_id = $1 ORDER BY id",
      run_id,
    )
    .await?,
    stage_attempt_completions: json_rows(
      tx,
      "SELECT completion FROM factory_stage_attempt_completions WHERE run_id = $1 ORDER BY id",
      run_id,
    )
    .await?,
    stage_handoffs: json_rows(
      tx,
      "SELECT handoff FROM factory_stage_handoffs WHERE run_id = $1 ORDER BY id",
      run_id,
    )
    .await?,
    context_manifests: json_rows(
      tx,
      "SELECT manifest FROM factory_context_manifests WHERE run_id = $1 ORDER BY id",
      run_id,
    )
    .await?,
    macro_calls: json_rows(
      tx,
      "SELECT call_node FROM factory_call_nodes WHERE run_id = $1 ORDER BY id",
      run_id,
    )
    .await?,
    macro_call_completions: json_rows(
      tx,
      "SELECT completion FROM factory_call_completions WHERE run_id = $1 ORDER BY call_id",
      run_id,
    )
    .await?,
    signal_requests: json_rows(
      tx,
      "SELECT request FROM factory_decision_signal_requests WHERE run_id = $1 ORDER BY id",
      run_id,
    )
    .await?,
    signal_receipts: json_rows(
      tx,
      "SELECT receipt FROM factory_decision_signal_receipts WHERE run_id = $1 ORDER BY id",
      run_id,
    )
    .await?,
    linked_builds: json_rows(
      tx,
      "SELECT link FROM factory_build_links WHERE run_id = $1 ORDER BY build_id",
      run_id,
    )
    .await?,
    build_observations: json_rows(
      tx,
      "SELECT observation FROM factory_build_observations WHERE run_id = $1 ORDER BY id",
      run_id,
    )
    .await?,
    candidates: json_rows(
      tx,
      "SELECT changeset FROM factory_changesets WHERE run_id = $1 ORDER BY id",
      run_id,
    )
    .await?,
    evidence: json_rows(
      tx,
      "SELECT manifest FROM factory_evidence_manifests WHERE run_id = $1 ORDER BY id",
      run_id,
    )
    .await?,
    evaluation_plans: json_rows(
      tx,
      "SELECT plan FROM factory_evaluation_plans WHERE run_id = $1 ORDER BY id",
      run_id,
    )
    .await?,
    assessments: json_rows(
      tx,
      "SELECT assessment FROM factory_assessments WHERE run_id = $1 ORDER BY id",
      run_id,
    )
    .await?,
    decisions: json_rows(
      tx,
      "SELECT decision FROM factory_decisions WHERE run_id = $1 ORDER BY id",
      run_id,
    )
    .await?,
    escalations: json_rows(
      tx,
      "SELECT escalation FROM factory_escalations WHERE run_id = $1 ORDER BY id",
      run_id,
    )
    .await?,
    delivery_attempts: json_rows(
      tx,
      "SELECT attempt FROM factory_delivery_attempts WHERE run_id = $1 ORDER BY id",
      run_id,
    )
    .await?,
    reporting_attempts: json_rows(
      tx,
      "SELECT attempt FROM factory_reporting_attempts WHERE run_id = $1 ORDER BY id",
      run_id,
    )
    .await?,
  })
}

pub(super) async fn stage_history(
  tx: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
) -> Result<
  (
    BTreeMap<octacity_server_factory::StageAttemptId, StageAttempt>,
    BTreeMap<FactoryDigest, StageAttemptCompletion>,
  ),
  StoreError,
> {
  let attempts = json_rows(
    tx,
    "SELECT stage_attempt FROM factory_stage_attempts WHERE run_id = $1 ORDER BY attempt_number",
    run_id,
  )
  .await?
  .into_iter()
  .map(|record: StageAttempt| (record.id(), record))
  .collect();
  let completions = json_rows(
    tx,
    "SELECT completion FROM factory_stage_attempt_completions WHERE run_id = $1 ORDER BY id",
    run_id,
  )
  .await?
  .into_iter()
  .map(|record: StageAttemptCompletion| (record.id(), record))
  .collect();
  Ok((attempts, completions))
}

pub(super) async fn call_context_history(
  tx: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
) -> Result<
  (
    BTreeMap<octacity_server_factory::StageHandoffId, StageHandoff>,
    BTreeMap<octacity_server_factory::ContextManifestId, ContextManifest>,
    BTreeMap<octacity_server_factory::MacroCallId, MacroCall>,
    BTreeMap<octacity_server_factory::MacroCallId, MacroCallCompletion>,
  ),
  StoreError,
> {
  let handoffs = json_rows(
    tx,
    "SELECT handoff FROM factory_stage_handoffs WHERE run_id = $1 ORDER BY id",
    run_id,
  )
  .await?
  .into_iter()
  .map(|record: StageHandoff| (record.id(), record))
  .collect();
  let manifests = json_rows(
    tx,
    "SELECT manifest FROM factory_context_manifests WHERE run_id = $1 ORDER BY id",
    run_id,
  )
  .await?
  .into_iter()
  .map(|record: ContextManifest| (record.id(), record))
  .collect();
  let calls = json_rows(
    tx,
    "SELECT call_node FROM factory_call_nodes WHERE run_id = $1 ORDER BY id",
    run_id,
  )
  .await?
  .into_iter()
  .map(|record: MacroCall| (record.id(), record))
  .collect();
  let completions = json_rows(
    tx,
    "SELECT completion FROM factory_call_completions WHERE run_id = $1 ORDER BY call_id",
    run_id,
  )
  .await?
  .into_iter()
  .map(|record: MacroCallCompletion| (record.call_id(), record))
  .collect();
  Ok((handoffs, manifests, calls, completions))
}

async fn json_rows<T: for<'de> serde::Deserialize<'de> + Send + Unpin + 'static>(
  tx: &mut Transaction<'_, Postgres>,
  query: &'static str,
  run_id: FactoryRunId,
) -> Result<Vec<T>, StoreError> {
  sqlx::query_scalar::<_, Json<T>>(query)
    .bind(run_id.as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(unavailable)
    .map(|rows| rows.into_iter().map(|row| row.0).collect())
}

fn document_digest(domain: &str, value: &impl Serialize) -> Result<FactoryDigest, StoreError> {
  let bytes = serde_json::to_vec(value).map_err(|_| StoreError::Unavailable)?;
  Ok(FactoryDigest::sha256(domain, &[&bytes]))
}

fn number(value: u64) -> Result<i64, StoreError> {
  i64::try_from(value).map_err(|_| StoreError::Unavailable)
}

fn exactly_one(rows: u64) -> Result<(), StoreError> {
  if rows == 1 {
    Ok(())
  } else {
    Err(StoreError::Unavailable)
  }
}

const fn stage_outcome(value: octacity_server_factory::StageAttemptOutcome) -> &'static str {
  match value {
    octacity_server_factory::StageAttemptOutcome::Succeeded => "succeeded",
    octacity_server_factory::StageAttemptOutcome::Failed => "failed",
    octacity_server_factory::StageAttemptOutcome::Cancelled => "cancelled",
  }
}
const fn build_state(value: octacity_server_orchestrator::BuildState) -> &'static str {
  match value {
    octacity_server_orchestrator::BuildState::Succeeded => "succeeded",
    octacity_server_orchestrator::BuildState::Failed => "failed",
    octacity_server_orchestrator::BuildState::Cancelled => "cancelled",
    octacity_server_orchestrator::BuildState::Queued => "queued",
    octacity_server_orchestrator::BuildState::Running => "running",
  }
}
const fn delivery_state(value: octacity_server_factory::DeliveryState) -> &'static str {
  match value {
    octacity_server_factory::DeliveryState::Succeeded => "succeeded",
    octacity_server_factory::DeliveryState::Failed => "failed",
    octacity_server_factory::DeliveryState::Unknown => "unknown",
  }
}
const fn reporting_state(value: octacity_server_factory::ReportingState) -> &'static str {
  match value {
    octacity_server_factory::ReportingState::Succeeded => "succeeded",
    octacity_server_factory::ReportingState::Failed => "failed",
    octacity_server_factory::ReportingState::Unknown => "unknown",
  }
}
