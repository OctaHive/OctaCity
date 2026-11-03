//! Bounded read of accepted, explicitly presented results for an exact Issue.
use crate::{FactoryRunListVisibility, FactoryRunPagePosition, IssueRevision, StoreError};
use octacity_server_domain::Timestamp;
use octacity_server_factory::{
  FactoryArtifactReference, FactoryDigest, FactoryKey, FactoryRunId, FactoryText, FlowIssuePresentation,
  FlowIssueReferenceKind, FlowIssueSlot, MAX_ISSUE_OUTCOME_CONCLUSION_BYTES, MAX_ISSUE_OUTCOME_REFERENCES,
  NodeAttemptId, WorkArtifacts, WorkEnvelopeId,
};

/// Maximum number of current slots returned for one Issue.
pub const MAX_ISSUE_OUTCOME_CARDS: u16 = 32;

/// Presentation fields selected by the immutable outcome binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IssueOutcomeContent {
  /// Advisory facts from an accepted triage result.
  Triage {
    /// Reviewed business decision.
    decision: FactoryKey,
    /// Verified scope of the duplicate check, when retained.
    duplicate_coverage: Option<FactoryKey>,
    /// Recommended priority, independent of Issue planning.
    priority: Option<u8>,
    /// Recommended size.
    size: Option<FactoryKey>,
    /// Recommended component.
    component: Option<FactoryKey>,
  },
  /// Accepted specifications.
  Specifications,
  /// Accepted review and its verdict.
  Review {
    /// Reviewed disposition.
    verdict: FactoryKey,
  },
  /// A short result under an author-selected title.
  Custom {
    /// Frozen title from the Flow definition.
    title: FactoryText,
  },
}

/// One current slot and the exact accepted occurrence that supplies it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssueOutcomeCard {
  /// Stable identity across versions of the declaring Flow.
  pub slot: FlowIssueSlot,
  /// Plain text from the designated field of the accepted result.
  pub conclusion: String,
  /// Allowlisted facts for the selected presentation.
  pub content: IssueOutcomeContent,
  /// Original Work association.
  pub work_id: WorkEnvelopeId,
  /// Original Run association.
  pub run_id: FactoryRunId,
  /// Exact accepted node occurrence.
  pub node_attempt_id: NodeAttemptId,
}

/// An accepted resource selection awaiting its owner's independent visibility check.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IssueOutcomeReference {
  /// Exact immutable Project document.
  Document(FactoryArtifactReference),
  /// Exact immutable Artifact.
  Artifact(FactoryArtifactReference),
  /// Exact retained Issue review association.
  Review(FactoryDigest),
}

/// Keyset position in newest admission and newest accepted occurrence order.
#[derive(Clone, Copy, Debug)]
pub struct IssueOutcomePosition {
  /// Original admission and its tie-breaker.
  pub run: FactoryRunPagePosition,
  /// Accepted occurrence time.
  pub completed_at: Timestamp,
  /// Occurrence tie-breaker.
  pub attempt: NodeAttemptId,
}

/// Candidate metadata used to authorize and rank outcomes before loading result bodies.
#[derive(Debug)]
pub struct IssueOutcomeCandidate {
  /// Exact ordered occurrence.
  pub position: IssueOutcomePosition,
  /// Exact Issue Work association.
  pub work_id: WorkEnvelopeId,
  /// Canonical slot computed from the published declaration.
  pub slot: FlowIssueSlot,
  /// Immutable presentation from the accepted occurrence's Flow.
  pub presentation: FlowIssuePresentation,
}

/// Selected accepted outcome with its immutable Work inputs.
#[derive(Debug)]
pub struct IssueOutcomeRecord {
  /// Raw accepted value, restricted to the projection boundary.
  pub value: serde_json::Value,
  /// Original input inventory used by the Issue artifact reader.
  pub inputs: WorkArtifacts,
}

