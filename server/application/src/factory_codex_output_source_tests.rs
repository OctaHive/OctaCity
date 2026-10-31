//! Installed output observation over real common journals and Artifact lifecycle ports.
use crate::factory_node_test_support::fixtures;
use crate::*;
use async_trait::async_trait;
use octacity_artifact_store::{ArtifactByteReader, ArtifactObject, ArtifactStoreError};
use octacity_server_domain::*;
use octacity_server_factory::*;
use octacity_server_job::{JobSpecBuildSnapshot, JobState};
use octacity_server_orchestrator::{AttemptState, BuildState};
use octacity_server_store::{testing::*, *};
use std::{
  collections::BTreeMap,
  sync::{Arc, Mutex},
};
#[path = "factory_mcp_output_tests.rs"]
mod mcp_evidence;
fn at(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}

#[test]
fn installed_tool_response_limits_are_part_of_the_frozen_task_identity() {
  crate::factory_admission_tests::run_ready(async {
    let mut profile = prepared_source().await.profile;
    let original = profile.task_digest();
    profile.tool_response_limits = Some(octacity_protocol::FactoryToolResponseLimitsV3 {
      per_call_bytes: 32,
      total_bytes: 64,
    });
    let bounded = profile.task_digest();
    assert_ne!(
      original, bounded,
      "installing reader authority must alter the immutable task pin"
    );
    profile.tool_response_limits.as_mut().unwrap().total_bytes = 65;
    assert_ne!(
      bounded,
      profile.task_digest(),
      "replacement cannot widen a frozen quota under its prior identity"
    );
    profile.tool_response_limits = None;
    assert_eq!(
      original,
      profile.task_digest(),
      "tool-free installations preserve their existing identity"
    );
  });
}

#[test]
fn installed_codex_compilation_metadata_reuses_native_task_and_output_contracts() {
  crate::factory_admission_tests::run_ready(async {
    let profile = prepared_source().await.profile;
    let metadata = profile
      .pipeline_contract()
      .compilation
      .expect("installed compilation metadata");
    assert_eq!(metadata.task_digest, profile.task_digest());
    assert!(metadata.role_profiles);
    assert_eq!(metadata.result_output.as_str(), "codex-run-result");
    assert_eq!(metadata.result_output_kind, EvidenceOutputKind::Report);
    assert_eq!(metadata.mcp_result_tool, Some(factory_mcp_evidence_verifier()));
  });
}

struct Clock;
impl FactoryExecutionClock for Clock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(1011))
  }
}
struct NeverDispatch;
#[async_trait]
impl FactoryNodeExecutor for NeverDispatch {
  async fn observe(&self, _: FactoryNodeExecutionRequest<'_>) -> Result<FactoryNodeExecutionStep, ApplicationError> {
    panic!("only the persisted first step is prepared");
  }
  async fn cancel(
    &self,
    _: FactoryNodeExecutionRequest<'_>,
    _: Timestamp,
  ) -> Result<FactoryNodeCancellationStep, ApplicationError> {
    unreachable!()
  }
}
struct ContextBytes(ArtifactId, Vec<u8>);
#[async_trait]
impl FactoryNodeContextSource for ContextBytes {
  async fn read_context(
    &self,
    _: &FlowNodeInput,
    observed_at: Timestamp,
    required_until: Timestamp,
  ) -> Result<BTreeMap<ArtifactId, Vec<u8>>, ApplicationError> {
    if required_until <= observed_at {
      return Err(ApplicationError::invalid());
    }
    Ok(BTreeMap::from([(self.0, self.1.clone())]))
  }
}
#[derive(Clone)]
struct Queries {
  build: BuildRecord,
  attempt: AttemptRecord,
  history: BTreeMap<AttemptId, AttemptRecord>,
}
#[async_trait]
impl BuildQueryStore for Queries {
  async fn build(&self, id: BuildId) -> Result<BuildRecord, StoreError> {
    assert_eq!(id, self.build.build.id);
    Ok(self.build.clone())
  }
  async fn attempt(&self, id: AttemptId) -> Result<AttemptRecord, StoreError> {
    if id == self.attempt.id {
      Ok(self.attempt.clone())
    } else {
      Ok(self.history.get(&id).expect("requested retained predecessor").clone())
    }
  }
  async fn latest_attempt(&self, id: BuildId) -> Result<AttemptRecord, StoreError> {
    assert_eq!(id, self.build.build.id);
    Ok(self.attempt.clone())
  }
  async fn job(&self, id: JobId) -> Result<JobRecord, StoreError> {
    assert_eq!(id, self.attempt.jobs[0].id());
    Ok(self.attempt.jobs[0].clone())
  }
}
#[derive(Default)]
struct Bytes(Mutex<BTreeMap<ArtifactId, Vec<u8>>>);
#[async_trait]
impl ArtifactByteReader for Bytes {
  async fn read_published(&self, object: &ArtifactObject, maximum: u64) -> Result<Vec<u8>, ArtifactStoreError> {
    let bytes = self.0.lock().unwrap().get(&object.artifact_id()).unwrap().clone();
    assert!(bytes.len() as u64 <= maximum);
    Ok(bytes)
  }
}

struct HistoryArtifacts {
  current: Arc<InMemoryArtifactRecordStore>,
  previous: Vec<ArtifactUploadRecord>,
}
#[async_trait]
impl ArtifactRecordStore for HistoryArtifacts {
  async fn reserve_artifact(&self, r: ReserveArtifact) -> Result<ArtifactRecord, StoreError> {
    self.current.reserve_artifact(r).await
  }
  async fn artifact(&self, id: ArtifactId) -> Result<ArtifactRecord, StoreError> {
    if let Some(record) = self
      .previous
      .iter()
      .find(|record| record.artifact.identity().artifact_id == id)
    {
      Ok(record.artifact.clone())
    } else {
      self.current.artifact(id).await
    }
  }
  async fn transition_artifact(&self, r: TransitionArtifact) -> Result<ArtifactRecord, StoreError> {
    self.current.transition_artifact(r).await
  }
  async fn begin_artifact_upload(&self, r: BeginArtifactUpload) -> Result<BeginArtifactUploadOutcome, StoreError> {
    self.current.begin_artifact_upload(r).await
  }
  async fn artifact_upload(&self, id: ArtifactUploadId) -> Result<ArtifactUploadRecord, StoreError> {
    self.current.artifact_upload(id).await
  }
  async fn begin_artifact_verification(&self, r: VerifyArtifactUpload) -> Result<ArtifactUploadRecord, StoreError> {
    self.current.begin_artifact_verification(r).await
  }
  async fn finish_artifact_verification(
    &self,
    r: VerifyArtifactUpload,
    v: ArtifactVerificationResult,
  ) -> Result<ArtifactUploadRecord, StoreError> {
    self.current.finish_artifact_verification(r, v).await
  }
  async fn published_artifact(&self, id: ArtifactId) -> Result<ArtifactUploadRecord, StoreError> {
    if let Some(record) = self
      .previous
      .iter()
      .find(|record| record.artifact.identity().artifact_id == id)
    {
      Ok(record.clone())
    } else {
      self.current.published_artifact(id).await
    }
  }
  async fn list_published_artifacts(&self, q: ListPublishedArtifacts) -> Result<Vec<ArtifactUploadRecord>, StoreError> {
    let mut records = self.current.list_published_artifacts(q.clone()).await?;
    records.extend(
      self
        .previous
        .iter()
        .filter(|record| {
          record.artifact.identity().build_id == q.build_id
            && q
              .attempt_id
              .is_none_or(|attempt| record.artifact.identity().attempt_id == attempt)
            && q.visibility.allows(&record.artifact.identity().artifact_id)
        })
        .cloned(),
    );
    records.truncate(usize::from(q.limit));
    Ok(records)
  }
}

async fn prepared_source() -> SourceFixture {
  prepared_source_with_capture(None).await
}

#[test]
fn native_subscription_records_charge_tokens_without_fabricating_a_monetary_price() {
  crate::factory_admission_tests::run_ready(async {
    let policy = serde_json::from_value(serde_json::json!({"kind":"subscription"})).unwrap();
    let fixture = prepared_role_options(PreparedRoleOptions {
      authentication: FactoryCodexAuthentication::NativeLoginFile,
      usage_policy: Some(policy),
      recorded_usage: Some(serde_json::json!({"input_tokens":2,"output_tokens":3})),
      ..Default::default()
    })
    .await;
    let source = fixture.source(fixture.query.clone());
    let observation = source
      .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 10000)
      .await
      .unwrap();
    let FactoryNodeBuildOutputObservation::Verified(outputs) = observation else {
      panic!("native subscription records must remain verifiable without a monetary counter")
    };
    assert_eq!(outputs.usage.tokens, 5);
    assert_eq!(outputs.usage.elapsed_millis, 4);
    assert_eq!(
      serde_json::to_value(outputs.usage).unwrap()["cost_micro_units"],
      serde_json::json!({"status":"unknown","known_micro_units":0})
    );
    let retained = source
      .terminal_usage(fixture.query.build.build.id, fixture.query.attempt.id)
      .await
      .unwrap();
    assert_eq!(retained.tokens, 5);
    assert!(!retained.cost_micro_units.is_known());
  });
}

fn approved_observer_policy(profile: &FactoryCodexBuildProfile) -> crate::JobSpecToolchainPolicy {
  let mut spec = crate::factory_job_test_support::canonical_v3_spec();
  spec
    .octa
    .plugin_digests
    .insert("codex".into(), profile.toolchain.plugin.digest().to_string());
  crate::JobSpecToolchainPolicy {
    source: octacity_server_job::SourcePluginPolicy::new(
      spec.source.provider,
      spec.source.plugin_version,
      spec.source.plugin_sha256,
      "url",
    )
    .unwrap(),
    octa: spec.octa,
    validity: octacity_server_job::JobSpecValidity::new(60).unwrap(),
  }
}

