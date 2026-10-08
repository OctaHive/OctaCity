//! Fail-closed admission for protected Factory execution intent.

use std::collections::{BTreeMap, BTreeSet};

use octacity_protocol::{
  FactoryCommandArgumentV3, FactoryCommandPermissionV3, FactoryEnforcementCapabilityV3, FactoryImmutableReferenceV3,
  FactoryMountModeV3, FactoryMountPermissionV3, FactoryPermissionSetV3, ManagedOctaExecutionV3, NetworkPolicy,
  OutputLimits, PROTECTED_INPUT_ROOT, ProtectedInputManifestV3, ProtectedInputTransferV3, RuntimeSpecV2,
};
use octacity_runner::{RunnerInstallation, VerifiedExternalExecutable};
use thiserror::Error;

/// Signed v3-only execution data retained until a qualified backend can start it.
#[derive(Clone, Debug)]
pub(super) struct ManagedFactoryExecution {
  pub(super) execution: ManagedOctaExecutionV3,
  pub(super) protected_inputs: ProtectedInputManifestV3,
  pub(super) permissions: FactoryPermissionSetV3,
  pub(super) required_enforcement: Vec<FactoryEnforcementCapabilityV3>,
}

/// Stable, secret-safe reasons a Factory Job is rejected before source or spawn.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum FactoryPreflightError {
  /// This Agent has no operator-owned Factory grant ceiling.
  #[error("Factory execution is disabled by local Agent policy")]
  Disabled,
  /// The configured local grant ceiling is malformed.
  #[error("local Factory grants are invalid")]
  InvalidLocalGrants,
  /// The already verified signed intent no longer satisfies local invariants.
  #[error("signed Factory intent is invalid")]
  InvalidSignedIntent,
  /// Lease transfer metadata differs from signed stable intent.
  #[error("protected-input transfers differ from signed Factory intent")]
  ProtectedInputTransfer,
  /// A protected mount is absent, writable, or resolves outside the reserved root.
  #[error("Factory protected-input mount policy is invalid")]
  ProtectedInputMount,
  /// Signed plugin authority exceeds local grants or the verified installation.
  #[error("Factory plugin authority is unavailable")]
  Plugin,
  /// Signed executable authority exceeds local grants or the verified installation.
  #[error("Factory executable authority is unavailable")]
  Executable,
  /// Signed tool authority exceeds local grants.
  #[error("Factory tool authority exceeds local grants")]
  Tool,
  /// Signed command or argument authority exceeds local grants.
  #[error("Factory command authority exceeds local grants")]
  Command,
  /// Signed descendant-process authority exceeds local grants.
  #[error("Factory descendant-process authority exceeds local grants")]
  Descendants,
  /// Signed portable path or mount authority exceeds local grants.
  #[error("Factory mount authority exceeds local grants")]
  Mount,
  /// Signed network authority exceeds local grants.
  #[error("Factory network authority exceeds local grants")]
  Network,
  /// Signed secret-profile authority exceeds local grants.
  #[error("Factory secret-profile authority exceeds local grants")]
  SecretProfile,
  /// Signed workload-identity authority exceeds local grants.
  #[error("Factory workload-identity authority exceeds local grants")]
  WorkloadIdentityProfile,
  /// Signed resource authority exceeds local grants.
  #[error("Factory resource authority exceeds local grants")]
  Resources,
  /// Signed output authority exceeds local grants.
  #[error("Factory output authority exceeds local grants")]
  Outputs,
  /// The selected execution route lacks one required semantic control.
  #[error("selected backend cannot enforce the complete Factory permission set")]
  BackendCapability,
  /// A local executable or plugin changed after Agent inventory.
  #[error("verified Factory toolchain changed on disk")]
  ToolchainDrift,
}

/// Complete immutable inputs to one v3 Agent admission decision.
pub(super) struct FactoryAdmission<'a> {
  pub(super) intent: &'a ManagedFactoryExecution,
  pub(super) runtime: &'a RuntimeSpecV2,
  pub(super) outputs: &'a OutputLimits,
  pub(super) transfers: &'a [ProtectedInputTransferV3],
  pub(super) local: Option<&'a FactoryPermissionSetV3>,
  pub(super) backend_capabilities: &'a BTreeSet<FactoryEnforcementCapabilityV3>,
  pub(super) runner: &'a RunnerInstallation,
  pub(super) external_executables: &'a BTreeMap<String, VerifiedExternalExecutable>,
}

