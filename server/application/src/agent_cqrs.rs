use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use octacity_protocol::{
  CacheCapability, ExecutionCapabilityV2, ExecutionContractRange, FactoryExecutionCapabilityV3, HostCapacity,
  OctaInventory, PlatformSpec, RuntimeCapability, SourcePluginInventory,
};
use octacity_server_domain::{
  AgentId, AgentVersion, AttemptId, BuildId, JobId, LeaseId, PoolId, PoolVersion, Timestamp,
};
use octacity_server_store::{
  AgentCurrentLeaseState, AgentDrainMode, AgentStatus, AgentStore, DrainAgent, IdempotencyKey, ListAgents,
  ReassignAgentPool,
};
use serde::{Deserialize, Serialize};

use crate::{
  ApplicationError, Command, CommandTransaction, ManagementAction, ManagementAuthorizationMapping,
  ManagementAuthorizationTarget, ManagementResource, ManagementResourceKind, ManagementResourceResult,
  MutationDisposition, Query,
  management_security::{audited_mutation, instance_resource},
};

/// Reads one enrolled Agent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetAgentQuery {
  /// Stable Agent identity.
  pub agent_id: AgentId,
}

impl Query for GetAgentQuery {
  type Outcome = AgentProjection;
}

impl ManagementAuthorizationTarget for GetAgentQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::View, ManagementResourceKind::Agent);

  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::Agent, self.agent_id)
  }
}

/// Lists enrolled Agents using deterministic pagination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListAgentsQuery {
  /// Optional exact current Pool filter.
  pub pool_id: Option<PoolId>,
  /// Exclusive stable-identity cursor.
  pub after: Option<AgentId>,
  /// Positive bounded page size.
  pub limit: u16,
}

impl Query for ListAgentsQuery {
  type Outcome = AgentPageProjection;
}

impl ManagementAuthorizationTarget for ListAgentsQuery {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::collection(ManagementAction::View, ManagementResourceKind::Agent);

  fn management_resource(&self) -> ManagementResourceResult {
    ManagementResource::collection(ManagementResourceKind::Agent)
  }
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

impl ManagementAuthorizationTarget for ReassignAgentPoolCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Update, ManagementResourceKind::Agent);

  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::Agent, self.agent_id)
  }
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

impl ManagementAuthorizationTarget for DrainAgentCommand {
  const AUTHORIZATION: ManagementAuthorizationMapping =
    ManagementAuthorizationMapping::instance(ManagementAction::Administer, ManagementResourceKind::Agent);

  fn management_resource(&self) -> ManagementResourceResult {
    instance_resource(ManagementResourceKind::Agent, self.agent_id)
  }
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
  /// Current non-terminal execution, or `None` when the Agent is idle.
  pub current_execution: Option<AgentCurrentExecutionProjection>,
}

/// Safe management projection of the execution currently owned by an Agent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentCurrentExecutionProjection {
  /// Active lease identity without fencing material.
  pub lease_id: LeaseId,
  /// Build containing the current Job.
  pub build_id: BuildId,
  /// Attempt containing the current Job.
  pub attempt_id: AttemptId,
  /// Current Job identity.
  pub job_id: JobId,
  /// Authoritative non-terminal lease state.
  pub lease_state: AgentCurrentLeaseStateProjection,
}

/// Stable non-terminal current-lease states exposed to management clients.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentCurrentLeaseStateProjection {
  /// The Agent may continue executing and renewing the lease.
  Active,
  /// The coordinator has requested cancellation.
  CancellationRequested,
  /// The coordinator has requested draining after execution stops.
  DrainRequested,
}

/// Scheduler-visible inventory that does not duplicate Agent identity or capacity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentInventoryProjection {
  /// Running Agent release version.
  pub agent_version: String,
  /// Coordinator protocols accepted by the Agent.
  pub coordinator_protocols: Vec<u16>,
  /// Inclusive signed execution-contract revisions accepted by the Agent.
  pub execution_contract: ExecutionContractRange,
  /// Operator-owned scheduler labels.
  pub labels: BTreeMap<String, String>,
  /// Host platform.
  pub host_platform: PlatformSpec,
  /// Validated execution capabilities.
  pub runtimes: Vec<RuntimeCapability>,
  /// Provider-backed current execution routes used for provider-neutral placement.
  pub executions: Vec<ExecutionCapabilityV2>,
  /// Current routes and the semantic Factory controls each can enforce.
  pub factory_executions: Vec<FactoryExecutionCapabilityV3>,
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

  async fn commit_command(
    &self,
    context: &crate::ManagementRequestContext,
    command: ReassignAgentPoolCommand,
  ) -> Result<AgentCommandOutcome, Self::Error> {
    let outcome = self
      .reassign_agent_pool(audited_mutation(
        context,
        ReassignAgentPool {
          agent_id: command.agent_id,
          expected_version: command.expected_version,
          target_pool_id: command.target_pool_id,
          idempotency_key: command.idempotency_key,
          reassigned_at: command.reassigned_at,
        },
      )?)
      .await?;
    Ok(AgentCommandOutcome {
      disposition: outcome.disposition.into(),
      agent: outcome.agent.into(),
    })
  }
}