#[test]
fn subscription_accounting_cannot_remove_monetary_verification_from_an_api_key_installation() {
  crate::factory_admission_tests::run_ready(async {
    let mut profile = prepared_source().await.profile;
    let approved = approved_observer_policy(&profile);
    profile.validate_installation(&approved).unwrap();
    profile.usage_policy = FactoryCodexUsagePolicy::Subscription;
    assert!(profile.validate_installation(&approved).is_err());
    profile.authentication = FactoryCodexAuthentication::NativeLoginFile;
    profile.validate_installation(&approved).unwrap();
  });
}

#[test]
fn installed_codex_capture_observer_retains_the_verified_candidate_separately_from_model_records() {
  crate::factory_admission_tests::run_ready(async {
    use octacity_protocol::{CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT};
    let tool = fixtures::reference("git.capture");
    let fixture = prepared_source_with_capture(Some(capture_policy(true, 2))).await;
    let observation = fixture
      .source(fixture.query.clone())
      .published_outputs(
        fixture.query.build.build.id,
        fixture.query.attempt.id,
        fixture.intent.budget().max_output_bytes(),
      )
      .await
      .unwrap();
    let FactoryNodeBuildOutputObservation::Verified(outputs) = observation else {
      panic!("successful author output must retain both its public result and trusted candidate");
    };
    let names = outputs
      .documents
      .iter()
      .map(|document| document.record.identity().logical_name.as_str())
      .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
      names,
      std::collections::BTreeSet::from(["codex-run-result", CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_OUTPUT])
    );
    let result = outputs
      .documents
      .iter()
      .find(|document| document.record.identity().logical_name.as_str() == "codex-run-result")
      .unwrap();
    assert_eq!(
      result.data.as_ref().unwrap().payload().value(),
      &serde_json::json!({"count":9})
    );
    assert!(
      outputs
        .documents
        .iter()
        .filter(|document| document.record.identity().logical_name.as_str() != "codex-run-result")
        .all(|document| document.tool == tool && document.data.is_none())
    );
    let execution = FlowBuildExecution::new(
      &fixture.intent,
      fixture.query.build.build.id,
      fixture.query.attempt.id,
      vec![fixture.query.attempt.jobs[0].id()],
    )
    .unwrap();
    let candidate = verify_factory_node_change_set(&fixture.intent, &execution, &outputs, &tool, at(1018)).unwrap();
    assert_eq!(
      candidate.subject().candidate_revision().as_str(),
      "0123456789abcdef0123456789abcdef0123456789"
    );
    assert_eq!(candidate.subject().exact(), fixture.intent.input().subject());
    assert_eq!(outputs.usage.cost_micro_units, 1);
  });
}
#[test]
fn installed_codex_capture_observer_rejects_empty_changes_when_the_frozen_policy_disallows_them() {
  crate::factory_admission_tests::run_ready(async {
    let fixture = prepared_source_with_capture(Some(capture_policy(false, 2))).await;
    assert!(
      matches!(
        fixture
          .source(fixture.query.clone())
          .published_outputs(
            fixture.query.build.build.id,
            fixture.query.attempt.id,
            fixture.intent.budget().max_output_bytes(),
          )
          .await,
        Err(OrdinaryBuildApplicationError::Invalid)
      ),
      "a valid empty capture cannot bypass its independently installed allow_empty=false policy"
    );
  });
}
async fn prepared_source_with_capture(capture: Option<FactoryCodexCapturePolicy>) -> SourceFixture {
  prepared_source_with_capture_paths(capture, Vec::new()).await
}
fn capture_policy(allow_empty: bool, max_changed_paths: u32) -> FactoryCodexCapturePolicy {
  use octacity_protocol::{
    ChangeSetBinaryPolicyV3, ChangeSetCapturePolicyV3, ChangeSetCaptureV3, ChangeSetSecretPatternV3,
  };
  FactoryCodexCapturePolicy::new(
    fixtures::reference("git.capture"),
    ChangeSetCaptureV3 {
      author_name: "Installed operator".to_owned(),
      author_email: "operator@example.test".to_owned(),
      committed_at: 1,
      patch_max_bytes: None,
      policy: ChangeSetCapturePolicyV3 {
        allowed_path_prefixes: vec!["public/approved".to_owned()],
        forbidden_control_paths: vec![".github/workflows".to_owned()],
        max_changed_paths,
        max_file_bytes: 128,
        max_total_bytes: 256,
        allow_empty,
        binary_policy: ChangeSetBinaryPolicyV3::Reject,
        forbidden_secret_patterns: ChangeSetSecretPatternV3::ALL.to_vec(),
      },

      observation: None,
    },
  )
  .unwrap()
}

#[test]
fn installed_codex_capture_observer_rejects_more_direct_changes_than_its_frozen_limit() {
  crate::factory_admission_tests::run_ready(async {
    let paths = ["public/approved/a.txt", "public/approved/b.txt"]
      .into_iter()
      .map(|path| octacity_protocol::CapturedChangeSetPathV1 {
        status: "A".to_owned(),
        path: path.to_owned(),
        old_mode: "000000".to_owned(),
        new_mode: "100644".to_owned(),
        content_sha256: Some(FactoryDigest::content_sha256(b"fixture-content").to_string()),
      })
      .collect();
    let fixture = prepared_source_with_capture_paths(Some(capture_policy(true, 1)), paths).await;
    assert!(
      matches!(
        fixture
          .source(fixture.query.clone())
          .published_outputs(
            fixture.query.build.build.id,
            fixture.query.attempt.id,
            fixture.intent.budget().max_output_bytes(),
          )
          .await,
        Err(OrdinaryBuildApplicationError::Invalid)
      ),
      "correctly pinned capture records cannot widen their installed direct-path count"
    );
  });
}
async fn prepared_source_with_capture_paths(
  capture: Option<FactoryCodexCapturePolicy>,
  capture_paths: Vec<octacity_protocol::CapturedChangeSetPathV1>,
) -> SourceFixture {
  prepared_source_with_capture_observation(capture, capture_paths, None).await
}
async fn prepared_source_with_capture_observation(
  capture: Option<FactoryCodexCapturePolicy>,
  capture_paths: Vec<octacity_protocol::CapturedChangeSetPathV1>,
  observation_override: Option<serde_json::Value>,
) -> SourceFixture {
  prepared_role_source(capture, capture_paths, observation_override, false).await
}
async fn prepared_role_source(
  capture: Option<FactoryCodexCapturePolicy>,
  capture_paths: Vec<octacity_protocol::CapturedChangeSetPathV1>,
  observation_override: Option<serde_json::Value>,
  selected_role: bool,
) -> SourceFixture {
  prepared_role_source_with_mcp(capture, capture_paths, observation_override, selected_role, None).await
}
async fn prepared_role_source_with_mcp(
  capture: Option<FactoryCodexCapturePolicy>,
  capture_paths: Vec<octacity_protocol::CapturedChangeSetPathV1>,
  observation_override: Option<serde_json::Value>,
  selected_role: bool,
  mcp: Option<FactoryMcpServer>,
) -> SourceFixture {
  prepared_role_source_with_mcp_evidence(
    capture,
    capture_paths,
    observation_override,
    selected_role,
    mcp,
    false,
    false,
  )
  .await
}
async fn prepared_role_source_with_mcp_evidence(
  capture: Option<FactoryCodexCapturePolicy>,
  capture_paths: Vec<octacity_protocol::CapturedChangeSetPathV1>,
  observation_override: Option<serde_json::Value>,
  selected_role: bool,
  mcp: Option<FactoryMcpServer>,
  verify_mcp: bool,
  native_mcp: bool,
) -> SourceFixture {
  prepared_role_options(PreparedRoleOptions {
    capture,
    capture_paths,
    observation_override,
    selected_role,
    mcp,
    verify_mcp,
    native_mcp,
    ..Default::default()
  })
  .await
}

#[derive(Default)]
struct PreparedRoleOptions {
  capture: Option<FactoryCodexCapturePolicy>,
  capture_paths: Vec<octacity_protocol::CapturedChangeSetPathV1>,
  observation_override: Option<serde_json::Value>,
  selected_role: bool,
  mcp: Option<FactoryMcpServer>,
  verify_mcp: bool,
  native_mcp: bool,
  authentication: FactoryCodexAuthentication,
  usage_policy: Option<FactoryCodexUsagePolicy>,
  recorded_usage: Option<serde_json::Value>,
}

