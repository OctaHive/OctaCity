//! Exact candidate publication as an installed trusted action, independent of Flow names.
use crate::*;
use async_trait::async_trait;
use octacity_server_domain::Timestamp;
pub(crate) use octacity_server_factory::PUBLISH_CANDIDATE_ACTION;
use octacity_server_factory::*;
use octacity_server_store::FactoryRunStore;
use serde::Deserialize;
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
    if !Self::is_safe_review_url(&url) {
      return Err(ApplicationError::invalid());
    }
    self.review_url = Some(url.into());
    Ok(self)
  }

  pub(crate) fn is_safe_review_url(url: &url::Url) -> bool {
    url.scheme() == "https"
      && url.host_str().is_some()
      && url.username().is_empty()
      && url.password().is_none()
      && url.query().is_none()
      && url.fragment().is_none()
      && url.as_str().len() <= 2048
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
  /// The retained checks do not authorize the selected candidate.
  CandidateProofMismatch,
}
const PUBLICATION_CONFLICT: &str = "publication_conflict";
const PUBLICATION_REJECTIONS: [FactoryPublicationRejection; 3] = [
  FactoryPublicationRejection::ProviderRejected,
  FactoryPublicationRejection::InvalidReviewMetadata,
  FactoryPublicationRejection::ValidationExpired,
];

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
  runs: Option<Arc<dyn FactoryRunStore>>,
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
  review_valid_until: Option<FlowDataMapping>,
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

  fn rejection_reasons(&self) -> impl Iterator<Item = FactoryPublicationRejection> {
    PUBLICATION_REJECTIONS.into_iter().chain(
      self
        .review_valid_until
        .as_ref()
        .map(|_| FactoryPublicationRejection::CandidateProofMismatch),
    )
  }
}
/// Resolves the same typed frozen policy used by publication, without dispatching the publisher.
pub(crate) fn publication_target(binding: &FlowActionBinding) -> Result<ImmutableReference, ApplicationError> {
  Ok(Parameters::parse(binding)?.target)
}

fn parse_expiry(value: &serde_json::Value) -> Result<Timestamp, ApplicationError> {
  value
    .as_i64()
    .filter(|value| *value >= 0)
    .and_then(|value| Timestamp::from_unix_millis(value).ok())
    .ok_or_else(ApplicationError::invalid)
}

fn valid_expiry_mapping(mapping: &FlowValueMapping) -> bool {
  match mapping {
    FlowValueMapping::Constant { value } => parse_expiry(value).is_ok(),
    FlowValueMapping::IfPresent { then, otherwise, .. } => {
      valid_expiry_mapping(then) && valid_expiry_mapping(otherwise)
    }
    FlowValueMapping::Pointer { .. } => true,
    _ => false,
  }
}

#[derive(Clone, Copy)]
enum PublicationProof {
  Validation,
  Review,
}

#[derive(Deserialize)]
struct CandidateReview {
  candidate_revision: octacity_server_domain::ImmutableRevision,
  candidate_digest: FactoryDigest,
  input_digest: FactoryDigest,
  quality: ReviewQuality,
  findings: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct ReviewQuality {
  correctness: AssessmentOutcome,
  maintainability: AssessmentOutcome,
  public_tests: AssessmentOutcome,
  security: AssessmentOutcome,
}

fn code_review_passed(record: &FlowNodeRecord, candidate: &CandidateSubject) -> bool {
  let Ok(report) = CandidateReview::deserialize(record.payload().value()) else {
    return false;
  };
  record.primary_build_output().is_some()
    && report.candidate_revision == *candidate.candidate_revision()
    && report.candidate_digest == candidate.changeset_digest()
    && record
      .input()
      .digest()
      .is_ok_and(|digest| digest == report.input_digest)
    && [
      report.quality.correctness,
      report.quality.maintainability,
      report.quality.public_tests,
      report.quality.security,
    ]
    .into_iter()
    .all(|outcome| outcome == AssessmentOutcome::Satisfied)
    && report.findings.is_empty()
}

/// Uses the same projection walk at authoring and publication. At publication,
/// each hop resolves through the immutable source references of its own input.
struct PublicationProofTrace<'a> {
  definition: &'a FlowDefinition,
  schemas: &'a [FlowDataSchema],
  proof: PublicationProof,
  records: &'a [FlowNodeRecord],
  candidate: Option<&'a CandidateSubject>,
  remaining: usize,
}

