use std::{collections::BTreeSet, fmt, num::NonZeroU64};

use octacity_server_domain::{
  AttemptId, AttemptNumber, BuildConfigurationId, BuildConfigurationVersion, BuildId, ImmutableRevision, JobId,
  PipelineId, PipelineNodeId, PipelineVersion, PoolId, ProjectId, RepositoryId, RepositoryVersion, Timestamp,
};
use octacity_server_job::{JobRequirements, JobSpecTemplate, JobState};
use octacity_server_orchestrator::{JobGraphNode, validate_job_graph};
use octacity_server_pipeline::DependencyPolicy;
use octacity_server_trigger::NormalizedTriggerOccurrence;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use thiserror::Error;

use crate::{StoreError, StoreInputError, StoreOperation};

/// Maximum UTF-8 bytes in one persisted Job-event classification.
pub const MAX_JOB_EVENT_KIND_BYTES: usize = 64;
/// Maximum encoded bytes in one structured JSON document stored atomically.
pub const MAX_STRUCTURED_DOCUMENT_BYTES: usize = 256 * 1_024;
/// Maximum encoded bytes accepted by one Trigger transaction.
pub const MAX_ACCEPT_TRIGGER_BYTES: usize = 8 * 1_024 * 1_024;
/// Maximum Jobs materialized by one atomic Trigger acceptance.
pub const MAX_MATERIALIZED_JOBS: usize = 1_024;
/// Maximum direct dependencies of one materialized Job.
pub const MAX_JOB_DEPENDENCIES: usize = 256;
/// Maximum dependency edges materialized by one atomic Trigger acceptance.
pub const MAX_MATERIALIZED_DEPENDENCY_EDGES: usize = 8_192;
/// Maximum allowed Pools captured for one materialized Job.
pub const MAX_ALLOWED_POOLS_PER_JOB: usize = 128;
/// Maximum events accepted by one append transaction.
pub const MAX_JOB_EVENT_BATCH_SIZE: usize = 256;
/// Maximum events returned by one durable Job-event read.
pub const MAX_JOB_EVENT_READ_PAGE_SIZE: usize = 256;
/// Maximum encoded bytes in one persisted Job-event payload.
pub const MAX_JOB_EVENT_PAYLOAD_BYTES: usize = 256 * 1_024;
/// Maximum aggregate payload bytes accepted by one event transaction.
pub const MAX_JOB_EVENT_BATCH_BYTES: usize = 8 * 1_024 * 1_024;

const ACCEPT_TRIGGER_ENVELOPE_BYTES: usize = 1_024;

/// Positive epoch identifying one current Agent process registration.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RegistrationEpoch(NonZeroU64);

impl RegistrationEpoch {
  /// Constructs a positive registration epoch.
  pub fn new(value: u64) -> Result<Self, StoreInputError> {
    NonZeroU64::new(value)
      .map(Self)
      .ok_or(StoreInputError::ZeroRegistrationEpoch)
  }

  /// Returns the positive numeric epoch.
  #[must_use]
  pub const fn get(self) -> u64 {
    self.0.get()
  }
}

/// Positive contiguous sequence in one Job event stream.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EventSequence(NonZeroU64);

impl EventSequence {
  /// Constructs a positive sequence number.
  pub fn new(value: u64) -> Result<Self, StoreInputError> {
    NonZeroU64::new(value)
      .map(Self)
      .ok_or(StoreInputError::NonContiguousEventBatch)
  }

  /// Returns the positive numeric sequence.
  #[must_use]
  pub const fn get(self) -> u64 {
    self.0.get()
  }
}

/// Immutable digest identifying the exact payload of one durable Job event.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EventDigest([u8; 32]);

impl EventDigest {
  pub(crate) const fn from_bytes(bytes: [u8; 32]) -> Self {
    Self(bytes)
  }

  /// Returns the digest bytes.
  #[must_use]
  pub const fn as_bytes(self) -> [u8; 32] {
    self.0
  }
}

/// Stable digest of caller intent used to recognize a Trigger replay before
/// consulting mutable external source state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
pub struct TriggerIntentDigest([u8; 32]);

impl TriggerIntentDigest {
  /// Constructs a digest from a canonical SHA-256 result.
  #[must_use]
  pub const fn from_bytes(bytes: [u8; 32]) -> Self {
    Self(bytes)
  }

  /// Returns the digest bytes for a persistence adapter.
  #[must_use]
  pub const fn as_bytes(self) -> [u8; 32] {
    self.0
  }

