//! Policy contents resolved only from exact Configuration pins or retained legacy mappings.
use octacity_server_factory::{FactoryAccessProfile, FactoryConfiguration, FactoryLaunchPolicy};

use crate::{
  FactoryLegacyPolicyReference, FactoryProjectPolicyRecord, FactoryProjectPolicyRef, FactoryProjectPolicyStore,
  StoreError,
};

/// Exact immutable policy contents for admission, independent of current Project defaults.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryResolvedConfigurationPolicies {
  /// Selected launch rules; trusted-origin enforcement belongs to admission.
  pub launch_policy: FactoryLaunchPolicy,
  /// Selected access ceiling; independent installed/Agent limits still apply.
  pub access_profile: FactoryAccessProfile,
}

/// Safe actionable missing-policy classifications; no private content is included.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FactoryPolicyPrerequisite {
  /// The exact Project launch pin is absent, incompatible or foreign.
  LaunchUnavailable,
  /// The exact Project access pin is absent, incompatible or foreign.
  AccessUnavailable,
  /// No retained mapping resolves the complete historical launch reference.
  LaunchUnmapped,
  /// No retained mapping resolves the complete historical access reference.
  AccessUnmapped,
}
impl FactoryPolicyPrerequisite {
  /// Configuration field requiring explicit repair.
  pub const fn field(self) -> &'static str {
    match self {
      Self::LaunchUnavailable | Self::LaunchUnmapped => "/admission_policy",
      Self::AccessUnavailable | Self::AccessUnmapped => "/permission_ceiling",
    }
  }
  /// Stable public reason; no live default is substituted.
  pub const fn reason(self) -> &'static str {
    match self {
      Self::LaunchUnavailable | Self::AccessUnavailable => "unavailable_reference",
      Self::LaunchUnmapped | Self::AccessUnmapped => "unmapped_legacy_policy",
    }
  }
}

/// Bounded exact resolution shared by Configuration, readiness and admission contexts.
/// Missing mappings are prerequisites, while backend failures remain backend failures.
pub async fn resolve_factory_configuration_policies(
  owner: &dyn FactoryProjectPolicyStore,
  configuration: &FactoryConfiguration,
) -> Result<Result<Box<FactoryResolvedConfigurationPolicies>, FactoryPolicyPrerequisite>, StoreError> {
  use FactoryPolicyPrerequisite::*;
  let project = configuration.reference().project_id();
  let launch = match configuration.admission_policy().project_policy() {
    Some(reference) => FactoryProjectPolicyRef::Launch(reference.clone()),
    None => {
      let Some(legacy) = configuration.admission_policy().legacy() else {
        return Ok(Err(LaunchUnavailable));
      };
      let Some(mapping) = owner
        .factory_legacy_policy_mapping(project, &FactoryLegacyPolicyReference::Launch(legacy.clone()))
        .await?
      else {
        return Ok(Err(LaunchUnmapped));
      };
      mapping.target
    }
  };
  let Some(FactoryProjectPolicyRecord::Launch(launch_policy)) = owner.factory_project_policy(project, &launch).await?
  else {
    return Ok(Err(LaunchUnavailable));
  };
  if launch_policy.reference().project_id != project
    || FactoryProjectPolicyRef::Launch(launch_policy.reference().clone()) != launch
  {
    return Ok(Err(LaunchUnavailable));
  }
  let access = match configuration.permission_ceiling().project_policy() {
    Some(reference) => FactoryProjectPolicyRef::Access(reference.clone()),
    None => {
      let Some(legacy) = configuration.permission_ceiling().legacy() else {
        return Ok(Err(AccessUnavailable));
      };
      let Some(mapping) = owner
        .factory_legacy_policy_mapping(project, &FactoryLegacyPolicyReference::Access(legacy.clone()))
        .await?
      else {
        return Ok(Err(AccessUnmapped));
      };
      mapping.target
    }
  };
  let Some(FactoryProjectPolicyRecord::Access(access_profile)) = owner.factory_project_policy(project, &access).await?
  else {
    return Ok(Err(AccessUnavailable));
  };
  if access_profile.reference().project_id != project
    || FactoryProjectPolicyRef::Access(access_profile.reference().clone()) != access
  {
    return Ok(Err(AccessUnavailable));
  }
  Ok(Ok(Box::new(FactoryResolvedConfigurationPolicies {
    launch_policy,
    access_profile: *access_profile,
  })))
}
