//! Ordinary Build execution shared by configured command and reasoning nodes.

use crate::{
  ApplicationError, CreateFactoryBuild, FactoryBuildAcceptance, OrdinaryBuildApplication, OrdinaryBuildApplicationError,
};
use async_trait::async_trait;
use octacity_server_artifacts::{ArtifactRecord, ArtifactState, ArtifactType};
use octacity_server_domain::{AttemptId, BuildId, JobId, Timestamp};
use octacity_server_factory::*;
use octacity_server_store::BuildQueryStore;
use std::sync::Arc;

/// Retained ordinary Build output with trusted execution provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryNodeBuildOutputDocument {
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
pub struct FactoryNodeBuildOutputs {
  /// Actual frozen execution selection reconstructed from ordinary Job facts.
  pub execution: FactoryNodeBuildExecutionFacts,
  /// Complete bounded published outputs for the observed Attempt.
  pub documents: Vec<FactoryNodeBuildOutputDocument>,
  /// Measured Node Attempt consumption, including all ordinary Build retries.
  pub usage: BudgetUsage,
  /// Persisted trusted validation time.
  pub verified_at: Timestamp,
  /// Exclusive freshness deadline, covered by Artifact retention.
  pub fresh_until: Timestamp,
}

/// Owner-verified execution facts, read from immutable Jobs rather than provider reports.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryNodeBuildExecutionFacts {
  /// Exact executed task, model/tool, plugin and Build configuration.
  pub profile: FlowBuildProfile,
  /// Digest of the complete input and selected Context actually supplied to execution.
  pub input_digest: FactoryDigest,
  /// Effective frozen authority of the ordinary Build.
  pub permissions: FactoryPermissionSet,
  /// Frozen execution ceiling.
  pub budget: BudgetLimit,
  /// Frozen deadline; restarting observation cannot extend it.
  pub deadline: Timestamp,
}

/// Reads bounded retained bytes and actual execution facts from ordinary Builds.
#[async_trait]
pub trait FactoryNodeBuildOutputSource: Send + Sync {
  /// Enforces the byte ceiling before reading and returns no provider-selected routing authority.
  /// Schema references must come from trusted schema-specific verification of these exact bytes,
  /// not from provider JSON or from the requested evidence requirement.
  async fn published_outputs(
    &self,
    build: BuildId,
    attempt: AttemptId,
    max_bytes: u64,
  ) -> Result<FactoryNodeBuildOutputs, OrdinaryBuildApplicationError>;
}

const MAX_BUILD_JOBS: usize = 64;

/// Result of observing one persisted command or reasoning operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactoryNodeBuildStep {
  /// The same ordinary Build is still executing.
  Waiting(Box<FlowBuildExecution>),
  /// Ordinary execution failure or cancellation, never a semantic observation.
  ExecutionStopped {
    /// Exact retained ordinary execution.
    execution: Box<FlowBuildExecution>,
    /// Build retaining authoritative diagnostics.
    build_id: BuildId,
    /// Latest Attempt, including ordinary retries.
    attempt_id: AttemptId,
    /// Failed or cancelled execution state.
    state: octacity_server_orchestrator::BuildState,
  },
  /// Verified common result and completion for one fenced owner transaction.
  Completed(Box<FactoryNodeBuildCompletion>),
}

/// Verified observations; only their atomic owner commit can advance the Flow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryNodeBuildCompletion {
  /// Exact ordinary execution, retained with the verified result.
  pub execution: FlowBuildExecution,
  /// Immutable common result with exact Build/Attempt/Job provenance.
  pub record: FlowNodeRecord,
  /// Generic completion charged independently of attempt creation.
  pub completion: NodeAttemptCompletion,
  /// Aggregate measured usage including this external execution exactly once.
  pub usage: BudgetUsage,
}