  /// Hashes one canonical serializable intent with a versioned domain prefix.
  pub fn derive<T: Serialize + ?Sized>(intent: &T) -> Result<Self, TriggerIntentDigestError> {
    let encoded = serde_json::to_vec(intent).map_err(|_| TriggerIntentDigestError)?;
    let mut digest = Sha256::new();
    digest.update(b"octacity.trigger-intent.v1\0");
    digest.update(encoded);
    Ok(Self(digest.finalize().into()))
  }
}

/// Stable Trigger intent could not be encoded for digest derivation.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("trigger intent could not be encoded")]
pub struct TriggerIntentDigestError;

/// Identity required to look up a terminal Trigger evaluation without
/// resolving mutable VCS selection again.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TriggerAcceptanceProbe {
  /// Normalized occurrence whose identity and deduplication key are queried.
  pub trigger: NormalizedTriggerOccurrence,
  /// Digest of the transport-independent caller intent.
  pub intent_digest: TriggerIntentDigest,
}

/// Complete atomic input for recording policy-suppressed Trigger evaluation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SuppressTrigger {
  /// Normalized occurrence and exact deduplication identity.
  pub trigger: NormalizedTriggerOccurrence,
  /// Digest of stable caller intent shared with an accepted evaluation.
  pub intent_digest: TriggerIntentDigest,
  /// Authoritative time at which policy selected suppression.
  pub suppressed_at: Timestamp,
}

impl SuppressTrigger {
  /// Creates and validates one durable suppression request.
  pub fn new(
    trigger: NormalizedTriggerOccurrence,
    intent_digest: TriggerIntentDigest,
    suppressed_at: Timestamp,
  ) -> Result<Self, StoreError> {
    let request = Self {
      trigger,
      intent_digest,
      suppressed_at,
    };
    request.validate()?;
    Ok(request)
  }

  /// Revalidates the normalized occurrence at an adapter seam.
  pub fn validate(&self) -> Result<(), StoreError> {
    self
      .trigger
      .validate()
      .map_err(|_| StoreError::invalid(StoreOperation::AcceptTrigger, StoreInputError::InvalidNormalizedTrigger))
  }
}

/// Secret opaque token proving current ownership of one Lease.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct LeaseFence([u8; 32]);

impl LeaseFence {
  /// Constructs a fence from cryptographically random bytes.
  #[must_use]
  pub const fn from_bytes(bytes: [u8; 32]) -> Self {
    Self(bytes)
  }

  /// Returns the bytes for hashing or protocol encoding at a trusted seam.
  #[must_use]
  pub const fn expose(self) -> [u8; 32] {
    self.0
  }
}

impl fmt::Debug for LeaseFence {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("LeaseFence([REDACTED])")
  }
}

/// Bounded stable classification of one persisted Job event.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct JobEventKind(String);

impl JobEventKind {
  /// Constructs a non-empty classification without control characters.
  pub fn new(value: impl Into<String>) -> Result<Self, StoreInputError> {
    let value = value.into();
    if value.is_empty()
      || value.len() > MAX_JOB_EVENT_KIND_BYTES
      || value.trim() != value
      || value.chars().any(char::is_control)
    {
      return Err(StoreInputError::InvalidEventKind);
    }
    Ok(Self(value))
  }

  /// Borrows the stable classification.
  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

/// Immutable Build identity, version references, and execution snapshots.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ImmutableBuildInput {
  /// Stable Build identity.
  pub id: BuildId,
  /// Owning Project.
  pub project_id: ProjectId,
  /// Build Configuration referenced by the Build.
  pub configuration_id: BuildConfigurationId,
  /// Immutable Build Configuration version.
  pub configuration_version: BuildConfigurationVersion,
  /// Pipeline referenced by the Build.
  pub pipeline_id: PipelineId,
  /// Immutable Pipeline version.
  pub pipeline_version: PipelineVersion,
  /// Repository referenced by the Build.
  pub repository_id: RepositoryId,
  /// Immutable Repository version.
  pub repository_version: RepositoryVersion,
  /// Exact immutable revision selected before acceptance.
  pub immutable_revision: ImmutableRevision,
  /// Validated parameters and initiating input.
  pub input_snapshot: Value,
  /// Effective inherited policy frozen for this Build.
  pub effective_policy_snapshot: Value,
  /// Durable ready-queue priority copied to materialized root Jobs.
  pub priority: i64,
}

