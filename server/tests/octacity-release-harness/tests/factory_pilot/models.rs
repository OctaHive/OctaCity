//! Operator-selected deterministic model; the production compiler and Agent stay real.
use super::{ReleaseBackend, required_path, resources::PilotResources, set_mode};
use octacity_release_harness::InstalledRelease;
use octacity_runner::RunnerInstallation;
use octacity_server_application::{FactoryCodexBuildProfile, FactoryCodexUsagePolicy};
use octacity_server_factory::*;
use sha2::{Digest as _, Sha256};
use std::{
  collections::BTreeMap,
  fs,
  path::{Path, PathBuf},
};

pub(super) struct InstalledModel {
  pub(super) profile: FactoryCodexBuildProfile,
  pub(super) executable: PathBuf,
  pub(super) platform: String,
  pub(super) capture_executable: Option<PathBuf>,
}

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
fn reference(name: &str, version: &str, sha256: &str) -> ImmutableReference {
  ImmutableReference::new(key(name), key(version), FactoryDigest::from_lower_hex(sha256).unwrap())
}

impl InstalledModel {
  pub(super) fn observer(
    directory: &Path,
    release: &InstalledRelease,
    resources: &PilotResources,
    backend: &ReleaseBackend,
  ) -> Self {
    let executable = directory.join("finite-model-cli");
    fs::copy(required_path("OCTACITY_FACTORY_PILOT_CLI", true), &executable).unwrap();
    set_mode(&executable, 0o700);
    let runner = RunnerInstallation::load(&release.octa_root).unwrap();
    let pin = &runner.plugins["codex"];
    let plugin = reference("codex", &pin.version, &pin.sha256);
    let executable_pin = reference(
      "codex-cli",
      "0.161.0",
      &hex::encode(Sha256::digest(fs::read(&executable).unwrap())),
    );
    let instructions = FactoryText::new(
      serde_json::json!({
        "fixture_version":1,"result":{"$input":"/payload/value"}
      })
      .to_string(),
    )
    .unwrap();
    let model = ImmutableReference::new(
      key("finite-json-model"),
      key("v1"),
      FactoryDigest::content_sha256(instructions.as_str().as_bytes()),
    );
    let exact = |argument: &str| CommandArgumentPattern::Exact(FactoryText::new(argument).unwrap());
    let run_directory = format!("run-{}", blake3::hash(b"codex").to_hex());
    let arguments = [
      "--ask-for-approval=never".to_owned(),
      "exec".to_owned(),
      "--json".to_owned(),
      "--sandbox=workspace-write".to_owned(),
      format!("--model={}", model.identity()),
      format!("--output-schema=/workspace/output/codex-runs/{run_directory}/.records.tmp/.result-schema.json"),
      "-".to_owned(),
    ];
    let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
      plugins: runner
        .plugins
        .iter()
        .map(|(name, pin)| reference(name, &pin.version, &pin.sha256))
        .collect(),
      executables: vec![executable_pin.clone()],
      commands: vec![
        CommandPermission::try_new(executable_pin.clone(), vec![exact("--version")]).unwrap(),
        CommandPermission::try_new(
          executable_pin.clone(),
          arguments.iter().map(|argument| exact(argument)).collect(),
        )
        .unwrap(),
      ],
      max_descendants: 63,
      mounts: vec![
        MountPermission::new(
          FactoryPath::new(octacity_protocol::PROTECTED_INPUT_ROOT).unwrap(),
          MountMode::ReadOnly,
        ),
        MountPermission::new(
          FactoryPath::new(octacity_protocol::FACTORY_SCRATCH_ROOT).unwrap(),
          MountMode::ReadWrite,
        ),
        MountPermission::new(
          FactoryPath::new(octacity_protocol::FACTORY_OUTPUT_ROOT).unwrap(),
          MountMode::ReadWrite,
        ),
      ],
      secret_profiles: vec![key("pilot-evaluator")],
      resources: FactoryResourceLimits::new(1000, 536_870_912, backend.workspace_bytes(), 64, 120_000).unwrap(),
      outputs: FactoryOutputPermissions::try_new(
        ["codex-run-result", "codex-run-provenance", "codex-run-trace"]
          .into_iter()
          .map(key)
          .collect(),
        2,
        4 * 1024 * 1024,
        1,
        1024 * 1024,
      )
      .unwrap(),
      ..Default::default()
    })
    .unwrap();
    Self {
      profile: FactoryCodexBuildProfile {
        mcp_configurations: vec![],
        toolchain: FactoryTaskToolchain {
          plugin,
          executable: executable_pin,
          model: model.clone(),
        },
        build_configuration: resources.configuration.clone(),
        instructions,
        credentials: FactoryCredentialProfiles::new(
          key("pilot-author"),
          key("pilot-evaluator"),
          key("pilot-source"),
          key("pilot-delivery"),
        )
        .unwrap(),
        consumer: FactoryCredentialConsumer::ReadOnlyReview,
        usage_policy: FactoryCodexUsagePolicy::TokenRates {
          model,
          input_micro_units_per_million: 1_000_000,
          cached_input_micro_units_per_million: 1_000_000,
          output_micro_units_per_million: 1_000_000,
        },
        tool_risk: None,
        tool_response_limits: None,
        reviewer: None,
        candidate_capture_tool: None,
        capture: None,
        pipeline_node: resources.pipeline_node.clone(),
        parameters: BTreeMap::new(),
        project_permissions: permissions.clone(),
        local_permissions: LocalPermissionCeiling::fully_enforced(permissions),
      },
      executable,
      platform: runner.capabilities.platform,
      capture_executable: None,
    }
  }
}

