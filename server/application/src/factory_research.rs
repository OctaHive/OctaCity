//! Isolated defect and feature research using the ordinary Build application and generic Node Attempts.

use crate::{ApplicationError, FactoryBuildAcceptance, OrdinaryBuildApplication, OrdinaryBuildApplicationError};
use async_trait::async_trait;
use octacity_server_artifacts::{ArtifactRecord, ArtifactState, ArtifactType};
use octacity_server_domain::{AttemptId, BuildId, Timestamp};
use octacity_server_factory::*;
use octacity_server_store::BuildQueryStore;
use std::sync::Arc;

/// Retained ordinary Build output with trusted execution provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryResearchOutputDocument {
  /// Authoritative Artifact publication and retention metadata.
  pub record: ArtifactRecord,
  /// Exact retained bytes, bounded before allocation by the output source.
  pub bytes: Vec<u8>,
  /// Exact schema attested by the trusted output verifier, never inferred from the report format or request.
  pub schema: ImmutableReference,
  /// Executed tool identity from the frozen Job, never from provider JSON.
  pub tool: ImmutableReference,
  /// Executed plugin identity from the frozen Job.
  pub plugin: ImmutableReference,
}

/// Measured execution and independently retained output metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryResearchBuildOutputs {
  /// Complete bounded published outputs for the observed Attempt.
  pub documents: Vec<FactoryResearchOutputDocument>,
  /// Measured Node Attempt consumption, including all ordinary Build retries.
  pub usage: BudgetUsage,
  /// Persisted trusted validation time.
  pub verified_at: Timestamp,
  /// Exclusive freshness deadline, covered by Artifact retention.
  pub fresh_until: Timestamp,
}

/// Reads bounded retained bytes and actual execution facts from ordinary Builds.
#[async_trait]
pub trait FactoryResearchOutputSource: Send + Sync {
  /// Enforces the byte ceiling before reading and returns no provider-selected routing authority.
  /// Schema references must come from trusted schema-specific verification of these exact bytes,
  /// not from provider JSON or from the requested evidence requirement.
  async fn published_outputs(
    &self,
    build: BuildId,
    attempt: AttemptId,
    max_bytes: u64,
  ) -> Result<FactoryResearchBuildOutputs, OrdinaryBuildApplicationError>;
}

/// One observation of a previously persisted isolated research intent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactoryResearchStep {
  /// The same ordinary Build is still executing.
  Waiting,
  /// Ordinary Build failure or cancellation, never a semantic research outcome.
  ExecutionStopped {
    /// Ordinary Build retaining authoritative failure diagnostics.
    build_id: BuildId,
    /// Latest ordinary Attempt, including regular Build retry.
    attempt_id: AttemptId,
    /// Failed or cancelled execution state.
    state: octacity_server_orchestrator::BuildState,
  },
  /// Validated observations and a generic completion for one atomic Flow-owner commit.
  Completed(Box<FactoryResearchCompletion>),
}

/// Immutable result, deterministic proof, and separately evaluated handoff.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryResearchCompletion {
  /// Bounded non-authoritative defect observations or a feature proposal.
  pub result: ResearchResult,
  /// Exact retained report or proposal accepted by deterministic validation.
  pub evidence: AcceptedResearchEvidence,
  /// Fenced generic Node Attempt completion; attempts are charged at creation.
  pub completion: NodeAttemptCompletion,
  /// Aggregate research consumption including this attempt exactly once.
  pub usage: BudgetUsage,
  /// Deterministic successor and non-authoritative Stage Handoff.
  pub acceptance: ResearchAcceptance,
}

