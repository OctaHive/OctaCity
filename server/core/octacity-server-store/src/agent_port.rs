use async_trait::async_trait;
use octacity_server_domain::AgentId;

use crate::{
  AgentDetail, AgentPage, DrainAgent, DrainAgentOutcome, ListAgents, ManagementMutation, ReassignAgentPool,
  ReassignAgentPoolOutcome, StoreError,
};

/// Backend-neutral management operations for enrolled Agents.
#[async_trait]
pub trait AgentStore: Send + Sync {
  /// Reads one enrolled Agent and its optional current non-terminal execution.
  async fn agent_detail(&self, agent_id: AgentId) -> Result<AgentDetail, StoreError>;

  /// Lists enrolled Agents in stable identity order.
  async fn list_agents(&self, request: ListAgents) -> Result<AgentPage, StoreError>;

  /// Moves one idle compatible Agent to the destination Pool's current version.
  async fn reassign_agent_pool(
    &self,
    request: ManagementMutation<ReassignAgentPool>,
  ) -> Result<ReassignAgentPoolOutcome, StoreError>;

  /// Prevents new placement and directs any current Lease to drain or cancel.
  async fn drain_agent(&self, request: ManagementMutation<DrainAgent>) -> Result<DrainAgentOutcome, StoreError>;
}