impl ImmutableBuildInput {
  /// Validates immutable source and JSON snapshot shape.
  pub fn validate(&self) -> Result<(), StoreInputError> {
    require_bounded_json_object(&self.input_snapshot)?;
    require_bounded_json_object(&self.effective_policy_snapshot)
  }
}

/// One immutable Job produced while materializing a validated Pipeline DAG.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MaterializedJob {
  /// Stable Job identity.
  pub id: JobId,
  /// Stable node identity from the immutable Pipeline version.
  pub pipeline_node_id: PipelineNodeId,
  /// Jobs that must reach their recorded success policy first.
  pub dependencies: Vec<JobId>,
  /// Immutable fan-in policy selected by the Pipeline domain.
  pub dependency_policy: DependencyPolicy,
  /// Effective Pool allowlist captured with the immutable Build input.
  pub allowed_pools: Vec<PoolId>,
  /// Backend-neutral placement requirements.
  pub requirements: JobRequirements,
  /// Stable server-derived intent signed only when this Job becomes ready.
  pub job_spec_template: JobSpecTemplate,
}

/// Validated snapshots and stable execution intent stored with one Job.
pub struct MaterializedJobPayload {
  requirements: JobRequirements,
  job_spec_template: JobSpecTemplate,
}

impl MaterializedJobPayload {
  /// Groups the immutable node, placement, and execution-intent documents.
  pub fn new(requirements: JobRequirements, job_spec_template: JobSpecTemplate) -> Result<Self, StoreInputError> {
    require_bounded_serializable_object(&requirements)?;
    Ok(Self {
      requirements,
      job_spec_template,
    })
  }
}

impl MaterializedJob {
  /// Validates and canonicalizes one materialized Job.
  pub fn new(
    id: JobId,
    pipeline_node_id: PipelineNodeId,
    mut dependencies: Vec<JobId>,
    dependency_policy: DependencyPolicy,
    mut allowed_pools: Vec<PoolId>,
    payload: MaterializedJobPayload,
  ) -> Result<Self, StoreInputError> {
    if dependencies.len() > MAX_JOB_DEPENDENCIES {
      return Err(StoreInputError::TooManyDependencies);
    }
    dependencies.sort_unstable();
    if dependencies.windows(2).any(|pair| pair[0] == pair[1]) {
      return Err(StoreInputError::DuplicateDependency);
    }
    if dependencies.binary_search(&id).is_ok() {
      return Err(StoreInputError::SelfDependency);
    }
    if allowed_pools.is_empty() {
      return Err(StoreInputError::EmptyAllowedPools);
    }
    if allowed_pools.len() > MAX_ALLOWED_POOLS_PER_JOB {
      return Err(StoreInputError::TooManyAllowedPools);
    }
    allowed_pools.sort_unstable();
    if allowed_pools.windows(2).any(|pair| pair[0] == pair[1]) {
      return Err(StoreInputError::DuplicateAllowedPool);
    }
    let MaterializedJobPayload {
      requirements,
      job_spec_template,
    } = payload;
    Ok(Self {
      id,
      pipeline_node_id,
      dependencies,
      dependency_policy,
      allowed_pools,
      requirements,
      job_spec_template,
    })
  }

  fn validate(&self) -> Result<(), StoreInputError> {
    if self.dependencies.len() > MAX_JOB_DEPENDENCIES {
      return Err(StoreInputError::TooManyDependencies);
    }
    if self.allowed_pools.is_empty() {
      return Err(StoreInputError::EmptyAllowedPools);
    }
    if self.allowed_pools.len() > MAX_ALLOWED_POOLS_PER_JOB {
      return Err(StoreInputError::TooManyAllowedPools);
    }
    if self.dependencies.windows(2).any(|pair| pair[0] >= pair[1]) {
      return Err(StoreInputError::DuplicateDependency);
    }
    if self.dependencies.binary_search(&self.id).is_ok() {
      return Err(StoreInputError::SelfDependency);
    }
    if self.allowed_pools.windows(2).any(|pair| pair[0] >= pair[1]) {
      return Err(StoreInputError::DuplicateAllowedPool);
    }
    require_bounded_serializable_object(&self.requirements)
  }
}