async fn prepared_role_options(options: PreparedRoleOptions) -> SourceFixture {
  let PreparedRoleOptions {
    capture,
    capture_paths,
    observation_override,
    selected_role,
    mcp,
    verify_mcp,
    native_mcp,
    authentication,
    usage_policy,
    recorded_usage,
  } = options;
  let capture_schema = capture
    .as_ref()
    .and_then(|capture| capture.instruction().observation.as_ref())
    .map(|output| {
      fixtures::supplied_base_flow_template()
        .data_schemas()
        .unwrap()
        .into_values()
        .find(|schema| {
          schema.reference().identity().as_str() == output.schema.identity
            && schema.reference().version().as_str() == output.schema.version
            && schema.reference().digest().to_string() == output.schema.sha256
        })
        .unwrap()
    });
  let previous = fixtures::work();
  let selected = b"Explicitly selected task facts.".to_vec();
  let task = FactoryArtifactReference::new(
    ArtifactId::generate(),
    FactoryDigest::content_sha256(&selected),
    selected.len() as u64,
  )
  .unwrap();
  let work = WorkEnvelope::new(
    previous.id(),
    previous.configuration().clone(),
    previous.external_identity().clone(),
    previous.subject().clone(),
    WorkArtifacts::new(task.clone(), previous.artifacts().acceptance().clone(), vec![]).unwrap(),
    WorkClassification::new(previous.priority(), previous.risk(), previous.metadata().clone()),
  )
  .unwrap();
  let mut grants = serde_json::to_value(crate::factory_job_test_support::permissions(false)).unwrap();
  for mount in grants["mounts"].as_array_mut().unwrap() {
    if mount["root"] == "/workspace/source" {
      mount["mode"] = serde_json::json!(if capture.is_some() { "read_write" } else { "read_only" });
    }
  }
  grants["outputs"]["kinds"] = serde_json::json!(["codex-run-trace", "codex-run-provenance", "codex-run-result"]);
  if capture.is_some() {
    grants["secret_profiles"] = serde_json::json!(["author-only"]);
    grants["outputs"]["kinds"] = serde_json::json!([
      "change-set.bundle",
      "change-set-manifest.json",
      "codex-run-provenance",
      "codex-run-result",
      "codex-run-trace"
    ]);
  }
  if let Some(output) = capture
    .as_ref()
    .and_then(|capture| capture.instruction().observation.as_ref())
  {
    grants["outputs"]["kinds"]
      .as_array_mut()
      .unwrap()
      .push(serde_json::json!(output.name));
    grants["outputs"]["max_report_count"] = serde_json::json!(2);
  }
  let grants: FactoryPermissionSet = serde_json::from_value(grants).unwrap();
  let instructions = FactoryText::new("Observe only the declared schema.").unwrap();
  let mut binding = fixtures::binding(work.subject().project_id());
  binding.plugin = crate::factory_job_test_support::reference("codex", "0.5.0");
  binding.tool = crate::factory_job_test_support::reference("codex-cli", "0.116.0");
  binding.result_output = FactoryKey::new("codex-run-result").unwrap();
  if verify_mcp {
    binding.result_tool = Some(mcp_verifier());
  }
  let base = authoritative_store_contract_fixture();
  let profile = FactoryCodexBuildProfile {
    authentication,
    mcp_configurations: vec![],
    toolchain: FactoryTaskToolchain {
      plugin: binding.plugin.clone(),
      executable: binding.tool.clone(),
      model: binding.model_or_tool.clone(),
    },
    build_configuration: binding.build_configuration.clone(),
    instructions,
    credentials: FactoryCredentialProfiles::new(
      FactoryKey::new("author-only").unwrap(),
      FactoryKey::new("model-coding").unwrap(),
      FactoryKey::new("source-only").unwrap(),
      FactoryKey::new("delivery-only").unwrap(),
    )
    .unwrap(),
    consumer: if capture.is_some() {
      FactoryCredentialConsumer::CodingNode
    } else {
      FactoryCredentialConsumer::ReadOnlyReview
    },
    usage_policy: usage_policy.unwrap_or(FactoryCodexUsagePolicy::ReportedCost),
    tool_risk: None,
    tool_response_limits: None,
    reviewer: None,
    candidate_capture_tool: None,
    capture,
    pipeline_node: base.request.jobs[0].pipeline_node_id.clone(),
    parameters: BTreeMap::new(),
    project_permissions: grants.clone(),
    local_permissions: LocalPermissionCeiling::fully_enforced(grants.clone()),
  };
  if let Some(capture) = &profile.capture {
    binding.capture_write_paths = Some(
      capture
        .instruction()
        .policy
        .allowed_path_prefixes
        .iter()
        .map(|path| FactoryRepositoryPath::new(path).unwrap())
        .collect(),
    );
    binding.evidence = [
      octacity_protocol::CHANGE_SET_BUNDLE_OUTPUT,
      octacity_protocol::CHANGE_SET_MANIFEST_OUTPUT,
    ]
    .into_iter()
    .map(|name| {
      EvidenceRequirement::new(
        fixtures::key(name),
        EvidenceOutputKind::Artifact,
        fixtures::reference("capture.artifacts"),
        capture.tool().clone(),
        binding.plugin.clone(),
      )
    })
    .collect();
    if let Some(output) = &capture.instruction().observation {
      binding.evidence.push(EvidenceRequirement::new(
        fixtures::key(&output.name),
        EvidenceOutputKind::Report,
        capture_schema.as_ref().unwrap().reference().clone(),
        capture.tool().clone(),
        binding.plugin.clone(),
      ));
    }
  }
  binding.task_digest = profile.task_digest();
  let (_, admitted, schema, input, _) =
    fixtures::execution_fixture_with_permissions(FlowNodeKind::Reasoning, work.clone(), binding, None, grants.clone());
  let (admitted, schema, input) = if verify_mcp {
    mcp_evidence::conclusion_admission(&work, admitted, schema, input)
  } else {
    (admitted, schema, input)
  };
  let runtime_settings = mcp.map(|server| {
    FactoryRuntimeSettings::new(
      work.subject().project_id(),
      fixtures::key("profile.0"),
      FactoryRuntimeSettingsVersion::INITIAL,
      FactoryRuntimeSettingsDefinition {
        toolchain: profile.toolchain.clone(),
        mcp_servers: [(
          fixtures::key("reader"),
          FactoryRuntimeMcpEntry {
            authorization: None,
            server,
            integration: None,
          },
        )]
        .into(),
        integrations: Default::default(),
        permissions: grants.clone(),
      },
    )
    .unwrap()
  });
  let mcp_configuration = runtime_settings.as_ref().map(|settings| {
    FactoryCodexMcpConfiguration::try_from(settings.mcp_configuration(&fixtures::key("reader")).unwrap().unwrap())
      .unwrap()
  });
  let role_content = selected_role.then(|| {
    let role = FactoryRole::publish(
      FactoryRoleId::generate(),
      work.subject().project_id(),
      FactoryRoleDefinition {
        avatar: None,
        skills: Default::default(),
        name: FactoryText::new("Observer").unwrap(),
        instructions: "Apply Project rules".into(),
        template: None,
        icon: None,
        description: String::new(),
        concurrency_limit: None,
      },
    )
    .unwrap();
    let mut definition = role_profile_contract_definition(work.subject().project_id());
    definition.toolchain = profile.toolchain.clone();
    definition.build_configuration = profile.build_configuration.clone();
    definition.permissions = grants.clone();
    definition.budget = admitted.closure().definitions()[0].nodes()[0].budget();
    definition.execution = Default::default();
    if let Some(configuration) = &mcp_configuration {
      definition.execution.mcp_servers.insert(
        fixtures::key("reader"),
        FactoryRoleMcpBinding {
          configuration: configuration.reference().clone(),
          integration: None,
        },
      );
    }
    definition.model_parameters.clear();
    definition.skills.clear();
    definition.context_requirements.clear();
    definition.credential_purpose = profile.consumer.purpose();
    let selected = FactoryRoleProfile::publish(
      FactoryRoleProfileId::generate(),
      work.subject().project_id(),
      &role,
      definition,
    )
    .unwrap();
    (role, selected)
  });
  let (admitted, input) = if let Some((_, selected)) = &role_content {
    let mut build = admitted.closure().definitions()[0].nodes()[0].build().unwrap().clone();
    build.task_digest = factory_role_task_digest(profile.task_digest(), selected);
    fixtures::with_selected_profile(
      &work,
      &admitted,
      &input,
      FlowProfileCandidate {
        profile: selected.clone(),
        build,
      },
    )
  } else {
    (admitted, input)
  };
  let admitted = if let Some(capture_schema) = capture_schema {
    admitted
      .with_data_schemas(vec![schema.clone(), capture_schema])
      .unwrap()
  } else {
    admitted
  };
  let run = FactoryRun::admitted(input.factory_run_id(), &work);
  let runs = Arc::new(InMemoryFactoryConfigurationStore::new());
  let admission = PublishedFactoryAdmission {
    work: work.clone(),
    run: run.clone(),
    flow: admitted.clone(),
    admitted_at: at(1),
  };
  let audit = management_mutation_with_request((), "codex-output-source")
    .audit()
    .clone();
  runs
    .seed_factory_run(admission, FactoryDigest::from_bytes([23; 32]), &audit)
    .unwrap();
  let claim = FactoryRunClaimRecord::new(
    run.id(),
    FactoryKey::new("current-owner").unwrap(),
    FactoryClaim::new(
      FactoryClaimFence::new(FactoryDigest::from_bytes([24; 32])),
      at(1000),
      at(5000),
    )
    .unwrap(),
  );
  runs
    .claim_factory_run(ClaimFactoryRun {
      run_id: run.id(),
      expected_version: run.version(),
      record: claim.clone(),
      audit: FactoryAuditFact::new(
        run.id(),
        AuditActorKind::Worker,
        None,
        FactoryKey::new("factory.claimed").unwrap(),
        claim.id,
        FactoryKey::new("claimed").unwrap(),
        at(1000),
      ),
    })
    .await
    .unwrap();
  let target = FactoryNodeTarget {
    flow_run_id: input.flow_run_id(),
    cycle_id: input.cycle_id(),
    node: input.node().clone(),
  };
  if selected_role {
    runs.seed_project(work.subject().project_id()).unwrap();
    let mut choices = factory_profile_selection_capabilities(&admitted);
    if let Some(settings) = &runtime_settings {
      choices = choices
        .with_runtime_settings_support(
          settings.runtime().clone(),
          FactoryRuntimeSettingsSupport {
            authenticated_mcp: Default::default(),
            mcp_transports: settings
              .definition()
              .mcp_servers
              .values()
              .map(|entry| entry.server.transport())
              .collect(),
            mcp_executables: Default::default(),
          },
        )
        .unwrap();
    }
    runs.enable_project(work.subject().project_id(), choices).unwrap();
  }
  if let Some(settings) = runtime_settings {
    runs
      .publish_factory_runtime_settings(management_mutation(PublishFactoryRuntimeSettings {
        intent: FactoryRuntimeSettingsIntent {
          project_id: settings.project_id(),
          runtime: settings.runtime().clone(),
          expected_version: None,
          definition: settings.definition().clone(),
        },
        idempotency_key: "output-runtime".parse().unwrap(),
        published_at: at(1),
      }))
      .await
      .unwrap();
  }
  let owner =
    FactoryNodeRunner::new(runs.clone(), Arc::new(NeverDispatch), Arc::new(Clock)).with_profile_catalog(runs.clone());
  owner
    .run_once(run.id(), claim.id, target.clone(), Some(input), at(1001))
    .await
    .unwrap();
  if selected_role {
    owner
      .run_once(run.id(), claim.id, target, None, at(1002))
      .await
      .unwrap();
  }
  let snapshot = runs.factory_run_snapshot(run.id()).await.unwrap();
  let intent = &snapshot.flow.data.build_intents[0];
  let context = Arc::new(ContextBytes(task.artifact_id(), selected.clone()));
  let context_map = BTreeMap::from([(task.artifact_id(), selected)]);
  let role = role_content.map(|(role, selected)| {
    let context = intent.input().context().unwrap();
    let task = FactoryTaskEnvelope::new(
      FactoryTaskDeclaration {
        id: TaskEnvelopeId::generate(),
        node_attempt_id: intent.node().id(),
        subject: context.subject().clone(),
        mode: if profile.capture.is_some() {
          FactoryTaskMode::Implement
        } else {
          FactoryTaskMode::Observe
        },
      },
      FactoryTaskDefinition {
        task,
        specifications: vec![],
        prior_findings: vec![],
        permissions: intent.permissions().clone(),
        budget: intent.budget(),
        result_schema: if profile.capture.is_some() {
          FactoryTaskResultSchema::ImplementationV1
        } else {
          FactoryTaskResultSchema::FlowV1
        },
        deliverables: if profile.capture.is_some() {
          vec![FactoryTaskDeliverable::new(
            fixtures::key("changeset"),
            FactoryRepositoryPath::new("outputs/changes.bundle").unwrap(),
            true,
          )]
        } else {
          vec![]
        },
      },
      context,
      profile.toolchain.clone(),
      FactoryTaskControlDigests {
        policy: FactoryDigest::from_bytes([1; 32]),
        prompt: FactoryDigest::from_bytes([2; 32]),
        prompt_provenance: FactoryDigest::from_bytes([3; 32]),
        provenance: FactoryDigest::from_bytes([4; 32]),
      },
    )
    .unwrap();
    FactoryRoleInvocation::new(
      FactoryRoleExecutionPackage::new(FactoryRoleExecutionPackageInput {
        project_rules: vec![],
        role,
        profile: selected,
        skills: Default::default(),
        task,
        context: context.clone(),
      })
      .unwrap(),
      None,
      Default::default(),
    )
    .unwrap()
  });
  let retained = Arc::new(crate::factory_command_materializer_tests::Inputs::default());
  if let Some(role) = &role {
    retained
      .retain_factory_role_invocation(
        intent,
        &FactoryClaimOwnership::new(claim.owner.clone(), claim.claim),
        role.retained().clone(),
        at(1002),
      )
      .await
      .unwrap();
  }
  let compiled = compile_codex_flow_task_with_options(
    intent,
    &admitted,
    &profile.instructions,
    &profile.credentials,
    profile.consumer,
    &profile.usage_policy,
    FactoryCodexFlowCompilationOptions {
      authentication: profile.authentication,
      mcp_configurations: mcp_configuration.as_ref().map(std::slice::from_ref),
      role: role.as_ref(),
      context: Some(&context_map),
      capture: profile.capture.as_ref(),
      ..Default::default()
    },
  )
  .unwrap();
  if verify_mcp {
    let schema: serde_json::Value =
      serde_json::from_slice(compiled.document(FactoryProtectedDocumentKind::ResultSchema).bytes()).unwrap();
    assert_eq!(
      schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>(),
      ["assessment", "claims"],
      "only model-authored fields belong in the emitted schema"
    );
    assert_eq!(schema["required"], serde_json::json!(["assessment", "claims"]));
    assert_eq!(
      schema["properties"]["claims"]["items"]["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>(),
      [
        "arguments_json",
        "connection",
        "json_text",
        "pointer",
        "tool",
        "value_json"
      ]
    );
    assert_eq!(
      schema["properties"]["claims"]["items"]["required"],
      serde_json::json!([
        "arguments_json",
        "connection",
        "json_text",
        "pointer",
        "tool",
        "value_json"
      ])
    );
    assert_eq!(
      schema["properties"]["claims"]["items"]["properties"]["json_text"],
      serde_json::json!({"anyOf":[{"type":"integer","minimum":0,"maximum":i64::MAX},{"type":"null"}]}),
      "native structured outputs require optional values to be explicitly nullable"
    );
  }
  let mut fixture = finish_prepared_source(PreparedSourceInput {
    runs,
    base,
    profile,
    intent: intent.clone(),
    context,
    schema,
    compiled,
    source: None,
    capture_paths,
    observation_override,
    model_result: serde_json::json!({"count":9}),
    native_mcp,
    recorded_usage,
  })
  .await;
  fixture.role = selected_role.then_some(retained);
  fixture
}
struct PreparedSourceInput {
  runs: Arc<InMemoryFactoryConfigurationStore>,
  base: StoreContractFixture,
  profile: FactoryCodexBuildProfile,
  intent: FlowBuildIntent,
  context: Arc<dyn FactoryNodeContextSource>,
  schema: FlowDataSchema,
  compiled: CompiledCodexTask,
  capture_paths: Vec<octacity_protocol::CapturedChangeSetPathV1>,
  observation_override: Option<serde_json::Value>,
  model_result: serde_json::Value,
  native_mcp: bool,
  recorded_usage: Option<serde_json::Value>,
  source: Option<(
    octacity_protocol::ChangeSetMaterializationV3,
    Vec<octacity_protocol::ProtectedInputV3>,
  )>,
}
async fn finish_prepared_source(input: PreparedSourceInput) -> SourceFixture {
  let PreparedSourceInput {
    runs,
    base,
    profile,
    intent,
    context,
    schema,
    compiled,
    source,
    capture_paths,
    observation_override,
    model_result,
    native_mcp,
    recorded_usage,
  } = input;
  let canonical = crate::factory_job_test_support::canonical_v3_spec();
  let grants = profile.project_permissions.clone();
  let started_at = if source.is_some() {
    intent.node().started_at().unix_millis() + 1
  } else {
    1012
  };
  let completed_at = started_at + 4;
  let published_at = completed_at + 1;
  let mut manifest = octacity_protocol::ProtectedInputManifestV3 {
    inputs: compiled
      .documents()
      .iter()
      .map(|document| octacity_protocol::ProtectedInputV3 {
        artifact_id: ArtifactId::generate().to_string(),
        destination: document.destination().to_owned(),
        media_type: document.media_type().to_owned(),
        size_bytes: document.bytes().len() as u64,
        sha256: document.content_digest().to_string(),
      })
      .collect(),
  };
  manifest.inputs.sort_by(|a, b| a.artifact_id.cmp(&b.artifact_id));
  let mut program = compiled.managed_program(manifest).unwrap();
  let materialization = source.map(|(instruction, inputs)| {
    program.protected_inputs.inputs.extend(inputs);
    program
      .protected_inputs
      .inputs
      .sort_by(|a, b| a.artifact_id.cmp(&b.artifact_id));
    instruction
  });
  let mut build = base.request.build.clone();
  build.project_id = intent.input().subject().project_id();
  build.repository_id = intent.input().subject().repository_id();
  build.configuration_id = profile.build_configuration.id();
  build.configuration_version = profile.build_configuration.version();
  build.immutable_revision = intent.input().subject().base_revision().clone();
  let mut job = base.request.jobs[0].clone();
  job.job_spec_template = derive_factory_managed_job_spec_template(FactoryManagedJobSpecInput {
    build: JobSpecBuildSnapshot::new(
      build.id,
      build.immutable_revision.clone(),
      None,
      RepositoryLocator::new("https://example.test/repository.git").unwrap(),
      BTreeMap::new(),
    )
    .unwrap(),
    pipeline_node_id: job.pipeline_node_id.clone(),
    execution: program.execution,
    protected_inputs: program.protected_inputs,
    required_enforcement: program.required_enforcement,
    causality: octacity_protocol::FactoryCausalityV3 {
      factory_run_id: intent.input().factory_run_id().to_string(),
      factory_configuration_id: intent.input().configuration().id().to_string(),
      factory_configuration_version: intent.input().configuration().version().get(),
      flow_run_id: intent.input().flow_run_id().to_string(),
      workflow_cycle_id: intent.input().cycle_id().to_string(),
      node_attempt_id: intent.node().id().to_string(),
      input_digest: intent.input().digest().unwrap().to_string(),
      subject_digest: intent.input().subject().digest().to_string(),
      source_write_paths: crate::factory_build_materialization::factory_capture_source_paths(
        &intent,
        profile.capture.as_ref().map(FactoryCodexCapturePolicy::instruction),
      )
      .unwrap(),
      change_set_capture: profile.capture.as_ref().map(|capture| capture.instruction().clone()),
      change_set_materialization: materialization,
    },
    policy_layers: FactoryBuildPolicyLayers {
      project: grants.clone(),
      configuration: grants.clone(),
      task: intent.permissions().clone(),
      local: LocalPermissionCeiling::fully_enforced(grants),
    },
    policy: crate::factory_job_test_support::policy(&canonical),
  })
  .unwrap()
  .with_execution_deadline(intent.node().deadline())
  .unwrap();
  let job_id = job.id;
  let query = Arc::new(Queries {
    history: BTreeMap::new(),
    build: BuildRecord {
      build: build.clone(),
      state: BuildState::Succeeded,
      version: BuildVersion::new(1).unwrap(),
      origin: BuildOrigin::Factory(FactoryBuildOrigin::from_intent(&intent).unwrap()),
      created_at: at(started_at),
      updated_at: at(completed_at),
    },
    attempt: AttemptRecord {
      id: base.request.attempt_id,
      build_id: build.id,
      number: AttemptNumber::FIRST,
      retry_of_attempt_id: None,
      state: AttemptState::Succeeded,
      version: AttemptVersion::new(1).unwrap(),
      created_at: at(started_at),
      updated_at: at(completed_at),
      jobs: vec![JobRecord {
        attempt_id: base.request.attempt_id,
        job,
        state: JobState::Succeeded,
        version: JobVersion::new(1).unwrap(),
        created_at: at(started_at),
        updated_at: at(completed_at),
        queue: None,
        assignment: Some(JobAssignmentRecord {
          pool_id: base.allowed_pool,
          agent_id: base.agent_id,
        }),
        terminal: Some(JobTerminalRecord {
          state: JobState::Succeeded,
          failure_class: None,
          timed_out: false,
          completed_at: at(completed_at),
        }),
        event_cursor: 2,
      }],
    },
  });
  let lease = LeaseAccess {
    lease_id: LeaseId::generate(),
    fence: LeaseFence::from_bytes([31; 32]),
    agent_id: base.agent_id,
    registration_epoch: base.registration_epoch,
  };
  let artifacts = Arc::new(InMemoryArtifactRecordStore::new(ArtifactLeaseFixture {
    access: lease,
    build_id: build.id,
    attempt_id: base.request.attempt_id,
    attempt: AttemptNumber::FIRST,
    job_id,
    expires_at: at(5000),
  }));
  let usage =
    recorded_usage.unwrap_or_else(|| serde_json::json!({"input_tokens":2,"output_tokens":3,"cost_micro_units":1}));
  let mut result=serde_json::to_vec(&serde_json::json!({"format_version":1,"outcome":"completed","structured_result":model_result,"harness_identifiers":{"session_id":"private-session-canary"},"usage":usage})).unwrap();
  let prompt = std::str::from_utf8(compiled.document(FactoryProtectedDocumentKind::Prompt).bytes()).unwrap();
  let selected_revision = intent
    .input()
    .context()
    .and_then(|context| context.subject().candidate())
    .map_or(&build.immutable_revision, |candidate| candidate.candidate_revision());
  let mut provenance=serde_json::to_vec(&serde_json::json!({"format_version":1,"trace_format_version":1,"result_format_version":1,"plugin":{"name":"octa_plugin_codex","version":profile.toolchain.plugin.version()},"codex":{"name":"codex-cli","version":profile.toolchain.executable.version()},"settings":{"model":profile.toolchain.model.identity(),"reasoning_effort":null},"prompt_digest":{"algorithm":"blake3","value":codex_prompt_provenance_digest(prompt)},"source_revision":selected_revision,"timing":{"started_unix_millis":started_at,"finished_unix_millis":completed_at,"duration_millis":4},"outcome":"completed","usage":usage})).unwrap();
  let mut trace = b"{\"format_version\":1,\"sequence\":0,\"event\":{\"type\":\"turn.completed\"}}\n".to_vec();
  let trace_id = ArtifactId::generate();
  if native_mcp {
    (result, trace, provenance) = mcp_evidence::native_records(
      trace_id,
      result,
      provenance,
      started_at,
      observation_override
        .as_ref()
        .and_then(|value| value["native_case"].as_str()),
    );
  }
  let bytes = Arc::new(Bytes::default());
  let mut publications = vec![
    (
      "codex-run-result",
      result,
      ArtifactType::Report(ArtifactReportFormat::new("octa.codex.result.v1").unwrap()),
      "application/json",
    ),
    (
      "codex-run-provenance",
      provenance,
      ArtifactType::Artifact,
      "application/json",
    ),
    ("codex-run-trace", trace, ArtifactType::Artifact, "application/x-ndjson"),
  ];
  if let Some(capture) = &profile.capture {
    use octacity_protocol::{
      CHANGE_SET_BUNDLE_MEDIA_TYPE, CHANGE_SET_BUNDLE_OUTPUT, CHANGE_SET_MANIFEST_MEDIA_TYPE,
      CHANGE_SET_MANIFEST_OUTPUT, CapturedChangeSetFileV1, CapturedChangeSetManifestV1, FactoryImmutableReferenceV3,
    };
    let bundle = b"\xff\0trusted-capture-binary-canary".to_vec();
    let manifest = CapturedChangeSetManifestV1 {
      cumulative_changes: None,
      format_version: 1,
      base_revision: build.immutable_revision.to_string(),
      candidate_revision: "0123456789abcdef0123456789abcdef0123456789".to_owned(),
      node_attempt_id: intent.node().id().to_string(),
      capture_tool: FactoryImmutableReferenceV3 {
        identity: capture.tool().identity().to_string(),
        version: capture.tool().version().to_string(),
        sha256: capture.tool().digest().to_string(),
      },
      changed_paths: capture_paths,
      bundle: CapturedChangeSetFileV1 {
        name: CHANGE_SET_BUNDLE_OUTPUT.to_owned(),
        size_bytes: bundle.len() as u64,
        sha256: FactoryDigest::content_sha256(&bundle).to_string(),
      },
      patch: None,
    };
    let mut manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    manifest_bytes.push(b'\n');
    CapturedChangeSetManifestV1::decode_canonical(&manifest_bytes).unwrap();
    if let Some(output) = &capture.instruction().observation {
      let mut value = serde_json::json!({
        "candidate_revision":manifest.candidate_revision,
        "candidate_digest":manifest.candidate_digest(&FactoryDigest::content_sha256(&manifest_bytes).to_string()).unwrap(),
        "input_digest":intent.input().digest().unwrap().to_string(),"fresh_until":intent.node().deadline().unix_millis()
      });
      if let Some(overrides) = observation_override {
        value
          .as_object_mut()
          .unwrap()
          .extend(overrides.as_object().unwrap().clone());
      }
      publications.push((
        output.name.as_str(),
        serde_json::to_vec(&value).unwrap(),
        ArtifactType::Report(ArtifactReportFormat::new(&output.schema.identity).unwrap()),
        "application/octet-stream",
      ));
    }
    publications.extend([
      (
        CHANGE_SET_BUNDLE_OUTPUT,
        bundle,
        ArtifactType::Artifact,
        CHANGE_SET_BUNDLE_MEDIA_TYPE,
      ),
      (
        CHANGE_SET_MANIFEST_OUTPUT,
        manifest_bytes,
        ArtifactType::Artifact,
        CHANGE_SET_MANIFEST_MEDIA_TYPE,
      ),
    ]);
  }
  for (name, content, kind, media) in publications {
    let id = if name == "codex-run-trace" {
      trace_id
    } else {
      ArtifactId::generate()
    };
    let captured = matches!(
      name,
      octacity_protocol::CHANGE_SET_BUNDLE_OUTPUT | octacity_protocol::CHANGE_SET_MANIFEST_OUTPUT
    ) || profile
      .capture
      .as_ref()
      .and_then(|capture| capture.instruction().observation.as_ref())
      .is_some_and(|output| output.name == name);
    let upload = artifacts
      .begin_artifact_upload(BeginArtifactUpload {
        artifact_id: id,
        upload_id: ArtifactUploadId::generate(),
        idempotency_key: IdempotencyKey::new(name).unwrap(),
        lease,
        job_id,
        attempt: AttemptNumber::FIRST,
        logical_name: ArtifactName::new(name).unwrap(),
        producer_run_id: if captured {
          octacity_protocol::CHANGE_SET_CAPTURE_PRODUCER_ID
        } else {
          1
        },
        producer_task_id: if captured {
          octacity_protocol::CHANGE_SET_CAPTURE_PRODUCER_ID
        } else {
          1
        },
        artifact_type: kind,
        media_type: ArtifactMediaType::new(media).unwrap(),
        transport_media_type: ArtifactMediaType::new(media).unwrap(),
        size_bytes: content.len() as u64,
        digest: ArtifactContentDigest::from_lower_hex(&FactoryDigest::content_sha256(&content).to_string()).unwrap(),
        reserved_at: at(completed_at),
        capability_expires_at: at(3000),
      })
      .await
      .unwrap();
    let verify = VerifyArtifactUpload {
      upload_id: upload.upload.upload_id,
      lease,
      job_id,
      attempt: AttemptNumber::FIRST,
      observed_at: at(published_at),
    };
    artifacts.begin_artifact_verification(verify).await.unwrap();
    artifacts
      .finish_artifact_verification(verify, ArtifactVerificationResult::Verified)
      .await
      .unwrap();
    bytes.0.lock().unwrap().insert(id, content);
  }
  SourceFixture {
    role: None,
    runs,
    query,
    artifacts,
    bytes,
    profile,
    context,
    schema,
    intent: intent.clone(),
  }
}
struct ObservationClock;
impl FactoryExecutionClock for ObservationClock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(1018))
  }
}
struct SourceFixture {
  role: Option<Arc<crate::factory_command_materializer_tests::Inputs>>,
  runs: Arc<InMemoryFactoryConfigurationStore>,
  query: Arc<Queries>,
  artifacts: Arc<InMemoryArtifactRecordStore>,
  bytes: Arc<Bytes>,
  profile: FactoryCodexBuildProfile,
  context: Arc<dyn FactoryNodeContextSource>,
  schema: FlowDataSchema,
  intent: FlowBuildIntent,
}
impl SourceFixture {
  fn source(
    &self,
    query: Arc<Queries>,
  ) -> FactoryCodexBuildOutputSource<InMemoryFactoryConfigurationStore, Queries, InMemoryArtifactRecordStore, Bytes> {
    let source = FactoryCodexBuildOutputSource::new(
      self.runs.clone(),
      query,
      self.artifacts.clone(),
      self.bytes.clone(),
      vec![self.profile.clone()],
      self.context.clone(),
      Arc::new(ObservationClock),
    )
    .unwrap();
    match &self.role {
      Some(role) => source.with_role_invocation_store(role.clone()),
      None => source,
    }
  }
}