impl FactoryAdmission<'_> {
  /// Revalidates every v3 authority boundary before source materialization.
  pub(super) fn authorize(self) -> Result<BTreeMap<String, VerifiedExternalExecutable>, FactoryPreflightError> {
    self
      .intent
      .permissions
      .validate()
      .map_err(|_| FactoryPreflightError::InvalidSignedIntent)?;
    let local = self.local.ok_or(FactoryPreflightError::Disabled)?;
    local
      .validate()
      .map_err(|_| FactoryPreflightError::InvalidLocalGrants)?;
    verify_transfers(&self.intent.protected_inputs, self.transfers)?;
    verify_protected_mount(self.intent)?;
    verify_grant_subset(&self.intent.permissions, local)?;
    verify_secret_profile_selection(&self.intent.execution, &self.intent.permissions)?;
    verify_runtime_projection(&self.intent.permissions, self.runtime, self.outputs)?;
    if self
      .intent
      .required_enforcement
      .iter()
      .any(|capability| !self.backend_capabilities.contains(capability))
    {
      return Err(FactoryPreflightError::BackendCapability);
    }
    verify_plugins(
      &self.intent.permissions.plugins,
      self.intent.execution.tool_control.as_ref(),
      self.runner,
    )?;
    let selected = verify_executables(&self.intent.permissions.executables, self.external_executables)?;
    self
      .runner
      .revalidate_files(&selected)
      .map_err(|_| FactoryPreflightError::ToolchainDrift)?;
    Ok(selected)
  }
}

fn verify_secret_profile_selection(
  execution: &ManagedOctaExecutionV3,
  permissions: &FactoryPermissionSetV3,
) -> Result<(), FactoryPreflightError> {
  match &execution.credential_profile {
    Some(profile) if permissions.secret_profiles.as_slice() == [profile.as_str()] => Ok(()),
    None if permissions.secret_profiles.is_empty() => Ok(()),
    Some(_) | None => Err(FactoryPreflightError::SecretProfile),
  }
}

fn verify_transfers(
  manifest: &ProtectedInputManifestV3,
  transfers: &[ProtectedInputTransferV3],
) -> Result<(), FactoryPreflightError> {
  if transfers.len() != manifest.inputs.len()
    || transfers
      .iter()
      .zip(&manifest.inputs)
      .any(|(transfer, input)| transfer.input != *input)
  {
    return Err(FactoryPreflightError::ProtectedInputTransfer);
  }
  Ok(())
}

fn verify_protected_mount(intent: &ManagedFactoryExecution) -> Result<(), FactoryPreflightError> {
  let mount = intent
    .permissions
    .mounts
    .iter()
    .find(|mount| mount.root == PROTECTED_INPUT_ROOT)
    .ok_or(FactoryPreflightError::ProtectedInputMount)?;
  if mount.mode != FactoryMountModeV3::ReadOnly
    || !intent
      .protected_inputs
      .inputs
      .iter()
      .any(|input| input.artifact_id == intent.execution.octafile_input)
  {
    return Err(FactoryPreflightError::ProtectedInputMount);
  }
  Ok(())
}

