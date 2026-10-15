//! Durable application ports for one current common-node observer.
use crate::{
  DecisionSignalDispatchAuthorization, DecisionSignalPortError, DecisionSignalReceiptPublication,
  DecisionSignalRequestObservation, FactoryExecutionClock,
};
use async_trait::async_trait;
use octacity_server_factory::{DecisionSignalProviderRequest, DecisionSignalReceipt, FactoryClaimOwnership};
use octacity_server_store::{
  FactoryDecisionSignalReceiptStore, FactoryDecisionSignalRequestStore, PublishFactoryDecisionSignalReceipt,
  ReserveFactoryDecisionSignalRequest, StoreError,
};
use std::sync::Arc;

/// Shares authoritative receipt recovery and fenced publication with the provider coordinator.
/// Ownership is mutable dispatch authority; it never changes the logical request.
pub struct FactoryDecisionSignalReceipts<S> {
  reservation_limit: Option<octacity_server_factory::BudgetLimit>,
  store: Arc<S>,
  ownership: FactoryClaimOwnership,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<S> FactoryDecisionSignalReceipts<S> {
  /// Binds the current observer, authoritative storage and trusted time.
  pub fn new(store: Arc<S>, ownership: FactoryClaimOwnership, clock: Arc<dyn FactoryExecutionClock>) -> Self {
    Self {
      reservation_limit: None,
      store,
      ownership,
      clock,
    }
  }
  /// Narrows the shared provider reservation for an installed Build-node policy.
  /// The native journal checks the complete used/pending total atomically.
  pub fn with_reservation_limit(mut self, limit: octacity_server_factory::BudgetLimit) -> Self {
    self.reservation_limit = Some(limit);
    self
  }
}
#[async_trait]
impl<S: FactoryDecisionSignalRequestStore> DecisionSignalDispatchAuthorization for FactoryDecisionSignalReceipts<S> {
  async fn was_reserved(&self, request: &DecisionSignalProviderRequest) -> Result<bool, DecisionSignalPortError> {
    self
      .store
      .factory_decision_signal_request_reserved(request)
      .await
      .map_err(classify)
  }
  async fn authorize_dispatch(
    &self,
    request: &DecisionSignalProviderRequest,
  ) -> Result<octacity_server_store::FactoryDecisionSignalRequestDisposition, DecisionSignalPortError> {
    self
      .store
      .reserve_factory_decision_signal_request(ReserveFactoryDecisionSignalRequest {
        lease_guard: None,
        reservation_limit: self.reservation_limit,
        request: request.clone(),
        ownership: self.ownership.clone(),
        observed_at: self.clock.now().map_err(|_| DecisionSignalPortError::Unavailable)?,
      })
      .await
      .map_err(classify)
  }
}
#[async_trait]
impl<S: FactoryDecisionSignalReceiptStore> DecisionSignalRequestObservation for FactoryDecisionSignalReceipts<S> {
  async fn observe_request(
    &self,
    request: &DecisionSignalProviderRequest,
  ) -> Result<Option<DecisionSignalReceipt>, DecisionSignalPortError> {
    self
      .store
      .factory_decision_signal_receipt(request)
      .await
      .map_err(classify)
  }
}
#[async_trait]
impl<S: FactoryDecisionSignalReceiptStore> DecisionSignalReceiptPublication for FactoryDecisionSignalReceipts<S> {
  async fn publish_receipt(
    &self,
    receipt: DecisionSignalReceipt,
  ) -> Result<DecisionSignalReceipt, DecisionSignalPortError> {
    self
      .store
      .publish_factory_decision_signal_receipt(PublishFactoryDecisionSignalReceipt {
        receipt: receipt.clone(),
        ownership: self.ownership.clone(),
        observed_at: self.clock.now().map_err(|_| DecisionSignalPortError::Unavailable)?,
      })
      .await
      .map_err(classify)?;
    Ok(receipt)
  }
}
fn classify(error: StoreError) -> DecisionSignalPortError {
  match error {
    StoreError::Unavailable => DecisionSignalPortError::Unavailable,
    _ => DecisionSignalPortError::Conflict,
  }
}
