//! Typed evaluator findings with exact evidence and stable fingerprints.

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::{
  EvaluationResultFinding, EvidenceManifest, FactoryArtifactReference, FactoryDigest, FactoryError, FactoryKey,
  FactorySafeText, FindingSeverity,
};

use super::MAX_ASSESSMENT_EVIDENCE_REFERENCES;

/// Exact manifest item cited by one evaluator finding.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssessmentEvidenceReference {
  kind: FactoryKey,
  artifact: FactoryArtifactReference,
}

impl AssessmentEvidenceReference {
  fn resolve(kind: &FactoryKey, evidence: &EvidenceManifest) -> Result<Self, FactoryError> {
    let item = evidence.item(kind).ok_or(FactoryError::InvalidReference {
      relationship: "assessment finding evidence",
    })?;
    Ok(Self {
      kind: kind.clone(),
      artifact: item.artifact().clone(),
    })
  }

  /// Returns the canonical evidence kind.
  #[must_use]
  pub const fn kind(&self) -> &FactoryKey {
    &self.kind
  }

  /// Returns the exact immutable evidence Artifact.
  #[must_use]
  pub const fn artifact(&self) -> &FactoryArtifactReference {
    &self.artifact
  }
}

/// Typed policy-relevant classification of an evaluator finding.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum AssessmentFindingKind {
  /// A candidate violation with deterministic severity.
  Violation {
    /// Severity interpreted only by deterministic policy.
    severity: FindingSeverity,
  },
  /// Available evidence was insufficient for a determination.
  EvidenceGap,
}

/// One schema-valid typed finding returned by an evaluator.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AssessmentFinding {
  finding: AssessmentFindingKind,
  summary: FactorySafeText,
  evidence: Vec<AssessmentEvidenceReference>,
  remediation: FactorySafeText,
  fingerprint: FactoryDigest,
}

impl AssessmentFinding {
  /// Resolves a provider-neutral result finding against one exact Evidence Manifest.
  pub fn from_result(finding: &EvaluationResultFinding, evidence: &EvidenceManifest) -> Result<Self, FactoryError> {
    let (kind, summary, evidence_kinds, remediation) = match finding {
      EvaluationResultFinding::Violation {
        severity,
        summary,
        evidence,
        remediation,
      } => (
        AssessmentFindingKind::Violation { severity: *severity },
        summary.clone(),
        evidence,
        remediation.clone(),
      ),
      EvaluationResultFinding::EvidenceGap {
        summary,
        evidence,
        remediation,
      } => (
        AssessmentFindingKind::EvidenceGap,
        summary.clone(),
        evidence,
        remediation.clone(),
      ),
    };
    let evidence = evidence_kinds
      .iter()
      .map(|evidence_kind| AssessmentEvidenceReference::resolve(evidence_kind, evidence))
      .collect::<Result<Vec<_>, _>>()?;
    Self::from_parts(kind, summary, evidence, remediation, None)
  }

  fn from_parts(
    finding: AssessmentFindingKind,
    summary: FactorySafeText,
    mut evidence: Vec<AssessmentEvidenceReference>,
    remediation: FactorySafeText,
    expected_fingerprint: Option<FactoryDigest>,
  ) -> Result<Self, FactoryError> {
    if evidence.is_empty() || evidence.len() > MAX_ASSESSMENT_EVIDENCE_REFERENCES {
      return Err(FactoryError::InvalidReference {
        relationship: "assessment finding evidence",
      });
    }
    evidence.sort();
    if evidence.windows(2).any(|pair| pair[0].kind == pair[1].kind) {
      return Err(FactoryError::InvalidReference {
        relationship: "assessment finding evidence",
      });
    }
    #[derive(Serialize)]
    struct FingerprintInput<'a> {
      finding: AssessmentFindingKind,
      summary: &'a FactorySafeText,
      evidence: &'a [AssessmentEvidenceReference],
      remediation: &'a FactorySafeText,
    }
    let encoded = serde_json::to_vec(&FingerprintInput {
      finding,
      summary: &summary,
      evidence: &evidence,
      remediation: &remediation,
    })
    .map_err(|_| FactoryError::InvalidReference {
      relationship: "assessment finding serialization",
    })?;
    let fingerprint = FactoryDigest::sha256("octacity.factory.assessment-finding.v1", &[&encoded]);
    if expected_fingerprint.is_some_and(|expected| expected != fingerprint) {
      return Err(FactoryError::InvalidReference {
        relationship: "assessment finding fingerprint",
      });
    }
    Ok(Self {
      finding,
      summary,
      evidence,
      remediation,
      fingerprint,
    })
  }

  /// Returns the typed finding classification.
  #[must_use]
  pub const fn kind(&self) -> AssessmentFindingKind {
    self.finding
  }

  /// Returns violation severity, or `None` for an evidence gap.
  #[must_use]
  pub const fn severity(&self) -> Option<FindingSeverity> {
    match self.finding {
      AssessmentFindingKind::Violation { severity } => Some(severity),
      AssessmentFindingKind::EvidenceGap => None,
    }
  }

  /// Returns the bounded secret-checked summary.
  #[must_use]
  pub const fn summary(&self) -> &FactorySafeText {
    &self.summary
  }

  /// Returns exact deterministic evidence references supporting the finding.
  #[must_use]
  pub fn evidence(&self) -> &[AssessmentEvidenceReference] {
    &self.evidence
  }

  /// Returns bounded corrective guidance that cannot select an outcome.
  #[must_use]
  pub const fn remediation(&self) -> &FactorySafeText {
    &self.remediation
  }

  /// Returns the stable server-derived finding fingerprint.
  #[must_use]
  pub const fn fingerprint(&self) -> FactoryDigest {
    self.fingerprint
  }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssessmentFindingWire {
  finding: AssessmentFindingKind,
  summary: FactorySafeText,
  evidence: Vec<AssessmentEvidenceReference>,
  remediation: FactorySafeText,
  fingerprint: FactoryDigest,
}

impl<'de> Deserialize<'de> for AssessmentFinding {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let wire = AssessmentFindingWire::deserialize(deserializer)?;
    Self::from_parts(
      wire.finding,
      wire.summary,
      wire.evidence,
      wire.remediation,
      Some(wire.fingerprint),
    )
    .map_err(D::Error::custom)
  }
}
