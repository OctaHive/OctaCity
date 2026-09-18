use octacity_server_domain::{EntityKind, JobId, LeaseId};
use thiserror::Error;

/// Atomic store operation associated with a classified failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoreOperation {
  /// Create one Project below an optional parent.
  CreateProject,
  /// Rename one Project using an optimistic version precondition.
  RenameProject,
  /// Move one Project using an optimistic version precondition.
  MoveProject,
  /// Delete one unreferenced Project using an optimistic version precondition.
  DeleteProject,
  /// Read one Project together with its ancestry.
  ReadProject,
  /// List one bounded page of direct child Projects.
  ListProjects,
  /// Create one Pipeline identity and immutable initial version.
  CreatePipeline,
  /// Append exactly the next immutable Pipeline version.
  PublishPipelineVersion,
  /// Read one exact immutable Pipeline version.
  ReadPipelineVersion,
  /// Create one Repository identity and immutable initial version.
  CreateRepository,
  /// Append exactly the next immutable Repository version.
  PublishRepositoryVersion,
  /// Read one exact immutable Repository version.
  ReadRepositoryVersion,
  /// Create one Build Configuration identity and immutable initial version.
  CreateBuildConfiguration,
  /// Append exactly the next immutable Build Configuration version.
  PublishBuildConfigurationVersion,
  /// Read one exact immutable Build Configuration version.
  ReadBuildConfigurationVersion,
  /// Deduplicate a Trigger and persist its complete initial Build graph.
  AcceptTrigger,
  /// Select one compatible ready Job and create its current Lease.
  ClaimReadyJob,
  /// Append one contiguous idempotent batch of Job events.
  AppendJobEvents,
  /// Commit one terminal Job outcome after its event history is durable.
  CompleteJob,
  /// Issue one short-lived, single-use Agent enrollment credential.
  IssueAgentEnrollment,
  /// Consume valid Agent authority and create a fresh registration epoch.
  RegisterAgent,
  /// Authenticate one current Agent registration credential.
  AuthenticateAgentRegistration,
  /// Revoke one enrollment or registration credential.
  RevokeAgentCredential,
}

/// Invalid caller input rejected before any authoritative state changes.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum StoreInputError {
  /// An idempotency key is empty, malformed, or exceeds its byte bound.
  #[error("an idempotency key is invalid")]
  InvalidIdempotencyKey,
  /// A Project page size is zero or exceeds the store contract bound.
  #[error("a project page size is outside its allowed range")]
  InvalidProjectPageSize,
  /// A Pipeline DAG failed structural revalidation at the adapter seam.
  #[error("an immutable pipeline snapshot is invalid")]
  InvalidPipelineSnapshot,
  /// A Repository definition is malformed or exceeds its encoded bound.
  #[error("an immutable repository definition is invalid")]
  InvalidRepositoryDefinition,
  /// A Build Configuration definition is malformed or exceeds its encoded bound.
  #[error("an immutable build configuration is invalid")]
  InvalidBuildConfiguration,
  /// A Trigger occurrence is malformed or violates source-specific invariants.
  #[error("a normalized trigger occurrence is invalid")]
  InvalidNormalizedTrigger,
  /// The normalized occurrence and materialized Build target different configuration versions.
  #[error("a normalized trigger target does not match the build configuration")]
  TriggerTargetMismatch,
  /// An accepted Build must materialize at least one Job.
  #[error("a materialized attempt must contain at least one job")]
  EmptyJobGraph,
  /// Two materialized Jobs use the same stable identity.
  #[error("a materialized attempt contains a duplicate job identity")]
  DuplicateJob,
  /// Two Jobs use the same Pipeline node identity.
  #[error("a materialized attempt contains a duplicate pipeline node")]
  DuplicatePipelineNode,
  /// One Job repeats the same dependency identity.
  #[error("a materialized job contains a duplicate dependency")]
  DuplicateDependency,
  /// One Job names itself as a dependency.
  #[error("a materialized job cannot depend on itself")]
  SelfDependency,
  /// One dependency does not belong to the same materialized Attempt.
  #[error("a materialized job references an unknown dependency")]
  UnknownDependency,
  /// The materialized dependency graph contains a cycle.
  #[error("a materialized attempt must be acyclic")]
  CyclicJobGraph,
  /// A ready Job has no Pool in which it may execute.
  #[error("a materialized job must allow at least one pool")]
  EmptyAllowedPools,
  /// One Pool is repeated in a Job's effective allowlist.
  #[error("a materialized job contains a duplicate allowed pool")]
  DuplicateAllowedPool,
  /// A persisted structured document must be a JSON object.
  #[error("a structured store document must be a JSON object")]
  JsonDocumentMustBeObject,
  /// A persisted structured document exceeds the store contract byte bound.
  #[error("a structured store document exceeds its byte bound")]
  JsonDocumentTooLarge,
  /// A persisted structured document exceeds the process-safe nesting bound.
  #[error("a structured store document exceeds its nesting bound")]
  JsonDocumentTooDeep,
  /// An immutable source revision is empty, malformed, or too long.
  #[error("an immutable source revision is invalid")]
  InvalidImmutableRevision,
  /// A Job-event classification is empty, malformed, or too long.
  #[error("a job event classification is invalid")]
  InvalidEventKind,
  /// A materialized Attempt contains more Jobs than one atomic request permits.
  #[error("a materialized attempt contains too many jobs")]
  TooManyJobs,
  /// A materialized Job contains more dependencies than the contract permits.
  #[error("a materialized job contains too many dependencies")]
  TooManyDependencies,
  /// One atomic Attempt contains more dependency edges than the contract permits.
  #[error("a materialized attempt contains too many dependency edges")]
  TooManyDependencyEdges,
  /// A materialized Job allows more Pools than the contract permits.
  #[error("a materialized job contains too many allowed pools")]
  TooManyAllowedPools,
  /// A Job-event payload exceeds the store contract byte bound.
  #[error("a job event payload exceeds its byte bound")]
  EventPayloadTooLarge,
  /// An append request contains more events than one transaction permits.
  #[error("an event batch exceeds its item bound")]
  EventBatchTooLarge,
  /// An atomic store request exceeds its total encoded-byte budget.
  #[error("an atomic store request exceeds its encoded-byte budget")]
  RequestTooLarge,
  /// A positive domain number cannot be represented by the configured store.
  #[error("a numeric value exceeds the authoritative store range")]
  NumericOutOfRange,
  /// Registration epochs start at one.
  #[error("a registration epoch must be greater than zero")]
  ZeroRegistrationEpoch,
  /// A credential must expire strictly after it is issued.
  #[error("a credential expiry must be later than its issue time")]
  InvalidCredentialWindow,
  /// An Agent platform label is empty, malformed, or too long.
  #[error("an agent platform is invalid")]
  InvalidAgentPlatform,
  /// A Lease must expire strictly after it is claimed.
  #[error("a lease expiry must be later than its claim time")]
  InvalidLeaseWindow,
  /// An append operation must carry at least one event.
  #[error("an event batch must not be empty")]
  EmptyEventBatch,
  /// Event sequences inside one request must be contiguous and increasing.
  #[error("an event batch must contain contiguous increasing sequences")]
  NonContiguousEventBatch,
}

