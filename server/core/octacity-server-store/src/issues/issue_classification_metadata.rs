//! Retained attribution and references; effective scalar values remain in content/planning.
use crate::{AuditActor, MAX_ISSUE_PROPERTIES, StoreInputError};
use octacity_server_domain::{IssueVersion, Timestamp};
use octacity_server_factory::{
  FactoryDigest, FactoryKey, FactoryRunId, FactoryTaskSubject, ImmutableReference, WorkEnvelopeId,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Maximum field attributions: Project fields plus built-in priority and dependencies.
pub const MAX_ISSUE_CLASSIFICATION_FIELDS: usize = MAX_ISSUE_PROPERTIES + 2;
/// Maximum evidence or dependency references, matching the existing triage dependency bound.
pub const MAX_ISSUE_CLASSIFICATION_REFERENCES: usize = 64;

/// Optional classification provenance on one immutable Issue revision.
/// This is retained store data, not a management command or verification grant.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "IssueClassificationMetadataWire")]
pub struct IssueClassificationMetadata {
  /// Attribution of selected effective fields, without another copy of their values.
  pub fields: Vec<IssueClassificationAttribution>,
  /// Exact evidence identities, never evidence bodies or an editable verified flag.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub evidence: Vec<IssueClassificationEvidenceReference>,
  /// Existing exact Work/Run/terminal dependencies, not a new Issue dependency graph.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub dependencies: Vec<IssueClassificationDependencyReference>,
}
impl IssueClassificationMetadata {
  /// Empty metadata denotes unknown classification origin on ordinary/legacy Issues.
  pub fn is_empty(&self) -> bool {
    self.fields.is_empty() && self.evidence.is_empty() && self.dependencies.is_empty()
  }
  /// Checks retained shape, bounds and internal references; establishes no source authority.
  pub fn validate(&self) -> Result<(), StoreInputError> {
    let invalid = StoreInputError::InvalidIssue;
    let mut fields = BTreeSet::new();
    let records: BTreeSet<_> = self.evidence.iter().map(|e| e.record).collect();
    let mut dependencies = BTreeSet::new();
    if self.fields.len() > MAX_ISSUE_CLASSIFICATION_FIELDS
      || self.evidence.len() > MAX_ISSUE_CLASSIFICATION_REFERENCES
      || self.dependencies.len() > MAX_ISSUE_CLASSIFICATION_REFERENCES
      || records.len() != self.evidence.len()
      || self
        .evidence
        .iter()
        .any(|e| e.fresh_until <= e.observed_at || e.subject.exact().is_err())
      || self
        .dependencies
        .iter()
        .any(|d| !dependencies.insert((&d.work_id, &d.run_id, &d.terminal)))
      || self.fields.iter().any(|field| {
        let identity = match &field.field {
          IssueClassificationFieldReference::Priority => (0, ""),
          IssueClassificationFieldReference::Dependencies => (1, ""),
          IssueClassificationFieldReference::Property { id, definition_version } => {
            if *definition_version == 0 || !super::issue_planning::valid_property_id(id) {
              return true;
            }
            (2, id.as_str())
          }
        };
        let actor = match &field.origin {
          IssueClassificationOrigin::Manual { actor }
          | IssueClassificationOrigin::Pipeline { actor, .. }
          | IssueClassificationOrigin::Source { actor, .. } => actor,
        };
        !fields.insert(identity)
          || field.requirements_version > field.assessed_version
          || crate::management_mutation::validate_management_actor(actor).is_err()
          || field.evidence.len() > MAX_ISSUE_CLASSIFICATION_REFERENCES
          || field.evidence.iter().collect::<BTreeSet<_>>().len() != field.evidence.len()
          || field.evidence.iter().any(|record| !records.contains(record))
      })
    {
      return Err(invalid);
    }
    Ok(())
  }
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct IssueClassificationMetadataWire {
  fields: Vec<IssueClassificationAttribution>,
  evidence: Vec<IssueClassificationEvidenceReference>,
  dependencies: Vec<IssueClassificationDependencyReference>,
}
impl TryFrom<IssueClassificationMetadataWire> for IssueClassificationMetadata {
  type Error = StoreInputError;
  fn try_from(value: IssueClassificationMetadataWire) -> Result<Self, Self::Error> {
    let result = Self {
      fields: value.fields,
      evidence: value.evidence,
      dependencies: value.dependencies,
    };
    result.validate()?;
    Ok(result)
  }
}

/// Location of an effective classification value in the canonical Issue.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum IssueClassificationFieldReference {
  /// Existing built-in content priority.
  Priority,
  /// One Project property and the exact catalog that supplied its classification meaning.
  Property {
    /// Stable Project property identity, independent of its display name.
    id: String,
    /// Positive immutable Project catalog publication.
    definition_version: u64,
  },
  /// Issue-owned typed dependency references, outside generic planning JSON.
  Dependencies,
}

/// Original trusted author of a field assessment; never client-supplied authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum IssueClassificationOrigin {
  /// A reviewed operator mutation with its actual accepted management attribution.
  Manual {
    /// Original actor; trusted-network operation does not fabricate a person.
    actor: AuditActor,
  },
  /// An explicitly configured accepted Factory outcome.
  Pipeline {
    /// Accepted management actor, independent of model identity.
    actor: AuditActor,
    /// Exact admitted Work.
    work_id: WorkEnvelopeId,
    /// Exact producing Run.
    run_id: FactoryRunId,
    /// Exact accepted result record.
    record: FactoryDigest,
    /// Configured classification policy identity.
    policy: FactoryKey,
    /// Stable trusted action identity retained across replay.
    operation_id: FactoryDigest,
  },
  /// A trusted normalized native observation under an explicit mapping.
  Source {
    /// Actual accepted management actor.
    actor: AuditActor,
    /// Exact reviewed field mapping.
    mapping: ImmutableReference,
    /// Original accepted observation record.
    observation: FactoryDigest,
    /// Original scoped native association, without provider payloads.
    native_correlation: FactoryDigest,
  },
}