impl<'history> PublicationProofTrace<'history> {
  fn selects(&mut self, node: &FlowNodeDefinition, input: Option<&FlowNodeInput>, mapping: &FlowDataMapping) -> bool {
    mapping.selects_only("", &mut |path| self.input(node, input, path))
  }

  fn input(&mut self, node: &FlowNodeDefinition, frozen: Option<&FlowNodeInput>, path: &str) -> bool {
    if self.remaining == 0 {
      return false;
    }
    self.remaining -= 1;
    node.input_binding().is_some_and(|binding| {
      binding.selects_only(path, |source, tail| match source {
        FlowInputSource::Record {
          node,
          outcome,
          view,
          path,
        } => self.source(node, outcome, *view, &format!("{path}{tail}"), frozen),
        FlowInputSource::Predecessor { view, path } => {
          let records = self.records;
          let mut incoming = self.definition.transitions().iter().filter(|edge| {
            (matches!(edge.target(), FlowTransitionTarget::Node(key) if key == node.key())
              || edge.exhausted_node() == Some(node.key()))
              && frozen
                .is_none_or(|input| Self::frozen_record(records, input, edge.predecessor(), edge.outcome()).is_some())
          });
          let path = format!("{path}{tail}");
          let mut check = |edge: &FlowTransition| self.source(edge.predecessor(), edge.outcome(), *view, &path, frozen);
          incoming.next().is_some_and(&mut check) && incoming.all(check)
        }
        _ => false,
      })
    })
  }