/// Complete atomic input for accepting one deduplicated Trigger occurrence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AcceptTrigger {
  /// Normalized occurrence and exact deduplication identity.
  pub trigger: NormalizedTriggerOccurrence,
  /// Immutable Build identity, references, and snapshots.
  pub build: ImmutableBuildInput,
  /// First materialized Attempt identity.
  pub attempt_id: AttemptId,
  /// Positive Attempt number within the Build.
  pub attempt_number: AttemptNumber,
  /// Validated and canonical materialized Job graph.
  pub jobs: Vec<MaterializedJob>,
  /// Digest of stable caller intent, independent of resolved VCS state.
  pub intent_digest: TriggerIntentDigest,
  /// Authoritative acceptance time supplied by the application clock.
  pub accepted_at: Timestamp,
}

impl AcceptTrigger {
  /// Validates cross-Job references and canonicalizes a materialized DAG.
  pub fn new(
    trigger: NormalizedTriggerOccurrence,
    build: ImmutableBuildInput,
    attempt_id: AttemptId,
    attempt_number: AttemptNumber,
    mut jobs: Vec<MaterializedJob>,
    intent_digest: TriggerIntentDigest,
    accepted_at: Timestamp,
  ) -> Result<Self, StoreError> {
    jobs.sort_unstable_by_key(|job| job.id);
    let request = Self {
      trigger,
      build,
      attempt_id,
      attempt_number,
      jobs,
      intent_digest,
      accepted_at,
    };
    request.validate()?;
    Ok(request)
  }

  /// Revalidates every bound and graph invariant at an adapter seam.
  pub fn validate(&self) -> Result<(), StoreError> {
    self
      .trigger
      .validate()
      .map_err(|_| StoreInputError::InvalidNormalizedTrigger)
      .and_then(|()| self.build.validate())
      .map_err(|source| StoreError::invalid(StoreOperation::AcceptTrigger, source))?;
    if self.trigger.target.configuration_id != self.build.configuration_id
      || self.trigger.target.configuration_version != self.build.configuration_version
    {
      return Err(StoreError::invalid(
        StoreOperation::AcceptTrigger,
        StoreInputError::TriggerTargetMismatch,
      ));
    }
    validate_materialized_jobs(StoreOperation::AcceptTrigger, self.build.id, &self.jobs)?;
    let encoded_bytes = serde_json::to_vec(&self.trigger)
      .ok()
      .zip(serde_json::to_vec(&self.build).ok())
      .and_then(|(trigger, build)| {
        ACCEPT_TRIGGER_ENVELOPE_BYTES
          .checked_add(trigger.len())?
          .checked_add(build.len())
      })
      .and_then(|total| {
        self.jobs.iter().try_fold(total, |total, job| {
          let encoded = serde_json::to_vec(job).ok()?;
          total.checked_add(encoded.len())
        })
      });
    if encoded_bytes.is_none_or(|bytes| bytes > MAX_ACCEPT_TRIGGER_BYTES) {
      return Err(StoreError::invalid(
        StoreOperation::AcceptTrigger,
        StoreInputError::RequestTooLarge,
      ));
    }
    Ok(())
  }
}

pub(crate) fn validate_materialized_jobs(
  operation: StoreOperation,
  build_id: BuildId,
  jobs: &[MaterializedJob],
) -> Result<(), StoreError> {
  if jobs.is_empty() {
    return Err(StoreError::invalid(operation, StoreInputError::EmptyJobGraph));
  }
  if jobs.len() > MAX_MATERIALIZED_JOBS {
    return Err(StoreError::invalid(operation, StoreInputError::TooManyJobs));
  }
  if jobs
    .iter()
    .try_fold(0_usize, |total, job| total.checked_add(job.dependencies.len()))
    .is_none_or(|total| total > MAX_MATERIALIZED_DEPENDENCY_EDGES)
  {
    return Err(StoreError::invalid(operation, StoreInputError::TooManyDependencyEdges));
  }
  if jobs.iter().any(|job| {
    job.job_spec_template.build_id() != build_id
      || job.job_spec_template.pipeline_node_id() != &job.pipeline_node_id
      || job.job_spec_template.validate().is_err()
  }) {
    return Err(StoreError::invalid(
      operation,
      StoreInputError::JobSpecTemplateBindingMismatch,
    ));
  }
  for job in jobs {
    job
      .validate()
      .map_err(|source| StoreError::invalid(operation, source))?;
  }
  if jobs.windows(2).any(|pair| pair[0].id >= pair[1].id) {
    return Err(StoreError::invalid(operation, StoreInputError::DuplicateJob));
  }
  let node_ids: BTreeSet<_> = jobs.iter().map(|job| &job.pipeline_node_id).collect();
  if node_ids.len() != jobs.len() {
    return Err(StoreError::invalid(operation, StoreInputError::DuplicatePipelineNode));
  }
  let graph = jobs
    .iter()
    .map(|job| {
      JobGraphNode::new(
        job.id,
        if job.dependencies.is_empty() {
          JobState::Ready
        } else {
          JobState::Blocked
        },
        job.dependencies.clone(),
        job.dependency_policy,
      )
    })
    .collect::<Result<Vec<_>, _>>()
    .map_err(|error| graph_error(operation, error))?;
  validate_job_graph(graph).map_err(|error| graph_error(operation, error))
}