/// Stateless bridge: durable Node Attempts own identity; ordinary Builds own execution.
///
/// The Flow owner persists an intent before calling this bridge and observes its
/// committed completion before replaying. This bridge never creates a second
/// executor, a fixed-stage projection, a candidate, or implementation readiness.
pub struct FactoryResearchBuildAdapter<B, O> {
  builds: Arc<B>,
  outputs: Arc<O>,
}
impl<B, O> FactoryResearchBuildAdapter<B, O>
where
  B: OrdinaryBuildApplication + BuildQueryStore,
  O: FactoryResearchOutputSource,
{
  /// Connects the ordinary Build application/read boundary and retained-output reader.
  #[must_use]
  pub fn new(builds: Arc<B>, outputs: Arc<O>) -> Self {
    Self { builds, outputs }
  }

  /// Observes the stable Build under current fenced ownership before any further research admission.
  pub async fn observe_or_dispatch(
    &self,
    intent: &ResearchBuildIntent,
    policy: &ResearchPolicy,
    ownership: FactoryClaimOwnership,
    at: Timestamp,
  ) -> Result<FactoryResearchStep, ApplicationError> {
    intent
      .node()
      .verify_observer(&ownership, at)
      .map_err(|_| ApplicationError::invalid())?;
    let checked = ResearchBuildIntent::new(
      intent.input().clone(),
      policy,
      intent.flow().clone(),
      intent.node().clone(),
      intent.usage_before(),
      intent.priority(),
    )
    .map_err(|_| ApplicationError::invalid())?;
    if &checked != intent {
      return Err(ApplicationError::invalid());
    }
    let accepted = self
      .builds
      .create_factory_build(crate::CreateFactoryBuild {
        operation_id: intent.operation_id().map_err(|_| ApplicationError::invalid())?,
        project_id: intent.input().subject().project_id(),
        repository_id: intent.input().subject().repository_id(),
        build_configuration: intent.profile().build_configuration.clone(),
        immutable_revision: intent.input().subject().base_revision().clone(),
        priority: intent.priority(),
        effective_permissions: intent.permissions().clone(),
        budget: intent.budget(),
        deadline: intent.node().deadline(),
        causality: crate::FactoryBuildCausality::Node(Box::new(crate::FactoryNodeBuildCausality {
          run_id: intent.node().factory_run_id(),
          flow_run_id: intent.flow().id(),
          cycle_id: intent.node().workflow_cycle_id(),
          node_attempt_id: intent.node().id(),
          profile: intent.profile().clone(),
          input_schema: ResearchSchema::Input
            .reference()
            .map_err(|_| ApplicationError::invalid())?,
          input: serde_json::to_vec(intent.input()).map_err(|_| ApplicationError::invalid())?,
          input_digest: intent.input().digest().map_err(|_| ApplicationError::invalid())?,
          context: intent.input().context().clone(),
        })),
      })
      .await
      .map_err(build_error)?;
    validate_acceptance(intent, &accepted)?;
    let build = self.builds.build(accepted.build_id).await?;
    let subject = intent.input().subject();
    let configuration = &intent.profile().build_configuration;
    if build.build.id != accepted.build_id
      || build.build.project_id != subject.project_id()
      || build.build.repository_id != subject.repository_id()
      || build.build.immutable_revision != *subject.base_revision()
      || build.build.configuration_id != configuration.id()
      || build.build.configuration_version != configuration.version()
      || build.build.priority != intent.priority()
      || build.updated_at > at
    {
      return Err(ApplicationError::invalid());
    }
    if !build.state.is_terminal() {
      return Ok(FactoryResearchStep::Waiting);
    }
    let attempt = self.builds.latest_attempt(accepted.build_id).await?;
    let expected = match build.state {
      octacity_server_orchestrator::BuildState::Succeeded => octacity_server_orchestrator::AttemptState::Succeeded,
      octacity_server_orchestrator::BuildState::Failed => octacity_server_orchestrator::AttemptState::Failed,
      octacity_server_orchestrator::BuildState::Cancelled => octacity_server_orchestrator::AttemptState::Cancelled,
      _ => return Err(ApplicationError::invalid()),
    };
    if attempt.build_id != accepted.build_id || attempt.state != expected || attempt.updated_at > at {
      return Err(ApplicationError::invalid());
    }
    if build.state != octacity_server_orchestrator::BuildState::Succeeded {
      return Ok(FactoryResearchStep::ExecutionStopped {
        build_id: accepted.build_id,
        attempt_id: attempt.id,
        state: build.state,
      });
    }
    let jobs = if attempt.id == accepted.attempt_id {
      accepted.job_ids.clone()
    } else {
      attempt.jobs.iter().map(|job| job.id()).collect()
    };
    if jobs.is_empty() || jobs.len() > MAX_RESEARCH_ITEMS {
      return Err(ApplicationError::invalid());
    }
    let outputs = self
      .outputs
      .published_outputs(accepted.build_id, attempt.id, intent.budget().max_output_bytes())
      .await
      .map_err(build_error)?;
    let observed = ResearchBuildObservation {
      build_id: accepted.build_id,
      attempt_id: attempt.id,
      jobs: &jobs,
    };
    let done = complete_research(intent, policy, observed, outputs, ownership, at)?;
    Ok(FactoryResearchStep::Completed(Box::new(done)))
  }
}