pub(super) const CANDIDATE_FILE: &str = "/workspace/source/public/facts.txt";
pub(super) const CANDIDATE_FACTS: &str = "verified-candidate-observation";
pub(super) fn candidate_schema() -> FlowDataSchema {
  FlowDataSchema::new(
    key("exact-candidate-observation"),
    key("v1"),
    FlowValueSchema::Object {
      fields: BTreeMap::from([
        (
          key("candidate_revision"),
          FlowFieldSchema::required(FlowValueSchema::String {
            min_bytes: 40,
            max_bytes: 64,
          }),
        ),
        (
          key("candidate_digest"),
          FlowFieldSchema::required(FlowValueSchema::String {
            min_bytes: 64,
            max_bytes: 64,
          }),
        ),
        (
          key("input_digest"),
          FlowFieldSchema::required(FlowValueSchema::String {
            min_bytes: 64,
            max_bytes: 64,
          }),
        ),
        (
          key("fresh_until"),
          FlowFieldSchema::required(FlowValueSchema::Integer {
            minimum: 0,
            maximum: 9007199254740991,
          }),
        ),
      ]),
    },
  )
  .unwrap()
}
impl InstalledModel {
  pub(super) async fn coder(
    directory: &Path,
    release: &InstalledRelease,
    resources: &PilotResources,
    backend: &ReleaseBackend,
  ) -> Self {
    use octacity_protocol::*;
    let mut installed = Self::observer(directory, release, resources, backend);
    let git = super::required_path("OCTACITY_CONTRACT_GIT_EXECUTABLE", true)
      .canonicalize()
      .unwrap();
    let version = tokio::time::timeout(
      std::time::Duration::from_secs(5),
      tokio::process::Command::new(&git)
        .arg("--version")
        .kill_on_drop(true)
        .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(version.status.success());
    let version = String::from_utf8(version.stdout)
      .unwrap()
      .split_whitespace()
      .nth(2)
      .unwrap()
      .to_owned();
    let capture_tool = reference("git", &version, &hex::encode(Sha256::digest(fs::read(&git).unwrap())));
    let schema = candidate_schema();
    let profile = &mut installed.profile;
    profile.consumer = FactoryCredentialConsumer::CodingNode;
    profile.instructions=FactoryText::new(serde_json::json!({"fixture_version":1,
      "files":[{"path":CANDIDATE_FILE,"text":"facts"}],
      "checks":[{"path":"/workspace/output/private-contract.rs","readable":false},{"path":"/octacity/protected/test-inventory.json","readable":false}],
      "result":"implemented"}).to_string()).unwrap();
    profile.toolchain.model = ImmutableReference::new(
      profile.toolchain.model.identity().clone(),
      profile.toolchain.model.version().clone(),
      FactoryDigest::content_sha256(profile.instructions.as_str().as_bytes()),
    );
    profile.usage_policy = FactoryCodexUsagePolicy::TokenRates {
      model: profile.toolchain.model.clone(),
      input_micro_units_per_million: 1_000_000,
      cached_input_micro_units_per_million: 1_000_000,
      output_micro_units_per_million: 1_000_000,
    };
    profile.capture = Some(
      octacity_server_application::FactoryCodexCapturePolicy::new(
        capture_tool,
        ChangeSetCaptureV3 {
          author_name: "Qualified pilot".into(),
          author_email: "pilot@example.test".into(),
          committed_at: 1,
          patch_max_bytes: None,
          policy: ChangeSetCapturePolicyV3 {
            allowed_path_prefixes: vec!["public".into()],
            forbidden_control_paths: vec![".github".into(), ".octa".into(), "Octafile.yml".into()],
            max_changed_paths: 1,
            max_file_bytes: 1024,
            max_total_bytes: 1024,
            allow_empty: false,
            binary_policy: ChangeSetBinaryPolicyV3::Reject,
            forbidden_secret_patterns: ChangeSetSecretPatternV3::ALL.to_vec(),
          },
          observation: Some(ChangeSetObservationOutputV3 {
            name: CANDIDATE_FACTS.into(),
            schema: FactoryImmutableReferenceV3 {
              identity: schema.reference().identity().to_string(),
              version: schema.reference().version().to_string(),
              sha256: schema.reference().digest().to_string(),
            },
          }),
        },
      )
      .unwrap(),
    );
    let mut grants = serde_json::to_value(&profile.project_permissions).unwrap();
    grants["secret_profiles"] = serde_json::json!([profile.credentials.profile(FactoryCredentialPurpose::Model)]);
    grants["mounts"]
      .as_array_mut()
      .unwrap()
      .push(serde_json::json!({"root":FACTORY_SOURCE_ROOT,"mode":"read_write"}));
    grants["outputs"] = serde_json::json!({"kinds":["codex-run-result","codex-run-provenance","codex-run-trace",CHANGE_SET_BUNDLE_OUTPUT,CHANGE_SET_MANIFEST_OUTPUT,CANDIDATE_FACTS],"max_artifact_count":4,"max_artifact_bytes":4*1024*1024,"max_report_count":2,"max_report_bytes":1024*1024});
    profile.project_permissions = serde_json::from_value(grants).unwrap();
    profile.local_permissions = LocalPermissionCeiling::fully_enforced(profile.project_permissions.clone());
    installed.capture_executable = Some(git);
    installed
  }
  pub(super) fn coder_entry(&self) -> serde_json::Value {
    let p = &self.profile;
    assert_eq!(p.consumer, FactoryCredentialConsumer::CodingNode);
    let capture = p.capture.as_ref().unwrap();
    serde_json::json!({"toolchain":{"plugin":p.toolchain.plugin,"executable":p.toolchain.executable,"model":p.toolchain.model},"build_configuration":p.build_configuration,"instructions":p.instructions,"credentials":p.credentials,"credential_purpose":"model","usage_policy":p.usage_policy,"pipeline_node":p.pipeline_node,"parameters":p.parameters,"project_permissions":p.project_permissions,"local_permissions":{"grants":p.local_permissions.grants(),"enforceable":PermissionCategory::ALL},"capture":{"tool":capture.tool(),"instruction":capture.instruction()}})
  }
}
