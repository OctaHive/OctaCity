//! Immutable protected acceptance inputs, independent of logical Flow nodes.
use crate::{
  EvidenceProducer, FactoryArtifactReference, FactoryDigest, FactoryError, FactoryKey, FactoryRepositoryPath,
  FactoryTaskSubject,
};
use octacity_server_domain::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Shared configured coverage and signed-input capacity for protected source files.
/// Reserved inputs include inventory, candidate and all compiler-owned documents;
/// their count is selected by the pinned execution profile, never a logical stage.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "CoverageWire")]
pub struct ProtectedTestCoverage {
  classes: Vec<FactoryKey>,
  max_files: u16,
  max_bytes: u64,
  reserved_inputs: u16,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CoverageWire {
  classes: Vec<FactoryKey>,
  max_files: u16,
  max_bytes: u64,
  reserved_inputs: u16,
}
impl TryFrom<CoverageWire> for ProtectedTestCoverage {
  type Error = FactoryError;
  fn try_from(w: CoverageWire) -> Result<Self, Self::Error> {
    Self::new(w.classes, w.max_files, w.max_bytes, w.reserved_inputs)
  }
}
impl ProtectedTestCoverage {
  /// Validates declared classes and reserves capacity before authoring can begin.
  pub fn new(
    mut classes: Vec<FactoryKey>,
    max_files: u16,
    max_bytes: u64,
    reserved_inputs: u16,
  ) -> Result<Self, FactoryError> {
    classes.sort();
    if classes.is_empty()
      || classes.len() > crate::MAX_TASK_ARTIFACTS
      || classes.windows(2).any(|p| p[0] == p[1])
      || usize::from(max_files) < classes.len()
      || reserved_inputs == 0
      || usize::from(max_files) + usize::from(reserved_inputs) > crate::MAX_CONTEXT_MANIFEST_ENTRIES
      || max_bytes == 0
      || max_bytes > crate::MAX_CONTEXT_MANIFEST_BYTES
    {
      return Err(invalid());
    }
    Ok(Self {
      classes,
      max_files,
      max_bytes,
      reserved_inputs,
    })
  }
  /// Returns arbitrary required classes in canonical order.
  pub fn classes(&self) -> &[FactoryKey] {
    &self.classes
  }
  /// Returns the configured raw source-file ceiling.
  pub const fn max_files(&self) -> u16 {
    self.max_files
  }
  /// Returns the aggregate inventory/source byte ceiling.
  pub const fn max_bytes(&self) -> u64 {
    self.max_bytes
  }
  /// Returns the reserved capacity for all non-source signed inputs.
  pub const fn reserved_inputs(&self) -> u16 {
    self.reserved_inputs
  }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "PlanWire")]
