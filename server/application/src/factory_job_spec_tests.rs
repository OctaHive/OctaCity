use std::{collections::BTreeMap, str::FromStr};

use octacity_protocol::{
  FactoryEnforcementCapabilityV3, JobBinding, JobSpecV3, VerifiedJobSpec, verify_compatible_job_spec,
};
use octacity_server_domain::{
  AttemptNumber, BuildId, ImmutableRevision, JobId, PipelineNodeId, RepositoryLocator, Timestamp,
};
use octacity_server_factory::{
  CommandArgumentPattern, CommandPermission, FactoryDigest, FactoryKey, FactoryOutputPermissions, FactoryPath,
  FactoryPermissionDraft, FactoryPermissionSet, FactoryResourceLimits, FactoryText, ImmutableReference,
  LocalPermissionCeiling, MountMode, MountPermission, NetworkHost, PermissionCategory,
};
use octacity_server_job::{
  JobRuntimePolicy, JobSpecBuildSnapshot, JobSpecPolicySnapshot, JobSpecSigner, JobSpecValidity, SourcePluginPolicy,
  sign_ready_job_spec,
};

use crate::{
  FactoryBuildPolicyLayers, FactoryManagedJobSpecInput, FactoryManagedJobSpecTemplateError,
  derive_factory_managed_job_spec_template,
};

const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[test]
fn factory_inputs_can_only_narrow_the_project_policy_before_v3_signing() {
  let canonical = canonical_v3_spec();
  let narrow = permissions(false);
  let broad = permissions(true);
  let template = derive_factory_managed_job_spec_template(FactoryManagedJobSpecInput {
    build: build(&canonical),
    pipeline_node_id: PipelineNodeId::new("factory-implementation").unwrap(),
    execution: canonical.execution.clone(),
    causality: canonical.factory.clone().unwrap(),
    protected_inputs: canonical.protected_inputs.clone(),
    policy_layers: FactoryBuildPolicyLayers {
      project: narrow.clone(),
      configuration: broad.clone(),
      task: broad.clone(),
      local: LocalPermissionCeiling::fully_enforced(broad),
    },
    required_enforcement: FactoryEnforcementCapabilityV3::ALL.to_vec(),
    policy: policy(&canonical),
  })
  .unwrap();
  let signer = JobSpecSigner::new("factory-key", [9; 32]).unwrap();
  let job_id = JobId::from_str("00000000-0000-0000-0000-000000000099").unwrap();
  let signed = sign_ready_job_spec(
    &template,
    AttemptNumber::FIRST,
    job_id,
    Timestamp::from_unix_millis(100_000).unwrap(),
    &signer,
  )
  .unwrap();
  let verified = verify_compatible_job_spec(
    signed.envelope(),
    &BTreeMap::from([(signer.key_id().to_owned(), signer.verifying_key())]),
    JobBinding {
      job_id: &job_id.to_string(),
      attempt: 1,
      now: 100,
    },
  )
  .unwrap();
  let VerifiedJobSpec::V3(spec) = verified else {
    panic!("Factory managed intent must sign as JobSpec v3");
  };
  assert_eq!(spec.permissions.plugins.len(), 1);
  assert_eq!(spec.permissions.plugins[0].identity, "codex");
  assert_eq!(spec.permissions.network_hosts, ["api.openai.com"]);
  assert_eq!(spec.permissions.outputs.kinds, ["codex-result"]);
  let payload = String::from_utf8(
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &signed.envelope().payload).unwrap(),
  )
  .unwrap();
  for forbidden in ["evil.example", "shell", "transfer_url", "download_url", "bearer"] {
    assert!(!payload.contains(forbidden), "signed payload retained {forbidden}");
  }
}

#[test]
fn incomplete_local_enforcement_fails_before_a_v3_template_exists() {
  let canonical = canonical_v3_spec();
  let narrow = permissions(false);
  let error = derive_factory_managed_job_spec_template(FactoryManagedJobSpecInput {
    build: build(&canonical),
    pipeline_node_id: PipelineNodeId::new("factory-implementation").unwrap(),
    execution: canonical.execution.clone(),
    causality: canonical.factory.clone().unwrap(),
    protected_inputs: canonical.protected_inputs.clone(),
    policy_layers: FactoryBuildPolicyLayers {
      project: narrow.clone(),
      configuration: narrow.clone(),
      task: narrow.clone(),
      local: LocalPermissionCeiling::try_new(
        narrow,
        PermissionCategory::ALL
          .into_iter()
          .filter(|category| *category != PermissionCategory::NetworkHosts)
          .collect(),
      )
      .unwrap(),
    },
    required_enforcement: FactoryEnforcementCapabilityV3::ALL.to_vec(),
    policy: policy(&canonical),
  })
  .unwrap_err();

  assert!(matches!(error, FactoryManagedJobSpecTemplateError::Permissions));
}

