use super::{RESEARCH_CONTRACT_VERSION, canonicalize, digest, invalid};
use crate::{
  AdmittedFlow, ClassificationReceipt, ContextManifest, ContextSourceKind, ExactSubject, FactoryArtifactReference,
  FactoryConfigurationRef, FactoryContextReference, FactoryDigest, FactoryError, FactoryKey, FactoryTaskSubject,
  FlowAdmissionLimits, RetrievalReceipt, TriageJournalRecord, TriageRoute, WorkEnvelope, WorkEnvelopeId, WorkKind,
};
use serde::{Deserialize, Serialize};

/// One exact source already frozen into the research Context Manifest.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResearchSourceReference {
  /// Declared context source category.
  pub source_kind: ContextSourceKind,
  /// Stable logical context identity.
  pub logical_identity: FactoryKey,
  /// Digest of the exact retained source bytes.
  pub content_digest: FactoryDigest,
}

/// Bounded branch-specific inputs; all mutable discovery is retained by reference.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum ResearchDetails {
  /// Exact defect symptoms, environment, and prior observations.
  Defect {
    /// Retained symptoms and regression expectations.
    symptoms: FactoryArtifactReference,
    /// Frozen environment contract for reproduction.
    environment: FactoryArtifactReference,
    /// Retained prior reproduction observations.
    prior_observations: Vec<FactoryArtifactReference>,
  },
  /// Project goals and source references for a feature proposal.
  Feature {
    /// Exact goals already accepted by eligibility.
    project_goals: FactoryArtifactReference,
    /// Non-empty frozen sources selected by the declared Context projection.
    sources: Vec<ResearchSourceReference>,
  },
}

/// Immutable authorized research input, reconstructed from accepted triage on restore.
///
/// This proof has no direct deserializer: stored bytes cross `restore` with the
/// exact Work and admitted policy, while providers receive only its serialization.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResearchInput {
  schema_version: u16,
  admission_limits: FlowAdmissionLimits,
  policy_digest: FactoryDigest,
  accepted_triage: ClassificationReceipt,
  context: ContextManifest,
  retrieval: Vec<RetrievalReceipt>,
  details: ResearchDetails,
}

impl ResearchInput {
  /// Freezes accepted research routing and every explicitly projected input.
  pub fn new(
    work: &WorkEnvelope,
    admitted: &AdmittedFlow,
    accepted_triage: ClassificationReceipt,
    context: ContextManifest,
    mut retrieval: Vec<RetrievalReceipt>,
    mut details: ResearchDetails,
    policy_digest: FactoryDigest,
  ) -> Result<Self, FactoryError> {
    TriageJournalRecord::Classification(Box::new(accepted_triage.clone())).validate(work, admitted)?;
    if accepted_triage.decision.route != TriageRoute::Research
      || context.subject() != &FactoryTaskSubject::Exact(work.subject().clone())
    {
      return Err(invalid("research accepted input"));
    }
    if retrieval.len() > super::MAX_RESEARCH_ITEMS {
      return Err(invalid("research retrieval bound"));
    }
    retrieval.sort_by_key(RetrievalReceipt::id);
    if retrieval.windows(2).any(|pair| pair[0].id() == pair[1].id())
      || retrieval.iter().any(|row| row.subject() != context.subject())
    {
      return Err(invalid("research retrieval identity"));
    }
    for entry in context.entries() {
      if let FactoryContextReference::RepositoryFragment(reference) = entry.source()
        && !retrieval
          .iter()
          .any(|receipt| receipt.contains_reference(reference).unwrap_or(false))
      {
        return Err(invalid("research retrieval receipt"));
      }
    }
    if retrieval.iter().any(|receipt| {
      !context.entries().iter().any(|entry| {
        matches!(entry.source(), FactoryContextReference::RepositoryFragment(reference) if reference.retrieval_receipt_id() == receipt.id())
      })
    }) {
      return Err(invalid("unused research retrieval"));
    }
    let eligibility = &accepted_triage.eligibility.input;
    let mut required = vec![
      eligibility.task(),
      eligibility.acceptance(),
      &accepted_triage.result.provenance().result,
    ];
    match &mut details {
      ResearchDetails::Defect {
        symptoms,
        environment,
        prior_observations,
      } => {
        if *accepted_triage.result.classification().work_kind.value() != WorkKind::Defect {
          return Err(invalid("research Work kind"));
        }
        canonicalize(prior_observations, 0)?;
        required.extend([&*symptoms, &*environment]);
        required.extend(prior_observations.iter());
      }
      ResearchDetails::Feature { project_goals, sources } => {
        if *accepted_triage.result.classification().work_kind.value() != WorkKind::FeatureRequest
          || project_goals != eligibility.project_goals()
        {
          return Err(invalid("research feature goals"));
        }
        canonicalize(sources, 1)?;
        if sources.iter().any(|source| !contains_source(&context, source)) {
          return Err(invalid("research frozen source"));
        }
        required.push(project_goals);
      }
    }
    if required.iter().any(|artifact| {
      !context
        .entries()
        .iter()
        .any(|entry| matches!(entry.source(), FactoryContextReference::Artifact(reference) if reference == *artifact))
    }) {
      return Err(invalid("research required context"));
    }
    let input = Self {
      schema_version: RESEARCH_CONTRACT_VERSION,
      admission_limits: admitted.limits().clone(),
      policy_digest,
      accepted_triage,
      context,
      retrieval,
      details,
    };
    if serde_json::to_vec(&input)
      .map_err(|_| invalid("research input serialization"))?
      .len()
      > super::MAX_RESEARCH_CONTRACT_BYTES
    {
      return Err(invalid("research input bytes"));
    }
    Ok(input)
  }

