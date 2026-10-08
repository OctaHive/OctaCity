use octacity_protocol::RunnerEventPayload;
use octacity_server_artifacts::{
  ArtifactContentDigest, ArtifactIdentity, ArtifactMediaType, ArtifactReportFormat, ArtifactRetentionPolicy,
  ArtifactType,
};
use octacity_server_domain::{ArtifactId, ArtifactName, AttemptId, BuildId, JobId, LeaseId, Timestamp};
use octacity_server_factory::{
  BoundedSummary, ContextManifestId, FactoryDigest, FactoryRunId, FactorySafeText, FactoryTaskMode,
  ImplementationOutcome, MacroCall, MacroCallId, MacroCallTerminal, StageHandoffId,
};
use octacity_server_orchestrator::BuildState;
use serde_json::{Value, json};

use crate::{
  CodexHarnessOutcome, FactoryBuildObservation, FactoryCodexObservationStatus, FactoryCodexOutputDocument,
  observe_codex_implementation,
};

use super::factory_codex_tests::{credential_profiles, permissions, reference, standard_deliverables, task_envelope};

struct Fixture {
  envelope: octacity_server_factory::FactoryTaskEnvelope,
  build: FactoryBuildObservation,
  documents: Vec<FactoryCodexOutputDocument>,
  runner_events: Vec<RunnerEventPayload>,
}

impl Fixture {
  fn successful() -> Self {
    let plugin = reference("codex", 10);
    let executable = reference("codex-cli", 11);
    let envelope = task_envelope(
      FactoryTaskMode::Implement,
      "Implement the selected stage and return only the required structured result.",
      permissions(FactoryTaskMode::Implement, plugin.clone(), executable.clone(), true),
      standard_deliverables(),
      plugin,
      executable,
    );
    let build_id = BuildId::generate();
    let attempt_id = AttemptId::generate();
    let job_id = JobId::generate();
    let lease_id = LeaseId::generate();
    let result = json!({
      "format_version": 1,
      "outcome": "completed",
      "structured_result": {
        "outcome": "succeeded",
        "summary": "Implemented the bounded change",
        "decisions": ["Keep the existing execution boundary"],
        "assumptions": ["The pinned toolchain remains installed"],
        "unresolved_items": [],
        "changed_components": ["server/application"],
        "validation_observations": ["Targeted tests passed"]
      },
      "harness_identifiers": {"thread_id": "thread-1"},
      "usage": {"input_tokens": 11, "output_tokens": 7}
    });
    let provenance = json!({
      "format_version": 1,
      "trace_format_version": 1,
      "result_format_version": 1,
      "plugin": {"name": "octa_plugin_codex", "version": "v1"},
      "codex": {"name": "codex-cli", "version": "v1"},
      "settings": {"model": "gpt-6-codex", "reasoning_effort": "high"},
      "prompt_digest": {
        "algorithm": "blake3",
        "value": crate::codex_prompt_provenance_digest(
          "Implement the selected stage and return only the required structured result."
        ).to_string()
      },
      "source_revision": "base-revision",
      "timing": {"started_unix_millis": 1000, "finished_unix_millis": 1010, "duration_millis": 10},
      "outcome": "completed",
      "usage": {"input_tokens": 11, "output_tokens": 7}
    });
    let trace = concat!(
      "{\"format_version\":1,\"sequence\":0,\"event\":{\"type\":\"turn.started\"}}\n",
      "{\"format_version\":1,\"sequence\":1,\"event\":{\"type\":\"turn.completed\"}}\n"
    )
    .as_bytes()
    .to_vec();
    let result = serde_json::to_vec(&result).unwrap();
    let provenance = serde_json::to_vec(&provenance).unwrap();
    let mut outputs = vec![
      output(
        "codex-run-result",
        ArtifactType::Report(ArtifactReportFormat::new("octa.codex.result.v1").unwrap()),
        "application/json",
        &result,
        build_id,
        attempt_id,
        job_id,
        lease_id,
      ),
      output(
        "codex-run-trace",
        ArtifactType::Artifact,
        "application/x-ndjson",
        &trace,
        build_id,
        attempt_id,
        job_id,
        lease_id,
      ),
      output(
        "codex-run-provenance",
        ArtifactType::Artifact,
        "application/json",
        &provenance,
        build_id,
        attempt_id,
        job_id,
        lease_id,
      ),
    ];
    outputs.push(output(
      "changeset",
      ArtifactType::Artifact,
      "application/octet-stream",
      &[1],
      build_id,
      attempt_id,
      job_id,
      lease_id,
    ));
    let provenance_digest = FactoryDigest::from_bytes(outputs[2].digest.as_bytes());
    let summary = BoundedSummary::new(
      envelope.subject().clone(),
      FactorySafeText::new("Implemented the bounded change").unwrap(),
      provenance_digest,
    );
    let summary_bytes = summary.canonical_bytes().unwrap();
    outputs.push(output(
      "stage-summary",
      ArtifactType::Artifact,
      "application/json",
      &summary_bytes,
      build_id,
      attempt_id,
      job_id,
      lease_id,
    ));
    let documents = outputs
      .iter()
      .filter_map(|identity| match identity.logical_name.as_str() {
        "codex-run-result" => Some(FactoryCodexOutputDocument {
          identity: identity.clone(),
          bytes: result.clone(),
        }),
        "codex-run-trace" => Some(FactoryCodexOutputDocument {
          identity: identity.clone(),
          bytes: trace.clone(),
        }),
        "codex-run-provenance" => Some(FactoryCodexOutputDocument {
          identity: identity.clone(),
          bytes: provenance.clone(),
        }),
        _ => None,
      })
      .collect();
    Self {
      envelope,
      build: FactoryBuildObservation::fixture(build_id, BuildState::Succeeded, attempt_id, vec![job_id], outputs),
      documents,
      runner_events: vec![
        runner_event(0, json!({"type": "run_started", "run_id": 7, "command": "implement"})),
        runner_event(
          1,
          json!({"type": "run_finished", "run_id": 7, "command": "implement", "status": "success"}),
        ),
      ],
    }
  }

