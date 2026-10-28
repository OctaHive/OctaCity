use octacity_protocol::RunnerEventPayload;
use octacity_server_artifacts::{
  ArtifactContentDigest, ArtifactIdentity, ArtifactMediaType, ArtifactReportFormat, ArtifactRetentionPolicy,
  ArtifactType,
};
use octacity_server_domain::{ArtifactId, ArtifactName, AttemptId, BuildId, JobId, LeaseId, Timestamp};
use octacity_server_factory::{
  AssessmentId, AssessmentOutcome, BoundedSummary, BudgetLimit, EvaluationPlan, EvaluationPlanId, EvidenceManifest,
  EvidenceManifestId, FactoryDigest, FactoryKey, FactorySafeText, FactoryTaskMode, ImmutableReference,
  ImplementationOutcome,
};
use octacity_server_orchestrator::BuildState;
use serde_json::{Value, json};

use crate::{
  CodexHarnessOutcome, FactoryBuildObservation, FactoryCodexObservationStatus, FactoryCodexOutputDocument,
  FactoryCodexReviewerBinding, FactoryCodexTerminalObservation, observe_codex_evaluation, observe_codex_implementation,
};

use super::factory_codex_tests::{credential_profiles, permissions, reference, standard_deliverables, task_envelope};

#[path = "factory_mcp_evidence_tests.rs"]
mod mcp_evidence;

fn node_observation_fixture() -> (
  Fixture,
  octacity_server_factory::FlowBuildProfile,
  octacity_server_factory::FlowDataSchema,
) {
  use octacity_server_factory::{FlowBuildIntent, FlowNodeKind};
  let mut fixture = Fixture::successful();
  let mut result: Value = serde_json::from_slice(&fixture.documents[0].bytes).unwrap();
  result["structured_result"] = json!({"count":9});
  result["usage"]["cost_micro_units"] = json!(2);
  let mut provenance: Value = serde_json::from_slice(&fixture.documents[2].bytes).unwrap();
  provenance["usage"] = result["usage"].clone();
  fixture.replace_document("codex-run-result", result);
  fixture.replace_document("codex-run-provenance", provenance);
  let (_, admitted, schema, input, node) =
    crate::factory_node_test_support::fixtures::execution_fixture(FlowNodeKind::Reasoning);
  let intent = FlowBuildIntent::new(input, &admitted, node, Default::default(), 0).unwrap();
  let mut profile = intent.profile().clone();
  profile.tool = fixture.envelope.executable().clone();
  profile.plugin = fixture.envelope.plugin().clone();
  profile.model_or_tool = fixture.envelope.model().clone();
  profile.result_output = FactoryKey::new("codex-run-result").unwrap();
  (fixture, profile, schema)
}

#[test]
fn codex_node_usage_prices_reported_tokens_under_the_frozen_exact_model_policy() {
  use crate::{FactoryCodexNodeRunContract, FactoryCodexUsagePolicy, verify_codex_node_run};
  let (mut fixture, profile, schema) = node_observation_fixture();
  let mut result: Value = serde_json::from_slice(&fixture.documents[0].bytes).unwrap();
  result["usage"] = json!({"input_tokens":12,"cached_input_tokens":2,"output_tokens":6});
  let mut provenance: Value = serde_json::from_slice(&fixture.documents[2].bytes).unwrap();
  provenance["usage"] = result["usage"].clone();
  fixture.replace_document("codex-run-result", result);
  fixture.replace_document("codex-run-provenance", provenance);
  let policy = FactoryCodexUsagePolicy::TokenRates {
    model: profile.model_or_tool.clone(),
    input_micro_units_per_million: 2_000_000,
    cached_input_micro_units_per_million: 1_000_000,
    output_micro_units_per_million: 5_000_000,
  };
  let observed = verify_codex_node_run(
    FactoryCodexNodeRunContract {
      model_identifier: None,
      profile: &profile,
      schema: &schema,
      prompt_digest: fixture.envelope.digests().prompt_provenance,
      source_revision: fixture.envelope.subject().exact().base_revision(),
      budget: fixture.envelope.budget(),
      usage_policy: &policy,
      execution_window: Timestamp::from_unix_millis(1000).unwrap()..=Timestamp::from_unix_millis(1010).unwrap(),
    },
    &fixture.documents,
  )
  .unwrap();
  assert_eq!(observed.usage.tokens, 18);
  assert_eq!(observed.usage.cost_micro_units, 52);
}

