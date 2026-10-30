//! Finite packaged qualification scenarios; these select test inputs, never runtime policy.
use super::*;
#[derive(Clone, Copy)]
pub(super) enum Scenario {
  Baseline,
  InjectedText,
  OutOfScope,
  Duplicate,
  AdmissionRestart,
  StageRestart,
  MutationRestart,
  MalformedModel,
  MalformedTool,
  McpTokenRevoked,
  WriterRevoked,
  McpConnectionRevoked,
  ReproductionFailed,
  IncompleteLookup,
}

impl Scenario {
  pub(super) fn selected() -> Self {
    match std::env::var("OCTACITY_TEST_TRIAGE_CASE").as_deref() {
      Err(std::env::VarError::NotPresent) => Self::Baseline,
      Ok("injected_text") => Self::InjectedText,
      Ok("out_of_scope") => Self::OutOfScope,
      Ok("duplicate") => Self::Duplicate,
      Ok("admission_restart") => Self::AdmissionRestart,
      Ok("stage_restart") => Self::StageRestart,
      Ok("mutation_restart") => Self::MutationRestart,
      Ok("malformed_tool") => Self::MalformedTool,
      Ok("malformed_model") => Self::MalformedModel,
      Ok("mcp_token_revoked") => Self::McpTokenRevoked,
      Ok("writer_revoked") => Self::WriterRevoked,
      Ok("mcp_connection_revoked") => Self::McpConnectionRevoked,
      Ok("reproduction_failed") => Self::ReproductionFailed,
      Ok("incomplete_lookup") => Self::IncompleteLookup,
      _ => panic!("unknown packaged triage qualification case"),
    }
  }