/// Stateless execution bridge selected entirely by the immutable node definition.
///
/// The Flow owner persists intents and owns readiness, retries and atomic
/// completion. Ordinary Builds retain their existing execution state machine.
pub struct FactoryNodeBuildAdapter<B, O> {
  builds: Arc<B>,
  outputs: Arc<O>,
}
impl<B, O> FactoryNodeBuildAdapter<B, O>
where
  B: OrdinaryBuildApplication + BuildQueryStore,
  O: FactoryNodeBuildOutputSource,
{
  /// Connects ordinary Build creation/query and bounded retained-output access.
  #[must_use]
  pub fn new(builds: Arc<B>, outputs: Arc<O>) -> Self {
    Self { builds, outputs }
  }
  /// Dispatches or observes the stable operation under current fenced ownership.
  pub async fn observe_or_dispatch(
    &self,
    intent: &FlowBuildIntent,
    admitted: &AdmittedFlow,
    schema: &FlowDataSchema,
    ownership: FactoryClaimOwnership,
    at: Timestamp,
  ) -> Result<FactoryNodeBuildStep, ApplicationError> {
    let checked = FlowBuildIntent::new(
      intent.input().clone(),
      admitted,
      intent.node().clone(),
      intent.usage_before(),
      intent.priority(),
    )
    .map_err(invalid)?;
    if &checked != intent {
      return Err(ApplicationError::invalid());
    }
    let definition = admitted
      .closure()
      .definition(intent.input().definition())
      .ok_or_else(ApplicationError::invalid)?;
    if definition
      .node(intent.input().node())
      .and_then(|node| node.outcome(intent.result_outcome()))
      .is_none_or(|outcome| outcome.schema() != schema.reference())
    {
      return Err(ApplicationError::invalid());
    }
    let input = intent.input();
    let request = CreateFactoryBuild {
      operation_id: intent.operation_id().map_err(invalid)?,
      project_id: input.subject().project_id(),
      repository_id: input.subject().repository_id(),
      build_configuration: intent.profile().build_configuration.clone(),
      immutable_revision: input.subject().base_revision().clone(),
      priority: intent.priority(),
      effective_permissions: intent.permissions().clone(),
      budget: intent.budget(),
      deadline: intent.node().deadline(),
      causality: crate::FactoryBuildCausality::Node(Box::new(crate::FactoryNodeBuildCausality {
        run_id: input.factory_run_id(),
        flow_run_id: input.flow_run_id(),
        cycle_id: input.cycle_id(),
        node_attempt_id: intent.node().id(),
        profile: intent.profile().clone(),
        input_schema: input.payload().schema().clone(),
        input: serde_json::to_vec(input).map_err(|_| ApplicationError::invalid())?,
        input_digest: input.digest().map_err(invalid)?,
        context: input.context().ok_or_else(ApplicationError::invalid)?.clone(),
      })),
    };
    match observe_build(self.builds.as_ref(), request, intent.node(), ownership.clone(), at).await? {
      BuildObservation::Waiting(observed) => Ok(FactoryNodeBuildStep::Waiting(Box::new(
        FlowBuildExecution::new(intent, observed.build_id, observed.attempt_id, observed.jobs).map_err(invalid)?,
      ))),
      BuildObservation::ExecutionStopped {
        build_id,
        attempt_id,
        state,
        jobs,
      } => Ok(FactoryNodeBuildStep::ExecutionStopped {
        execution: Box::new(FlowBuildExecution::new(intent, build_id, attempt_id, jobs).map_err(invalid)?),
        build_id,
        attempt_id,
        state,
      }),
      BuildObservation::Succeeded(observed) => {
        let outputs = self
          .outputs
          .published_outputs(
            observed.build_id,
            observed.attempt_id,
            intent.budget().max_output_bytes(),
          )
          .await
          .map_err(build_error)?;
        let done = complete_node(intent, admitted, schema, &observed, outputs, ownership, at)?;
        Ok(FactoryNodeBuildStep::Completed(Box::new(done)))
      }
    }
  }
}