fn validate_acceptance(
  intent: &ResearchBuildIntent,
  accepted: &FactoryBuildAcceptance,
) -> Result<(), ApplicationError> {
  let mut jobs = accepted.job_ids.clone();
  jobs.sort();
  if jobs.is_empty()
    || jobs.len() > MAX_RESEARCH_ITEMS
    || jobs.windows(2).any(|pair| pair[0] == pair[1])
    || accepted.effective_policy_digest != intent.permissions().digest()
  {
    return Err(ApplicationError::invalid());
  }
  Ok(())
}
fn build_error(error: OrdinaryBuildApplicationError) -> ApplicationError {
  match error {
    OrdinaryBuildApplicationError::Unavailable => ApplicationError::unavailable(),
    OrdinaryBuildApplicationError::Invalid | OrdinaryBuildApplicationError::Conflict => ApplicationError::invalid(),
  }
}

struct ResearchBuildObservation<'a> {
  build_id: BuildId,
  attempt_id: AttemptId,
  jobs: &'a [octacity_server_domain::JobId],
}

fn complete_research(
  intent: &ResearchBuildIntent,
  policy: &ResearchPolicy,
  observed: ResearchBuildObservation<'_>,
  outputs: FactoryResearchBuildOutputs,
  ownership: FactoryClaimOwnership,
  at: Timestamp,
) -> Result<FactoryResearchCompletion, ApplicationError> {
  if outputs.documents.is_empty()
    || outputs.documents.len() > MAX_RESEARCH_ITEMS
    || outputs.usage.attempts != 1
    || outputs.verified_at > at
    || outputs.verified_at >= outputs.fresh_until
  {
    return Err(ApplicationError::invalid());
  }
  let mut bytes = 0u64;
  let mut identities = std::collections::BTreeSet::new();
  for document in &outputs.documents {
    let identity = document.record.identity();
    bytes = bytes
      .checked_add(identity.size_bytes)
      .ok_or_else(ApplicationError::invalid)?;
    if bytes > intent.budget().max_output_bytes()
      || document.bytes.len() > MAX_RESEARCH_CONTRACT_BYTES
      || !identities.insert(identity.artifact_id)
      || document.record.state() != ArtifactState::Published
      || document
        .record
        .published_at()
        .is_none_or(|published| published > outputs.verified_at || published >= intent.node().deadline())
      || identity.build_id != observed.build_id
      || identity.attempt_id != observed.attempt_id
      || !observed.jobs.contains(&identity.job_id)
      || identity
        .retention
        .delete_after()
        .is_some_and(|deadline| deadline < outputs.fresh_until || deadline <= at)
      || identity.size_bytes != u64::try_from(document.bytes.len()).unwrap_or(u64::MAX)
      || identity.digest.as_bytes() != FactoryDigest::content_sha256(&document.bytes).as_bytes()
    {
      return Err(ApplicationError::invalid());
    }
  }
  if outputs.usage.output_bytes < bytes {
    return Err(ApplicationError::invalid());
  }
  outputs
    .usage
    .validate(intent.budget())
    .map_err(|_| ApplicationError::invalid())?;
  let usage = intent
    .usage_before()
    .checked_add(outputs.usage)
    .ok_or_else(ApplicationError::invalid)?;
  usage
    .validate(policy.settings().budget)
    .map_err(|_| ApplicationError::invalid())?;
  let evidence_kind = match intent.input().kind() {
    WorkKind::Defect => ResearchEvidenceKind::Reproduction,
    WorkKind::FeatureRequest => ResearchEvidenceKind::Proposal,
  };
  let requirement = policy
    .settings()
    .evidence
    .get(&evidence_kind)
    .ok_or_else(ApplicationError::invalid)?;
  let report = selected_document(
    &outputs.documents,
    requirement.kind(),
    requirement.output_kind(),
    requirement.schema(),
    requirement.tool(),
    requirement.plugin(),
  )?;
  let raw = selected_document(
    &outputs.documents,
    &intent.profile().result_output,
    EvidenceOutputKind::Report,
    &ResearchSchema::result(intent.input().kind())
      .reference()
      .map_err(|_| ApplicationError::invalid())?,
    &intent.profile().tool,
    &intent.profile().plugin,
  )?;
  let (observations, fact, environment_digest) = match intent.input().details() {
    ResearchDetails::Defect { environment, .. } => {
      let reproduction: DefectReproductionReport =
        serde_json::from_slice(&report.bytes).map_err(|_| ApplicationError::invalid())?;
      let outcome = reproduction
        .classify(intent.input())
        .map_err(|_| ApplicationError::invalid())?;
      let observations: DefectResearchResult =
        serde_json::from_slice(&raw.bytes).map_err(|_| ApplicationError::invalid())?;
      if observations.outcome != outcome || observations.reproduction_report != artifact_reference(report)? {
        return Err(ApplicationError::invalid());
      }
      (
        ResearchObservations::Defect(Box::new(observations)),
        ResearchEvidenceFact::Reproduction(outcome),
        Some(environment.content_digest()),
      )
    }
    ResearchDetails::Feature { .. } => {
      if report.record.identity().size_bytes > policy.settings().max_proposal_bytes {
        return Err(ApplicationError::invalid());
      }
      let proposal: FeatureResearchProposal =
        serde_json::from_slice(&report.bytes).map_err(|_| ApplicationError::invalid())?;
      let observations: FeatureResearchResult =
        serde_json::from_slice(&raw.bytes).map_err(|_| ApplicationError::invalid())?;
      proposal
        .validate_result(intent.input(), &observations)
        .map_err(|_| ApplicationError::invalid())?;
      if observations.proposal != artifact_reference(report)? {
        return Err(ApplicationError::invalid());
      }
      (
        ResearchObservations::Feature(Box::new(observations)),
        ResearchEvidenceFact::Proposal,
        None,
      )
    }
  };
  let observed_at = raw.record.published_at().ok_or_else(ApplicationError::invalid)?;
  let result = ResearchResult::new(
    intent.input(),
    ResearchProvenance {
      input_digest: intent.input().digest().map_err(|_| ApplicationError::invalid())?,
      context_digest: intent
        .input()
        .context()
        .digest()
        .map_err(|_| ApplicationError::invalid())?,
      definition: intent.flow().definition(),
      flow_run_id: intent.flow().id(),
      cycle_id: intent.node().workflow_cycle_id(),
      node: intent.node().node_key().clone(),
      node_attempt_id: intent.node().id(),
      attempt: intent.node().number(),
      execution_attempt: intent.execution_attempt(),
      producer: document_producer(raw),
      model_or_tool: intent.profile().model_or_tool.clone(),
      task_digest: intent.profile().task_digest,
      result: artifact_reference(raw)?,
      observed_at,
    },
    observations,
  )
  .map_err(|_| ApplicationError::invalid())?;
  let evidence = AcceptedResearchEvidence::new(
    intent.input(),
    &result,
    ResearchEvidenceRecord {
      subject: intent.input().subject().clone(),
      input_digest: result.provenance().input_digest,
      environment_digest,
      fact,
      artifact: artifact_reference(report)?,
      schema: report.schema.clone(),
      output_kind: requirement.output_kind(),
      producer: document_producer(report),
      verified_at: outputs.verified_at,
      fresh_until: outputs.fresh_until,
    },
    DeterministicGateOutcome::Passed,
  )
  .map_err(|_| ApplicationError::invalid())?;
  let definition = policy
    .closure()
    .definition(intent.flow().definition())
    .ok_or_else(ApplicationError::invalid)?;
  let node = definition
    .node(intent.node().node_key())
    .ok_or_else(ApplicationError::invalid)?;
  let schema = ResearchSchema::result(intent.input().kind())
    .reference()
    .map_err(|_| ApplicationError::invalid())?;
  let outcomes = node
    .outcomes()
    .iter()
    .filter(|outcome| outcome.kind() == FlowOutcomeKind::Success && outcome.schema() == &schema)
    .collect::<Vec<_>>();
  let [declared] = outcomes.as_slice() else {
    return Err(ApplicationError::invalid());
  };
  let completion = NodeAttemptCompletion::new(
    intent.node(),
    definition,
    NodeAttemptCompletionInput {
      outcome: declared.key().clone(),
      output_schema: schema,
      output_digest: result.digest().map_err(|_| ApplicationError::invalid())?,
      ownership,
      usage: BudgetUsage {
        attempts: 0,
        ..outputs.usage
      },
      observed_at: at,
    },
  )
  .map_err(|_| ApplicationError::invalid())?;
  let id = StageHandoffId::from_uuid(uuid::Uuid::new_v5(&intent.node().id().as_uuid(), b"research-handoff"))
    .map_err(|_| ApplicationError::invalid())?;
  let acceptance = policy
    .accept(id, intent.input(), &result, std::slice::from_ref(&evidence), usage, at)
    .map_err(|_| ApplicationError::invalid())?;
  Ok(FactoryResearchCompletion {
    result,
    evidence,
    completion,
    usage,
    acceptance,
  })
}

