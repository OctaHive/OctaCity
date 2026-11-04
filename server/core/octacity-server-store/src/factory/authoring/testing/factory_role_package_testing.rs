//! Memory adapter for the private package extension of ordinary input publication.
use crate::{testing::InMemoryFactoryConfigurationStore, *};
use octacity_server_domain::Timestamp;
use octacity_server_factory::{FactoryClaimOwnership, FactoryDigest, FlowBuildIntent};

pub(crate) fn verify_dispatch(
  snapshot: &FactoryRunSnapshot,
  intent: &FlowBuildIntent,
  ownership: &FactoryClaimOwnership,
  now: Timestamp,
) -> Result<(), StoreError> {
  if snapshot.run.state().is_terminal()
    || snapshot.current_claim.as_ref().is_none_or(|claim| {
      claim.owner != *ownership.owner()
        || claim.claim != ownership.claim()
        || now < claim.claim.claimed_at()
        || now >= claim.claim.expires_at()
    })
    || snapshot
      .lifecycle_checkpoints
      .iter()
      .find(|row| row.id == snapshot.current.lifecycle_checkpoint_id)
      .is_none_or(|row| row.cancellation_requested)
    || intent.node().verify_observer(ownership, now).is_err()
    || now >= intent.node().deadline()
    || !snapshot.flow.data.build_intents.contains(intent)
    || snapshot
      .flow
      .data
      .stops
      .iter()
      .any(|row| row.node_attempt_id() == intent.node().id())
    || snapshot
      .flow
      .completions
      .iter()
      .any(|row| row.node_attempt_id() == intent.node().id())
  {
    return Err(StoreError::Conflict {
      entity: octacity_server_domain::EntityKind::FactoryRun,
    });
  }
  Ok(())
}
#[async_trait::async_trait]
impl FactoryRoleInvocationStore for InMemoryFactoryConfigurationStore {
  async fn factory_role_invocation(
    &self,
    operation: FactoryDigest,
  ) -> Result<Option<RetainedFactoryRoleInvocation>, StoreError> {
    Ok(self.lock()?.role_invocations.get(&operation).cloned())
  }
  async fn retain_factory_role_invocation(
    &self,
    intent: &FlowBuildIntent,
    ownership: &FactoryClaimOwnership,
    invocation: RetainedFactoryRoleInvocation,
    now: Timestamp,
  ) -> Result<(), StoreError> {
    invocation.verify_intent(intent)?;
    let operation = intent.operation_id().map_err(|_| StoreError::Unavailable)?;
    let mut state = self.lock()?;
    if let Some(previous) = state.role_invocations.get(&operation) {
      return if previous == &invocation {
        Ok(())
      } else {
        Err(StoreError::Conflict {
          entity: octacity_server_domain::EntityKind::Build,
        })
      };
    }
    let run = state
      .factory_runs
      .get(&intent.input().factory_run_id())
      .ok_or(StoreError::Unavailable)?
      .snapshot()?;
    verify_dispatch(&run, intent, ownership, now)?;
    state.role_profiles.verify_package(&invocation.package)?;
    state.role_invocations.insert(operation, invocation);
    Ok(())
  }
}