impl IssueOutcomeReference {
  /// Selects only the resource fields explicitly allowed by the published presentation.
  pub fn from_accepted(presentation: &FlowIssuePresentation, value: &serde_json::Value) -> Vec<Self> {
    let mut references = Vec::new();
    let mut select = |kind: FlowIssueReferenceKind, field: &serde_json::Value| {
      let reference = match kind {
        FlowIssueReferenceKind::Document => serde_json::from_value(field.clone()).ok().map(Self::Document),
        FlowIssueReferenceKind::Artifact => serde_json::from_value(field.clone()).ok().map(Self::Artifact),
        FlowIssueReferenceKind::Review => field.as_str().and_then(|id| id.parse().ok()).map(Self::Review),
      };
      if let Some(reference) = reference
        && !references.contains(&reference)
      {
        references.push(reference);
      }
    };
    match presentation {
      FlowIssuePresentation::SpecificationsV1 { .. } => {
        if let Some(documents) = value["documents"].as_array() {
          for document in documents.iter().take(usize::from(MAX_ISSUE_OUTCOME_REFERENCES)) {
            select(FlowIssueReferenceKind::Document, document);
          }
        }
      }
      FlowIssuePresentation::ReviewV1 { .. } => select(FlowIssueReferenceKind::Review, &value["review"]),
      FlowIssuePresentation::CustomV1 {
        references: bindings, ..
      } => {
        for binding in bindings.iter().take(usize::from(MAX_ISSUE_OUTCOME_REFERENCES)) {
          select(binding.kind, &value[binding.field.as_str()]);
        }
      }
      FlowIssuePresentation::TriageV1 { .. } => {}
    }
    references
  }
}

/// Repository read scoped to the current Issue revision and caller's Run visibility.
#[async_trait::async_trait]
pub trait IssueOutcomeStore: Send + Sync {
  /// Reads one bounded page of metadata in descending position order.
  async fn issue_outcome_candidates(
    &self,
    issue: &IssueRevision,
    visibility: &FactoryRunListVisibility,
    after: Option<IssueOutcomePosition>,
  ) -> Result<Vec<IssueOutcomeCandidate>, StoreError>;

  /// Loads only a selected exact occurrence while its Issue revision and Run remain visible.
  async fn issue_outcome_record(
    &self,
    issue: &IssueRevision,
    candidate: &IssueOutcomeCandidate,
  ) -> Result<Option<IssueOutcomeRecord>, StoreError>;
}

impl IssueOutcomeContent {
  /// Extracts only the fields permitted by the published presentation.
  pub fn project(
    presentation: FlowIssuePresentation,
    result: &serde_json::Value,
  ) -> Result<(String, Self), StoreError> {
    let conclusion_field = match &presentation {
      FlowIssuePresentation::CustomV1 { conclusion_field, .. } => conclusion_field.as_str(),
      _ => "conclusion",
    };
    let conclusion = result[conclusion_field]
      .as_str()
      .filter(|value| !value.is_empty() && value.len() <= MAX_ISSUE_OUTCOME_CONCLUSION_BYTES)
      .ok_or(StoreError::Unavailable)?;
    let field = |name: &str| -> Result<Option<FactoryKey>, StoreError> {
      result
        .get(name)
        .map(|value| {
          value
            .as_str()
            .ok_or(StoreError::Unavailable)?
            .parse()
            .map_err(|_| StoreError::Unavailable)
        })
        .transpose()
    };
    let content = match presentation {
      FlowIssuePresentation::TriageV1 { .. } => Self::Triage {
        decision: field("decision")?.ok_or(StoreError::Unavailable)?,
        duplicate_coverage: field("duplicate_coverage")?,
        priority: result["priority"].as_u64().and_then(|value| u8::try_from(value).ok()),
        size: field("size")?,
        component: field("component")?,
      },
      FlowIssuePresentation::SpecificationsV1 { .. } => Self::Specifications,
      FlowIssuePresentation::ReviewV1 { .. } => Self::Review {
        verdict: field("verdict")?.ok_or(StoreError::Unavailable)?,
      },
      FlowIssuePresentation::CustomV1 { title, .. } => Self::Custom { title },
    };
    Ok((conclusion.to_owned(), content))
  }
}
