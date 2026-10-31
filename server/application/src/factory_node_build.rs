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
  /// Schema-validated facts projected by the installed output verifier.
  /// Absent for outputs whose retained bytes are already the declared JSON.
  /// Wire formats remain inside the selected capability adapter.
  pub data: Option<FlowVerifiedBuildData>,
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
  /// Schema-verified outputs eligible for the common result and evidence.
  /// Private harness records remain ordinary Build Artifacts; usage covers them.
  pub documents: Vec<FactoryNodeBuildOutputDocument>,
  /// Measured Node Attempt consumption, including all ordinary Build retries.
  pub usage: BudgetUsage,
  /// Persisted trusted validation time.
  pub verified_at: Timestamp,
  /// Exclusive freshness deadline, covered by Artifact retention.
  pub fresh_until: Timestamp,
}

/// Verified capability observation, independent of the ordinary process exit status.
#[derive(Clone, Eq, PartialEq)]
pub enum FactoryNodeBuildOutputObservation {
  /// Exact schema-bound data and independently verified execution facts.
  Verified(Box<FactoryNodeBuildOutputs>),
  /// The capability failed technically; no semantic result or transition exists.
  ExecutionStopped {
    /// Measured aggregate resources; Node Attempt creation already charged attempts.
    usage: BudgetUsage,
  },
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
  /// Reads total measured consumption of the operation through its terminal Attempt,
  /// including resources spent by earlier retries. Availability is required before
  /// the owner can retain a stop receipt; an absent measurement is not zero usage.
  async fn terminal_usage(
    &self,
    build: BuildId,
    attempt: AttemptId,
  ) -> Result<BudgetUsage, OrdinaryBuildApplicationError>;
  /// Enforces the byte ceiling before reading and returns no routing authority.
  /// Schema references come from trusted verification of these exact bytes,
  /// rather than provider JSON or the requested evidence requirement.
  async fn published_outputs(
    &self,
    build: BuildId,
    attempt: AttemptId,
    max_bytes: u64,
  ) -> Result<FactoryNodeBuildOutputObservation, OrdinaryBuildApplicationError>;
}

use crate::factory_build::MAX_FACTORY_BUILD_JOBS as MAX_BUILD_JOBS;