fn verify_grant_subset(
  requested: &FactoryPermissionSetV3,
  local: &FactoryPermissionSetV3,
) -> Result<(), FactoryPreflightError> {
  require_references(&requested.plugins, &local.plugins, FactoryPreflightError::Plugin)?;
  require_references(
    &requested.executables,
    &local.executables,
    FactoryPreflightError::Executable,
  )?;
  require_references(&requested.tools, &local.tools, FactoryPreflightError::Tool)?;
  if requested
    .commands
    .iter()
    .any(|command| !local.commands.iter().any(|grant| command_is_within(command, grant)))
  {
    return Err(FactoryPreflightError::Command);
  }
  if requested.max_descendants > local.max_descendants {
    return Err(FactoryPreflightError::Descendants);
  }
  if requested
    .mounts
    .iter()
    .any(|mount| !local.mounts.iter().any(|grant| mount_is_within(mount, grant)))
  {
    return Err(FactoryPreflightError::Mount);
  }
  require_strings(
    &requested.network_hosts,
    &local.network_hosts,
    FactoryPreflightError::Network,
  )?;
  require_strings(
    &requested.secret_profiles,
    &local.secret_profiles,
    FactoryPreflightError::SecretProfile,
  )?;
  require_strings(
    &requested.workload_identity_profiles,
    &local.workload_identity_profiles,
    FactoryPreflightError::WorkloadIdentityProfile,
  )?;
  let resources = requested.resources;
  let local_resources = local.resources;
  if resources.cpu_millis > local_resources.cpu_millis
    || resources.memory_bytes > local_resources.memory_bytes
    || resources.disk_bytes > local_resources.disk_bytes
    || resources.process_count > local_resources.process_count
    || resources.elapsed_millis > local_resources.elapsed_millis
  {
    return Err(FactoryPreflightError::Resources);
  }
  let output = &requested.outputs;
  let local_output = &local.outputs;
  if output.kinds.iter().any(|kind| !local_output.kinds.contains(kind))
    || output.max_artifact_count > local_output.max_artifact_count
    || output.max_artifact_bytes > local_output.max_artifact_bytes
    || output.max_report_count > local_output.max_report_count
    || output.max_report_bytes > local_output.max_report_bytes
  {
    return Err(FactoryPreflightError::Outputs);
  }
  Ok(())
}

fn verify_runtime_projection(
  permissions: &FactoryPermissionSetV3,
  runtime: &RuntimeSpecV2,
  outputs: &OutputLimits,
) -> Result<(), FactoryPreflightError> {
  if runtime
    .workload_identity_profile
    .as_ref()
    .is_some_and(|profile| !permissions.workload_identity_profiles.contains(profile))
  {
    return Err(FactoryPreflightError::WorkloadIdentityProfile);
  }
  match &runtime.network {
    NetworkPolicy::Disabled if permissions.network_hosts.is_empty() => {}
    NetworkPolicy::Restricted { allowed_hosts } if allowed_hosts == &permissions.network_hosts => {}
    NetworkPolicy::Disabled | NetworkPolicy::Restricted { .. } | NetworkPolicy::Unrestricted => {
      return Err(FactoryPreflightError::Network);
    }
  }
  let resources = permissions.resources;
  if resources.cpu_millis > runtime.cpu_millis
    || resources.memory_bytes > runtime.memory_bytes
    || resources.disk_bytes > runtime.writable_disk_bytes
    || resources.elapsed_millis > runtime.timeout_seconds.saturating_mul(1_000)
  {
    return Err(FactoryPreflightError::Resources);
  }
  let permitted = &permissions.outputs;
  if permitted.max_artifact_count > outputs.artifact_count
    || permitted.max_artifact_bytes > outputs.artifact_bytes
    || permitted.max_report_count > outputs.report_count
    || permitted.max_report_bytes > outputs.report_bytes
  {
    return Err(FactoryPreflightError::Outputs);
  }
  Ok(())
}

fn verify_plugins(
  required: &[FactoryImmutableReferenceV3],
  tool_control: Option<&octacity_protocol::FactoryToolControlV3>,
  runner: &RunnerInstallation,
) -> Result<(), FactoryPreflightError> {
  if required.iter().any(|reference| {
    runner.plugins.get(&reference.identity).is_none_or(|installed| {
      installed.version != reference.version
        || installed.sha256 != reference.sha256
        || tool_control.is_some_and(|control| {
          control.plugin == reference.identity && !installed.capabilities.contains(&control.capability)
        })
    })
  }) {
    return Err(FactoryPreflightError::Plugin);
  }
  if tool_control.is_some_and(|control| !required.iter().any(|plugin| plugin.identity == control.plugin)) {
    return Err(FactoryPreflightError::Plugin);
  }
  Ok(())
}

