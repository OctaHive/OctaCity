//! Canonical permission projection shared by signing and atomic Factory Build admission.
use crate::StoreInputError;
use octacity_protocol::{
  FactoryCommandArgumentV3, FactoryCommandPermissionV3, FactoryImmutableReferenceV3, FactoryMountModeV3,
  FactoryMountPermissionV3, FactoryOutputPermissionsV3, FactoryPermissionSetV3, FactoryResourceLimitsV3,
  MAX_FACTORY_COMMAND_ARGUMENT_BYTES,
};
use octacity_server_factory::{CommandArgumentPattern, FactoryPermissionSet, ImmutableReference, MountMode};
/// Projects every canonical permission category into the protected Job wire contract.
pub fn factory_job_permissions(permissions: &FactoryPermissionSet) -> Result<FactoryPermissionSetV3, StoreInputError> {
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
  value.validate().map_err(|_| StoreInputError::InvalidFactoryBuild)?;
  Ok(value)
}

fn wire_reference(reference: &ImmutableReference) -> FactoryImmutableReferenceV3 {
  FactoryImmutableReferenceV3 {
    identity: reference.identity().as_str().to_owned(),
    version: reference.version().as_str().to_owned(),
    sha256: reference.digest().to_string(),
  }
}
