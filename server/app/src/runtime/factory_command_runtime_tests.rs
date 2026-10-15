//! Native command installation uses ordinary Build ports without dependency I/O.
use super::*;
use octacity_server_application::*;
use octacity_server_domain::{BuildConfigurationId, BuildConfigurationVersion, PipelineNodeId, ProjectId};
use octacity_server_factory::*;
use std::collections::BTreeMap;

pub(super) fn command_profile(project: ProjectId) -> FactoryCommandBuildProfile {
  let reference = |name| {
    ImmutableReference::new(
      FactoryKey::new(name).unwrap(),
      FactoryKey::new("v1").unwrap(),
      FactoryDigest::from_bytes([3; 32]),
    )
  };
  let plugin = reference("shell");
  let mut inventory = super::factory_tests::managed::canonical_v3_spec().octa.plugin_digests;
  inventory.insert(plugin.identity().to_string(), plugin.digest().to_string());
  let output = EvidenceRequirement::new(
    FactoryKey::new("receipt").unwrap(),
    EvidenceOutputKind::Report,
    reference("receipt-schema"),
    reference("installed-command"),
    plugin.clone(),
  );
  let permissions = FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins: inventory
      .into_iter()
      .map(|(identity, digest)| {
        ImmutableReference::new(
          FactoryKey::new(identity).unwrap(),
          FactoryKey::new("v1").unwrap(),
          FactoryDigest::from_lower_hex(&digest).unwrap(),
        )
      })
      .collect(),
    max_descendants: 4,
    mounts: vec![
      MountPermission::new(FactoryPath::new("/octacity/protected").unwrap(), MountMode::ReadOnly),
      MountPermission::new(FactoryPath::new("/workspace/source").unwrap(), MountMode::ReadOnly),
      MountPermission::new(FactoryPath::new("/workspace/output").unwrap(), MountMode::ReadWrite),
      MountPermission::new(FactoryPath::new("/workspace/scratch").unwrap(), MountMode::ReadWrite),
    ],
    resources: FactoryResourceLimits::new(1000, 268435456, 16777216, 8, 5000).unwrap(),
    outputs: FactoryOutputPermissions::try_new(
      vec![FactoryKey::new("receipt").unwrap(), FactoryKey::new("facts").unwrap()],
      0,
      0,
      2,
      2048,
    )
    .unwrap(),
    ..Default::default()
  })
  .unwrap();
  FactoryCommandBuildProfile {
    identity: reference("command-policy"),
    plugin,
    executor: reference("installed-command"),
    build_configuration: BuildConfigurationRef::new(
      BuildConfigurationId::generate(),
      BuildConfigurationVersion::INITIAL,
      project,
      FactoryDigest::from_bytes([5; 32]),
    ),
    pipeline_node: PipelineNodeId::new("configured-pipeline-job").unwrap(),
    program: FactoryCommandProgram {
      octafile: FactoryCommandOctafile::new(r#"{"version":1,"tasks":{"observe":{"shell":"true"}}}"#).unwrap(),
      tasks: vec!["observe".into()],
      workspace_root: None,
      credential_profile: None,
    },
    input_destination: "/octacity/protected/command-input.json".into(),
    parameters: BTreeMap::new(),
    project_permissions: permissions.clone(),
    local_permissions: LocalPermissionCeiling::fully_enforced(permissions),
    outputs: vec![output],
    candidate_capture_tool: None,
  }
}

#[tokio::test]
async fn native_command_binding_requires_the_installed_program_plugin_before_dependency_io() {
  let mut resources = super::factory_tests::resources();
  let profile = command_profile(ProjectId::generate());
  resources.toolchain.octa.plugin_digests.insert(
    profile.plugin.identity().as_str().to_owned(),
    profile.plugin.digest().to_string(),
  );
  let installed = resources.command_executor(vec![profile.clone()]).unwrap();
  assert_eq!(installed.0, FlowNodeKind::BuildCommand);
  assert_eq!(installed.1, NodeExecutionIdentity::External(profile.executor.clone()));
  let mut drift = profile;
  drift.plugin = ImmutableReference::new(
    drift.plugin.identity().clone(),
    drift.plugin.version().clone(),
    FactoryDigest::from_bytes([9; 32]),
  );
  assert!(resources.command_executor(vec![drift]).is_err());
}

