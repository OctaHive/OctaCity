use async_trait::async_trait;
use octacity_server_domain::{PoolId, PoolVersion};

use crate::{
  AgentPoolMutationOutcome, AgentPoolPage, CreateAgentPool, DeleteAgentPool, DeleteAgentPoolOutcome, ListAgentPools,
  PublishAgentPoolVersion, PublishedAgentPool, StoreError,
};

/// Backend-neutral atomic operations for versioned static Agent Pools.
#[async_trait]
pub trait AgentPoolStore: Send + Sync {
  /// Creates one stable Pool identity and immutable initial version.
  async fn create_agent_pool(&self, request: CreateAgentPool) -> Result<AgentPoolMutationOutcome, StoreError>;

  /// Appends exactly the next Pool version using optimistic concurrency.
  async fn publish_agent_pool_version(
    &self,
    request: PublishAgentPoolVersion,
  ) -> Result<AgentPoolMutationOutcome, StoreError>;

  /// Reads one exact immutable Pool version.
  async fn agent_pool_version(&self, pool_id: PoolId, version: PoolVersion) -> Result<PublishedAgentPool, StoreError>;

  /// Lists current Pool versions in stable identity order.
  async fn list_agent_pools(&self, request: ListAgentPools) -> Result<AgentPoolPage, StoreError>;

  /// Deletes every version only when no protected resource references the Pool.
  async fn delete_agent_pool(&self, request: DeleteAgentPool) -> Result<DeleteAgentPoolOutcome, StoreError>;
}