struct CandidateObservationClock;
impl FactoryExecutionClock for CandidateObservationClock {
  fn now(&self) -> Result<Timestamp, ApplicationError> {
    Ok(at(2012))
  }
}

async fn prepared_candidate_source() -> (SourceFixture, Arc<dyn FactoryRetainedCandidateContextSource>) {
  struct NoRawCandidate;
  #[async_trait]
  impl FactoryNodeContextSource for NoRawCandidate {
    async fn read_context(
      &self,
      _: &FlowNodeInput,
      _: Timestamp,
      _: Timestamp,
    ) -> Result<BTreeMap<ArtifactId, Vec<u8>>, ApplicationError> {
      panic!("candidate bundle and manifest cannot be interpreted as model text")
    }
  }
  let ordinary = prepared_source().await;
  let previous = ordinary
    .runs
    .factory_run_snapshot(ordinary.intent.input().factory_run_id())
    .await
    .unwrap();
  let profile = ordinary.profile;
  let candidate = crate::factory_candidate_context_test_support::candidate_context_fixture(
    crate::factory_candidate_context_test_support::ReadonlyCandidateFixtureProfile {
      work: previous.work,
      build_configuration: profile.build_configuration.clone(),
      toolchain: profile.toolchain.clone(),
      permissions: profile.project_permissions.clone(),
      credentials: profile.credentials.clone(),
      instructions: profile.instructions.clone(),
    },
  )
  .await;
  let intent = &candidate.intent;
  assert!(at(2012) > intent.node().deadline());
  let schema = candidate
    .admitted
    .closure()
    .definition(intent.input().definition())
    .unwrap()
    .node(intent.input().node())
    .unwrap()
    .outcome(intent.result_outcome())
    .unwrap()
    .schema();
  let schema = candidate.admitted.data_schema(schema).unwrap().clone();
  let compiled = compile_codex_flow_task_with_options(
    intent,
    &candidate.admitted,
    &profile.instructions,
    &profile.credentials,
    profile.consumer,
    &profile.usage_policy,
    FactoryCodexFlowCompilationOptions {
      candidate_context: Some(&candidate.context),
      ..Default::default()
    },
  )
  .unwrap();
  let source_inputs = [
    (
      candidate.context.bundle(),
      octacity_protocol::CHANGE_SET_BUNDLE_INPUT,
      octacity_protocol::CHANGE_SET_BUNDLE_MEDIA_TYPE,
    ),
    (
      candidate.context.manifest(),
      octacity_protocol::CHANGE_SET_MANIFEST_INPUT,
      octacity_protocol::CHANGE_SET_MANIFEST_MEDIA_TYPE,
    ),
  ]
  .into_iter()
  .map(|(record, destination, media_type)| {
    let identity = record.identity();
    octacity_protocol::ProtectedInputV3 {
      artifact_id: identity.artifact_id.to_string(),
      destination: destination.to_owned(),
      media_type: media_type.to_owned(),
      size_bytes: identity.size_bytes,
      sha256: identity.digest.to_string(),
    }
  })
  .collect();
  let fixture = finish_prepared_source(PreparedSourceInput {
    runs: candidate.runs.clone(),
    base: authoritative_store_contract_fixture(),
    profile,
    intent: intent.clone(),
    context: Arc::new(NoRawCandidate),
    schema,
    compiled,
    source: Some((
      octacity_protocol::ChangeSetMaterializationV3 {
        bundle_input: candidate.bundle_id.to_string(),
        manifest_input: candidate.manifest_id.to_string(),
        candidate_revision: "0123456789abcdef0123456789abcdef0123456789".to_owned(),
      },
      source_inputs,
    )),
    capture_paths: Vec::new(),
    observation_override: None,
    model_result: serde_json::json!({"count":9}),
    native_mcp: false,
    recorded_usage: None,
  })
  .await;
  let source = Arc::new(FactoryStoredCandidateContextSource::new(
    candidate.runs,
    candidate.source,
    fixtures::reference("git.capture"),
    Arc::new(CandidateObservationClock),
  ));
  (fixture, source)
}