  fn replace_document(&mut self, name: &str, value: Value) {
    let document = self
      .documents
      .iter_mut()
      .find(|document| document.identity.logical_name.as_str() == name)
      .unwrap();
    document.bytes = serde_json::to_vec(&value).unwrap();
    document.identity.size_bytes = document.bytes.len() as u64;
    document.identity.digest = digest(&document.bytes);
    let output = self
      .build
      .outputs_mut()
      .iter_mut()
      .find(|output| output.logical_name.as_str() == name)
      .unwrap();
    *output = document.identity.clone();
  }
}

#[test]
fn factory_implementation_build_captures_typed_result_usage_trace_and_provenance() {
  let fixture = Fixture::successful();
  let compiled = crate::compile_codex_task(
    &fixture.envelope,
    "Implement the selected stage and return only the required structured result.",
    &credential_profiles(),
  )
  .unwrap();
  assert_eq!(compiled.task_name(), "implement");
  let observation = observe_codex_implementation(
    &fixture.envelope,
    StageHandoffId::generate(),
    &fixture.build,
    10,
    &fixture.runner_events,
    &fixture.documents,
  );

  assert_eq!(observation.status(), FactoryCodexObservationStatus::Succeeded);
  assert_eq!(observation.usage().tokens, 18);
  assert_eq!(observation.usage().elapsed_millis, 10);
  assert_eq!(observation.deliverables().len(), 2);
  assert!(observation.result().is_some());
  assert!(observation.trace().is_some());
  assert!(observation.provenance().is_some());
  assert!(observation.handoff().is_some());
  let report = observation.report().unwrap();
  assert_eq!(report.harness_outcome(), CodexHarnessOutcome::Completed);
  assert_eq!(report.outcome(), ImplementationOutcome::Succeeded);
  assert_eq!(report.summary().as_str(), "Implemented the bounded change");
  assert_eq!(report.changed_components()[0].as_str(), "server/application");

  let call: MacroCall = serde_json::from_value(json!({
    "id": MacroCallId::generate(),
    "stage_attempt_id": fixture.envelope.stage_attempt_id(),
    "run_id": FactoryRunId::generate(),
    "subject": fixture.envelope.subject(),
    "kind": "implement",
    "context_manifest_id": ContextManifestId::generate(),
    "context_digest": FactoryDigest::content_sha256(&[20]),
    "budget": fixture.envelope.budget(),
    "parent_id": null,
    "stage_dependencies": [],
    "call_dependencies": [],
    "depth": 1
  }))
  .unwrap();
  let append = observation
    .history_append(&call, &fixture.envelope, Timestamp::from_unix_millis(1_100).unwrap())
    .unwrap();
  assert_eq!(append.macro_call_completions.len(), 1);
  assert_eq!(
    append.macro_call_completions[0].terminal(),
    MacroCallTerminal::Succeeded
  );
  assert_eq!(append.stage_handoffs.len(), 1);
}