fn complete_node(
  intent: &FlowBuildIntent,
  admitted: &AdmittedFlow,
  schema: &FlowDataSchema,
  observed: &ObservedBuild,
  outputs: FactoryNodeBuildOutputs,
  ownership: FactoryClaimOwnership,
  at: Timestamp,
) -> Result<FactoryNodeBuildCompletion, ApplicationError> {
  if outputs.execution.profile != *intent.profile()
    || outputs.execution.input_digest != intent.input().digest().map_err(invalid)?
    || outputs.execution.permissions != *intent.permissions()
    || outputs.execution.budget != intent.budget()
    || outputs.execution.deadline != intent.node().deadline()
  {
    return Err(ApplicationError::invalid());
  }
  validate_outputs(&outputs, intent.budget(), intent.node(), observed, at)?;
  let definition = admitted
    .closure()
    .definition(intent.input().definition())
    .ok_or_else(ApplicationError::invalid)?;
  let node = definition
    .node(intent.input().node())
    .ok_or_else(ApplicationError::invalid)?;
  if node
    .outcome(intent.result_outcome())
    .is_none_or(|outcome| outcome.schema() != schema.reference())
  {
    return Err(ApplicationError::invalid());
  }
  let document = selected_document(
    &outputs.documents,
    &intent.profile().result_output,
    EvidenceOutputKind::Report,
    schema.reference(),
    &intent.profile().tool,
    &intent.profile().plugin,
  )?;
  let payload = FlowPayload::new(
    schema,
    serde_json::from_slice(&document.bytes).map_err(|_| ApplicationError::invalid())?,
  )
  .map_err(invalid)?;
  let mut verified = vec![FlowVerifiedBuildOutput {
    name: intent.profile().result_output.clone(),
    output_kind: EvidenceOutputKind::Report,
    schema: document.schema.clone(),
    artifact: artifact_reference(document)?,
    producer: document_producer(document),
    verified_at: outputs.verified_at,
    fresh_until: outputs.fresh_until,
    data: None,
  }];
  for requirement in &node.build().ok_or_else(ApplicationError::invalid)?.evidence {
    let evidence = selected_document(
      &outputs.documents,
      requirement.kind(),
      requirement.output_kind(),
      requirement.schema(),
      requirement.tool(),
      requirement.plugin(),
    )?;
    verified.push(FlowVerifiedBuildOutput {
      name: requirement.kind().clone(),
      output_kind: requirement.output_kind(),
      schema: evidence.schema.clone(),
      artifact: artifact_reference(evidence)?,
      producer: document_producer(evidence),
      verified_at: outputs.verified_at,
      fresh_until: outputs.fresh_until,
      data: if requirement.output_kind() == EvidenceOutputKind::Report {
        let contract = admitted
          .data_schema(requirement.schema())
          .ok_or_else(ApplicationError::invalid)?;
        Some(
          FlowVerifiedBuildData::new(
            contract.clone(),
            serde_json::from_slice(&evidence.bytes).map_err(|_| ApplicationError::invalid())?,
          )
          .map_err(invalid)?,
        )
      } else {
        None
      },
    });
  }
  let provenance = FlowBuildProvenance::new(intent.profile().clone(), verified).map_err(invalid)?;
  let record = FlowNodeRecord::new(
    intent.input(),
    intent.node(),
    definition,
    FlowRecordObservation {
      outcome: intent.result_outcome().clone(),
      payload,
      producer: intent.node().execution().clone(),
      observed_at: document.record.published_at().ok_or_else(ApplicationError::invalid)?,
    },
  )
  .map_err(invalid)?
  .with_build_provenance(provenance, definition)
  .map_err(invalid)?;
  let usage = intent
    .usage_before()
    .checked_add(outputs.usage)
    .ok_or_else(ApplicationError::invalid)?
    .validate(definition.execution().budget())
    .map_err(invalid)?;
  let completion = record
    .completion(
      intent.node(),
      definition,
      ownership,
      BudgetUsage {
        attempts: 0,
        ..outputs.usage
      },
      at,
    )
    .map_err(invalid)?;
  Ok(FactoryNodeBuildCompletion {
    execution: FlowBuildExecution::new(intent, observed.build_id, observed.attempt_id, observed.jobs.clone())
      .map_err(invalid)?,
    record,
    completion,
    usage,
  })
}