#[test]
fn installed_candidate_output_source_verifies_frozen_source_inputs_and_candidate_provenance_after_deadline() {
  crate::factory_admission_tests::run_ready(async {
    let (fixture, candidate_source) = prepared_candidate_source().await;
    let managed = fixture.query.attempt.jobs[0]
      .job
      .job_spec_template
      .managed_intent()
      .unwrap();
    assert_eq!(managed.protected_inputs().inputs.len(), 6);
    assert!(managed.factory().source_write_paths.is_none());
    assert!(managed.factory().change_set_capture.is_none());
    assert_eq!(
      fixture.query.build.build.immutable_revision,
      fixture.intent.input().subject().base_revision().clone()
    );
    let source = FactoryCodexBuildOutputSource::new(
      fixture.runs.clone(),
      fixture.query.clone(),
      fixture.artifacts.clone(),
      fixture.bytes.clone(),
      vec![fixture.profile.clone()],
      fixture.context.clone(),
      Arc::new(CandidateObservationClock),
    )
    .unwrap();
    assert!(
      matches!(
        source
          .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 10000)
          .await,
        Err(OrdinaryBuildApplicationError::Invalid)
      ),
      "default candidate observation remains unavailable"
    );
    let source = source.with_retained_candidate_context_source(candidate_source);
    let observed = source
      .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 10000)
      .await
      .expect("exact readonly candidate settlement survives restart beyond execution deadline");
    let FactoryNodeBuildOutputObservation::Verified(outputs) = observed else {
      panic!("valid candidate observation")
    };
    assert_eq!(outputs.documents.len(), 1);
    assert_eq!(
      outputs.documents[0].data.as_ref().unwrap().payload().value(),
      &serde_json::json!({"count":9})
    );
    assert_eq!(outputs.usage.tokens, 5);
    assert_eq!(outputs.usage.cost_micro_units, 1);
  });
}

