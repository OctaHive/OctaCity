//! Common Project policy checks before ordinary Build materialization.
use crate::BuildExecutionContext;
use octacity_protocol::{ExecutionMode, PlatformSpec};
use octacity_server_domain::RuntimeClass;

#[derive(Clone, Copy, Debug)]
pub(crate) enum BuildConfigurationPolicyError {
  RepositoryNotAllowed,
  RuntimeNotAllowed,
  CacheNotAllowed,
  WorkloadIdentityNotAllowed,
  SecretProfileNotAllowed,
  ArtifactPolicyTooBroad,
}

pub(crate) fn validate_configuration_policy(
  context: &BuildExecutionContext,
) -> Result<(), BuildConfigurationPolicyError> {
  let definition = &context.configuration.definition;
  let policy = &context.effective_policy.policy;
  if !policy.repositories.contains(&context.repository.id) {
    return Err(BuildConfigurationPolicyError::RepositoryNotAllowed);
  }
  if !runtime_allowed(policy, &definition.runtime) {
    return Err(BuildConfigurationPolicyError::RuntimeNotAllowed);
  }
  if let Some(namespace) = &definition.cache.namespace {
    let namespace_allowed = policy
      .cache
      .namespaces
      .iter()
      .any(|allowed| allowed.as_str() == namespace);
    if !namespace_allowed
      || definition.cache.read && !policy.cache.read
      || definition.cache.write && !policy.cache.write
    {
      return Err(BuildConfigurationPolicyError::CacheNotAllowed);
    }
  }
  if let Some(identity) = &definition.runtime.workload_identity_profile
    && !policy.identity_profiles.contains(identity)
  {
    return Err(BuildConfigurationPolicyError::WorkloadIdentityNotAllowed);
  }
  if let Some(profile) = &definition.secrets_profile
    && !policy.secret_profiles.contains(profile)
  {
    return Err(BuildConfigurationPolicyError::SecretProfileNotAllowed);
  }
  if !artifact_policy_within(definition.artifacts, policy.artifacts) {
    return Err(BuildConfigurationPolicyError::ArtifactPolicyTooBroad);
  }
  Ok(())
}

fn runtime_allowed(policy: &crate::ProjectPolicy, runtime: &octacity_server_store::ConfigurationRuntimePolicy) -> bool {
  match runtime.class {
    RuntimeClass::Native | RuntimeClass::OciProcess | RuntimeClass::OciHypervisor => {
      policy.runtimes.contains(&runtime.class)
    }
    RuntimeClass::Host | RuntimeClass::Isolation | RuntimeClass::Virtualization => {
      let Some(host_platform) = runtime.host_platform else {
        return false;
      };
      let mode = match runtime.class {
        RuntimeClass::Host => ExecutionMode::Host,
        RuntimeClass::Isolation => ExecutionMode::Isolation,
        RuntimeClass::Virtualization => ExecutionMode::Virtualization,
        RuntimeClass::Native | RuntimeClass::OciProcess | RuntimeClass::OciHypervisor => unreachable!(),
      };
      policy.execution_targets.contains(&crate::ProjectExecutionTarget {
        mode,
        host_platform,
        target_platform: PlatformSpec {
          os: runtime.operating_system,
          architecture: runtime.architecture,
        },
        required_guarantees: runtime.required_guarantees.clone(),
      })
    }
  }
}

fn artifact_policy_within(
  requested: octacity_server_domain::ArtifactPolicy,
  allowed: octacity_server_domain::ArtifactPolicy,
) -> bool {
  requested.artifact_count <= allowed.artifact_count
    && requested.artifact_bytes <= allowed.artifact_bytes
    && requested.report_count <= allowed.report_count
    && requested.report_bytes <= allowed.report_bytes
    && requested.single_output_bytes <= allowed.single_output_bytes
}
