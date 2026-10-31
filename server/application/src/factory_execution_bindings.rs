//! Read-only execution binding resolution from existing installation and Role/Profile facts.
use crate::*;
use octacity_server_factory::*;
use octacity_server_store::{
  FactoryConfigurationStore, FactoryRoleProfileStore, IntegrationAdministrationStore, IntegrationConnectionStore,
  IntegrationProjectAccessStore,
};

pub(super) fn for_profile(
  choices: &FactoryConfigurationChoices,
  profile: &FactoryRoleProfile,
  result_outcome: &FactoryKey,
) -> Option<FlowBuildBinding> {
  let definition = profile.definition();
  let mut candidates = choices.build_pipeline_contracts().iter().filter_map(|installed| {
    if installed.build_configuration != definition.build_configuration
      || installed.tool != definition.toolchain.executable
      || installed.plugin != definition.toolchain.plugin
      || installed.model_or_tool != definition.toolchain.model
    {
      return None;
    }
    let compilation = installed.compilation.as_ref()?;
    let result_tool = if definition.execution.mcp_servers.is_empty() {
      None
    } else {
      Some(compilation.mcp_result_tool.clone()?)
    };
    let build = FlowBuildBinding {
      tool: installed.tool.clone(),
      plugin: installed.plugin.clone(),
      model_or_tool: installed.model_or_tool.clone(),
      build_configuration: installed.build_configuration.clone(),
      task_digest: if compilation.role_profiles {
        crate::factory_role_task_digest(compilation.task_digest, profile)
      } else {
        compilation.task_digest
      },
      result_output: compilation.result_output.clone(),
      result_output_kind: compilation.result_output_kind,
      result_outcome: result_outcome.clone(),
      result_tool,
      capture_write_paths: None,
      capture_deliverables: None,
      evidence: vec![],
    };
    choices.supports_build_binding(&build).then_some(build)
  });
  let first = candidates.next()?;
  candidates.next().is_none().then_some(first)
}

/// Resolves one ordinary node binding without an existing Flow or any publication.
#[derive(Clone, Debug)]
pub struct ResolveFactoryExecutionBindingQuery {
  /// Authorized owning Project.
  pub project_id: octacity_server_domain::ProjectId,
  /// Exact already-published executor profile; never mutable inline settings.
  pub profile: FactoryRoleProfileRef,
  /// Explicit installed capability selected from the authorized Project catalog.
  pub capability: FactoryKey,
  /// Caller-declared finite observation outcome; no business routing is inferred.
  pub result_outcome: FactoryKey,
}
impl Query for ResolveFactoryExecutionBindingQuery {
  type Outcome = FactoryExecutionBindingResolution;
}
impl ManagementAuthorizationTarget for ResolveFactoryExecutionBindingQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping = GetFactoryRoleProfileQuery::AUTHORIZATION;
  fn management_resource(&self) -> ManagementResourceResult {
    crate::management_security::instance_resource(ManagementResourceKind::Project, self.project_id)
  }
}
/// Safe prerequisites, without hidden dependency identities or credential details.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryExecutionBindingProblem {
  /// The retained profile does not fit the currently authorized installed ceiling.
  ProfileIncompatible,
  /// No unique installed compiler/result contract matches the selected profile.
  BuildUnavailable,
}
/// Exact compiler facts for review, or an actionable prerequisite; not an execution grant.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FactoryExecutionBindingResolution {
  /// Ordinary immutable binding with its server-derived profile task digest.
  pub build: Option<FlowBuildBinding>,
  /// Missing/incompatible installation prerequisite, absent only with a binding.
  pub incompatibility: Option<FactoryExecutionBindingProblem>,
}
#[async_trait::async_trait]
impl<
  S: FactoryConfigurationStore
    + FactoryRoleProfileStore
    + IntegrationProjectAccessStore
    + IntegrationConnectionStore
    + IntegrationAdministrationStore
    + 'static,
> ManagementQueryUseCase<ResolveFactoryExecutionBindingQuery> for FactoryRoleProfileHandlers<S>
{
  type Error = ApplicationError;
  async fn execute_management_query(
    &self,
    context: &ManagementRequestContext,
    grant: &ManagementAuthorizationGrant,
    query: ResolveFactoryExecutionBindingQuery,
  ) -> Result<FactoryExecutionBindingResolution, ApplicationError> {
    crate::factory_role_profiles::require_project_visibility(grant, query.project_id)?;
    if query.profile.project_id != query.project_id {
      return Err(ApplicationError::invalid());
    }
    let profile = self
      .store
      .factory_role_profile(query.project_id, query.profile.id, Some(query.profile.version))
      .await?;
    if profile.reference() != &query.profile {
      return Err(ApplicationError::invalid());
    }
    let (capability, choices) = self
      .profile_capability(context, query.project_id, &query.capability)
      .await?;
    if !choices.role_is_available(profile.role().id) || capability.validate(&profile).is_err() {
      return Ok(FactoryExecutionBindingResolution {
        build: None,
        incompatibility: Some(FactoryExecutionBindingProblem::ProfileIncompatible),
      });
    }
    let build = for_profile(&choices, &profile, &query.result_outcome).filter(|build| {
      // Manual agent selection requires a compiler that actually retains the
      // exact Role/Profile, rather than the static legacy task contract.
      choices.build_pipeline_contract(build).is_some_and(|installed| {
        installed
          .compilation
          .as_ref()
          .is_some_and(|compiler| compiler.role_profiles)
      })
    });
    let incompatibility = build
      .is_none()
      .then_some(FactoryExecutionBindingProblem::BuildUnavailable);
    Ok(FactoryExecutionBindingResolution { build, incompatibility })
  }
}