/// Technical cancellation observation, independent of configured business outcomes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactoryNodeBuildCancellation {
  /// Cancellation intent is accepted; owned Jobs may still be stopping.
  Waiting(Box<FlowBuildExecution>),
  /// The operation is absent or its ordinary execution and all owned Jobs have stopped.
  Stopped {
    /// Exact retained execution when the operation was previously dispatched.
    execution: Option<Box<FlowBuildExecution>>,
  },
}

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
    /// Observed ordinary state. A successful process may still have a verified
    /// capability failure without a semantic node result.
    state: octacity_server_orchestrator::BuildState,
    /// Already verified aggregate consumption; ordinary failures recover it separately.
    usage: Option<BudgetUsage>,
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
/// The shared Build boundary accepts only common Node Attempt causality:
///
/// ```compile_fail
/// use octacity_server_application::{FactoryBuildBridge, FactoryStageBuildCausality};
/// ```
pub struct FactoryNodeBuildAdapter<B, O> {
  builds: Arc<B>,
  outputs: Arc<O>,
  action_input: Option<serde_json::Value>,
}
impl<B, O> FactoryNodeBuildAdapter<B, O>
where
  B: OrdinaryBuildApplication + BuildQueryStore,
  O: FactoryNodeBuildOutputSource,
{
  /// Connects ordinary Build creation/query and bounded retained-output access.
  #[must_use]
  pub fn new(builds: Arc<B>, outputs: Arc<O>) -> Self {
    Self {
      builds,
      outputs,
      action_input: None,
    }
  }
  // Installed capabilities derive this bounded document from the frozen owner
  // request. The common adapter does not interpret business policy or Flow names.
  pub(crate) fn with_action_input(&self, input: serde_json::Value) -> Result<Self, ApplicationError> {
    if !input.is_object()
      || serde_json::to_vec(&input)
        .map_err(|_| ApplicationError::invalid())?
        .len()
        > MAX_FLOW_DATA_BYTES
    {
      return Err(ApplicationError::invalid());
    }
    Ok(Self {
      builds: self.builds.clone(),
      outputs: self.outputs.clone(),
      action_input: Some(input),
    })
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
    intent.verify_admission(admitted).map_err(invalid)?;
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
    let mut request = factory_build_request(intent, ownership.clone())?;
    if self.action_input.is_some() && intent.action().is_none() {
      return Err(ApplicationError::invalid());
    }
    request.causality.action_input = self.action_input.clone();
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
        usage: None,
      }),
      BuildObservation::Succeeded(observed) => {
        let observation = self
          .outputs
          .published_outputs(
            observed.build_id,
            observed.attempt_id,
            intent.budget().max_output_bytes(),
          )
          .await
          .map_err(build_error)?;
        let outputs = match observation {
          FactoryNodeBuildOutputObservation::Verified(outputs) => *outputs,
          FactoryNodeBuildOutputObservation::ExecutionStopped { usage } => {
            if usage.attempts != 0 || usage.output_bytes > intent.budget().max_output_bytes() {
              return Err(ApplicationError::invalid());
            }
            return Ok(FactoryNodeBuildStep::ExecutionStopped {
              execution: Box::new(
                FlowBuildExecution::new(intent, observed.build_id, observed.attempt_id, observed.jobs)
                  .map_err(invalid)?,
              ),
              build_id: observed.build_id,
              attempt_id: observed.attempt_id,
              state: octacity_server_orchestrator::BuildState::Succeeded,
              usage: Some(usage),
            });
          }
        };
        let done = complete_node(intent, admitted, schema, &observed, outputs, ownership, at)?;
        Ok(FactoryNodeBuildStep::Completed(Box::new(done)))
      }
    }
  }
}

impl<B, O> FactoryNodeBuildAdapter<B, O>
where
  B: crate::OrdinaryBuildCancellationApplication + BuildQueryStore,
  O: FactoryNodeBuildOutputSource,
{
  /// Cancels only an existing operation and observes its ordinary terminal facts.
  /// The original cancellation time and idempotency identity remain frozen through
  /// restart. Observation may continue after the execution deadline, without dispatch.
  pub async fn cancel(
    &self,
    intent: &FlowBuildIntent,
    admitted: &AdmittedFlow,
    ownership: FactoryClaimOwnership,
    requested_at: Timestamp,
    at: Timestamp,
  ) -> Result<FactoryNodeBuildCancellation, ApplicationError> {
    if requested_at > at {
      return Err(ApplicationError::invalid());
    }
    // Recheck the retained contract, including any pinned profile selection.
    intent.verify_admission(admitted).map_err(invalid)?;
    intent.node().verify_observer(&ownership, at).map_err(invalid)?;
    let request = factory_build_request(intent, ownership.clone())?;
    let Some(accepted) = self
      .builds
      .factory_build_for_operation(request.operation_id)
      .await
      .map_err(build_error)?
    else {
      return Ok(FactoryNodeBuildCancellation::Stopped { execution: None });
    };
    match observe_accepted_build(self.builds.as_ref(), &request, accepted.clone(), at).await? {
      BuildObservation::Waiting(observed) => {
        let execution =
          FlowBuildExecution::new(intent, observed.build_id, observed.attempt_id, observed.jobs).map_err(invalid)?;
        self
          .builds
          .cancel_factory_build(crate::CancelFactoryBuild {
            ownership: ownership.clone(),
            operation_id: FactoryDigest::sha256(
              "octacity.factory.build-cancellation.v1",
              &[request.operation_id.as_bytes().as_slice()],
            ),
            build_operation_id: request.operation_id,
            build_id: observed.build_id,
            requested_at,
          })
          .await
          .map_err(build_error)?;
        Ok(FactoryNodeBuildCancellation::Waiting(Box::new(execution)))
      }
      terminal => {
        let observed = match terminal {
          BuildObservation::Succeeded(observed) => observed,
          BuildObservation::ExecutionStopped {
            build_id,
            attempt_id,
            jobs,
            ..
          } => ObservedBuild {
            build_id,
            attempt_id,
            jobs,
          },
          BuildObservation::Waiting(_) => unreachable!("waiting execution handled above"),
        };
        let stopped = owned_execution_has_stopped(self.builds.as_ref(), &accepted, &observed, at).await?;
        let execution = Box::new(
          FlowBuildExecution::new(intent, observed.build_id, observed.attempt_id, observed.jobs).map_err(invalid)?,
        );
        if stopped {
          Ok(FactoryNodeBuildCancellation::Stopped {
            execution: Some(execution),
          })
        } else {
          // A terminal aggregate can precede acknowledgements from still-stopping Jobs.
          Ok(FactoryNodeBuildCancellation::Waiting(execution))
        }
      }
    }
  }
}

