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
  /// Publish the next immutable Project policy version.
  PublishProjectPolicy,
  /// Read one Project together with its ancestry.
  ReadProject,
  /// List one bounded page of direct child Projects.
  ListProjects,
  /// Create one static Agent Pool and its initial version.
  CreateAgentPool,
  /// Append the next immutable Agent Pool version.
  PublishAgentPoolVersion,
  /// Read one exact Agent Pool version.
  ReadAgentPoolVersion,
  /// List one bounded page of current Agent Pools.
  ListAgentPools,
  /// Delete one unreferenced Agent Pool.
  DeleteAgentPool,
  /// Read one enrolled Agent.
  ReadAgent,
  /// List one bounded page of enrolled Agents.
  ListAgents,
  /// Move one idle Agent to another Pool.
  ReassignAgentPool,
  /// Place one Agent into graceful or forced drain.
  DrainAgent,
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
  /// Create one immutable Trigger definition.
  CreateTriggerDefinition,
  /// Create version one of an internal Trigger definition.
  CreateInternalTriggerDefinition,
  /// Publish the next immutable internal Trigger definition version.
  PublishInternalTriggerVersion,
  /// Read one exact internal Trigger definition version.
  ReadInternalTriggerDefinition,
  /// List current internal Trigger definitions.
  ListInternalTriggerDefinitions,
  /// Atomically create an unmanaged webhook integration and external Trigger.
  CreateUnmanagedWebhook,
  /// Read one unmanaged webhook integration.
  ReadUnmanagedWebhook,
  /// Reserve one managed webhook integration and external Trigger.
  CreateManagedWebhook,
  /// Read one managed webhook integration.
  ReadManagedWebhook,
  /// Commit one normalized managed-registration result.
  RecordManagedWebhookRegistration,
  /// Enqueue one idempotent managed-provider operation.
  EnqueueManagedWebhookOperation,
  /// Claim due managed-provider operations.
  ClaimManagedWebhookOperations,
  /// Schedule retry or dead-letter one managed-provider operation.
  FailManagedWebhookOperation,
  /// Durably admit one raw webhook receipt.
  EnqueueWebhookDelivery,
  /// Claim due webhook verification or Trigger work.
  ClaimWebhookDeliveries,
  /// Commit one authenticated normalized webhook event.
  RecordWebhookEvent,
  /// Schedule a retry or retain a webhook dead letter.
  FailWebhookDelivery,
  /// Complete one normalized webhook receipt.
  CompleteWebhookDelivery,
  /// Read secret-free webhook delivery diagnostics.
  ReadWebhookDelivery,
  /// Create one immutable scheduled Trigger and its durable cursor.
  CreateSchedule,
  /// Read one durable schedule.
  ReadSchedule,
  /// Claim a bounded batch of due schedules.
  ClaimDueSchedules,
  /// Advance one owned schedule claim after evaluation.
  CompleteScheduleClaim,
  /// Claim terminal Build events from the transactional outbox.
  ClaimInternalTriggerEvents,
  /// Mark one owned internal-Trigger source event delivered.
  CompleteInternalTriggerEvent,
  /// Deduplicate a Trigger and persist its complete initial Build graph.
  AcceptTrigger,
  /// Persist and initially claim one manual Trigger evaluation.
  ReserveTriggerEvaluation,
  /// Claim due manual Trigger evaluations.
  ClaimTriggerEvaluations,
  /// Persist one immutable revision checkpoint for durable Trigger evaluation.
  RecordTriggerEvaluationRevision,
  /// Complete one owned manual Trigger evaluation.
  CompleteTriggerEvaluation,
  /// Retry or dead-letter one owned manual Trigger evaluation.
  FailTriggerEvaluation,
  /// Select one compatible ready Job and create its current Lease.
  ClaimReadyJob,
  /// Renew one current Lease and select its control directive.
  RenewLease,
  /// Claim a bounded durable batch of expired Leases.
  ClaimExpiredLeases,
  /// Fence and recover one durably claimed expired Lease.
  RecoverExpiredLease,
  /// Append one contiguous idempotent batch of Job events.
  AppendJobEvents,
  /// Inspect current cursor before preparing immutable log objects.
  PrepareJobEventAppend,
  /// Check whether a logical log chunk is durably visible.
  ReadLogChunkManifest,
  /// Claim a bounded batch of durable Build-log indexing work.
  ClaimLogIndexWork,
  /// Mark one owned Build-log indexing item complete.
  CompleteLogIndexWork,
  /// Schedule retry or retain a dead letter for Build-log indexing work.
  FailLogIndexWork,
  /// Read one bounded ordered page from a Job event stream.
  ReadJobEvents,
  /// Commit one terminal Job outcome after its event history is durable.
  CompleteJob,
  /// Apply durable cancellation intent to one Build and its current Attempt.
  CancelBuild,
  /// Materialize the next Attempt from one failed Build's immutable snapshots.
  RetryBuild,
  /// Issue one short-lived, single-use Agent enrollment credential.
  IssueAgentEnrollment,
  /// Consume valid Agent authority and create a fresh registration epoch.
  RegisterAgent,
  /// Authenticate one current Agent registration credential.
  AuthenticateAgentRegistration,
  /// Revoke one enrollment or registration credential.
  RevokeAgentCredential,
  /// Reserve one pending logical Artifact under a current Lease.
  ReserveArtifact,
  /// Read one logical Artifact record.
  ReadArtifact,
  /// Apply one logical Artifact lifecycle transition.
  TransitionArtifact,
  /// Reserve one logical Artifact and pending upload idempotently.
  BeginArtifactUpload,
  /// Read one pending or completed Artifact upload.
  ReadArtifactUpload,
  /// Begin independent verification of uploaded bytes.
  BeginArtifactVerification,
  /// Commit the result of independent byte verification.
  FinishArtifactVerification,
  /// List visible logical outputs of one Build.
  ListPublishedArtifacts,
  /// Begin one fenced short-lived cache session.
  BeginCacheSession,
  /// Revoke one fenced cache session idempotently.
  RevokeCacheSession,
  /// Authorize one namespace-scoped cache operation.
  AuthorizeCacheSession,
  /// Read one secret-free cache-session diagnostic.
  ReadCacheSession,
  /// List bounded cache-session diagnostics for one Build.
  ListCacheSessions,
  /// Query scoped L2 cache blob metadata.
  ReadCacheBlob,
  /// Publish scoped L2 cache blob metadata.
  PublishCacheBlob,
  /// Query scoped L2 cache action metadata.
  ReadCacheAction,
  /// Publish scoped L2 cache action metadata.
  PublishCacheAction,
  /// Apply scoped L2 cache retention.
  PruneCache,
  /// Claim a bounded batch of due Build Result retention work.
  ClaimRetentionWork,
  /// Remove component visibility and enumerate bounded cleanup objects.
  PrepareRetentionWork,
  /// Record a durable Build-log search tombstone.
  CompleteRetentionSearch,
  /// Record one safely released retained object.
  CompleteRetentionObject,
  /// Complete or release one bounded retention pass.
  FinishRetentionPass,
  /// Schedule another durable retention attempt.
  FailRetentionWork,
  /// Stage cleanup before writing one archived log object.
  StageOrphanLogChunk,
  /// Claim a bounded batch of due orphan-log cleanup candidates.
  ClaimOrphanLogChunks,
  /// Complete one orphan-log cleanup candidate.
  CompleteOrphanLogChunk,
  /// Retry or dead-letter one orphan-log cleanup candidate.
  FailOrphanLogChunk,
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
  /// An Agent Pool page size is zero or exceeds the store contract bound.
  #[error("an agent pool page size is outside its allowed range")]
  InvalidAgentPoolPageSize,
  /// An Agent page size is zero or exceeds the store contract bound.
  #[error("an agent page size is outside its allowed range")]
  InvalidAgentPageSize,
  /// An internal Trigger page size is zero or exceeds its contract bound.
  #[error("an internal trigger page size is outside its allowed range")]
  InvalidInternalTriggerPageSize,
  /// A durable worker owner, deadline, or batch size is invalid.
  #[error("a durable worker claim is invalid")]
  InvalidWorkerClaim,
  /// One internal event matches more Trigger definitions than one bounded delivery permits.
  #[error("an internal trigger event has too many matching definitions")]
  TooManyInternalTriggerMatches,
  /// An Agent Pool admission policy is empty or exceeds its bound.
  #[error("an agent pool admission policy is invalid")]
  InvalidPoolAdmissionPolicy,
  /// An Agent Pool capacity is zero, inconsistent, or exceeds its bound.
  #[error("an agent pool capacity policy is invalid")]
  InvalidPoolCapacity,
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
  /// A durable Trigger-evaluation payload or diagnostic is invalid.
  #[error("a durable trigger evaluation is invalid")]
  InvalidTriggerEvaluation,
  /// An unmanaged webhook definition is malformed or exceeds its bounds.
  #[error("an unmanaged webhook definition is invalid")]
  InvalidWebhookDefinition,
  /// A webhook delivery, normalized event, or diagnostic is malformed or exceeds its bounds.
  #[error("a webhook delivery is invalid")]
  InvalidWebhookDelivery,
  /// A Build's typed effective scheduling policy is outside its valid range.
  #[error("an immutable build scheduling policy is invalid")]
  InvalidBuildSchedulingPolicy,
  /// A Build Result automatic-retention deadline precedes Build acceptance.
  #[error("an immutable build retention deadline is invalid")]
  InvalidBuildRetention,
  /// The normalized occurrence and materialized Build target different configuration versions.
  #[error("a normalized trigger target does not match the build configuration")]
  TriggerTargetMismatch,
  /// Stable execution intent was not derived for its surrounding Build and Pipeline node.
  #[error("a JobSpec template does not match its materialized job graph")]
  JobSpecTemplateBindingMismatch,
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
  /// A Job-event read page size is zero or exceeds its contract bound.
  #[error("a job event page size is outside its allowed range")]
  InvalidJobEventPageSize,
  /// A Job-event long-poll wait exceeds the application contract bound.
  #[error("a job event wait exceeds its allowed bound")]
  InvalidJobEventWait,
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
  /// A registration inventory is incomplete or violates the shared protocol contract.
  #[error("an agent inventory is invalid")]
  InvalidAgentInventory,
  /// A Lease must expire strictly after it is claimed.
  #[error("a lease expiry must be later than its claim time")]
  InvalidLeaseWindow,
  /// An append operation must carry at least one event.
  #[error("an event batch must not be empty")]
  EmptyEventBatch,
  /// Event sequences inside one request must be contiguous and increasing.
  #[error("an event batch must contain contiguous increasing sequences")]
  NonContiguousEventBatch,
  /// A log-chunk manifest is malformed, overlaps another chunk, or does not
  /// cover exactly the newly accepted stdout/stderr events.
  #[error("log chunk manifests do not match the event batch")]
  InvalidLogChunkManifest,
  /// A durable Build-log indexing failure code or retry time is invalid.
  #[error("durable log-index work failure is invalid")]
  InvalidLogIndexWorkFailure,
  /// A retry must name an Attempt number after the initial Attempt.
  #[error("a retry attempt number must be greater than one")]
  InvalidRetryAttempt,
  /// Artifact identity, retention, or transition authority is invalid.
  #[error("an artifact record request is invalid")]
  InvalidArtifact,
  /// Cache session identity, policy, expiry, or page size is invalid.
  #[error("a cache session request is invalid")]
  InvalidCacheSession,
  /// Cache action or blob metadata violates the published protocol contract.
  #[error("cache data-plane metadata is invalid")]
  InvalidCacheData,
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
