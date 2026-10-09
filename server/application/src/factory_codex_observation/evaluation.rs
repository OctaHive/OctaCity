//! Projection of one independent read-only Codex review into Factory records.

use octacity_protocol::RunnerEventPayload;
use octacity_server_domain::Timestamp;
use octacity_server_factory::{
  Assessment, AssessmentFinding, AssessmentId, AssessmentInput, BoundedSummary, BudgetUsage, EvaluationBranchResult,
  EvaluationPlan, EvaluationProgress, EvaluationResult, EvidenceManifest, FactoryArtifactReference, FactoryError,
  FactoryTaskEnvelope, FactoryTaskMode, ImmutableReference, MacroCall, MacroCallCompletion, MacroCallCompletionOutputs,
  MacroCallKind, StageAttempt,
};
use octacity_server_store::FactoryRunHistoryAppend;

use super::{
  CodexHarnessOutcome, FactoryCodexObservationStatus, FactoryCodexOutputDocument, records::evaluation_report,
  validation::validate_codex_run,
};
use crate::FactoryBuildObservation;

/// Trusted terminal projection of one independent read-only evaluation Build.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryCodexEvaluationObservation {
  status: FactoryCodexObservationStatus,
  usage: BudgetUsage,
  result: Option<EvaluationResult>,
  assessment: Option<Assessment>,
  result_artifact: Option<FactoryArtifactReference>,
  trace: Option<FactoryArtifactReference>,
  provenance: Option<FactoryArtifactReference>,
}

/// Authoritative records and joined progress produced by one evaluator result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryCodexEvaluationCompletion {
  progress: EvaluationProgress,
  history: FactoryRunHistoryAppend,
}

impl FactoryCodexEvaluationCompletion {
  /// Returns progress with the exact immutable branch result recorded.
  #[must_use]
  pub const fn progress(&self) -> &EvaluationProgress {
    &self.progress
  }

  /// Returns the append-only call completion and Assessment records.
  #[must_use]
  pub const fn history(&self) -> &FactoryRunHistoryAppend {
    &self.history
  }

  /// Splits the completion into values ready for one fenced store transition.
  #[must_use]
  pub fn into_parts(self) -> (EvaluationProgress, FactoryRunHistoryAppend) {
    (self.progress, self.history)
  }
}

/// Immutable cross-record bindings for one independent evaluator invocation.
#[derive(Clone, Copy, Debug)]
pub struct FactoryCodexReviewerBinding<'a> {
  envelope: &'a FactoryTaskEnvelope,
  plan: &'a EvaluationPlan,
  evidence: &'a EvidenceManifest,
  evaluator: &'a ImmutableReference,
  assessment_id: AssessmentId,
}

impl<'a> FactoryCodexReviewerBinding<'a> {
  /// Binds the exact envelope, evidence, plan, evaluator, and Assessment identity.
  pub fn new(
    envelope: &'a FactoryTaskEnvelope,
    plan: &'a EvaluationPlan,
    evidence: &'a EvidenceManifest,
    evaluator: &'a ImmutableReference,
    assessment_id: AssessmentId,
  ) -> Result<Self, FactoryError> {
    if envelope.mode() != FactoryTaskMode::Evaluate
      || envelope.subject().candidate() != Some(plan.subject())
      || plan.evidence_id() != evidence.id()
      || plan.subject() != evidence.subject()
      || !plan.has_evaluator(evaluator)
    {
      return Err(FactoryError::InvalidReference {
        relationship: "Codex reviewer bindings",
      });
    }
    Ok(Self {
      envelope,
      plan,
      evidence,
      evaluator,
      assessment_id,
    })
  }
}

/// Trusted terminal data collected from one ordinary evaluation Build.
#[derive(Clone, Copy, Debug)]
pub struct FactoryCodexTerminalObservation<'a> {
  /// Authoritative Build projection.
  pub build: &'a FactoryBuildObservation,
  /// Measured elapsed wall-clock time.
  pub elapsed_millis: u64,
  /// Ordered generic runner events.
  pub runner_events: &'a [RunnerEventPayload],
  /// Verified retained output documents.
  pub documents: &'a [FactoryCodexOutputDocument],
}

impl FactoryCodexEvaluationObservation {
  /// Returns the exact terminal classification without collapsing execution failures.
  #[must_use]
  pub const fn status(&self) -> FactoryCodexObservationStatus {
    self.status
  }

  /// Returns bounded usage available for deterministic budget accounting.
  #[must_use]
  pub const fn usage(&self) -> BudgetUsage {
    self.usage
  }

  /// Returns the validated provider-neutral result only for a successful invocation.
  #[must_use]
  pub const fn result(&self) -> Option<&EvaluationResult> {
    self.result.as_ref()
  }

  /// Returns the immutable Assessment only after full schema and plan validation.
  #[must_use]
  pub const fn assessment(&self) -> Option<&Assessment> {
    self.assessment.as_ref()
  }

  /// Returns the exact sanitized trace Artifact without exposing its bytes.
  #[must_use]
  pub const fn trace(&self) -> Option<&FactoryArtifactReference> {
    self.trace.as_ref()
  }

  /// Returns the exact evaluator provenance Artifact.
  #[must_use]
  pub const fn provenance(&self) -> Option<&FactoryArtifactReference> {
    self.provenance.as_ref()
  }

