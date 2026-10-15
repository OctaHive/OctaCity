//! Shared immutable Build Configuration projection into ordinary Job policy.
use octacity_protocol::{
  CachePolicy, ExecutionMode, ExecutionTargetV2, NetworkPolicy, OciIsolation, OutputLimits, PlatformSpec, RuntimeSpec,
  RuntimeSpecV2, RuntimeTarget,
};
use octacity_server_domain::RuntimeClass;
use octacity_server_job::{JobRuntimePolicy, JobSpecPolicySnapshot};

pub(crate) fn configuration_job_policy(
  context: &crate::BuildExecutionContext,
  secrets_profile: Option<crate::SecretProfileName>,
) -> Result<JobSpecPolicySnapshot, octacity_server_job::JobSpecDerivationError> {
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
    secrets_profile,
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
}

fn network_policy(
  network: &octacity_server_store::ConfigurationNetworkPolicy,
) -> Result<NetworkPolicy, octacity_server_job::JobSpecDerivationError> {
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

fn invalid_policy() -> octacity_server_job::JobSpecDerivationError {
  octacity_server_job::JobSpecDerivationError::InvalidPolicy
}

/// Projects the same immutable placement requirements for ordinary and managed Jobs.
pub(crate) fn configuration_job_requirements(
  definition: &octacity_server_store::BuildConfigurationDefinition,
  node: &octacity_server_pipeline::PipelineNode,
) -> octacity_server_job::JobRequirements {
  let mut capabilities: std::collections::BTreeSet<_> =
    definition.agent_requirements.capabilities.iter().cloned().collect();
  capabilities.extend(node.required_capabilities().iter().cloned());
  octacity_server_job::JobRequirements {
    capabilities,
    labels: definition.agent_requirements.labels.clone(),
    minimum_cpu_millis: definition
      .agent_requirements
      .minimum_cpu_millis
      .max(definition.runtime.cpu_millis),
    minimum_memory_bytes: definition
      .agent_requirements
      .minimum_memory_bytes
      .max(definition.runtime.memory_bytes),
    minimum_disk_bytes: definition
      .agent_requirements
      .minimum_disk_bytes
      .max(definition.runtime.writable_disk_bytes),
    runtime_class: definition.runtime.class,
    operating_system: definition.runtime.operating_system,
    architecture: definition.runtime.architecture,
    host_platform: definition.runtime.host_platform,
    required_guarantees: definition.runtime.required_guarantees.clone(),
  }
}