#[test]
fn codex_node_observation_verifies_exact_execution_provenance_and_projects_only_declared_data() {
  use crate::{FactoryCodexNodeRunContract, verify_codex_node_run};
  let (fixture, profile, schema) = node_observation_fixture();
  let expected = || FactoryCodexNodeRunContract {
    model_identifier: None,
    usage_policy: &crate::FactoryCodexUsagePolicy::ReportedCost,
    profile: &profile,
    schema: &schema,
    prompt_digest: fixture.envelope.digests().prompt_provenance,
    source_revision: fixture.envelope.subject().exact().base_revision(),
    budget: fixture.envelope.budget(),
    execution_window: Timestamp::from_unix_millis(1000).unwrap()..=Timestamp::from_unix_millis(1010).unwrap(),
  };
  let observed = verify_codex_node_run(expected(), &fixture.documents).unwrap();
  assert_eq!(observed.outcome, CodexHarnessOutcome::Completed);
  assert_eq!(observed.data.unwrap().payload().value(), &json!({"count":9}));
  assert_eq!(observed.usage.tokens, 18);
  assert_eq!(observed.usage.cost_micro_units, 2);
  assert_eq!(observed.usage.elapsed_millis, 10);
  assert_eq!(
    observed.usage.output_bytes,
    fixture
      .documents
      .iter()
      .map(|document| document.bytes.len() as u64)
      .sum::<u64>()
  );
}

#[test]
fn codex_node_execution_failure_keeps_measured_usage_without_a_semantic_result() {
  use crate::{FactoryCodexNodeRunContract, verify_codex_node_run};
  let (mut fixture, profile, schema) = node_observation_fixture();
  let mut result: Value = serde_json::from_slice(&fixture.documents[0].bytes).unwrap();
  result["outcome"] = json!("blocked");
  result["structured_result"] = Value::Null;
  let mut provenance: Value = serde_json::from_slice(&fixture.documents[2].bytes).unwrap();
  provenance["outcome"] = result["outcome"].clone();
  fixture.replace_document("codex-run-result", result);
  fixture.replace_document("codex-run-provenance", provenance);
  let observed = verify_codex_node_run(
    FactoryCodexNodeRunContract {
      model_identifier: None,
      usage_policy: &crate::FactoryCodexUsagePolicy::ReportedCost,
      profile: &profile,
      schema: &schema,
      prompt_digest: fixture.envelope.digests().prompt_provenance,
      source_revision: fixture.envelope.subject().exact().base_revision(),
      budget: fixture.envelope.budget(),
      execution_window: Timestamp::from_unix_millis(1000).unwrap()..=Timestamp::from_unix_millis(1010).unwrap(),
    },
    &fixture.documents,
  )
  .expect("a verified technical failure still has measured consumption");
  assert_eq!(observed.outcome, CodexHarnessOutcome::Blocked);
  assert!(
    observed.data.is_none(),
    "technical status cannot fabricate a configured semantic outcome"
  );
  assert_eq!(observed.usage.tokens, 18);
  assert_eq!(observed.usage.cost_micro_units, 2);
}

pub(crate) struct Fixture {
  envelope: octacity_server_factory::FactoryTaskEnvelope,
  build: FactoryBuildObservation,
  pub(crate) documents: Vec<FactoryCodexOutputDocument>,
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

  pub(crate) fn replace_document(&mut self, name: &str, value: Value) {
    self.replace_document_bytes(name, serde_json::to_vec(&value).unwrap());
  }

