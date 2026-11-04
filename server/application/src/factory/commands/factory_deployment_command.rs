//! Explicit installed deployment authority for static ordinary command programs.
use crate::{
  CompiledFactoryCommandTask, FactoryCommandBuildProfile, JobSpecToolchainPolicy, OrdinaryBuildApplicationError,
  compile_factory_command_task,
};
use octacity_server_factory::{
  AdmittedFlow, FactoryDigest, FactoryKey, FlowBuildIntent, ImmutableReference, MountMode, NetworkHost, PromotionPlan,
};
use serde::Serialize;
use url::{Host, Url};

/// One immutable operator-installed deployment target; no credential bytes are retained.
#[derive(Clone, Serialize)]
pub struct FactoryDeploymentCommandAuthority {
  authority: ImmutableReference,
  environment: ImmutableReference,
  endpoint: String,
  credential_profile: Option<FactoryKey>,
}
impl FactoryDeploymentCommandAuthority {
  /// Pins the exact environment, canonical HTTP(S) endpoint and separate optional
  /// Agent-local credential selection under a domain-separated policy identity.
  pub fn new(
    identity: FactoryKey,
    version: FactoryKey,
    environment: ImmutableReference,
    endpoint: String,
    credential_profile: Option<FactoryKey>,
  ) -> Result<Self, OrdinaryBuildApplicationError> {
    endpoint_host(&endpoint)?;
    let bytes = serde_json::to_vec(&(&environment, &endpoint, &credential_profile))
      .map_err(|_| OrdinaryBuildApplicationError::Invalid)?;
    let authority = ImmutableReference::new(
      identity,
      version,
      FactoryDigest::sha256("octacity.factory.deployment-command-authority.v1", &[&bytes]),
    );
    Ok(Self {
      authority,
      environment,
      endpoint,
      credential_profile,
    })
  }
  /// Returns the exact policy reference required by the installed command identity.
  pub fn reference(&self) -> &ImmutableReference {
    &self.authority
  }
  /// Returns the single independently installed immutable environment contract.
  pub fn environment(&self) -> &ImmutableReference {
    &self.environment
  }
  /// Returns the canonical operator-selected target, independent of Flow data.
  pub fn endpoint(&self) -> &str {
    &self.endpoint
  }
  /// Returns only the logical Agent-local selection, never credential bytes.
  pub fn credential_profile(&self) -> Option<&FactoryKey> {
    self.credential_profile.as_ref()
  }
}
fn endpoint_host(endpoint: &str) -> Result<NetworkHost, OrdinaryBuildApplicationError> {
  let invalid = || OrdinaryBuildApplicationError::Invalid;
  let url = Url::parse(endpoint).map_err(|_| invalid())?;
  if !matches!(url.scheme(), "http" | "https")
    || url.as_str() != endpoint
    || !url.username().is_empty()
    || url.password().is_some()
    || url.query().is_some()
    || url.fragment().is_some()
  {
    return Err(invalid());
  }
  let host = match url.host().ok_or_else(invalid)? {
    Host::Domain(name) => name.to_owned(),
    Host::Ipv4(address) => address.to_string(),
    Host::Ipv6(address) => address.to_string(),
  };
  NetworkHost::new(host).map_err(|_| invalid())
}

