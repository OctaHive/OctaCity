//! Pure, retained invocation content; storage, authorization and dispatch keep their existing owners.
use super::{ContextManifest, FactoryArtifactReference, FactoryRepositoryPath, FactoryTaskEnvelope, common::*};
use crate::{FactoryDigest, FactoryError, FactoryKey, FactoryRole, FactoryRoleProfile, ImmutableReference};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// One ordered Project rule, bound to a selected input in the task's Context Manifest.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRoleProjectRule {
  /// Exact logical input identity; provenance and byte identity live in the common manifest.
  pub source: FactoryKey,
  /// Exact rule text, kept separate from role/profile additions and never interpreted as authority.
  pub instructions: String,
}

/// Content-pinned skill tree over existing immutable Artifact references.
/// This inventory contains no host paths or read grants; transfer verifies the referenced bytes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "SkillBundleWire")]
pub struct FactoryRoleSkillBundle {
  reference: ImmutableReference,
  files: BTreeMap<FactoryRepositoryPath, FactoryArtifactReference>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillBundleWire {
  reference: ImmutableReference,
  files: BTreeMap<FactoryRepositoryPath, FactoryArtifactReference>,
}
impl TryFrom<SkillBundleWire> for FactoryRoleSkillBundle {
  type Error = FactoryError;
  fn try_from(wire: SkillBundleWire) -> Result<Self, Self::Error> {
    let bundle = Self::new(
      wire.reference.identity().clone(),
      wire.reference.version().clone(),
      wire.files,
    )?;
    if bundle.reference != wire.reference {
      return Err(invalid("role skill bundle digest"));
    }
    Ok(bundle)
  }
}
impl FactoryRoleSkillBundle {
  /// Freezes a skill's complete relative-path inventory in canonical order.
  pub fn new(
    identity: FactoryKey,
    version: FactoryKey,
    files: BTreeMap<FactoryRepositoryPath, FactoryArtifactReference>,
  ) -> Result<Self, FactoryError> {
    if files.len() > MAX_CONTEXT_MANIFEST_ENTRIES
      || !files.keys().any(|path| path.as_str() == "SKILL.md")
      || files.keys().enumerate().any(|(index, path)| {
        files
          .keys()
          .skip(index + 1)
          .any(|other| path.is_within(other) || other.is_within(path))
      })
    {
      return Err(invalid("role skill file tree"));
    }
    let total = files
      .values()
      .try_fold(0_u64, |total, file| total.checked_add(file.encoded_size()))
      .ok_or_else(|| invalid("role skill bytes"))?;
    if total > MAX_CONTEXT_MANIFEST_BYTES {
      return Err(invalid("role skill bytes"));
    }
    let digest = record_digest("octacity.factory.role-skill-bundle.v1", &files)?;
    Ok(Self {
      reference: ImmutableReference::new(identity, version, digest),
      files,
    })
  }
  /// Exact bundle identity selected by the ordinary profile definition.
  pub const fn reference(&self) -> &ImmutableReference {
    &self.reference
  }
  /// Exact retained file identities in portable path order, including supporting dependencies.
  pub const fn files(&self) -> &BTreeMap<FactoryRepositoryPath, FactoryArtifactReference> {
    &self.files
  }
}

/// Explicit frozen inputs to one reproducible role invocation; no current catalog or ambient lookup.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryRoleExecutionPackageInput {
  /// Complete selected Policy input texts in their controlling Project rule order.
  pub project_rules: Vec<FactoryRoleProjectRule>,
  /// Exact shared role content and provenance.
  pub role: FactoryRole,
  /// Exact complete profile settings, including logical secret references rather than secret bytes.
  pub profile: FactoryRoleProfile,
  /// Exact selected skill inventories; file bytes retain the ordinary Artifact owner.
  pub skills: BTreeMap<FactoryKey, FactoryRoleSkillBundle>,
  /// Existing stage task, requirements, outputs, technical pins and enclosing authority.
  pub task: FactoryTaskEnvelope,
  /// Existing explicitly selected Context, including rule provenance.
  pub context: ContextManifest,
}

