//! Project AI-agent definitions; publication constructs values and grants no execution authority.

use octacity_server_domain::ProjectId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::{
  BudgetLimit, BuildConfigurationRef, FactoryCredentialPurpose, FactoryKey, FactoryPermissionSet, FactoryRoleProfileId,
  FactoryRoleProfileVersion, FactoryTaskToolchain, ImmutableReference,
};
use crate::{FactoryDigest, FactoryError, FactoryRoleId, FactoryRoleVersion, FactoryText};

/// Maximum combined exact UTF-8 bytes across instructions in one role or profile.
pub const MAX_FACTORY_ROLE_INSTRUCTION_BYTES: usize = 64 * 1024;
/// Maximum encoded JSON bytes in one complete role or profile definition.
pub const MAX_FACTORY_ROLE_DEFINITION_BYTES: usize = 256 * 1024;
/// Maximum instruction entries or selections in each role/profile collection.
pub const MAX_FACTORY_ROLE_PROFILE_ENTRIES: usize = 64;

/// Editable role responsibility, independent of model and worker identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRoleDefinition {
  /// Exact release template used for this copy; provenance only, never a live parent.
  /// Absent on custom and retained legacy definitions.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub template: Option<ImmutableReference>,
  /// Arbitrary human-readable agent name; spelling selects no behavior.
  pub name: FactoryText,
  /// Exact ordered shared Markdown entries; legacy text remains one entry.
  pub instructions: crate::FactoryInstructions,
  /// Shared exact skill versions used by every profile pinned to this agent version.
  #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
  pub skills: BTreeMap<FactoryKey, ImmutableReference>,
  /// Console icon key; absent selects the neutral presentation default.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub icon: Option<FactoryKey>,
  /// Uploaded presentation image, independent of execution settings and permissions.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub avatar: Option<crate::FactoryRoleAvatar>,
  /// Bounded descriptive text, separate from execution instructions.
  #[serde(default, skip_serializing_if = "String::is_empty")]
  pub description: String,
  /// Agent-wide positive execution ceiling; absent preserves inherited legacy capacity.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub concurrency_limit: Option<std::num::NonZeroU32>,
}

/// Project-scoped exact role version, retained by each profile independently.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRoleRef {
  /// Owning Project, checked when a profile binds this role.
  pub project_id: ProjectId,
  /// Stable identity across replacements.
  pub id: FactoryRoleId,
  /// Exact positive publication version.
  pub version: FactoryRoleVersion,
  /// Canonical role content identity.
  pub digest: FactoryDigest,
}

/// Immutable published role. Replacement returns a new value and retains this snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "RetainedRole")]
pub struct FactoryRole {
  reference: FactoryRoleRef,
  definition: FactoryRoleDefinition,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedRole {
  reference: FactoryRoleRef,
  definition: FactoryRoleDefinition,
}

impl TryFrom<RetainedRole> for FactoryRole {
  type Error = FactoryError;
  fn try_from(value: RetainedRole) -> Result<Self, Self::Error> {
    let digest = role_digest(&value.definition)?;
    if value.reference.digest != digest {
      return Err(invalid("role digest"));
    }
    Ok(Self {
      reference: value.reference,
      definition: value.definition,
    })
  }
}

impl FactoryRole {
  /// Constructs the initial definition; authorization and atomic current-pointer updates belong to the store.
  pub fn publish(
    id: FactoryRoleId,
    project_id: ProjectId,
    definition: FactoryRoleDefinition,
  ) -> Result<Self, FactoryError> {
    let digest = role_digest(&definition)?;
    Ok(Self {
      reference: FactoryRoleRef {
        id,
        project_id,
        version: FactoryRoleVersion::INITIAL,
        digest,
      },
      definition,
    })
  }

  /// Constructs only the next version under the same Project and stable identity.
  pub fn replace(&self, version: FactoryRoleVersion, definition: FactoryRoleDefinition) -> Result<Self, FactoryError> {
    if self.reference.version.get().checked_add(1) != Some(version.get()) {
      return Err(invalid("role replacement version"));
    }
    let mut next = Self::publish(self.reference.id, self.reference.project_id, definition)?;
    next.reference.version = version;
    Ok(next)
  }

  /// Borrows this immutable publication's Project, identity, version and content digest.
  #[must_use]
  pub const fn reference(&self) -> &FactoryRoleRef {
    &self.reference
  }

  /// Borrows exact role content without resolving current catalog values.
  #[must_use]
  pub const fn definition(&self) -> &FactoryRoleDefinition {
    &self.definition
  }

