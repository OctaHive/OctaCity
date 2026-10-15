//! Operator-owned file profiles reach the same qualified native installers.
use super::*;
use octacity_server_factory::{FactoryKey, FlowNodeKind, ImmutableReference, NodeExecutionIdentity};
use std::sync::Arc;

struct Ready;
#[async_trait::async_trait]
impl FactoryRuntimeReadiness for Ready {
  async fn available(&self) -> bool {
    true
  }
}

#[tokio::test]
async fn file_review_planning_profile_installs_its_exact_native_executor_without_io() {
  let project = octacity_server_domain::ProjectId::generate();
  let profile = super::factory_tests::review_planning_profile(project);
  let identity = ImmutableReference::new(
    FactoryKey::new("installed-review-policy").unwrap(),
    FactoryKey::new("v1").unwrap(),
    profile.digest().unwrap(),
  );
  let document = serde_json::json!({
    "version": 1,
    "projects": [{"project_id":project,"references":[],"build_configurations":[]}],
    "codex": [],
    "review_planning": [{"identity":identity,"profile":profile}],
  });
  let catalog = FactoryRuntimeProfileCatalog::parse_json(&serde_json::to_vec(&document).unwrap())
    .expect("an exact installed review policy is deployable through the native file profile");
  let installed = FactoryProfileRuntimeInstaller::new(catalog, Arc::new(Ready), vec![])
    .unwrap()
    .install(super::factory_tests::resources())
    .await
    .unwrap();
  assert_eq!(installed.executors.len(), 1);
  assert_eq!(installed.executors[0].0, FlowNodeKind::TrustedAction);
  assert_eq!(installed.executors[0].1, NodeExecutionIdentity::External(identity));
}

#[tokio::test]
async fn file_validation_evidence_profile_installs_original_input_verification_without_io() {
  use octacity_server_application::{FactoryProtectedTestPolicy, FactoryProtectedValidationEvidenceProfile};
  use octacity_server_factory::*;
  let key = |name: &str| FactoryKey::new(name).unwrap();
  let reference = |name: &str| ImmutableReference::new(key(name), key("v1"), FactoryDigest::from_bytes([17; 32]));
  let project = octacity_server_domain::ProjectId::generate();
  let profile = FactoryProtectedValidationEvidenceProfile {
    action: key("publish-evidence"),
    ready: FlowOutcomeDefinition::new(key("verified"), FlowOutcomeKind::Success, reference("manifest-schema")),
    report: EvidenceRequirement::new(
      key("check-report"),
      EvidenceOutputKind::Report,
      reference("report-schema"),
      reference("verifier-tool"),
      reference("verifier-plugin"),
    ),
    facts: EvidenceRequirement::new(
      key("check-facts"),
      EvidenceOutputKind::Report,
      reference("facts-schema"),
      reference("verifier-tool"),
      reference("verifier-plugin"),
    ),
  };
  let identity = ImmutableReference::new(key("installed-evidence-owner"), key("v1"), profile.digest().unwrap());
  let policy = FactoryProtectedTestPolicy::new(
    ProtectedTestCoverage::new(vec![key("contract")], 4, 9000, 4).unwrap(),
    900,
  )
  .unwrap();
  let inventory = EvidenceRequirement::new(
    key("private-inventory"),
    EvidenceOutputKind::Artifact,
    reference("inventory-schema"),
    reference("capture-tool"),
    reference("capture-plugin"),
  );
  let document = serde_json::json!({
    "version":1,"projects":[{"project_id":project,"references":[],"build_configurations":[]}],"codex":[],
    "protected_validation_evidence":[{"identity":identity,"profile":profile,"author_profile":reference("test-author-policy"),"policy":policy,"inventory":inventory,"capture_tool":reference("source-capture")}],
  });
  let installer =
    super::factory_profile_file::FileProfileInstaller::parse_json(&serde_json::to_vec(&document).unwrap(), &[])
      .expect("file deployment retains the concrete original-input verifier until resources are supplied");
  let installed = installer.install(super::factory_tests::resources()).await.unwrap();
  assert_eq!(installed.executors.len(), 1);
  assert_eq!(installed.executors[0].0, FlowNodeKind::TrustedAction);
  assert_eq!(installed.executors[0].1, NodeExecutionIdentity::External(identity));
}

