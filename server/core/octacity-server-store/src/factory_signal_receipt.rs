//! Durable private Decision Signal observations under common node ownership.
use crate::{LeaseAccess, StoreError};
use async_trait::async_trait;
use octacity_server_domain::{AttemptNumber, JobId, Timestamp};
use octacity_server_factory::{
  BudgetUsage, DecisionSignalProviderRequest, DecisionSignalReceipt, FactoryClaimOwnership, FactoryRunId, NodeAttemptId,
};

/// Measured calls and unresolved full ceilings for one exact node. Reading this
/// summary never renews a claim, dispatches a provider or releases pending usage.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FactoryDecisionSignalNodeUsage {
  /// Immutable measured receipts, each counted once.
  pub consumed: BudgetUsage,
  /// Complete reserved ceilings until authoritative receipts replace them.
  pub pending: BudgetUsage,
}
/// Bounded accounting shared by Build output settlement and tool-risk dispatch.
#[async_trait]
pub trait FactoryDecisionSignalUsageStore: Send + Sync {
  /// Reads one Project-bound aggregate under the same Run lock as reservations.
  async fn factory_decision_signal_node_usage(
    &self,
    run_id: FactoryRunId,
    node_id: NodeAttemptId,
  ) -> Result<FactoryDecisionSignalNodeUsage, StoreError>;
}

/// Durable authorization reserved before contacting an external signal provider.
/// Private request bytes are excluded from debug and management projections.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FactoryDecisionSignalLeaseGuard {
  /// Current ordinary lease, secret fence and registration epoch.
  pub lease: LeaseAccess,
  /// Exact Job that owns the blocked proposal.
  pub job_id: JobId,
  /// Exact positive ordinary Attempt number.
  pub attempt: AttemptNumber,
}

/// Durable authorization reserved before contacting an external signal provider.
/// Private request bytes are excluded from debug and management projections.
#[derive(Clone, Eq, PartialEq)]
pub struct ReserveFactoryDecisionSignalRequest {
  /// Additional ordinary execution authority for lease-bound tool-risk calls.
  /// The native reservation transaction checks this with the same Run lock;
  /// a previous application read is never a dispatch authorization proof.
  pub lease_guard: Option<FactoryDecisionSignalLeaseGuard>,
  /// Optional installed sub-budget shared by every Signal call in this node.
  /// Build compilers reserve the remaining tokens/cost/output for the workload.
  pub reservation_limit: Option<octacity_server_factory::BudgetLimit>,
  /// Complete frozen request, including exact model, policy, input and deadline.
  pub request: DecisionSignalProviderRequest,
  /// Current fenced node observer authorizing this dispatch.
  pub ownership: FactoryClaimOwnership,
  /// Trusted time of authorization, within the original dispatch window.
  pub observed_at: Timestamp,
}

/// A repeated request holds the original budget reservation without adding a call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FactoryDecisionSignalRequestDisposition {
  /// New immutable dispatch intent was committed before external I/O.
  Reserved,
  /// Existing intent agrees under current ownership and time. Its unresolved
  /// external outcome must be observed; replay never permits another evaluation.
  Replayed,
}

/// Fenced dispatch intents retain budget when the external outcome is unknown.
#[async_trait]
pub trait FactoryDecisionSignalRequestStore: Send + Sync {
  /// Read-only recovery of an exact request; no renewed dispatch authority.
  async fn factory_decision_signal_request_reserved(
    &self,
    request: &DecisionSignalProviderRequest,
  ) -> Result<bool, StoreError>;
  /// Atomically verifies current ownership and reserves the complete call ceiling.
  /// Existing measured receipts and unresolved reservations share the node budget.
  async fn reserve_factory_decision_signal_request(
    &self,
    request: ReserveFactoryDecisionSignalRequest,
  ) -> Result<FactoryDecisionSignalRequestDisposition, StoreError>;
}

/// Complete private consumed observation. It grants no Flow transition authority.
#[derive(Clone, Eq, PartialEq)]
pub struct PublishFactoryDecisionSignalReceipt {
  /// Exact request, observation and deterministic consumption to retain.
  pub receipt: DecisionSignalReceipt,
  /// Current owner authorizing a fresh publication.
  pub ownership: FactoryClaimOwnership,
  /// Trusted publication time; a replay retains its original time.
  pub observed_at: Timestamp,
}

/// Fenced immutable receipt persistence shared by routing and tool-risk purposes.
#[async_trait]
pub trait FactoryDecisionSignalReceiptStore: Send + Sync {
  /// Recovers only an exactly matching request; substitution conflicts instead of redispatching.
  async fn factory_decision_signal_receipt(
    &self,
    request: &DecisionSignalProviderRequest,
  ) -> Result<Option<DecisionSignalReceipt>, StoreError>;
  /// Atomically publishes private bytes and an immutable safe audit fact.
  /// Current node ownership, cancellation, deadline and frozen input are checked
  /// for new effects. Identical publication replay is read-only after expiry.
  async fn publish_factory_decision_signal_receipt(
    &self,
    request: PublishFactoryDecisionSignalReceipt,
  ) -> Result<(), StoreError>;
}
