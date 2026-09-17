use async_trait::async_trait;

use crate::{
  AcceptTrigger, AcceptTriggerOutcome, AppendJobEvents, AppendJobEventsOutcome, CompletionDisposition, JobClaim,
  JobClaimOutcome, JobCompletion, StoreError,
};

/// Backend-neutral authoritative store shaped around complete atomic use cases.
///
/// Every method is one transaction boundary. Callers never coordinate table
/// repositories, database transactions, queue rows, or lease rows themselves.
#[async_trait]
pub trait AuthoritativeStore: Send + Sync {
  /// Deduplicates a Trigger occurrence and commits its Build, first Attempt,
  /// complete materialized DAG, and root ready-queue entries together.
  async fn accept_trigger(&self, request: AcceptTrigger) -> Result<AcceptTriggerOutcome, StoreError>;

  /// Selects one compatible root Job from the global ready queue and commits
  /// its exclusive current Lease in the same operation.
  async fn claim_ready_job(&self, request: JobClaim) -> Result<JobClaimOutcome, StoreError>;

  /// Appends a contiguous idempotent Job-event batch and advances its durable
  /// cursor only after the complete batch commits.
  async fn append_job_events(&self, request: AppendJobEvents) -> Result<AppendJobEventsOutcome, StoreError>;

  /// Commits an idempotent terminal outcome only when the declared final event
  /// cursor is already durable for the current Lease.
  async fn complete_job(&self, request: JobCompletion) -> Result<CompletionDisposition, StoreError>;
}
