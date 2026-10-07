use async_trait::async_trait;
use octacity_server_factory::FactoryRunId;

use crate::{
  ClaimFactoryOutbox, ClaimFactoryRun, ClaimFactoryRunOutcome, ClaimFactoryRuns, ClaimedFactoryOutbox,
  ClaimedFactoryRun, CommitFactoryRunTransition, CommitFactoryRunTransitionOutcome, FactoryOutboxRecord,
  FactoryRunDiagnosticPage, FactoryRunSnapshot, ListFactoryRunDiagnostics, SettleFactoryOutbox, StoreError,
};

/// Backend-neutral atomic operations for durable Factory Run reconciliation.
///
/// The port exposes complete use cases rather than table CRUD: a bounded
/// snapshot read, append-only ownership claims, one fenced aggregate
/// transition, and fenced recovery of durable external side effects.
#[async_trait]
pub trait FactoryRunStore: Send + Sync {
  /// Loads the complete bounded authoritative snapshot for one Run.
  async fn factory_run_snapshot(&self, run_id: FactoryRunId) -> Result<FactoryRunSnapshot, StoreError>;

  /// Lists one typed bounded diagnostic collection in stable identity order.
  async fn list_factory_run_diagnostics(
    &self,
    request: ListFactoryRunDiagnostics,
  ) -> Result<FactoryRunDiagnosticPage, StoreError>;

  /// Appends one exclusive claim or returns its exact prior result.
  async fn claim_factory_run(&self, request: ClaimFactoryRun) -> Result<ClaimFactoryRunOutcome, StoreError>;

  /// Atomically claims a deterministic bounded set of eligible Runs.
  async fn claim_factory_runs(&self, request: ClaimFactoryRuns) -> Result<Vec<ClaimedFactoryRun>, StoreError>;

  /// Atomically appends immutable history, audit and outbox rows and then
  /// advances the current projection under the expected version and fence.
  async fn commit_factory_run_transition(
    &self,
    request: CommitFactoryRunTransition,
  ) -> Result<CommitFactoryRunTransitionOutcome, StoreError>;

  /// Claims a bounded due batch, taking over expired unknown outcomes by the same stable operation identity.
  async fn claim_factory_outbox(&self, request: ClaimFactoryOutbox) -> Result<Vec<ClaimedFactoryOutbox>, StoreError>;

  /// Appends one fenced delivered, retry, or terminal-failure observation.
  async fn settle_factory_outbox(&self, request: SettleFactoryOutbox) -> Result<FactoryOutboxRecord, StoreError>;
}
