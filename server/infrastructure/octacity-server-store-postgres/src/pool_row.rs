use octacity_server_domain::{PoolId, PoolName, PoolVersion, Timestamp};
use octacity_server_scheduler::PoolDrainState;
use octacity_server_store::{
  AgentPoolDefinition, PoolAdmissionPolicy, PoolFairnessPolicy, PublishedAgentPool, StoreError,
};
use serde_json::Value;
use sqlx::{FromRow, types::Json};
use uuid::Uuid;

#[derive(FromRow)]
pub(crate) struct PoolRow {
  id: Uuid,
  name: String,
  version: i64,
  enabled: bool,
  drain_state: String,
  admission_policy: Json<Value>,
  concurrency_limit: i64,
  fairness_policy: String,
  static_capacity_limit: i64,
  published_at_millis: i64,
}

impl TryFrom<PoolRow> for PublishedAgentPool {
  type Error = StoreError;

  fn try_from(row: PoolRow) -> Result<Self, Self::Error> {
    let drain_state = match row.drain_state.as_str() {
      "accepting" => PoolDrainState::Accepting,
      "graceful_drain" => PoolDrainState::GracefulDrain,
      "forced_drain" => PoolDrainState::ForcedDrain,
      "drained" => PoolDrainState::Drained,
      _ => return Err(StoreError::Unavailable),
    };
    let definition = AgentPoolDefinition {
      enabled: row.enabled,
      drain_state,
      admission_policy: serde_json::from_value::<PoolAdmissionPolicy>(row.admission_policy.0)
        .map_err(|_| StoreError::Unavailable)?,
      concurrency_limit: u32::try_from(row.concurrency_limit).map_err(|_| StoreError::Unavailable)?,
      fairness_policy: match row.fairness_policy.as_str() {
        "priority_fifo" => PoolFairnessPolicy::PriorityFifo,
        "configuration_fair" => PoolFairnessPolicy::ConfigurationFair,
        _ => return Err(StoreError::Unavailable),
      },
      static_capacity_limit: u32::try_from(row.static_capacity_limit).map_err(|_| StoreError::Unavailable)?,
    };
    definition.validate().map_err(|_| StoreError::Unavailable)?;
    Ok(Self {
      id: PoolId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?,
      name: PoolName::new(row.name).map_err(|_| StoreError::Unavailable)?,
      version: PoolVersion::new(u64::try_from(row.version).map_err(|_| StoreError::Unavailable)?)
        .map_err(|_| StoreError::Unavailable)?,
      definition,
      published_at: Timestamp::from_unix_millis(row.published_at_millis).map_err(|_| StoreError::Unavailable)?,
    })
  }
}
