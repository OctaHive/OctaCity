use std::{collections::BTreeSet, fmt, num::NonZeroU64};

use octacity_server_domain::{
  AttemptId, AttemptNumber, BuildConfigurationId, BuildConfigurationVersion, BuildId, JobId, PipelineId,
  PipelineNodeId, PipelineVersion, PoolId, ProjectId, RepositoryId, RepositoryVersion, Timestamp,
};
use octacity_server_pipeline::DependencyPolicy;
use octacity_server_trigger::NormalizedTriggerOccurrence;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{StoreError, StoreInputError, StoreOperation};

/// Maximum UTF-8 bytes in one persisted Job-event classification.
pub const MAX_JOB_EVENT_KIND_BYTES: usize = 64;
/// Maximum UTF-8 bytes in one resolved immutable source revision.
pub const MAX_IMMUTABLE_REVISION_BYTES: usize = 512;
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
  pub immutable_revision: String,
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
    if self.immutable_revision.is_empty()
      || self.immutable_revision.len() > MAX_IMMUTABLE_REVISION_BYTES
      || self.immutable_revision.trim() != self.immutable_revision
      || self.immutable_revision.chars().any(char::is_control)
    {
      return Err(StoreInputError::InvalidImmutableRevision);
    }
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
  /// Immutable validated Job template and execution inputs.
  pub snapshot: Value,
  /// Backend-neutral placement requirements.
  pub requirements: Value,
}

impl MaterializedJob {
  /// Validates and canonicalizes one materialized Job.
  pub fn new(
    id: JobId,
    pipeline_node_id: PipelineNodeId,
    mut dependencies: Vec<JobId>,
    dependency_policy: DependencyPolicy,
    mut allowed_pools: Vec<PoolId>,
    snapshot: Value,
    requirements: Value,
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
    require_bounded_json_object(&snapshot)?;
    require_bounded_json_object(&requirements)?;
    Ok(Self {
      id,
      pipeline_node_id,
      dependencies,
      dependency_policy,
      allowed_pools,
      snapshot,
      requirements,
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
    require_bounded_json_object(&self.snapshot)?;
    require_bounded_json_object(&self.requirements)
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
    accepted_at: Timestamp,
  ) -> Result<Self, StoreError> {
    jobs.sort_unstable_by_key(|job| job.id);
    let request = Self {
      trigger,
      build,
      attempt_id,
      attempt_number,
      jobs,
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
    if self.jobs.is_empty() {
      return Err(StoreError::invalid(
        StoreOperation::AcceptTrigger,
        StoreInputError::EmptyJobGraph,
      ));
    }
    if self.jobs.len() > MAX_MATERIALIZED_JOBS {
      return Err(StoreError::invalid(
        StoreOperation::AcceptTrigger,
        StoreInputError::TooManyJobs,
      ));
    }
    if self
      .jobs
      .iter()
      .try_fold(0_usize, |total, job| total.checked_add(job.dependencies.len()))
      .is_none_or(|total| total > MAX_MATERIALIZED_DEPENDENCY_EDGES)
    {
      return Err(StoreError::invalid(
        StoreOperation::AcceptTrigger,
        StoreInputError::TooManyDependencyEdges,
      ));
    }
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
    for job in &self.jobs {
      job
        .validate()
        .map_err(|source| StoreError::invalid(StoreOperation::AcceptTrigger, source))?;
    }
    if self.jobs.windows(2).any(|pair| pair[0].id >= pair[1].id) {
      return Err(StoreError::invalid(
        StoreOperation::AcceptTrigger,
        StoreInputError::DuplicateJob,
      ));
    }
    let node_ids: BTreeSet<_> = self.jobs.iter().map(|job| &job.pipeline_node_id).collect();
    if node_ids.len() != self.jobs.len() {
      return Err(StoreError::invalid(
        StoreOperation::AcceptTrigger,
        StoreInputError::DuplicatePipelineNode,
      ));
    }
    let identities: BTreeSet<_> = self.jobs.iter().map(|job| job.id).collect();
    if self
      .jobs
      .iter()
      .flat_map(|job| job.dependencies.iter())
      .any(|dependency| !identities.contains(dependency))
    {
      return Err(StoreError::invalid(
        StoreOperation::AcceptTrigger,
        StoreInputError::UnknownDependency,
      ));
    }
    validate_acyclic(&self.jobs)
  }
}

fn validate_acyclic(jobs: &[MaterializedJob]) -> Result<(), StoreError> {
  let mut completed = BTreeSet::new();
  loop {
    let before = completed.len();
    for job in jobs {
      if !completed.contains(&job.id) && job.dependencies.iter().all(|dependency| completed.contains(dependency)) {
        completed.insert(job.id);
      }
    }
    if completed.len() == jobs.len() {
      return Ok(());
    }
    if completed.len() == before {
      return Err(StoreError::invalid(
        StoreOperation::AcceptTrigger,
        StoreInputError::CyclicJobGraph,
      ));
    }
  }
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
  /// Build durably associated with the occurrence.
  pub build_id: BuildId,
  /// Attempt durably associated with the occurrence.
  pub attempt_id: AttemptId,
  /// Root Jobs inserted into the global ready queue.
  pub ready_jobs: Vec<JobId>,
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
