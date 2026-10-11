use octacity_protocol::{
  FactoryCausalityV3, FactoryEnforcementCapabilityV3, FactoryPermissionSetV3, ManagedOctaExecutionV3,
  ProtectedInputManifestV3,
};
use octacity_server_domain::PipelineNodeId;
use octacity_server_factory::{FactoryPermissionSet, resolve_factory_permissions};
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
  octacity_server_store::factory_job_permissions(permissions)
    .map_err(|_| FactoryManagedJobSpecTemplateError::PermissionProjection)
}