/// Frozen coverage, subject, limits and retention selected before authoring.
pub struct ProtectedTestPlan {
  accepted_contract: FactoryDigest,
  subject: FactoryTaskSubject,
  #[serde(flatten)]
  coverage: ProtectedTestCoverage,
  retain_until: Timestamp,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanWire {
  accepted_contract: FactoryDigest,
  subject: FactoryTaskSubject,
  classes: Vec<FactoryKey>,
  max_files: u16,
  max_bytes: u64,
  reserved_inputs: u16,
  retain_until: Timestamp,
}
impl TryFrom<PlanWire> for ProtectedTestPlan {
  type Error = FactoryError;
  fn try_from(w: PlanWire) -> Result<Self, Self::Error> {
    Self::new(
      w.accepted_contract,
      w.subject,
      ProtectedTestCoverage::new(w.classes, w.max_files, w.max_bytes, w.reserved_inputs)?,
      w.retain_until,
    )
  }
}
impl ProtectedTestPlan {
  /// Freezes arbitrary required class keys under existing context-size ceilings.
  pub fn new(
    accepted_contract: FactoryDigest,
    subject: FactoryTaskSubject,
    coverage: ProtectedTestCoverage,
    retain_until: Timestamp,
  ) -> Result<Self, FactoryError> {
    Ok(Self {
      accepted_contract,
      subject,
      coverage,
      retain_until,
    })
  }
  /// Returns the immutable accepted Work or specification contract digest.
  pub const fn accepted_contract(&self) -> FactoryDigest {
    self.accepted_contract
  }
  /// Returns the exact subject used to author these tests.
  pub const fn subject(&self) -> &FactoryTaskSubject {
    &self.subject
  }
  /// Returns every required class, including Project-specific choices.
  pub fn classes(&self) -> &[FactoryKey] {
    self.coverage.classes()
  }
  /// Returns the configured file ceiling.
  pub const fn max_files(&self) -> u16 {
    self.coverage.max_files()
  }
  /// Returns the aggregate retained byte ceiling for inventory and raw source files.
  pub const fn max_bytes(&self) -> u64 {
    self.coverage.max_bytes()
  }
  /// Returns configured capacity for inventory, candidate and compiler inputs.
  pub const fn reserved_inputs(&self) -> u16 {
    self.coverage.reserved_inputs()
  }
  /// Returns the minimum required retention deadline.
  pub const fn retain_until(&self) -> Timestamp {
    self.retain_until
  }
  /// Content-addresses the complete frozen test plan.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    Ok(FactoryDigest::content_sha256(
      &serde_json::to_vec(self).map_err(|_| invalid())?,
    ))
  }
}
/// Protected file metadata; names and assertion bytes are never public handoff fields.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedTestFile {
  /// Exact configured class whose files this entry contributes.
  pub class: FactoryKey,
  /// Portable path inside the isolated protected bundle, not the candidate tree.
  pub path: FactoryRepositoryPath,
  /// Immutable Artifact identity verified against the actual source bytes.
  pub artifact: FactoryArtifactReference,
}
/// Safe closed handoff: no file names, Artifact grants, expected values or prose.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedTestHandoff {
  /// Accepted immutable contract identity.
  pub accepted_contract: FactoryDigest,
  /// Independently verified required coverage classes.
  pub classes: Vec<FactoryKey>,
}
/// Retained protected bundle accepted from verified ordinary Build outputs.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "BundleWire")]
pub struct ProtectedTestBundle {
  plan: ProtectedTestPlan,
  artifact: FactoryArtifactReference,
  files: Vec<ProtectedTestFile>,
  producer: EvidenceProducer,
  constructed_at: Timestamp,
  retained_until: Timestamp,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BundleWire {
  plan: ProtectedTestPlan,
  artifact: FactoryArtifactReference,
  files: Vec<ProtectedTestFile>,
  producer: EvidenceProducer,
  constructed_at: Timestamp,
  retained_until: Timestamp,
}
impl TryFrom<BundleWire> for ProtectedTestBundle {
  type Error = FactoryError;
  fn try_from(w: BundleWire) -> Result<Self, Self::Error> {
    Self::new(
      w.plan,
      w.artifact,
      w.files,
      w.producer,
      w.constructed_at,
      w.retained_until,
    )
  }
}
impl ProtectedTestBundle {
  /// Requires exact required-class completeness, unique files and sufficient retention.
  /// Callers must first verify every file and the enclosing Artifact's actual bytes.
  pub fn new(
    plan: ProtectedTestPlan,
    artifact: FactoryArtifactReference,
    mut files: Vec<ProtectedTestFile>,
    producer: EvidenceProducer,
    constructed_at: Timestamp,
    retained_until: Timestamp,
  ) -> Result<Self, FactoryError> {
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let classes = files.iter().map(|f| &f.class).collect::<BTreeSet<_>>();
    let total = files
      .iter()
      .try_fold(artifact.encoded_size(), |n, f| n.checked_add(f.artifact.encoded_size()))
      .ok_or_else(invalid)?;
    let file_ids = files.iter().map(|f| f.artifact.artifact_id()).collect::<BTreeSet<_>>();
    if files.is_empty()
      || files.len() > usize::from(plan.max_files())
      || files.windows(2).any(|p| p[0].path == p[1].path)
      || file_ids.len() != files.len()
      || file_ids.contains(&artifact.artifact_id())
      || classes != plan.classes().iter().collect()
      || total > plan.max_bytes()
      || constructed_at >= plan.retain_until
      || retained_until < plan.retain_until
    {
      return Err(invalid());
    }
    let bundle = Self {
      plan,
      artifact,
      files,
      producer,
      constructed_at,
      retained_until,
    };
    if bundle.canonical_bytes()?.len() > crate::MAX_FLOW_DATA_BYTES {
      return Err(invalid());
    }
    Ok(bundle)
  }
  /// Returns the frozen authoring contract.
  pub const fn plan(&self) -> &ProtectedTestPlan {
    &self.plan
  }
  /// Returns the exact retained protected Artifact.
  pub const fn artifact(&self) -> &FactoryArtifactReference {
    &self.artifact
  }
  /// Returns the verified file inventory for trusted validators only.
  pub fn files(&self) -> &[ProtectedTestFile] {
    &self.files
  }
  /// Returns actual ordinary Build provenance.
  pub const fn producer(&self) -> &EvidenceProducer {
    &self.producer
  }
  /// Returns the exclusive retained byte deadline.
  pub const fn retained_until(&self) -> Timestamp {
    self.retained_until
  }
  /// Returns only public contract and coverage identities.
  pub fn public_handoff(&self) -> ProtectedTestHandoff {
    ProtectedTestHandoff {
      accepted_contract: self.plan.accepted_contract,
      classes: self.plan.classes().to_vec(),
    }
  }
  /// Produces canonical protected metadata for restoration.
  pub fn canonical_bytes(&self) -> Result<Vec<u8>, FactoryError> {
    serde_json::to_vec(self).map_err(|_| invalid())
  }
  /// Binds the plan, protected Artifact, inventory, producer and retention together.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    Ok(FactoryDigest::content_sha256(&self.canonical_bytes()?))
  }
}
fn invalid() -> FactoryError {
  FactoryError::InvalidConfiguration {
    field: "protected test bundle",
  }
}
