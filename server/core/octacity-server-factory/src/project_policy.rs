//! Project-owned policy contents, independent of installation and execution authority.
//!
//! Publications bind the Project, stable identity, positive revision and complete definition
//! in a domain-separated digest. Deserialization verifies that binding and the publication
//! size bound; it does not establish provenance or authorization. Management and store owners
//! must authorize publication and atomically enforce replacement preconditions.
//! Permission semantics and canonical ordering remain owned by [`FactoryPermissionSet`].

use octacity_server_domain::ProjectId;
use serde::{Deserialize, Serialize};

use crate::{
  FactoryAccessProfileId, FactoryAccessProfileVersion, FactoryDigest, FactoryError, FactoryLaunchPolicyId,
  FactoryLaunchPolicyVersion, FactoryPermissionSet, FactoryText,
};

/// Maximum canonical JSON bytes hashed for one complete Project policy revision.
pub const MAX_FACTORY_POLICY_DEFINITION_BYTES: usize = 256 * 1024;

/// Trusted admission path, assigned by the server rather than by Work metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FactoryAdmissionOrigin {
  /// Explicit operator Work or Issue execution, including externally linked Issues.
  Manual,
  /// Admission requested by an independently enabled and authorized Source.
  AutomaticSource,
}

/// Editable launch rules; the name grants no behavior and both modes may be denied.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryLaunchPolicyDefinition {
  /// Bounded display name, independent of the admission decision.
  pub name: FactoryText,
  /// Whether an otherwise authorized explicit operator admission is permitted.
  pub allow_manual: bool,
  /// Whether an otherwise authorized automatic Source admission is permitted.
  pub allow_automatic_source: bool,
}

impl FactoryLaunchPolicyDefinition {
  /// Evaluates only the origin rule; this neither authorizes Work nor enables a Source.
  #[must_use]
  pub const fn allows(&self, origin: FactoryAdmissionOrigin) -> bool {
    match origin {
      FactoryAdmissionOrigin::Manual => self.allow_manual,
      FactoryAdmissionOrigin::AutomaticSource => self.allow_automatic_source,
    }
  }
}

/// Exact Project-owned launch policy publication, never a live default lookup.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryLaunchPolicyRef {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Stable identity shared by replacements.
  pub id: FactoryLaunchPolicyId,
  /// Exact positive revision.
  pub version: FactoryLaunchPolicyVersion,
  /// Verified content identity including Project, stable identity and revision.
  pub digest: FactoryDigest,
}

/// Immutable launch-policy contents; publication itself authorizes no execution.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "RetainedLaunchPolicy")]
pub struct FactoryLaunchPolicy {
  reference: FactoryLaunchPolicyRef,
  definition: FactoryLaunchPolicyDefinition,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedLaunchPolicy {
  reference: FactoryLaunchPolicyRef,
  definition: FactoryLaunchPolicyDefinition,
}

impl TryFrom<RetainedLaunchPolicy> for FactoryLaunchPolicy {
  type Error = FactoryError;

  fn try_from(value: RetainedLaunchPolicy) -> Result<Self, Self::Error> {
    let expected = Self::new(
      value.reference.id,
      value.reference.project_id,
      value.reference.version,
      value.definition,
    )?;
    if expected.reference.digest != value.reference.digest {
      return Err(invalid("launch policy digest"));
    }
    Ok(expected)
  }
}

impl FactoryLaunchPolicy {
  /// Creates version one; authorization and atomic publication belong to management/store owners.
  pub fn publish(
    id: FactoryLaunchPolicyId,
    project_id: ProjectId,
    definition: FactoryLaunchPolicyDefinition,
  ) -> Result<Self, FactoryError> {
    Self::new(id, project_id, FactoryLaunchPolicyVersion::INITIAL, definition)
  }

  /// Reconstructs one bounded immutable revision and computes its content digest.
  /// Store publication must independently enforce the current-head precondition.
  pub fn new(
    id: FactoryLaunchPolicyId,
    project_id: ProjectId,
    version: FactoryLaunchPolicyVersion,
    definition: FactoryLaunchPolicyDefinition,
  ) -> Result<Self, FactoryError> {
    let digest = policy_digest(
      "octacity.factory.launch-policy.v1",
      &(project_id, id, version, &definition),
    )?;
    Ok(Self {
      reference: FactoryLaunchPolicyRef {
        project_id,
        id,
        version,
        digest,
      },
      definition,
    })
  }

  /// Creates only the next immutable version under the same Project and identity.
  pub fn replace(
    &self,
    version: FactoryLaunchPolicyVersion,
    definition: FactoryLaunchPolicyDefinition,
  ) -> Result<Self, FactoryError> {
    if self.reference.version.get().checked_add(1) != Some(version.get()) {
      return Err(invalid("launch policy replacement version"));
    }
    Self::new(self.reference.id, self.reference.project_id, version, definition)
  }