  pub(super) fn name(self) -> &'static str {
    match self {
      Self::Baseline => "baseline",
      Self::InjectedText => "injected_text",
      Self::OutOfScope => "out_of_scope",
      Self::Duplicate => "duplicate",
      Self::AdmissionRestart => "admission_restart",
      Self::StageRestart => "stage_restart",
      Self::MutationRestart => "mutation_restart",
      Self::MalformedModel => "malformed_model",
      Self::MalformedTool => "malformed_tool",
      Self::McpTokenRevoked => "mcp_token_revoked",
      Self::WriterRevoked => "writer_revoked",
      Self::McpConnectionRevoked => "mcp_connection_revoked",
      Self::ReproductionFailed => "reproduction_failed",
      Self::IncompleteLookup => "incomplete_lookup",
    }
  }

  pub(super) fn accepted_stages(self) -> usize {
    match self {
      Self::Baseline
      | Self::InjectedText
      | Self::AdmissionRestart
      | Self::StageRestart
      | Self::MutationRestart
      | Self::WriterRevoked
      | Self::ReproductionFailed => 2,
      Self::OutOfScope
      | Self::Duplicate
      | Self::MalformedModel
      | Self::MalformedTool
      | Self::McpTokenRevoked
      | Self::McpConnectionRevoked
      | Self::IncompleteLookup => 1,
    }
  }

  pub(super) fn eligibility(self) -> &'static str {
    match self {
      Self::Baseline
      | Self::InjectedText
      | Self::AdmissionRestart
      | Self::StageRestart
      | Self::MutationRestart
      | Self::WriterRevoked
      | Self::ReproductionFailed => "classify",
      Self::OutOfScope => "out_of_scope",
      Self::Duplicate => "duplicate",
      Self::MalformedModel
      | Self::MalformedTool
      | Self::McpTokenRevoked
      | Self::McpConnectionRevoked
      | Self::IncompleteLookup => "review",
    }
  }

  pub(super) fn unverified(self) -> bool {
    matches!(
      self,
      Self::MalformedModel
        | Self::MalformedTool
        | Self::McpTokenRevoked
        | Self::McpConnectionRevoked
        | Self::IncompleteLookup
    )
  }

  pub(super) fn reproduction_failed(self) -> bool {
    matches!(self, Self::ReproductionFailed)
  }
  pub(super) fn injected_text(self) -> bool {
    matches!(self, Self::InjectedText)
  }
  pub(super) fn incomplete_lookup(self) -> bool {
    matches!(self, Self::IncompleteLookup)
  }

  pub(super) async fn after_install(self, execution: &mut ExecutionEnvironment) {
    if matches!(self, Self::McpConnectionRevoked) {
      let path = execution.environment.configuration_path();
      let mut config: toml::Value = toml::from_str(&fs::read_to_string(path).unwrap()).unwrap();
      config
        .as_table_mut()
        .unwrap()
        .insert("issue_mcp_readers".into(), toml::Value::Array(vec![]));
      fs::write(path, toml::to_string(&config).unwrap()).unwrap();
      execution.environment.restart().await;
    }
    if matches!(self, Self::McpTokenRevoked) {
      let input: Value =
        serde_json::from_slice(&fs::read(required_path("OCTACITY_FACTORY_PILOT_NATIVE_MCP", true)).unwrap()).unwrap();
      let setup: Value = serde_json::from_slice(&fs::read(input["setup_file"].as_str().unwrap()).unwrap()).unwrap();
      let replacement = format!("ocmcp.packaged-triage.{}", "cd".repeat(32));
      execution
        .environment
        .install_issue_reader(
          &execution.resources.project,
          setup["visible"].as_str().unwrap(),
          &replacement,
        )
        .await;
    }
  }

  pub(super) async fn admission_checkpoint(self, project: &str) -> Option<source_checkpoint::Checkpoint> {
    match self {
      Self::AdmissionRestart => Some(source_checkpoint::Checkpoint::admission(project).await),
      _ => None,
    }
  }

  pub(super) async fn after_admission(
    self,
    execution: &mut ExecutionEnvironment,
    run: &str,
    checkpoint: Option<source_checkpoint::Checkpoint>,
  ) -> Option<Value> {
    if matches!(self, Self::MutationRestart) {
      execution
        .environment
        .provider
        .reporting
        .lock()
        .await
        .as_mut()
        .unwrap()
        .labels
        .insert("human-owned".into());
    }
    let checkpoint = checkpoint?;
    checkpoint.wait().await;
    let recorded = restart_at(execution, run, "admission").await;
    checkpoint.release().await;
    resume_model();
    Some(recorded)
  }

  pub(super) async fn pause_mutation(
    self,
    environment: &source_environment::SourceEnvironment,
  ) -> Option<source_provider::MutationPause> {
    if !matches!(self, Self::MutationRestart) {
      return None;
    }
    environment.provider.reporting.lock().await.as_mut().unwrap().fault =
      Some(("POST", source_provider::reporting::LostResponse::Disconnect));
    Some(environment.provider.pause_next_mutation().await)
  }

  /// The installed 10-second attempt expires before a crashed worker's 30-second lease.
  /// Accepted decisions survive; recovery must not renew the deadline or claim delivery.
  pub(super) fn expected_delivery(self) -> (&'static str, Option<&'static str>) {
    if matches!(self, Self::MutationRestart) {
      ("failed", Some("exhausted"))
    } else if matches!(self, Self::WriterRevoked) {
      ("failed", Some("forbidden"))
    } else {
      ("delivered", None)
    }
  }

  pub(super) async fn after_mutation(
    self,
    execution: &mut ExecutionEnvironment,
    run: &str,
    paused: Option<source_provider::MutationPause>,
    work: &Value,
    completions: &Value,
    checkpoints: &mut Vec<Value>,
  ) {
    if let Some(paused) = paused {
      paused.wait().await;
      assert!(
        execution
          .environment
          .provider
          .reporting
          .lock()
          .await
          .as_ref()
          .unwrap()
          .labels
          .contains("ready / review")
      );
      // No management read can delay the crash while a real provider reply is held.
      let (previous, current) = execution.environment.crash_and_restart().await;
      paused.resume();
      checkpoints.push(
        retained_checkpoint(
          execution,
          run,
          "provider_mutation",
          work,
          accepted_results(completions),
          previous,
          current,
        )
        .await,
      );
    }
  }

  pub(super) fn delivery_wait(self) -> Duration {
    Duration::from_secs(if matches!(self, Self::MutationRestart) { 60 } else { 30 })
  }

  pub(super) fn expected_labels(self) -> std::collections::BTreeSet<String> {
    if matches!(self, Self::WriterRevoked) {
      return std::collections::BTreeSet::new();
    }
    let mut labels = std::collections::BTreeSet::from(["ready / review".into()]);
    if matches!(self, Self::MutationRestart) {
      labels.insert("human-owned".into());
    }
    labels
  }

  pub(super) async fn after_stage(
    self,
    execution: &mut ExecutionEnvironment,
    run: &str,
    connection: &str,
    completions: &Value,
    checkpoints: &mut Vec<Value>,
  ) {
    if matches!(self, Self::StageRestart | Self::WriterRevoked)
      && checkpoints.is_empty()
      && completions["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["record"]["node_key"] == "eligibility_gate")
    {
      checkpoints.push(if matches!(self, Self::WriterRevoked) {
        revoke_writer(execution, run, connection).await
      } else {
        restart_at(execution, run, "stage_acceptance").await
      });
      resume_model();
    }
  }

  pub(super) fn native_writes(self) -> usize {
    if matches!(self, Self::WriterRevoked) { 0 } else { 1 }
  }
}