/// Walks the bounded ordinary retry lineage, retaining no routing authority from it.
/// An earlier failed Attempt may still have Jobs acknowledging cancellation.
async fn owned_execution_has_stopped<B: BuildQueryStore>(
  builds: &B,
  accepted: &FactoryBuildAcceptance,
  observed: &ObservedBuild,
  at: Timestamp,
) -> Result<bool, ApplicationError> {
  let mut attempt = builds.attempt(observed.attempt_id).await?;
  if attempt.id != observed.attempt_id {
    return Err(ApplicationError::invalid());
  }
  let mut stopped = true;
  // This is the absolute execution-history bound, independent of logical node names.
  for _ in 0..MAX_FACTORY_ATTEMPTS {
    let jobs: std::collections::BTreeSet<_> = attempt.jobs.iter().map(|job| job.id()).collect();
    if attempt.build_id != accepted.build_id
      || attempt.updated_at > at
      || jobs.is_empty()
      || jobs.len() > MAX_BUILD_JOBS
      || jobs.len() != attempt.jobs.len()
      || attempt
        .jobs
        .iter()
        .any(|job| job.attempt_id != attempt.id || job.updated_at > at)
    {
      return Err(ApplicationError::invalid());
    }
    stopped &= attempt.state.is_terminal() && attempt.jobs.iter().all(|job| job.state.is_terminal());
    if attempt.id == accepted.attempt_id {
      if attempt.number != octacity_server_domain::AttemptNumber::FIRST
        || attempt.retry_of_attempt_id.is_some()
        || jobs != accepted.job_ids.iter().copied().collect()
      {
        return Err(ApplicationError::invalid());
      }
      return Ok(stopped);
    }
    let predecessor_id = attempt.retry_of_attempt_id.ok_or_else(ApplicationError::invalid)?;
    let predecessor = builds.attempt(predecessor_id).await?;
    if predecessor.id != predecessor_id || predecessor.number.next().ok() != Some(attempt.number) {
      return Err(ApplicationError::invalid());
    }
    attempt = predecessor;
  }
  Err(ApplicationError::invalid())
}

