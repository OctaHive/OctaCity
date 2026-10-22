//! Latest bounded host observations, independent of placement and Lease authority.
use crate::{RegistrationEpoch, StoreError};
use async_trait::async_trait;
use octacity_protocol::HostSnapshot;
use octacity_server_domain::{AgentId, Timestamp};
use serde::{Deserialize, Serialize};

/// Safe latest availability; missing observation is different from zero free space.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentResources {
  /// Server receipt time of the observation.
  pub observed_at: Timestamp,
  /// Reported currently available CPU in thousandths of one logical CPU.
  pub available_cpu_millis: u64,
  /// Reported currently available memory.
  pub available_memory_bytes: u64,
  /// Bytes available on the work filesystem.
  pub work_disk_free_bytes: u64,
  /// Bytes available on the state filesystem.
  pub state_disk_free_bytes: u64,
}

/// A validated observation from one independently authenticated Agent registration.
#[derive(Clone, Debug)]
pub struct AgentResourceObservation {
  /// Stable enrolled Agent.
  pub agent_id: AgentId,
  /// Registration fence; superseded processes cannot replace current observations.
  pub registration_epoch: RegistrationEpoch,
  /// Bounded original observation; only resource measurements are retained.
  pub snapshot: HostSnapshot,
  /// Server receipt time, never supplied by the Agent.
  pub observed_at: Timestamp,
}

/// Updates only the latest safe resource view, without execution state changes.
#[async_trait]
pub trait AgentResourceStore: Send + Sync {
  /// Retains a fresh current-registration sample; stale/fenced samples are ignored.
  async fn record_agent_resources(&self, observation: AgentResourceObservation) -> Result<(), StoreError>;
}
