//! Operator-owned file profiles reach the same qualified native installers.
use crate::runtime::*;
use octacity_server_factory::{FactoryKey, FlowNodeKind, ImmutableReference, NodeExecutionIdentity};
use std::sync::Arc;

struct Ready;

#[test]
fn installed_choice_permissions_remain_available_to_ordinary_console_authoring() {
  use octacity_server_factory::{FactoryChoiceKind, FactoryPermissionSet};
  let project = octacity_server_domain::ProjectId::generate();
  let identity = crate::runtime::factory::factory_tests::managed::reference("reviewed-ceiling", "v1");
  let mut permissions = serde_json::to_value(FactoryPermissionSet::deny_all()).unwrap();
  permissions["network_hosts"] = serde_json::json!(["api.allowed.example"]);
  let document = serde_json::json!({"version":1,"codex":[],"projects":[{
    "project_id":project,"build_configurations":[],"references":[{
      "kind":"permission_ceiling","alias":"reviewed-ceiling","reference":identity,
      "metadata":{"node_kinds":[],"signal_purposes":[],"enforcement":[],"compatible_choices":[],
        "permission_declarations":permissions}
    }]
  }]});
  let catalog = FactoryRuntimeProfileCatalog::parse_json(&serde_json::to_vec(&document).unwrap())
    .expect("trusted installation retains the existing typed choice metadata");
  let metadata = catalog.projects[0]
    .1
    .choice_metadata(
      FactoryChoiceKind::PermissionCeiling,
      &FactoryKey::new("reviewed-ceiling").unwrap(),
    )
    .unwrap();
  assert_eq!(
    serde_json::to_value(metadata).unwrap()["permission_declarations"]["network_hosts"],
    serde_json::json!(["api.allowed.example"])
  );
}

#[async_trait::async_trait]
impl FactoryRuntimeReadiness for Ready {
  async fn available(&self) -> bool {
    true
  }
}

#[tokio::test]
async fn file_issue_status_action_installs_only_its_explicit_identity_and_authority() {
  let identity = crate::runtime::factory::factory_tests::managed::reference("reviewed-issue-status", "v1");
  let project = octacity_server_domain::ProjectId::generate();
  let mut document = serde_json::json!({
    "version":1,"projects":[{"project_id":project,"references":[],"build_configurations":[]}],"codex":[],
    "issue_status_actions":[{"identity":identity,"principal":"reviewed-operator"}]
  });
  let installer = crate::runtime::factory::factory_profile_file::FileProfileInstaller::parse_json(
    &serde_json::to_vec(&document).unwrap(),
    &[],
  )
  .expect("an operator can install the ordinary Issue action without a custom Server entry point");
  let installed = installer
    .install(crate::runtime::factory::factory_tests::resources())
    .await
    .unwrap();
  assert_eq!(installed.executors.len(), 1);
  assert_eq!(installed.executors[0].0, FlowNodeKind::TrustedAction);
  assert_eq!(installed.executors[0].1, NodeExecutionIdentity::External(identity));
  assert!(
    FactoryRuntimeProfileCatalog::parse_json(&serde_json::to_vec(&document).unwrap()).is_err(),
    "metadata-only parsing cannot silently discard an action"
  );
  document["issue_status_actions"][0]["principal"] = serde_json::json!("");
  assert!(
    crate::runtime::factory::factory_profile_file::FileProfileInstaller::parse_json(
      &serde_json::to_vec(&document).unwrap(),
      &[],
    )
    .is_err(),
    "an action cannot acquire an anonymous installation owner"
  );
}