  /// Resolves shared skills and retained legacy profile additions without silent overrides.
  pub fn effective_skills(
    &self,
    profile: &FactoryRoleProfile,
  ) -> Result<BTreeMap<FactoryKey, ImmutableReference>, FactoryError> {
    if profile.role() != self.reference() {
      return Err(invalid("role skill owner"));
    }
    let mut skills = self.definition.skills.clone();
    for (alias, reference) in &profile.definition().skills {
      if skills.get(alias).is_some_and(|shared| shared != reference) {
        return Err(invalid("conflicting role skill"));
      }
      skills.insert(alias.clone(), reference.clone());
    }
    if skills.len() > MAX_FACTORY_ROLE_PROFILE_ENTRIES {
      return Err(invalid("role skill count"));
    }
    Ok(skills)
  }
}

/// Complete editable model-task configuration, independent of any enrolled worker.
/// Publication checks bounded content; installed compatibility and authority qualification precede execution.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRoleProfileDefinition {
  /// Exact release template used for this copy; provenance only, never a live parent.
  /// Absent on custom and retained legacy definitions.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub template: Option<ImmutableReference>,
  /// Arbitrary display name; default/pro names carry no privilege or execution policy.
  pub name: FactoryText,
  /// Ordered additional Markdown entries layered after the pinned shared role instructions.
  pub instructions: crate::FactoryInstructions,
  /// Exact installed model, harness executable, and plugin references.
  pub toolchain: FactoryTaskToolchain,
  /// Explicit environment/argv, installed MCP and managed integration selections.
  #[serde(default, skip_serializing_if = "crate::FactoryRoleProfileExecution::is_empty")]
  pub execution: crate::FactoryRoleProfileExecution,
  /// Bounded scalar parameters, including any reasoning setting; supported keys/values come from installed capabilities.
  pub model_parameters: BTreeMap<FactoryKey, serde_json::Value>,
  /// Installed, safe reasoning-setting key in `model_parameters`; never a copied value.
  /// Omitted on legacy profiles, which publish no inferred reasoning metadata.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub thinking_parameter: Option<crate::FactoryKey>,
  /// Named exact immutable skill bundles, including their retained supporting-file closure.
  pub skills: BTreeMap<FactoryKey, ImmutableReference>,
  /// Named requirements satisfied only by authorized stage Context projections.
  pub context_requirements: BTreeSet<FactoryKey>,
  /// Exact compatible ordinary Build configuration in the same Project.
  pub build_configuration: BuildConfigurationRef,
  /// Logical purpose only; no credential bytes are part of this definition.
  pub credential_purpose: FactoryCredentialPurpose,
  /// Existing deny-by-default tool and resource ceiling, intersected with enclosing authority before execution.
  pub permissions: FactoryPermissionSet,
  /// Existing finite execution and usage ceiling.
  pub budget: BudgetLimit,
}

/// Exact immutable profile identity used by later selection and dispatch.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRoleProfileRef {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Stable identity across profile replacements.
  pub id: FactoryRoleProfileId,
  /// Exact positive profile version.
  pub version: FactoryRoleProfileVersion,
  /// Canonical content identity, including the exact shared role reference.
  pub digest: FactoryDigest,
}

/// One immutable profile snapshot pinned to an exact role version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "RetainedProfile")]
pub struct FactoryRoleProfile {
  reference: FactoryRoleProfileRef,
  role: FactoryRoleRef,
  definition: FactoryRoleProfileDefinition,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedProfile {
  reference: FactoryRoleProfileRef,
  role: FactoryRoleRef,
  definition: FactoryRoleProfileDefinition,
}

impl TryFrom<RetainedProfile> for FactoryRoleProfile {
  type Error = FactoryError;
  fn try_from(value: RetainedProfile) -> Result<Self, Self::Error> {
    if value.role.project_id != value.reference.project_id
      || value.reference.digest != profile_digest(&value.role, &value.definition)?
    {
      return Err(invalid("profile reference"));
    }
    Ok(Self {
      reference: value.reference,
      role: value.role,
      definition: value.definition,
    })
  }
}

impl FactoryRoleProfile {
  /// Publishes a definition under an already validated role; this does not qualify or execute its capabilities.
  pub fn publish(
    id: FactoryRoleProfileId,
    project_id: ProjectId,
    role: &FactoryRole,
    definition: FactoryRoleProfileDefinition,
  ) -> Result<Self, FactoryError> {
    if role.reference.project_id != project_id {
      return Err(invalid("profile role project"));
    }
    let digest = profile_digest(role.reference(), &definition)?;
    Ok(Self {
      reference: FactoryRoleProfileRef {
        id,
        project_id,
        version: FactoryRoleProfileVersion::INITIAL,
        digest,
      },
      role: role.reference().clone(),
      definition,
    })
  }

