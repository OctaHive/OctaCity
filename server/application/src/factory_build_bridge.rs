use std::{
  sync::Arc,
  time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use octacity_protocol::{
  CHANGE_SET_BUNDLE_INPUT, CHANGE_SET_BUNDLE_MEDIA_TYPE, CHANGE_SET_MANIFEST_INPUT, CHANGE_SET_MANIFEST_MEDIA_TYPE,
  ChangeSetMaterializationV3, ProtectedInputV3,
};
use octacity_server_artifacts::ArtifactIdentity;
use octacity_server_domain::{AttemptId, BuildId, ImmutableRevision, JobId, ProjectId, RepositoryId, Timestamp};
use octacity_server_factory::{
  BuildConfigurationRef, EvaluationBranchState, EvaluationState, FactoryDigest, FactoryKey, FactoryLifecycleProgress,
  FactoryPermissionSet, FactoryRun, FactoryRunVersion, FactoryStageProgress, FactoryStageTarget,
  LocalPermissionCeiling, StageAttempt, resolve_factory_permissions,
};
use octacity_server_job::{JobFailureClass, JobState};
use octacity_server_orchestrator::BuildState;
use octacity_server_store::{
  AttemptRecord, AuditActorKind, BuildQueryStore, BuildRecord, ClaimedFactoryOutbox, ClaimedFactoryRun,
  CommitFactoryRunTransition, FactoryAuditFact, FactoryBudgetRecord, FactoryBuildLink, FactoryBuildLinkInput,
  FactoryBuildObservationInput, FactoryBuildObservationRecord, FactoryBuildParent, FactoryConfigurationStore,
  FactoryLifecycleCheckpoint, FactoryOutboxSettlement, FactoryRunHistoryAppend, FactoryRunSnapshot, FactoryRunStore,
  SettleFactoryOutbox, StoreError,
};
use thiserror::Error;

use crate::MutationDisposition;

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

impl FactoryBuildPolicyLayers {
  fn effective(&self) -> Result<FactoryPermissionSet, FactoryBuildBridgeError> {
    resolve_factory_permissions(&self.project, &self.configuration, &self.task, &self.local)
      .map_err(|_| FactoryBuildBridgeError::InvalidSnapshot)
  }
}

/// Exact immutable identities needed to resolve Build permission layers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryBuildPolicyRequest {
  /// Exact Project-owned Build Configuration selected for this stage.
  pub build_configuration: BuildConfigurationRef,
  /// Exact permission-ceiling definition selected by Factory Configuration.
  pub configuration_permission_digest: FactoryDigest,
  /// Digest of the immutable task input for this Stage Attempt.
  pub task_envelope_digest: FactoryDigest,
}

/// Resolves immutable permission definitions without exposing their storage.
#[async_trait]
pub trait FactoryBuildPolicySource: Send + Sync {
  /// Loads the three immutable permission layers selected by the request.
  async fn policy_layers(
    &self,
    request: FactoryBuildPolicyRequest,
  ) -> Result<FactoryBuildPolicyLayers, FactoryBuildPolicySourceError>;
}

/// Stable failure classification for immutable Factory Build policy loading.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum FactoryBuildPolicySourceError {
  /// One exact referenced policy definition no longer exists.
  #[error("Factory Build policy was not found")]
  NotFound,
  /// Persisted policy data is malformed or inconsistent with its reference.
  #[error("Factory Build policy is invalid")]
  Invalid,
  /// The authoritative policy source is temporarily unavailable.
  #[error("Factory Build policy is unavailable")]
  Unavailable,
}

/// Bounded Factory causality supplied beside an ordinary Build command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryStageBuildCausality {
  /// Factory Run owning the Stage Attempt.
  pub run_id: octacity_server_factory::FactoryRunId,
  /// Append-only Stage Attempt identity.
  pub stage_attempt_id: octacity_server_factory::StageAttemptId,
  /// Exact program-owned stage or evaluator branch target.
  pub target: FactoryStageTarget,
  /// Exact predecessor that made this Build eligible.
  pub parent: Option<FactoryBuildParent>,
  /// Accepted candidate bytes required by every non-initial stage.
  pub candidate: Option<FactoryCandidateMaterialization>,
  /// Digest of the immutable Task Envelope input.
  pub task_envelope_digest: FactoryDigest,
}

