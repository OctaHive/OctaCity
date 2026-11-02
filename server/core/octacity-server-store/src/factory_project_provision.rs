//! Explicit lifecycle inputs; reads and catalog order never provision authority.
use crate::{FactoryProjectPolicyDefaults, IdempotencyKey, MutationDisposition};
use octacity_server_domain::{ProjectId, Timestamp};
use octacity_server_factory::{FactoryAccessProfileRef, FactoryLaunchPolicyRef, ImmutableReference};
use serde::{Deserialize, Serialize};

/// Exact legacy kind: admission markers and permission ceilings are distinct bindings.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", content = "reference", rename_all = "snake_case", deny_unknown_fields)]
pub enum FactoryLegacyPolicyReference {
  /// Original admission marker.
  Launch(ImmutableReference),
  /// Original access ceiling.
  Access(ImmutableReference),
}
impl FactoryLegacyPolicyReference {
  /// Exact technical category and immutable reference; aliases confer no authority.
  #[must_use]
  pub const fn installed_choice(&self) -> (octacity_server_factory::FactoryChoiceKind, &ImmutableReference) {
    match self {
      Self::Launch(reference) => (octacity_server_factory::FactoryChoiceKind::AdmissionPolicy, reference),
      Self::Access(reference) => (octacity_server_factory::FactoryChoiceKind::PermissionCeiling, reference),
    }
  }
}
/// Retained compatibility provenance; does not rewrite a Configuration or follow a current head.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryLegacyPolicyMapping {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Complete original kind, identity, version and digest.
  pub legacy: FactoryLegacyPolicyReference,
  /// Exact imported Project policy revision, never a default lookup.
  pub target: crate::FactoryProjectPolicyRef,
}
/// Unresolved exact input retained in the receipt without creating a compatibility binding.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryLegacyPolicyUnresolved {
  /// Original complete reference.
  pub legacy: FactoryLegacyPolicyReference,
  /// Why its contents could not be imported safely.
  pub reason: FactoryPolicyProvisionPrerequisite,
}
/// Original operator intent; declarations come from the trusted capability owner.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryProjectProvisionIntent {
  /// Existing owning Project.
  pub project_id: ProjectId,
  /// Explicit reviewed installed default; None cannot select from inventory order.
  pub designated_access: Option<ImmutableReference>,
  /// Exact bindings selected for a bounded backfill operation.
  pub legacy_references: Vec<FactoryLegacyPolicyReference>,
}
/// Maximum exact legacy references reviewed in one atomic lifecycle command.
pub const MAX_FACTORY_LEGACY_POLICY_REFERENCES: usize = 256;

impl FactoryProjectProvisionIntent {
  /// Bounds the complete backfill and rejects duplicate typed references before encoding intent.
  pub fn validate(&self) -> Result<(), crate::StoreError> {
    let mut references = std::collections::BTreeSet::new();
    if self.legacy_references.len() > MAX_FACTORY_LEGACY_POLICY_REFERENCES
      || self
        .legacy_references
        .iter()
        .any(|reference| !references.insert(reference))
    {
      return Err(crate::StoreError::invalid(
        crate::StoreOperation::ProvisionFactoryProjectPolicies,
        crate::StoreInputError::InvalidFactoryProjectPolicy,
      ));
    }
    Ok(())
  }
}
/// Why exact installed contents could not be copied during initialization or backfill.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryPolicyProvisionPrerequisite {
  /// No reviewed installed default was explicitly designated.
  NoDesignatedAccessDefault,
  /// The complete typed reference is absent from the Project's current installed inventory.
  ReferenceNotInstalled,
  /// Exact installed access declarations are unavailable.
  MissingPermissionDeclarations,
  /// Aliases for the same reference disagree about its permission contents.
  AmbiguousPermissionDeclarations,
}

/// Copies only unambiguous deployment declarations for an exact installed ceiling.
/// This initializes editable policy data, not runtime/Agent grants or backend enforcement.
pub fn factory_policy_access_declaration(
  choices: &octacity_server_factory::FactoryConfigurationChoices,
  reference: &ImmutableReference,
) -> Result<octacity_server_factory::FactoryPermissionSet, FactoryPolicyProvisionPrerequisite> {
  use octacity_server_factory::FactoryChoiceKind;
  let mut permissions = None;
  for entry in choices
    .entries()
    .references
    .into_iter()
    .filter(|entry| entry.kind == FactoryChoiceKind::PermissionCeiling && entry.reference == *reference)
  {
    let declared = choices
      .choice_metadata(entry.kind, &entry.alias)
      .and_then(|metadata| metadata.permission_declarations.as_ref())
      .ok_or(FactoryPolicyProvisionPrerequisite::MissingPermissionDeclarations)?;
    if permissions.as_ref().is_some_and(|prior| prior != declared) {
      return Err(FactoryPolicyProvisionPrerequisite::AmbiguousPermissionDeclarations);
    }
    permissions = Some(declared.clone());
  }
  permissions.ok_or(FactoryPolicyProvisionPrerequisite::ReferenceNotInstalled)
}
/// Immutable initial provenance, independent of later policy/default edits.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryProjectPolicyProvisioning {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Ordinary editable Standard launch policy's initial version.
  pub standard_launch: FactoryLaunchPolicyRef,
  /// Ordinary editable Standard access profile's initial version.
  pub standard_access: FactoryAccessProfileRef,
  /// Exact installation reference verified at initialization, if any.
  pub access_origin: Option<ImmutableReference>,
  /// Initial prerequisite only; reprovisioning must not overwrite subsequent edits.
  pub access_prerequisite: Option<FactoryPolicyProvisionPrerequisite>,
}
/// Authorized explicit initialization/backfill command with scoped replay.
#[derive(Clone, Debug)]
pub struct ProvisionFactoryProjectPolicies {
  /// Original reviewed intent, excluding mutable installed declarations.
  pub intent: FactoryProjectProvisionIntent,
  /// Scoped caller key.
  pub idempotency_key: IdempotencyKey,
  /// Trusted first acceptance time.
  pub provisioned_at: Timestamp,
}
/// Original receipt; replay does not resolve current installation or defaults.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionFactoryProjectPoliciesOutcome {
  /// Applied or historical replay.
  pub disposition: MutationDisposition,
  /// Whether this command created initialization provenance.
  pub created: bool,
  /// Original identities reused across independent invocations.
  pub provisioning: FactoryProjectPolicyProvisioning,
  /// Defaults retained at acceptance; operator selections take precedence.
  pub defaults: FactoryProjectPolicyDefaults,
  /// Exact resolved bindings, in reviewed request order.
  pub mappings: Vec<FactoryLegacyPolicyMapping>,
  /// Missing bindings; callers must block their use rather than select a default.
  pub unresolved: Vec<FactoryLegacyPolicyUnresolved>,
  /// Original acceptance time.
  pub provisioned_at: Timestamp,
}