  /// Replaces only the next profile version under its original role, explicitly adopting the supplied role version.
  pub fn replace(
    &self,
    version: FactoryRoleProfileVersion,
    role: &FactoryRole,
    definition: FactoryRoleProfileDefinition,
  ) -> Result<Self, FactoryError> {
    if self.reference.version.get().checked_add(1) != Some(version.get()) || role.reference.id != self.role.id {
      return Err(invalid("profile replacement version or role"));
    }
    let mut next = Self::publish(self.reference.id, self.reference.project_id, role, definition)?;
    next.reference.version = version;
    Ok(next)
  }

  /// Borrows the exact immutable profile reference.
  #[must_use]
  pub const fn reference(&self) -> &FactoryRoleProfileRef {
    &self.reference
  }
  /// Borrows the exact shared role version, unaffected by later role replacements.
  #[must_use]
  pub const fn role(&self) -> &FactoryRoleRef {
    &self.role
  }
  /// Borrows this profile's frozen content and settings.
  #[must_use]
  pub const fn definition(&self) -> &FactoryRoleProfileDefinition {
    &self.definition
  }
}

impl FactoryRoleProfileDefinition {
  /// Validates the common bounded definition contract for one owning Project.
  /// Installed declarations and published profiles use these same shape rules.
  pub fn validate(&self, project: ProjectId) -> Result<(), FactoryError> {
    let definition = self;
    if definition.build_configuration.project_id() != project {
      return Err(invalid("profile Build project"));
    }
    definition.instructions.validate()?;
    definition.execution.validate(project)?;
    if definition.skills.len() > MAX_FACTORY_ROLE_PROFILE_ENTRIES
      || definition.model_parameters.len() > MAX_FACTORY_ROLE_PROFILE_ENTRIES
      || definition.context_requirements.len() > MAX_FACTORY_ROLE_PROFILE_ENTRIES
    {
      return Err(invalid("profile entries"));
    }
    if definition.model_parameters.values().any(|value| {
      !matches!(
        value,
        serde_json::Value::String(_) | serde_json::Value::Bool(_) | serde_json::Value::Number(_)
      )
    }) {
      return Err(invalid("profile model parameters"));
    }
    if definition
      .thinking_parameter
      .as_ref()
      .is_some_and(|key| !definition.model_parameters.contains_key(key))
    {
      return Err(invalid("profile thinking parameter"));
    }
    definition_digest("octacity.factory.role-profile-definition.v1", self)?;
    Ok(())
  }
}

fn profile_digest(
  role: &FactoryRoleRef,
  definition: &FactoryRoleProfileDefinition,
) -> Result<FactoryDigest, FactoryError> {
  definition.validate(role.project_id)?;
  #[derive(Serialize)]
  struct Content<'a> {
    role: &'a FactoryRoleRef,
    definition: &'a FactoryRoleProfileDefinition,
  }
  definition_digest("octacity.factory.role-profile.v1", &Content { role, definition })
}

pub(super) fn role_digest(definition: &FactoryRoleDefinition) -> Result<FactoryDigest, FactoryError> {
  if definition.skills.len() > MAX_FACTORY_ROLE_PROFILE_ENTRIES {
    return Err(invalid("role skill count"));
  }
  validate_text(
    &definition.description,
    crate::MAX_FACTORY_TEXT_BYTES,
    "role description",
  )?;
  definition.instructions.validate()?;
  definition_digest("octacity.factory.role-definition.v1", definition)
}

pub(super) fn validate_instructions(value: &str) -> Result<(), FactoryError> {
  validate_text(value, MAX_FACTORY_ROLE_INSTRUCTION_BYTES, "role instructions")
}
fn validate_text(value: &str, max_bytes: usize, field: &'static str) -> Result<(), FactoryError> {
  if value.len() > max_bytes
    || value
      .chars()
      .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
  {
    return Err(invalid(field));
  }
  Ok(())
}

pub(super) fn definition_digest(domain: &str, definition: &impl Serialize) -> Result<FactoryDigest, FactoryError> {
  let bytes = serde_json::to_vec(definition).map_err(|_| invalid("role definition serialization"))?;
  if bytes.len() > MAX_FACTORY_ROLE_DEFINITION_BYTES {
    return Err(invalid("role definition bytes"));
  }
  Ok(FactoryDigest::sha256(domain, &[&bytes]))
}

pub(super) fn invalid(field: &'static str) -> FactoryError {
  FactoryError::InvalidConfiguration { field }
}
