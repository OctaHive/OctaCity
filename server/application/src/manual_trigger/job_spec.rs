use octacity_protocol::{
  CachePolicy, NetworkPolicy, OciIsolation, OutputLimits, PlatformSpec, RuntimeSpec, RuntimeTarget,
};
use octacity_server_domain::{BuildId, ImmutableRevision, RuntimeClass};
use octacity_server_job::{JobSpecBuildSnapshot, JobSpecPolicySnapshot, JobSpecTemplate, derive_job_spec_template};
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
  let target = match definition.runtime.class {
    RuntimeClass::Native => RuntimeTarget::Native { platform },
    RuntimeClass::OciProcess | RuntimeClass::OciHypervisor => RuntimeTarget::Oci {
      platform,
      isolation: match definition.runtime.class {
        RuntimeClass::OciProcess => OciIsolation::Process,
        RuntimeClass::OciHypervisor => OciIsolation::Hypervisor,
        RuntimeClass::Native => unreachable!("Native was matched separately"),
      },
      image: definition.runtime.immutable_image.clone().ok_or_else(invalid_policy)?,
    },
  };
  let network = match &definition.runtime.network {
    octacity_server_store::ConfigurationNetworkPolicy::Disabled => NetworkPolicy::Disabled,
    octacity_server_store::ConfigurationNetworkPolicy::Unrestricted => NetworkPolicy::Unrestricted,
    octacity_server_store::ConfigurationNetworkPolicy::Restricted { .. } => NetworkPolicy::Restricted {
      allowed_hosts: definition
        .runtime
        .network
        .allowed_hosts()
        .ok_or_else(invalid_policy)?
        .iter()
        .map(ToString::to_string)
        .collect(),
    },
  };
  let cache = definition.cache.namespace.as_ref().map(|namespace| CachePolicy {
    namespace: namespace.clone(),
    read: definition.cache.read,
    write: definition.cache.write,
  });
  JobSpecPolicySnapshot::new(
    context.job_spec_toolchain.source.clone(),
    context.job_spec_toolchain.octa.clone(),
    RuntimeSpec {
      target,
      cpu_millis: definition.runtime.cpu_millis,
      memory_bytes: definition.runtime.memory_bytes,
      writable_disk_bytes: definition.runtime.writable_disk_bytes,
      timeout_seconds: definition.runtime.timeout_seconds,
      network,
      workload_identity_profile: definition
        .runtime
        .workload_identity_profile
        .as_ref()
        .map(ToString::to_string),
    },
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

fn invalid_policy() -> ManualTriggerError {
  ManualTriggerError::JobSpec(octacity_server_job::JobSpecDerivationError::InvalidPolicy)
}

fn source_reference(source: &ManualSourceSelection) -> Option<octacity_server_domain::SourceReference> {
  match source {
    ManualSourceSelection::Reference(reference) => Some(reference.clone()),
    ManualSourceSelection::DefaultReference | ManualSourceSelection::ExactRevision(_) => None,
  }
}