#[tokio::test]
async fn file_issue_classification_installs_an_explicit_local_writer() {
  let identity = crate::runtime::factory::factory_tests::managed::reference("reviewed-classification", "v1");
  let project = octacity_server_domain::ProjectId::generate();
  let mut document = serde_json::json!({
    "version":1,"projects":[{"project_id":project,"references":[],"build_configurations":[]}],"codex":[],
    "issue_classification_actions":[{"identity":identity,"principal":"reviewed-operator"}]
  });
  let parse = |document: &serde_json::Value| {
    crate::runtime::factory::factory_profile_file::FileProfileInstaller::parse_json(
      &serde_json::to_vec(document).unwrap(),
      &[],
    )
  };
  assert!(
    parse(&document)
      .unwrap()
      .install(crate::runtime::factory::factory_tests::resources())
      .await
      .is_err(),
    "a writer principal in the Factory catalog cannot grant itself management access"
  );
  let installed = parse(&document)
    .unwrap()
    .with_management_principal(Some("reviewed-operator"))
    .install(crate::runtime::factory::factory_tests::resources())
    .await
    .unwrap();
  assert_eq!(installed.executors.len(), 1);
  assert_eq!(installed.executors[0].0, FlowNodeKind::TrustedAction);
  assert_eq!(
    installed.executors[0].1,
    NodeExecutionIdentity::External(identity.clone())
  );
  document["projects"][0]["references"] = serde_json::json!([{
    "kind":"node_capability","alias":"local-classification","reference":identity,
    "metadata":{
      "node_kinds":["trusted_action"],"signal_purposes":[],"enforcement":[],"compatible_choices":[],
      "issue_classification":{"policy":"triage","size_labels":{"small":"size_small"}}
    }
  }]);
  assert!(parse(&document).is_ok());
  let writer = document["issue_classification_actions"].take();
  document.as_object_mut().unwrap().remove("issue_classification_actions");
  assert!(
    parse(&document).is_err(),
    "a Project mapping must refer to an installed local classification writer"
  );
  document["issue_classification_actions"] = writer;
  document["issue_status_actions"] = document["issue_classification_actions"].clone();
  assert!(
    parse(&document).is_err(),
    "one exact executor identity cannot name two writers"
  );
}

#[tokio::test]
async fn file_review_planning_profile_installs_its_exact_native_executor_without_io() {
  let project = octacity_server_domain::ProjectId::generate();
  let profile = crate::runtime::factory::factory_tests::review_planning_profile(project);
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
    .install(crate::runtime::factory::factory_tests::resources())
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
  let installer = crate::runtime::factory::factory_profile_file::FileProfileInstaller::parse_json(
    &serde_json::to_vec(&document).unwrap(),
    &[],
  )
  .expect("file deployment retains the concrete original-input verifier until resources are supplied");
  let installed = installer
    .install(crate::runtime::factory::factory_tests::resources())
    .await
    .unwrap();
  assert_eq!(installed.executors.len(), 1);
  assert_eq!(installed.executors[0].0, FlowNodeKind::TrustedAction);
  assert_eq!(installed.executors[0].1, NodeExecutionIdentity::External(identity));
}

