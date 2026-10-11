//! Ordinary execution service for the exact persisted common node intent.

use crate::{
  CreateFactoryBuild, FactoryBuildAcceptance, FactoryExecutionClock, OrdinaryBuildApplication,
  OrdinaryBuildApplicationError,
};
use async_trait::async_trait;
use octacity_server_domain::{AttemptId, BuildId, JobId, Timestamp};
use octacity_server_factory::{FactoryClaimOwnership, FactoryDigest, FlowBuildIntent};
use octacity_server_store::{
  AcceptFactoryBuild, AcceptedFactoryBuild, AttemptRecord, BuildQueryStore, BuildRecord, FactoryBuildAcceptanceStore,
  FactoryRunStore, JobRecord, StoreError,
};
use std::sync::Arc;

/// Trusted installed compiler for a pinned capability's ordinary Job DAG.
///
/// Implementations resolve the exact selected tool/profile, narrow the Project
/// policy, compile protected execution inputs and verify any task envelope
/// against `intent` before returning signed-ready templates. Logical node names
/// do not select compilers or change the source, permissions or deadline.
#[async_trait]
pub trait FactoryBuildMaterializer: Send + Sync {
  /// Prepares immutable materialization; dispatch authority is checked again atomically.
  async fn materialize(
    &self,
    intent: &FlowBuildIntent,
    ownership: FactoryClaimOwnership,
    at: Timestamp,
  ) -> Result<AcceptFactoryBuild, OrdinaryBuildApplicationError>;
}

/// Concrete application over authoritative ordinary Build persistence.
///
/// The installed materializer owns capability-specific compilation. This service
/// selects only an already persisted intent, verifies its complete request,
/// and commits through the ordinary atomic Build/Attempt/Job boundary.
pub struct FactoryOrdinaryBuildService<S, R, C: ?Sized> {
  store: Arc<S>,
  runs: Arc<R>,
  compiler: Arc<C>,
  clock: Arc<dyn FactoryExecutionClock>,
}
impl<S, R, C: ?Sized> FactoryOrdinaryBuildService<S, R, C> {
  /// Installs explicit persistence, capability compiler and trusted clock ports.
  pub fn new(store: Arc<S>, runs: Arc<R>, compiler: Arc<C>, clock: Arc<dyn FactoryExecutionClock>) -> Self {
    Self {
      store,
      runs,
      compiler,
      clock,
    }
  }
}

#[async_trait]
impl<S, R, C> OrdinaryBuildApplication for FactoryOrdinaryBuildService<S, R, C>
where
  S: FactoryBuildAcceptanceStore,
  R: FactoryRunStore,
  C: FactoryBuildMaterializer + ?Sized,
{
  async fn factory_build_for_operation(
    &self,
    operation_id: FactoryDigest,
  ) -> Result<Option<FactoryBuildAcceptance>, OrdinaryBuildApplicationError> {
    self
      .store
      .factory_build_for_operation(operation_id)
      .await
      .map(|row| row.map(acceptance))
      .map_err(classify)
  }
  async fn create_factory_build(
    &self,
    request: CreateFactoryBuild,
  ) -> Result<FactoryBuildAcceptance, OrdinaryBuildApplicationError> {
    let snapshot = self
      .runs
      .factory_run_snapshot(request.causality.run_id)
      .await
      .map_err(classify)?;
    let intent = snapshot
      .flow
      .data
      .build_intents
      .iter()
      .find(|intent| intent.operation_id().ok() == Some(request.operation_id))
      .ok_or(OrdinaryBuildApplicationError::Invalid)?;
    let canonical = crate::factory_node_build::factory_build_request(intent, request.ownership.clone())
      .map_err(|_| OrdinaryBuildApplicationError::Invalid)?;
    if canonical != request {
      return Err(OrdinaryBuildApplicationError::Conflict);
    }
    if let Some(accepted) = self
      .store
      .factory_build_for_operation(request.operation_id)
      .await
      .map_err(classify)?
    {
      return Ok(acceptance(accepted));
    }
    let now = self
      .clock
      .now()
      .map_err(|_| OrdinaryBuildApplicationError::Unavailable)?;
    if snapshot
      .lifecycle_checkpoints
      .iter()
      .find(|checkpoint| checkpoint.id == snapshot.current.lifecycle_checkpoint_id)
      .is_none_or(|checkpoint| checkpoint.cancellation_requested)
      || snapshot.run.state().is_terminal()
      || snapshot
        .current_claim
        .as_ref()
        .is_none_or(|claim| claim.owner != *request.ownership.owner() || claim.claim != request.ownership.claim())
      || now >= intent.node().deadline()
      || intent.node().verify_observer(&request.ownership, now).is_err()
    {
      return Err(OrdinaryBuildApplicationError::Conflict);
    }
    let mut prepared = self
      .compiler
      .materialize(intent, request.ownership.clone(), now)
      .await?;
    if prepared.intent != *intent || prepared.ownership != request.ownership || prepared.accepted_at != now {
      return Err(OrdinaryBuildApplicationError::Invalid);
    }
    // Compilation may have taken time. The store fences cancellation and owner
    // replacement and checks the original deadline at the actual commit time.
    prepared.accepted_at = self
      .clock
      .now()
      .map_err(|_| OrdinaryBuildApplicationError::Unavailable)?;
    self
      .store
      .accept_factory_build(prepared)
      .await
      .map(acceptance)
      .map_err(classify)
  }
}