pub(super) fn validate_outputs(
  outputs: &FactoryNodeBuildOutputs,
  budget: BudgetLimit,
  node: &NodeAttempt,
  observed: &ObservedBuild,
  at: Timestamp,
) -> Result<(), ApplicationError> {
  if outputs.documents.is_empty()
    || outputs.documents.len() > MAX_BUILD_JOBS
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
    if bytes > budget.max_output_bytes()
      || document.bytes.len() > MAX_FLOW_DATA_BYTES
      || !identities.insert(identity.artifact_id)
      || document.record.state() != ArtifactState::Published
      || document
        .record
        .published_at()
        .is_none_or(|published| published > outputs.verified_at || published >= node.deadline())
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
  outputs.usage.validate(budget).map_err(invalid)?;
  Ok(())
}
pub(super) fn selected_document<'a>(
  documents: &'a [FactoryNodeBuildOutputDocument],
  name: &FactoryKey,
  kind: EvidenceOutputKind,
  schema: &ImmutableReference,
  tool: &ImmutableReference,
  plugin: &ImmutableReference,
) -> Result<&'a FactoryNodeBuildOutputDocument, ApplicationError> {
  let mut selected = documents
    .iter()
    .filter(|document| document.record.identity().logical_name.as_str() == name.as_str());
  let document = selected.next().ok_or_else(ApplicationError::invalid)?;
  let matches_type = match (&document.record.identity().artifact_type, kind) {
    (ArtifactType::Artifact, EvidenceOutputKind::Artifact) => true,
    (ArtifactType::Report(format), EvidenceOutputKind::Report) => format.as_str() == schema.identity().as_str(),
    _ => false,
  };
  if selected.next().is_some()
    || document.schema != *schema
    || document.tool != *tool
    || document.plugin != *plugin
    || !matches_type
  {
    return Err(ApplicationError::invalid());
  }
  Ok(document)
}
pub(super) fn artifact_reference(
  document: &FactoryNodeBuildOutputDocument,
) -> Result<FactoryArtifactReference, ApplicationError> {
  let identity = document.record.identity();
  FactoryArtifactReference::new(
    identity.artifact_id,
    FactoryDigest::from_bytes(identity.digest.as_bytes()),
    identity.size_bytes,
  )
  .map_err(invalid)
}
pub(super) fn document_producer(document: &FactoryNodeBuildOutputDocument) -> EvidenceProducer {
  let identity = document.record.identity();
  EvidenceProducer::new(
    identity.build_id,
    identity.attempt_id,
    identity.job_id,
    document.tool.clone(),
    document.plugin.clone(),
  )
}

