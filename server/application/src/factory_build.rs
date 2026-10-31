//! Ordinary Build contracts and observations for configured Factory nodes.
//! Logical node names and business routes carry no execution authority here.

use async_trait::async_trait;
use octacity_protocol::{
  CHANGE_SET_BUNDLE_INPUT, CHANGE_SET_BUNDLE_MEDIA_TYPE, CHANGE_SET_MANIFEST_INPUT, CHANGE_SET_MANIFEST_MEDIA_TYPE,
  ChangeSetMaterializationV3, ProtectedInputV3,
};
use octacity_server_artifacts::ArtifactIdentity;
use octacity_server_domain::{AttemptId, BuildId, ImmutableRevision, JobId, ProjectId, Timestamp};
use octacity_server_factory::{BuildConfigurationRef, FactoryDigest, FactoryPermissionSet, LocalPermissionCeiling};
use octacity_server_job::{JobFailureClass, JobState};
use octacity_server_orchestrator::BuildState;
use octacity_server_store::{AttemptRecord, BuildRecord};
use thiserror::Error;

/// Absolute bounded ordinary Job/output history accepted by Factory adapters.
pub(super) use octacity_server_store::MAX_FACTORY_BUILD_JOBS;

/// Immutable permission layers used to narrow one Factory-owned Build.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryBuildPolicyLayers {
  /// Maximum authority allowed by the effective Project policy.
  pub project: FactoryPermissionSet,
  /// Maximum authority allowed by the immutable Factory Configuration.
  pub configuration: FactoryPermissionSet,
  /// Maximum authority requested by the immutable task input.
  pub task: FactoryPermissionSet,
  /// Locally advertised semantic and enforcement ceiling.
  pub local: LocalPermissionCeiling,
}

/// Frozen inputs and profile for a generic Build or reasoning node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryNodeBuildCausality {
  /// Owning Factory Run.
  pub run_id: octacity_server_factory::FactoryRunId,
  /// Exact immutable Flow execution.
  pub flow_run_id: octacity_server_factory::FlowRunId,
  /// Append-only workflow cycle.
  pub cycle_id: octacity_server_factory::WorkflowCycleId,
  /// Persisted Node Attempt owning this operation.
  pub node_attempt_id: octacity_server_factory::NodeAttemptId,
  /// Exact operator-selected model, command, task and Build configuration.
  pub profile: octacity_server_factory::FlowBuildProfile,
  /// Exact installed trusted-action parameters; absent for model and command nodes.
  pub action: Option<octacity_server_factory::FlowActionBinding>,
  /// Bounded protected input derived by the installed action from immutable owner
  /// facts. Recomputed identically on replay; absent for model and command nodes.
  pub action_input: Option<serde_json::Value>,
  /// Exact public deliverable requirements frozen for task compilation.
  /// Contains acceptance and scope, never private test content or Artifact grants.
  pub deliverable_contract: Option<octacity_server_factory::CandidateDeliverableContract>,
  /// Common immutable verification plan and exact per-check target/input identity.
  /// Absent for ordinary nodes that select no verification contract.
  pub verification_check: Option<octacity_server_factory::VerificationCheckInput>,
  /// Exact typed task input schema.
  pub input_schema: octacity_server_factory::ImmutableReference,
  /// Complete frozen task document, bounded by its schema before dispatch.
  pub input: Vec<u8>,
  /// Validated semantic digest of that exact task input.
  pub input_digest: FactoryDigest,
  /// Exact frozen context with retained source identities.
  pub context: octacity_server_factory::ContextManifest,
}

/// Exact accepted ChangeSet inputs supplied to a later ordinary Build.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryCandidateMaterialization {
  /// Accepted immutable ChangeSet identity.
  pub change_set_id: octacity_server_factory::ChangeSetId,
  /// Original exact repository revision materialized by the source plugin.
  pub base_revision: ImmutableRevision,
  /// Exact candidate commit reconstructed from the bundle.
  pub candidate_revision: ImmutableRevision,
  /// Verified generic output containing the Git bundle.
  pub bundle: ArtifactIdentity,
  /// Verified generic output containing the capture manifest.
  pub manifest: ArtifactIdentity,
}