/// Causality of any Factory-owned ordinary Build.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FactoryBuildCausality {
  /// Lossless fixed-stage projection used by existing Factory consumers.
  Stage(Box<FactoryStageBuildCausality>),
  /// Generic Flow node with no synthetic fixed-stage identity.
  Node(Box<FactoryNodeBuildCausality>),
}
impl FactoryBuildCausality {
  /// Returns fixed-stage causality when this Build projects a fixed stage.
  #[must_use]
  pub fn stage(&self) -> Option<&FactoryStageBuildCausality> {
    match self {
      Self::Stage(stage) => Some(stage),
      Self::Node(_) => None,
    }
  }
  /// Returns generic node causality for ordinary Flow execution.
  #[must_use]
  pub fn node(&self) -> Option<&FactoryNodeBuildCausality> {
    match self {
      Self::Node(node) => Some(node),
      Self::Stage(_) => None,
    }
  }
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
  /// Stable logical identity reused after an unknown response.
  pub operation_id: FactoryDigest,
  /// Project owning both the Run and selected Build Configuration.
  pub project_id: ProjectId,
  /// Repository selected by the immutable Work subject.
  pub repository_id: RepositoryId,
  /// Exact immutable Build Configuration selected by the stage.
  pub build_configuration: BuildConfigurationRef,
  /// Exact immutable revision the ordinary Build must materialize.
  pub immutable_revision: ImmutableRevision,
  /// Existing Build ready-queue priority.
  pub priority: i64,
  /// Effective Factory permissions after pure narrowing.
  pub effective_permissions: FactoryPermissionSet,
  /// Hard execution ceiling of the owning Stage or Node Attempt.
  pub budget: octacity_server_factory::BudgetLimit,
  /// Authoritative stop deadline inherited from the persisted execution intent.
  pub deadline: Timestamp,
  /// Factory causality retained outside the ordinary Build aggregate.
  pub causality: FactoryBuildCausality,
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
/// causality is returned to this bridge and is not added to Build state.
/// Enforce the supplied budget/deadline across ordinary retries; an idempotent
/// replay may return an existing Build but must never start a new expired one.
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

/// Reads the bounded published-output provenance of one ordinary Build Attempt.
#[async_trait]
pub trait FactoryBuildOutputSource: Send + Sync {
  /// Returns all published logical outputs for the exact terminal Attempt.
  async fn published_outputs(
    &self,
    build_id: BuildId,
    attempt_id: AttemptId,
  ) -> Result<Vec<ArtifactIdentity>, OrdinaryBuildApplicationError>;
}

/// Result of dispatching one durable `build.create` operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryBuildDispatchOutcome {
  /// Whether the Factory link was newly committed or already present.
  pub disposition: MutationDisposition,
  /// Ordinary Build linked to the Stage Attempt.
  pub build_id: BuildId,
  /// First ordinary Attempt created for the Build.
  pub attempt_id: AttemptId,
  /// Initial ordinary Job DAG identities.
  pub job_ids: Vec<JobId>,
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
  /// Whether a new terminal Factory checkpoint was committed.
  disposition: Option<MutationDisposition>,
}

impl FactoryBuildObservation {
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

