//! Trusted installed compatibility, attached to the existing Factory choice catalog.
use crate::*;
use serde_json::Value;

/// Advisory plugin-owned model suggestions; refresh does not mutate published profiles.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRoleModelCatalog {
  /// Whether the latest discovery produced runtime metadata.
  pub available: bool,
  /// Whether identifiers absent from suggestions can be entered explicitly.
  pub manual_model: bool,
  /// Bounded runtime-provided suggestions, without credentials or execution grants.
  pub models: Vec<FactoryRoleModelChoice>,
}
/// One opaque runtime model name and its plugin-defined parameter contract.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRoleModelChoice {
  /// Exact identifier passed unchanged to the installed plugin.
  pub identifier: String,
  /// Presentation label from the runtime.
  pub label: String,
  /// Existing shared shape, normalized from the plugin's supported schema.
  pub parameters: FlowDataSchema,
}

/// Installed ceiling and supported settings for one exact technical profile.
/// This declaration is read-only validation input, never dispatch or credential authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryRoleProfileCapability {
  /// Optional plugin runtime metadata. Absence preserves the exact legacy model binding.
  pub model_catalog: Option<FactoryRoleModelCatalog>,
  /// Reuses ordinary profile settings as the installed ceiling; name/instructions are authoring content.
  pub ceiling: FactoryRoleProfileDefinition,
  /// Existing closed schema for model parameters, including provider-declared thinking values.
  pub parameters: FlowDataSchema,
  /// Existing closed schema for explicit public environment names and values.
  pub public_environment: std::collections::BTreeMap<String, FlowDataSchema>,
  /// Explicit environment names required by each configured MCP; values remain in profile mappings.
  pub mcp_required_environment: std::collections::BTreeMap<FactoryKey, std::collections::BTreeSet<String>>,
  /// Qualified backend enforcement and local grants, independent of instructional text.
  pub local_permissions: LocalPermissionCeiling,
}
/// Safe incompatibility categories; no submitted values or private content.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
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
    let definition = profile.definition();
    let model = definition.model_identifier.as_deref();
    let selected = self.model_catalog.as_ref().and_then(|catalog| {
      catalog
        .models
        .iter()
        .find(|item| item.identifier == definition.runtime_model())
    });
    if model.is_some() && selected.is_none() && !self.model_catalog.as_ref().is_some_and(|catalog| catalog.manual_model)
    {
      return Err(FactoryRoleProfileIncompatibility::Toolchain);
    }
    let schema = selected.map_or(&self.parameters, |choice| &choice.parameters);
    let parameters = serde_json::to_value(&definition.model_parameters)
      .map_err(|_| FactoryRoleProfileIncompatibility::ModelParameters)?;
    FlowPayload::new(schema, parameters).map_err(|_| FactoryRoleProfileIncompatibility::ModelParameters)?;
    use FactoryRoleProfileIncompatibility as Failure;
    let ceiling = &self.ceiling;
    if definition.thinking_parameter.is_some() && definition.thinking_parameter != ceiling.thinking_parameter {
      return Err(Failure::ModelParameters);
    }
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
    if definition.execution.mcp_servers.keys().any(|alias| {
      self.mcp_required_environment.get(alias).is_some_and(|names| {
        names
          .iter()
          .any(|name| !definition.execution.environment.contains_key(name))
      })
    }) {
      return Err(Failure::Environment);
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