impl FactoryCandidateMaterialization {
  /// Produces the strict wire instruction and protected inputs used by JobSpec v3.
  pub fn wire_inputs(&self) -> (ChangeSetMaterializationV3, Vec<ProtectedInputV3>) {
    let bundle_input = self.bundle.artifact_id.to_string();
    let manifest_input = self.manifest.artifact_id.to_string();
    let instruction = ChangeSetMaterializationV3 {
      bundle_input: bundle_input.clone(),
      manifest_input: manifest_input.clone(),
      candidate_revision: self.candidate_revision.as_str().to_owned(),
    };
    let mut inputs = vec![
      ProtectedInputV3 {
        artifact_id: bundle_input,
        size_bytes: self.bundle.size_bytes,
        sha256: self.bundle.digest.to_string(),
        media_type: CHANGE_SET_BUNDLE_MEDIA_TYPE.to_owned(),
        destination: CHANGE_SET_BUNDLE_INPUT.to_owned(),
      },
      ProtectedInputV3 {
        artifact_id: manifest_input,
        size_bytes: self.manifest.size_bytes,
        sha256: self.manifest.digest.to_string(),
        media_type: CHANGE_SET_MANIFEST_MEDIA_TYPE.to_owned(),
        destination: CHANGE_SET_MANIFEST_INPUT.to_owned(),
      },
    ];
    inputs.sort_by(|left, right| left.artifact_id.cmp(&right.artifact_id));
    (instruction, inputs)
  }
}

/// Idempotent request to the existing ordinary Build application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateFactoryBuild {
  /// Current fenced dispatch owner, excluded from the stable operation identity.
  pub ownership: octacity_server_factory::FactoryClaimOwnership,
  /// Stable logical identity reused after an unknown response.
  pub operation_id: FactoryDigest,
  /// Project owning both the Run and selected Build Configuration.
  pub project_id: ProjectId,
  /// Exact selected task resource, or explicit Work-input scope with no code attachment.
  pub subject: octacity_server_factory::FactoryTaskSubject,
  /// Exact immutable Build Configuration selected by the node.
  pub build_configuration: BuildConfigurationRef,
  /// Existing Build ready-queue priority.
  pub priority: i64,
  /// Effective Factory permissions after pure narrowing.
  pub effective_permissions: FactoryPermissionSet,
  /// Hard execution ceiling of the owning Node Attempt.
  pub budget: octacity_server_factory::BudgetLimit,
  /// Authoritative stop deadline inherited from the persisted execution intent.
  pub deadline: Timestamp,
  /// Factory causality retained outside the ordinary Build aggregate.
  pub causality: Box<FactoryNodeBuildCausality>,
}

/// Identifiers returned by the ordinary Build application after acceptance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryBuildAcceptance {
  /// Accepted ordinary Build.
  pub build_id: BuildId,
  /// First immutable Attempt created by ordinary Build admission.
  pub attempt_id: AttemptId,
  /// Complete initial Job DAG identities.
  pub job_ids: Vec<JobId>,
  /// Digest of the effective Factory permissions actually accepted.
  pub effective_policy_digest: FactoryDigest,
}

/// Failure from the existing Build application boundary.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrdinaryBuildApplicationError {
  /// The immutable request is invalid under ordinary Build policy.
  #[error("ordinary Build request is invalid")]
  Invalid,
  /// A stable operation identity was already used with different input.
  #[error("ordinary Build request conflicts with an earlier intent")]
  Conflict,
  /// The ordinary Build application is temporarily unavailable.
  #[error("ordinary Build application is unavailable")]
  Unavailable,
}