#[tokio::test]
async fn file_command_profile_installs_its_exact_native_executor_without_io() {
  let project = octacity_server_domain::ProjectId::generate();
  let profile = super::factory_command_runtime_tests::command_profile(project);
  let entry = command_entry(&profile);
  let document = serde_json::json!({"version":1,"projects":[{"project_id":project,"references":[],"build_configurations":[{"alias":"installed-command-build","reference":profile.build_configuration}]}],"codex":[],"commands":[entry]});
  let installer =
    super::factory_profile_file::FileProfileInstaller::parse_json(&serde_json::to_vec(&document).unwrap(), &[])
      .expect("operator command survives native file deployment");
  let mut resources = super::factory_tests::resources();
  resources.toolchain.octa.plugin_digests.insert(
    profile.plugin.identity().to_string(),
    profile.plugin.digest().to_string(),
  );
  let installed = installer.install(resources).await.unwrap();
  assert_eq!(installed.executors.len(), 1);
  assert!(
    installed.output_finalizer.is_none(),
    "ordinary command installation keeps its original completion path"
  );
  assert_eq!(installed.executors[0].0, FlowNodeKind::BuildCommand);
  assert_eq!(
    installed.executors[0].1,
    NodeExecutionIdentity::External(profile.executor)
  );
  assert!(
    FactoryRuntimeProfileCatalog::parse_json(&serde_json::to_vec(&document).unwrap()).is_err(),
    "metadata-only parser cannot silently drop native commands"
  );
}

fn command_entry(profile: &octacity_server_application::FactoryCommandBuildProfile) -> serde_json::Value {
  use octacity_server_factory::PermissionCategory;
  serde_json::json!({
    "identity":profile.identity,"plugin":profile.plugin,"executor":profile.executor,
    "build_configuration":profile.build_configuration,"pipeline_node":profile.pipeline_node,
    "program":profile.program,"input_destination":profile.input_destination,
    "parameters":profile.parameters,"project_permissions":profile.project_permissions,
    "local_permissions":{"grants":profile.local_permissions.grants(),"enforceable":PermissionCategory::ALL},
    "outputs":profile.outputs,"candidate_capture_tool":profile.candidate_capture_tool,
  })
}

#[test]
fn file_command_refuses_a_build_configuration_outside_its_project_choices() {
  let project = octacity_server_domain::ProjectId::generate();
  let profile = super::factory_command_runtime_tests::command_profile(project);
  let document = serde_json::json!({"version":1,"projects":[{"project_id":project,"references":[],"build_configurations":[]}],"codex":[],"commands":[command_entry(&profile)]});
  assert!(
    super::factory_profile_file::FileProfileInstaller::parse_json(&serde_json::to_vec(&document).unwrap(), &[])
      .is_err(),
    "a command cannot introduce an undeclared Build Configuration into a Project installation"
  );
}

#[tokio::test]
async fn file_promotion_profile_installs_the_same_exact_approved_command_plugin() {
  use octacity_server_factory::*;
  let project = octacity_server_domain::ProjectId::generate();
  let mut profile = super::factory_command_runtime_tests::command_profile(project);
  profile.executor = profile.plugin.clone();
  profile.candidate_capture_tool = Some(profile.identity.clone());
  let output = &profile.outputs[0];
  profile.outputs = vec![EvidenceRequirement::new(
    output.kind().clone(),
    output.output_kind(),
    output.schema().clone(),
    profile.executor.clone(),
    profile.plugin.clone(),
  )];
  let document = serde_json::json!({"version":1,"projects":[{"project_id":project,"references":[],"build_configurations":[{"alias":"installed-command-build","reference":profile.build_configuration}]}],"codex":[],"promotions":[{"identity":profile.executor,"profiles":[command_entry(&profile)]}]});
  let installer =
    super::factory_profile_file::FileProfileInstaller::parse_json(&serde_json::to_vec(&document).unwrap(), &[])
      .expect("promotion static command is installed explicitly");
  let mut resources = super::factory_tests::resources();
  resources.toolchain.octa.plugin_digests.insert(
    profile.plugin.identity().to_string(),
    profile.plugin.digest().to_string(),
  );
  let installed = installer.install(resources).await.unwrap();
  assert_eq!(installed.executors.len(), 1);
  assert_eq!(installed.executors[0].0, FlowNodeKind::TrustedAction);
  assert_eq!(
    installed.executors[0].1,
    NodeExecutionIdentity::External(profile.executor)
  );
}

