use octacity_protocol::AgentInventory;
use octacity_server_domain::{AgentId, AgentName, AgentVersion, PoolId, PoolVersion, Timestamp};
use octacity_server_store::{AgentStatus, EnrolledAgent, StoreError};
use serde_json::Value;
use sqlx::{FromRow, types::Json};
use uuid::Uuid;

#[derive(FromRow)]
pub(crate) struct AgentRow {
  pub(crate) id: Uuid,
  pub(crate) name: String,
  pub(crate) version: i64,
  pub(crate) pool_id: Uuid,
  pub(crate) pool_version: i64,
  pub(crate) inventory: Json<Value>,
  pub(crate) state: String,
  pub(crate) last_seen_at_millis: i64,
  pub(crate) created_at_millis: i64,
  pub(crate) updated_at_millis: i64,
}

impl TryFrom<AgentRow> for EnrolledAgent {
  type Error = StoreError;

  fn try_from(row: AgentRow) -> Result<Self, Self::Error> {
    let inventory: AgentInventory = serde_json::from_value(row.inventory.0).map_err(|_| StoreError::Unavailable)?;
    inventory.validate().map_err(|_| StoreError::Unavailable)?;
    Ok(Self {
      id: AgentId::from_uuid(row.id).map_err(|_| StoreError::Unavailable)?,
      name: AgentName::new(row.name).map_err(|_| StoreError::Unavailable)?,
      version: version(row.version, AgentVersion::new)?,
      pool_id: PoolId::from_uuid(row.pool_id).map_err(|_| StoreError::Unavailable)?,
      pool_version: version(row.pool_version, PoolVersion::new)?,
      inventory,
      status: match row.state.as_str() {
        "online" => AgentStatus::Online,
        "offline" => AgentStatus::Offline,
        "draining" => AgentStatus::Draining,
        _ => return Err(StoreError::Unavailable),
      },
      last_seen_at: timestamp(row.last_seen_at_millis)?,
      created_at: timestamp(row.created_at_millis)?,
      updated_at: timestamp(row.updated_at_millis)?,
    })
  }
}

fn version<T>(
  value: i64,
  create: impl FnOnce(u64) -> Result<T, octacity_server_domain::DomainValueError>,
) -> Result<T, StoreError> {
  create(u64::try_from(value).map_err(|_| StoreError::Unavailable)?).map_err(|_| StoreError::Unavailable)
}

fn timestamp(value: i64) -> Result<Timestamp, StoreError> {
  Timestamp::from_unix_millis(value).map_err(|_| StoreError::Unavailable)
}
