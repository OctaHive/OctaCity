use octacity_protocol::{
  FactoryCausalityV3, FactoryCommandArgumentV3, FactoryCommandPermissionV3, FactoryEnforcementCapabilityV3,
  FactoryImmutableReferenceV3, FactoryMountModeV3, FactoryMountPermissionV3, FactoryOutputPermissionsV3,
  FactoryPermissionSetV3, FactoryResourceLimitsV3, MAX_FACTORY_COMMAND_ARGUMENT_BYTES, ManagedOctaExecutionV3,
  ProtectedInputManifestV3,
};
use octacity_server_domain::PipelineNodeId;
use octacity_server_factory::{
  CommandArgumentPattern, FactoryPermissionSet, ImmutableReference, MountMode, resolve_factory_permissions,
};
use octacity_server_job::{
  JobSpecBuildSnapshot, JobSpecDerivationError, JobSpecPolicySnapshot, JobSpecTemplate, ManagedJobSpecIntent,
  derive_managed_job_spec_template,
};
use thiserror::Error;

use crate::FactoryBuildPolicyLayers;

/// Complete immutable application input for one Factory-owned JobSpec v3 template.
///
/// Short-lived download capabilities, credentials, leases, and fences are
/// intentionally absent. They are supplied only after compatible placement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryManagedJobSpecInput {
  /// Ordinary Build and exact source facts already accepted by the server.
  pub build: JobSpecBuildSnapshot,
  /// Pipeline node owning the materialized Job.
  pub pipeline_node_id: PipelineNodeId,
  /// Server-generated Octafile identity and exact task selection.
  pub execution: ManagedOctaExecutionV3,
  /// Immutable Factory causal identities and digests.
  pub causality: FactoryCausalityV3,
  /// Logical immutable protected inputs without transport capabilities.
  pub protected_inputs: ProtectedInputManifestV3,
  /// Project, configuration, task, and local permission ceilings.
  pub policy_layers: FactoryBuildPolicyLayers,
  /// Semantic backend capabilities required by the signed intent.
  pub required_enforcement: Vec<FactoryEnforcementCapabilityV3>,
  /// Exact source, Octa, runtime, cache, output, and validity policy.
  pub policy: JobSpecPolicySnapshot,
}

/// Failure while deriving stable signed-ready Factory Job intent.
#[derive(Debug, Error)]
pub enum FactoryManagedJobSpecTemplateError {
  /// The immutable permission layers are inconsistent or cannot be enforced.
  #[error("Factory Job permissions are invalid")]
  Permissions,
  /// The narrowed policy cannot be represented by the strict JobSpec v3 contract.
  #[error("Factory JobSpec v3 permission projection is invalid")]
  PermissionProjection,
  /// The complete stable template violates JobSpec v3.
  #[error("Factory JobSpec v3 derivation failed")]
  JobSpec(#[from] JobSpecDerivationError),
}

/// Derives one stable v3 template after pure permission intersection.
///
/// The function repeats the authoritative four-layer intersection at the
/// signing boundary, so configuration or task input cannot widen the Project
/// policy even if a caller supplies broader Factory grants.
pub fn derive_factory_managed_job_spec_template(
  input: FactoryManagedJobSpecInput,
) -> Result<JobSpecTemplate, FactoryManagedJobSpecTemplateError> {
  let permissions = resolve_factory_permissions(
    &input.policy_layers.project,
    &input.policy_layers.configuration,
    &input.policy_layers.task,
    &input.policy_layers.local,
  )
  .map_err(|_| FactoryManagedJobSpecTemplateError::Permissions)?;
  debug_assert!(permissions.is_no_broader_than(&input.policy_layers.project));
  let permissions = wire_permissions(&permissions)?;
  let intent = ManagedJobSpecIntent::new(
    input.execution,
    input.causality,
    input.protected_inputs,
    permissions,
    input.required_enforcement,
  );
  derive_managed_job_spec_template(&input.build, input.pipeline_node_id, intent, &input.policy).map_err(Into::into)
}

fn wire_permissions(
  permissions: &FactoryPermissionSet,
) -> Result<FactoryPermissionSetV3, FactoryManagedJobSpecTemplateError> {
  let resources = permissions.resources();
  let outputs = permissions.outputs();
  let value = FactoryPermissionSetV3 {
    plugins: permissions.plugins().map(wire_reference).collect(),
    executables: permissions.executables().map(wire_reference).collect(),
    tools: permissions.tools().map(wire_reference).collect(),
    commands: permissions
      .commands()
      .map(|command| FactoryCommandPermissionV3 {
        executable: wire_reference(command.executable()),
        arguments: command
          .arguments()
          .iter()
          .map(|argument| match argument {
            CommandArgumentPattern::Exact(value) => FactoryCommandArgumentV3::Exact {
              value: value.as_str().to_owned(),
            },
            CommandArgumentPattern::Any => FactoryCommandArgumentV3::Any {
              max_bytes: u32::try_from(MAX_FACTORY_COMMAND_ARGUMENT_BYTES)
                .expect("the protocol command argument bound fits u32"),
            },
          })
          .collect(),
      })
      .collect(),
    max_descendants: permissions.max_descendants(),
    mounts: permissions
      .mounts()
      .map(|mount| FactoryMountPermissionV3 {
        root: mount.root().as_str().to_owned(),
        mode: match mount.mode() {
          MountMode::ReadOnly => FactoryMountModeV3::ReadOnly,
          MountMode::ReadWrite => FactoryMountModeV3::ReadWrite,
        },
      })
      .collect(),
    network_hosts: permissions
      .network_hosts()
      .map(|host| host.as_str().to_owned())
      .collect(),
    secret_profiles: permissions
      .secret_profiles()
      .map(|profile| profile.as_str().to_owned())
      .collect(),
    workload_identity_profiles: permissions
      .workload_identity_profiles()
      .map(|profile| profile.as_str().to_owned())
      .collect(),
    resources: FactoryResourceLimitsV3 {
      cpu_millis: resources.cpu_millis(),
      memory_bytes: resources.memory_bytes(),
      disk_bytes: resources.disk_bytes(),
      process_count: resources.process_count(),
      elapsed_millis: resources.elapsed_millis(),
    },
    outputs: FactoryOutputPermissionsV3 {
      kinds: outputs.kinds().map(|kind| kind.as_str().to_owned()).collect(),
      max_artifact_count: outputs.max_artifact_count(),
      max_artifact_bytes: outputs.max_artifact_bytes(),
      max_report_count: outputs.max_report_count(),
      max_report_bytes: outputs.max_report_bytes(),
    },
  };
  value
    .validate()
    .map_err(|_| FactoryManagedJobSpecTemplateError::PermissionProjection)?;
  Ok(value)
}

fn wire_reference(reference: &ImmutableReference) -> FactoryImmutableReferenceV3 {
  FactoryImmutableReferenceV3 {
    identity: reference.identity().as_str().to_owned(),
    version: reference.version().as_str().to_owned(),
    sha256: reference.digest().to_string(),
  }
}
