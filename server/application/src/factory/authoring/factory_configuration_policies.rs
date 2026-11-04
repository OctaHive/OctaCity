//! Exact Project policy resolution for Configuration management; defaults never repin a document.
use crate::{ApplicationError, FactoryConfigurationValidation, FactoryValidationDiagnostics};
use octacity_server_factory::*;
use octacity_server_store::FactoryProjectPolicyStore;

pub(crate) async fn resolve(
  owner: Option<&dyn FactoryProjectPolicyStore>,
  policy: &dyn crate::ManagementAuthorizationPolicy,
  context: &crate::ManagementRequestContext,
  configuration: FactoryConfiguration,
  choices: &FactoryConfigurationChoices,
) -> Result<FactoryConfigurationValidation, ApplicationError> {
  let Some(owner) = owner else {
    return if configuration.admission_policy().project_policy().is_some() {
      Err(FactoryValidationDiagnostics::unavailable("/admission_policy"))
    } else {
      Ok(validation(configuration, None))
    };
  };
  crate::factory::authoring::factory_configuration_choices::require_visible_reference(
    policy,
    context,
    crate::ManagementResourceKind::Project,
    configuration.reference().project_id(),
  )
  .await?;
  let policies = octacity_server_store::resolve_factory_configuration_policies(owner, &configuration)
    .await?
    .map_err(prerequisite)?;
  if !crate::factory::authoring::factory_project_policies::access_fits_choices(&policies.access_profile, choices) {
    return Err(access_limit());
  }
  let configuration = configuration
    .with_project_policies(
      policies.launch_policy.reference().clone(),
      policies.access_profile.reference().clone(),
    )
    .map_err(FactoryValidationDiagnostics::from_factory_error)?;
  Ok(validation(configuration, Some(&policies)))
}

pub(crate) fn prerequisite(missing: octacity_server_store::FactoryPolicyPrerequisite) -> ApplicationError {
  FactoryValidationDiagnostics::unavailable_with_code(missing.field(), missing.reason())
}

fn validation(
  configuration: FactoryConfiguration,
  policies: Option<&octacity_server_store::FactoryResolvedConfigurationPolicies>,
) -> FactoryConfigurationValidation {
  let mut diagnostics = FactoryValidationDiagnostics::for_configuration(&configuration);
  if let Some(policies) = policies {
    for node in &mut diagnostics.effective_permissions {
      node.permissions = node
        .permissions
        .intersection(&policies.access_profile.definition().permissions);
      node.sources.push("project_policy");
      node.unresolved_sources.retain(|source| *source != "project_policy");
      for profile in &mut node.profiles {
        profile.permissions = profile.permissions.intersection(&node.permissions);
        profile.sources.push("project_policy");
      }
    }
  }
  FactoryConfigurationValidation {
    configuration,
    diagnostics,
  }
}

fn access_limit() -> ApplicationError {
  ApplicationError::FactoryValidation(Box::new(FactoryValidationDiagnostics::single_issue(
    "/permission_ceiling",
    "permission_exceeds_ceiling",
    Some("installed_runtime_policy"),
  )))
}
