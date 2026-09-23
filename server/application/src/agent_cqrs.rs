use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use octacity_protocol::{
  CacheCapability, HostCapacity, OctaInventory, PlatformSpec, RuntimeCapability, SourcePluginInventory,
};
use octacity_server_domain::{AgentId, AgentVersion, PoolId, PoolVersion, Timestamp};
use octacity_server_store::{
  AgentDrainMode, AgentStatus, AgentStore, DrainAgent, IdempotencyKey, ListAgents, ReassignAgentPool,
};
use serde::{Deserialize, Serialize};

use crate::{ApplicationError, Command, CommandHandler, CommandTransaction, MutationDisposition, Query, QueryHandler};

/// Reads one enrolled Agent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetAgentQuery {
  /// Stable Agent identity.
  pub agent_id: AgentId,
}

impl Query for GetAgentQuery {
  type Outcome = AgentProjection;
}

/// Lists enrolled Agents using deterministic pagination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListAgentsQuery {
  /// Exclusive stable-identity cursor.
  pub after: Option<AgentId>,
  /// Positive bounded page size.
  pub limit: u16,
}

impl Query for ListAgentsQuery {
  type Outcome = AgentPageProjection;
}

/// Moves one idle Agent to another Pool.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReassignAgentPoolCommand {
  /// Stable Agent identity.
  pub agent_id: AgentId,
  /// Agent version that must still be current.
  pub expected_version: AgentVersion,
  /// Destination Pool identity.
  pub target_pool_id: PoolId,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative mutation time.
  pub reassigned_at: Timestamp,
}

impl Command for ReassignAgentPoolCommand {
  type Outcome = AgentCommandOutcome;
}

/// Prevents one Agent from receiving more Jobs and directs current work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DrainAgentCommand {
  /// Stable Agent identity.
  pub agent_id: AgentId,
  /// Agent version that must still be current.
  pub expected_version: AgentVersion,
  /// Graceful or forced handling of current work.
  pub mode: AgentDrainMode,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative mutation time.
  pub requested_at: Timestamp,
}

impl Command for DrainAgentCommand {
  type Outcome = AgentCommandOutcome;
}

/// Safe normalized management projection of one Agent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentProjection {
  /// Stable Agent identity.
  pub id: AgentId,
  /// Operator-facing name.
  pub name: String,
  /// Optimistic mutable-state version.
  pub version: AgentVersion,
  /// Exactly one current Pool identity.
  pub pool_id: PoolId,
  /// Exact Pool policy version governing the assignment.
  pub pool_version: PoolVersion,
  /// Normalized inventory excluding capacity, which is projected separately.
  pub inventory: AgentInventoryProjection,
  /// Static host capacity.
  pub capacity: AgentCapacityProjection,
  /// Normalized lifecycle state.
  pub status: AgentStatusProjection,
  /// Last authoritative Agent contact time.
  pub last_seen_at: Timestamp,
}

/// Scheduler-visible inventory that does not duplicate Agent identity or capacity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentInventoryProjection {
  /// Running Agent release version.
  pub agent_version: String,
  /// Coordinator protocols accepted by the Agent.
  pub coordinator_protocols: Vec<u16>,
  /// Operator-owned scheduler labels.
  pub labels: BTreeMap<String, String>,
  /// Host platform.
  pub host_platform: PlatformSpec,
  /// Validated execution capabilities.
  pub runtimes: Vec<RuntimeCapability>,
  /// Verified Octa installation.
  pub octa: OctaInventory,
  /// Verified source plugins.
  pub source_plugins: Vec<SourcePluginInventory>,
  /// Optional task-result cache capability.
  pub cache: Option<CacheCapability>,
}

/// Static host capacity reported at last registration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentCapacityProjection {
  /// Logical processors.
  pub logical_cpu_count: u32,
  /// Physical memory in bytes.
  pub total_memory_bytes: u64,
  /// Work filesystem capacity in bytes.
  pub work_disk_total_bytes: u64,
  /// State filesystem capacity in bytes.
  pub state_disk_total_bytes: u64,
  /// Availability of validated hypervisor isolation.
  pub virtualization_available: bool,
}

/// Stable management lifecycle values.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatusProjection {
  /// Agent is available.
  Online,
  /// Agent is unavailable.
  Offline,
  /// Agent is finishing current work.
  Draining,
}

/// One deterministic Agent page.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentPageProjection {
  /// Ordered Agents.
  pub agents: Vec<AgentProjection>,
  /// Cursor for the next page.
  pub next_cursor: Option<AgentId>,
}

/// Result of an Agent management command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentCommandOutcome {
  /// Whether the command applied or replayed.
  pub disposition: MutationDisposition,
  /// Complete committed Agent projection.
  pub agent: AgentProjection,
}

/// Application handlers backed by the Agent store port.
pub struct AgentHandlers<S> {
  store: Arc<S>,
}