/// Backend-neutral failure from an authoritative atomic operation.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum StoreError {
  /// The request is invalid independently of current stored state.
  #[error("invalid {operation:?} input: {source}")]
  InvalidInput {
    /// Operation that rejected the input.
    operation: StoreOperation,
    /// Stable validation failure.
    source: StoreInputError,
  },
  /// A referenced entity does not exist.
  #[error("{entity} was not found")]
  NotFound {
    /// Missing entity kind.
    entity: EntityKind,
  },
  /// Existing authoritative state conflicts with this mutation.
  #[error("{entity} conflicts with the requested mutation")]
  Conflict {
    /// Entity whose authoritative state conflicts.
    entity: EntityKind,
  },
  /// A unique mutation identity already belongs to another request.
  #[error("{entity} already exists")]
  Duplicate {
    /// Entity whose identity was already used.
    entity: EntityKind,
  },
  /// The supplied Lease no longer owns the Job.
  #[error("lease {lease} is fenced")]
  Fenced {
    /// Rejected Lease identity.
    lease: LeaseId,
  },
  /// The supplied Lease reached its authoritative expiry.
  #[error("lease {lease} is expired")]
  Expired {
    /// Expired Lease identity.
    lease: LeaseId,
  },
  /// A Job-event batch starts after the next required sequence.
  #[error("job {job} event gap: expected sequence {expected}, received {actual}")]
  EventGap {
    /// Job whose event stream contains the gap.
    job: JobId,
    /// Next sequence required by durable state.
    expected: u64,
    /// First new sequence supplied by the caller.
    actual: u64,
  },
  /// Completion references event history that is not durable yet.
  #[error("job {job} completion requires event {required}, durable through {durable_through}")]
  EventsMissing {
    /// Job whose completion was rejected.
    job: JobId,
    /// Greatest durable sequence, or zero when the stream is empty.
    durable_through: u64,
    /// Final sequence declared by completion.
    required: u64,
  },
  /// A protected Agent credential is unknown, expired, consumed, revoked, or superseded.
  #[error("agent credential was rejected")]
  CredentialRejected,
  /// The adapter cannot currently execute the operation.
  #[error("authoritative store is unavailable")]
  Unavailable,
}

impl StoreError {
  pub(crate) const fn invalid(operation: StoreOperation, source: StoreInputError) -> Self {
    Self::InvalidInput { operation, source }
  }
}
