//! Trusted installed compatibility, attached to the existing Factory choice catalog.
use crate::*;
use serde_json::Value;

/// Installed ceiling and supported settings for one exact technical profile.
/// This declaration is read-only validation input, never dispatch or credential authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryRoleProfileCapability {
  /// Reuses ordinary profile settings as the installed ceiling; name/instructions are authoring content.
  pub ceiling: FactoryRoleProfileDefinition,
  /// Existing closed schema for model parameters, including provider-declared thinking values.
  pub parameters: FlowDataSchema,
  /// Existing closed schema for explicit public environment names and values.
  pub public_environment: std::collections::BTreeMap<String, FlowDataSchema>,
  /// Qualified backend enforcement and local grants, independent of instructional text.
  pub local_permissions: LocalPermissionCeiling,
}
/// Safe incompatibility categories; no submitted values or private content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FactoryRoleProfileIncompatibility {
  /// Parameters are unsupported by the installed runtime/model schema.
  ModelParameters,
  /// Exact runtime/model or ordinary Build binding is unavailable.
  Toolchain,
  /// An explicit environment entry is undeclared or changes a secret binding.
  Environment,
  /// Arguments do not match the installed approved vector.
  Arguments,
  /// A skill, Context, MCP or integration selection is outside the installed ceiling.
  Dependencies,
  /// Credential consumer purpose differs from the installed contract.
  CredentialPurpose,
  /// Requested usage exceeds its installed ceiling.
  Budget,
  /// Requested authority cannot be preserved under installed local enforcement.
  Permissions,
}
impl FactoryRoleProfileCapability {
  /// Checks settings against exact installed contracts without executing or resolving secrets.
  pub fn validate(&self, profile: &FactoryRoleProfile) -> Result<(), FactoryRoleProfileIncompatibility> {
    let parameters = serde_json::to_value(&profile.definition().model_parameters)
      .map_err(|_| FactoryRoleProfileIncompatibility::ModelParameters)?;
    FlowPayload::new(&self.parameters, parameters).map_err(|_| FactoryRoleProfileIncompatibility::ModelParameters)?;
    use FactoryRoleProfileIncompatibility as Failure;
    let definition = profile.definition();
    let ceiling = &self.ceiling;
    if definition.toolchain != ceiling.toolchain || definition.build_configuration != ceiling.build_configuration {
      return Err(Failure::Toolchain);
    }
    if definition.credential_purpose != ceiling.credential_purpose {
      return Err(Failure::CredentialPurpose);
    }
    if definition.execution.cli_arguments != ceiling.execution.cli_arguments {
      return Err(Failure::Arguments);
    }
    for (name, value) in &definition.execution.environment {
      match value {
        FactoryRoleEnvironmentValue::Public { value } => {
          if matches!(
            ceiling.execution.environment.get(name),
            Some(FactoryRoleEnvironmentValue::Secret { .. })
          ) {
            return Err(Failure::Environment);
          }
          let schema = self.public_environment.get(name).ok_or(Failure::Environment)?;
          FlowPayload::new(schema, Value::String(value.clone())).map_err(|_| Failure::Environment)?;
        }
        FactoryRoleEnvironmentValue::Secret { .. } if ceiling.execution.environment.get(name) == Some(value) => {}
        FactoryRoleEnvironmentValue::Secret { .. } => return Err(Failure::Environment),
      }
    }
    if definition
      .skills
      .iter()
      .any(|(key, value)| ceiling.skills.get(key) != Some(value))
      || !definition.context_requirements.is_subset(&ceiling.context_requirements)
      || definition
        .execution
        .mcp_servers
        .iter()
        .any(|(key, value)| ceiling.execution.mcp_servers.get(key) != Some(value))
      || definition
        .execution
        .integrations
        .iter()
        .any(|(key, value)| ceiling.execution.integrations.get(key) != Some(value))
    {
      return Err(Failure::Dependencies);
    }
    if !definition.budget.fits_within(ceiling.budget) {
      return Err(Failure::Budget);
    }
    let effective = resolve_factory_permissions(
      &ceiling.permissions,
      &ceiling.permissions,
      &definition.permissions,
      &self.local_permissions,
    )
    .map_err(|_| Failure::Permissions)?;
    if effective != definition.permissions {
      return Err(Failure::Permissions);
    }
    Ok(())
  }
}