pub(crate) fn factory_build_request(
  intent: &FlowBuildIntent,
  ownership: FactoryClaimOwnership,
) -> Result<CreateFactoryBuild, ApplicationError> {
  let input = intent.input();
  if let Some(selected) = intent.verification_check() {
    let check = selected
      .plan()
      .policy()
      .checks
      .get(input.node())
      .ok_or_else(ApplicationError::invalid)?;
    let target = selected.target();
    if let Some(endpoint) = &target.endpoint {
      let url = url::Url::parse(endpoint).map_err(|_| ApplicationError::invalid())?;
      let host = url.host().map(|host| match host {
        url::Host::Ipv6(address) => address.to_string(),
        other => other.to_string(),
      });
      if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || host.as_ref().is_none_or(|host| !check.endpoints.contains(host))
      {
        return Err(ApplicationError::invalid());
      }
    } else if !check.endpoints.is_empty() {
      return Err(ApplicationError::invalid());
    }
    let source_visible = check.source_visibility == VerificationSourceVisibility::ReadOnlySource;
    if intent.permissions().mounts().any(|mount| {
      !matches!(
        (mount.root().as_str(), mount.mode()),
        (octacity_protocol::PROTECTED_INPUT_ROOT, MountMode::ReadOnly)
          | (
            octacity_protocol::FACTORY_OUTPUT_ROOT | octacity_protocol::FACTORY_SCRATCH_ROOT,
            MountMode::ReadWrite
          )
      ) && !(source_visible
        && mount.root().as_str() == octacity_protocol::FACTORY_SOURCE_ROOT
        && mount.mode() == MountMode::ReadOnly)
    }) || intent.profile().capture_write_paths.is_some()
      || intent.profile().capture_deliverables.is_some()
    {
      return Err(ApplicationError::invalid());
    }
  }
  Ok(CreateFactoryBuild {
    ownership,
    operation_id: intent.operation_id().map_err(invalid)?,
    project_id: input.subject().project_id(),
    subject: input.task_subject().map_err(|_| ApplicationError::invalid())?,
    build_configuration: intent.profile().build_configuration.clone(),
    priority: intent.priority(),
    effective_permissions: intent.permissions().clone(),
    budget: intent.budget(),
    deadline: intent.node().deadline(),
    causality: Box::new(crate::FactoryNodeBuildCausality {
      run_id: input.factory_run_id(),
      flow_run_id: input.flow_run_id(),
      cycle_id: input.cycle_id(),
      node_attempt_id: intent.node().id(),
      profile: intent.profile().clone(),
      action: intent.action().cloned(),
      action_input: None,
      verification_check: intent.verification_check().cloned(),
      deliverable_contract: intent
        .profile()
        .capture_deliverables
        .as_ref()
        .map(|binding| binding.freeze(input))
        .transpose()
        .map_err(invalid)?,
      input_schema: input.payload().schema().clone(),
      input: serde_json::to_vec(input).map_err(|_| ApplicationError::invalid())?,
      input_digest: input.digest().map_err(invalid)?,
      context: input.context().ok_or_else(ApplicationError::invalid)?.clone(),
    }),
  })
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
  validate_execution_facts(intent, &outputs)?;
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
    intent.profile().result_output_kind,
    schema.reference(),
    intent.profile().result_tool(),
    &intent.profile().plugin,
  )?;
  let payload = FlowPayload::new(schema, output_value(document, schema.reference())?).map_err(invalid)?;
  if let Some(selected) = intent.verification_check() {
    let observation: VerificationObservation =
      serde_json::from_value(payload.value().clone()).map_err(|_| ApplicationError::invalid())?;
    observation.verify(selected).map_err(invalid)?;
    if &observation.check != intent.input().node() {
      return Err(ApplicationError::invalid());
    }
  }
  let mut verified = vec![FlowVerifiedBuildOutput {
    name: intent.profile().result_output.clone(),
    output_kind: intent.profile().result_output_kind,
    schema: document.schema.clone(),
    artifact: artifact_reference(document)?,
    producer: document_producer(document),
    verified_at: outputs.verified_at,
    fresh_until: outputs.fresh_until,
    data: document.data.clone(),
  }];
  for requirement in &intent.profile().binding(definition).map_err(invalid)?.evidence {
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
          FlowVerifiedBuildData::new(contract.clone(), output_value(evidence, requirement.schema())?)
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
    .validate(admitted.limits().budget())
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

pub(super) fn validate_execution_facts(
  intent: &FlowBuildIntent,
  outputs: &FactoryNodeBuildOutputs,
) -> Result<(), ApplicationError> {
  if outputs.execution.profile != *intent.profile()
    || outputs.execution.input_digest != intent.input().digest().map_err(invalid)?
    || outputs.execution.permissions != *intent.permissions()
    || outputs.execution.budget != intent.budget()
    || outputs.execution.deadline != intent.node().deadline()
  {
    return Err(ApplicationError::invalid());
  }
  Ok(())
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
    (ArtifactType::Report(format), EvidenceOutputKind::Report) => {
      document.data.is_some() || format.as_str() == schema.identity().as_str()
    }
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

fn output_value(
  document: &FactoryNodeBuildOutputDocument,
  schema: &ImmutableReference,
) -> Result<serde_json::Value, ApplicationError> {
  match &document.data {
    Some(data) if data.payload().schema() == schema => Ok(data.payload().value().clone()),
    Some(_) => Err(ApplicationError::invalid()),
    None => serde_json::from_slice(&document.bytes).map_err(|_| ApplicationError::invalid()),
  }
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
  let accepted = match builds
    .factory_build_for_operation(request.operation_id)
    .await
    .map_err(build_error)?
  {
    Some(accepted) => accepted,
    None if at >= node.deadline() => return Err(ApplicationError::invalid()),
    None => {
      let accepted = builds
        .create_factory_build(request.clone())
        .await
        .map_err(build_error)?;
      validate_acceptance(&accepted, request.effective_permissions.digest())?;
      // Admission can finish after this pass's observation time. Retain its
      // atomically verified receipt first; the next fenced owner pass observes
      // the Build using its own fresh time rather than manufacturing one here.
      return Ok(BuildObservation::Waiting(ObservedBuild {
        build_id: accepted.build_id,
        attempt_id: accepted.attempt_id,
        jobs: accepted.job_ids,
      }));
    }
  };
  observe_accepted_build(builds, &request, accepted, at).await
}
async fn observe_accepted_build<B: BuildQueryStore>(
  builds: &B,
  request: &CreateFactoryBuild,
  accepted: FactoryBuildAcceptance,
  at: Timestamp,
) -> Result<BuildObservation, ApplicationError> {
  validate_acceptance(&accepted, request.effective_permissions.digest())?;
  let build = builds.build(accepted.build_id).await?;
  let expected_origin = octacity_server_store::BuildOrigin::Factory(octacity_server_store::FactoryBuildOrigin {
    operation_id: request.operation_id,
    run_id: request.causality.run_id,
    node_attempt_id: request.causality.node_attempt_id,
    input_digest: request.causality.input_digest,
  });
  if build.origin != expected_origin
    || build.build.id != accepted.build_id
    || build.build.project_id != request.project_id
    || build.build.repository_id != request.subject.exact().ok().map(|resource| resource.repository_id())
    || build.build.immutable_revision.as_ref() != request.subject.exact().ok().map(|resource| resource.base_revision())
    || build.build.source
      != if matches!(request.subject, FactoryTaskSubject::WorkInputV1(_)) {
        Some(octacity_server_domain::BuildSourceMode::SourceFreeV1)
      } else {
        None
      }
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
  B: crate::OrdinaryBuildCancellationApplication + BuildQueryStore,
  O: FactoryNodeBuildOutputSource,
{
  async fn cancel(
    &self,
    request: crate::FactoryNodeExecutionRequest<'_>,
    requested_at: Timestamp,
  ) -> Result<crate::FactoryNodeCancellationStep, ApplicationError> {
    let intent = request.build.ok_or_else(ApplicationError::invalid)?;
    if intent.node() != request.attempt || intent.input() != request.input {
      return Err(ApplicationError::invalid());
    }
    match FactoryNodeBuildAdapter::cancel(
      self,
      intent,
      request.admitted,
      request.ownership,
      requested_at,
      request.observed_at,
    )
    .await?
    {
      FactoryNodeBuildCancellation::Waiting(execution) => Ok(crate::FactoryNodeCancellationStep::Waiting {
        execution: Some(execution),
      }),
      FactoryNodeBuildCancellation::Stopped { execution } => {
        let usage = match &execution {
          Some(row) => match self.outputs.terminal_usage(row.build_id(), row.attempt_id()).await {
            Ok(usage) => usage,
            // The owned Job has stopped, but settlement still needs real usage.
            // Keep its reservation without blocking unrelated Run cancellation.
            Err(crate::OrdinaryBuildApplicationError::Unavailable) => {
              return Ok(crate::FactoryNodeCancellationStep::Waiting { execution });
            }
            Err(error) => return Err(build_error(error)),
          },
          None => BudgetUsage::default(), // No operation was ever accepted; no execution resources were consumed.
        };
        if usage.attempts != 0 {
          return Err(ApplicationError::invalid());
        }
        Ok(crate::FactoryNodeCancellationStep::Stopped { execution, usage })
      }
    }
  }
  async fn observe(
    &self,
    request: crate::FactoryNodeExecutionRequest<'_>,
  ) -> Result<crate::FactoryNodeExecutionStep, ApplicationError> {
    let intent = request.build.ok_or_else(ApplicationError::invalid)?;
    if intent.node() != request.attempt || intent.input() != request.input {
      return Err(ApplicationError::invalid());
    }
    let schema = request
      .admitted
      .closure()
      .definition(request.input.definition())
      .and_then(|definition| definition.node(request.input.node()))
      .and_then(|node| node.outcome(intent.result_outcome()))
      .and_then(|outcome| request.admitted.data_schema(outcome.schema()))
      .ok_or_else(ApplicationError::invalid)?;
    match self
      .observe_or_dispatch(intent, request.admitted, schema, request.ownership, request.observed_at)
      .await?
    {
      FactoryNodeBuildStep::Waiting(execution) => Ok(crate::FactoryNodeExecutionStep::Waiting {
        execution: Some(execution),
      }),
      FactoryNodeBuildStep::ExecutionStopped { execution, usage, .. } => {
        let accepted = self
          .builds
          .factory_build_for_operation(intent.operation_id().map_err(invalid)?)
          .await
          .map_err(build_error)?
          .ok_or_else(ApplicationError::invalid)?;
        validate_acceptance(&accepted, intent.permissions().digest())?;
        if accepted.build_id != execution.build_id() {
          return Err(ApplicationError::invalid());
        }
        let observed = ObservedBuild {
          build_id: execution.build_id(),
          attempt_id: execution.attempt_id(),
          jobs: execution.jobs().to_vec(),
        };
        if !owned_execution_has_stopped(self.builds.as_ref(), &accepted, &observed, request.observed_at).await? {
          return Ok(crate::FactoryNodeExecutionStep::Waiting {
            execution: Some(execution),
          });
        }
        let usage = match usage {
          Some(usage) => usage,
          None => match self
            .outputs
            .terminal_usage(execution.build_id(), execution.attempt_id())
            .await
          {
            Ok(usage) => usage,
            // Preserve this execution and its reservation. Unknown consumption
            // is a local wait, not a failure of the shared reconciliation pass.
            Err(crate::OrdinaryBuildApplicationError::Unavailable) => {
              return Ok(crate::FactoryNodeExecutionStep::Waiting {
                execution: Some(execution),
              });
            }
            Err(error) => return Err(build_error(error)),
          },
        };
        Ok(crate::FactoryNodeExecutionStep::ExecutionStopped {
          execution: Some(execution),
          usage,
        })
      }
      FactoryNodeBuildStep::Completed(done) => Ok(crate::FactoryNodeExecutionStep::Completed {
        execution: Some(Box::new(done.execution)),
        record: Box::new(done.record),
        usage: done.completion.usage(),
        outbox: vec![],
      }),
    }
  }
}
