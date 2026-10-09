//! Shared validation of generic Build outputs from the pinned Codex harness.

use std::collections::BTreeMap;

use octacity_protocol::RunnerEventPayload;
use octacity_server_artifacts::ArtifactIdentity;
use octacity_server_factory::{BudgetUsage, FactoryArtifactReference, FactoryTaskEnvelope, FactoryTaskMode};
use octacity_server_orchestrator::BuildState;
use serde::de::DeserializeOwned;

use super::{
  CodexHarnessOutcome, FactoryCodexObservationStatus, FactoryCodexOutputDocument,
  evidence::{
    artifact_reference, document_map, output_map, record_output_types, valid_provenance,
    valid_provider_unavailable_events, valid_runner_events, valid_trace,
  },
  records::{CodexProvenanceRecord, CodexResultRecord},
};
use crate::{
  FactoryBuildObservation,
  factory_codex_contract::{CODEX_RUN_PROVENANCE, CODEX_RUN_RESULT, CODEX_RUN_TRACE},
};

pub(super) struct ValidatedCodexRun<T> {
  pub(super) harness_outcome: CodexHarnessOutcome,
  pub(super) structured_result: T,
  pub(super) usage: BudgetUsage,
  pub(super) outputs: BTreeMap<String, ArtifactIdentity>,
  pub(super) result: FactoryArtifactReference,
  pub(super) trace: FactoryArtifactReference,
  pub(super) provenance: FactoryArtifactReference,
}

pub(super) struct CodexRunFailure {
  pub(super) status: FactoryCodexObservationStatus,
  pub(super) usage: BudgetUsage,
}

pub(super) fn validate_codex_run<T: DeserializeOwned>(
  envelope: &FactoryTaskEnvelope,
  expected_mode: FactoryTaskMode,
  build: &FactoryBuildObservation,
  elapsed_millis: u64,
  runner_events: &[RunnerEventPayload],
  supplied_documents: &[FactoryCodexOutputDocument],
) -> Result<ValidatedCodexRun<T>, CodexRunFailure> {
  let output_bytes = build
    .outputs()
    .iter()
    .try_fold(0_u64, |total, output| total.checked_add(output.size_bytes))
    .ok_or_else(|| failure(FactoryCodexObservationStatus::OutputOverflow, BudgetUsage::default()))?;
  let base_usage = BudgetUsage {
    attempts: 1,
    elapsed_millis,
    output_bytes,
    ..BudgetUsage::default()
  };
  validate_terminal(envelope, expected_mode, build, runner_events, base_usage)?;
  if base_usage.output_bytes > envelope.budget().max_output_bytes() {
    return Err(failure(FactoryCodexObservationStatus::OutputOverflow, base_usage));
  }
  if !valid_runner_events(runner_events) {
    return Err(failure(FactoryCodexObservationStatus::InvalidSchema, base_usage));
  }
  let outputs =
    output_map(build.outputs()).ok_or_else(|| failure(FactoryCodexObservationStatus::InvalidSchema, base_usage))?;
  let required = envelope.deliverables().iter().map(|item| item.kind().as_str()).chain([
    CODEX_RUN_TRACE,
    CODEX_RUN_PROVENANCE,
    CODEX_RUN_RESULT,
  ]);
  if required.into_iter().any(|name| !outputs.contains_key(name)) {
    return Err(failure(FactoryCodexObservationStatus::MissingDeliverable, base_usage));
  }
  let documents = document_map(supplied_documents, &outputs)
    .ok_or_else(|| failure(FactoryCodexObservationStatus::IntegrityFailure, base_usage))?;
  let result_document = required_document(&documents, CODEX_RUN_RESULT, base_usage)?;
  let trace_document = required_document(&documents, CODEX_RUN_TRACE, base_usage)?;
  let provenance_document = required_document(&documents, CODEX_RUN_PROVENANCE, base_usage)?;
  if !record_output_types(result_document, trace_document, provenance_document) {
    return Err(failure(FactoryCodexObservationStatus::InvalidSchema, base_usage));
  }
  let result: CodexResultRecord<T> = parse_record(&result_document.bytes, base_usage)?;
  let provenance: CodexProvenanceRecord = parse_record(&provenance_document.bytes, base_usage)?;
  if !valid_provenance(envelope, &result, &provenance) || !valid_trace(&trace_document.bytes) {
    return Err(failure(FactoryCodexObservationStatus::InvalidSchema, base_usage));
  }
  let tokens = result
    .usage
    .get("input_tokens")
    .copied()
    .unwrap_or(0)
    .checked_add(result.usage.get("output_tokens").copied().unwrap_or(0))
    .ok_or_else(|| failure(FactoryCodexObservationStatus::BudgetExceeded, base_usage))?;
  let usage = BudgetUsage {
    tokens,
    cost_micro_units: result.usage.get("cost_micro_units").copied().unwrap_or(0),
    ..base_usage
  };
  if usage.validate(envelope.budget()).is_err() {
    return Err(failure(FactoryCodexObservationStatus::BudgetExceeded, usage));
  }
  let references = (
    artifact_reference(&result_document.identity),
    artifact_reference(&trace_document.identity),
    artifact_reference(&provenance_document.identity),
  );
  let (Some(result_reference), Some(trace_reference), Some(provenance_reference)) = references else {
    return Err(failure(FactoryCodexObservationStatus::IntegrityFailure, usage));
  };
  Ok(ValidatedCodexRun {
    harness_outcome: result.outcome.into(),
    structured_result: result.structured_result,
    usage,
    outputs: outputs
      .into_iter()
      .map(|(name, identity)| (name.to_owned(), identity.clone()))
      .collect(),
    result: result_reference,
    trace: trace_reference,
    provenance: provenance_reference,
  })
}

