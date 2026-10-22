//! Project AI-agent definitions; publication constructs values and grants no execution authority.

use octacity_server_domain::ProjectId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::{
  BudgetLimit, BuildConfigurationRef, FactoryCredentialPurpose, FactoryKey, FactoryPermissionSet, FactoryRoleProfileId,
  FactoryRoleProfileVersion, FactoryTaskToolchain, ImmutableReference,
};
use crate::{FactoryDigest, FactoryError, FactoryRoleId, FactoryRoleVersion, FactoryText};

/// Maximum exact UTF-8 instruction bytes in one role or profile.
pub const MAX_FACTORY_ROLE_INSTRUCTION_BYTES: usize = 64 * 1024;
/// Maximum encoded JSON bytes in one complete role or profile definition.
pub const MAX_FACTORY_ROLE_DEFINITION_BYTES: usize = 256 * 1024;
/// Maximum skill bundles, model parameters, or context requirements in one profile.
pub const MAX_FACTORY_ROLE_PROFILE_ENTRIES: usize = 64;

/// Editable role responsibility, independent of model and worker identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRoleDefinition {
  /// Arbitrary human-readable agent name; spelling selects no behavior.
  pub name: FactoryText,
  /// Exact shared Markdown instructions, including line breaks and trailing whitespace.
  pub instructions: String,
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
}

/// Complete editable model-task configuration, independent of any enrolled worker.
/// Installed capability compatibility and authority intersection are checked by application publication/dispatch.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRoleProfileDefinition {
  /// Arbitrary display name; default/pro names carry no privilege or execution policy.
  pub name: FactoryText,
  /// Additional exact Markdown instructions layered after the pinned shared role instructions.
  pub instructions: String,
  /// Exact installed model, harness executable, and plugin references.
  pub toolchain: FactoryTaskToolchain,
  /// Bounded scalar parameters, including any reasoning setting; supported keys/values come from installed capabilities.
  pub model_parameters: BTreeMap<FactoryKey, serde_json::Value>,
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

fn profile_digest(
  role: &FactoryRoleRef,
  definition: &FactoryRoleProfileDefinition,
) -> Result<FactoryDigest, FactoryError> {
  if definition.build_configuration.project_id() != role.project_id {
    return Err(invalid("profile Build project"));
  }
  validate_instructions(&definition.instructions)?;
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
  #[derive(Serialize)]
  struct Content<'a> {
    role: &'a FactoryRoleRef,
    definition: &'a FactoryRoleProfileDefinition,
  }
  definition_digest("octacity.factory.role-profile.v1", &Content { role, definition })
}

fn role_digest(definition: &FactoryRoleDefinition) -> Result<FactoryDigest, FactoryError> {
  validate_instructions(&definition.instructions)?;
  definition_digest("octacity.factory.role-definition.v1", definition)
}

fn validate_instructions(value: &str) -> Result<(), FactoryError> {
  if value.len() > MAX_FACTORY_ROLE_INSTRUCTION_BYTES
    || value
      .chars()
      .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
  {
    return Err(invalid("role instructions"));
  }
  Ok(())
}

fn definition_digest(domain: &str, definition: &impl Serialize) -> Result<FactoryDigest, FactoryError> {
  let bytes = serde_json::to_vec(definition).map_err(|_| invalid("role definition serialization"))?;
  if bytes.len() > MAX_FACTORY_ROLE_DEFINITION_BYTES {
    return Err(invalid("role definition bytes"));
  }
  Ok(FactoryDigest::sha256(domain, &[&bytes]))
}

fn invalid(field: &'static str) -> FactoryError {
  FactoryError::InvalidConfiguration { field }
}