fn verify_executables(
  required: &[FactoryImmutableReferenceV3],
  installed: &BTreeMap<String, VerifiedExternalExecutable>,
) -> Result<BTreeMap<String, VerifiedExternalExecutable>, FactoryPreflightError> {
  required
    .iter()
    .map(|reference| {
      let executable = installed
        .get(&reference.identity)
        .filter(|executable| {
          executable.product == reference.identity
            && executable.version == reference.version
            && executable.sha256 == reference.sha256
        })
        .cloned()
        .ok_or(FactoryPreflightError::Executable)?;
      Ok((reference.identity.clone(), executable))
    })
    .collect()
}

fn require_references(
  requested: &[FactoryImmutableReferenceV3],
  local: &[FactoryImmutableReferenceV3],
  error: FactoryPreflightError,
) -> Result<(), FactoryPreflightError> {
  if requested.iter().any(|reference| !local.contains(reference)) {
    Err(error)
  } else {
    Ok(())
  }
}

fn require_strings(
  requested: &[String],
  local: &[String],
  error: FactoryPreflightError,
) -> Result<(), FactoryPreflightError> {
  if requested.iter().any(|value| !local.contains(value)) {
    Err(error)
  } else {
    Ok(())
  }
}

fn command_is_within(requested: &FactoryCommandPermissionV3, local: &FactoryCommandPermissionV3) -> bool {
  requested.executable == local.executable
    && requested.arguments.len() == local.arguments.len()
    && requested
      .arguments
      .iter()
      .zip(&local.arguments)
      .all(|(requested, local)| match (requested, local) {
        (
          FactoryCommandArgumentV3::Any { max_bytes: requested },
          FactoryCommandArgumentV3::Any { max_bytes: local },
        ) => requested <= local,
        (FactoryCommandArgumentV3::Exact { value }, FactoryCommandArgumentV3::Any { max_bytes }) => {
          usize::try_from(*max_bytes).is_ok_and(|max_bytes| value.len() <= max_bytes)
        }
        (FactoryCommandArgumentV3::Exact { value: requested }, FactoryCommandArgumentV3::Exact { value: local }) => {
          requested == local
        }
        (FactoryCommandArgumentV3::Any { .. }, FactoryCommandArgumentV3::Exact { .. }) => false,
      })
}

fn mount_is_within(requested: &FactoryMountPermissionV3, local: &FactoryMountPermissionV3) -> bool {
  path_is_within(&local.root, &requested.root)
    && (requested.mode == FactoryMountModeV3::ReadOnly || local.mode == FactoryMountModeV3::ReadWrite)
}

fn path_is_within(root: &str, path: &str) -> bool {
  path == root || path.strip_prefix(root).is_some_and(|suffix| suffix.starts_with('/'))
}

#[cfg(test)]
mod tests {
  use octacity_protocol::{FactoryOutputPermissionsV3, FactoryResourceLimitsV3};

  use super::*;

  const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

  #[test]
  fn every_repository_authority_is_bounded_by_independent_local_grants() {
    let requested = permissions();

    assert_rejected_after(&requested, |local| local.plugins.clear(), FactoryPreflightError::Plugin);
    assert_rejected_after(
      &requested,
      |local| local.executables.clear(),
      FactoryPreflightError::Executable,
    );
    assert_rejected_after(&requested, |local| local.tools.clear(), FactoryPreflightError::Tool);
    assert_rejected_after(
      &requested,
      |local| local.commands.clear(),
      FactoryPreflightError::Command,
    );
    assert_rejected_after(
      &requested,
      |local| local.max_descendants -= 1,
      FactoryPreflightError::Descendants,
    );
    assert_rejected_after(
      &requested,
      |local| {
        local.mounts.pop();
      },
      FactoryPreflightError::Mount,
    );
    assert_rejected_after(
      &requested,
      |local| local.network_hosts.clear(),
      FactoryPreflightError::Network,
    );
    assert_rejected_after(
      &requested,
      |local| local.secret_profiles.clear(),
      FactoryPreflightError::SecretProfile,
    );
    assert_rejected_after(
      &requested,
      |local| local.workload_identity_profiles.clear(),
      FactoryPreflightError::WorkloadIdentityProfile,
    );
    assert_rejected_after(
      &requested,
      |local| local.resources.memory_bytes -= 1,
      FactoryPreflightError::Resources,
    );
    assert_rejected_after(
      &requested,
      |local| local.outputs.max_artifact_bytes -= 1,
      FactoryPreflightError::Outputs,
    );
  }