fn canonical_v3_spec() -> JobSpecV3 {
  serde_json::from_str(include_str!(
    "../../../shared/protocol-fixtures/job-spec/job-spec-v3.json"
  ))
  .unwrap()
}

fn build(spec: &JobSpecV3) -> JobSpecBuildSnapshot {
  JobSpecBuildSnapshot::new(
    BuildId::from_str("00000000-0000-0000-0000-000000000001").unwrap(),
    ImmutableRevision::new(spec.source.revision.clone()).unwrap(),
    None,
    RepositoryLocator::new("https://example.test/repository.git").unwrap(),
    BTreeMap::new(),
  )
  .unwrap()
}

fn policy(spec: &JobSpecV3) -> JobSpecPolicySnapshot {
  JobSpecPolicySnapshot::new(
    SourcePluginPolicy::new(
      spec.source.provider.clone(),
      spec.source.plugin_version.clone(),
      spec.source.plugin_sha256.clone(),
      "url",
    )
    .unwrap(),
    spec.octa.clone(),
    JobRuntimePolicy::Current(spec.runtime.clone()),
    None,
    spec.cache.clone(),
    spec.outputs.clone(),
    JobSpecValidity::new(spec.expires_at - spec.issued_at).unwrap(),
  )
  .unwrap()
}

fn permissions(broad: bool) -> FactoryPermissionSet {
  let codex = reference("codex", "0.5.0");
  let executable = reference("codex-cli", "0.116.0");
  let mut plugins = vec![codex];
  let mut executables = vec![executable.clone()];
  let mut commands = vec![
    CommandPermission::try_new(
      executable,
      vec![CommandArgumentPattern::Exact(FactoryText::new("--json").unwrap())],
    )
    .unwrap(),
  ];
  let mut mounts = vec![
    MountPermission::new(FactoryPath::new("/octacity/protected").unwrap(), MountMode::ReadOnly),
    MountPermission::new(FactoryPath::new("/workspace/output").unwrap(), MountMode::ReadWrite),
    MountPermission::new(FactoryPath::new("/workspace/scratch").unwrap(), MountMode::ReadWrite),
    MountPermission::new(FactoryPath::new("/workspace/source").unwrap(), MountMode::ReadWrite),
  ];
  let mut network_hosts = vec![NetworkHost::new("api.openai.com").unwrap()];
  let mut secret_profiles = vec![key("model-coding")];
  let mut output_kinds = vec![key("codex-result")];
  if broad {
    let shell = reference("shell", "1.0.0");
    let shell_executable = reference("shell-cli", "1.0.0");
    plugins.push(shell);
    executables.push(shell_executable.clone());
    commands.push(CommandPermission::try_new(shell_executable, Vec::new()).unwrap());
    mounts.push(MountPermission::new(
      FactoryPath::new("/workspace/extra").unwrap(),
      MountMode::ReadWrite,
    ));
    network_hosts.push(NetworkHost::new("evil.example").unwrap());
    secret_profiles.push(key("extra-secret"));
    output_kinds.push(key("extra-output"));
  }
  FactoryPermissionSet::try_new(FactoryPermissionDraft {
    plugins,
    executables,
    commands,
    max_descendants: if broad { 8 } else { 4 },
    mounts,
    network_hosts,
    secret_profiles,
    resources: FactoryResourceLimits::new(
      if broad { 2_000 } else { 1_000 },
      if broad { 1_073_741_824 } else { 536_870_912 },
      if broad { 2_147_483_648 } else { 1_073_741_824 },
      if broad { 16 } else { 8 },
      if broad { 1_200_000 } else { 600_000 },
    )
    .unwrap(),
    outputs: FactoryOutputPermissions::try_new(
      output_kinds,
      if broad { 20 } else { 10 },
      if broad { 2_048 } else { 1_024 },
      if broad { 20 } else { 10 },
      if broad { 2_048 } else { 1_024 },
    )
    .unwrap(),
    ..FactoryPermissionDraft::default()
  })
  .unwrap()
}

fn reference(identity: &str, version: &str) -> ImmutableReference {
  ImmutableReference::new(
    key(identity),
    key(version),
    FactoryDigest::from_lower_hex(DIGEST).unwrap(),
  )
}

fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