/// Versioned immutable package for compilation and retention, not an execution grant.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "PackageWire")]
pub struct FactoryRoleExecutionPackage {
  schema_version: u16,
  inputs: FactoryRoleExecutionPackageInput,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageWire {
  schema_version: u16,
  inputs: FactoryRoleExecutionPackageInput,
}
impl TryFrom<PackageWire> for FactoryRoleExecutionPackage {
  type Error = FactoryError;
  fn try_from(wire: PackageWire) -> Result<Self, Self::Error> {
    require_version(wire.schema_version)?;
    Self::new(wire.inputs)
  }
}
impl FactoryRoleExecutionPackage {
  /// Assembles already selected content without reading storage or contacting a provider.
  pub fn new(inputs: FactoryRoleExecutionPackageInput) -> Result<Self, FactoryError> {
    if inputs.project_rules.len() > crate::MAX_FACTORY_ROLE_PROFILE_ENTRIES {
      return Err(invalid("role Project rule count"));
    }
    let profile = inputs.profile.definition();
    let skills = inputs.role.effective_skills(&inputs.profile)?;
    let task = &inputs.task;
    let context = &inputs.context;
    if inputs.profile.role() != inputs.role.reference()
      || inputs.role.reference().project_id != task.subject().project_id()
      || context.subject() != task.subject()
      || context.id() != task.context_manifest_id()
      || context.digest()? != task.context_manifest_digest()
      || task.plugin() != &profile.toolchain.plugin
      || task.executable() != &profile.toolchain.executable
      || task.model() != &profile.toolchain.model
      || !task.permissions().is_no_broader_than(&profile.permissions)
      || !task.budget().fits_within(profile.budget)
      || profile.context_requirements.iter().any(|required| {
        !context
          .entries()
          .iter()
          .any(|entry| entry.logical_identity() == required)
      })
      || inputs.skills.len() != skills.len()
      || inputs
        .skills
        .iter()
        .any(|(alias, bundle)| skills.get(alias) != Some(bundle.reference()))
    {
      return Err(invalid("role execution package bindings"));
    }
    let mut rule_sources = BTreeSet::new();
    let mut rule_bytes = 0;
    for rule in &inputs.project_rules {
      crate::role_profile::validate_instructions(&rule.instructions)?;
      rule_bytes += rule.instructions.len();
      let source = context
        .entries()
        .iter()
        .find(|entry| entry.logical_identity() == &rule.source && entry.source_kind() == ContextSourceKind::Policy)
        .ok_or_else(|| invalid("role Project rule source"))?;
      if !rule_sources.insert(&rule.source)
        || source.content_digest() != FactoryDigest::content_sha256(rule.instructions.as_bytes())
        || source.encoded_size() != rule.instructions.len() as u64
      {
        return Err(invalid("role Project rule bytes"));
      }
    }
    if rule_sources.len()
      != context
        .entries()
        .iter()
        .filter(|entry| entry.source_kind() == ContextSourceKind::Policy)
        .count()
    {
      return Err(invalid("role Project rule closure"));
    }
    if rule_bytes > crate::MAX_FACTORY_ROLE_INSTRUCTION_BYTES {
      return Err(invalid("role Project rule bytes"));
    }
    let package = Self {
      schema_version: FACTORY_TASK_CONTRACT_VERSION,
      inputs,
    };
    if package.canonical_bytes()?.len() > crate::MAX_FLOW_DATA_BYTES {
      return Err(invalid("role execution package bytes"));
    }
    Ok(package)
  }
  /// Borrows the original source layers and exact settings without flattening or dropping them.
  pub const fn inputs(&self) -> &FactoryRoleExecutionPackageInput {
    &self.inputs
  }
  /// Canonical retained JSON; ordered instructions stay ordered and maps use their stable key order.
  pub fn canonical_bytes(&self) -> Result<Vec<u8>, FactoryError> {
    canonical_json(self, "role execution package serialization")
  }
  /// Binds the complete package to its versioned invocation identity.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    record_digest("octacity.factory.role-execution-package.v1", self)
  }
}

// Generic diagnostics expose neither instruction content nor runtime settings.
impl std::fmt::Debug for FactoryRoleProjectRule {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("FactoryRoleProjectRule").finish_non_exhaustive()
  }
}

// Generic diagnostics expose neither instruction content nor runtime settings.
impl std::fmt::Debug for FactoryRoleExecutionPackageInput {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("FactoryRoleExecutionPackageInput")
      .finish_non_exhaustive()
  }
}

// Generic diagnostics expose neither instruction content nor runtime settings.
impl std::fmt::Debug for FactoryRoleExecutionPackage {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("FactoryRoleExecutionPackage").finish_non_exhaustive()
  }
}
