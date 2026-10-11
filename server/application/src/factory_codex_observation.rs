//! Validation of generic Build outputs produced by the pinned Codex task.
//!
//! The Agent and coordinator retain only generic runner events and outputs.
//! This trusted application adapter recognizes the compiler-owned logical
//! names, validates the immutable bytes, and projects them into bounded
//! Factory values. Raw trace content never enters authoritative Factory state.

use octacity_protocol::RunnerEventPayload;
use octacity_server_artifacts::ArtifactIdentity;
use octacity_server_factory::{
  BoundedSummary, BudgetUsage, FactoryArtifactReference, FactoryProducedDeliverable, FactoryRepositoryPath,
  FactorySafeText, FactoryTaskEnvelope, FactoryTaskMode, ImplementationOutcome,
};

use crate::{FactoryBuildObservation, factory_codex_contract::CODEX_STAGE_SUMMARY};

mod evaluation;
mod evidence;
mod records;
mod validation;

pub use evaluation::{
  FactoryCodexEvaluationObservation, FactoryCodexReviewerBinding, FactoryCodexTerminalObservation,
  observe_codex_evaluation,
};

use evidence::artifact_reference;
use records::implementation_report;
use validation::validate_codex_run;

/// Stable semantic outcome recorded by the pinned Codex plugin.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodexHarnessOutcome {
  /// The harness completed the requested turn.
  Completed,
  /// The harness reported that policy or environment blocked progress.
  Blocked,
  /// The harness needs additional bounded input.
  NeedsInput,
  /// The harness exhausted its own budget.
  BudgetExhausted,
  /// The harness reported a semantic failure.
  Failed,
}

/// Distinct terminal classification retained by Factory coordination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FactoryCodexObservationStatus {
  /// Every required output was verified and projected successfully.
  Succeeded,
  /// The signed execution deadline elapsed.
  TimedOut,
  /// Cancellation became terminal.
  Cancelled,
  /// The provider or provider transport was unavailable.
  ProviderUnavailable,
  /// The ordinary Build reported an execution failure without a narrower cause.
  ExecutionFailed,
  /// The ordinary Build reported an infrastructure failure without a narrower cause.
  InfrastructureFailed,
  /// Output collection or the immutable output budget overflowed.
  OutputOverflow,
  /// A required logical output was absent.
  MissingDeliverable,
  /// A retained record was not syntactically valid JSON.
  InvalidJson,
  /// JSON was syntactically valid but violated a pinned record schema.
  InvalidSchema,
  /// Retained bytes did not match their immutable Artifact identity.
  IntegrityFailure,
  /// Non-output usage exceeded another immutable task budget.
  BudgetExceeded,
}

/// Exact immutable bytes fetched for one already-published generic output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryCodexOutputDocument {
  /// Published output identity.
  pub identity: ArtifactIdentity,
  /// Verified object bytes returned by the Artifact Store.
  pub bytes: Vec<u8>,
}

/// Bounded provider-neutral implementation report parsed from structured output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryImplementationReport {
  pub(super) harness_outcome: CodexHarnessOutcome,
  pub(super) outcome: ImplementationOutcome,
  pub(super) summary: FactorySafeText,
  pub(super) decisions: Vec<FactorySafeText>,
  pub(super) assumptions: Vec<FactorySafeText>,
  pub(super) unresolved_items: Vec<FactorySafeText>,
  pub(super) changed_components: Vec<FactoryRepositoryPath>,
  pub(super) validation_observations: Vec<FactorySafeText>,
}

impl FactoryImplementationReport {
  /// Returns the harness-level semantic outcome.
  #[must_use]
  pub const fn harness_outcome(&self) -> CodexHarnessOutcome {
    self.harness_outcome
  }

  /// Returns the schema-valid implementation conclusion.
  #[must_use]
  pub const fn outcome(&self) -> ImplementationOutcome {
    self.outcome
  }

  /// Returns the bounded, secret-checked summary.
  #[must_use]
  pub const fn summary(&self) -> &FactorySafeText {
    &self.summary
  }

  /// Returns candidate-affecting decisions in canonical order.
  #[must_use]
  pub fn decisions(&self) -> &[FactorySafeText] {
    &self.decisions
  }

  /// Returns assumptions in canonical order.
  #[must_use]
  pub fn assumptions(&self) -> &[FactorySafeText] {
    &self.assumptions
  }

  /// Returns unresolved work in canonical order.
  #[must_use]
  pub fn unresolved_items(&self) -> &[FactorySafeText] {
    &self.unresolved_items
  }

  /// Returns changed repository components in canonical order.
  #[must_use]
  pub fn changed_components(&self) -> &[FactoryRepositoryPath] {
    &self.changed_components
  }

  /// Returns validation observations in canonical order.
  #[must_use]
  pub fn validation_observations(&self) -> &[FactorySafeText] {
    &self.validation_observations
  }
}