  /// Returns the terminal checkpoint mutation disposition, when terminal.
  #[must_use]
  pub const fn disposition(&self) -> Option<MutationDisposition> {
    self.disposition
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
      disposition: None,
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

/// Failure while dispatching or observing one Factory-owned ordinary Build.
#[derive(Debug, Error)]
pub enum FactoryBuildBridgeError {
  /// The authoritative wall clock could not be represented safely.
  #[error("Factory Build bridge clock is unavailable")]
  ClockUnavailable,
  /// Persisted Factory state does not match the claimed operation.
  #[error("Factory Build bridge snapshot is invalid")]
  InvalidSnapshot,
  /// Immutable permission layers could not be loaded.
  #[error("Factory Build policy could not be loaded")]
  Policy(#[from] FactoryBuildPolicySourceError),
  /// The ordinary Build application rejected or could not complete admission.
  #[error("ordinary Build application failed")]
  Build(#[from] OrdinaryBuildApplicationError),
  /// The authoritative store rejected or could not complete an operation.
  #[error("Factory Build bridge store failed")]
  Store(#[from] StoreError),
}

/// Bridges durable Factory Stage Attempts to the existing Build application.
pub struct FactoryBuildBridge<S, B, P> {
  store: Arc<S>,
  builds: Arc<B>,
  policies: Arc<P>,
  clock: Arc<dyn FactoryBuildClock>,
}

impl<S, B, P> FactoryBuildBridge<S, B, P>
where
  S: FactoryRunStore + FactoryConfigurationStore + 'static,
  B: OrdinaryBuildApplication + BuildQueryStore + FactoryBuildOutputSource + 'static,
  P: FactoryBuildPolicySource + 'static,
{
  /// Creates a bridge from narrow authoritative boundaries.
  pub fn new(store: Arc<S>, builds: Arc<B>, policies: Arc<P>) -> Self {
    Self::with_clock(store, builds, policies, Arc::new(SystemFactoryBuildClock))
  }

  fn with_clock(store: Arc<S>, builds: Arc<B>, policies: Arc<P>, clock: Arc<dyn FactoryBuildClock>) -> Self {
    Self {
      store,
      builds,
      policies,
      clock,
    }
  }

  #[cfg(test)]
  pub(super) fn new_with_clock(
    store: Arc<S>,
    builds: Arc<B>,
    policies: Arc<P>,
    clock: Arc<dyn FactoryBuildClock>,
  ) -> Self {
    Self::with_clock(store, builds, policies, clock)
  }

  /// Dispatches one claimed `build.create` operation and records only the
  /// bounded Factory causal link after ordinary Build acceptance.
  pub async fn dispatch(
    &self,
    claimed: ClaimedFactoryOutbox,
  ) -> Result<FactoryBuildDispatchOutcome, FactoryBuildBridgeError> {
    let observed_at = self.now()?;
    let snapshot = self.store.factory_run_snapshot(claimed.record.run_id).await?;
    let context = DispatchContext::load(&snapshot, &claimed, observed_at)?;
    if let Some(link) = snapshot
      .linked_builds
      .iter()
      .find(|link| link.stage_attempt_id == context.stage.id())
    {
      settle_delivered(self.store.as_ref(), &claimed, self.now()?).await?;
      return Ok(FactoryBuildDispatchOutcome {
        disposition: MutationDisposition::Replayed,
        build_id: link.build_id,
        attempt_id: link.attempt_id,
        job_ids: link.job_ids.clone(),
      });
    }
    let configuration = self
      .store
      .factory_configuration_version(
        snapshot.run.configuration().id(),
        snapshot.run.configuration().version(),
      )
      .await?;
    let stage_definition = configuration
      .configuration
      .stages()
      .iter()
      .find(|definition| definition.kind() == context.stage.kind())
      .ok_or(FactoryBuildBridgeError::InvalidSnapshot)?;
    let (immutable_revision, parent, candidate) = build_subject(&snapshot, &context.target)?;
    let layers = self
      .policies
      .policy_layers(FactoryBuildPolicyRequest {
        build_configuration: stage_definition.build_configuration().clone(),
        configuration_permission_digest: configuration.configuration.permission_ceiling().digest(),
        task_envelope_digest: context.stage.input_digest(),
      })
      .await?;
    let effective_permissions = layers.effective()?;
    debug_assert!(effective_permissions.is_no_broader_than(&layers.project));
    debug_assert!(effective_permissions.is_no_broader_than(&layers.configuration));
    debug_assert!(effective_permissions.is_no_broader_than(&layers.task));
    debug_assert!(effective_permissions.is_no_broader_than(layers.local.grants()));
    let effective_policy_digest = effective_permissions.digest();
    let request = CreateFactoryBuild {
      operation_id: claimed.record.operation_id,
      project_id: snapshot.run.subject().project_id(),
      repository_id: snapshot.run.subject().repository_id(),
      build_configuration: stage_definition.build_configuration().clone(),
      immutable_revision: immutable_revision.clone(),
      priority: i64::from(snapshot.work.priority().get()),
      effective_permissions,
      budget: context.stage.budget(),
      deadline: context.stage.claim().expires_at(),
      causality: FactoryBuildCausality::Stage(Box::new(FactoryStageBuildCausality {
        run_id: snapshot.run.id(),
        stage_attempt_id: context.stage.id(),
        target: context.target.clone(),
        parent,
        candidate,
        task_envelope_digest: context.stage.input_digest(),
      })),
    };
    let input_digest = build_input_digest(&request)?;
    let accepted = self.builds.create_factory_build(request).await?;
    let mut accepted_job_ids = accepted.job_ids.clone();
    accepted_job_ids.sort_unstable();
    if accepted.effective_policy_digest != effective_policy_digest
      || accepted.job_ids.is_empty()
      || accepted.job_ids.len() > octacity_server_store::MAX_MATERIALIZED_JOBS
      || accepted_job_ids.windows(2).any(|pair| pair[0] == pair[1])
    {
      return Err(FactoryBuildBridgeError::InvalidSnapshot);
    }
    let link = FactoryBuildLink::new(
      context.stage,
      FactoryBuildLinkInput {
        build_id: accepted.build_id,
        attempt_id: accepted.attempt_id,
        job_ids: accepted.job_ids.clone(),
        factory_configuration: snapshot.run.configuration().clone(),
        target: context.target.clone(),
        build_configuration: stage_definition.build_configuration().clone(),
        task_envelope_digest: context.stage.input_digest(),
        exact_revision: immutable_revision,
        parent,
        effective_policy_digest,
        input_digest,
      },
    );
    commit_link(self.store.as_ref(), &snapshot, &claimed, context, link, self.now()?).await?;
    settle_delivered(self.store.as_ref(), &claimed, self.now()?).await?;
    Ok(FactoryBuildDispatchOutcome {
      disposition: MutationDisposition::Applied,
      build_id: accepted.build_id,
      attempt_id: accepted.attempt_id,
      job_ids: accepted.job_ids,
    })
  }

  /// Observes one linked Build through the existing Build read API and records
  /// only the corresponding terminal Factory checkpoint.
  pub async fn observe(&self, claimed: &ClaimedFactoryRun) -> Result<FactoryBuildObservation, FactoryBuildBridgeError> {
    let observed_at = self.now()?;
    let snapshot = self.store.factory_run_snapshot(claimed.run_id).await?;
    validate_run_claim(&snapshot, claimed, observed_at)?;
    let build_id = snapshot
      .current
      .build_id
      .ok_or(FactoryBuildBridgeError::InvalidSnapshot)?;
    let link = snapshot
      .linked_builds
      .iter()
      .find(|link| link.build_id == build_id)
      .ok_or(FactoryBuildBridgeError::InvalidSnapshot)?;
    if snapshot.current.stage_attempt_id != Some(link.stage_attempt_id) {
      return Err(FactoryBuildBridgeError::InvalidSnapshot);
    }
    let build = self.builds.build(build_id).await?;
    if build.build.project_id != snapshot.run.subject().project_id()
      || build.build.configuration_id != link.build_configuration.id()
      || build.build.configuration_version != link.build_configuration.version()
      || build.build.repository_id != snapshot.run.subject().repository_id()
      || build.build.immutable_revision != link.exact_revision
    {
      return Err(FactoryBuildBridgeError::InvalidSnapshot);
    }
    let attempt = self.builds.latest_attempt(build_id).await?;
    let job_ids: Vec<JobId> = attempt.jobs.iter().map(|job| job.id()).collect();
    let infrastructure_retry_eligible = build.state == BuildState::Failed && infrastructure_retry_eligible(&attempt);
    let timed_out = build.state == BuildState::Failed
      && attempt
        .jobs
        .iter()
        .any(|job| job.terminal.is_some_and(|terminal| terminal.timed_out));
    let (outputs, disposition) = if build.state.is_terminal() {
      if progress_reflects_state(
        &current_checkpoint(&snapshot)?.progress,
        &link.target,
        build.state,
        infrastructure_retry_eligible,
      ) {
        let observation = terminal_observation(&snapshot, link, &build, &attempt, infrastructure_retry_eligible)?;
        (observation.outputs.clone(), Some(MutationDisposition::Replayed))
      } else {
        let outputs = self.builds.published_outputs(build_id, attempt.id).await?;
        let committed_at = self.now()?;
        let observation = FactoryBuildObservationRecord::new(
          link,
          FactoryBuildObservationInput {
            build_version: build.version,
            attempt_id: attempt.id,
            attempt_version: attempt.version,
            job_ids: job_ids.clone(),
            outputs: outputs.clone(),
            state: build.state,
            infrastructure_retry_eligible,
            observed_at: committed_at,
          },
        )?;
        commit_terminal_observation(
          self.store.as_ref(),
          &snapshot,
          claimed,
          link,
          &observation,
          committed_at,
        )
        .await?;
        (outputs, Some(MutationDisposition::Applied))
      }
    } else {
      (Vec::new(), None)
    };
    Ok(FactoryBuildObservation {
      build_id,
      state: build.state,
      attempt_id: attempt.id,
      job_ids,
      outputs,
      infrastructure_retry_eligible,
      timed_out,
      disposition,
    })
  }

  fn now(&self) -> Result<Timestamp, FactoryBuildBridgeError> {
    self.clock.now()
  }
}

pub(super) trait FactoryBuildClock: Send + Sync {
  fn now(&self) -> Result<Timestamp, FactoryBuildBridgeError>;
}

struct SystemFactoryBuildClock;

impl FactoryBuildClock for SystemFactoryBuildClock {
  fn now(&self) -> Result<Timestamp, FactoryBuildBridgeError> {
    let milliseconds = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .ok()
      .and_then(|duration| i64::try_from(duration.as_millis()).ok())
      .ok_or(FactoryBuildBridgeError::ClockUnavailable)?;
    Timestamp::from_unix_millis(milliseconds).map_err(|_| FactoryBuildBridgeError::ClockUnavailable)
  }
}

struct DispatchContext<'a> {
  stage: &'a StageAttempt,
  target: FactoryStageTarget,
  checkpoint: &'a FactoryLifecycleCheckpoint,
}

impl<'a> DispatchContext<'a> {
  fn load(
    snapshot: &'a FactoryRunSnapshot,
    claimed: &ClaimedFactoryOutbox,
    observed_at: Timestamp,
  ) -> Result<Self, FactoryBuildBridgeError> {
    if claimed.record.kind.as_str() != "build.create"
      || claimed.record.state != octacity_server_store::FactoryOutboxState::Claimed
    {
      return Err(FactoryBuildBridgeError::InvalidSnapshot);
    }
    let outbox_owner = claimed
      .record
      .owner
      .as_ref()
      .ok_or(FactoryBuildBridgeError::InvalidSnapshot)?;
    let outbox_claim = claimed.record.claim.ok_or(FactoryBuildBridgeError::InvalidSnapshot)?;
    outbox_claim
      .verify_fence(outbox_claim.fence(), observed_at)
      .map_err(|_| FactoryBuildBridgeError::InvalidSnapshot)?;
    let run_claim = snapshot
      .current_claim
      .as_ref()
      .ok_or(FactoryBuildBridgeError::InvalidSnapshot)?;
    if &run_claim.owner != outbox_owner {
      return Err(FactoryBuildBridgeError::InvalidSnapshot);
    }
    run_claim
      .claim
      .verify_fence(run_claim.claim.fence(), observed_at)
      .map_err(|_| FactoryBuildBridgeError::InvalidSnapshot)?;
    let stage_id = snapshot
      .current
      .stage_attempt_id
      .ok_or(FactoryBuildBridgeError::InvalidSnapshot)?;
    let stage = snapshot
      .stage_attempts
      .iter()
      .find(|stage| stage.id() == stage_id)
      .ok_or(FactoryBuildBridgeError::InvalidSnapshot)?;
    let checkpoint = current_checkpoint(snapshot)?;
    let target = stage.target().clone();
    match &checkpoint.progress {
      FactoryLifecycleProgress::Stage {
        target: current,
        progress: FactoryStageProgress::AttemptCreated,
      } if current == &target => {}
      FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(branches))
        if matches!(&target, FactoryStageTarget::Evaluation(key) if branches
          .branches()
          .iter()
          .any(|branch| branch.key() == key && branch.state() == EvaluationBranchState::AttemptCreated)) => {}
      _ => return Err(FactoryBuildBridgeError::InvalidSnapshot),
    }
    Ok(Self {
      stage,
      target,
      checkpoint,
    })
  }
}

fn build_subject(
  snapshot: &FactoryRunSnapshot,
  target: &FactoryStageTarget,
) -> Result<
  (
    ImmutableRevision,
    Option<FactoryBuildParent>,
    Option<FactoryCandidateMaterialization>,
  ),
  FactoryBuildBridgeError,
> {
  let candidate = (!matches!(target, FactoryStageTarget::Implementation))
    .then(|| current_candidate(snapshot))
    .transpose()?;
  let decision = if matches!(target, FactoryStageTarget::Rework) {
    let id = snapshot
      .current
      .decision_id
      .ok_or(FactoryBuildBridgeError::InvalidSnapshot)?;
    let record = snapshot
      .decisions
      .iter()
      .find(|record| record.id() == id)
      .ok_or(FactoryBuildBridgeError::InvalidSnapshot)?;
    if candidate.is_none_or(|candidate| candidate.subject() != record.subject()) {
      return Err(FactoryBuildBridgeError::InvalidSnapshot);
    }
    Some((id, record.subject().candidate_revision().clone()))
  } else {
    None
  };
  let selected = selected_build_subject(
    target,
    snapshot.run.subject().base_revision().clone(),
    candidate.map(|record| (record.id(), record.subject().candidate_revision().clone())),
    decision,
  )?;
  let materialization = candidate
    .map(|candidate| candidate_materialization(snapshot, candidate))
    .transpose()?;
  Ok((selected.0, selected.1, materialization))
}

/// Selects the exact ordinary-Build revision and bounded Factory predecessor.
pub(super) fn selected_build_subject(
  target: &FactoryStageTarget,
  base_revision: ImmutableRevision,
  candidate: Option<(octacity_server_factory::ChangeSetId, ImmutableRevision)>,
  decision: Option<(octacity_server_factory::DecisionId, ImmutableRevision)>,
) -> Result<(ImmutableRevision, Option<FactoryBuildParent>), FactoryBuildBridgeError> {
  match target {
    FactoryStageTarget::Implementation => Ok((base_revision, None)),
    FactoryStageTarget::Validation | FactoryStageTarget::Evaluation(_) => candidate
      .map(|(id, _)| (base_revision, Some(FactoryBuildParent::ChangeSet(id))))
      .ok_or(FactoryBuildBridgeError::InvalidSnapshot),
    FactoryStageTarget::Rework => decision
      .map(|(id, _)| (base_revision, Some(FactoryBuildParent::Decision(id))))
      .ok_or(FactoryBuildBridgeError::InvalidSnapshot),
  }
}

fn candidate_materialization(
  snapshot: &FactoryRunSnapshot,
  candidate: &octacity_server_factory::ChangeSet,
) -> Result<FactoryCandidateMaterialization, FactoryBuildBridgeError> {
  let bundle = candidate_artifact(snapshot, candidate.bundle_artifact(), CHANGE_SET_BUNDLE_MEDIA_TYPE)?;
  let manifest = candidate_artifact(snapshot, candidate.manifest_artifact(), CHANGE_SET_MANIFEST_MEDIA_TYPE)?;
  Ok(FactoryCandidateMaterialization {
    change_set_id: candidate.id(),
    base_revision: candidate.subject().exact().base_revision().clone(),
    candidate_revision: candidate.subject().candidate_revision().clone(),
    bundle,
    manifest,
  })
}

fn candidate_artifact(
  snapshot: &FactoryRunSnapshot,
  artifact_id: octacity_server_domain::ArtifactId,
  media_type: &str,
) -> Result<ArtifactIdentity, FactoryBuildBridgeError> {
  let mut matches = snapshot
    .build_observations
    .iter()
    .flat_map(|observation| &observation.outputs)
    .filter(|identity| identity.artifact_id == artifact_id);
  let identity = matches.next().ok_or(FactoryBuildBridgeError::InvalidSnapshot)?;
  if matches.next().is_some() || identity.media_type.as_str() != media_type {
    return Err(FactoryBuildBridgeError::InvalidSnapshot);
  }
  Ok(identity.clone())
}

fn current_candidate(
  snapshot: &FactoryRunSnapshot,
) -> Result<&octacity_server_factory::ChangeSet, FactoryBuildBridgeError> {
  let id = snapshot
    .current
    .candidate_id
    .ok_or(FactoryBuildBridgeError::InvalidSnapshot)?;
  snapshot
    .candidates
    .iter()
    .find(|candidate| candidate.id() == id)
    .ok_or(FactoryBuildBridgeError::InvalidSnapshot)
}

fn build_input_digest(request: &CreateFactoryBuild) -> Result<FactoryDigest, FactoryBuildBridgeError> {
  let stage = request
    .causality
    .stage()
    .ok_or(FactoryBuildBridgeError::InvalidSnapshot)?;
  let parent = match stage.parent {
    Some(FactoryBuildParent::ChangeSet(id)) => id.as_uuid(),
    Some(FactoryBuildParent::Decision(id)) => id.as_uuid(),
    None => uuid::Uuid::nil(),
  };
  let candidate_revision = stage
    .candidate
    .as_ref()
    .map_or(&[][..], |candidate| candidate.candidate_revision.as_str().as_bytes());
  let bundle_digest = stage
    .candidate
    .as_ref()
    .map(|candidate| candidate.bundle.digest.as_bytes().to_vec())
    .unwrap_or_default();
  let manifest_digest = stage
    .candidate
    .as_ref()
    .map(|candidate| candidate.manifest.digest.as_bytes().to_vec())
    .unwrap_or_default();
  Ok(FactoryDigest::sha256(
    "octacity.factory.build-input.v1",
    &[
      &request.operation_id.as_bytes(),
      request.project_id.as_uuid().as_bytes(),
      request.repository_id.as_uuid().as_bytes(),
      request.build_configuration.id().as_uuid().as_bytes(),
      &request.build_configuration.version().get().to_be_bytes(),
      request.immutable_revision.as_str().as_bytes(),
      &request.effective_permissions.digest().as_bytes(),
      stage.run_id.as_uuid().as_bytes(),
      stage.stage_attempt_id.as_uuid().as_bytes(),
      stage.target.canonical_key().as_bytes(),
      parent.as_bytes(),
      candidate_revision,
      &bundle_digest,
      &manifest_digest,
      &stage.task_envelope_digest.as_bytes(),
    ],
  ))
}

async fn commit_link<S: FactoryRunStore>(
  store: &S,
  snapshot: &FactoryRunSnapshot,
  claimed: &ClaimedFactoryOutbox,
  context: DispatchContext<'_>,
  link: FactoryBuildLink,
  committed_at: Timestamp,
) -> Result<(), FactoryBuildBridgeError> {
  let next_progress = advance_progress(
    &context.checkpoint.progress,
    &context.target,
    BuildState::Running,
    false,
  )?;
  let mut append = FactoryRunHistoryAppend::default();
  append.linked_builds.push(link);
  commit_checkpoint(
    store,
    CommitCheckpointInput {
      snapshot,
      claim: snapshot
        .current_claim
        .as_ref()
        .ok_or(FactoryBuildBridgeError::InvalidSnapshot)?,
      progress: next_progress,
      append,
      operation: key("factory.build.linked")?,
      request_identity: claimed.record.operation_id,
      committed_at,
    },
  )
  .await
}

async fn commit_terminal_observation<S: FactoryRunStore>(
  store: &S,
  snapshot: &FactoryRunSnapshot,
  claimed: &ClaimedFactoryRun,
  link: &FactoryBuildLink,
  observation: &FactoryBuildObservationRecord,
  committed_at: Timestamp,
) -> Result<(), FactoryBuildBridgeError> {
  let checkpoint = current_checkpoint(snapshot)?;
  let target = link.target.clone();
  let next_progress = advance_progress(
    &checkpoint.progress,
    &target,
    observation.state,
    observation.infrastructure_retry_eligible,
  )?;
  let mut append = FactoryRunHistoryAppend::default();
  append.build_observations.push(observation.clone());
  commit_checkpoint(
    store,
    CommitCheckpointInput {
      snapshot,
      claim: &claimed.record,
      progress: next_progress,
      append,
      operation: key("factory.build.observed")?,
      request_identity: observation.id,
      committed_at,
    },
  )
  .await
}

struct CommitCheckpointInput<'a> {
  snapshot: &'a FactoryRunSnapshot,
  claim: &'a octacity_server_store::FactoryRunClaimRecord,
  progress: FactoryLifecycleProgress,
  append: FactoryRunHistoryAppend,
  operation: FactoryKey,
  request_identity: FactoryDigest,
  committed_at: Timestamp,
}

async fn commit_checkpoint<S: FactoryRunStore>(
  store: &S,
  input: CommitCheckpointInput<'_>,
) -> Result<(), FactoryBuildBridgeError> {
  let CommitCheckpointInput {
    snapshot,
    claim,
    progress,
    append,
    operation,
    request_identity,
    committed_at,
  } = input;
  let current_checkpoint = current_checkpoint(snapshot)?;
  let current_budget = snapshot
    .budgets
    .iter()
    .find(|budget| budget.id == snapshot.current.budget_id)
    .ok_or(FactoryBuildBridgeError::InvalidSnapshot)?;
  let next_version = FactoryRunVersion::new(
    snapshot
      .run
      .version()
      .get()
      .checked_add(1)
      .ok_or(FactoryBuildBridgeError::InvalidSnapshot)?,
  )
  .map_err(|_| FactoryBuildBridgeError::InvalidSnapshot)?;
  let next_run = FactoryRun::restore(
    snapshot.run.id(),
    snapshot.run.configuration().clone(),
    &snapshot.work,
    snapshot.run.subject().clone(),
    snapshot.run.state(),
    next_version,
  )
  .map_err(|_| FactoryBuildBridgeError::InvalidSnapshot)?;
  let budget = FactoryBudgetRecord::new(snapshot.run.id(), next_version, current_budget.usage, committed_at);
  let checkpoint = FactoryLifecycleCheckpoint::new(
    snapshot.run.id(),
    next_version,
    progress,
    current_checkpoint.signal,
    current_checkpoint.cancellation_requested,
    committed_at,
  );
  let mut current = snapshot.current.clone();
  current.budget_id = budget.id;
  current.lifecycle_checkpoint_id = checkpoint.id;
  if let Some(link) = append.linked_builds.first() {
    current.build_id = Some(link.build_id);
  }
  store
    .commit_factory_run_transition(CommitFactoryRunTransition {
      run_id: snapshot.run.id(),
      expected_version: snapshot.run.version(),
      claim_id: claim.id,
      owner: claim.owner.clone(),
      fence: claim.claim.fence(),
      committed_at,
      next_run,
      budget,
      lifecycle_checkpoint: checkpoint,
      append,
      current,
      audit: FactoryAuditFact::new(
        snapshot.run.id(),
        AuditActorKind::Worker,
        Some(FactoryDigest::sha256(
          "octacity.factory.build-worker.v1",
          &[claim.owner.as_str().as_bytes()],
        )),
        operation,
        request_identity,
        key("accepted")?,
        committed_at,
      ),
      outbox: Vec::new(),
    })
    .await?;
  Ok(())
}

fn advance_progress(
  progress: &FactoryLifecycleProgress,
  target: &FactoryStageTarget,
  state: BuildState,
  infrastructure_retry_eligible: bool,
) -> Result<FactoryLifecycleProgress, FactoryBuildBridgeError> {
  let observed = observed_build_state(state, infrastructure_retry_eligible);
  match progress {
    FactoryLifecycleProgress::Stage {
      target: current,
      progress,
    } if current == target
      && matches!(
        progress,
        FactoryStageProgress::AttemptCreated | FactoryStageProgress::BuildActive
      ) =>
    {
      Ok(FactoryLifecycleProgress::Stage {
        target: current.clone(),
        progress: observed.stage,
      })
    }
    FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(branches)) => {
      let FactoryStageTarget::Evaluation(key) = target else {
        return Err(FactoryBuildBridgeError::InvalidSnapshot);
      };
      let branch = branches
        .branches()
        .iter()
        .find(|branch| branch.key() == key)
        .ok_or(FactoryBuildBridgeError::InvalidSnapshot)?;
      let next = if observed.branch == EvaluationBranchState::Completed && branch.is_plan_bound() {
        EvaluationBranchState::ResultPending
      } else {
        observed.branch
      };
      Ok(FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(
        branches
          .advance_branch(key, next)
          .map_err(|_| FactoryBuildBridgeError::InvalidSnapshot)?,
      )))
    }
    _ => Err(FactoryBuildBridgeError::InvalidSnapshot),
  }
}