  /// Borrows the exact version and verified content identity.
  #[must_use]
  pub const fn reference(&self) -> &FactoryLaunchPolicyRef {
    &self.reference
  }

  /// Borrows the retained rules without evaluating current authorization or Source state.
  #[must_use]
  pub const fn definition(&self) -> &FactoryLaunchPolicyDefinition {
    &self.definition
  }
}

/// Editable, bounded permission ceiling; selecting it attaches no input, tool or credential.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryAccessProfileDefinition {
  /// Bounded display name; names do not select execution behavior.
  pub name: FactoryText,
  /// Existing typed permission declarations, narrowed again by independent execution owners.
  pub permissions: FactoryPermissionSet,
}

impl FactoryAccessProfileDefinition {
  /// Rejects a broader request without clamping it; installation/backend qualification is separate.
  pub fn validate_within(&self, ceiling: &FactoryPermissionSet) -> Result<(), FactoryError> {
    if !self.permissions.is_no_broader_than(ceiling) {
      return Err(invalid("access profile permissions"));
    }
    Ok(())
  }
}

/// Exact Project-owned access-profile publication, independent of Runtime/model profiles.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryAccessProfileRef {
  /// Owning Project.
  pub project_id: ProjectId,
  /// Stable identity shared by replacements.
  pub id: FactoryAccessProfileId,
  /// Exact positive revision.
  pub version: FactoryAccessProfileVersion,
  /// Verified content identity including Project, stable identity and revision.
  pub digest: FactoryDigest,
}

/// Immutable access ceiling; publication itself grants no execution authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "RetainedAccessProfile")]
pub struct FactoryAccessProfile {
  reference: FactoryAccessProfileRef,
  definition: FactoryAccessProfileDefinition,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedAccessProfile {
  reference: FactoryAccessProfileRef,
  definition: FactoryAccessProfileDefinition,
}

impl TryFrom<RetainedAccessProfile> for FactoryAccessProfile {
  type Error = FactoryError;

  fn try_from(value: RetainedAccessProfile) -> Result<Self, Self::Error> {
    let expected = Self::new(
      value.reference.id,
      value.reference.project_id,
      value.reference.version,
      value.definition,
    )?;
    if expected.reference.digest != value.reference.digest {
      return Err(invalid("access profile digest"));
    }
    Ok(expected)
  }
}

impl FactoryAccessProfile {
  /// Creates version one from bounded declarations; installed authority is checked independently.
  pub fn publish(
    id: FactoryAccessProfileId,
    project_id: ProjectId,
    definition: FactoryAccessProfileDefinition,
  ) -> Result<Self, FactoryError> {
    Self::new(id, project_id, FactoryAccessProfileVersion::INITIAL, definition)
  }

  /// Reconstructs one bounded immutable revision and computes its content digest.
  /// Store publication must independently enforce the current-head precondition.
  pub fn new(
    id: FactoryAccessProfileId,
    project_id: ProjectId,
    version: FactoryAccessProfileVersion,
    definition: FactoryAccessProfileDefinition,
  ) -> Result<Self, FactoryError> {
    let digest = policy_digest(
      "octacity.factory.access-profile.v1",
      &(project_id, id, version, &definition),
    )?;
    Ok(Self {
      reference: FactoryAccessProfileRef {
        project_id,
        id,
        version,
        digest,
      },
      definition,
    })
  }

  /// Creates only the next immutable version under the same Project and identity.
  pub fn replace(
    &self,
    version: FactoryAccessProfileVersion,
    definition: FactoryAccessProfileDefinition,
  ) -> Result<Self, FactoryError> {
    if self.reference.version.get().checked_add(1) != Some(version.get()) {
      return Err(invalid("access profile replacement version"));
    }
    Self::new(self.reference.id, self.reference.project_id, version, definition)
  }

  /// Borrows the exact version and verified content identity.
  #[must_use]
  pub const fn reference(&self) -> &FactoryAccessProfileRef {
    &self.reference
  }

  /// Borrows the original permission ceiling without resolving newer defaults.
  #[must_use]
  pub const fn definition(&self) -> &FactoryAccessProfileDefinition {
    &self.definition
  }
}

fn policy_digest(domain: &str, content: &impl Serialize) -> Result<FactoryDigest, FactoryError> {
  let bytes = serde_json::to_vec(content).map_err(|_| invalid("project policy serialization"))?;
  if bytes.len() > MAX_FACTORY_POLICY_DEFINITION_BYTES {
    return Err(invalid("project policy definition bytes"));
  }
  Ok(FactoryDigest::sha256(domain, &[&bytes]))
}

const fn invalid(field: &'static str) -> FactoryError {
  FactoryError::InvalidConfiguration { field }
}