/// A deployment-only qualification, separate from hermetic author/validator commands.
/// This concrete adapter supports one environment per action; the core Promotion
/// Plan remains capable of expressing multiple environments.
#[derive(Clone)]
pub struct FactoryDeploymentCommandProfile {
  command: FactoryCommandBuildProfile,
  authority: FactoryDeploymentCommandAuthority,
}
impl FactoryDeploymentCommandProfile {
  /// Requires an explicit immutable command identity repin for the target policy.
  pub fn new(
    command: FactoryCommandBuildProfile,
    authority: FactoryDeploymentCommandAuthority,
  ) -> Result<Self, OrdinaryBuildApplicationError> {
    if command.identity != *authority.reference() {
      return Err(OrdinaryBuildApplicationError::Invalid);
    }
    Ok(Self { command, authority })
  }
  /// Returns the unchanged static command and its independently pinned ceilings.
  pub fn command(&self) -> &FactoryCommandBuildProfile {
    &self.command
  }
  /// Returns the exact separate deployment authority.
  pub fn authority(&self) -> &FactoryDeploymentCommandAuthority {
    &self.authority
  }
  /// Qualifies exact approved plugins and only the installed target's host and
  /// optional credential, without external executable, tool or identity authority.
  pub fn validate_installation(&self, approved: &JobSpecToolchainPolicy) -> Result<(), OrdinaryBuildApplicationError> {
    use octacity_protocol::{FACTORY_OUTPUT_ROOT, FACTORY_SCRATCH_ROOT, FACTORY_SOURCE_ROOT, PROTECTED_INPUT_ROOT};
    let invalid = || OrdinaryBuildApplicationError::Invalid;
    self.command.validate_plugin_inventory(approved)?;
    let host = endpoint_host(&self.authority.endpoint)?;
    let expected_secrets = self.authority.credential_profile.iter().collect::<Vec<_>>();
    if self.command.identity != *self.authority.reference()
      || self.command.program.credential_profile != self.authority.credential_profile
      || self.command.outputs.is_empty()
      || self.command.outputs.len() > 64
      || self.command.outputs.iter().enumerate().any(|(index, output)| {
        output.plugin() != &self.command.plugin
          || output.tool() != &self.command.executor
          || self.command.outputs[..index]
            .iter()
            .any(|prior| prior.kind() == output.kind())
          || !self
            .command
            .project_permissions
            .outputs()
            .kinds()
            .any(|kind| kind == output.kind())
      })
    {
      return Err(invalid());
    }
    for grants in [
      &self.command.project_permissions,
      self.command.local_permissions.grants(),
    ] {
      if grants.executables().len() != 0
        || grants.commands().len() != 0
        || grants.tools().len() != 0
        || grants.workload_identity_profiles().len() != 0
        || grants.network_hosts().collect::<Vec<_>>() != [&host]
        || grants.secret_profiles().collect::<Vec<_>>() != expected_secrets
        || grants.mounts().any(|mount| {
          !matches!(
            (mount.root().as_str(), mount.mode()),
            (FACTORY_SOURCE_ROOT | PROTECTED_INPUT_ROOT, MountMode::ReadOnly)
              | (FACTORY_OUTPUT_ROOT | FACTORY_SCRATCH_ROOT, MountMode::ReadWrite)
          )
        })
        || [
          FACTORY_SOURCE_ROOT,
          PROTECTED_INPUT_ROOT,
          FACTORY_OUTPUT_ROOT,
          FACTORY_SCRATCH_ROOT,
        ]
        .into_iter()
        .any(|root| grants.mounts().filter(|mount| mount.root().as_str() == root).count() != 1)
      {
        return Err(invalid());
      }
    }
    Ok(())
  }
}

/// Qualifies a static deployment command against its admitted Promotion Plan.
/// This compiles no readiness, deployment receipt, result or lifecycle transition.
pub fn compile_factory_deployment_command_task(
  intent: &FlowBuildIntent,
  admitted: &AdmittedFlow,
  profile: &FactoryDeploymentCommandProfile,
  approved: &JobSpecToolchainPolicy,
) -> Result<CompiledFactoryCommandTask, OrdinaryBuildApplicationError> {
  profile.validate_installation(approved)?;
  let effective = octacity_server_factory::resolve_factory_permissions(
    &profile.command.project_permissions,
    admitted.limits().permissions(),
    intent.permissions(),
    &profile.command.local_permissions,
  )
  .map_err(|_| OrdinaryBuildApplicationError::Invalid)?;
  if &effective != intent.permissions() {
    return Err(OrdinaryBuildApplicationError::Invalid);
  }
  let plan = PromotionPlan::freeze(admitted, intent.input()).map_err(|_| OrdinaryBuildApplicationError::Invalid)?;
  if plan.policy().environments.len() != 1
    || plan.policy().environments.values().next() != Some(profile.authority.environment())
  {
    return Err(OrdinaryBuildApplicationError::Invalid);
  }
  compile_factory_command_task(intent, admitted, &profile.command)?
    .with_promotion(&plan, admitted)?
    .with_deployment_authority(&profile.authority)
}