/// Narrow facade over the existing ordinary Build creation application.
///
/// Implementations must preserve the regular immutable Build, Attempt, Job,
/// cancellation, retry, output, and terminal-state contracts. Factory
/// causality is retained as immutable Build origin, separate from execution state.
/// Enforce the supplied aggregate output ceiling and execution deadline.
/// Management Build Retry carries no Factory ownership authority; retries use
/// the exact Factory RetryNode control and consume a new configured attempt.
/// An idempotent replay may recover an existing Build after its deadline.
#[async_trait]
pub trait OrdinaryBuildApplication: Send + Sync {
  /// Reads an already accepted operation without creating, retrying or starting execution.
  /// Recovery may call this after the original execution deadline; absence must not dispatch work.
  async fn factory_build_for_operation(
    &self,
    operation_id: FactoryDigest,
  ) -> Result<Option<FactoryBuildAcceptance>, OrdinaryBuildApplicationError>;

  /// Creates or observes the one ordinary Build for a stable operation.
  async fn create_factory_build(
    &self,
    request: CreateFactoryBuild,
  ) -> Result<FactoryBuildAcceptance, OrdinaryBuildApplicationError>;
}

/// Exact cancellation of an already accepted Factory-owned ordinary Build.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelFactoryBuild {
  /// Current owner authorizing fresh cancellation, separate from logical operation identity.
  pub ownership: octacity_server_factory::FactoryClaimOwnership,
  /// Stable cancellation operation; repeated observations reuse this identity.
  pub operation_id: FactoryDigest,
  /// Original creation operation, used to confirm the exact ordinary Build.
  pub build_operation_id: FactoryDigest,
  /// Already accepted Build; cancellation must never create an absent operation.
  pub build_id: BuildId,
  /// Original retained cancellation time, unchanged across retries.
  pub requested_at: Timestamp,
}

/// Explicit ordinary Build cancellation capability installed alongside creation.
#[async_trait]
pub trait OrdinaryBuildCancellationApplication: OrdinaryBuildApplication {
  /// Confirms the creation-operation binding and applies idempotent cancellation.
  /// Returning success records intent; terminal state is still read independently.
  async fn cancel_factory_build(&self, request: CancelFactoryBuild) -> Result<(), OrdinaryBuildApplicationError>;
}

/// Authoritative ordinary Build observation consumed by Factory coordination.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryBuildObservation {
  /// Ordinary Build observed through the existing read API.
  build_id: BuildId,
  /// Current ordinary Build state.
  state: BuildState,
  /// Latest immutable Attempt.
  attempt_id: AttemptId,
  /// Complete Job DAG of that Attempt.
  job_ids: Vec<JobId>,
  /// Published logical outputs with exact immutable content provenance.
  outputs: Vec<ArtifactIdentity>,
  /// Whether every failed Job was classified as an infrastructure failure.
  infrastructure_retry_eligible: bool,
  /// Whether at least one failed Job reached its signed execution deadline.
  timed_out: bool,
}