#[test]
fn execution_terminal_states_remain_distinct_without_partial_typed_output() {
  for (state, timed_out, infrastructure, expected) in [
    (BuildState::Failed, true, false, FactoryCodexObservationStatus::TimedOut),
    (
      BuildState::Cancelled,
      false,
      false,
      FactoryCodexObservationStatus::Cancelled,
    ),
    (
      BuildState::Failed,
      false,
      true,
      FactoryCodexObservationStatus::InfrastructureFailed,
    ),
    (
      BuildState::Failed,
      false,
      false,
      FactoryCodexObservationStatus::ExecutionFailed,
    ),
  ] {
    let mut fixture = Fixture::successful();
    fixture.build.set_state(state);
    fixture.build.set_terminal_cause(timed_out, infrastructure);
    let observation = observe_codex_implementation(
      &fixture.envelope,
      StageHandoffId::generate(),
      &fixture.build,
      50,
      &[],
      &[],
    );
    assert_eq!(observation.status(), expected);
    assert!(observation.report().is_none());
    assert!(observation.trace().is_none());
  }
}

#[test]
fn provider_outage_requires_one_well_formed_failed_runner_sequence() {
  let mut fixture = Fixture::successful();
  fixture.build.set_state(BuildState::Failed);
  let events = vec![
    runner_event(0, json!({"type": "run_started", "run_id": 7, "command": "implement"})),
    runner_event_with_category(1, "diagnostic", json!({"type": "provider_unavailable"})),
    runner_event(
      2,
      json!({"type": "run_finished", "run_id": 7, "command": "implement", "status": "failed"}),
    ),
  ];
  assert_eq!(
    observe_codex_implementation(
      &fixture.envelope,
      StageHandoffId::generate(),
      &fixture.build,
      50,
      &events,
      &[],
    )
    .status(),
    FactoryCodexObservationStatus::ProviderUnavailable
  );
}

#[test]
fn invalid_json_schema_and_missing_deliverables_are_not_collapsed() {
  let mut invalid_json = Fixture::successful();
  let result = invalid_json
    .documents
    .iter_mut()
    .find(|document| document.identity.logical_name.as_str() == "codex-run-result")
    .unwrap();
  result.bytes = b"{".to_vec();
  result.identity.size_bytes = 1;
  result.identity.digest = digest(&result.bytes);
  *invalid_json
    .build
    .outputs_mut()
    .iter_mut()
    .find(|output| output.logical_name.as_str() == "codex-run-result")
    .unwrap() = result.identity.clone();
  assert_eq!(
    observe_codex_implementation(
      &invalid_json.envelope,
      StageHandoffId::generate(),
      &invalid_json.build,
      1,
      &invalid_json.runner_events,
      &invalid_json.documents,
    )
    .status(),
    FactoryCodexObservationStatus::InvalidJson
  );

  let mut invalid_schema = Fixture::successful();
  invalid_schema.replace_document(
    "codex-run-result",
    json!({
      "format_version": 1,
      "outcome": "completed",
      "structured_result": {"outcome": "succeeded"},
      "harness_identifiers": {},
      "usage": {}
    }),
  );
  assert_eq!(
    observe_codex_implementation(
      &invalid_schema.envelope,
      StageHandoffId::generate(),
      &invalid_schema.build,
      1,
      &invalid_schema.runner_events,
      &invalid_schema.documents,
    )
    .status(),
    FactoryCodexObservationStatus::InvalidSchema
  );

  let mut missing = Fixture::successful();
  missing
    .build
    .outputs_mut()
    .retain(|output| output.logical_name.as_str() != "stage-summary");
  assert_eq!(
    observe_codex_implementation(
      &missing.envelope,
      StageHandoffId::generate(),
      &missing.build,
      1,
      &missing.runner_events,
      &missing.documents,
    )
    .status(),
    FactoryCodexObservationStatus::MissingDeliverable
  );
}