async fn revoke_writer(execution: &ExecutionEnvironment, run: &str, connection: &str) -> Value {
  let project = &execution.resources.project;
  let origin = &execution.environment.origin;
  let client = &execution.environment.client;
  let grant = source_setup::publish(client, origin,
    &format!("/api/v1/integrations/{connection}/projects/{project}/access"),
    &format!("{}-revoke-writer", execution.run), "if-match", "\"1\"",
    json!({"intent":{"connection_id":connection,"project_id":project,"connection_version":1,
      "expected_current_version":1,"resources":[{"reference":execution.environment.installation.resource["reference"],"purposes":["task_read"]}]}})).await;
  let choices = get_json(
    client,
    format!(
      "{origin}/api/v1/projects/{project}/factory-source-choices?connection_id={connection}&connection_version=1"
    ),
  )
  .await;
  assert_eq!(choices["items"].as_array().unwrap().len(), 1);
  assert_eq!(choices["items"][0]["purpose"], "task_read");
  json!({"at":"writer_revocation","run_id":run,"grant":grant,"choices":choices})
}

fn resume_model() {
  let input: Value =
    serde_json::from_slice(&fs::read(required_path("OCTACITY_FACTORY_PILOT_NATIVE_MCP", true)).unwrap()).unwrap();
  fs::write(
    Path::new(input["setup_file"].as_str().unwrap()).with_extension("resume"),
    b"",
  )
  .unwrap();
}

async fn restart_at(execution: &mut ExecutionEnvironment, run: &str, at: &str) -> Value {
  let base = format!("{}/api/v1/factory-runs/{run}", execution.environment.origin);
  let client = execution.environment.client.clone();
  let before = get_json(&client, format!("{base}/work")).await;
  let completions = get_json(&client, format!("{base}/node-completions?limit=100")).await;
  let (previous, current) = execution.environment.crash_and_restart().await;
  retained_checkpoint(
    execution,
    run,
    at,
    &before,
    accepted_results(&completions),
    previous,
    current,
  )
  .await
}

fn accepted_results(completions: &Value) -> Vec<Value> {
  completions["items"]
    .as_array()
    .unwrap()
    .iter()
    .filter(|item| item["record"]["public_result"].is_object())
    .cloned()
    .collect()
}

async fn retained_checkpoint(
  execution: &ExecutionEnvironment,
  run: &str,
  at: &str,
  before: &Value,
  accepted: Vec<Value>,
  previous: u32,
  current: u32,
) -> Value {
  let base = format!("{}/api/v1/factory-runs/{run}", execution.environment.origin);
  let client = &execution.environment.client;
  let after = get_json(client, format!("{base}/work")).await;
  assert_eq!(
    before["items"], after["items"],
    "restart retains the original frozen Work"
  );
  let retained = get_json(client, format!("{base}/node-completions?limit=100")).await;
  for completion in &accepted {
    assert!(
      retained["items"].as_array().unwrap().contains(completion),
      "restart must retain the exact accepted stage result"
    );
  }
  let delivery = get_json(client, format!("{base}/effect-delivery?limit=100")).await;
  let effects: Vec<_> = delivery["items"]
    .as_array()
    .unwrap()
    .iter()
    .filter(|item| item["record"]["kind"] == "work.status.projection")
    .map(|item| {
      let record = &item["record"];
      json!({"id":record["id"],"input_digest":record["input_digest"]})
    })
    .collect();
  json!({"at":at,"run_id":run,"work":before["items"],"accepted":accepted,"effects":effects,"server_before":previous,"server_after":current})
}
