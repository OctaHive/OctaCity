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
  /// Optional public title and summary from a reviewed Flow projection.
  pub review_metadata: Option<FactoryPublicationReview>,
  /// Whether the configured outcome requires a verified review URL.
  pub require_review_url: bool,
  /// Hard authority ceiling to enforce before credential resolution or effects.
  pub permissions: FactoryPermissionSet,
  /// Resource ceiling inherited from the action.
  pub budget: BudgetLimit,
  /// Original attempt deadline, unchanged on replay.
  pub deadline: Timestamp,
  /// Optional earlier cutoff for forge writes; observing the original operation is read-only.
  pub publish_until: Option<Timestamp>,
  /// Original persisted attempt start, used for cumulative elapsed accounting.
  pub started_at: Timestamp,
  /// Current observer authority; a replacement observer cannot extend the hard deadline.
  pub authority_expires_at: Timestamp,
}
/// Public metadata for review. The target and draft mode belong to the installed policy.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactoryPublicationReview {
  title: String,
  summary: String,
}
impl FactoryPublicationReview {
  /// Title sent to the forge.
  pub fn title(&self) -> &str {
    &self.title
  }

  /// Public summary sent to the forge.
  pub fn summary(&self) -> &str {
    &self.summary
  }

  fn project(mapping: &FlowDataMapping, input: &FlowPayload) -> Result<Self, ApplicationError> {
    let value = mapping
      .project_raw(input.value())
      .map_err(|_| ApplicationError::invalid())?;
    // The installed publisher applies its protocol's metadata rules before effects.
    serde_json::from_value(value).map_err(|_| ApplicationError::invalid())
  }
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
/// Bounded publication facts. A review URL may come from an installed target policy.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FactoryPublicationFacts {
  branch_reference: FactorySafeText,
  remote_head: octacity_server_domain::ImmutableRevision,
  review_identity: FactorySafeText,
  review_state: FactoryReviewState,
  #[serde(skip_serializing_if = "Option::is_none")]
  review_url: Option<String>,
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
      review_url: None,
    })
  }
  /// Adds a credential-free HTTPS URL resolved by the trusted publisher.
  pub fn with_review_url(mut self, url: url::Url) -> Result<Self, ApplicationError> {
    if url.scheme() != "https"
      || url.host_str().is_none()
      || !url.username().is_empty()
      || url.password().is_some()
      || url.query().is_some()
      || url.fragment().is_some()
      || url.as_str().len() > 2048
    {
      return Err(ApplicationError::invalid());
    }
    self.review_url = Some(url.into());
    Ok(self)
  }
}
/// Safe reasons that prevent another publication under the same intent.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactoryPublicationRejection {
  /// The selected provider refused the operation.
  ProviderRejected,
  /// The prepared title or summary violates the Delivery contract.
  InvalidReviewMetadata,
  /// The validation window has closed and no successful publication was observed.
  ValidationExpired,
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
  /// Validates the projected review metadata before effects and publishes the verified
  /// exact candidate after observing absence. No merge authority.
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
  #[serde(default)]
  review_metadata: Option<FlowDataMapping>,
  #[serde(default)]
  require_review_url: bool,
  conflict_outcome: Option<FactoryKey>,
  rejected_outcome: Option<FactoryKey>,
}
impl Parameters {
  fn parse(binding: &FlowActionBinding) -> Result<Self, ApplicationError> {
    serde_json::from_value(binding.parameters().clone()).map_err(|_| ApplicationError::invalid())
  }
}
/// Resolves the same typed frozen policy used by publication, without dispatching the publisher.
pub(crate) fn publication_target(binding: &FlowActionBinding) -> Result<ImmutableReference, ApplicationError> {
  Ok(Parameters::parse(binding)?.target)
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
    parameters: Parameters,
  ) -> Result<FactoryCandidatePublicationRequest, ApplicationError> {
    let context = request.input.context().ok_or_else(ApplicationError::invalid)?;
    let publish_until = if let Some(validity) = parameters.valid_until {
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
      Some(
        Timestamp::from_unix_millis(value.value().as_i64().ok_or_else(ApplicationError::invalid)?)
          .map_err(|_| ApplicationError::invalid())?,
      )
    } else {
      None
    };
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
      review_metadata: parameters
        .review_metadata
        .as_ref()
        .map(|mapping| FactoryPublicationReview::project(mapping, request.input.payload()))
        .transpose()?,
      require_review_url: parameters.require_review_url,
      permissions: request.permissions.clone(),
      budget: request.budget,
      deadline: request.deadline,
      publish_until,
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
    let parameters = Parameters::parse(binding)?;
    if binding.action().as_str() != PUBLISH_CANDIDATE_ACTION
      || !outcomes.contains(&self.outcome)
      || parameters.bundle == parameters.manifest
      || [
        parameters.conflict_outcome.as_ref(),
        parameters.rejected_outcome.as_ref(),
      ]
      .into_iter()
      .flatten()
      .any(|key| {
        !outcomes
          .iter()
          .any(|outcome| outcome.key() == key && outcome.kind() == FlowOutcomeKind::Failure)
      })
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
    let parameters = Parameters::parse(request.binding)?;
    self
      .publisher
      .cancel(&self.request(request, parameters)?, requested_at)
      .await
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
    let parameters = Parameters::parse(request.binding)?;
    let conflict = parameters
      .conflict_outcome
      .as_ref()
      .map(|key| publication_failure(&request, key, "publication_conflict"))
      .transpose()?;
    let rejected = parameters
      .rejected_outcome
      .as_ref()
      .map(|key| {
        [
          FactoryPublicationRejection::ProviderRejected,
          FactoryPublicationRejection::InvalidReviewMetadata,
          FactoryPublicationRejection::ValidationExpired,
        ]
        .map(|reason| publication_failure(&request, key, reason).map(|result| (reason, result)))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
      })
      .transpose()?;
    let observed_at = request.observed_at;
    let intent = self.request(request, parameters)?;
    if observed_at >= intent.deadline {
      return Err(ApplicationError::invalid());
    }
    // Establish compatibility with the entire producer contract before effects.
    if !schema.accepts_all(&publication_shape(&intent.candidate, intent.require_review_url)?) {
      return Err(ApplicationError::invalid());
    }
    let published = match self.publisher.observe(&intent).await {
      Ok(Some(publication)) => Ok(publication),
      Ok(None) if intent.publish_until.is_some_and(|cutoff| observed_at >= cutoff) => Err(
        ApplicationError::PublicationRejected(FactoryPublicationRejection::ValidationExpired),
      ),
      Ok(None) => match self.publisher.publish(&intent).await {
        Err(error @ ApplicationError::Store(StoreError::Unavailable)) => {
          // A committed write can lose its reply. Only observe under the original intent.
          match self.publisher.observe(&intent).await {
            Ok(Some(publication)) => Ok(publication),
            Err(error @ (ApplicationError::PreconditionFailed | ApplicationError::PublicationRejected(_))) => {
              Err(error)
            }
            _ => Err(error),
          }
        }
        result => result,
      },
      Err(error) => Err(error),
    };
    let mut publication = match (published, conflict) {
      (Err(ApplicationError::PreconditionFailed), Some(conflict)) => return Ok(conflict),
      (Err(ApplicationError::PublicationRejected(reason)), _) => {
        return rejected
          .into_iter()
          .flatten()
          .find_map(|(candidate, result)| (candidate == reason).then_some(result))
          .ok_or_else(ApplicationError::invalid);
      }
      (result, _) => result?,
    };
    if !intent.require_review_url {
      publication.facts.review_url = None;
    }
    if publication.operation_id != intent.operation_id
      || publication.candidate != intent.candidate
      || publication.facts.remote_head != *intent.candidate.candidate_revision()
      || (intent.require_review_url && publication.facts.review_url.is_none())
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
fn publication_failure(
  request: &FactoryFlowActionRequest<'_>,
  key: &FactoryKey,
  reason: impl serde::Serialize,
) -> Result<FactoryFlowActionObservation, ApplicationError> {
  let outcome = request
    .outcomes
    .iter()
    .find(|outcome| outcome.key() == key)
    .ok_or_else(ApplicationError::invalid)?;
  let schema = request
    .schemas
    .iter()
    .find(|schema| schema.reference() == outcome.schema())
    .ok_or_else(ApplicationError::invalid)?;
  Ok(FactoryFlowActionObservation {
    discovery: None,
    outcome: key.clone(),
    payload: FlowPayload::new(schema, serde_json::json!({"reason":reason})).map_err(|_| ApplicationError::invalid())?,
    usage: BudgetUsage::default(),
    outbox: vec![],
  })
}

fn publication_payload(
  receipt: FactoryDigest,
  candidate: &CandidateSubject,
  facts: &FactoryPublicationFacts,
) -> serde_json::Value {
  serde_json::json!({"receipt":receipt,"candidate_revision":candidate.candidate_revision(),"changeset_digest":candidate.changeset_digest(),"external":facts})
}

fn publication_shape(
  candidate: &CandidateSubject,
  require_review_url: bool,
) -> Result<FlowValueSchema, ApplicationError> {
  fn object<'a>(
    fields: impl IntoIterator<Item = (&'a str, FlowValueSchema)>,
  ) -> Result<FlowValueSchema, ApplicationError> {
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
  let mut external = vec![
    ("branch_reference", text(1, 1024)),
    ("remote_head", exact(serde_json::json!(candidate.candidate_revision()))),
    ("review_identity", text(1, 256)),
    (
      "review_state",
      FlowValueSchema::Enum {
        values: vec![serde_json::json!("open"), serde_json::json!("merged_externally")],
      },
    ),
  ];
  if require_review_url {
    external.push(("review_url", text(1, 2048)));
  }
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
    ("external", object(external)?),
  ])
}