#[tokio::test]
async fn file_command_profile_installs_its_exact_native_executor_without_io() {
  let project = octacity_server_domain::ProjectId::generate();
  let profile = crate::runtime::factory::factory_command_runtime_tests::command_profile(project);
  let entry = command_entry(&profile);
  let document = serde_json::json!({"version":1,"projects":[{"project_id":project,"references":[],"build_configurations":[{"alias":"installed-command-build","reference":profile.build_configuration}]}],"codex":[],"commands":[entry]});
  let installer = crate::runtime::factory::factory_profile_file::FileProfileInstaller::parse_json(
    &serde_json::to_vec(&document).unwrap(),
    &[],
  )
  .expect("operator command survives native file deployment");
  let mut resources = crate::runtime::factory::factory_tests::resources();
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

pub(in crate::runtime) fn command_entry(
  profile: &octacity_server_application::FactoryCommandBuildProfile,
) -> serde_json::Value {
  use octacity_server_factory::PermissionCategory;
  serde_json::json!({
    "identity":profile.identity,"plugin":profile.plugin,"executor":profile.executor,
    "build_configuration":profile.build_configuration,"pipeline_node":profile.pipeline_node,
    "program":profile.program,"input_destination":profile.input_destination,
    "parameters":profile.parameters,"project_permissions":profile.project_permissions,
    "local_permissions":{"grants":profile.local_permissions.grants(),"enforceable":PermissionCategory::ALL},
    "outputs":profile.outputs,"candidate_capture_tool":profile.candidate_capture_tool,
    "observation_contract":profile.observation_contract,
  })
}

#[test]
fn file_command_refuses_a_build_configuration_outside_its_project_choices() {
  let project = octacity_server_domain::ProjectId::generate();
  let profile = crate::runtime::factory::factory_command_runtime_tests::command_profile(project);
  let document = serde_json::json!({"version":1,"projects":[{"project_id":project,"references":[],"build_configurations":[]}],"codex":[],"commands":[command_entry(&profile)]});
  assert!(
    crate::runtime::factory::factory_profile_file::FileProfileInstaller::parse_json(
      &serde_json::to_vec(&document).unwrap(),
      &[]
    )
    .is_err(),
    "a command cannot introduce an undeclared Build Configuration into a Project installation"
  );
}

#[tokio::test]
async fn file_promotion_profile_installs_the_same_exact_approved_command_plugin() {
  use octacity_server_factory::*;
  let project = octacity_server_domain::ProjectId::generate();
  let mut profile = crate::runtime::factory::factory_command_runtime_tests::command_profile(project);
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
  let installer = crate::runtime::factory::factory_profile_file::FileProfileInstaller::parse_json(
    &serde_json::to_vec(&document).unwrap(),
    &[],
  )
  .expect("promotion static command is installed explicitly");
  let mut resources = crate::runtime::factory::factory_tests::resources();
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
  let mut profile = crate::runtime::factory::factory_command_runtime_tests::command_profile(project);
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
  let installer = crate::runtime::factory::factory_profile_file::FileProfileInstaller::parse_json(
    &serde_json::to_vec(&document).unwrap(),
    &[],
  )
  .expect("trusted validation preserves its original producer selection");
  let mut resources = crate::runtime::factory::factory_tests::resources();
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
  let profile = crate::runtime::factory::factory_command_runtime_tests::author_profile(project);
  let policy = octacity_server_application::FactoryProtectedTestAuthoringPolicy::new(
    profile.policy().clone(),
    profile.inventory().clone(),
    profile.files().to_vec(),
  )
  .unwrap();
  let mut document = serde_json::json!({"version":1,"projects":[{"project_id":project,"references":[],"build_configurations":[{"alias":"author-build","reference":profile.command().build_configuration}]}],"codex":[],"protected_test_authors":[{"command":command_entry(profile.command()),"policy":policy}]});
  let installer = crate::runtime::factory::factory_profile_file::FileProfileInstaller::parse_json(
    &serde_json::to_vec(&document).unwrap(),
    &[],
  )
  .expect("the exact installed author policy must survive file deployment");
  let mut resources = crate::runtime::factory::factory_tests::resources();
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
    crate::runtime::factory::factory_profile_file::FileProfileInstaller::parse_json(
      &serde_json::to_vec(&document).unwrap(),
      &[]
    )
    .is_err(),
    "file policy changes require an independently repinned command identity"
  );
}

#[tokio::test]
async fn file_deployment_authority_recomputes_the_policy_pin_before_native_installation() {
  let project = octacity_server_domain::ProjectId::generate();
  let deployment = crate::runtime::factory::factory_command_runtime_tests::deployment_profile(project);
  let command = deployment.command();
  let authority = deployment.authority();
  let document = serde_json::json!({"version":1,
  "projects":[{"project_id":project,"references":[],"build_configurations":[{"alias":"bounded-deployment","reference":command.build_configuration}]}],"codex":[],
  "promotions":[{"identity":command.executor,"profiles":[command_entry(command)],"deployment_authority":{
    "identity":authority.reference().identity(),"version":authority.reference().version(),"environment":authority.environment(),
    "endpoint":authority.endpoint(),"credential_profile":authority.credential_profile()
  }}]});
  let installer = crate::runtime::factory::factory_profile_file::FileProfileInstaller::parse_json(
    &serde_json::to_vec(&document).unwrap(),
    &[],
  )
  .expect("the explicit deployment authority must install through the existing promotion entry");
  let mut resources = crate::runtime::factory::factory_tests::resources();
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
      crate::runtime::factory::factory_profile_file::FileProfileInstaller::parse_json(
        &serde_json::to_vec(&substituted).unwrap(),
        &[]
      )
      .is_err(),
      "changing installed authority must require an explicit command identity/task repin"
    );
  }
}

