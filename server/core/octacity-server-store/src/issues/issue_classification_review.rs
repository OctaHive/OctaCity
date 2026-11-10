//! Derived classification diagnostics; reading never renews an assessment or grants admission.
use crate::{IssueClassificationFieldReference, IssueRevision};
use octacity_server_domain::IssueVersion;
use serde::Serialize;

/// A bounded reason why an effective field cannot supply a current assessment.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum IssueClassificationProblem {
  /// No effective scalar/dependency value has been supplied for this field.
  MissingValue,
  /// A legacy value has no retained assessment origin or applicability.
  MissingAssessment,
  /// The property's exact original definition is unavailable, so its meaning cannot be established.
  DefinitionUnavailable,
  /// The assessment reviewed requirements that have since changed.
  RequirementsChanged,
  /// The declared code subject differs from the independently resolved review subject.
  CodeSubjectChanged,
  /// A code-dependent assessment cannot be reviewed without a resolved code subject.
  CodeSubjectUnavailable,
  /// A declared input is no longer selected with its original owner, identity and digest.
  MaterialChanged {
    /// Original safe source pin, without its body.
    reference: crate::IssueMaterialReference,
  },
  /// A reproduced/not-reproduced value has no selected independent evidence.
  EvidenceMissing,
  /// A selected reference has not been independently read and qualified under current access.
  EvidenceUnavailable {
    /// Exact selected observation, without its body.
    record: octacity_server_factory::FactoryDigest,
  },
  /// The original freshness deadline has passed.
  EvidenceExpired {
    /// Exact selected observation.
    record: octacity_server_factory::FactoryDigest,
  },
  /// The evidence concerns another exact base or candidate.
  EvidenceSubjectChanged {
    /// Exact selected observation.
    record: octacity_server_factory::FactoryDigest,
  },
  /// No current code subject was resolved for an evidence-dependent field.
  EvidenceSubjectUnavailable {
    /// Exact selected observation.
    record: octacity_server_factory::FactoryDigest,
  },
  /// Independent observations contradict the stored reproduction claim.
  EvidenceClaimConflict {
    /// Exact selected observation.
    record: octacity_server_factory::FactoryDigest,
  },
  /// Execution conditions or coverage establish neither reproduction outcome.
  EvidenceInconclusive {
    /// Exact selected observation.
    record: octacity_server_factory::FactoryDigest,
  },
  /// A retained value is outside the supported preliminary reproduction contract.
  UnsupportedReproduction,
}
/// Applicability of one canonical value; the value and its origin remain in the Issue revision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct IssueClassificationFieldReview {
  /// Canonical field and original definition pin.
  pub field: IssueClassificationFieldReference,
  /// Empty only when the selected assessment has no known applicability blocker.
  pub problems: Vec<IssueClassificationProblem>,
  /// Whether the assessment actually claims an independently verified observation.
  pub requires_verified_evidence: bool,
  /// Independent evidence diagnostics; an unresolved value does not claim these observations.
  pub evidence: Vec<IssueClassificationEvidenceReview>,
}
/// Safe diagnostics for an exact selected observation, without any report bytes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct IssueClassificationEvidenceReview {
  /// Original selected observation identity.
  pub record: octacity_server_factory::FactoryDigest,
  /// Current access, freshness and subject problems for this observation.
  pub problems: Vec<IssueClassificationProblem>,
}
/// Read-only diagnostics for an exact Issue revision, not an execution readiness verdict.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct IssueClassificationReview {
  /// Revision whose values were reviewed.
  pub version: IssueVersion,
  /// Bounded per-field assessments.
  pub fields: Vec<IssueClassificationFieldReview>,
}
impl IssueRevision {
  /// Derives requirements freshness without mutating retained provenance or evidence.
  /// `assessment_catalogs` supplies exact original publications for attributed properties;
  /// the current `catalog` discovers unassessed fields without redefining retained semantics.
  pub fn review_classification(
    &self,
    catalog: &crate::PublishedIssueProperties,
    assessment_catalogs: &std::collections::BTreeMap<u64, crate::PublishedIssueProperties>,
    subject: Option<&octacity_server_factory::FactoryTaskSubject>,
    checked_at: octacity_server_domain::Timestamp,
  ) -> IssueClassificationReview {
    use crate::IssueClassificationFieldReference as Field;
    let mut fields: Vec<_> = self.classification.fields.iter().map(|f| f.field.clone()).collect();
    let priority_version = if matches!(self.content.priority, Some(crate::IssuePriority::LegacyNumber(_))) {
      None
    } else {
      Some(catalog.version)
    };
    for field in [
      Field::Priority {
        definition_version: priority_version,
      },
      Field::Component {
        definition_version: catalog.version,
      },
      Field::Type {
        definition_version: catalog.version,
      },
      Field::Size,
    ]
    .into_iter()
    .chain(
      catalog
        .definition
        .classification
        .fields()
        .map(|(_, id)| Field::Property {
          id: id.to_owned(),
          definition_version: catalog.version,
        }),
    ) {
      if !fields.iter().any(|prior| prior.identity() == field.identity()) {
        fields.push(field);
      }
    }
    IssueClassificationReview {
      version: self.version,
      fields: fields
        .into_iter()
        .map(|reference| {
          let mut problems = vec![];
          if !has_value(self, &reference) {
            problems.push(IssueClassificationProblem::MissingValue);
          }
          let Some(field) = self
            .classification
            .fields
            .iter()
            .find(|f| f.field.identity() == reference.identity())
          else {
            if problems.is_empty() {
              problems.push(IssueClassificationProblem::MissingAssessment);
            }
            return IssueClassificationFieldReview {
              field: reference,
              problems,
              requires_verified_evidence: false,
              evidence: vec![],
            };
          };
          if field.requirements_version != self.applicability_version {
            problems.push(IssueClassificationProblem::RequirementsChanged);
          }
          if let IssueClassificationFieldReference::Property { definition_version, .. } = &field.field
            && assessment_catalogs
              .get(definition_version)
              .is_none_or(|catalog| catalog.project_id != self.project_id || catalog.version != *definition_version)
          {
            problems.push(IssueClassificationProblem::DefinitionUnavailable);
          }
          if let Some(expected) = &field.inputs.subject {
            match subject {
              None => problems.push(IssueClassificationProblem::CodeSubjectUnavailable),
              Some(actual) if actual != expected => problems.push(IssueClassificationProblem::CodeSubjectChanged),
              _ => {}
            }
          }
          for reference in &field.inputs.materials {
            if !self
              .materials
              .entries()
              .iter()
              .any(|entry| &entry.reference == reference)
            {
              problems.push(IssueClassificationProblem::MaterialChanged {
                reference: reference.clone(),
              });
            }
          }
          let reproduction = self.classification_reproduction_claim(&field.field, assessment_catalogs);
          let requires_verified_evidence = match reproduction {
            Some(claim) => match crate::IssueReproductionClaim::from_choice(claim) {
              Some(claim) => claim.requires_verified_evidence(),
              None => {
                problems.push(IssueClassificationProblem::UnsupportedReproduction);
                false
              }
            },
            None => !field.evidence.is_empty(),
          };
          if requires_verified_evidence && field.evidence.is_empty() {
            problems.push(IssueClassificationProblem::EvidenceMissing);
          }
          let mut evidence_reviews = vec![];
          for record in &field.evidence {
            let mut evidence_problems = vec![];
            if let Some(evidence) = self.classification.evidence.iter().find(|e| e.record == *record) {
              match subject {
                None => {
                  evidence_problems.push(IssueClassificationProblem::EvidenceSubjectUnavailable { record: *record })
                }
                Some(subject) if subject != &evidence.subject => {
                  evidence_problems.push(IssueClassificationProblem::EvidenceSubjectChanged { record: *record })
                }
                _ => {}
              }
              if checked_at >= evidence.fresh_until {
                evidence_problems.push(IssueClassificationProblem::EvidenceExpired { record: *record });
              }
            }
            evidence_problems.push(IssueClassificationProblem::EvidenceUnavailable { record: *record });
            if requires_verified_evidence {
              problems.extend(evidence_problems.clone());
            }
            evidence_reviews.push(IssueClassificationEvidenceReview {
              record: *record,
              problems: evidence_problems,
            });
          }
          IssueClassificationFieldReview {
            field: field.field.clone(),
            problems,
            requires_verified_evidence,
            evidence: evidence_reviews,
          }
        })
        .collect(),
    }
  }

