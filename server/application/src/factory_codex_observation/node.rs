//! Qualified Codex wire verification for arbitrary declared node data.
use super::{CodexHarnessOutcome, FactoryCodexOutputDocument, evidence::*, records::*};
use crate::ApplicationError;
use octacity_server_domain::{ImmutableRevision, Timestamp};
use octacity_server_factory::{
  BudgetLimit, BudgetUsage, FactoryDigest, FlowBuildProfile, FlowDataSchema, FlowVerifiedBuildData,
};

/// Expectations established from the frozen executed Job and protected input manifest.
/// Provider records cannot establish or replace these facts.
pub struct FactoryCodexNodeRunContract<'a> {
  /// Exact opaque model identifier from the retained Role Profile, absent for legacy tasks.
  pub model_identifier: Option<&'a str>,
  /// Executed immutable plugin, executable and model selection.
  pub profile: &'a FlowBuildProfile,
  /// Exact result contract declared by the admitted node.
  pub schema: &'a FlowDataSchema,
  /// BLAKE3 of the complete compiler-owned prompt actually supplied to execution.
  pub prompt_digest: FactoryDigest,
  /// Exact source revision actually materialized for the Job.
  pub source_revision: &'a ImmutableRevision,
  /// Frozen common execution ceiling.
  pub budget: BudgetLimit,
  /// Installed cost basis, verified against the executed protected manifest.
  pub usage_policy: &'a crate::FactoryCodexUsagePolicy,
  /// Authoritative Job creation through terminal completion, capped by the
  /// original signed deadline. A provider cannot extend this window.
  pub execution_window: std::ops::RangeInclusive<Timestamp>,
}

impl FactoryCodexNodeRunContract<'_> {
  pub(super) fn provenance_expectation(&self) -> CodexProvenanceExpectation<'_> {
    CodexProvenanceExpectation {
      plugin: &self.profile.plugin,
      executable: &self.profile.tool,
      model: &self.profile.model_or_tool,
      model_identifier: self.model_identifier,
      prompt_digest: self.prompt_digest,
      source_revision: self.source_revision.as_str(),
    }
  }
}
/// Bounded typed facts with private harness identifiers and trace content removed.
pub struct FactoryCodexNodeRunObservation {
  /// Harness execution status, independent of a configured semantic outcome.
  pub outcome: CodexHarnessOutcome,
  /// Schema-validated public data; execution failures provide no semantic result.
  pub data: Option<FlowVerifiedBuildData>,
  /// Independently reported and cross-checked measured consumption.
  pub usage: BudgetUsage,
}

/// Verifies retained output identity, exact invocation provenance and measured usage.
/// This adapter-local boundary performs no I/O and selects no Flow transition.
pub fn verify_codex_node_run(
  expected: FactoryCodexNodeRunContract<'_>,
  documents: &[FactoryCodexOutputDocument],
) -> Result<FactoryCodexNodeRunObservation, ApplicationError> {
  let records = verified_records(&expected, documents)?;
  let data = if records.outcome == CodexHarnessOutcome::Completed {
    Some(FlowVerifiedBuildData::new(expected.schema.clone(), records.value).map_err(|_| ApplicationError::invalid())?)
  } else {
    None
  };
  Ok(FactoryCodexNodeRunObservation {
    outcome: records.outcome,
    data,
    usage: records.usage,
  })
}

/// Projects a qualified readonly evaluation into an immutable server-owned Assessment.
/// The installed Source independently verifies the executed Job and protected program.
/// A model may return only EvaluationV1; it cannot supply Assessment provenance.
pub fn verify_codex_review_node_run(
  expected: FactoryCodexNodeRunContract<'_>,
  intent: &octacity_server_factory::FlowBuildIntent,
  review: crate::FactoryCodexReviewCompilation<'_>,
  compiled: &crate::CompiledCodexTask,
  documents: &[FactoryCodexOutputDocument],
) -> Result<FactoryCodexNodeRunObservation, ApplicationError> {
  use octacity_server_factory::{
    Assessment, AssessmentFinding, AssessmentId, AssessmentInput, BoundedSummary, FactoryTaskSubject,
  };
  let invalid = ApplicationError::invalid;
  review.validate(intent).map_err(|_| invalid())?;
  let prompt = compiled.document(crate::FactoryProtectedDocumentKind::Prompt);
  let text = std::str::from_utf8(prompt.bytes()).map_err(|_| invalid())?;
  let envelope: serde_json::Value = serde_json::from_slice(
    compiled
      .document(crate::FactoryProtectedDocumentKind::TaskEnvelope)
      .bytes(),
  )
  .map_err(|_| invalid())?;
  if expected.profile != intent.profile()
    || expected.budget != intent.budget()
    || expected.source_revision != review.invocation.plan().subject().candidate_revision()
    || expected.prompt_digest != crate::codex_prompt_provenance_digest(text)
    || expected.execution_window.start() < &intent.node().started_at()
    || expected.execution_window.end() > &intent.node().deadline()
    || envelope["input"] != serde_json::to_value(intent.input()).map_err(|_| invalid())?
    || envelope["profile"] != serde_json::to_value(intent.profile()).map_err(|_| invalid())?
    || envelope["reviewer"] != serde_json::to_value(review.policy).map_err(|_| invalid())?
    || envelope["result_schema"] != serde_json::to_value(expected.schema.reference()).map_err(|_| invalid())?
  {
    return Err(invalid());
  }
  let records = verified_records(&expected, documents)?;
  if records.outcome != CodexHarnessOutcome::Completed {
    return Ok(FactoryCodexNodeRunObservation {
      outcome: records.outcome,
      data: None,
      usage: records.usage,
    });
  }
  let report = serde_json::from_value(records.value).map_err(|_| invalid())?;
  let (outcome, summary, findings) = evaluation_report(report).ok_or_else(invalid)?;
  let findings = findings
    .iter()
    .map(|finding| AssessmentFinding::from_result(finding, review.invocation.evidence()))
    .collect::<Result<Vec<_>, _>>()
    .map_err(|_| invalid())?;
  let id = AssessmentId::from_uuid(uuid::Uuid::new_v5(
    &intent.node().id().as_uuid(),
    b"octacity.factory.assessment.v1",
  ))
  .map_err(|_| invalid())?;
  let assessment = Assessment::new(
    id,
    review.invocation.plan(),
    AssessmentInput {
      subject: review.invocation.plan().subject().clone(),
      evaluator: review.policy.evaluator.clone(),
      outcome,
      summary: BoundedSummary::new(
        FactoryTaskSubject::Candidate(review.invocation.plan().subject().clone()),
        summary,
        records.provenance.content_digest(),
      ),
      findings,
      model: expected.profile.model_or_tool.clone(),
      prompt_digest: prompt.content_digest(),
      result: records.result,
      provenance: records.provenance,
    },
  )
  .map_err(|_| invalid())?;
  assessment
    .validate_bindings(review.invocation.plan(), review.invocation.evidence())
    .map_err(|_| invalid())?;
  let data = FlowVerifiedBuildData::new(
    expected.schema.clone(),
    serde_json::to_value(assessment).map_err(|_| invalid())?,
  )
  .map_err(|_| invalid())?;
  Ok(FactoryCodexNodeRunObservation {
    outcome: records.outcome,
    data: Some(data),
    usage: records.usage,
  })
}