#[test]
fn candidate_output_source_discards_completion_cancelled_during_output_io() {
  use std::sync::atomic::{AtomicBool, Ordering};
  struct CancelDuringRead {
    bytes: Arc<Bytes>,
    runs: Arc<InMemoryFactoryConfigurationStore>,
    run_id: FactoryRunId,
    first_read: AtomicBool,
  }
  #[async_trait]
  impl ArtifactByteReader for CancelDuringRead {
    async fn read_published(&self, object: &ArtifactObject, maximum: u64) -> Result<Vec<u8>, ArtifactStoreError> {
      if self.first_read.swap(false, Ordering::SeqCst) {
        let snapshot = self.runs.factory_run_snapshot(self.run_id).await.unwrap();
        FactoryControlHandlers::new(self.runs.clone())
          .execute_management_command(
            &ManagementRequestContext::trusted_network(ManagementRequestId::new(uuid::Uuid::new_v4()).unwrap()),
            &ManagementAuthorizationGrant::new(ManagementVisibility::all()),
            CancelFactoryRunCommand {
              run_id: self.run_id,
              expected_version: snapshot.run.version(),
              idempotency_key: IdempotencyKey::new("cancel-during-candidate-output-read").unwrap(),
              requested_at: at(2012),
            },
          )
          .await
          .unwrap();
      }
      self.bytes.read_published(object, maximum).await
    }
  }
  crate::factory_admission_tests::run_ready(async {
    let (fixture, candidate_source) = prepared_candidate_source().await;
    let run_id = fixture.intent.input().factory_run_id();
    let source = FactoryCodexBuildOutputSource::new(
      fixture.runs.clone(),
      fixture.query.clone(),
      fixture.artifacts.clone(),
      Arc::new(CancelDuringRead {
        bytes: fixture.bytes.clone(),
        runs: fixture.runs.clone(),
        run_id,
        first_read: AtomicBool::new(true),
      }),
      vec![fixture.profile.clone()],
      fixture.context.clone(),
      Arc::new(CandidateObservationClock),
    )
    .unwrap()
    .with_retained_candidate_context_source(candidate_source);
    assert!(
      matches!(
        source
          .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 10000)
          .await,
        Err(OrdinaryBuildApplicationError::Invalid)
      ),
      "a cancellation accepted during output I/O cannot produce a current candidate completion"
    );
    let cancelled = fixture.runs.factory_run_snapshot(run_id).await.unwrap();
    assert!(
      cancelled
        .lifecycle_checkpoints
        .iter()
        .find(|row| row.id == cancelled.current.lifecycle_checkpoint_id)
        .unwrap()
        .cancellation_requested
    );
    assert_eq!(
      fixture
        .query
        .job(fixture.query.attempt.jobs[0].id())
        .await
        .unwrap()
        .state,
      JobState::Succeeded,
      "discarding a Factory completion does not rewrite the known ordinary Job outcome or usage publications"
    );
    let retained = fixture
      .artifacts
      .list_published_artifacts(ListPublishedArtifacts {
        build_id: fixture.query.build.build.id,
        attempt_id: Some(fixture.query.attempt.id),
        limit: 4,
        visibility: ArtifactListVisibility::all(),
      })
      .await
      .unwrap();
    assert_eq!(retained.len(), 3);
  });
}

#[test]
fn codex_output_source_rejects_a_retained_result_after_another_build_attempt_became_current() {
  struct NewerAttempt(Arc<Queries>);
  #[async_trait]
  impl BuildQueryStore for NewerAttempt {
    async fn build(&self, id: BuildId) -> Result<BuildRecord, StoreError> {
      self.0.build(id).await
    }
    async fn attempt(&self, id: AttemptId) -> Result<AttemptRecord, StoreError> {
      self.0.attempt(id).await
    }
    async fn job(&self, id: JobId) -> Result<JobRecord, StoreError> {
      self.0.job(id).await
    }
    async fn latest_attempt(&self, id: BuildId) -> Result<AttemptRecord, StoreError> {
      let mut latest = self.0.latest_attempt(id).await?;
      latest.id = AttemptId::generate();
      latest.number = AttemptNumber::new(2).unwrap();
      latest.retry_of_attempt_id = Some(self.0.attempt.id);
      latest.jobs = vec![];
      latest.state = AttemptState::Created;
      Ok(latest)
    }
  }
  crate::factory_admission_tests::run_ready(async {
    let fixture = prepared_source().await;
    let source = FactoryCodexBuildOutputSource::new(
      fixture.runs.clone(),
      Arc::new(NewerAttempt(fixture.query.clone())),
      fixture.artifacts.clone(),
      fixture.bytes.clone(),
      vec![fixture.profile.clone()],
      fixture.context.clone(),
      Arc::new(ObservationClock),
    )
    .unwrap();
    assert!(
      matches!(
        source
          .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 10000)
          .await,
        Err(OrdinaryBuildApplicationError::Invalid)
      ),
      "retained predecessor outputs cannot complete the current node"
    );
  });
}
#[test]
fn installed_codex_task_identity_pins_an_optional_tool_risk_policy_without_changing_the_base_contract() {
  crate::factory_admission_tests::run_ready(async {
    let fixture = prepared_source().await;
    let mut profile = fixture.profile;
    let base = profile.task_digest();
    let mut binding = crate::factory_tool_risk_authorizer_tests::binding_fixture();
    let first = FactoryCodexToolRiskPolicy::from_binding(&binding).unwrap();
    profile.tool_risk = Some(first);
    let pinned = profile.task_digest();
    assert_ne!(pinned, base);
    assert_eq!(profile.base_task_digest(), base);
    assert_eq!(
      profile.tool_risk.unwrap().signal_allowance(),
      BudgetLimit::new(2, 200, 20, 200, 200).unwrap()
    );
    binding.network_host = octacity_server_factory::NetworkHost::new("another-installed-connector.internal").unwrap();
    profile.tool_risk = Some(FactoryCodexToolRiskPolicy::from_binding(&binding).unwrap());
    assert_ne!(
      profile.task_digest(),
      pinned,
      "provider authority cannot silently change an admitted task identity"
    );
    profile.tool_risk = None;
    assert_eq!(
      profile.task_digest(),
      base,
      "the existing un-controlled task identity remains compatible"
    );
  });
}

