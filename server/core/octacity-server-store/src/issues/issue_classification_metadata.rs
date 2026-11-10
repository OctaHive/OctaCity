//! Retained attribution and references; effective scalar values remain in content/planning.
use crate::{AuditActor, IssueRevision, MAX_ISSUE_PROPERTIES, PublishedIssueProperties, StoreInputError};
use octacity_server_domain::{IssueVersion, Timestamp};
use octacity_server_factory::{
  FactoryDigest, FactoryKey, FactoryRunId, FactoryTaskSubject, ImmutableReference, WorkEnvelopeId,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Maximum field attributions: Project fields plus priority, component, type, size and dependencies.
pub const MAX_ISSUE_CLASSIFICATION_FIELDS: usize = MAX_ISSUE_PROPERTIES + 5;
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
    let mut property_count = 0;
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
        let valid_version = match &field.field {
          IssueClassificationFieldReference::Priority { .. } | IssueClassificationFieldReference::Type { .. } => true,
          IssueClassificationFieldReference::Component { definition_version }
          | IssueClassificationFieldReference::Property { definition_version, .. } => *definition_version != 0,
          IssueClassificationFieldReference::Size | IssueClassificationFieldReference::Dependencies => true,
        };
        let identity = field.field.identity();
        if let IssueClassificationFieldIdentity::Property { id } = identity {
          property_count += 1;
          if property_count > MAX_ISSUE_PROPERTIES || !super::issue_planning::valid_property_id(id) {
            return true;
          }
        }
        let actor = match &field.origin {
          IssueClassificationOrigin::Manual { actor }
          | IssueClassificationOrigin::Pipeline { actor, .. }
          | IssueClassificationOrigin::Source { actor, .. } => actor,
        };
        !valid_version
          || !fields.insert(identity)
          || field.requirements_version > field.assessed_version
          || field.inputs.validate().is_err()
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

impl IssueRevision {
  /// Records only changed manual classification values after ordinary edit validation.
  /// The Issue writer commits this metadata with the values and original replay receipt.
  /// Unchanged fields keep their original applicability, age and evidence references.
  pub fn attribute_manual_classification(
    &mut self,
    previous: Option<&Self>,
    catalog: &PublishedIssueProperties,
    actor: &AuditActor,
  ) -> Result<(), crate::StoreError> {
    use IssueClassificationFieldReference as Field;
    // A combined requirements/value edit assesses the new requirements in this revision.
    let assessed_version = previous.map_or(self.version, |p| p.version.max(self.applicability_version));
    if self.content.priority.as_ref() != previous.and_then(|p| p.content.priority.as_ref()) {
      let definition_version = match &self.content.priority {
        Some(crate::IssuePriority::Choice(_)) => Some(catalog.version),
        _ => None,
      };
      self.record_manual_field(
        assessed_version,
        Field::Priority { definition_version },
        self.content.priority.is_some(),
        actor,
      );
    }
    for (field, changed, present) in [
      (
        Field::Component {
          definition_version: catalog.version,
        },
        self.planning.component.as_ref() != previous.and_then(|p| p.planning.component.as_ref()),
        self.planning.component.is_some(),
      ),
      (
        Field::Type {
          definition_version: catalog.version,
        },
        self.planning.issue_type.as_ref() != previous.and_then(|p| p.planning.issue_type.as_ref()),
        self.planning.issue_type.is_some(),
      ),
      (
        Field::Size,
        self.planning.size != previous.and_then(|p| p.planning.size),
        self.planning.size.is_some(),
      ),
    ] {
      if changed {
        self.record_manual_field(assessed_version, field, present, actor);
      }
    }
    let ids: BTreeSet<_> = previous
      .into_iter()
      .flat_map(|p| p.planning.values.keys())
      .chain(self.planning.values.keys())
      .cloned()
      .collect();
    for id in ids {
      if self.planning.values.get(&id) != previous.and_then(|p| p.planning.values.get(&id)) {
        let present = self.planning.values.contains_key(&id)
          && catalog.definition.classification.fields().any(|(_, bound)| bound == id);
        self.record_manual_field(
          assessed_version,
          Field::Property {
            id,
            definition_version: catalog.version,
          },
          present,
          actor,
        );
      }
    }
    self
      .classification
      .validate()
      .map_err(|error| crate::StoreError::invalid(crate::StoreOperation::EditIssuePlanning, error))
  }

  fn record_manual_field(
    &mut self,
    assessed_version: IssueVersion,
    field: IssueClassificationFieldReference,
    present: bool,
    actor: &AuditActor,
  ) {
    self
      .classification
      .fields
      .retain(|prior| prior.field.identity() != field.identity());
    if present {
      self.classification.fields.push(IssueClassificationAttribution {
        field,
        origin: IssueClassificationOrigin::Manual { actor: actor.clone() },
        assessed_version,
        requirements_version: self.applicability_version,
        assessed_at: self.updated_at,
        evidence: vec![],
        inputs: Default::default(),
      });
    }
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
  /// Built-in content priority; legacy numeric attribution has no choice catalog pin.
  Priority {
    /// Exact Project priority-choice publication for new assessments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    definition_version: Option<u64>,
  },
  /// Built-in finite size, independent of legacy size-label effects.
  Size,
  /// Built-in type and the exact Project type-list publication.
  Type {
    /// Exact Project catalog publication; zero pins the built-in default template.
    definition_version: u64,
  },
  /// Built-in component and the exact Project component-list publication.
  Component {
    /// Positive immutable Project catalog publication.
    definition_version: u64,
  },
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

/// Canonical effective field location, independent of its assessment's catalog pin.
/// Shared by duplicate validation, attribution replacement and receipt lookup.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IssueClassificationFieldIdentity<'a> {
  /// Built-in content priority, including retained numeric values.
  Priority,
  /// Built-in Project component.
  Component,
  /// Built-in Project type.
  Type,
  /// Built-in finite size.
  Size,
  /// One stable custom property within the owning Issue's Project.
  Property {
    /// Stable identity, independent of the definition publication.
    id: &'a str,
  },
  /// Issue-owned exact dependency references.
  Dependencies,
}
impl IssueClassificationFieldReference {
  /// Identifies the same effective field across Project definition publications.
  pub fn identity(&self) -> IssueClassificationFieldIdentity<'_> {
    match self {
      Self::Priority { .. } => IssueClassificationFieldIdentity::Priority,
      Self::Component { .. } => IssueClassificationFieldIdentity::Component,
      Self::Type { .. } => IssueClassificationFieldIdentity::Type,
      Self::Size => IssueClassificationFieldIdentity::Size,
      Self::Property { id, .. } => IssueClassificationFieldIdentity::Property { id },
      Self::Dependencies => IssueClassificationFieldIdentity::Dependencies,
    }
  }
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
  /// Explicit immutable dependencies of this assessment; legacy data has none declared.
  #[serde(default, skip_serializing_if = "IssueClassificationAssessmentInputs::is_empty")]
  pub inputs: IssueClassificationAssessmentInputs,
}

/// Bounded assessment input pins, without input bodies or execution authority.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueClassificationAssessmentInputs {
  /// Exact code subject only when this field's assessment depends on code.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub subject: Option<FactoryTaskSubject>,
  /// Exact explicitly selected material dependencies; bodies stay with their original owners.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub materials: Vec<crate::IssueMaterialReference>,
}
impl IssueClassificationAssessmentInputs {
  /// Reuses the shared exact-material bounds and rejects correlation-only code subjects.
  pub fn validate(&self) -> Result<(), StoreInputError> {
    if self.subject.as_ref().is_some_and(|subject| subject.exact().is_err()) {
      return Err(StoreInputError::InvalidIssue);
    }
    super::issue_material::validate_references(self.materials.iter())
  }
  /// Empty pins preserve the wire encoding of historical attributions.
  pub fn is_empty(&self) -> bool {
    self.subject.is_none() && self.materials.is_empty()
  }
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