/// Provenance of one field, pinned to the requirements that were assessed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueClassificationAttribution {
  /// Canonical value location; scalar data is not copied here.
  pub field: IssueClassificationFieldReference,
  /// Trusted manual or producing-source attribution.
  pub origin: IssueClassificationOrigin,
  /// Exact Issue revision originally assessed.
  pub assessed_version: IssueVersion,
  /// Requirements applicability at assessment, not the latest edit's version.
  pub requirements_version: IssueVersion,
  /// Original assessment time, retained across later unrelated edits.
  pub assessed_at: Timestamp,
  /// Exact evidence records supporting this field; absence claims no verified evidence.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub evidence: Vec<FactoryDigest>,
}

/// A retained observation reference; qualification still requires independent trusted reads.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueClassificationEvidenceReference {
  /// Original observation Work.
  pub work_id: WorkEnvelopeId,
  /// Original observation Run.
  pub run_id: FactoryRunId,
  /// Exact retained observation identity, not model text.
  pub record: FactoryDigest,
  /// Exact repository base/candidate assessed by the observation.
  pub subject: FactoryTaskSubject,
  /// Immutable environment contract.
  pub environment: ImmutableReference,
  /// Immutable reproduction/verification criterion.
  pub criterion: ImmutableReference,
  /// Original observation time.
  pub observed_at: Timestamp,
  /// Exclusive freshness deadline; retention does not extend it.
  pub fresh_until: Timestamp,
}

/// The existing bounded dependency vocabulary retained by the Issue owner.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueClassificationDependencyReference {
  /// Exact dependency Work.
  pub work_id: WorkEnvelopeId,
  /// Exact dependency Run.
  pub run_id: FactoryRunId,
  /// Original immutable Work digest.
  pub work_digest: FactoryDigest,
  /// Declared producing terminal, never an inferred latest result.
  pub terminal: FactoryKey,
}