#[test]
fn codex_output_source_can_settle_retained_usage_after_the_original_node_deadline() {
  struct LateClock;
  impl FactoryExecutionClock for LateClock {
    fn now(&self) -> Result<Timestamp, ApplicationError> {
      Ok(at(2012))
    }
  }
  crate::factory_admission_tests::run_ready(async {
    let fixture = prepared_source().await;
    assert!(at(2012) > fixture.intent.node().deadline());
    let source = FactoryCodexBuildOutputSource::new(
      fixture.runs.clone(),
      fixture.query.clone(),
      fixture.artifacts.clone(),
      fixture.bytes.clone(),
      vec![fixture.profile.clone()],
      fixture.context.clone(),
      Arc::new(LateClock),
    )
    .unwrap();
    let usage = source
      .terminal_usage(fixture.query.build.build.id, fixture.query.attempt.id)
      .await
      .expect("read-only settlement must survive owner restart after its original execution deadline");
    assert_eq!(usage.attempts, 0);
    assert_eq!(usage.tokens, 5);
    assert_eq!(usage.cost_micro_units, 1);
  });
}

#[test]
fn codex_output_source_rejects_provenance_outside_the_executed_job_window() {
  crate::factory_admission_tests::run_ready(async {
    let fixture = prepared_source().await;
    for (created_at, completed_at) in [(at(1013), at(1016)), (at(1012), at(1015))] {
      let mut query = (*fixture.query).clone();
      query.attempt.jobs[0].created_at = created_at;
      query.attempt.jobs[0].terminal.as_mut().unwrap().completed_at = completed_at;
      let source = fixture.source(Arc::new(query));
      assert!(
        matches!(
          source
            .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 10000)
            .await,
          Err(OrdinaryBuildApplicationError::Invalid)
        ),
        "a retained provider report cannot establish its own execution window"
      );
    }
  });
}

#[test]
fn installed_codex_output_source_reads_fenced_ordinary_job_facts_and_retained_artifacts() {
  crate::factory_admission_tests::run_ready(async {
    let fixture = prepared_source().await;
    let source = fixture.source(fixture.query.clone());
    let observed = source
      .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 10000)
      .await
      .unwrap();
    let FactoryNodeBuildOutputObservation::Verified(observed) = observed else {
      panic!("verified retained output must remain a semantic observation")
    };
    assert_eq!(observed.execution.profile, *fixture.intent.profile());
    assert_eq!(observed.documents.len(), 1);
    assert_eq!(
      observed.documents[0].data.as_ref().unwrap().payload().value(),
      &serde_json::json!({"count":9})
    );
    assert_eq!(observed.documents[0].schema, *fixture.schema.reference());
    assert_eq!(observed.usage.tokens, 5);
    assert_eq!(observed.usage.cost_micro_units, 1);
    assert_eq!(observed.verified_at, at(1017));
    let stopped = source
      .terminal_usage(fixture.query.build.build.id, fixture.query.attempt.id)
      .await
      .unwrap();
    assert_eq!(stopped.attempts, 0);
    assert_eq!(stopped.tokens, 5);
  });
}

#[test]
fn codex_output_source_retains_measured_consumption_when_the_ordinary_job_failed_after_the_call() {
  crate::factory_admission_tests::run_ready(async {
    let fixture = prepared_source().await;
    let mut query = (*fixture.query).clone();
    query.build.state = BuildState::Failed;
    query.attempt.state = AttemptState::Failed;
    query.attempt.jobs[0].state = JobState::Failed;
    let terminal = query.attempt.jobs[0].terminal.as_mut().unwrap();
    terminal.state = JobState::Failed;
    terminal.failure_class = Some(octacity_server_job::JobFailureClass::Execution);
    let source = fixture.source(Arc::new(query));
    let observed = source
      .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 10000)
      .await
      .expect("a later ordinary execution failure cannot erase independently measured provider consumption");
    let FactoryNodeBuildOutputObservation::ExecutionStopped { usage } = observed else {
      panic!("a failed ordinary job cannot produce a semantic node result")
    };
    assert_eq!(usage.attempts, 0);
    assert_eq!(usage.tokens, 5);
    assert_eq!(usage.cost_micro_units, 1);
    assert_eq!(
      source
        .terminal_usage(fixture.query.build.build.id, fixture.query.attempt.id)
        .await
        .unwrap(),
      usage
    );
  });
}

#[test]
fn codex_output_source_charges_retained_consumption_from_every_ordinary_retry() {
  crate::factory_admission_tests::run_ready(async {
    let fixture = prepared_source().await;
    let mut query = (*fixture.query).clone();
    let mut previous = query.attempt.clone();
    previous.id = AttemptId::generate();
    previous.state = AttemptState::Failed;
    previous.jobs[0].attempt_id = previous.id;
    previous.jobs[0].job.id = JobId::generate();
    previous.jobs[0].state = JobState::Failed;
    previous.jobs[0].terminal.as_mut().unwrap().state = JobState::Failed;
    previous.jobs[0].terminal.as_mut().unwrap().failure_class =
      Some(octacity_server_job::JobFailureClass::Infrastructure);
    query.attempt.number = AttemptNumber::new(2).unwrap();
    query.attempt.retry_of_attempt_id = Some(previous.id);
    query.history.insert(previous.id, previous.clone());
    let current = fixture
      .artifacts
      .list_published_artifacts(ListPublishedArtifacts {
        build_id: query.build.build.id,
        attempt_id: Some(query.attempt.id),
        limit: 4,
        visibility: ArtifactListVisibility::all(),
      })
      .await
      .unwrap();
    let mut records = vec![];
    for upload in current {
      let mut identity = upload.artifact.identity().clone();
      let content = fixture.bytes.0.lock().unwrap()[&identity.artifact_id].clone();
      identity.artifact_id = ArtifactId::generate();
      identity.attempt_id = previous.id;
      identity.job_id = previous.jobs[0].id();
      let artifact = ArtifactRecord::restore(
        identity.clone(),
        upload.artifact.state(),
        upload.artifact.version(),
        upload.artifact.created_at(),
        upload.artifact.published_at(),
        None,
      )
      .unwrap();
      fixture.bytes.0.lock().unwrap().insert(identity.artifact_id, content);
      records.push(ArtifactUploadRecord {
        artifact,
        upload_id: ArtifactUploadId::generate(),
        ..upload
      });
    }
    let artifacts = Arc::new(HistoryArtifacts {
      current: fixture.artifacts.clone(),
      previous: records,
    });
    let source = FactoryCodexBuildOutputSource::new(
      fixture.runs.clone(),
      Arc::new(query),
      artifacts,
      fixture.bytes.clone(),
      vec![fixture.profile.clone()],
      fixture.context.clone(),
      Arc::new(ObservationClock),
    )
    .unwrap();
    let observed = source
      .published_outputs(fixture.query.build.build.id, fixture.query.attempt.id, 10000)
      .await
      .unwrap();
    let FactoryNodeBuildOutputObservation::Verified(observed) = observed else {
      panic!("the latest successful retry has a verified result")
    };
    assert_eq!(
      observed.usage.tokens, 10,
      "the successful retry cannot erase five tokens spent by the failed predecessor"
    );
    assert_eq!(observed.usage.cost_micro_units, 2);
    let stopped = source
      .terminal_usage(fixture.query.build.build.id, fixture.query.attempt.id)
      .await
      .unwrap();
    assert_eq!(stopped.attempts, 0);
    assert_eq!(stopped.tokens, 10);
  });
}

fn observation_capture_policy() -> FactoryCodexCapturePolicy {
  let legacy = capture_policy(true, 2);
  let schema = fixtures::supplied_base_flow_template()
    .data_schemas()
    .unwrap()
    .remove(&fixtures::key("candidate_capture_facts"))
    .unwrap();
  let mut instruction = legacy.instruction().clone();
  instruction.observation = Some(octacity_protocol::ChangeSetObservationOutputV3 {
    name: "operator.capture-summary".into(),
    schema: octacity_protocol::FactoryImmutableReferenceV3 {
      identity: schema.reference().identity().to_string(),
      version: schema.reference().version().to_string(),
      sha256: schema.reference().digest().to_string(),
    },
  });
  FactoryCodexCapturePolicy::new(legacy.tool().clone(), instruction).unwrap()
}