fn graph_error(operation: StoreOperation, error: octacity_server_orchestrator::OrchestrationError) -> StoreError {
  use octacity_server_orchestrator::OrchestrationError;

  let source = match error {
    OrchestrationError::DuplicateJob { .. } => StoreInputError::DuplicateJob,
    OrchestrationError::SelfDependency { .. } => StoreInputError::SelfDependency,
    OrchestrationError::DuplicateDependency { .. } => StoreInputError::DuplicateDependency,
    OrchestrationError::UnknownDependency { .. } => StoreInputError::UnknownDependency,
    OrchestrationError::CyclicGraph => StoreInputError::CyclicJobGraph,
    OrchestrationError::EmptyGraph => StoreInputError::EmptyJobGraph,
    OrchestrationError::BlockedRoot { .. }
    | OrchestrationError::InvalidJobTransition { .. }
    | OrchestrationError::CancellationNotPropagated { .. } => StoreInputError::CyclicJobGraph,
  };
  StoreError::invalid(operation, source)
}

/// Whether an idempotent mutation was newly applied or exactly replayed.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationDisposition {
  /// The authoritative state changed during this call.
  Applied,
  /// An identical prior mutation already produced the returned state.
  Replayed,
}

/// Result of atomically accepting a Trigger and materializing its first Attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptTriggerOutcome {
  /// Idempotency disposition.
  pub disposition: MutationDisposition,
  /// Original occurrence durably associated with the Build.
  pub trigger_occurrence_id: octacity_server_domain::TriggerOccurrenceId,
  /// Build durably associated with the occurrence.
  pub build_id: BuildId,
  /// Attempt durably associated with the occurrence.
  pub attempt_id: AttemptId,
  /// Root Jobs inserted into the global ready queue.
  pub ready_jobs: Vec<JobId>,
}

/// Result of durably suppressing a Trigger without creating queued work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SuppressTriggerOutcome {
  /// Idempotency disposition.
  pub disposition: MutationDisposition,
  /// Original occurrence that received the terminal suppressed outcome.
  pub trigger_occurrence_id: octacity_server_domain::TriggerOccurrenceId,
}

/// Durable terminal result returned by Trigger evaluation and replay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TriggerEvaluationOutcome {
  /// Evaluation created and materialized one Build.
  Accepted(AcceptTriggerOutcome),
  /// Policy intentionally created no Build or queued work.
  Suppressed(SuppressTriggerOutcome),
}

impl TriggerEvaluationOutcome {
  /// Returns whether this call applied new state or replayed prior state.
  #[must_use]
  pub const fn disposition(&self) -> MutationDisposition {
    match self {
      Self::Accepted(outcome) => outcome.disposition,
      Self::Suppressed(outcome) => outcome.disposition,
    }
  }
}

pub(crate) fn require_bounded_json_object(value: &Value) -> Result<(), StoreInputError> {
  if !value.is_object() {
    return Err(StoreInputError::JsonDocumentMustBeObject);
  }
  require_bounded_json(
    value,
    MAX_STRUCTURED_DOCUMENT_BYTES,
    StoreInputError::JsonDocumentTooLarge,
  )
}

fn require_bounded_serializable_object(value: &impl Serialize) -> Result<(), StoreInputError> {
  let value = serde_json::to_value(value).map_err(|_| StoreInputError::JsonDocumentTooLarge)?;
  require_bounded_json_object(&value)
}

pub(crate) fn require_bounded_json(
  value: &Value,
  max_bytes: usize,
  too_large: StoreInputError,
) -> Result<(), StoreInputError> {
  let encoded = serde_json::to_vec(value).map_err(|_| too_large)?;
  if encoded.len() > max_bytes {
    return Err(too_large);
  }
  Ok(())
}