  /// Binds this observation to its distinct durable evaluator call node.
  pub fn completion(
    &self,
    call: &MacroCall,
    envelope: &FactoryTaskEnvelope,
    completed_at: Timestamp,
  ) -> Result<MacroCallCompletion, FactoryError> {
    if call.kind() != MacroCallKind::Evaluate {
      return Err(FactoryError::InvalidReference {
        relationship: "evaluation macro call",
      });
    }
    MacroCallCompletion::new(
      call,
      envelope,
      self.status.into(),
      self.usage,
      MacroCallCompletionOutputs {
        result: self.result_artifact.clone(),
        summary: self.result.as_ref().map(|result| result.summary().clone()),
        summary_artifact: None,
        trace: self.trace.clone(),
        provenance: self.provenance.clone(),
      },
      completed_at,
    )
  }

  /// Produces the append-only completion and Assessment batch for this call.
  pub fn history_append(
    &self,
    call: &MacroCall,
    envelope: &FactoryTaskEnvelope,
    completed_at: Timestamp,
  ) -> Result<FactoryRunHistoryAppend, FactoryError> {
    Ok(FactoryRunHistoryAppend {
      macro_call_completions: vec![self.completion(call, envelope, completed_at)?],
      assessments: self.assessment.iter().cloned().collect(),
      ..FactoryRunHistoryAppend::default()
    })
  }

  /// Records the exact branch result and its immutable rows as one transition input.
  pub fn complete_branch(
    &self,
    progress: &EvaluationProgress,
    plan: &EvaluationPlan,
    stage: &StageAttempt,
    call: &MacroCall,
    envelope: &FactoryTaskEnvelope,
    completed_at: Timestamp,
  ) -> Result<FactoryCodexEvaluationCompletion, FactoryError> {
    let assessment = self.assessment.as_ref().ok_or(FactoryError::InvalidReference {
      relationship: "successful evaluation assessment",
    })?;
    let result = EvaluationBranchResult::new(plan, stage, call, assessment)?;
    Ok(FactoryCodexEvaluationCompletion {
      progress: progress.record_result(result)?,
      history: self.history_append(call, envelope, completed_at)?,
    })
  }
}

/// Validates one terminal evaluation Build and projects a provider-neutral Assessment.
///
/// Build, transport, timeout, cancellation, integrity, and schema failures remain
/// terminal observation statuses and never create an `indeterminate` Assessment.
#[must_use]
pub fn observe_codex_evaluation(
  binding: FactoryCodexReviewerBinding<'_>,
  terminal: FactoryCodexTerminalObservation<'_>,
) -> FactoryCodexEvaluationObservation {
  let validated = match validate_codex_run::<super::records::EvaluationReportWire>(
    binding.envelope,
    FactoryTaskMode::Evaluate,
    terminal.build,
    terminal.elapsed_millis,
    terminal.runner_events,
    terminal.documents,
  ) {
    Ok(value) => value,
    Err(failure) => return failed(failure.status, failure.usage),
  };
  if validated.harness_outcome != CodexHarnessOutcome::Completed {
    return failed(FactoryCodexObservationStatus::ExecutionFailed, validated.usage);
  }
  let Some((outcome, summary, findings)) = evaluation_report(validated.structured_result) else {
    return failed(FactoryCodexObservationStatus::InvalidSchema, validated.usage);
  };
  let bounded_summary = BoundedSummary::new(
    binding.envelope.subject().clone(),
    summary,
    validated.provenance.content_digest(),
  );
  let result = match EvaluationResult::new(
    binding.envelope,
    outcome,
    bounded_summary.clone(),
    findings.clone(),
    validated.provenance.content_digest(),
  ) {
    Ok(value) => value,
    Err(_) => return failed(FactoryCodexObservationStatus::InvalidSchema, validated.usage),
  };
  let assessment_findings = match findings
    .iter()
    .map(|finding| AssessmentFinding::from_result(finding, binding.evidence))
    .collect::<Result<Vec<_>, _>>()
  {
    Ok(value) => value,
    Err(_) => return failed(FactoryCodexObservationStatus::InvalidSchema, validated.usage),
  };
  let assessment = match Assessment::new(
    binding.assessment_id,
    binding.plan,
    AssessmentInput {
      subject: binding.plan.subject().clone(),
      evaluator: binding.evaluator.clone(),
      outcome,
      summary: bounded_summary,
      findings: assessment_findings,
      model: binding.envelope.model().clone(),
      prompt_digest: binding.envelope.digests().prompt,
      result: validated.result.clone(),
      provenance: validated.provenance.clone(),
    },
  ) {
    Ok(value) => value,
    Err(_) => return failed(FactoryCodexObservationStatus::InvalidSchema, validated.usage),
  };
  FactoryCodexEvaluationObservation {
    status: FactoryCodexObservationStatus::Succeeded,
    usage: validated.usage,
    result: Some(result),
    assessment: Some(assessment),
    result_artifact: Some(validated.result),
    trace: Some(validated.trace),
    provenance: Some(validated.provenance),
  }
}

fn failed(status: FactoryCodexObservationStatus, usage: BudgetUsage) -> FactoryCodexEvaluationObservation {
  FactoryCodexEvaluationObservation {
    status,
    usage,
    result: None,
    assessment: None,
    result_artifact: None,
    trace: None,
    provenance: None,
  }
}