  fn frozen_record(
    records: &'history [FlowNodeRecord],
    input: &FlowNodeInput,
    key: &FactoryKey,
    outcome: &FactoryKey,
  ) -> Option<&'history FlowNodeRecord> {
    let mut records = records.iter().filter(|record| {
      record.input().node() == key
        && record.outcome() == outcome
        && input.sources().iter().any(|source| {
          source.node_attempt_id() == record.node_attempt_id()
            && record.digest().is_ok_and(|digest| digest == source.digest())
        })
    });
    let record = records.next()?;
    records.next().is_none().then_some(record)
  }

  fn declares_fields(&self, schema: &ImmutableReference, required: &[&str]) -> bool {
    self.schemas.iter().any(|entry| {
      entry.reference() == schema
        && matches!(entry.shape(), FlowValueSchema::Object { fields } if required.iter().all(|name| {
          fields.iter().any(|(key, field)| key.as_str() == *name && field.is_required())
        }))
    })
  }

  fn source(
    &mut self,
    key: &FactoryKey,
    outcome: &FactoryKey,
    view: FlowRecordView,
    path: &str,
    input: Option<&FlowNodeInput>,
  ) -> bool {
    let Some(producer) = self.definition.node(key) else {
      return false;
    };
    let record = match input {
      Some(input) => match Self::frozen_record(self.records, input, key, outcome) {
        Some(record) => Some(record),
        None => return false,
      },
      None => None,
    };
    let Some(result) = producer
      .outcome(outcome)
      .filter(|outcome| outcome.kind() == FlowOutcomeKind::Success)
    else {
      return false;
    };
    let frozen = record.map(FlowNodeRecord::input);
    match view {
      FlowRecordView::InputPayload => self.input(producer, frozen, path),
      FlowRecordView::Payload => FlowGateProgram::from_node(producer).is_ok_and(|program| {
        program.outcome_selects_only(outcome, path, &mut |path| self.input(producer, frozen, path))
      }),
      FlowRecordView::Metadata => {
        self.candidate.is_none_or(|candidate| {
          record.is_some_and(|record| {
            record
              .input()
              .context()
              .and_then(|context| context.subject().candidate())
              == Some(candidate)
          })
        }) && match self.proof {
          PublicationProof::Review => {
            path == "/build/primary_fresh_until"
              && producer.kind() == FlowNodeKind::Reasoning
              && self.declares_fields(
                result.schema(),
                &[
                  "candidate_revision",
                  "candidate_digest",
                  "input_digest",
                  "quality",
                  "findings",
                ],
              )
              && producer
                .build_bindings()
                .any(|build| &build.result_outcome == outcome && build.result_output_kind == EvidenceOutputKind::Report)
              && self
                .candidate
                .is_none_or(|candidate| record.is_some_and(|record| code_review_passed(record, candidate)))
          }
          PublicationProof::Validation => path
            .strip_prefix("/build/outputs/")
            .and_then(|path| path.strip_suffix("/data/payload/value/fresh_until"))
            .is_some_and(|output| {
              if producer.kind() != FlowNodeKind::BuildCommand
                || !self.declares_fields(result.schema(), &["candidate", "tests", "classes"])
              {
                return false;
              }
              let mut bindings = producer.build_bindings();
              let verifies = |build: &FlowBuildBinding| {
                &build.result_outcome == outcome
                  && build.result_output_kind == EvidenceOutputKind::Report
                  && build.evidence.iter().any(|evidence| {
                    evidence.kind().as_str() == output
                      && evidence.output_kind() == EvidenceOutputKind::Report
                      && self.declares_fields(evidence.schema(), &["accepted", "report_digest", "fresh_until"])
                  })
              };
              bindings.next().is_some_and(verifies)
                && bindings.all(verifies)
                && self.candidate.is_none_or(|candidate| {
                  record.is_some_and(|record| {
                    crate::factory::validation::factory_protected_validation::protected_validation_passed(
                      record, output, candidate,
                    )
                  })
                })
            }),
        }
      }
      FlowRecordView::PrimaryArtifact => false,
    }
  }
}

