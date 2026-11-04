//! Exact candidate publication as an installed trusted action, independent of Flow names.
use crate::*;
pub(crate) const PUBLISH_CANDIDATE_ACTION: &str = "publish_candidate";
use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_factory::*;
use std::sync::Arc;

/// Frozen publication intent; the connector receives no reasoning session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryCandidatePublicationRequest {
  /// Stable identity reused after an unknown response.
  pub operation_id: FactoryDigest,
  /// Exact Work and correction cycle owning this publication.
  pub work_id: WorkEnvelopeId,
  /// Exact cycle; receipts from a superseded cycle cannot authorize this action.
  pub cycle_id: WorkflowCycleId,
  /// Complete frozen configured input and its trusted record provenance.
  /// This proof binds the publication to the selected owner decisions.
  pub accepted_input_digest: FactoryDigest,
  /// Captured source identity, including the original immutable base.
  pub candidate: CandidateSubject,
  /// Self-contained candidate bytes selected by the declared Context.
  pub bundle: FactoryArtifactReference,
  /// Exact capture manifest selected by the declared Context.
  pub manifest: FactoryArtifactReference,
  /// Operator-selected delivery policy; never a model-selected branch or merge.
  pub target: ImmutableReference,
  /// Hard authority ceiling to enforce before credential resolution or effects.
  pub permissions: FactoryPermissionSet,
  /// Resource ceiling inherited from the action.
  pub budget: BudgetLimit,
  /// Original attempt deadline, unchanged on replay.
  pub deadline: Timestamp,
  /// Original persisted attempt start, used for cumulative elapsed accounting.
  pub started_at: Timestamp,
  /// Current observer authority; a replacement observer cannot extend the hard deadline.
  pub authority_expires_at: Timestamp,
}
/// Observed durable publication of the exact requested candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryCandidatePublication {
  /// Original publication operation observed by the connector.
  pub operation_id: FactoryDigest,
  /// Actual published subject; substitution is rejected by the action.
  pub candidate: CandidateSubject,
  /// Retained connector receipt for the original operation.
  pub receipt: FactoryDigest,
  /// Verified external identities retained in the common append-only node record.
  pub facts: FactoryPublicationFacts,
  /// Measured usage, excluding the owner's attempt charge.
  pub usage: BudgetUsage,
}
/// External review facts are observations; they never authorize a Flow transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryReviewState {
  /// Candidate is available for review.
  Open,
  /// Externally closed without merge.
  Closed,
  /// The same candidate was merged externally; this action cannot merge it.
  MergedExternally,
}
/// Bounded provider-neutral publication facts, with no provider URL or credential.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FactoryPublicationFacts {
  branch_reference: FactorySafeText,
  remote_head: octacity_server_domain::ImmutableRevision,
  review_identity: FactorySafeText,
  review_state: FactoryReviewState,
}
impl FactoryPublicationFacts {
  /// Accepts only bounded non-URL external identities and an exact immutable head.
  pub fn new(
    branch: String,
    head: octacity_server_domain::ImmutableRevision,
    review: String,
    state: FactoryReviewState,
  ) -> Result<Self, ApplicationError> {
    if branch.is_empty()
      || branch.len() > 1024
      || branch.contains("://")
      || branch.chars().any(char::is_control)
      || review.is_empty()
      || review.len() > 256
      || !review
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
    {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      branch_reference: FactorySafeText::new(branch).map_err(|_| ApplicationError::invalid())?,
      remote_head: head,
      review_identity: FactorySafeText::new(review).map_err(|_| ApplicationError::invalid())?,
      review_state: state,
    })
  }
}
/// Protected write boundary separate from read-only VCS and repository Jobs.
/// Implementations verify bundle/manifest bytes, candidate digest and original
/// base ancestry, enforce the target policy and authority, and deduplicate the
/// complete request by operation identity. Changed remote ancestry is a conflict;
/// neither observing nor publishing may silently rebase or substitute a candidate.
#[async_trait]
pub trait FactoryCandidatePublisher: Send + Sync {
  /// Observes only this exact logical operation, verifying its target and ancestry.
  async fn observe(
    &self,
    request: &FactoryCandidatePublicationRequest,
  ) -> Result<Option<FactoryCandidatePublication>, ApplicationError>;
  /// Publishes the verified exact candidate after observing absence; no merge authority.
  async fn publish(
    &self,
    request: &FactoryCandidatePublicationRequest,
  ) -> Result<FactoryCandidatePublication, ApplicationError>;
  /// Stops or observes only the original operation, without creating a new write.
  async fn cancel(
    &self,
    request: &FactoryCandidatePublicationRequest,
    requested_at: Timestamp,
  ) -> Result<Option<BudgetUsage>, ApplicationError>;
}
/// Reusable publication capability installed under an immutable plugin identity.
pub struct FactoryCandidatePublicationAction<P> {
  identity: ImmutableReference,
  publisher: Arc<P>,
  outcome: FlowOutcomeDefinition,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameters {
  bundle: FactoryKey,
  manifest: FactoryKey,
  target: ImmutableReference,
  #[serde(default)]
  valid_until: Option<FlowDataMapping>,
}
/// Resolves the same typed frozen policy used by publication, without dispatching the publisher.
pub(crate) fn publication_target(binding: &FlowActionBinding) -> Result<ImmutableReference, ApplicationError> {
  let parameters: Parameters =
    serde_json::from_value(binding.parameters().clone()).map_err(|_| ApplicationError::invalid())?;
  Ok(parameters.target)
}
impl<P> FactoryCandidatePublicationAction<P> {
  /// Selects the trusted publisher and the configured successful receipt outcome.
  #[must_use]
  pub fn new(identity: ImmutableReference, publisher: Arc<P>, outcome: FlowOutcomeDefinition) -> Self {
    Self {
      identity,
      publisher,
      outcome,
    }
  }
  fn request(
    &self,
    request: FactoryFlowActionRequest<'_>,
  ) -> Result<FactoryCandidatePublicationRequest, ApplicationError> {
    let parameters: Parameters =
      serde_json::from_value(request.binding.parameters().clone()).map_err(|_| ApplicationError::invalid())?;
    let context = request.input.context().ok_or_else(ApplicationError::invalid)?;
    let mut deadline = request.deadline;
    if let Some(validity) = parameters.valid_until {
      let schema = FlowDataSchema::new(
        FactoryKey::new("publication.validity").map_err(|_| ApplicationError::invalid())?,
        FactoryKey::new("v1").map_err(|_| ApplicationError::invalid())?,
        FlowValueSchema::Integer {
          minimum: 0,
          maximum: octacity_server_domain::MAX_TIMESTAMP_MILLIS,
        },
      )
      .map_err(|_| ApplicationError::invalid())?;
      let value = validity
        .project(request.input.payload(), &schema)
        .map_err(|_| ApplicationError::invalid())?;
      deadline = deadline.min(
        Timestamp::from_unix_millis(value.value().as_i64().ok_or_else(ApplicationError::invalid)?)
          .map_err(|_| ApplicationError::invalid())?,
      );
    }
    let candidate = context.subject().candidate().ok_or_else(ApplicationError::invalid)?;
    if candidate.exact()
      != request
        .input
        .exact_resource()
        .map_err(|_| ApplicationError::invalid())?
    {
      return Err(ApplicationError::invalid());
    }
    let artifact = |name: &FactoryKey| {
      let mut entries = context
        .entries()
        .iter()
        .filter(|entry| entry.logical_identity() == name);
      let entry = entries.next().ok_or_else(ApplicationError::invalid)?;
      if entries.next().is_some() {
        return Err(ApplicationError::invalid());
      }
      match entry.source() {
        FactoryContextReference::Artifact(reference) => Ok(reference.clone()),
        _ => Err(ApplicationError::invalid()),
      }
    };
    let bundle = artifact(&parameters.bundle)?;
    let manifest = artifact(&parameters.manifest)?;
    if bundle.artifact_id() == manifest.artifact_id() {
      return Err(ApplicationError::invalid());
    }
    Ok(FactoryCandidatePublicationRequest {
      operation_id: request.operation_id,
      work_id: request.work_id,
      cycle_id: request.input.cycle_id(),
      accepted_input_digest: request.input.digest().map_err(|_| ApplicationError::invalid())?,
      candidate: candidate.clone(),
      bundle,
      manifest,
      target: parameters.target,
      permissions: request.permissions.clone(),
      budget: request.budget,
      deadline,
      started_at: request.started_at,
      authority_expires_at: request.authority_expires_at,
    })
  }
}
#[async_trait]
impl<P: FactoryCandidatePublisher> FactoryFlowActionPlugin for FactoryCandidatePublicationAction<P> {
  fn identity(&self) -> ImmutableReference {
    self.identity.clone()
  }
  fn validate(&self, binding: &FlowActionBinding, outcomes: &[FlowOutcomeDefinition]) -> Result<(), ApplicationError> {
    let parameters: Parameters =
      serde_json::from_value(binding.parameters().clone()).map_err(|_| ApplicationError::invalid())?;
    if binding.action().as_str() != PUBLISH_CANDIDATE_ACTION
      || !outcomes.contains(&self.outcome)
      || parameters.bundle == parameters.manifest
    {
      return Err(ApplicationError::invalid());
    }
    Ok(())
  }
  async fn cancel(
    &self,
    request: FactoryFlowActionRequest<'_>,
    requested_at: Timestamp,
  ) -> Result<Option<BudgetUsage>, ApplicationError> {
    self.publisher.cancel(&self.request(request)?, requested_at).await
  }
  async fn execute(
    &self,
    request: FactoryFlowActionRequest<'_>,
  ) -> Result<FactoryFlowActionObservation, ApplicationError> {
    let schema = request
      .schemas
      .iter()
      .find(|schema| schema.reference() == self.outcome.schema())
      .ok_or_else(ApplicationError::invalid)?;
    let observed_at = request.observed_at;
    let intent = self.request(request)?;
    if observed_at >= intent.deadline {
      return Err(ApplicationError::invalid());
    }
    // Establish compatibility with the entire producer contract before effects.
    if !schema.accepts_all(&publication_shape(&intent.candidate)?) {
      return Err(ApplicationError::invalid());
    }
    let publication = match self.publisher.observe(&intent).await? {
      Some(publication) => publication,
      None => self.publisher.publish(&intent).await?,
    };
    if publication.operation_id != intent.operation_id
      || publication.candidate != intent.candidate
      || publication.facts.remote_head != *intent.candidate.candidate_revision()
      || publication.facts.review_state == FactoryReviewState::Closed
      || publication.usage.attempts != 0
      || publication.usage.validate(intent.budget).is_err()
    {
      return Err(ApplicationError::invalid());
    }
    Ok(FactoryFlowActionObservation {
      discovery: Some(FlowDiscoveryFacts::candidate(publication.candidate.clone())),
      outcome: self.outcome.key().clone(),
      payload: FlowPayload::new(
        schema,
        publication_payload(publication.receipt, &publication.candidate, &publication.facts),
      )
      .map_err(|_| ApplicationError::invalid())?,
      usage: publication.usage,
      outbox: vec![],
    })
  }
}
fn publication_payload(
  receipt: FactoryDigest,
  candidate: &CandidateSubject,
  facts: &FactoryPublicationFacts,
) -> serde_json::Value {
  serde_json::json!({"receipt":receipt,"candidate_revision":candidate.candidate_revision(),"changeset_digest":candidate.changeset_digest(),"external":facts})
}

fn publication_shape(candidate: &CandidateSubject) -> Result<FlowValueSchema, ApplicationError> {
  fn object<const N: usize>(fields: [(&str, FlowValueSchema); N]) -> Result<FlowValueSchema, ApplicationError> {
    Ok(FlowValueSchema::Object {
      fields: fields
        .into_iter()
        .map(|(name, shape)| {
          Ok((
            FactoryKey::new(name).map_err(|_| ApplicationError::invalid())?,
            FlowFieldSchema::required(shape),
          ))
        })
        .collect::<Result<_, ApplicationError>>()?,
    })
  }
  let text = |min_bytes, max_bytes| FlowValueSchema::String { min_bytes, max_bytes };
  let exact = |value| FlowValueSchema::Enum { values: vec![value] };
  object([
    ("receipt", text(64, 64)),
    (
      "candidate_revision",
      exact(serde_json::json!(candidate.candidate_revision())),
    ),
    (
      "changeset_digest",
      exact(serde_json::json!(candidate.changeset_digest())),
    ),
    (
      "external",
      object([
        ("branch_reference", text(1, 1024)),
        ("remote_head", exact(serde_json::json!(candidate.candidate_revision()))),
        ("review_identity", text(1, 256)),
        (
          "review_state",
          FlowValueSchema::Enum {
            values: vec![serde_json::json!("open"), serde_json::json!("merged_externally")],
          },
        ),
      ])?,
    ),
  ])
}