#[test]
fn capture_observer_exposes_only_independently_verified_candidate_report_data() {
  crate::factory_admission_tests::run_ready(async {
    let fixture = prepared_source_with_capture(Some(observation_capture_policy())).await;
    let FactoryNodeBuildOutputObservation::Verified(outputs) = fixture
      .source(fixture.query.clone())
      .published_outputs(
        fixture.query.build.build.id,
        fixture.query.attempt.id,
        fixture.intent.budget().max_output_bytes(),
      )
      .await
      .expect("the installed capture report is an independently verified ordinary output")
    else {
      panic!("a successful candidate must retain its trusted report");
    };
    let execution = FlowBuildExecution::new(
      &fixture.intent,
      fixture.query.build.build.id,
      fixture.query.attempt.id,
      vec![fixture.query.attempt.jobs[0].id()],
    )
    .unwrap();
    let capture = fixture.profile.capture.as_ref().unwrap();
    let candidate =
      verify_factory_node_change_set(&fixture.intent, &execution, &outputs, capture.tool(), at(1018)).unwrap();
    let report = outputs
      .documents
      .iter()
      .find(|document| document.record.identity().logical_name.as_str() == "operator.capture-summary")
      .unwrap();
    assert_eq!(
      report
        .data
        .as_ref()
        .expect("only verified Report bytes become Flow data")
        .payload()
        .value(),
      &serde_json::json!({"candidate_revision":"0123456789abcdef0123456789abcdef0123456789",
        "candidate_digest":candidate.subject().changeset_digest(),
        "input_digest":fixture.intent.input().digest().unwrap(),
        "fresh_until":fixture.intent.node().deadline().unix_millis()})
    );
    assert_eq!(&report.tool, capture.tool());
    assert_eq!(outputs.documents.len(), 4);
    assert!(
      outputs
        .documents
        .iter()
        .filter(|document| matches!(document.record.identity().artifact_type, ArtifactType::Artifact))
        .all(|document| document.data.is_none()),
      "capture bundle and manifest remain opaque; the closed Report carries no private file bytes"
    );
  });
}

#[test]
fn capture_observer_rejects_schema_valid_but_substituted_candidate_facts() {
  crate::factory_admission_tests::run_ready(async {
    for (field, value) in [
      (
        "candidate_revision",
        serde_json::json!("ffffffffffffffffffffffffffffffffffffffff"),
      ),
      ("candidate_digest", serde_json::json!("ff".repeat(32))),
      ("input_digest", serde_json::json!("ee".repeat(32))),
      ("fresh_until", serde_json::json!(9007199254740991_i64)),
    ] {
      let fixture = prepared_source_with_capture_observation(
        Some(observation_capture_policy()),
        vec![],
        Some(serde_json::json!({field:value})),
      )
      .await;
      assert!(
        matches!(
          fixture
            .source(fixture.query.clone())
            .published_outputs(
              fixture.query.build.build.id,
              fixture.query.attempt.id,
              fixture.intent.budget().max_output_bytes()
            )
            .await,
          Err(OrdinaryBuildApplicationError::Invalid)
        ),
        "schema-valid {field} substitutions must not become trusted candidate facts"
      );
    }
  });
}

#[test]
fn installed_capture_observations_cannot_reuse_model_publication_names() {
  let valid = observation_capture_policy();
  for name in ["codex-run-result", "codex-run-provenance", "codex-run-trace"] {
    let mut instruction = valid.instruction().clone();
    instruction.observation.as_mut().unwrap().name = name.into();
    assert!(
      FactoryCodexCapturePolicy::new(valid.tool().clone(), instruction).is_err(),
      "a trusted capture Report must not replace the model's independently verified {name}"
    );
  }
}
#[test]
fn installed_model_policy_accepts_independent_mcp_profiles_and_rejects_other_consumers() {
  crate::factory_admission_tests::run_ready(async {
    let mut profile = prepared_source().await.profile;
    let approved = approved_observer_policy(&profile);
    let base = serde_json::to_value(&profile.project_permissions).unwrap();
    for extra in ["mcp-read", "source-only", "delivery-only", "author-only"] {
      let mut grants = base.clone();
      grants["secret_profiles"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!(extra));
      profile.project_permissions = serde_json::from_value(grants).unwrap();
      profile.local_permissions = LocalPermissionCeiling::fully_enforced(profile.project_permissions.clone());
      assert_eq!(
        profile.validate_installation(&approved).is_ok(),
        extra == "mcp-read",
        "{extra}"
      );
    }
  });
}

#[test]
fn installed_reviewer_policy_repins_the_task_and_qualifies_only_readonly_evaluation_with_candidate() {
  crate::factory_admission_tests::run_ready(async {
    let fixture = prepared_source().await;
    let mut profile = fixture.profile;
    let mut spec = crate::factory_job_test_support::canonical_v3_spec();
    spec
      .octa
      .plugin_digests
      .insert("codex".into(), profile.toolchain.plugin.digest().to_string());
    let approved = crate::JobSpecToolchainPolicy {
      source: octacity_server_job::SourcePluginPolicy::new(
        spec.source.provider,
        spec.source.plugin_version,
        spec.source.plugin_sha256,
        "url",
      )
      .unwrap(),
      octa: spec.octa,
      validity: octacity_server_job::JobSpecValidity::new(60).unwrap(),
    };
    profile.validate_installation(&approved).unwrap();
    let legacy = profile.task_digest();
    profile.reviewer = Some(FactoryCodexReviewerPolicy {
      evaluator: fixtures::reference("installed-evaluator"),
    });
    assert_ne!(
      legacy,
      profile.task_digest(),
      "review authority requires an immutable task repin"
    );
    assert!(
      profile.validate_installation(&approved).is_err(),
      "a review must select a qualified retained candidate"
    );
    profile.candidate_capture_tool = Some(fixtures::reference("installed-capture"));
    profile.validate_installation(&approved).unwrap();
    let selected = profile.task_digest();
    profile.reviewer.as_mut().unwrap().evaluator = fixtures::reference("replacement-evaluator");
    assert_ne!(
      selected,
      profile.task_digest(),
      "an evaluator substitution cannot retain the task identity"
    );
    {
      let mut incompatible = profile.clone();
      incompatible.consumer = FactoryCredentialConsumer::CodingNode;
      assert!(incompatible.validate_installation(&approved).is_err());
    }
    {
      let mut incompatible = profile.clone();
      incompatible.tool_response_limits = Some(octacity_protocol::FactoryToolResponseLimitsV3 {
        per_call_bytes: 1,
        total_bytes: 1,
      });
      assert!(
        incompatible.validate_installation(&approved).is_err(),
        "evaluation is tool-free, including hidden installed tool policy"
      );
    }
    profile.reviewer = None;
    assert_eq!(
      profile.task_digest(),
      legacy,
      "unselected optional policy preserves existing task pins"
    );
  });
}

#[path = "factory_codex_review_tests.rs"]
mod review;

#[test]
fn selected_role_outputs_recover_the_original_package_and_verify_the_ordinary_job() {
  crate::factory_admission_tests::run_ready(async {
    for capture in [None, Some(capture_policy(true, 2))] {
      let fixture = prepared_role_source(capture, vec![], None, true).await;
      let source = fixture.source(fixture.query.clone());
      let FactoryNodeBuildOutputObservation::Verified(outputs) = source
        .published_outputs(
          fixture.query.build.build.id,
          fixture.query.attempt.id,
          fixture.intent.budget().max_output_bytes(),
        )
        .await
        .unwrap()
      else {
        panic!("a selected role must retain its ordinary verified result");
      };
      assert_eq!(outputs.execution.profile, *fixture.intent.profile());
      assert_eq!(
        outputs.documents[0].data.as_ref().unwrap().payload().value(),
        &serde_json::json!({"count":9})
      );
    }
  });
}

fn mcp_verifier() -> ImmutableReference {
  ImmutableReference::new(
    fixtures::key("native-mcp-evidence"),
    fixtures::key("v1"),
    FactoryDigest::content_sha256(b"octacity.factory.native-mcp-evidence.v1"),
  )
}

#[test]
fn ordinary_mcp_evidence_result_requires_observable_native_capture_instead_of_accepting_model_only_output() {
  crate::factory_admission_tests::run_ready(async {
    let f = prepared_role_source_with_mcp_evidence(
      None,
      vec![],
      None,
      true,
      Some(FactoryMcpServer::Http {
        url: "https://reader.example.test/mcp".into(),
        bearer_token_env_var: None,
        enabled_tools: Some(vec!["lookup".into()]),
      }),
      true,
      false,
    )
    .await;
    let source = f.source(f.query.clone()).with_runtime_settings_store(f.runs.clone());
    assert!(
      source
        .published_outputs(
          f.query.build.build.id,
          f.query.attempt.id,
          f.intent.budget().max_output_bytes()
        )
        .await
        .is_err(),
      "legacy/model-only capture cannot satisfy the explicitly selected MCP evidence verifier"
    );
  });
}

#[test]
fn configured_mcp_outputs_verify_original_transport_after_settings_replacement() {
  crate::factory_admission_tests::run_ready(async {
    let fixture = prepared_role_source_with_mcp(
      None,
      vec![],
      None,
      true,
      Some(FactoryMcpServer::Http {
        url: "https://reader.example.test/original".into(),
        bearer_token_env_var: None,
        enabled_tools: None,
      }),
    )
    .await;
    let old = fixture
      .runs
      .factory_runtime_settings(
        fixture.intent.input().subject().project_id(),
        &fixtures::key("profile.0"),
        None,
      )
      .await
      .unwrap()
      .unwrap();
    let mut definition = old.definition().clone();
    definition.mcp_servers.clear();
    fixture
      .runs
      .publish_factory_runtime_settings(management_mutation(PublishFactoryRuntimeSettings {
        intent: FactoryRuntimeSettingsIntent {
          project_id: old.project_id(),
          runtime: old.runtime().clone(),
          expected_version: Some(old.version()),
          definition,
        },
        idempotency_key: "remove-output-mcp".parse().unwrap(),
        published_at: at(1003),
      }))
      .await
      .unwrap();
    let source = fixture
      .source(fixture.query.clone())
      .with_runtime_settings_store(fixture.runs.clone());
    let FactoryNodeBuildOutputObservation::Verified(outputs) = source
      .published_outputs(
        fixture.query.build.build.id,
        fixture.query.attempt.id,
        fixture.intent.budget().max_output_bytes(),
      )
      .await
      .unwrap()
    else {
      panic!("output verification must resolve the retained transport rather than current defaults");
    };
    assert_eq!(
      outputs.documents[0].data.as_ref().unwrap().payload().value(),
      &serde_json::json!({"count":9})
    );
  });
}