/// Validates the installed publication target, emitted receipt, and explicit inputs.
pub(crate) fn validate_publication_selection(
  definition: &FlowDefinition,
  node: &octacity_server_factory::FlowNodeDefinition,
  binding: &FlowActionBinding,
  choices: &octacity_server_factory::FactoryConfigurationChoices,
  schemas: &[FlowDataSchema],
  authoring_flow: bool,
) -> Result<(), &'static str> {
  let parameters = Parameters::parse(binding).map_err(|_| "/action/parameters")?;
  let choice = choices
    .candidate_publications()
    .iter()
    .find(|choice| {
      &choice.plugin == binding.plugin()
        && choice.target == parameters.target
        && (!parameters.require_review_url || choice.review_url)
    })
    .ok_or("/action/target")?;
  if !choice.permissions.is_no_broader_than(node.permissions()) {
    return Err("/permissions");
  }
  validate_publication_outcomes(&parameters, node.outcomes(), &choice.outcome).map_err(|_| "/action/parameters")?;
  let receipt = schemas
    .iter()
    .find(|schema| schema.reference() == choice.outcome.schema())
    .ok_or("/outcomes")?;
  if !publication_receipt_is_compatible(receipt, parameters.require_review_url) {
    return Err("/outcomes");
  }
  let text = FlowValueSchema::String {
    min_bytes: 0,
    max_bytes: MAX_FLOW_DATA_BYTES,
  };
  let metadata = object([("title", text.clone()), ("summary", text)]).map_err(|_| "/action/parameters")?;
  let timestamp = FlowValueSchema::Integer {
    minimum: i64::MIN,
    maximum: i64::MAX,
  };
  let input = node
    .input_schema()
    .and_then(|reference| schemas.iter().find(|schema| schema.reference() == reference))
    .map(FlowDataSchema::shape);
  let requires_freshness = authoring_flow && choice.review_url && choice.receipt_schema.is_some();
  for (mapping, path, proof) in [
    (
      &parameters.valid_until,
      "/action/parameters/valid_until",
      PublicationProof::Validation,
    ),
    (
      &parameters.review_valid_until,
      "/action/parameters/review_valid_until",
      PublicationProof::Review,
    ),
  ] {
    if !mapping.as_ref().map_or(!requires_freshness, |mapping| {
      input.is_some_and(|shape| mapping.fits(shape, &timestamp))
        && if requires_freshness {
          (PublicationProofTrace {
            definition,
            schemas,
            proof,
            records: &[],
            candidate: None,
            remaining: MAX_FLOW_SCHEMA_ENTRIES,
          })
          .selects(node, None, mapping)
        } else {
          valid_expiry_mapping(mapping.value())
        }
    }) {
      return Err(path);
    }
  }
  if parameters
    .review_metadata
    .as_ref()
    .is_some_and(|mapping| !input.is_some_and(|shape| mapping.fits(shape, &metadata)))
  {
    return Err("/action/parameters/review_metadata");
  }
  if let Some(key) = &parameters.conflict_outcome {
    publication_failure_payload(node.outcomes(), schemas, key, PUBLICATION_CONFLICT).map_err(|_| "/outcomes")?;
  }
  if let Some(key) = &parameters.rejected_outcome {
    for reason in parameters.rejection_reasons() {
      publication_failure_payload(node.outcomes(), schemas, key, reason).map_err(|_| "/outcomes")?;
    }
  }
  if node.outcomes().iter().any(|outcome| {
    matches!(
      outcome.issue_presentation(),
      Some(octacity_server_factory::FlowIssuePresentation::PublicationV1 { .. })
    ) && !outcome.matches_contract(&choice.outcome)
  }) {
    return Err("/outcomes/issue_presentation");
  }
  let candidate_source = matches!(
    node.code(),
    Some(octacity_server_factory::FlowCodeBinding::Source {
      revision: octacity_server_factory::FlowCodeRevision::AcceptedCandidate { .. },
      ..
    })
  );
  if !node.context_binding().is_some_and(|context| {
    (context.selects_candidate() || candidate_source)
      && context.guarantees_artifact(&parameters.bundle)
      && context.guarantees_artifact(&parameters.manifest)
  }) {
    return Err("/context_binding");
  }
  Ok(())
}