fn acceptance(row: AcceptedFactoryBuild) -> FactoryBuildAcceptance {
  FactoryBuildAcceptance {
    build_id: row.build_id,
    attempt_id: row.attempt_id,
    job_ids: row.job_ids,
    effective_policy_digest: row.effective_policy_digest,
  }
}
fn classify(error: StoreError) -> OrdinaryBuildApplicationError {
  match error {
    StoreError::Conflict { .. } => OrdinaryBuildApplicationError::Conflict,
    StoreError::InvalidInput { .. } | StoreError::NotFound { .. } => OrdinaryBuildApplicationError::Invalid,
    _ => OrdinaryBuildApplicationError::Unavailable,
  }
}

#[async_trait]
impl<S: BuildQueryStore, R: Send + Sync, C: Send + Sync + ?Sized> BuildQueryStore
  for FactoryOrdinaryBuildService<S, R, C>
{
  async fn build(&self, id: BuildId) -> Result<BuildRecord, StoreError> {
    self.store.build(id).await
  }
  async fn attempt(&self, id: AttemptId) -> Result<AttemptRecord, StoreError> {
    self.store.attempt(id).await
  }
  async fn job(&self, id: JobId) -> Result<JobRecord, StoreError> {
    self.store.job(id).await
  }
  async fn latest_attempt(&self, id: BuildId) -> Result<AttemptRecord, StoreError> {
    self.store.latest_attempt(id).await
  }
}

#[async_trait]
impl<S, R, C> crate::OrdinaryBuildCancellationApplication for FactoryOrdinaryBuildService<S, R, C>
where
  S: FactoryBuildAcceptanceStore + octacity_server_store::FactoryBuildCancellationStore,
  R: FactoryRunStore,
  C: FactoryBuildMaterializer + ?Sized,
{
  async fn cancel_factory_build(
    &self,
    request: crate::CancelFactoryBuild,
  ) -> Result<(), OrdinaryBuildApplicationError> {
    let observed_at = self
      .clock
      .now()
      .map_err(|_| OrdinaryBuildApplicationError::Unavailable)?;
    self
      .store
      .cancel_factory_build_execution(octacity_server_store::CancelFactoryBuildExecution {
        operation_id: request.operation_id,
        build_operation_id: request.build_operation_id,
        build_id: request.build_id,
        ownership: request.ownership,
        requested_at: request.requested_at,
        observed_at,
      })
      .await
      .map(|_| ())
      .map_err(classify)
  }
}