/// Verified capability data from one ordinary Build; the common node owner retains its result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryCodexImplementationObservation {
  status: FactoryCodexObservationStatus,
  usage: BudgetUsage,
  report: Option<FactoryImplementationReport>,
  result: Option<FactoryArtifactReference>,
  trace: Option<FactoryArtifactReference>,
  provenance: Option<FactoryArtifactReference>,
  deliverables: Vec<FactoryProducedDeliverable>,
  summary: Option<BoundedSummary>,
}

impl FactoryCodexImplementationObservation {
  /// Returns the exact terminal classification.
  #[must_use]
  pub const fn status(&self) -> FactoryCodexObservationStatus {
    self.status
  }

  /// Returns bounded call usage available for deterministic budget accounting.
  #[must_use]
  pub const fn usage(&self) -> BudgetUsage {
    self.usage
  }

  /// Returns the typed report only after complete successful validation.
  #[must_use]
  pub const fn report(&self) -> Option<&FactoryImplementationReport> {
    self.report.as_ref()
  }

  /// Returns the exact structured-result Artifact.
  #[must_use]
  pub const fn result(&self) -> Option<&FactoryArtifactReference> {
    self.result.as_ref()
  }

  /// Returns the exact sanitized-trace Artifact without exposing its bytes.
  #[must_use]
  pub const fn trace(&self) -> Option<&FactoryArtifactReference> {
    self.trace.as_ref()
  }

  /// Returns the exact provenance Artifact.
  #[must_use]
  pub const fn provenance(&self) -> Option<&FactoryArtifactReference> {
    self.provenance.as_ref()
  }

  /// Returns declared deliverables bound to immutable generic outputs.
  #[must_use]
  pub fn deliverables(&self) -> &[FactoryProducedDeliverable] {
    &self.deliverables
  }

  /// Returns verified bounded successor data without creating a lifecycle record.
  #[must_use]
  pub const fn summary(&self) -> Option<&BoundedSummary> {
    self.summary.as_ref()
  }
}

/// Validates one terminal implementation Build and projects only bounded data.
///
/// `elapsed_millis` comes from the generic execution lifecycle rather than a
/// provider session. The output documents must be fetched by immutable
/// Artifact identity and are checked again before parsing.
#[must_use]
pub fn observe_codex_implementation(
  envelope: &FactoryTaskEnvelope,
  build: &FactoryBuildObservation,
  elapsed_millis: u64,
  runner_events: &[RunnerEventPayload],
  documents: &[FactoryCodexOutputDocument],
) -> FactoryCodexImplementationObservation {
  let validated = match validate_codex_run::<records::ImplementationReportWire>(
    envelope,
    FactoryTaskMode::Implement,
    build,
    elapsed_millis,
    runner_events,
    documents,
  ) {
    Ok(value) => value,
    Err(failure) => return failed(failure.status, failure.usage),
  };
  let report = match implementation_report(validated.harness_outcome, validated.structured_result) {
    Some(value) => value,
    None => return failed(FactoryCodexObservationStatus::InvalidSchema, validated.usage),
  };
  let usage = validated.usage;
  let deliverables = envelope
    .deliverables()
    .iter()
    .filter_map(|declaration| {
      validated.outputs.get(declaration.kind().as_str()).and_then(|identity| {
        artifact_reference(identity).map(|artifact| FactoryProducedDeliverable::new(declaration.clone(), artifact))
      })
    })
    .collect::<Vec<_>>();
  if deliverables.len() != envelope.deliverables().len() {
    return failed(FactoryCodexObservationStatus::IntegrityFailure, usage);
  }
  let result_reference = validated.result;
  let trace_reference = validated.trace;
  let provenance_reference = validated.provenance;
  let provenance_digest = provenance_reference.content_digest();
  let summary = BoundedSummary::new(envelope.subject().clone(), report.summary.clone(), provenance_digest);
  let Ok(summary_bytes) = summary.canonical_bytes() else {
    return failed(FactoryCodexObservationStatus::InvalidSchema, usage);
  };
  let Some(summary_artifact) = deliverables
    .iter()
    .find(|deliverable| deliverable.declaration().kind().as_str() == CODEX_STAGE_SUMMARY)
    .map(FactoryProducedDeliverable::artifact)
  else {
    return failed(FactoryCodexObservationStatus::MissingDeliverable, usage);
  };
  if summary_artifact.content_digest() != octacity_server_factory::FactoryDigest::content_sha256(&summary_bytes)
    || summary_artifact.encoded_size() != u64::try_from(summary_bytes.len()).unwrap_or(u64::MAX)
  {
    return failed(FactoryCodexObservationStatus::IntegrityFailure, usage);
  }
  FactoryCodexImplementationObservation {
    status: FactoryCodexObservationStatus::Succeeded,
    usage,
    report: Some(report),
    result: Some(result_reference),
    trace: Some(trace_reference),
    provenance: Some(provenance_reference),
    deliverables,
    summary: Some(summary),
  }
}

fn failed(status: FactoryCodexObservationStatus, usage: BudgetUsage) -> FactoryCodexImplementationObservation {
  FactoryCodexImplementationObservation {
    status,
    usage,
    report: None,
    result: None,
    trace: None,
    provenance: None,
    deliverables: Vec::new(),
    summary: None,
  }
}