#[derive(Clone, Copy)]
struct ObservedBuildState {
  stage: FactoryStageProgress,
  branch: EvaluationBranchState,
}

const fn observed_build_state(state: BuildState, infrastructure_retry_eligible: bool) -> ObservedBuildState {
  match (state, infrastructure_retry_eligible) {
    (BuildState::Queued, _) => ObservedBuildState {
      stage: FactoryStageProgress::BuildActive,
      branch: EvaluationBranchState::BuildActive,
    },
    (BuildState::Running, _) => ObservedBuildState {
      stage: FactoryStageProgress::BuildActive,
      branch: EvaluationBranchState::BuildActive,
    },
    (BuildState::Succeeded, _) => ObservedBuildState {
      stage: FactoryStageProgress::BuildSucceeded,
      branch: EvaluationBranchState::Completed,
    },
    (BuildState::Failed, true) => ObservedBuildState {
      stage: FactoryStageProgress::RetryableFailure,
      branch: EvaluationBranchState::RetryableFailure,
    },
    (BuildState::Failed, false) => ObservedBuildState {
      stage: FactoryStageProgress::Failed,
      branch: EvaluationBranchState::Failed,
    },
    (BuildState::Cancelled, _) => ObservedBuildState {
      stage: FactoryStageProgress::Cancelled,
      branch: EvaluationBranchState::Cancelled,
    },
  }
}