  #[test]
  fn portable_path_containment_is_segment_aware() {
    assert!(path_is_within("/workspace", "/workspace/source"));
    assert!(path_is_within("/workspace", "/workspace"));
    assert!(!path_is_within("/workspace", "/workspace-escape"));
  }

  #[test]
  fn command_wildcards_cannot_widen_local_argument_bounds() {
    let mut requested = permissions();
    requested.commands[0].arguments = vec![FactoryCommandArgumentV3::Any { max_bytes: 5 }];
    let mut local = requested.clone();
    local.commands[0].arguments = vec![FactoryCommandArgumentV3::Any { max_bytes: 4 }];
    assert_eq!(
      verify_grant_subset(&requested, &local),
      Err(FactoryPreflightError::Command)
    );

    requested.commands[0].arguments = vec![FactoryCommandArgumentV3::Exact {
      value: "five!".to_owned(),
    }];
    assert_eq!(
      verify_grant_subset(&requested, &local),
      Err(FactoryPreflightError::Command)
    );
  }

  #[test]
  fn selected_secret_profile_must_be_the_only_signed_profile() {
    let execution = ManagedOctaExecutionV3 {
      octafile_input: "managed-octafile".to_owned(),
      tasks: vec!["implement".to_owned()],
      credential_profile: Some("model-coding".to_owned()),
      tool_control: None,
    };
    let permissions = permissions();
    assert_eq!(verify_secret_profile_selection(&execution, &permissions), Ok(()));

    let mut mismatched = permissions.clone();
    mismatched.secret_profiles = vec!["delivery-write".to_owned()];
    assert_eq!(
      verify_secret_profile_selection(&execution, &mismatched),
      Err(FactoryPreflightError::SecretProfile)
    );
  }

  fn assert_rejected_after(
    requested: &FactoryPermissionSetV3,
    narrow: impl FnOnce(&mut FactoryPermissionSetV3),
    expected: FactoryPreflightError,
  ) {
    let mut local = requested.clone();
    narrow(&mut local);
    assert_eq!(verify_grant_subset(requested, &local), Err(expected));
  }

  fn permissions() -> FactoryPermissionSetV3 {
    let executable = reference("codex-cli");
    FactoryPermissionSetV3 {
      plugins: vec![reference("codex")],
      executables: vec![executable.clone()],
      tools: vec![reference("git")],
      commands: vec![FactoryCommandPermissionV3 {
        executable,
        arguments: vec![FactoryCommandArgumentV3::Exact {
          value: "exec".to_owned(),
        }],
      }],
      max_descendants: 2,
      mounts: vec![
        FactoryMountPermissionV3 {
          root: "/octacity/protected".to_owned(),
          mode: FactoryMountModeV3::ReadOnly,
        },
        FactoryMountPermissionV3 {
          root: "/workspace/output".to_owned(),
          mode: FactoryMountModeV3::ReadWrite,
        },
        FactoryMountPermissionV3 {
          root: "/workspace/scratch".to_owned(),
          mode: FactoryMountModeV3::ReadWrite,
        },
        FactoryMountPermissionV3 {
          root: "/workspace/source".to_owned(),
          mode: FactoryMountModeV3::ReadWrite,
        },
      ],
      network_hosts: vec!["api.openai.com".to_owned()],
      secret_profiles: vec!["model-coding".to_owned()],
      workload_identity_profiles: vec!["github".to_owned()],
      resources: FactoryResourceLimitsV3 {
        cpu_millis: 1_000,
        memory_bytes: 1_024,
        disk_bytes: 2_048,
        process_count: 4,
        elapsed_millis: 60_000,
      },
      outputs: FactoryOutputPermissionsV3 {
        kinds: vec!["codex-result".to_owned()],
        max_artifact_count: 1,
        max_artifact_bytes: 1_024,
        max_report_count: 0,
        max_report_bytes: 0,
      },
    }
  }

  fn reference(identity: &str) -> FactoryImmutableReferenceV3 {
    FactoryImmutableReferenceV3 {
      identity: identity.to_owned(),
      version: "1.0.0".to_owned(),
      sha256: DIGEST.to_owned(),
    }
  }
}