struct VerifiedRecords {
  outcome: CodexHarnessOutcome,
  value: serde_json::Value,
  usage: BudgetUsage,
  result: octacity_server_factory::FactoryArtifactReference,
  provenance: octacity_server_factory::FactoryArtifactReference,
}
fn verified_records(
  expected: &FactoryCodexNodeRunContract<'_>,
  documents: &[FactoryCodexOutputDocument],
) -> Result<VerifiedRecords, ApplicationError> {
  use crate::factory_codex_contract::{
    CODEX_EXECUTABLE_IDENTITY, CODEX_PLUGIN_IDENTITY, CODEX_RUN_PROVENANCE, CODEX_RUN_RESULT, CODEX_RUN_TRACE,
  };
  let invalid = ApplicationError::invalid;
  if !expected
    .usage_policy
    .matches_selection(&expected.profile.model_or_tool, expected.model_identifier)
  {
    return Err(invalid());
  }
  if documents.len() != 3
    || expected.profile.plugin.identity().as_str() != CODEX_PLUGIN_IDENTITY
    || expected.profile.tool.identity().as_str() != CODEX_EXECUTABLE_IDENTITY
    || expected.profile.result_output.as_str() != CODEX_RUN_RESULT
  {
    return Err(invalid());
  }
  let output_bytes = documents
    .iter()
    .try_fold(0_u64, |total, document| total.checked_add(document.identity.size_bytes))
    .ok_or_else(invalid)?;
  if output_bytes > expected.budget.max_output_bytes() {
    return Err(invalid());
  }
  let identities = documents
    .iter()
    .map(|document| document.identity.clone())
    .collect::<Vec<_>>();
  let first = &identities[0];
  if identities.iter().any(|identity| {
    identity.build_id != first.build_id || identity.attempt_id != first.attempt_id || identity.job_id != first.job_id
  }) {
    return Err(invalid());
  }
  let outputs = output_map(&identities).ok_or_else(invalid)?;
  let documents = document_map(documents, &outputs).ok_or_else(invalid)?;
  let result_document = documents.get(CODEX_RUN_RESULT).ok_or_else(invalid)?;
  let trace_document = documents.get(CODEX_RUN_TRACE).ok_or_else(invalid)?;
  let provenance_document = documents.get(CODEX_RUN_PROVENANCE).ok_or_else(invalid)?;
  if !record_output_types(result_document, trace_document, provenance_document) {
    return Err(invalid());
  }
  let result: CodexResultRecord<serde_json::Value> =
    serde_json::from_slice(&result_document.bytes).map_err(|_| invalid())?;
  let provenance: CodexProvenanceRecord = serde_json::from_slice(&provenance_document.bytes).map_err(|_| invalid())?;
  if !valid_trace(&trace_document.bytes, &provenance) {
    return Err(invalid());
  }
  if !valid_provenance_window(&provenance, &expected.execution_window) {
    return Err(invalid());
  }
  if !valid_provenance_for(expected.provenance_expectation(), &result, &provenance) {
    return Err(invalid());
  }
  let usage = BudgetUsage {
    attempts: 1,
    elapsed_millis: provenance.timing.duration_millis,
    tokens: result
      .usage
      .get("input_tokens")
      .copied()
      .ok_or_else(invalid)?
      .checked_add(result.usage.get("output_tokens").copied().ok_or_else(invalid)?)
      .ok_or_else(invalid)?,
    cost_micro_units: expected
      .usage_policy
      .cost(&expected.profile.model_or_tool, &result.usage)?,
    output_bytes,
  };
  usage.validate(expected.budget).map_err(|_| invalid())?;
  Ok(VerifiedRecords {
    outcome: result.outcome.into(),
    value: result.structured_result,
    usage,
    result: artifact_reference(&result_document.identity).ok_or_else(invalid)?,
    provenance: artifact_reference(&provenance_document.identity).ok_or_else(invalid)?,
  })
}