fn validate_terminal(
  envelope: &FactoryTaskEnvelope,
  expected_mode: FactoryTaskMode,
  build: &FactoryBuildObservation,
  runner_events: &[RunnerEventPayload],
  usage: BudgetUsage,
) -> Result<(), CodexRunFailure> {
  if envelope.mode() != expected_mode || !build.state().is_terminal() {
    return Err(failure(FactoryCodexObservationStatus::InvalidSchema, usage));
  }
  match build.state() {
    BuildState::Cancelled => Err(failure(FactoryCodexObservationStatus::Cancelled, usage)),
    BuildState::Failed if build.timed_out() => Err(failure(FactoryCodexObservationStatus::TimedOut, usage)),
    BuildState::Failed if valid_provider_unavailable_events(runner_events) => {
      Err(failure(FactoryCodexObservationStatus::ProviderUnavailable, usage))
    }
    BuildState::Failed if build.infrastructure_retry_eligible() => {
      Err(failure(FactoryCodexObservationStatus::InfrastructureFailed, usage))
    }
    BuildState::Failed => Err(failure(FactoryCodexObservationStatus::ExecutionFailed, usage)),
    BuildState::Succeeded => Ok(()),
    BuildState::Queued | BuildState::Running => Err(failure(FactoryCodexObservationStatus::InvalidSchema, usage)),
  }
}

fn required_document<'a>(
  documents: &'a BTreeMap<&str, &'a FactoryCodexOutputDocument>,
  name: &str,
  usage: BudgetUsage,
) -> Result<&'a FactoryCodexOutputDocument, CodexRunFailure> {
  documents
    .get(name)
    .copied()
    .ok_or_else(|| failure(FactoryCodexObservationStatus::MissingDeliverable, usage))
}

fn parse_record<T: DeserializeOwned>(bytes: &[u8], usage: BudgetUsage) -> Result<T, CodexRunFailure> {
  serde_json::from_slice(bytes).map_err(|error| {
    let status = if error.is_syntax() || error.is_eof() {
      FactoryCodexObservationStatus::InvalidJson
    } else {
      FactoryCodexObservationStatus::InvalidSchema
    };
    failure(status, usage)
  })
}

const fn failure(status: FactoryCodexObservationStatus, usage: BudgetUsage) -> CodexRunFailure {
  CodexRunFailure { status, usage }
}