#[tokio::test]
async fn file_protected_validation_installs_one_shared_original_input_contract() {
  use octacity_server_application::FactoryProtectedTestPolicy;
  use octacity_server_factory::*;
  let project = octacity_server_domain::ProjectId::generate();
  let mut profile = super::factory_command_runtime_tests::command_profile(project);
  let key = |value: &str| FactoryKey::new(value).unwrap();
  let reference = |value: &str| ImmutableReference::new(key(value), key("v1"), FactoryDigest::from_bytes([8; 32]));
  let report = profile.outputs[0].clone();
  let facts = EvidenceRequirement::new(
    key("qualification-facts"),
    EvidenceOutputKind::Report,
    reference("facts-schema"),
    profile.executor.clone(),
    profile.plugin.clone(),
  );
  profile.outputs.push(facts.clone());
  profile.candidate_capture_tool = Some(reference("source-capture"));
  let mut permissions = serde_json::to_value(&profile.project_permissions).unwrap();
  permissions["outputs"]["kinds"] = serde_json::json!([report.kind(), facts.kind()]);
  permissions["outputs"]["max_report_count"] = serde_json::json!(2);
  profile.project_permissions = serde_json::from_value(permissions).unwrap();
  profile.local_permissions = LocalPermissionCeiling::fully_enforced(profile.project_permissions.clone());
  let policy = FactoryProtectedTestPolicy::new(
    ProtectedTestCoverage::new(vec![key("contract")], 4, 9000, 8).unwrap(),
    900,
  )
  .unwrap();
  let inventory = EvidenceRequirement::new(
    key("private-inventory"),
    EvidenceOutputKind::Artifact,
    reference("inventory-schema"),
    reference("capture-tool"),
    reference("capture-plugin"),
  );
  let document = serde_json::json!({"version":1,"projects":[{"project_id":project,"references":[],"build_configurations":[{"alias":"installed-command-build","reference":profile.build_configuration}]}],"codex":[],"protected_validation":[{"profiles":[command_entry(&profile)],"inputs":{"author_profile":reference("private-author"),"policy":policy,"inventory":inventory,"capture_tool":profile.candidate_capture_tool},"report":report,"facts":facts}]});
  let installer =
    super::factory_profile_file::FileProfileInstaller::parse_json(&serde_json::to_vec(&document).unwrap(), &[])
      .expect("trusted validation preserves its original producer selection");
  let mut resources = super::factory_tests::resources();
  resources.toolchain.octa.plugin_digests.insert(
    profile.plugin.identity().to_string(),
    profile.plugin.digest().to_string(),
  );
  let installed = installer.install(resources).await.unwrap();
  assert_eq!(installed.executors.len(), 1);
  assert_eq!(installed.executors[0].0, FlowNodeKind::BuildCommand);
  assert_eq!(
    installed.executors[0].1,
    NodeExecutionIdentity::External(profile.executor)
  );
}