  fn replace_document_bytes(&mut self, name: &str, bytes: Vec<u8>) {
    let document = self
      .documents
      .iter_mut()
      .find(|document| document.identity.logical_name.as_str() == name)
      .unwrap();
    document.bytes = bytes;
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

  fn successful_evaluation(outcome: &str, findings: Value) -> Self {
    const PROMPT: &str = "Review the exact candidate against the selected criteria and return only the assessment.";
    let mut fixture = Self::successful();
    let plugin = reference("codex", 10);
    let executable = reference("codex-cli", 11);
    fixture.envelope = task_envelope(
      FactoryTaskMode::Evaluate,
      PROMPT,
      permissions(FactoryTaskMode::Evaluate, plugin.clone(), executable.clone(), true),
      Vec::new(),
      plugin,
      executable,
    );
    fixture.build.outputs_mut().retain(|output| {
      matches!(
        output.logical_name.as_str(),
        "codex-run-result" | "codex-run-trace" | "codex-run-provenance"
      )
    });
    fixture.replace_document(
      "codex-run-result",
      json!({
        "format_version": 1,
        "outcome": "completed",
        "structured_result": {
          "outcome": outcome,
          "summary": "Independent review completed",
          "findings": findings
        },
        "harness_identifiers": {"thread_id": "review-thread-1"},
        "usage": {"input_tokens": 13, "output_tokens": 5}
      }),
    );
    fixture.replace_document(
      "codex-run-provenance",
      json!({
        "format_version": 1,
        "trace_format_version": 1,
        "result_format_version": 1,
        "plugin": {"name": "octa_plugin_codex", "version": "v1"},
        "codex": {"name": "codex-cli", "version": "v1"},
        "settings": {"model": "gpt-6-codex", "reasoning_effort": "high"},
        "prompt_digest": {
          "algorithm": "blake3",
          "value": crate::codex_prompt_provenance_digest(PROMPT).to_string()
        },
        "source_revision": "candidate-revision",
        "timing": {"started_unix_millis": 1000, "finished_unix_millis": 1010, "duration_millis": 10},
        "outcome": "completed",
        "usage": {"input_tokens": 13, "output_tokens": 5}
      }),
    );
    fixture.runner_events = vec![
      runner_event(0, json!({"type": "run_started", "run_id": 8, "command": "evaluate"})),
      runner_event(
        1,
        json!({"type": "run_finished", "run_id": 8, "command": "evaluate", "status": "success"}),
      ),
    ];
    fixture
  }
}

#[test]
fn independent_evaluation_build_projects_only_a_schema_valid_assessment() {
  let fixture = Fixture::successful_evaluation(
    "violated",
    json!([{
      "kind": "violation",
      "severity": "high",
      "summary": "Dependency direction is invalid",
      "evidence": ["tests"],
      "remediation": "Depend on the application port"
    }]),
  );
  let review = evaluation_records(&fixture.envelope, "review-code");
  let observation = observe_codex_evaluation(
    FactoryCodexReviewerBinding::new(
      &fixture.envelope,
      &review.plan,
      &review.evidence,
      &review.evaluator,
      AssessmentId::generate(),
    )
    .unwrap(),
    terminal(&fixture),
  );

  assert_eq!(observation.status(), FactoryCodexObservationStatus::Succeeded);
  assert_eq!(observation.result().unwrap().outcome(), AssessmentOutcome::Violated);
  let assessment = observation.assessment().expect("schema-valid assessment");
  assert_eq!(assessment.plan_id(), review.plan.id());
  assert_eq!(assessment.evaluator(), &key("review-code"));
  assert_eq!(assessment.outcome(), AssessmentOutcome::Violated);
  assert_eq!(assessment.findings().len(), 1);

  let compiled = crate::compile_codex_reviewer(
    &review.plan,
    &review.evaluator,
    &fixture.envelope,
    "Review the exact candidate against the selected criteria and return only the assessment.",
    &credential_profiles(),
  )
  .unwrap();
  assert_eq!(compiled.task_name(), "evaluate");
  assert_eq!(compiled.credential_profile(), &key("model-evaluation"));
  assert_eq!(
    crate::compile_codex_reviewer(
      &review.plan,
      &reference("review-code", 99),
      &fixture.envelope,
      "Review the exact candidate against the selected criteria and return only the assessment.",
      &credential_profiles(),
    ),
    Err(crate::CodexCompilationError::ReviewerBindings)
  );
  assert!(observation.result_artifact().is_some());
  assert!(observation.trace().is_some());
  assert!(observation.provenance().is_some());
}

#[test]
fn indeterminate_is_an_assessment_but_execution_terminals_are_not() {
  let fixture = Fixture::successful_evaluation(
    "indeterminate",
    json!([{
      "kind": "evidence_gap",
      "summary": "Performance evidence is insufficient",
      "evidence": ["tests"],
      "remediation": "Publish the required performance report"
    }]),
  );
  let review = evaluation_records(&fixture.envelope, "review-code");
  let observation = observe_codex_evaluation(
    FactoryCodexReviewerBinding::new(
      &fixture.envelope,
      &review.plan,
      &review.evidence,
      &review.evaluator,
      AssessmentId::generate(),
    )
    .unwrap(),
    terminal(&fixture),
  );
  assert_eq!(observation.status(), FactoryCodexObservationStatus::Succeeded);
  assert_eq!(
    observation.assessment().unwrap().outcome(),
    AssessmentOutcome::Indeterminate
  );

  for (state, timed_out, expected) in [
    (BuildState::Cancelled, false, FactoryCodexObservationStatus::Cancelled),
    (BuildState::Failed, true, FactoryCodexObservationStatus::TimedOut),
    (
      BuildState::Failed,
      false,
      FactoryCodexObservationStatus::ExecutionFailed,
    ),
  ] {
    let mut failed = Fixture::successful_evaluation("satisfied", json!([]));
    failed.build.set_state(state);
    failed.build.set_terminal_cause(timed_out, false);
    let failed_review = evaluation_records(&failed.envelope, "review-code");
    let observation = observe_codex_evaluation(
      FactoryCodexReviewerBinding::new(
        &failed.envelope,
        &failed_review.plan,
        &failed_review.evidence,
        &failed_review.evaluator,
        AssessmentId::generate(),
      )
      .unwrap(),
      FactoryCodexTerminalObservation {
        build: &failed.build,
        elapsed_millis: 10,
        runner_events: &[],
        documents: &[],
      },
    );
    assert_eq!(observation.status(), expected);
    assert!(observation.assessment().is_none());
  }
}

#[test]
fn factory_implementation_build_captures_typed_result_usage_trace_and_provenance() {
  let fixture = Fixture::successful();
  let compiled = crate::compile_codex_task(
    &fixture.envelope,
    "Implement the selected stage and return only the required structured result.",
    &credential_profiles(),
    None,
  )
  .unwrap();
  assert_eq!(compiled.task_name(), "implement");
  let observation = observe_codex_implementation(
    &fixture.envelope,
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
  assert!(observation.summary().is_some());
  let report = observation.report().unwrap();
  assert_eq!(report.harness_outcome(), CodexHarnessOutcome::Completed);
  assert_eq!(report.outcome(), ImplementationOutcome::Succeeded);
  assert_eq!(report.summary().as_str(), "Implemented the bounded change");
  assert_eq!(report.changed_components()[0].as_str(), "server/application");
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
    let observation = observe_codex_implementation(&fixture.envelope, &fixture.build, 50, &[], &[]);
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
    observe_codex_implementation(&fixture.envelope, &fixture.build, 50, &events, &[],).status(),
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
fn native_capture_handoff_keeps_partial_evidence_without_creating_a_model_result() {
  use crate::{FactoryNativeCaptureStatus, verify_codex_native_capture};
  let (_, profile, _) = node_observation_fixture();
  for (status, count, expected) in [
    ("complete", 3, FactoryNativeCaptureStatus::Complete),
    ("cancelled", 1, FactoryNativeCaptureStatus::Cancelled),
    ("failed", 0, FactoryNativeCaptureStatus::Failed),
  ] {
    let mut fixture = native_capture_fixture("retained MCP facts");
    let trace = std::str::from_utf8(&fixture.documents[1].bytes)
      .unwrap()
      .lines()
      .take(count)
      .flat_map(|line| line.bytes().chain(std::iter::once(b'\n')))
      .collect::<Vec<_>>();
    let mut provenance: Value = serde_json::from_slice(&fixture.documents[2].bytes).unwrap();
    provenance["capture"]["status"] = json!(status);
    provenance["capture"]["record_count"] = json!(count);
    provenance["capture"]["terminal_observed"] = json!(status == "complete");
    provenance["capture"]["trace_digest"]["value"] = json!(FactoryDigest::content_blake3(&trace).to_string());
    if status != "complete" {
      provenance["outcome"] = json!("failed");
      provenance["usage"] = json!({});
    }
    fixture.replace_document_bytes("codex-run-trace", trace);
    fixture.replace_document("codex-run-provenance", provenance);
    fixture
      .documents
      .retain(|document| document.identity.logical_name.as_str() != "codex-run-result");
    let captured =
      verify_codex_native_capture(native_capture_contract(&fixture, &profile), &fixture.documents).unwrap();
    assert_eq!(captured.status(), expected);
    assert_eq!(captured.record_count(), count as u64);
    assert_eq!(captured.trace(), &fixture.documents[0].identity);
    assert_eq!(captured.provenance(), &fixture.documents[1].identity);
  }
}

fn native_capture_contract<'a>(
  fixture: &'a Fixture,
  profile: &'a octacity_server_factory::FlowBuildProfile,
) -> crate::FactoryCodexNativeCaptureContract<'a> {
  let identity = &fixture.documents[0].identity;
  crate::FactoryCodexNativeCaptureContract {
    build_id: identity.build_id,
    attempt_id: identity.attempt_id,
    job_id: identity.job_id,
    provenance: crate::FactoryCodexProvenanceContract {
      model_identifier: None,
      plugin: &profile.plugin,
      executable: &profile.tool,
      model: &profile.model_or_tool,
      prompt_digest: fixture.envelope.digests().prompt_provenance,
      source_revision: fixture.envelope.subject().exact().base_revision().as_str(),
    },
    maximum_bytes: fixture.envelope.budget().max_output_bytes(),
    execution_window: Timestamp::from_unix_millis(1000).unwrap()..=Timestamp::from_unix_millis(1010).unwrap(),
  }
}

#[test]
fn native_capture_handoff_rejects_unsupported_or_misbound_evidence() {
  use crate::{ApplicationFailure, verify_codex_native_capture};
  let (_, profile, _) = node_observation_fixture();
  for mutation in [
    "legacy",
    "job",
    "profile",
    "window",
    "count",
    "digest",
    "coverage",
    "post_terminal",
  ] {
    let mut fixture = if mutation == "legacy" {
      Fixture::successful()
    } else {
      native_capture_fixture("facts")
    };
    match mutation {
      "job" => fixture.documents[1].identity.job_id = JobId::generate(),
      "profile" | "count" | "digest" | "coverage" => {
        let mut provenance: Value = serde_json::from_slice(&fixture.documents[2].bytes).unwrap();
        match mutation {
          "profile" => provenance["plugin"]["version"] = json!("another-plugin"),
          "count" => provenance["capture"]["record_count"] = json!(99),
          "digest" => provenance["capture"]["trace_digest"]["value"] = json!("00".repeat(32)),
          _ => provenance["capture"]["coverage"] = json!("all_remote_issues"),
        }
        fixture.replace_document("codex-run-provenance", provenance);
      }
      "post_terminal" => {
        let mut trace = fixture.documents[1].bytes.clone();
        trace.extend_from_slice(b"{\"format_version\":2,\"sequence\":3,\"observed_at_unix_millis\":1010,\"event\":{\"type\":\"item.started\",\"item\":{\"type\":\"mcp_tool_call\"}}}\n");
        let mut provenance: Value = serde_json::from_slice(&fixture.documents[2].bytes).unwrap();
        provenance["capture"]["record_count"] = json!(4);
        provenance["capture"]["trace_digest"]["value"] = json!(FactoryDigest::content_blake3(&trace).to_string());
        fixture.replace_document_bytes("codex-run-trace", trace);
        fixture.replace_document("codex-run-provenance", provenance);
      }
      _ => {}
    }
    let mut contract = native_capture_contract(&fixture, &profile);
    if mutation == "window" {
      contract.execution_window =
        Timestamp::from_unix_millis(1000).unwrap()..=Timestamp::from_unix_millis(1004).unwrap();
    }
    let expected = if mutation == "legacy" {
      ApplicationFailure::CapabilityUnavailable
    } else {
      ApplicationFailure::Invalid
    };
    match verify_codex_native_capture(contract, &fixture.documents) {
      Ok(_) => panic!("{mutation} must not establish native capture"),
      Err(error) => assert_eq!(error.classification(), expected, "{mutation}"),
    }
  }
}

#[test]
fn native_capture_v2_preserves_verified_mcp_trace_provenance() {
  let fixture = native_capture_fixture("candidate evidence");
  let observed = observe_codex_implementation(
    &fixture.envelope,
    &fixture.build,
    10,
    &fixture.runner_events,
    &fixture.documents,
  );
  assert_eq!(observed.status(), FactoryCodexObservationStatus::Succeeded);
  assert!(observed.trace().is_some());
  assert!(observed.provenance().is_some());
}

#[test]
fn native_capture_preserves_multiline_mcp_results() {
  let fixture = native_capture_fixture("issue title\nretained details\tand a quoted line\r\n");
  let observed = observe_codex_implementation(
    &fixture.envelope,
    &fixture.build,
    10,
    &fixture.runner_events,
    &fixture.documents,
  );
  assert_eq!(observed.status(), FactoryCodexObservationStatus::Succeeded);
}

pub(crate) fn native_capture_fixture(result_text: &str) -> Fixture {
  let mut fixture = Fixture::successful();
  let item = json!({"id":"call-1","type":"mcp_tool_call","server":"issues","tool":"lookup",
    "arguments":{"query":"candidate"},"status":"in_progress","result":null,"error":null});
  let mut completed = item.clone();
  completed["status"] = json!("completed");
  completed["result"] = json!({"content":[{"type":"text","text":result_text}],"structured_content":null});
  let records = [
    json!({"format_version":2,"sequence":0,"observed_at_unix_millis":1001,"event":{"type":"item.started","item":item}}),
    json!({"format_version":2,"sequence":1,"observed_at_unix_millis":1005,"event":{"type":"item.completed","item":completed}}),
    json!({"format_version":2,"sequence":2,"observed_at_unix_millis":1009,"event":{"type":"turn.completed"}}),
  ];
  let trace = records
    .iter()
    .flat_map(|record| {
      let mut bytes = serde_json::to_vec(record).unwrap();
      bytes.push(b'\n');
      bytes
    })
    .collect::<Vec<_>>();
  let mut provenance: Value = serde_json::from_slice(&fixture.documents[2].bytes).unwrap();
  provenance["format_version"] = json!(2);
  provenance["trace_format_version"] = json!(2);
  provenance["capture"] = json!({"status":"complete","coverage":"emitted_native_events",
    "terminal_observed":true,"record_count":3,"trace_digest":{"algorithm":"blake3","value":FactoryDigest::content_blake3(&trace).to_string()}});
  fixture.replace_document_bytes("codex-run-trace", trace);
  fixture.replace_document("codex-run-provenance", provenance);
  // The implementation summary is independently bound to the exact provenance
  // bytes, so publishing the v2 document also requires its new summary identity.
  let provenance_digest = FactoryDigest::from_bytes(fixture.documents[2].identity.digest.as_bytes());
  let summary = BoundedSummary::new(
    fixture.envelope.subject().clone(),
    FactorySafeText::new("Implemented the bounded change").unwrap(),
    provenance_digest,
  )
  .canonical_bytes()
  .unwrap();
  let output = fixture
    .build
    .outputs_mut()
    .iter_mut()
    .find(|output| output.logical_name.as_str() == "stage-summary")
    .unwrap();
  output.digest = digest(&summary);
  output.size_bytes = summary.len() as u64;
  fixture
}

#[test]
fn output_identity_and_trace_structure_are_revalidated_before_acceptance() {
  let mut tampered = Fixture::successful();
  tampered.documents[0].bytes.push(b' ');
  assert_eq!(
    observe_codex_implementation(
      &tampered.envelope,
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

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}

struct EvaluationRecords {
  evidence: EvidenceManifest,
  plan: EvaluationPlan,
  evaluator: ImmutableReference,
}

fn evaluation_records(envelope: &octacity_server_factory::FactoryTaskEnvelope, evaluator: &str) -> EvaluationRecords {
  let schema = reference("test-schema", 36);
  let tool = reference("test-tool", 37);
  let plugin = reference("test-plugin", 38);
  let evidence_artifact = json!({
    "artifact_id": ArtifactId::generate(),
    "content_digest": FactoryDigest::from_bytes([39; 32]),
    "encoded_size": 128
  });
  let evidence: EvidenceManifest = serde_json::from_value(json!({
    "id": EvidenceManifestId::generate(),
    "changeset_id": octacity_server_factory::ChangeSetId::generate(),
    "subject": envelope.subject().candidate().unwrap(),
    "constructed_at": Timestamp::from_unix_millis(1_000).unwrap(),
    "requirements": [{
      "kind": "tests",
      "output_kind": "report",
      "schema": schema,
      "tool": tool,
      "plugin": plugin
    }],
    "items": [{
      "subject": envelope.subject().candidate().unwrap(),
      "kind": "tests",
      "output_kind": "report",
      "artifact": evidence_artifact,
      "schema": schema,
      "producer": {
        "build_id": BuildId::generate(),
        "attempt_id": AttemptId::generate(),
        "job_id": JobId::generate(),
        "tool": tool,
        "plugin": plugin
      },
      "outcome": "passed",
      "published_at": Timestamp::from_unix_millis(900).unwrap(),
      "fresh_until": Timestamp::from_unix_millis(100_000).unwrap()
    }]
  }))
  .unwrap();
  let evaluator = reference(evaluator, 41);
  let criterion_digest = FactoryDigest::from_bytes([43; 32]);
  let plan: EvaluationPlan = serde_json::from_value(json!({
    "id": EvaluationPlanId::generate(),
    "evidence_id": evidence.id(),
    "purpose": "code_review",
    "subject": envelope.subject().candidate().unwrap(),
    "criterion_packs": [{
      "project_id": envelope.subject().candidate().unwrap().exact().project_id(),
      "reference": {
        "identity": "code-quality",
        "version": "v1",
        "digest": criterion_digest
      },
      "schema": reference("criterion-schema", 44),
      "artifact": {
        "artifact_id": ArtifactId::generate(),
        "content_digest": criterion_digest,
        "encoded_size": 128
      }
    }],
    "branches": [{
      "key": evaluator.identity(),
      "evaluator": evaluator,
      "required": true
    }],
    "required_quorum": 1,
    "budget": BudgetLimit::new(1, 60_000, 100_000, 10_000_000, 16 * 1024 * 1024).unwrap(),
    "created_at": Timestamp::from_unix_millis(1_000).unwrap(),
    "deadline": Timestamp::from_unix_millis(61_000).unwrap(),
    "data_handling": reference("review-data", 42)
  }))
  .unwrap();

  EvaluationRecords {
    evidence,
    plan,
    evaluator,
  }
}

fn terminal(fixture: &Fixture) -> FactoryCodexTerminalObservation<'_> {
  FactoryCodexTerminalObservation {
    build: &fixture.build,
    elapsed_millis: 10,
    runner_events: &fixture.runner_events,
    documents: &fixture.documents,
  }
}

#[test]
fn a_trusted_capability_observation_returns_bounded_data_without_creating_a_stage_or_call() {
  let fixture = Fixture::successful();
  let observed = observe_codex_implementation(
    &fixture.envelope,
    &fixture.build,
    10,
    &fixture.runner_events,
    &fixture.documents,
  );
  assert_eq!(observed.status(), FactoryCodexObservationStatus::Succeeded);
  let summary = observed.summary().expect("verified successor data");
  assert_eq!(summary.content().as_str(), "Implemented the bounded change");
  assert_eq!(summary.subject(), fixture.envelope.subject());
  assert_eq!(
    observed.report().unwrap().decisions()[0].as_str(),
    "Keep the existing execution boundary"
  );
  assert_eq!(observed.deliverables().len(), 2);
}

#[test]
fn codex_node_accepts_the_released_report_media_without_weakening_its_record_contract() {
  use crate::{FactoryCodexNodeRunContract, verify_codex_node_run};
  let (mut fixture, profile, schema) = node_observation_fixture();
  let envelope = fixture.envelope.clone();
  let expected = || FactoryCodexNodeRunContract {
    model_identifier: None,
    usage_policy: &crate::FactoryCodexUsagePolicy::ReportedCost,
    profile: &profile,
    schema: &schema,
    prompt_digest: envelope.digests().prompt_provenance,
    source_revision: envelope.subject().exact().base_revision(),
    budget: envelope.budget(),
    execution_window: Timestamp::from_unix_millis(1000).unwrap()..=Timestamp::from_unix_millis(1010).unwrap(),
  };
  fixture.documents[0].identity.media_type = ArtifactMediaType::new("application/octet-stream").unwrap();
  let observed = verify_codex_node_run(expected(), &fixture.documents)
    .expect("released Octa reports retain their format while regular-file media defaults to binary");
  assert_eq!(observed.data.unwrap().payload().value(), &json!({"count":9}));
  let mut unrelated = fixture.documents.clone();
  unrelated[0].identity.artifact_type = ArtifactType::Report(ArtifactReportFormat::new("unrelated.format.v1").unwrap());
  assert!(verify_codex_node_run(expected(), &unrelated).is_err());
  let mut malformed = fixture.documents.clone();
  malformed[0].bytes = b"not a JSON record".to_vec();
  malformed[0].identity.size_bytes = malformed[0].bytes.len() as u64;
  malformed[0].identity.digest = digest(&malformed[0].bytes);
  assert!(
    verify_codex_node_run(expected(), &malformed).is_err(),
    "matching immutable bytes do not make malformed report JSON valid"
  );
  let mut result: Value = serde_json::from_slice(&fixture.documents[0].bytes).unwrap();
  result["structured_result"] = json!({"count":"wrong schema type"});
  fixture.replace_document("codex-run-result", result);
  assert!(verify_codex_node_run(expected(), &fixture.documents).is_err());
}
#[test]
fn qualified_common_reviewer_records_produce_server_owned_assessment_and_reject_a_model_authored_assessment() {
  use crate::FactoryReviewInvocationSource;
  crate::factory_admission_tests::run_ready(async {
    let (runs, intent, admitted, profile, context) =
      crate::factory_review_planning_tests::prepared_codex_review_invocation().await;
    let policy = profile.reviewer.as_ref().unwrap();
    let invocation = crate::FactoryStoredReviewInvocationSource::new(runs)
      .invocation(
        intent.input(),
        &policy.evaluator,
        Timestamp::from_unix_millis(31).unwrap(),
      )
      .await
      .unwrap();
    let review = crate::FactoryCodexReviewCompilation {
      policy,
      invocation: &invocation,
    };
    let compiled = crate::compile_codex_flow_task_with_options(
      &intent,
      &admitted,
      &profile.instructions,
      &profile.credentials,
      profile.consumer,
      &profile.usage_policy,
      crate::FactoryCodexFlowCompilationOptions {
        context: Some(&context),
        reviewer: Some(review),
        ..Default::default()
      },
    )
    .unwrap();
    let prompt = std::str::from_utf8(compiled.document(crate::FactoryProtectedDocumentKind::Prompt).bytes()).unwrap();
    let mut fixture = Fixture::successful_evaluation("satisfied", json!([]));
    let usage = json!({"input_tokens":1,"output_tokens":2,"cost_micro_units":1});
    let mut result: Value = serde_json::from_slice(&fixture.documents[0].bytes).unwrap();
    result["usage"] = usage.clone();
    fixture.replace_document("codex-run-result", result);
    let mut provenance: Value = serde_json::from_slice(&fixture.documents[2].bytes).unwrap();
    provenance["plugin"]["version"] = json!(profile.toolchain.plugin.version().as_str());
    provenance["codex"]["version"] = json!(profile.toolchain.executable.version().as_str());
    provenance["settings"]["model"] = json!(profile.toolchain.model.identity().as_str());
    provenance["prompt_digest"]["value"] = json!(crate::codex_prompt_provenance_digest(prompt).to_string());
    provenance["source_revision"] = json!(invocation.plan().subject().candidate_revision().as_str());
    provenance["timing"] = json!({"started_unix_millis":32,"finished_unix_millis":36,"duration_millis":4});
    provenance["usage"] = usage;
    fixture.replace_document("codex-run-provenance", provenance);
    let schema = admitted
      .closure()
      .definition(intent.input().definition())
      .unwrap()
      .node(intent.input().node())
      .unwrap()
      .outcome(intent.result_outcome())
      .and_then(|o| admitted.data_schema(o.schema()))
      .unwrap();
    let expected = || crate::FactoryCodexNodeRunContract {
      model_identifier: None,
      profile: intent.profile(),
      schema,
      prompt_digest: crate::codex_prompt_provenance_digest(prompt),
      source_revision: invocation.plan().subject().candidate_revision(),
      budget: intent.budget(),
      usage_policy: &profile.usage_policy,
      execution_window: Timestamp::from_unix_millis(32).unwrap()..=Timestamp::from_unix_millis(40).unwrap(),
    };
    let observed =
      crate::verify_codex_review_node_run(expected(), &intent, review, &compiled, &fixture.documents).unwrap();
    let assessment: octacity_server_factory::Assessment =
      serde_json::from_value(observed.data.unwrap().payload().value().clone()).unwrap();
    assessment
      .validate_bindings(invocation.plan(), invocation.evidence())
      .unwrap();
    assert_eq!(assessment.outcome(), AssessmentOutcome::Satisfied);
    assert_eq!(assessment.evaluator_reference(), &policy.evaluator);
    assert_eq!(assessment.provenance().model(), &profile.toolchain.model);
    assert_eq!(
      assessment.provenance().prompt_digest(),
      crate::codex_prompt_digest(prompt)
    );
    assert_eq!(
      assessment.provenance().result().artifact_id(),
      fixture.documents[0].identity.artifact_id
    );
    assert_eq!(
      assessment.provenance().record().artifact_id(),
      fixture.documents[2].identity.artifact_id
    );
    let replay =
      crate::verify_codex_review_node_run(expected(), &intent, review, &compiled, &fixture.documents).unwrap();
    assert_eq!(
      replay.data.unwrap().payload().value(),
      &serde_json::to_value(&assessment).unwrap()
    );
    let mut result: Value = serde_json::from_slice(&fixture.documents[0].bytes).unwrap();
    result["structured_result"] = serde_json::to_value(&assessment).unwrap();
    fixture.replace_document("codex-run-result", result);
    assert!(
      crate::verify_codex_review_node_run(expected(), &intent, review, &compiled, &fixture.documents).is_err(),
      "a provider-authored Assessment must never establish its own plan/model/provenance"
    );
  });
}

#[test]
fn manual_model_provenance_must_match_the_retained_plugin_identifier() {
  use crate::{FactoryCodexNodeRunContract, verify_codex_node_run};
  let (mut fixture, profile, schema) = node_observation_fixture();
  let mut provenance: Value = serde_json::from_slice(&fixture.documents[2].bytes).unwrap();
  provenance["settings"]["model"] = json!("Vendor/future:preview");
  fixture.replace_document("codex-run-provenance", provenance);
  let expected = || FactoryCodexNodeRunContract {
    model_identifier: Some("Vendor/future:preview"),
    profile: &profile,
    schema: &schema,
    prompt_digest: fixture.envelope.digests().prompt_provenance,
    source_revision: fixture.envelope.subject().exact().base_revision(),
    budget: fixture.envelope.budget(),
    usage_policy: &crate::FactoryCodexUsagePolicy::ReportedCost,
    execution_window: Timestamp::from_unix_millis(0).unwrap()..=Timestamp::from_unix_millis(10_000).unwrap(),
  };
  assert!(verify_codex_node_run(expected(), &fixture.documents).is_ok());
  let mut different = expected();
  different.model_identifier = Some("different-model");
  assert!(verify_codex_node_run(different, &fixture.documents).is_err());
}

#[test]
fn manual_model_cannot_reuse_another_models_exact_token_prices() {
  use crate::{FactoryCodexNodeRunContract, FactoryCodexUsagePolicy, verify_codex_node_run};
  let (mut fixture, profile, schema) = node_observation_fixture();
  let mut provenance: Value = serde_json::from_slice(&fixture.documents[2].bytes).unwrap();
  provenance["settings"]["model"] = json!("Vendor/future:preview");
  fixture.replace_document("codex-run-provenance", provenance);
  // Seven reported output tokens price to the same two units in the fixture.
  // Reject the wrong model binding even though the arithmetic would agree.
  let pricing = FactoryCodexUsagePolicy::TokenRates {
    model: profile.model_or_tool.clone(),
    input_micro_units_per_million: 0,
    cached_input_micro_units_per_million: 0,
    output_micro_units_per_million: 250_000,
  };
  let expected = FactoryCodexNodeRunContract {
    model_identifier: Some("Vendor/future:preview"),
    profile: &profile,
    schema: &schema,
    prompt_digest: fixture.envelope.digests().prompt_provenance,
    source_revision: fixture.envelope.subject().exact().base_revision(),
    budget: fixture.envelope.budget(),
    usage_policy: &pricing,
    execution_window: Timestamp::from_unix_millis(1000).unwrap()..=Timestamp::from_unix_millis(1010).unwrap(),
  };
  assert!(verify_codex_node_run(expected, &fixture.documents).is_err());
}