fn validate_publication_outcomes(
  parameters: &Parameters,
  outcomes: &[FlowOutcomeDefinition],
  receipt: &FlowOutcomeDefinition,
) -> Result<(), ApplicationError> {
  if !outcomes.iter().any(|outcome| outcome.matches_contract(receipt))
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
impl<P> FactoryCandidatePublicationAction<P> {
  /// Selects the trusted publisher and the configured successful receipt outcome.
  #[must_use]
  pub fn new(identity: ImmutableReference, publisher: Arc<P>, outcome: FlowOutcomeDefinition) -> Self {
    Self {
      identity,
      publisher,
      outcome,
      runs: None,
    }
  }

  /// Reads exact, frozen source records for publication that requires independent review.
  #[must_use]
  pub fn with_run_store(mut self, runs: Arc<dyn FactoryRunStore>) -> Self {
    self.runs = Some(runs);
    self
  }

  async fn check_candidate_proofs(
    &self,
    input: &FlowNodeInput,
    candidate: &CandidateSubject,
    validation: Option<&FlowDataMapping>,
    review: &FlowDataMapping,
  ) -> Result<(), ApplicationError> {
    let mismatch = || ApplicationError::PublicationRejected(FactoryPublicationRejection::CandidateProofMismatch);
    let snapshot = self
      .runs
      .as_ref()
      .ok_or_else(mismatch)?
      .factory_run_snapshot(input.factory_run_id())
      .await?;
    let definition = snapshot
      .admitted_flow
      .closure()
      .definition(input.definition())
      .ok_or_else(mismatch)?;
    let node = definition.node(input.node()).ok_or_else(mismatch)?;
    for (proof, mapping) in [
      (PublicationProof::Validation, validation.ok_or_else(mismatch)?),
      (PublicationProof::Review, review),
    ] {
      let mut trace = PublicationProofTrace {
        definition,
        schemas: snapshot.admitted_flow.data_schemas(),
        proof,
        records: &snapshot.flow.data.records,
        candidate: Some(candidate),
        remaining: MAX_FLOW_SCHEMA_ENTRIES,
      };
      if !trace.selects(node, Some(input), mapping) {
        return Err(mismatch());
      }
    }
    Ok(())
  }

  fn request(
    &self,
    request: &FactoryFlowActionRequest<'_>,
    parameters: &Parameters,
  ) -> Result<FactoryCandidatePublicationRequest, ApplicationError> {
    let context = request.input.context().ok_or_else(ApplicationError::invalid)?;
    let publish_until = [parameters.valid_until.as_ref(), parameters.review_valid_until.as_ref()]
      .into_iter()
      .flatten()
      .map(|validity| {
        let value = validity
          .project_raw(request.input.payload().value())
          .map_err(|_| ApplicationError::invalid())?;
        parse_expiry(&value)
      })
      .try_fold(None, |earliest: Option<Timestamp>, cutoff| {
        let cutoff = cutoff?;
        Ok::<_, ApplicationError>(Some(earliest.map_or(cutoff, |current| current.min(cutoff))))
      })?;
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
      target: parameters.target.clone(),
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
    if binding.action().as_str() != PUBLISH_CANDIDATE_ACTION {
      return Err(ApplicationError::invalid());
    }
    validate_publication_outcomes(&parameters, outcomes, &self.outcome)
  }
  async fn cancel(
    &self,
    request: FactoryFlowActionRequest<'_>,
    requested_at: Timestamp,
  ) -> Result<Option<BudgetUsage>, ApplicationError> {
    let parameters = Parameters::parse(request.binding)?;
    self
      .publisher
      .cancel(&self.request(&request, &parameters)?, requested_at)
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
      .map(|key| publication_failure(&request, key, PUBLICATION_CONFLICT))
      .transpose()?;
    let rejected = parameters
      .rejected_outcome
      .as_ref()
      .map(|key| {
        parameters
          .rejection_reasons()
          .map(|reason| publication_failure(&request, key, reason).map(|result| (reason, result)))
          .collect::<Result<Vec<_>, _>>()
      })
      .transpose()?;
    let observed_at = request.observed_at;
    let intent = self.request(&request, &parameters)?;
    if observed_at >= intent.deadline {
      return Err(ApplicationError::invalid());
    }
    // Establish compatibility with the entire producer contract before effects.
    if !schema.accepts_all(&publication_shape(
      FlowValueSchema::Enum {
        values: vec![serde_json::json!(intent.candidate.candidate_revision())],
      },
      FlowValueSchema::Enum {
        values: vec![serde_json::json!(intent.candidate.changeset_digest())],
      },
      intent.require_review_url,
    )?) {
      return Err(ApplicationError::invalid());
    }
    let published = match self.publisher.observe(&intent).await {
      Ok(Some(publication)) => Ok(publication),
      Ok(None) if intent.publish_until.is_some_and(|cutoff| observed_at >= cutoff) => Err(
        ApplicationError::PublicationRejected(FactoryPublicationRejection::ValidationExpired),
      ),
      Ok(None) => match async {
        if let Some(review) = &parameters.review_valid_until {
          self
            .check_candidate_proofs(
              request.input,
              &intent.candidate,
              parameters.valid_until.as_ref(),
              review,
            )
            .await?;
        }
        self.publisher.publish(&intent).await
      }
      .await
      {
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
    let discovery = match &publication.facts.review_url {
      Some(url) => FlowDiscoveryFacts::candidate_with_review(publication.candidate.clone(), url.clone()),
      None => FlowDiscoveryFacts::candidate(publication.candidate.clone()),
    };
    Ok(FactoryFlowActionObservation {
      discovery: Some(discovery),
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
  Ok(FactoryFlowActionObservation {
    discovery: None,
    outcome: key.clone(),
    payload: publication_failure_payload(request.outcomes, request.schemas, key, reason)?,
    usage: BudgetUsage::default(),
    outbox: vec![],
  })
}

fn publication_failure_payload(
  outcomes: &[FlowOutcomeDefinition],
  schemas: &[FlowDataSchema],
  key: &FactoryKey,
  reason: impl serde::Serialize,
) -> Result<FlowPayload, ApplicationError> {
  let outcome = outcomes
    .iter()
    .find(|outcome| outcome.key() == key)
    .ok_or_else(ApplicationError::invalid)?;
  let schema = schemas
    .iter()
    .find(|schema| schema.reference() == outcome.schema())
    .ok_or_else(ApplicationError::invalid)?;
  FlowPayload::new(schema, serde_json::json!({"reason":reason})).map_err(|_| ApplicationError::invalid())
}

fn publication_payload(
  receipt: FactoryDigest,
  candidate: &CandidateSubject,
  facts: &FactoryPublicationFacts,
) -> serde_json::Value {
  serde_json::json!({"receipt":receipt,"candidate_revision":candidate.candidate_revision(),"changeset_digest":candidate.changeset_digest(),"external":facts})
}

/// Checks the invariant receipt shape before offering or saving a publisher.
pub(crate) fn publication_receipt_is_compatible(schema: &FlowDataSchema, require_review_url: bool) -> bool {
  let FlowValueSchema::Object { fields } = schema.shape() else {
    return false;
  };
  let Some(revision) = fields
    .get(&FactoryKey::new("candidate_revision").expect("static key"))
    .map(FlowFieldSchema::value)
  else {
    return false;
  };
  let digest_shape = FlowValueSchema::String {
    min_bytes: 64,
    max_bytes: 64,
  };
  let changeset = fields
    .get(&FactoryKey::new("changeset_digest").expect("static key"))
    .map(FlowFieldSchema::value);
  let changeset = match changeset {
    Some(shape @ FlowValueSchema::Enum { values })
      if values.iter().all(|value| {
        value
          .as_str()
          .is_some_and(|value| value.parse::<FactoryDigest>().is_ok())
      }) =>
    {
      shape.clone()
    }
    _ => digest_shape,
  };
  let valid = match revision {
    FlowValueSchema::String { max_bytes, .. } => *max_bytes > 0,
    FlowValueSchema::Enum { values } => values.iter().all(|value| value.as_str().is_some_and(|s| !s.is_empty())),
    _ => false,
  };
  // Context supplies the revision and changeset. Legacy receipts may pin an
  // exact digest; the executor still validates the actual candidate value.
  valid
    && publication_shape(revision.clone(), changeset, require_review_url)
      .is_ok_and(|producer| schema.accepts_all(&producer))
}

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

fn publication_shape(
  revision: FlowValueSchema,
  changeset_digest: FlowValueSchema,
  require_review_url: bool,
) -> Result<FlowValueSchema, ApplicationError> {
  let text = |min_bytes, max_bytes| FlowValueSchema::String { min_bytes, max_bytes };
  let mut external = vec![
    ("branch_reference", text(1, 1024)),
    ("remote_head", revision.clone()),
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
    ("candidate_revision", revision),
    ("changeset_digest", changeset_digest),
    ("external", object(external)?),
  ])
}