#[tokio::test]
async fn native_promotion_command_uses_its_exact_trusted_action_identity() {
  let mut resources = super::factory_tests::resources();
  let mut profile = command_profile(ProjectId::generate());
  let ungranted_action = profile.clone();
  profile.executor = profile.plugin.clone();
  profile.outputs = profile
    .outputs
    .iter()
    .map(|output| {
      EvidenceRequirement::new(
        output.kind().clone(),
        output.output_kind(),
        output.schema().clone(),
        profile.executor.clone(),
        profile.plugin.clone(),
      )
    })
    .collect();
  let missing_capture = profile.clone();
  profile.candidate_capture_tool = Some(profile.identity.clone());
  resources.toolchain.octa.plugin_digests.insert(
    profile.plugin.identity().as_str().to_owned(),
    profile.plugin.digest().to_string(),
  );
  let installed = resources
    .promotion_command_executor(profile.executor.clone(), vec![profile.clone()])
    .unwrap();
  assert_eq!(installed.0, FlowNodeKind::TrustedAction);
  assert_eq!(installed.1, NodeExecutionIdentity::External(profile.executor.clone()));
  assert!(
    resources
      .promotion_command_executor(missing_capture.executor.clone(), vec![missing_capture])
      .is_err(),
    "promotion must install an exact retained candidate verifier before advertising its capability"
  );
  assert!(
    resources
      .promotion_command_executor(profile.identity.clone(), vec![profile])
      .is_err()
  );
  assert!(
    resources
      .promotion_command_executor(ungranted_action.executor.clone(), vec![ungranted_action])
      .is_err(),
    "a promotion action needs the same exact granted plugin, without phantom grants"
  );
}

#[tokio::test]
async fn native_validation_command_requires_original_capture_and_distinct_declared_reports() {
  let mut resources = super::factory_tests::resources();
  let mut profile = command_profile(ProjectId::generate());
  let reference = |name| {
    ImmutableReference::new(
      FactoryKey::new(name).unwrap(),
      FactoryKey::new("v1").unwrap(),
      FactoryDigest::from_bytes([41; 32]),
    )
  };
  let capture = reference("original-source-capture");
  profile.candidate_capture_tool = Some(capture.clone());
  let report = profile.outputs[0].clone();
  let facts = EvidenceRequirement::new(
    FactoryKey::new("facts").unwrap(),
    EvidenceOutputKind::Report,
    reference("facts-schema"),
    profile.executor.clone(),
    profile.plugin.clone(),
  );
  profile.outputs.push(facts.clone());
  resources.toolchain.octa.plugin_digests.insert(
    profile.plugin.identity().to_string(),
    profile.plugin.digest().to_string(),
  );
  let inputs = FactoryRuntimeProtectedValidationProfile {
    author_profile: reference("independent-test-author"),
    policy: FactoryProtectedTestPolicy::new(
      ProtectedTestCoverage::new(vec![FactoryKey::new("operator.contract").unwrap()], 4, 9000, 4).unwrap(),
      900,
    )
    .unwrap(),
    inventory: EvidenceRequirement::new(
      FactoryKey::new("original-inventory").unwrap(),
      EvidenceOutputKind::Artifact,
      reference("inventory-schema"),
      reference("inventory-tool"),
      reference("inventory-plugin"),
    ),
    capture_tool: capture,
  };
  let binding = resources
    .protected_validation_command_executor(vec![profile.clone()], inputs.clone(), report.clone(), facts.clone())
    .unwrap();
  assert_eq!(binding.0, FlowNodeKind::BuildCommand);
  assert_eq!(binding.1, NodeExecutionIdentity::External(profile.executor.clone()));
  let mut substituted = inputs.clone();
  substituted.capture_tool = reference("different-source-capture");
  assert!(
    resources
      .protected_validation_command_executor(vec![profile.clone()], substituted, report.clone(), facts.clone())
      .is_err()
  );
  assert!(
    resources
      .protected_validation_command_executor(vec![profile.clone()], inputs.clone(), report.clone(), report.clone())
      .is_err()
  );
  profile.outputs.pop();
  assert!(
    resources
      .protected_validation_command_executor(vec![profile], inputs, report, facts)
      .is_err()
  );
}

pub(super) fn author_profile(project: ProjectId) -> FactoryProtectedTestAuthoringProfile {
  let mut command = command_profile(project);
  let key = |value| FactoryKey::new(value).unwrap();
  let inventory = EvidenceRequirement::new(
    key("private.inventory"),
    EvidenceOutputKind::Artifact,
    command.outputs[0].schema().clone(),
    command.executor.clone(),
    command.plugin.clone(),
  );
  command.outputs.push(inventory.clone());
  let mut grants = serde_json::to_value(&command.project_permissions).unwrap();
  grants["outputs"]["kinds"] = serde_json::json!(["receipt", "private.inventory", "hidden.contract"]);
  grants["outputs"]["max_artifact_count"] = serde_json::json!(2);
  grants["outputs"]["max_artifact_bytes"] = serde_json::json!(1024);
  grants["outputs"]["max_report_count"] = serde_json::json!(1);
  command.project_permissions = serde_json::from_value(grants).unwrap();
  command.local_permissions = LocalPermissionCeiling::fully_enforced(command.project_permissions.clone());
  command.candidate_capture_tool = Some(command.identity.clone());
  let policy = FactoryProtectedTestAuthoringPolicy::new(
    FactoryProtectedTestPolicy::new(
      ProtectedTestCoverage::new(vec![key("operator.contract")], 1, 1024, 4).unwrap(),
      1000,
    )
    .unwrap(),
    inventory,
    vec![FactoryProtectedTestFileOutput {
      output: key("hidden.contract"),
      class: key("operator.contract"),
      path: FactoryRepositoryPath::new("contract.rs").unwrap(),
    }],
  )
  .unwrap();
  command.identity = ImmutableReference::new(
    command.identity.identity().clone(),
    command.identity.version().clone(),
    policy.digest().unwrap(),
  );
  FactoryProtectedTestAuthoringProfile::new(command, policy).unwrap()
}