#[test]
fn output_identity_and_trace_structure_are_revalidated_before_acceptance() {
  let mut tampered = Fixture::successful();
  tampered.documents[0].bytes.push(b' ');
  assert_eq!(
    observe_codex_implementation(
      &tampered.envelope,
      StageHandoffId::generate(),
      &tampered.build,
      1,
      &tampered.runner_events,
      &tampered.documents,
    )
    .status(),
    FactoryCodexObservationStatus::IntegrityFailure
  );

  let mut invalid_trace = Fixture::successful();
  let trace = invalid_trace
    .documents
    .iter_mut()
    .find(|document| document.identity.logical_name.as_str() == "codex-run-trace")
    .unwrap();
  trace.bytes = b"{\"format_version\":1,\"sequence\":9,\"event\":{}}\n".to_vec();
  trace.identity.size_bytes = trace.bytes.len() as u64;
  trace.identity.digest = digest(&trace.bytes);
  *invalid_trace
    .build
    .outputs_mut()
    .iter_mut()
    .find(|output| output.logical_name.as_str() == "codex-run-trace")
    .unwrap() = trace.identity.clone();
  assert_eq!(
    observe_codex_implementation(
      &invalid_trace.envelope,
      StageHandoffId::generate(),
      &invalid_trace.build,
      1,
      &invalid_trace.runner_events,
      &invalid_trace.documents,
    )
    .status(),
    FactoryCodexObservationStatus::InvalidSchema
  );

  let mut sensitive_trace = Fixture::successful();
  let trace = sensitive_trace
    .documents
    .iter_mut()
    .find(|document| document.identity.logical_name.as_str() == "codex-run-trace")
    .unwrap();
  trace.bytes =
    b"{\"format_version\":1,\"sequence\":0,\"event\":{\"type\":\"turn.failed\",\"token\":\"sk-proj-secret\"}}\n"
      .to_vec();
  trace.identity.size_bytes = trace.bytes.len() as u64;
  trace.identity.digest = digest(&trace.bytes);
  *sensitive_trace
    .build
    .outputs_mut()
    .iter_mut()
    .find(|output| output.logical_name.as_str() == "codex-run-trace")
    .unwrap() = trace.identity.clone();
  assert_eq!(
    observe_codex_implementation(
      &sensitive_trace.envelope,
      StageHandoffId::generate(),
      &sensitive_trace.build,
      1,
      &sensitive_trace.runner_events,
      &sensitive_trace.documents,
    )
    .status(),
    FactoryCodexObservationStatus::InvalidSchema
  );
}

#[test]
fn overflowing_output_accounting_is_terminal_without_parsing_documents() {
  let mut fixture = Fixture::successful();
  fixture.build.outputs_mut()[0].size_bytes = u64::MAX;
  fixture.build.outputs_mut()[1].size_bytes = 1;
  assert_eq!(
    observe_codex_implementation(
      &fixture.envelope,
      StageHandoffId::generate(),
      &fixture.build,
      1,
      &fixture.runner_events,
      &fixture.documents,
    )
    .status(),
    FactoryCodexObservationStatus::OutputOverflow
  );
}

#[allow(clippy::too_many_arguments)]
fn output(
  name: &str,
  artifact_type: ArtifactType,
  media_type: &str,
  bytes: &[u8],
  build_id: BuildId,
  attempt_id: AttemptId,
  job_id: JobId,
  lease_id: LeaseId,
) -> ArtifactIdentity {
  ArtifactIdentity {
    artifact_id: ArtifactId::generate(),
    build_id,
    attempt_id,
    job_id,
    lease_id,
    logical_name: ArtifactName::new(name).unwrap(),
    artifact_type,
    media_type: ArtifactMediaType::new(media_type).unwrap(),
    size_bytes: bytes.len() as u64,
    digest: digest(bytes),
    retention: ArtifactRetentionPolicy::Keep,
  }
}

fn digest(bytes: &[u8]) -> ArtifactContentDigest {
  ArtifactContentDigest::from_bytes(octacity_server_factory::FactoryDigest::content_sha256(bytes).as_bytes())
}

fn runner_event(sequence: u64, data: Value) -> RunnerEventPayload {
  runner_event_with_category(sequence, "execution", data)
}

fn runner_event_with_category(sequence: u64, category: &str, data: Value) -> RunnerEventPayload {
  RunnerEventPayload {
    schema_version: 4,
    sequence,
    timestamp: "2026-10-09T00:00:00Z".to_owned(),
    category: category.to_owned(),
    data: data.as_object().unwrap().clone(),
  }
}