impl<S> AgentHandlers<S> {
  /// Creates handlers from a backend-neutral Agent port.
  pub fn new(store: Arc<S>) -> Self {
    Self { store }
  }
}

#[async_trait]
impl<S> CommandTransaction<ReassignAgentPoolCommand> for S
where
  S: AgentStore + 'static,
{
  type Error = ApplicationError;

  async fn commit_command(&self, command: ReassignAgentPoolCommand) -> Result<AgentCommandOutcome, Self::Error> {
    let outcome = self
      .reassign_agent_pool(ReassignAgentPool {
        agent_id: command.agent_id,
        expected_version: command.expected_version,
        target_pool_id: command.target_pool_id,
        idempotency_key: command.idempotency_key,
        reassigned_at: command.reassigned_at,
      })
      .await?;
    Ok(AgentCommandOutcome {
      disposition: outcome.disposition.into(),
      agent: outcome.agent.into(),
    })
  }
}

#[async_trait]
impl<S: AgentStore + 'static> CommandHandler<ReassignAgentPoolCommand> for AgentHandlers<S> {
  type Error = ApplicationError;

  async fn handle_command(&self, command: ReassignAgentPoolCommand) -> Result<AgentCommandOutcome, Self::Error> {
    self.store.commit_command(command).await
  }
}

#[async_trait]
impl<S> CommandTransaction<DrainAgentCommand> for S
where
  S: AgentStore + 'static,
{
  type Error = ApplicationError;

  async fn commit_command(&self, command: DrainAgentCommand) -> Result<AgentCommandOutcome, Self::Error> {
    let outcome = self
      .drain_agent(DrainAgent {
        agent_id: command.agent_id,
        expected_version: command.expected_version,
        mode: command.mode,
        idempotency_key: command.idempotency_key,
        requested_at: command.requested_at,
      })
      .await?;
    Ok(AgentCommandOutcome {
      disposition: outcome.disposition.into(),
      agent: outcome.agent.into(),
    })
  }
}

#[async_trait]
impl<S: AgentStore + 'static> CommandHandler<DrainAgentCommand> for AgentHandlers<S> {
  type Error = ApplicationError;

  async fn handle_command(&self, command: DrainAgentCommand) -> Result<AgentCommandOutcome, Self::Error> {
    self.store.commit_command(command).await
  }
}

#[async_trait]
impl<S: AgentStore + 'static> QueryHandler<GetAgentQuery> for AgentHandlers<S> {
  type Error = ApplicationError;

  async fn handle_query(&self, query: GetAgentQuery) -> Result<AgentProjection, Self::Error> {
    Ok(self.store.agent(query.agent_id).await?.into())
  }
}

#[async_trait]
impl<S: AgentStore + 'static> QueryHandler<ListAgentsQuery> for AgentHandlers<S> {
  type Error = ApplicationError;

  async fn handle_query(&self, query: ListAgentsQuery) -> Result<AgentPageProjection, Self::Error> {
    let page = self
      .store
      .list_agents(ListAgents::new(query.after, query.limit)?)
      .await?;
    Ok(AgentPageProjection {
      agents: page.agents.into_iter().map(Into::into).collect(),
      next_cursor: page.next_cursor,
    })
  }
}

impl From<octacity_server_store::EnrolledAgent> for AgentProjection {
  fn from(agent: octacity_server_store::EnrolledAgent) -> Self {
    let capacity = capacity(agent.inventory.host_capacity.clone());
    let inventory = AgentInventoryProjection {
      agent_version: agent.inventory.agent_version,
      coordinator_protocols: agent.inventory.coordinator_protocols,
      labels: agent.inventory.labels,
      host_platform: agent.inventory.host_platform,
      runtimes: agent.inventory.runtimes,
      octa: agent.inventory.octa,
      source_plugins: agent.inventory.source_plugins,
      cache: agent.inventory.cache,
    };
    Self {
      id: agent.id,
      name: agent.name.into_inner(),
      version: agent.version,
      pool_id: agent.pool_id,
      pool_version: agent.pool_version,
      inventory,
      capacity,
      status: match agent.status {
        AgentStatus::Online => AgentStatusProjection::Online,
        AgentStatus::Offline => AgentStatusProjection::Offline,
        AgentStatus::Draining => AgentStatusProjection::Draining,
      },
      last_seen_at: agent.last_seen_at,
    }
  }
}

fn capacity(value: HostCapacity) -> AgentCapacityProjection {
  AgentCapacityProjection {
    logical_cpu_count: value.logical_cpu_count,
    total_memory_bytes: value.total_memory_bytes,
    work_disk_total_bytes: value.work_disk_total_bytes,
    state_disk_total_bytes: value.state_disk_total_bytes,
    virtualization_available: value.virtualization_available,
  }
}