#[tokio::test]
async fn native_author_command_installs_the_selected_readonly_reasoning_capability_without_io() {
  let mut resources = super::factory_tests::resources();
  let profile = author_profile(ProjectId::generate());
  resources.toolchain.octa.plugin_digests.insert(
    profile.command().plugin.identity().to_string(),
    profile.command().plugin.digest().to_string(),
  );
  let binding = resources
    .protected_test_authoring_command_executor(vec![profile.clone()])
    .unwrap();
  assert_eq!(binding.0, FlowNodeKind::Reasoning);
  assert_eq!(
    binding.1,
    NodeExecutionIdentity::External(profile.command().executor.clone())
  );
  assert!(
    resources
      .protected_test_authoring_command_executor(vec![profile.clone(), profile])
      .is_err()
  );
}

pub(super) fn deployment_profile(project: ProjectId) -> FactoryDeploymentCommandProfile {
  let mut command = command_profile(project);
  command.executor = command.plugin.clone();
  command.candidate_capture_tool = Some(command.identity.clone());
  command.outputs = command
    .outputs
    .iter()
    .map(|output| {
      EvidenceRequirement::new(
        output.kind().clone(),
        output.output_kind(),
        output.schema().clone(),
        command.executor.clone(),
        command.plugin.clone(),
      )
    })
    .collect();
  let authority = FactoryDeploymentCommandAuthority::new(
    FactoryKey::new("bounded-target-policy").unwrap(),
    FactoryKey::new("v1").unwrap(),
    command.identity.clone(),
    "http://staging.example.test/deploy".into(),
    Some(FactoryKey::new("deployment-credential").unwrap()),
  )
  .unwrap();
  command.identity = authority.reference().clone();
  command.program.credential_profile = authority.credential_profile().cloned();
  let mut grants = serde_json::to_value(&command.project_permissions).unwrap();
  grants["network_hosts"] = serde_json::json!(["staging.example.test"]);
  grants["secret_profiles"] = serde_json::json!(["deployment-credential"]);
  command.project_permissions = serde_json::from_value(grants).unwrap();
  command.local_permissions = LocalPermissionCeiling::fully_enforced(command.project_permissions.clone());
  FactoryDeploymentCommandProfile::new(command, authority).unwrap()
}

#[tokio::test]
async fn native_deployment_command_installs_only_its_exact_action_capture_and_approved_inventory() {
  let mut resources = super::factory_tests::resources();
  let deployment = deployment_profile(ProjectId::generate());
  let command = deployment.command();
  resources.toolchain.octa.plugin_digests.insert(
    command.plugin.identity().to_string(),
    command.plugin.digest().to_string(),
  );
  let installed = resources
    .deployment_command_executor(command.executor.clone(), vec![deployment.clone()])
    .unwrap();
  assert_eq!(installed.0, FlowNodeKind::TrustedAction);
  assert_eq!(installed.1, NodeExecutionIdentity::External(command.executor.clone()));
  assert!(
    resources
      .promotion_command_executor(command.executor.clone(), vec![command.clone()])
      .is_err(),
    "network credentials are not hermetic ordinary promotion authority"
  );
  assert!(
    resources
      .deployment_command_executor(command.identity.clone(), vec![deployment.clone()])
      .is_err()
  );
  let mut missing_capture = command.clone();
  missing_capture.candidate_capture_tool = None;
  let missing_capture = FactoryDeploymentCommandProfile::new(missing_capture, deployment.authority().clone()).unwrap();
  assert!(
    resources
      .deployment_command_executor(command.executor.clone(), vec![missing_capture])
      .is_err()
  );
  resources.toolchain.octa.plugin_digests.insert(
    "unapproved-extra-plugin".into(),
    FactoryDigest::from_bytes([8; 32]).to_string(),
  );
  assert!(
    resources
      .deployment_command_executor(command.executor.clone(), vec![deployment.clone()])
      .is_err(),
    "the operator's approved inventory cannot widen without a command permission and task repin"
  );
}
