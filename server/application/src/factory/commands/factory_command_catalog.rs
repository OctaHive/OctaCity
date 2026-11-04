//! One installed role selects the same compilation at dispatch and observation.
use crate::*;
use octacity_server_factory::{AdmittedFlow, FlowBuildIntent};

pub(crate) enum FactoryCommandCatalog {
  Builtin(Vec<FactoryCommandBuildProfile>),
  Promotion(Vec<FactoryCommandBuildProfile>),
  Deployment {
    profiles: Vec<FactoryDeploymentCommandProfile>,
    approved: Box<JobSpecToolchainPolicy>,
  },
}
impl FactoryCommandCatalog {
  pub(crate) fn builtin(profiles: Vec<FactoryCommandBuildProfile>) -> Result<Self, OrdinaryBuildApplicationError> {
    if profiles.is_empty() || profiles.len() > MAX_FACTORY_EXECUTOR_BINDINGS {
      return Err(OrdinaryBuildApplicationError::Invalid);
    }
    Ok(Self::Builtin(profiles))
  }
  pub(crate) fn deployment(
    profiles: Vec<FactoryDeploymentCommandProfile>,
    approved: JobSpecToolchainPolicy,
  ) -> Result<Self, OrdinaryBuildApplicationError> {
    if profiles.is_empty() || profiles.len() > MAX_FACTORY_EXECUTOR_BINDINGS {
      return Err(OrdinaryBuildApplicationError::Invalid);
    }
    for (index, profile) in profiles.iter().enumerate() {
      profile.validate_installation(&approved)?;
      if profiles[..index]
        .iter()
        .any(|prior| prior.command().task_digest() == profile.command().task_digest())
      {
        return Err(OrdinaryBuildApplicationError::Invalid);
      }
    }
    Ok(Self::Deployment {
      profiles,
      approved: Box::new(approved),
    })
  }
  pub(crate) fn profiles(&self) -> impl Iterator<Item = &FactoryCommandBuildProfile> {
    let (builtin, deployment): (&[FactoryCommandBuildProfile], &[FactoryDeploymentCommandProfile]) = match self {
      Self::Builtin(profiles) | Self::Promotion(profiles) => (profiles, &[]),
      Self::Deployment { profiles, .. } => (&[], profiles),
    };
    builtin
      .iter()
      .chain(deployment.iter().map(FactoryDeploymentCommandProfile::command))
  }
  pub(crate) fn profile(
    &self,
    intent: &FlowBuildIntent,
  ) -> Result<&FactoryCommandBuildProfile, OrdinaryBuildApplicationError> {
    let mut selected = self.profiles().filter(|profile| profile.matches_intent(intent));
    let profile = selected.next().ok_or(OrdinaryBuildApplicationError::Invalid)?;
    if selected.next().is_some() {
      return Err(OrdinaryBuildApplicationError::Invalid);
    }
    Ok(profile)
  }
  pub(crate) fn is_promotion(&self) -> bool {
    !matches!(self, Self::Builtin(_))
  }
  pub(crate) fn with_promotion(self) -> Self {
    match self {
      Self::Builtin(profiles) => Self::Promotion(profiles),
      role => role,
    }
  }
  pub(crate) fn validate_dispatch(
    &self,
    intent: &FlowBuildIntent,
    actual: &JobSpecToolchainPolicy,
  ) -> Result<(), OrdinaryBuildApplicationError> {
    match self {
      Self::Deployment { approved, .. } => {
        if approved.as_ref() != actual {
          return Err(OrdinaryBuildApplicationError::Invalid);
        }
        self.profile(intent)?;
        Ok(())
      }
      Self::Builtin(_) | Self::Promotion(_) => self.profile(intent)?.validate_installation(actual),
    }
  }
  pub(crate) fn validate_observed_toolchain(
    &self,
    build: &octacity_server_store::ImmutableBuildInput,
    template: &octacity_server_job::JobSpecTemplate,
  ) -> Result<(), OrdinaryBuildApplicationError> {
    if let Self::Deployment { approved, .. } = self {
      let actual: JobSpecToolchainPolicy = serde_json::from_value(
        build
          .effective_policy_snapshot
          .get("job_spec_toolchain")
          .cloned()
          .ok_or(OrdinaryBuildApplicationError::Invalid)?,
      )
      .map_err(|_| OrdinaryBuildApplicationError::Invalid)?;
      if &actual != approved.as_ref() || template.placement_policy().octa != &approved.octa {
        return Err(OrdinaryBuildApplicationError::Invalid);
      }
    }
    Ok(())
  }
  pub(crate) fn compile(
    &self,
    intent: &FlowBuildIntent,
    admitted: &AdmittedFlow,
  ) -> Result<CompiledFactoryCommandTask, OrdinaryBuildApplicationError> {
    let base = self.profile(intent)?;
    match self {
      Self::Builtin(_) => compile_factory_command_task(intent, admitted, base),
      Self::Promotion(_) => compile_factory_command_task(intent, admitted, base)?.with_promotion(
        &octacity_server_factory::PromotionPlan::freeze(admitted, intent.input())
          .map_err(|_| OrdinaryBuildApplicationError::Invalid)?,
        admitted,
      ),
      Self::Deployment { profiles, approved } => compile_factory_deployment_command_task(
        intent,
        admitted,
        profiles
          .iter()
          .find(|profile| profile.command().matches_intent(intent))
          .ok_or(OrdinaryBuildApplicationError::Invalid)?,
        approved,
      ),
    }
  }
}
