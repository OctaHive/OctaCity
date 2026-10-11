//! Shared literal protected Job fixtures for signing and persisted ordinary acceptance tests.
use octacity_protocol::JobSpecV3;
use octacity_server_domain::{BuildId, ImmutableRevision, RepositoryLocator};
use octacity_server_factory::{
  CommandArgumentPattern, CommandPermission, FactoryDigest, FactoryKey, FactoryOutputPermissions, FactoryPath,
  FactoryPermissionDraft, FactoryPermissionSet, FactoryResourceLimits, FactoryText, ImmutableReference, MountMode,
  MountPermission, NetworkHost,
};
use octacity_server_job::{
  JobRuntimePolicy, JobSpecBuildSnapshot, JobSpecPolicySnapshot, JobSpecValidity, SourcePluginPolicy,
};
use std::{collections::BTreeMap, str::FromStr};
const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
pub(crate) fn canonical_v3_spec() -> JobSpecV3 {
  serde_json::from_str(include_str!(
    "../../../shared/protocol-fixtures/job-spec/job-spec-v3.json"
  ))
  .unwrap()
}

pub(crate) fn build(spec: &JobSpecV3) -> JobSpecBuildSnapshot {
  JobSpecBuildSnapshot::new(
    BuildId::from_str("00000000-0000-0000-0000-000000000001").unwrap(),
    ImmutableRevision::new(spec.source.revision.clone()).unwrap(),
    None,
    RepositoryLocator::new("https://example.test/repository.git").unwrap(),
    BTreeMap::new(),
  )
  .unwrap()
}

pub(crate) fn policy(spec: &JobSpecV3) -> JobSpecPolicySnapshot {
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

pub(crate) fn permissions(broad: bool) -> FactoryPermissionSet {
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

pub(crate) fn reference(identity: &str, version: &str) -> ImmutableReference {
  ImmutableReference::new(
    key(identity),
    key(version),
    FactoryDigest::from_lower_hex(DIGEST).unwrap(),
  )
}

pub(crate) fn key(value: &str) -> FactoryKey {
  FactoryKey::new(value).unwrap()
}
