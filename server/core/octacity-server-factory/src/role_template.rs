//! Release-owned starting content. Models and privileges are explicit Project choices.
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Incomplete profile content; no runtime, model, permissions or budget defaults.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRoleProfileTemplate {
  /// Initial display label, freely editable after copying.
  pub name: FactoryText,
  /// Initial additional instructions, copied without live inheritance.
  pub instructions: FactoryInstructions,
}

/// Versioned release data, distinct from Project-owned roles and executable profiles.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRoleTemplate {
  /// Catalog identity; names never dispatch business behavior.
  pub identity: FactoryKey,
  /// Explicit release-data revision, independent of model provider versions.
  pub version: FactoryKey,
  /// Initial responsibility and presentation content.
  pub role: FactoryRoleDefinition,
  /// Required profile seeds, keyed independently of their editable display names.
  pub profiles: BTreeMap<FactoryKey, FactoryRoleProfileTemplate>,
}
impl FactoryRoleTemplate {
  /// Validates bounded starting content and computes its exact release provenance.
  pub fn reference(&self) -> Result<ImmutableReference, FactoryError> {
    if self.profiles.is_empty() || self.profiles.len() > MAX_FACTORY_ROLE_PROFILE_ENTRIES {
      return Err(crate::role_profile::invalid("role template profiles"));
    }
    crate::role_profile::role_digest(&self.role)?;
    for seed in self.profiles.values() {
      seed.instructions.validate()?;
    }
    Ok(ImmutableReference::new(
      self.identity.clone(),
      self.version.clone(),
      crate::role_profile::definition_digest("octacity.factory.role-template.v1", self)?,
    ))
  }
}

/// Reads the bundled copy-on-create catalog without creating Project definitions.
/// New release data never updates an already copied role/profile version.
pub fn factory_role_templates() -> Result<Vec<FactoryRoleTemplate>, FactoryError> {
  let templates: Vec<FactoryRoleTemplate> = serde_json::from_str(include_str!("role_templates.v1.json"))
    .map_err(|_| crate::role_profile::invalid("bundled role templates"))?;
  for template in &templates {
    template.reference()?;
  }
  Ok(templates)
}

/// A complete operator-reviewed profile, with an explicit new Project identity.
/// Start from the catalog seed's text; all execution settings must be resolved
/// explicitly. Reviewed edits to names/instructions are preserved as supplied.
#[derive(Clone, Debug)]
pub struct FactoryRoleTemplateBinding {
  /// Caller-owned stable identity, retained across publication retries.
  pub id: FactoryRoleProfileId,
  /// Complete settings accepted by the ordinary profile definition contract.
  pub definition: FactoryRoleProfileDefinition,
}

/// Independent ordinary definitions, ready for the common atomic publication port.
#[derive(Clone, Debug)]
pub struct FactoryRoleTemplateInstance {
  /// Initial Project role; no link to mutable template or catalog state.
  pub role: FactoryRole,
  /// Initial profiles, each pinned to this exact role version.
  pub profiles: BTreeMap<FactoryKey, FactoryRoleProfile>,
}
impl FactoryRoleTemplate {
  /// Copies reviewed responsibility and complete profile settings into initial
  /// Project versions. Seeds are authoring defaults, never silent overrides of
  /// reviewed content. This pure operation neither persists nor qualifies execution;
  /// publication uses the existing role/profile transaction and authorization.
  /// Every seed requires complete choices; additional custom profiles are allowed
  /// under the same shared cardinality and Project ownership bounds.
  pub fn instantiate(
    &self,
    project: octacity_server_domain::ProjectId,
    role_id: FactoryRoleId,
    mut role: FactoryRoleDefinition,
    profiles: BTreeMap<FactoryKey, FactoryRoleTemplateBinding>,
  ) -> Result<FactoryRoleTemplateInstance, FactoryError> {
    let mut ids = std::collections::BTreeSet::new();
    if profiles.len() > MAX_FACTORY_ROLE_PROFILE_ENTRIES
      || self.profiles.keys().any(|key| !profiles.contains_key(key))
      || profiles.values().any(|binding| !ids.insert(binding.id))
    {
      return Err(crate::role_profile::invalid("role template choices"));
    }
    let reference = self.reference()?;
    role.template = Some(reference.clone());
    let role = FactoryRole::publish(role_id, project, role)?;
    let profiles = profiles
      .into_iter()
      .map(|(key, mut binding)| {
        binding.definition.template = Some(reference.clone());
        Ok((
          key,
          FactoryRoleProfile::publish(binding.id, project, &role, binding.definition)?,
        ))
      })
      .collect::<Result<_, FactoryError>>()?;
    Ok(FactoryRoleTemplateInstance { role, profiles })
  }
}