  /// Resolves a retained property's reproduction meaning from its exact original catalog.
  /// Current semantic bindings must not reinterpret a historical assessment.
  pub fn classification_reproduction_claim(
    &self,
    field: &IssueClassificationFieldReference,
    assessment_catalogs: &std::collections::BTreeMap<u64, crate::PublishedIssueProperties>,
  ) -> Option<&str> {
    let IssueClassificationFieldReference::Property { id, definition_version } = field else {
      return None;
    };
    let catalog = assessment_catalogs.get(definition_version)?;
    if catalog.project_id != self.project_id
      || catalog.version != *definition_version
      || catalog.definition.classification.reproducibility.as_ref() != Some(id)
    {
      return None;
    }
    match self.planning.values.get(id) {
      Some(crate::IssuePropertyValue::Choice(claim)) => Some(claim.as_str()),
      _ => None,
    }
  }
}

fn has_value(issue: &IssueRevision, field: &IssueClassificationFieldReference) -> bool {
  use crate::IssueClassificationFieldIdentity as Field;
  match field.identity() {
    Field::Priority => issue.content.priority.is_some(),
    Field::Component => issue.planning.component.is_some(),
    Field::Type => issue.planning.issue_type.is_some(),
    Field::Size => issue.planning.size.is_some(),
    Field::Property { id } => issue.planning.values.contains_key(id),
    Field::Dependencies => !issue.classification.dependencies.is_empty(),
  }
}