fn selected_document<'a>(
  documents: &'a [FactoryResearchOutputDocument],
  name: &FactoryKey,
  output_kind: EvidenceOutputKind,
  schema: &ImmutableReference,
  tool: &ImmutableReference,
  plugin: &ImmutableReference,
) -> Result<&'a FactoryResearchOutputDocument, ApplicationError> {
  let selected = documents
    .iter()
    .filter(|doc| doc.record.identity().logical_name.as_str() == name.as_str())
    .collect::<Vec<_>>();
  let [document] = selected.as_slice() else {
    return Err(ApplicationError::invalid());
  };
  let matches_type = match (&document.record.identity().artifact_type, output_kind) {
    (ArtifactType::Artifact, EvidenceOutputKind::Artifact) => true,
    (ArtifactType::Report(format), EvidenceOutputKind::Report) => format.as_str() == schema.identity().as_str(),
    _ => false,
  };
  if document.schema != *schema || document.tool != *tool || document.plugin != *plugin || !matches_type {
    return Err(ApplicationError::invalid());
  }
  Ok(document)
}
fn artifact_reference(document: &FactoryResearchOutputDocument) -> Result<FactoryArtifactReference, ApplicationError> {
  let identity = document.record.identity();
  FactoryArtifactReference::new(
    identity.artifact_id,
    FactoryDigest::from_bytes(identity.digest.as_bytes()),
    identity.size_bytes,
  )
  .map_err(|_| ApplicationError::invalid())
}
fn document_producer(document: &FactoryResearchOutputDocument) -> EvidenceProducer {
  let identity = document.record.identity();
  EvidenceProducer::new(
    identity.build_id,
    identity.attempt_id,
    identity.job_id,
    document.tool.clone(),
    document.plugin.clone(),
  )
}