impl FactoryBuildObservation {
  /// Reconstructs a terminal capability observation from the exact common
  /// execution and independently retained ordinary Build, Attempt and outputs.
  /// A terminal Build alone is insufficient while any owned Job is stopping.
  pub fn from_execution(
    execution: &octacity_server_factory::FlowBuildExecution,
    build: &BuildRecord,
    attempt: &AttemptRecord,
    outputs: Vec<ArtifactIdentity>,
    at: Timestamp,
  ) -> Result<Self, crate::ApplicationError> {
    use octacity_server_orchestrator::AttemptState;
    use std::collections::BTreeSet;
    let jobs = attempt.jobs.iter().map(|job| job.id()).collect::<BTreeSet<_>>();
    let valid_state = matches!(
      (build.state, attempt.state),
      (BuildState::Succeeded, AttemptState::Succeeded)
        | (BuildState::Failed, AttemptState::Failed)
        | (BuildState::Cancelled, AttemptState::Cancelled)
    );
    if build.build.id != execution.build_id()
      || attempt.build_id != execution.build_id()
      || attempt.id != execution.attempt_id()
      || !valid_state
      || build.created_at > at
      || build.updated_at > at
      || attempt.created_at > at
      || attempt.updated_at > at
      || jobs.is_empty()
      || jobs.len() > MAX_FACTORY_BUILD_JOBS
      || jobs.len() != attempt.jobs.len()
      || jobs != execution.jobs().iter().copied().collect()
      || attempt.jobs.iter().any(|job| {
        job.attempt_id != attempt.id
          || !job.state.is_terminal()
          || job.updated_at > at
          || (build.state == BuildState::Succeeded && job.state != JobState::Succeeded)
      })
      || outputs.len() > MAX_FACTORY_BUILD_JOBS
      || outputs
        .iter()
        .map(|output| output.artifact_id)
        .collect::<BTreeSet<_>>()
        .len()
        != outputs.len()
      || outputs.iter().any(|output| {
        output.build_id != execution.build_id()
          || output.attempt_id != execution.attempt_id()
          || !jobs.contains(&output.job_id)
      })
    {
      return Err(crate::ApplicationError::invalid());
    }
    Ok(Self {
      build_id: execution.build_id(),
      state: build.state,
      attempt_id: execution.attempt_id(),
      job_ids: execution.jobs().to_vec(),
      outputs,
      infrastructure_retry_eligible: build.state == BuildState::Failed && infrastructure_retry_eligible(attempt),
      timed_out: build.state == BuildState::Failed
        && attempt
          .jobs
          .iter()
          .any(|job| job.terminal.is_some_and(|terminal| terminal.timed_out)),
    })
  }
  /// Returns the observed ordinary Build.
  #[must_use]
  pub const fn build_id(&self) -> BuildId {
    self.build_id
  }

  /// Returns the authoritative Build state.
  #[must_use]
  pub const fn state(&self) -> BuildState {
    self.state
  }

  /// Returns the observed latest Attempt.
  #[must_use]
  pub const fn attempt_id(&self) -> AttemptId {
    self.attempt_id
  }

  /// Returns the complete Job DAG identities.
  #[must_use]
  pub fn job_ids(&self) -> &[JobId] {
    &self.job_ids
  }

  /// Returns exact published output identities.
  #[must_use]
  pub fn outputs(&self) -> &[ArtifactIdentity] {
    &self.outputs
  }

  /// Returns whether every failed Job had an infrastructure failure.
  #[must_use]
  pub const fn infrastructure_retry_eligible(&self) -> bool {
    self.infrastructure_retry_eligible
  }

  /// Returns whether a signed execution deadline caused failure.
  #[must_use]
  pub const fn timed_out(&self) -> bool {
    self.timed_out
  }

  #[cfg(test)]
  pub(super) fn fixture(
    build_id: BuildId,
    state: BuildState,
    attempt_id: AttemptId,
    job_ids: Vec<JobId>,
    outputs: Vec<ArtifactIdentity>,
  ) -> Self {
    Self {
      build_id,
      state,
      attempt_id,
      job_ids,
      outputs,
      infrastructure_retry_eligible: false,
      timed_out: false,
    }
  }

  #[cfg(test)]
  pub(super) fn set_terminal_cause(&mut self, timed_out: bool, infrastructure_retry_eligible: bool) {
    self.timed_out = timed_out;
    self.infrastructure_retry_eligible = infrastructure_retry_eligible;
  }

  #[cfg(test)]
  pub(super) fn set_state(&mut self, state: BuildState) {
    self.state = state;
  }

  #[cfg(test)]
  pub(super) fn outputs_mut(&mut self) -> &mut Vec<ArtifactIdentity> {
    &mut self.outputs
  }
}

fn infrastructure_retry_eligible(attempt: &octacity_server_store::AttemptRecord) -> bool {
  failed_job_classes_are_retryable(
    attempt
      .jobs
      .iter()
      .filter(|job| job.state == JobState::Failed)
      .map(|job| job.terminal.and_then(|terminal| terminal.failure_class)),
  )
}

pub(super) fn failed_job_classes_are_retryable(classes: impl IntoIterator<Item = Option<JobFailureClass>>) -> bool {
  let mut saw_failure = false;
  for class in classes {
    saw_failure = true;
    if class != Some(JobFailureClass::Infrastructure) {
      return false;
    }
  }
  saw_failure
}