pub(super) struct ObservedBuild {
  pub build_id: BuildId,
  pub attempt_id: AttemptId,
  pub jobs: Vec<JobId>,
}
pub(super) enum BuildObservation {
  Waiting(ObservedBuild),
  ExecutionStopped {
    build_id: BuildId,
    attempt_id: AttemptId,
    state: octacity_server_orchestrator::BuildState,
    jobs: Vec<JobId>,
  },
  Succeeded(ObservedBuild),
}
pub(super) async fn observe_build<B: OrdinaryBuildApplication + BuildQueryStore>(
  builds: &B,
  request: CreateFactoryBuild,
  node: &NodeAttempt,
  ownership: FactoryClaimOwnership,
  at: Timestamp,
) -> Result<BuildObservation, ApplicationError> {
  node.verify_observer(&ownership, at).map_err(invalid)?;
  let accepted = if at >= node.deadline() {
    builds
      .factory_build_for_operation(request.operation_id)
      .await
      .map_err(build_error)?
      .ok_or_else(ApplicationError::invalid)?
  } else {
    builds
      .create_factory_build(request.clone())
      .await
      .map_err(build_error)?
  };
  validate_acceptance(&accepted, request.effective_permissions.digest())?;
  let build = builds.build(accepted.build_id).await?;
  if build.build.id != accepted.build_id
    || build.build.project_id != request.project_id
    || build.build.repository_id != request.repository_id
    || build.build.immutable_revision != request.immutable_revision
    || build.build.configuration_id != request.build_configuration.id()
    || build.build.configuration_version != request.build_configuration.version()
    || build.build.priority != request.priority
    || build.updated_at > at
  {
    return Err(ApplicationError::invalid());
  }
  if !build.state.is_terminal() {
    return Ok(BuildObservation::Waiting(ObservedBuild {
      build_id: accepted.build_id,
      attempt_id: accepted.attempt_id,
      jobs: accepted.job_ids,
    }));
  }
  let attempt = builds.latest_attempt(accepted.build_id).await?;
  let expected = match build.state {
    octacity_server_orchestrator::BuildState::Succeeded => octacity_server_orchestrator::AttemptState::Succeeded,
    octacity_server_orchestrator::BuildState::Failed => octacity_server_orchestrator::AttemptState::Failed,
    octacity_server_orchestrator::BuildState::Cancelled => octacity_server_orchestrator::AttemptState::Cancelled,
    _ => return Err(ApplicationError::invalid()),
  };
  if attempt.build_id != accepted.build_id || attempt.state != expected || attempt.updated_at > at {
    return Err(ApplicationError::invalid());
  }
  let jobs = if attempt.id == accepted.attempt_id {
    accepted.job_ids
  } else {
    attempt.jobs.iter().map(|job| job.id()).collect()
  };
  if jobs.is_empty() || jobs.len() > MAX_BUILD_JOBS {
    return Err(ApplicationError::invalid());
  }
  if build.state != octacity_server_orchestrator::BuildState::Succeeded {
    return Ok(BuildObservation::ExecutionStopped {
      build_id: accepted.build_id,
      attempt_id: attempt.id,
      state: build.state,
      jobs,
    });
  }
  Ok(BuildObservation::Succeeded(ObservedBuild {
    build_id: accepted.build_id,
    attempt_id: attempt.id,
    jobs,
  }))
}
fn validate_acceptance(accepted: &FactoryBuildAcceptance, permissions: FactoryDigest) -> Result<(), ApplicationError> {
  let mut jobs = accepted.job_ids.clone();
  jobs.sort();
  if jobs.is_empty()
    || jobs.len() > MAX_BUILD_JOBS
    || jobs.windows(2).any(|pair| pair[0] == pair[1])
    || accepted.effective_policy_digest != permissions
  {
    return Err(ApplicationError::invalid());
  }
  Ok(())
}
pub(super) fn build_error(error: OrdinaryBuildApplicationError) -> ApplicationError {
  match error {
    OrdinaryBuildApplicationError::Unavailable => ApplicationError::unavailable(),
    OrdinaryBuildApplicationError::Invalid | OrdinaryBuildApplicationError::Conflict => ApplicationError::invalid(),
  }
}
fn invalid(_: FactoryError) -> ApplicationError {
  ApplicationError::invalid()
}

#[async_trait]
impl<B, O> crate::FactoryNodeExecutor for FactoryNodeBuildAdapter<B, O>
where
  B: OrdinaryBuildApplication + BuildQueryStore,
  O: FactoryNodeBuildOutputSource,
{
  async fn observe(
    &self,
    request: crate::FactoryNodeExecutionRequest<'_>,
  ) -> Result<crate::FactoryNodeExecutionStep, ApplicationError> {
    let intent = request.build.ok_or_else(ApplicationError::invalid)?;
    if intent.node() != request.attempt || intent.input() != request.input {
      return Err(ApplicationError::invalid());
    }
    let schema = request
      .snapshot
      .admitted_flow
      .closure()
      .definition(request.input.definition())
      .and_then(|definition| definition.node(request.input.node()))
      .and_then(|node| node.outcome(intent.result_outcome()))
      .and_then(|outcome| request.snapshot.admitted_flow.data_schema(outcome.schema()))
      .ok_or_else(ApplicationError::invalid)?;
    match self
      .observe_or_dispatch(
        intent,
        &request.snapshot.admitted_flow,
        schema,
        request.ownership,
        request.observed_at,
      )
      .await?
    {
      FactoryNodeBuildStep::Waiting(execution) => Ok(crate::FactoryNodeExecutionStep::Waiting {
        execution: Some(execution),
      }),
      FactoryNodeBuildStep::ExecutionStopped { execution, .. } => {
        Ok(crate::FactoryNodeExecutionStep::ExecutionStopped {
          execution: Some(execution),
        })
      }
      FactoryNodeBuildStep::Completed(done) => Ok(crate::FactoryNodeExecutionStep::Completed {
        execution: Some(Box::new(done.execution)),
        record: Box::new(done.record),
        usage: done.completion.usage(),
      }),
    }
  }
}
