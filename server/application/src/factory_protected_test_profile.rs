//! Canonical installed protected-test policy bound to a command identity.
use crate::{ApplicationError, FactoryCommandBuildProfile, FactoryProtectedTestFileOutput, FactoryProtectedTestPolicy};
use octacity_server_factory::{EvidenceOutputKind, EvidenceRequirement, FactoryDigest};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Finite private output mapping and coverage selected by a trusted installation.
#[derive(Clone, Deserialize, Serialize)]
#[serde(try_from = "AuthoringPolicyWire")]
pub struct FactoryProtectedTestAuthoringPolicy {
  policy: FactoryProtectedTestPolicy,
  inventory: EvidenceRequirement,
  files: Vec<FactoryProtectedTestFileOutput>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthoringPolicyWire {
  policy: FactoryProtectedTestPolicy,
  inventory: EvidenceRequirement,
  files: Vec<FactoryProtectedTestFileOutput>,
}
impl TryFrom<AuthoringPolicyWire> for FactoryProtectedTestAuthoringPolicy {
  type Error = ApplicationError;
  fn try_from(value: AuthoringPolicyWire) -> Result<Self, Self::Error> {
    Self::new(value.policy, value.inventory, value.files)
  }
}
impl FactoryProtectedTestAuthoringPolicy {
  /// Validates complete class coverage, unique private outputs and bounded capacity.
  pub fn new(
    policy: FactoryProtectedTestPolicy,
    inventory: EvidenceRequirement,
    mut files: Vec<FactoryProtectedTestFileOutput>,
  ) -> Result<Self, ApplicationError> {
    files.sort_by(|left, right| left.path.cmp(&right.path));
    let classes = files.iter().map(|file| &file.class).collect::<BTreeSet<_>>();
    let outputs = files.iter().map(|file| &file.output).collect::<BTreeSet<_>>();
    if inventory.output_kind() != EvidenceOutputKind::Artifact
      || files.is_empty()
      || files.len() > usize::from(policy.coverage().max_files())
      // Publication lists also contain the private inventory and public handoff.
      || files.len() + 2 > usize::from(octacity_server_store::MAX_ARTIFACT_PAGE_SIZE)
      || files.windows(2).any(|pair| pair[0].path == pair[1].path)
      || outputs.len() != files.len()
      || outputs.contains(inventory.kind())
      || classes != policy.coverage().classes().iter().collect()
    {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      policy,
      inventory,
      files,
    })
  }
  /// Returns the canonical policy content pin used by the command identity.
  pub fn digest(&self) -> Result<FactoryDigest, ApplicationError> {
    serde_json::to_vec(self)
      .map(|bytes| FactoryDigest::content_sha256(&bytes))
      .map_err(|_| ApplicationError::invalid())
  }
  /// Returns immutable coverage, retention and input-capacity limits.
  pub const fn policy(&self) -> &FactoryProtectedTestPolicy {
    &self.policy
  }
  /// Returns the exact private inventory contract.
  pub const fn inventory(&self) -> &EvidenceRequirement {
    &self.inventory
  }
  /// Returns canonical private file mappings, without future Artifact identities.
  pub fn files(&self) -> &[FactoryProtectedTestFileOutput] {
    &self.files
  }
}

/// Exact command and canonical author policy; installation qualification remains separate.
#[derive(Clone)]
pub struct FactoryProtectedTestAuthoringProfile {
  command: FactoryCommandBuildProfile,
  policy: FactoryProtectedTestAuthoringPolicy,
}
impl FactoryProtectedTestAuthoringProfile {
  /// Rejects changes to private file policy under an existing command identity.
  /// This constructor grants no execution authority or writable Source access.
  pub fn new(
    command: FactoryCommandBuildProfile,
    policy: FactoryProtectedTestAuthoringPolicy,
  ) -> Result<Self, ApplicationError> {
    let public = command
      .outputs
      .iter()
      .filter(|output| output.output_kind() == EvidenceOutputKind::Report)
      .collect::<Vec<_>>();
    if command.identity.digest() != policy.digest()?
      || command.outputs.len() != 2
      || public.len() != 1
      || !command.outputs.contains(policy.inventory())
      || policy.inventory().tool() != &command.executor
      || policy.inventory().plugin() != &command.plugin
      || policy.files().iter().any(|file| {
        file.output == *public[0].kind()
          || !command
            .project_permissions
            .outputs()
            .kinds()
            .any(|kind| kind == &file.output)
      })
    {
      return Err(ApplicationError::invalid());
    }
    Ok(Self { command, policy })
  }
  /// Returns the existing immutable command compilation profile.
  pub const fn command(&self) -> &FactoryCommandBuildProfile {
    &self.command
  }
  /// Returns the installed coverage, retention and signed-input limits.
  pub const fn policy(&self) -> &FactoryProtectedTestPolicy {
    self.policy.policy()
  }
  /// Returns the exact private inventory contract.
  pub const fn inventory(&self) -> &EvidenceRequirement {
    self.policy.inventory()
  }
  /// Returns finite private file names, classes and paths from the same pinned policy.
  pub fn files(&self) -> &[FactoryProtectedTestFileOutput] {
    self.policy.files()
  }
}
