use async_trait::async_trait;

use crate::{
  AcceptTrigger, AcceptTriggerOutcome, AppendJobEvents, AppendJobEventsOutcome, CancelBuild, CancellationDisposition,
  CompletionDisposition, JobClaim, JobClaimOutcome, JobCompletion, JobEventAppendPreparation, JobEventPage,
  LeaseAccess, LeaseHeartbeatOutcome, ReadJobEvents, RenewLease, RetryBuild, RetryDisposition, StoreError,
  SuppressTrigger, SuppressTriggerOutcome, TriggerAcceptanceProbe, TriggerEvaluationOutcome,
};
use octacity_server_domain::LogChunkId;

/// Atomic persistence used by Trigger acceptance.
#[async_trait]
pub trait TriggerAcceptanceStore: Send + Sync {
  /// Returns an identical accepted outcome before mutable external resolution,
  /// or reports a conflict when the same identity carries different intent.
  async fn replay_trigger_acceptance(
    &self,
    request: TriggerAcceptanceProbe,
  ) -> Result<Option<TriggerEvaluationOutcome>, StoreError>;

  /// Deduplicates a Trigger occurrence and commits its Build, first Attempt,
  /// complete materialized DAG, and root ready-queue entries together.
  async fn accept_trigger(&self, request: AcceptTrigger) -> Result<AcceptTriggerOutcome, StoreError>;

  /// Commits one terminal suppressed occurrence without Build or queue state.
  async fn suppress_trigger(&self, request: SuppressTrigger) -> Result<SuppressTriggerOutcome, StoreError>;
}

/// Atomic persistence used only by the latency-sensitive Lease heartbeat path.
#[async_trait]
pub trait LeaseHeartbeatStore: Send + Sync {
  /// Validates current ownership, renews an eligible Lease, and returns one
  /// control directive without depending on event or telemetry ingestion.
  async fn renew_lease(&self, request: RenewLease) -> Result<LeaseHeartbeatOutcome, StoreError>;
}

/// Atomic persistence used by Job placement and execution.
#[async_trait]
pub trait JobExecutionStore: Send + Sync {
  /// Selects one compatible root Job from the global ready queue and commits
  /// its exclusive current Lease in the same operation.
  async fn claim_ready_job(&self, request: JobClaim) -> Result<JobClaimOutcome, StoreError>;

  /// Revalidates ownership and returns the cursor used to avoid rewriting an
  /// already archived replay prefix. Implementations that cannot archive logs
  /// reject the operation rather than acknowledging unarchived output.
  async fn prepare_job_event_append(
    &self,
    _lease: LeaseAccess,
    _accepted_at: octacity_server_domain::Timestamp,
  ) -> Result<JobEventAppendPreparation, StoreError> {
    Err(StoreError::Unavailable)
  }

  /// Appends a contiguous idempotent Job-event batch and advances its durable
  /// cursor only after the complete batch commits.
  async fn append_job_events(&self, request: AppendJobEvents) -> Result<AppendJobEventsOutcome, StoreError>;

  /// Commits an idempotent terminal outcome only when the declared final event
  /// cursor is already durable for the current Lease.
  async fn complete_job(&self, request: JobCompletion) -> Result<CompletionDisposition, StoreError>;
}

/// Authoritative visibility check used before deleting an orphan object.
#[async_trait]
pub trait LogChunkManifestStore: Send + Sync {
  /// Returns true only when a committed visible manifest owns the identity.
  async fn log_chunk_is_committed(&self, chunk_id: LogChunkId) -> Result<bool, StoreError>;
}

/// Read-only authoritative access to immutable Job-event streams.
///
/// Each invocation is a complete short store operation. Implementations must
/// not retain a transaction or checked-out database connection after return.
#[async_trait]
pub trait JobEventReadStore: Send + Sync {
  /// Reads one bounded ordered page after the caller's durable cursor.
  async fn read_job_events(&self, request: ReadJobEvents) -> Result<JobEventPage, StoreError>;
}

/// Atomic persistence used by Build cancellation and retry.
#[async_trait]
pub trait BuildControlStore: Send + Sync {
  /// Persists one idempotent Build cancellation intent and atomically removes
  /// queued work or requests cancellation from current Lease owners.
  async fn cancel_build(&self, request: CancelBuild) -> Result<CancellationDisposition, StoreError>;

  /// Locks one failed Build, allocates exactly its next Attempt number, verifies
  /// the candidate DAG against immutable prior snapshots, and enqueues roots.
  async fn retry_build(&self, request: RetryBuild) -> Result<RetryDisposition, StoreError>;
}

/// Complete backend-neutral authoritative-store contract.
///
/// Every method inherited from the operation-specific ports is one transaction
/// boundary. Applications SHOULD depend on the narrowest port that serves
/// their use case; adapter contract suites use this composite interface.
pub trait AuthoritativeStore: TriggerAcceptanceStore + JobExecutionStore + BuildControlStore + Send + Sync {}

impl<T> AuthoritativeStore for T where
  T: TriggerAcceptanceStore + JobExecutionStore + BuildControlStore + Send + Sync + ?Sized
{
}