#[test]
fn file_role_profile_declarations_extend_existing_project_choices_without_installing_an_executor() {
  use octacity_server_factory::*;
  let project = octacity_server_domain::ProjectId::generate();
  let definition = octacity_server_store::testing::role_profile_contract_definition(project);
  let alias = FactoryKey::new("runtime-profile").unwrap();
  let parameters = FlowDataSchema::new(
    FactoryKey::new("parameters").unwrap(),
    FactoryKey::new("v1").unwrap(),
    FlowValueSchema::Object {
      fields: Default::default(),
    },
  )
  .unwrap();
  let document = serde_json::json!({"version":1,"codex":[],"projects":[{
    "project_id":project,
    "references":[{"kind":"node_capability","alias":alias,"reference":definition.toolchain.executable}],
    "build_configurations":[{"alias":"build","reference":definition.build_configuration}],
    "role_profiles":{"runtime-profile":{"ceiling":definition,"parameters":parameters,"public_environment":{},"enforcement":PermissionCategory::ALL}}
  }]});
  let catalog = FactoryRuntimeProfileCatalog::parse_json(&serde_json::to_vec(&document).unwrap()).unwrap();
  assert_eq!(catalog.projects[0].1.role_profile_capabilities().len(), 1);
  assert_eq!(
    catalog.projects[0].1.role_profile_capabilities()[&alias].ceiling,
    definition
  );
  assert!(catalog.codex.is_empty());
  assert!(catalog.executors.is_empty());
  let mut foreign = document.clone();
  foreign["projects"][0]["role_profiles"]["runtime-profile"]["ceiling"]["build_configuration"]["project_id"] =
    serde_json::to_value(octacity_server_domain::ProjectId::generate()).unwrap();
  assert!(FactoryRuntimeProfileCatalog::parse_json(&serde_json::to_vec(&foreign).unwrap()).is_err());
}

