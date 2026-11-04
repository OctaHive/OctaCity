//! Bounded native policy discovery and exact-capability phase admission.
use async_trait::async_trait;
use octacity_server_domain::Timestamp;
use octacity_server_factory::{
  FactoryDigest, FactoryKey, ImmutableReference, MAX_FACTORY_EXECUTOR_BINDINGS, MAX_PHASE_POOL_BATCH,
};
use octacity_server_store::{FactoryPhasePoolStore, SelectPhasePool, StoreError};
use std::{
  collections::BTreeSet,
  sync::{Arc, Mutex},
};

/// Optional admission pass run before ordinary owner claims.
#[async_trait]
pub trait FactoryPhasePoolCoordination: Send + Sync {
  /// Selects bounded ready work using the same owner and claim window as reconciliation.
  async fn select_once(&self, owner: &FactoryKey, at: Timestamp, expires: Timestamp) -> Result<(), StoreError>;
}

/// Discovers only persisted policy identities and delegates authoritative ranking,
/// WIP, dependencies, reservations and fences to the existing pool store.
pub struct FactoryPhasePoolSelector<S> {
  store: Arc<S>,
  capabilities: BTreeSet<ImmutableReference>,
  limit: u16,
  after: Mutex<Option<FactoryDigest>>,
}
impl<S> FactoryPhasePoolSelector<S> {
  /// Freezes exact installed capabilities; node requirements never establish availability.
  pub fn new(
    store: Arc<S>,
    capabilities: BTreeSet<ImmutableReference>,
    limit: u16,
  ) -> Result<Self, crate::ApplicationError> {
    if limit == 0 || limit > MAX_PHASE_POOL_BATCH || capabilities.len() > MAX_FACTORY_EXECUTOR_BINDINGS {
      return Err(crate::ApplicationError::invalid());
    }
    Ok(Self {
      store,
      capabilities,
      limit,
      after: Mutex::new(None),
    })
  }
}
#[async_trait]
impl<S: FactoryPhasePoolStore> FactoryPhasePoolCoordination for FactoryPhasePoolSelector<S> {
  async fn select_once(&self, owner: &FactoryKey, at: Timestamp, expires: Timestamp) -> Result<(), StoreError> {
    let cursor = *self.after.lock().map_err(|_| StoreError::Unavailable)?;
    let policies = self.store.ready_phase_policies(cursor, self.limit).await?;
    let mut remaining = self.limit;
    let mut last_processed = None;
    for policy in &policies {
      let bytes = serde_json::to_vec(&(policy, owner, at, expires, &self.capabilities, remaining))
        .map_err(|_| StoreError::Unavailable)?;
      let selected = self
        .store
        .select_phase_ready(SelectPhasePool {
          policy_digest: *policy,
          request_id: FactoryDigest::sha256("octacity.factory.phase-selector-pass.v1", &[&bytes]),
          owner: owner.clone(),
          observed_at: at,
          expires_at: expires,
          capabilities: self.capabilities.clone(),
          limit: remaining,
        })
        .await?;
      remaining = remaining
        .checked_sub(u16::try_from(selected.len()).map_err(|_| StoreError::Unavailable)?)
        .ok_or(StoreError::Unavailable)?;
      last_processed = Some(*policy);
      if remaining == 0 {
        break;
      }
    }
    *self.after.lock().map_err(|_| StoreError::Unavailable)? =
      if policies.len() < usize::from(self.limit) && last_processed == policies.last().copied() {
        None
      } else {
        last_processed
      };
    Ok(())
  }
}
