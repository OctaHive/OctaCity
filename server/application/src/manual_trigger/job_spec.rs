use octacity_protocol::{
  CachePolicy, ExecutionMode, ExecutionTargetV2, NetworkPolicy, OciIsolation, OutputLimits, PlatformSpec, RuntimeSpec,
  RuntimeSpecV2, RuntimeTarget,
};
use octacity_server_domain::{BuildId, ImmutableRevision, RuntimeClass};
use octacity_server_job::{
  JobRuntimePolicy, JobSpecBuildSnapshot, JobSpecPolicySnapshot, JobSpecTemplate, derive_job_spec_template,
};
use octacity_server_pipeline::PipelineNode;

use super::{
  model::{ManualSourceSelection, ManualTriggerContext, ManualTriggerError},
  preparation::PreparedManualTrigger,
};

pub(super) struct JobSpecTemplateDeriver {
  build: JobSpecBuildSnapshot,
  policy: JobSpecPolicySnapshot,
}

impl JobSpecTemplateDeriver {
  pub(super) fn new(
    context: &ManualTriggerContext,
    build_id: BuildId,
    prepared: &PreparedManualTrigger,
    immutable_revision: &ImmutableRevision,
  ) -> Result<Self, ManualTriggerError> {
    let build = JobSpecBuildSnapshot::new(
      build_id,
      immutable_revision.clone(),
      source_reference(&prepared.source),
      context.repository.definition.repository_locator.clone(),
      prepared.parameters.clone(),
    )
    .map_err(ManualTriggerError::JobSpec)?;
    Ok(Self {
      build,
      policy: policy(context)?,
    })
  }

  pub(super) fn derive(&self, node: &PipelineNode) -> Result<JobSpecTemplate, ManualTriggerError> {
    derive_job_spec_template(&self.build, node.id().clone(), node.template().clone(), &self.policy)
      .map_err(ManualTriggerError::JobSpec)
  }
}

fn policy(context: &ManualTriggerContext) -> Result<JobSpecPolicySnapshot, ManualTriggerError> {
  let definition = &context.configuration.definition;
  let platform = PlatformSpec {
    os: definition.runtime.operating_system,
    architecture: definition.runtime.architecture,
  };
  let runtime = match definition.runtime.class {
    RuntimeClass::Native => JobRuntimePolicy::Legacy(RuntimeSpec {
      target: RuntimeTarget::Native { platform },
      cpu_millis: definition.runtime.cpu_millis,
      memory_bytes: definition.runtime.memory_bytes,
      writable_disk_bytes: definition.runtime.writable_disk_bytes,
      timeout_seconds: definition.runtime.timeout_seconds,
      network: network_policy(&definition.runtime.network)?,
      workload_identity_profile: definition
        .runtime
        .workload_identity_profile
        .as_ref()
        .map(ToString::to_string),
    }),
    RuntimeClass::OciProcess | RuntimeClass::OciHypervisor => {
      let target = RuntimeTarget::Oci {
        platform,
        isolation: match definition.runtime.class {
          RuntimeClass::OciProcess => OciIsolation::Process,
          RuntimeClass::OciHypervisor => OciIsolation::Hypervisor,
          RuntimeClass::Native | RuntimeClass::Host | RuntimeClass::Isolation | RuntimeClass::Virtualization => {
            unreachable!("legacy variants were matched separately")
          }
        },
        image: definition.runtime.immutable_image.clone().ok_or_else(invalid_policy)?,
      };
      JobRuntimePolicy::Legacy(RuntimeSpec {
        target,
        cpu_millis: definition.runtime.cpu_millis,
        memory_bytes: definition.runtime.memory_bytes,
        writable_disk_bytes: definition.runtime.writable_disk_bytes,
        timeout_seconds: definition.runtime.timeout_seconds,
        network: network_policy(&definition.runtime.network)?,
        workload_identity_profile: definition
          .runtime
          .workload_identity_profile
          .as_ref()
          .map(ToString::to_string),
      })
    }
    RuntimeClass::Host | RuntimeClass::Isolation | RuntimeClass::Virtualization => {
      let mode = match definition.runtime.class {
        RuntimeClass::Host => ExecutionMode::Host,
        RuntimeClass::Isolation => ExecutionMode::Isolation,
        RuntimeClass::Virtualization => ExecutionMode::Virtualization,
        RuntimeClass::Native | RuntimeClass::OciProcess | RuntimeClass::OciHypervisor => unreachable!(),
      };
      JobRuntimePolicy::Current(RuntimeSpecV2 {
        target: ExecutionTargetV2 {
          mode,
          host_platform: definition.runtime.host_platform.ok_or_else(invalid_policy)?,
          target_platform: platform,
          required_guarantees: definition.runtime.required_guarantees.clone(),
          immutable_image: definition.runtime.immutable_image.clone(),
        },
        cpu_millis: definition.runtime.cpu_millis,
        memory_bytes: definition.runtime.memory_bytes,
        writable_disk_bytes: definition.runtime.writable_disk_bytes,
        timeout_seconds: definition.runtime.timeout_seconds,
        network: network_policy(&definition.runtime.network)?,
        workload_identity_profile: definition
          .runtime
          .workload_identity_profile
          .as_ref()
          .map(ToString::to_string),
      })
    }
  };
  let cache = definition.cache.namespace.as_ref().map(|namespace| CachePolicy {
    namespace: namespace.clone(),
    read: definition.cache.read,
    write: definition.cache.write,
  });
  JobSpecPolicySnapshot::new(
    context.job_spec_toolchain.source.clone(),
    context.job_spec_toolchain.octa.clone(),
    runtime,
    definition.secrets_profile.clone(),
    cache,
    OutputLimits {
      artifact_count: definition.artifacts.artifact_count,
      artifact_bytes: definition.artifacts.artifact_bytes,
      report_count: definition.artifacts.report_count,
      report_bytes: definition.artifacts.report_bytes,
      single_output_bytes: definition.artifacts.single_output_bytes,
    },
    context.job_spec_toolchain.validity,
  )
  .map_err(ManualTriggerError::JobSpec)
}

fn network_policy(
  network: &octacity_server_store::ConfigurationNetworkPolicy,
) -> Result<NetworkPolicy, ManualTriggerError> {
  Ok(match network {
    octacity_server_store::ConfigurationNetworkPolicy::Disabled => NetworkPolicy::Disabled,
    octacity_server_store::ConfigurationNetworkPolicy::Unrestricted => NetworkPolicy::Unrestricted,
    octacity_server_store::ConfigurationNetworkPolicy::Restricted { .. } => NetworkPolicy::Restricted {
      allowed_hosts: network
        .allowed_hosts()
        .ok_or_else(invalid_policy)?
        .iter()
        .map(ToString::to_string)
        .collect(),
    },
  })
}

fn invalid_policy() -> ManualTriggerError {
  ManualTriggerError::JobSpec(octacity_server_job::JobSpecDerivationError::InvalidPolicy)
}

fn source_reference(source: &ManualSourceSelection) -> Option<octacity_server_domain::SourceReference> {
  match source {
    ManualSourceSelection::Reference(reference) => Some(reference.clone()),
    ManualSourceSelection::DefaultReference | ManualSourceSelection::ExactRevision(_) => None,
  }
}