#[async_trait]
impl<S: AgentStore + 'static> crate::ManagementCommandUseCase<ReassignAgentPoolCommand> for AgentHandlers<S> {
  type Error = ApplicationError;

  async fn execute_management_command(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    command: ReassignAgentPoolCommand,
  ) -> Result<AgentCommandOutcome, Self::Error> {
    self.store.commit_command(context, command).await
  }
}

#[async_trait]
impl<S> CommandTransaction<DrainAgentCommand> for S
where
  S: AgentStore + 'static,
{
  type Error = ApplicationError;

  async fn commit_command(
    &self,
    context: &crate::ManagementRequestContext,
    command: DrainAgentCommand,
  ) -> Result<AgentCommandOutcome, Self::Error> {
    let outcome = self
      .drain_agent(audited_mutation(
        context,
        DrainAgent {
          agent_id: command.agent_id,
          expected_version: command.expected_version,
          mode: command.mode,
          idempotency_key: command.idempotency_key,
          requested_at: command.requested_at,
        },
      )?)
      .await?;
    Ok(AgentCommandOutcome {
      disposition: outcome.disposition.into(),
      agent: octacity_server_store::AgentDetail {
        agent: outcome.agent,
        current_execution: outcome.current_execution,
      }
      .into(),
    })
  }
}

#[async_trait]
impl<S: AgentStore + 'static> crate::ManagementCommandUseCase<DrainAgentCommand> for AgentHandlers<S> {
  type Error = ApplicationError;

  async fn execute_management_command(
    &self,
    context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    command: DrainAgentCommand,
  ) -> Result<AgentCommandOutcome, Self::Error> {
    self.store.commit_command(context, command).await
  }
}

#[async_trait]
impl<S: AgentStore + 'static> crate::ManagementQueryUseCase<GetAgentQuery> for AgentHandlers<S> {
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &crate::ManagementRequestContext,
    _grant: &crate::ManagementAuthorizationGrant,
    query: GetAgentQuery,
  ) -> Result<AgentProjection, Self::Error> {
    Ok(self.store.agent_detail(query.agent_id).await?.into())
  }
}

#[async_trait]
impl<S: AgentStore + 'static> crate::ManagementQueryUseCase<ListAgentsQuery> for AgentHandlers<S> {
  type Error = ApplicationError;

  async fn execute_management_query(
    &self,
    _context: &crate::ManagementRequestContext,
    grant: &crate::ManagementAuthorizationGrant,
    query: ListAgentsQuery,
  ) -> Result<AgentPageProjection, Self::Error> {
    let page = self
      .store
      .list_agents(
        ListAgents::new(
          query.after,
          query.limit,
          grant
            .visibility_for::<ListAgentsQuery>()
            .map_err(|_| ApplicationError::InvalidAuthorizationVisibility)?,
        )?
        .for_pool(query.pool_id),
      )
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
      execution_contract: agent.inventory.execution_contract,
      labels: agent.inventory.labels,
      host_platform: agent.inventory.host_platform,
      runtimes: agent.inventory.runtimes,
      executions: agent.inventory.executions,
      factory_executions: agent.inventory.factory_executions,
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
      current_execution: None,
    }
  }
}

impl From<octacity_server_store::AgentDetail> for AgentProjection {
  fn from(detail: octacity_server_store::AgentDetail) -> Self {
    let mut projection = Self::from(detail.agent);
    projection.current_execution = detail
      .current_execution
      .map(|execution| AgentCurrentExecutionProjection {
        lease_id: execution.lease_id,
        build_id: execution.build_id,
        attempt_id: execution.attempt_id,
        job_id: execution.job_id,
        lease_state: match execution.lease_state {
          AgentCurrentLeaseState::Active => AgentCurrentLeaseStateProjection::Active,
          AgentCurrentLeaseState::CancellationRequested => AgentCurrentLeaseStateProjection::CancellationRequested,
          AgentCurrentLeaseState::DrainRequested => AgentCurrentLeaseStateProjection::DrainRequested,
        },
      });
    projection
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