#[tokio::test]
async fn file_author_policy_installs_its_exact_reasoning_binding_and_rejects_unpinned_changes() {
  let project = octacity_server_domain::ProjectId::generate();
  let profile = super::factory_command_runtime_tests::author_profile(project);
  let policy = octacity_server_application::FactoryProtectedTestAuthoringPolicy::new(
    profile.policy().clone(),
    profile.inventory().clone(),
    profile.files().to_vec(),
  )
  .unwrap();
  let mut document = serde_json::json!({"version":1,"projects":[{"project_id":project,"references":[],"build_configurations":[{"alias":"author-build","reference":profile.command().build_configuration}]}],"codex":[],"protected_test_authors":[{"command":command_entry(profile.command()),"policy":policy}]});
  let installer =
    super::factory_profile_file::FileProfileInstaller::parse_json(&serde_json::to_vec(&document).unwrap(), &[])
      .expect("the exact installed author policy must survive file deployment");
  let mut resources = super::factory_tests::resources();
  resources.toolchain.octa.plugin_digests.insert(
    profile.command().plugin.identity().to_string(),
    profile.command().plugin.digest().to_string(),
  );
  let installed = installer.install(resources).await.unwrap();
  assert!(
    installed.output_finalizer.is_some(),
    "the native author must finalize actual file IDs before Job success"
  );
  assert_eq!(installed.executors.len(), 1);
  assert_eq!(installed.executors[0].0, FlowNodeKind::Reasoning);
  assert_eq!(
    installed.executors[0].1,
    NodeExecutionIdentity::External(profile.command().executor.clone())
  );
  assert!(
    FactoryRuntimeProfileCatalog::parse_json(&serde_json::to_vec(&document).unwrap()).is_err(),
    "metadata-only installation must not discard required author actors"
  );
  document["protected_test_authors"][0]["policy"]["files"][0]["path"] = serde_json::json!("substituted.rs");
  assert!(
    super::factory_profile_file::FileProfileInstaller::parse_json(&serde_json::to_vec(&document).unwrap(), &[])
      .is_err(),
    "file policy changes require an independently repinned command identity"
  );
}

#[tokio::test]
async fn file_deployment_authority_recomputes_the_policy_pin_before_native_installation() {
  let project = octacity_server_domain::ProjectId::generate();
  let deployment = super::factory_command_runtime_tests::deployment_profile(project);
  let command = deployment.command();
  let authority = deployment.authority();
  let document = serde_json::json!({"version":1,
  "projects":[{"project_id":project,"references":[],"build_configurations":[{"alias":"bounded-deployment","reference":command.build_configuration}]}],"codex":[],
  "promotions":[{"identity":command.executor,"profiles":[command_entry(command)],"deployment_authority":{
    "identity":authority.reference().identity(),"version":authority.reference().version(),"environment":authority.environment(),
    "endpoint":authority.endpoint(),"credential_profile":authority.credential_profile()
  }}]});
  let installer =
    super::factory_profile_file::FileProfileInstaller::parse_json(&serde_json::to_vec(&document).unwrap(), &[])
      .expect("the explicit deployment authority must install through the existing promotion entry");
  let mut resources = super::factory_tests::resources();
  resources.toolchain.octa.plugin_digests.insert(
    command.plugin.identity().to_string(),
    command.plugin.digest().to_string(),
  );
  let installed = installer.install(resources).await.unwrap();
  assert_eq!(installed.executors.len(), 1);
  assert_eq!(installed.executors[0].0, FlowNodeKind::TrustedAction);
  assert_eq!(
    installed.executors[0].1,
    NodeExecutionIdentity::External(command.executor.clone())
  );
  assert!(
    FactoryRuntimeProfileCatalog::parse_json(&serde_json::to_vec(&document).unwrap()).is_err(),
    "a metadata catalog must not silently discard a required deployment adapter"
  );
  for (field, value) in [
    ("endpoint", serde_json::json!("http://substituted.example.test/deploy")),
    ("credential_profile", serde_json::json!("other-credential")),
    (
      "environment",
      serde_json::json!({"identity":"other.environment","version":"v1","digest":authority.environment().digest()}),
    ),
  ] {
    let mut substituted = document.clone();
    substituted["promotions"][0]["deployment_authority"][field] = value;
    assert!(
      super::factory_profile_file::FileProfileInstaller::parse_json(&serde_json::to_vec(&substituted).unwrap(), &[])
        .is_err(),
      "changing installed authority must require an explicit command identity/task repin"
    );
  }
}