  /// Restores bounded stored bytes through the same admission and context checks.
  pub fn restore(
    bytes: &[u8],
    work: &WorkEnvelope,
    admitted: &AdmittedFlow,
    policy_digest: FactoryDigest,
  ) -> Result<Self, FactoryError> {
    if bytes.len() > super::MAX_RESEARCH_CONTRACT_BYTES {
      return Err(invalid("research input bytes"));
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Wire {
      schema_version: u16,
      admission_limits: FlowAdmissionLimits,
      policy_digest: FactoryDigest,
      accepted_triage: ClassificationReceipt,
      context: ContextManifest,
      retrieval: Vec<RetrievalReceipt>,
      details: ResearchDetails,
    }
    let wire: Wire = serde_json::from_slice(bytes).map_err(|_| invalid("research input schema"))?;
    if wire.schema_version != RESEARCH_CONTRACT_VERSION
      || &wire.admission_limits != admitted.limits()
      || wire.policy_digest != policy_digest
    {
      return Err(invalid("research input version"));
    }
    Self::new(
      work,
      admitted,
      wire.accepted_triage,
      wire.context,
      wire.retrieval,
      wire.details,
      policy_digest,
    )
  }

  /// Returns the immutable Work identity.
  #[must_use]
  pub fn work_id(&self) -> WorkEnvelopeId {
    self.accepted_triage.eligibility.input.work_id()
  }
  /// Returns the exact base subject.
  #[must_use]
  pub fn subject(&self) -> &ExactSubject {
    self.accepted_triage.eligibility.input.subject()
  }
  /// Returns the exact Factory Configuration version.
  #[must_use]
  pub fn configuration(&self) -> &FactoryConfigurationRef {
    self.accepted_triage.eligibility.input.configuration()
  }
  /// Returns the accepted Work kind.
  #[must_use]
  pub fn kind(&self) -> WorkKind {
    *self.accepted_triage.result.classification().work_kind.value()
  }
  /// Returns complete accepted triage provenance and policy.
  #[must_use]
  pub const fn accepted_triage(&self) -> &ClassificationReceipt {
    &self.accepted_triage
  }
  /// Returns the exact frozen context projection.
  #[must_use]
  pub const fn context(&self) -> &ContextManifest {
    &self.context
  }
  /// Returns the admitted aggregate resource and permission ceilings.
  #[must_use]
  pub const fn admission_limits(&self) -> &FlowAdmissionLimits {
    &self.admission_limits
  }
  /// Returns the immutable research policy selected before dispatch.
  #[must_use]
  pub const fn policy_digest(&self) -> FactoryDigest {
    self.policy_digest
  }
  /// Returns applicable exact Retrieval Receipts.
  #[must_use]
  pub fn retrieval(&self) -> &[RetrievalReceipt] {
    &self.retrieval
  }
  /// Returns branch-specific frozen inputs.
  #[must_use]
  pub const fn details(&self) -> &ResearchDetails {
    &self.details
  }
  /// Returns the canonical complete input identity.
  pub fn digest(&self) -> Result<FactoryDigest, FactoryError> {
    digest("octacity.factory.research-input.v1", self)
  }
}

pub(super) fn contains_source(context: &ContextManifest, source: &ResearchSourceReference) -> bool {
  context.entries().iter().any(|entry| {
    entry.source_kind() == source.source_kind
      && entry.logical_identity() == &source.logical_identity
      && entry.content_digest() == source.content_digest
  })
}