fn progress_reflects_state(
  progress: &FactoryLifecycleProgress,
  target: &FactoryStageTarget,
  state: BuildState,
  infrastructure_retry_eligible: bool,
) -> bool {
  let observed = observed_build_state(state, infrastructure_retry_eligible);
  match progress {
    FactoryLifecycleProgress::Stage {
      target: current,
      progress,
    } => current == target && *progress == observed.stage,
    FactoryLifecycleProgress::Evaluating(EvaluationState::Branches(branches)) => {
      let FactoryStageTarget::Evaluation(key) = target else {
        return false;
      };
      branches.branches().iter().any(|branch| {
        branch.key() == key
          && (branch.state() == observed.branch
            || (state == BuildState::Succeeded
              && matches!(
                branch.state(),
                EvaluationBranchState::ResultPending
                  | EvaluationBranchState::Completed
                  | EvaluationBranchState::Substituted
              )))
      })
    }
    _ => false,
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

fn current_checkpoint(snapshot: &FactoryRunSnapshot) -> Result<&FactoryLifecycleCheckpoint, FactoryBuildBridgeError> {
  snapshot
    .lifecycle_checkpoints
    .iter()
    .find(|checkpoint| checkpoint.id == snapshot.current.lifecycle_checkpoint_id)
    .ok_or(FactoryBuildBridgeError::InvalidSnapshot)
}

fn terminal_observation<'a>(
  snapshot: &'a FactoryRunSnapshot,
  link: &FactoryBuildLink,
  build: &BuildRecord,
  attempt: &AttemptRecord,
  infrastructure_retry_eligible: bool,
) -> Result<&'a FactoryBuildObservationRecord, FactoryBuildBridgeError> {
  let mut matches = snapshot
    .build_observations
    .iter()
    .filter(|observation| observation.build_id == link.build_id);
  let observation = matches.next().ok_or(FactoryBuildBridgeError::InvalidSnapshot)?;
  if matches.next().is_some()
    || observation.stage_attempt_id != link.stage_attempt_id
    || observation.target != link.target
    || observation.build_version != build.version
    || observation.attempt_id != attempt.id
    || observation.attempt_version != attempt.version
    || observation.state != build.state
    || observation.infrastructure_retry_eligible != infrastructure_retry_eligible
  {
    return Err(FactoryBuildBridgeError::InvalidSnapshot);
  }
  Ok(observation)
}