#[tokio::test]
async fn file_installs_a_protected_observation_on_the_work_base() {
  use octacity_server_factory::{EvidenceOutputKind, EvidenceRequirement, FlowDataSchema, FlowValueSchema};
  let reference = |name| crate::runtime::factory::factory_tests::managed::reference(name, "v1");
  let project = octacity_server_domain::ProjectId::generate();
  let mut profile = crate::runtime::factory::factory_command_runtime_tests::command_profile(project);
  let author = crate::runtime::factory::factory_command_runtime_tests::author_profile(project);
  let observation = octacity_server_application::FactoryCommandObservationContract {
    environment: reference("installed-baseline-environment"),
    criterion: reference("expected-baseline-failure"),
    observations: FlowDataSchema::new(
      FactoryKey::new("baseline-observations").unwrap(),
      FactoryKey::new("v1").unwrap(),
      FlowValueSchema::Boolean,
    )
    .unwrap(),
    completion: serde_json::from_value(serde_json::json!({"operation":"exists","operand":{"source":"path","path":""}}))
      .unwrap(),
    symptom: serde_json::from_value(serde_json::json!({
      "operation":"compare","left":{"source":"path","path":""},"comparison":"equal",
      "right":{"source":"literal","value":true}
    }))
    .unwrap(),
    requires_protected_tests: true,
    stop_on_indeterminate: false,
  };
  let schema = observation.report_schema().unwrap();
  let report = EvidenceRequirement::new(
    profile.outputs[0].kind().clone(),
    EvidenceOutputKind::Report,
    schema.reference().clone(),
    profile.executor.clone(),
    profile.plugin.clone(),
  );
  let ordinary = command_entry(&profile);
  profile.outputs = vec![report.clone()];
  profile.observation_contract = Some(observation);
  let mut document = serde_json::json!({
    "version":1,"projects":[{"project_id":project,"references":[],
      "build_configurations":[{"alias":"baseline-build","reference":profile.build_configuration}]}],
    "codex":[],"commands":[ordinary],"protected_validation":[{"profiles":[command_entry(&profile)],
      "inputs":{"author_profile":author.command().identity,"policy":author.policy(),"inventory":author.inventory()},
      "report":report}]
  });
  let parse = |value: &serde_json::Value| {
    crate::runtime::factory::factory_profile_file::FileProfileInstaller::parse_json(
      &serde_json::to_vec(value).unwrap(),
      &[],
    )
  };
  assert!(parse(&document).is_err(), "baseline needs its installed author");
  document["projects"][0]["build_configurations"]
    .as_array_mut()
    .unwrap()
    .push(serde_json::json!({
      "alias":"author-build","reference":author.command().build_configuration,
    }));
  let mut author_policy = serde_json::to_value(
    octacity_server_application::FactoryProtectedTestAuthoringPolicy::new(
      author.policy().clone(),
      author.inventory().clone(),
      author.files().to_vec(),
    )
    .unwrap(),
  )
  .unwrap();
  document["protected_test_authors"] = serde_json::json!([{
    "command":command_entry(author.command()),"policy":author_policy,
  }]);
  assert!(
    parse(&document).is_err(),
    "the baseline's exact author must retain a private transcript"
  );
  author_policy["transcript"] = serde_json::json!("private.transcript");
  let policy: octacity_server_application::FactoryProtectedTestAuthoringPolicy =
    serde_json::from_value(author_policy.clone()).unwrap();
  let mut author_command = author.command().clone();
  author_command.identity = ImmutableReference::new(
    author_command.identity.identity().clone(),
    author_command.identity.version().clone(),
    policy.digest().unwrap(),
  );
  let mut grants = serde_json::to_value(&author_command.project_permissions).unwrap();
  grants["outputs"]["kinds"]
    .as_array_mut()
    .unwrap()
    .push(serde_json::json!("private.transcript"));
  grants["outputs"]["max_artifact_count"] = serde_json::json!(3);
  author_command.project_permissions = serde_json::from_value(grants).unwrap();
  author_command.local_permissions =
    octacity_server_factory::LocalPermissionCeiling::fully_enforced(author_command.project_permissions.clone());
  document["protected_test_authors"][0] = serde_json::json!({
    "command":command_entry(&author_command),"policy":author_policy,
  });
  document["protected_validation"][0]["inputs"]["author_profile"] = serde_json::json!(author_command.identity);
  let installer = parse(&document).expect("an exact protected observation is installable without a candidate");
  let mut resources = crate::runtime::factory::factory_tests::resources();
  resources.toolchain.octa.plugin_digests.insert(
    profile.plugin.identity().to_string(),
    profile.plugin.digest().to_string(),
  );
  let installed = installer.install(resources).await.unwrap();
  let commands = installed
    .executors
    .iter()
    .filter(|binding| binding.0 == FlowNodeKind::BuildCommand)
    .collect::<Vec<_>>();
  assert_eq!(commands.len(), 1, "one executor must serve both exact command profiles");
  assert_eq!(commands[0].1, NodeExecutionIdentity::External(profile.executor));
  let mut tester = crate::runtime::factory::factory_tests::qualified_codex_profile();
  tester.build_configuration = author_command.build_configuration.clone();
  let mut grants = serde_json::to_value(&tester.project_permissions).unwrap();
  grants["outputs"]["kinds"] = serde_json::json!([
    "codex-run-result",
    "codex-run-provenance",
    "codex-run-trace",
    "hidden.contract",
    "private.inventory",
    "protected_handoff"
  ]);
  grants["outputs"]["max_artifact_count"] = serde_json::json!(4);
  grants["outputs"]["max_report_count"] = serde_json::json!(2);
  tester.project_permissions = serde_json::from_value(grants).unwrap();
  tester.local_permissions =
    octacity_server_factory::LocalPermissionCeiling::fully_enforced(tester.project_permissions.clone());
  let mut codex = crate::runtime::factory::factory_tests::codex_catalog_document(&tester)["codex"].clone();
  author_policy["inventory"]["tool"] = serde_json::json!(tester.toolchain.executable);
  author_policy["inventory"]["plugin"] = serde_json::json!(tester.toolchain.plugin);
  author_policy["transcript"] = serde_json::json!("codex-run-trace");
  codex[0]["protected_tests"] = author_policy.clone();
  let mut generated = document.clone();
  generated["protected_test_authors"] = serde_json::json!([]);
  generated["codex"] = codex;
  generated["protected_validation"][0]["inputs"]["author_profile"] = serde_json::json!(tester.toolchain.model);
  generated["protected_validation"][0]["inputs"]["inventory"] = author_policy["inventory"].clone();
  let installer = parse(&generated).expect("a native Tester can author the exact inputs of the baseline");
  let mut resources = crate::runtime::factory::factory_tests::resources();
  for pin in [&profile.plugin, &tester.toolchain.plugin] {
    resources
      .toolchain
      .octa
      .plugin_digests
      .insert(pin.identity().to_string(), pin.digest().to_string());
  }
  assert!(installer.install(resources).await.unwrap().output_finalizer.is_some());
  generated["codex"][0]["protected_tests"]["transcript"] = serde_json::Value::Null;
  assert!(
    parse(&generated).is_err(),
    "the baseline cannot select a Tester without its private transcript"
  );
  document["protected_validation"][0]["profiles"][0]
    .as_object_mut()
    .unwrap()
    .remove("observation_contract");
  assert!(
    parse(&document).is_err(),
    "an unqualified command must not obtain protected files"
  );
}