fn validate_run_claim(
  snapshot: &FactoryRunSnapshot,
  claimed: &ClaimedFactoryRun,
  observed_at: Timestamp,
) -> Result<(), FactoryBuildBridgeError> {
  if snapshot.run.version() != claimed.expected_version
    || snapshot.current_claim.as_ref() != Some(&claimed.record)
    || snapshot.run.id() != claimed.run_id
  {
    return Err(FactoryBuildBridgeError::InvalidSnapshot);
  }
  claimed
    .record
    .claim
    .verify_fence(claimed.record.claim.fence(), observed_at)
    .map_err(|_| FactoryBuildBridgeError::InvalidSnapshot)
}

async fn settle_delivered<S: FactoryRunStore>(
  store: &S,
  claimed: &ClaimedFactoryOutbox,
  observed_at: Timestamp,
) -> Result<(), FactoryBuildBridgeError> {
  let owner = claimed
    .record
    .owner
    .clone()
    .ok_or(FactoryBuildBridgeError::InvalidSnapshot)?;
  let claim = claimed.record.claim.ok_or(FactoryBuildBridgeError::InvalidSnapshot)?;
  store
    .settle_factory_outbox(SettleFactoryOutbox {
      operation_id: claimed.record.operation_id,
      owner,
      fence: claim.fence(),
      observed_at,
      settlement: FactoryOutboxSettlement::Delivered,
    })
    .await?;
  Ok(())
}

fn key(value: &str) -> Result<FactoryKey, FactoryBuildBridgeError> {
  FactoryKey::new(value).map_err(|_| FactoryBuildBridgeError::InvalidSnapshot)
}
