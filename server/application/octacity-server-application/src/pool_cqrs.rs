use std::sync::Arc;

use async_trait::async_trait;
use octacity_server_domain::{PoolId, PoolName, PoolVersion, Timestamp};
use octacity_server_scheduler::PoolDrainState;
use octacity_server_store::{
  AgentPoolDefinition, AgentPoolStore, CreateAgentPool, DeleteAgentPool, IdempotencyKey, ListAgentPools,
  PoolAdmissionPolicy, PublishAgentPoolVersion,
};
use serde::{Deserialize, Serialize};

use crate::{ApplicationError, Command, CommandHandler, CommandTransaction, MutationDisposition, Query, QueryHandler};

/// Creates one static Agent Pool and its initial version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateAgentPoolCommand {
  /// Stable Pool identity selected once by the application caller.
  pub id: PoolId,
  /// Stable operator-facing name.
  pub name: PoolName,
  /// Initial versioned policy.
  pub definition: AgentPoolDefinition,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

/// Publishes exactly the next Agent Pool version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishAgentPoolVersionCommand {
  /// Stable Pool identity.
  pub id: PoolId,
  /// Version that must still be current.
  pub expected_current_version: PoolVersion,
  /// Replacement versioned policy.
  pub definition: AgentPoolDefinition,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

/// Deletes one unreferenced Agent Pool.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteAgentPoolCommand {
  /// Stable Pool identity.
  pub id: PoolId,
  /// Version that must still be current.
  pub expected_current_version: PoolVersion,
  /// Stable replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative deletion time.
  pub deleted_at: Timestamp,
}

/// Reads one exact immutable Agent Pool version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GetAgentPoolQuery {
  /// Stable Pool identity.
  pub pool_id: PoolId,
  /// Exact immutable version.
  pub version: PoolVersion,
}

impl Query for GetAgentPoolQuery {
  type Outcome = AgentPoolProjection;
}

/// Lists current Agent Pool versions using deterministic pagination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListAgentPoolsQuery {
  /// Exclusive stable-identity cursor.
  pub after: Option<PoolId>,
  /// Positive bounded page size.
  pub limit: u16,
}

impl Query for ListAgentPoolsQuery {
  type Outcome = AgentPoolPageProjection;
}

/// Safe application projection of one exact Agent Pool version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentPoolProjection {
  /// Stable Pool identity.
  pub id: PoolId,
  /// Stable operator-facing name.
  pub name: String,
  /// Exact immutable version.
  pub version: PoolVersion,
  /// Whether the Pool is enabled.
  pub enabled: bool,
  /// Current drain lifecycle in this version.
  pub drain_state: AgentPoolDrainStateProjection,
  /// Agent enrollment admission policy.
  pub admission_policy: AgentPoolAdmissionPolicyProjection,
  /// Maximum concurrent Jobs.
  pub concurrency_limit: u32,
  /// Deterministic placement fairness policy.
  pub fairness_policy: AgentPoolFairnessPolicyProjection,
  /// Maximum statically enrolled Agents.
  pub static_capacity_limit: u32,
  /// Authoritative publication time.
  pub published_at: Timestamp,
}

/// Transport-independent Pool fairness projection.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentPoolFairnessPolicyProjection {
  /// Global priority followed by durable FIFO order.
  PriorityFifo,
  /// Prefer configurations with fewer active Jobs at equal priority.
  ConfigurationFair,
}

/// Transport-independent Pool drain-state projection.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentPoolDrainStateProjection {
  /// New Jobs may be placed.
  Accepting,
  /// Placement stopped while active Leases finish.
  GracefulDrain,
  /// Placement stopped and active Leases must terminate.
  ForcedDrain,
  /// No active Lease remains and placement is stopped.
  Drained,
}

/// Transport-independent Pool admission-policy projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentPoolAdmissionPolicyProjection {
  /// Any valid Agent platform may enroll.
  Any,
  /// Only exact listed host platforms may enroll.
  Allowlist {
    /// Canonically ordered accepted platform pairs.
    platforms: Vec<AgentPlatformProjection>,
  },
}

/// Safe application projection of one Agent host platform.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentPlatformProjection {
  /// Canonical operating-system label.
  pub operating_system: String,
  /// Canonical architecture label.
  pub architecture: String,
}

/// Result shared by Pool create and publish commands.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentPoolCommandOutcome {
  /// Whether the mutation was newly applied or replayed.
  pub disposition: MutationDisposition,
  /// Safe immutable state committed by the original command.
  pub pool: AgentPoolProjection,
}

/// Result of deleting one Pool.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeleteAgentPoolCommandOutcome {
  /// Whether the mutation was newly applied or replayed.
  pub disposition: MutationDisposition,
  /// Stable identity of the deleted Pool.
  pub pool_id: PoolId,
}

/// One bounded page of current Agent Pool projections.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AgentPoolPageProjection {
  /// Current Pool versions ordered by stable identity.
  pub pools: Vec<AgentPoolProjection>,
  /// Cursor for the next page, when present.
  pub next_cursor: Option<PoolId>,
}

impl Command for CreateAgentPoolCommand {
  type Outcome = AgentPoolCommandOutcome;
}

impl Command for PublishAgentPoolVersionCommand {
  type Outcome = AgentPoolCommandOutcome;
}

impl Command for DeleteAgentPoolCommand {
  type Outcome = DeleteAgentPoolCommandOutcome;
}

/// Typed Agent Pool command and query handlers backed by one narrow port.
pub struct AgentPoolHandlers<S> {
  store: Arc<S>,
}

impl<S> AgentPoolHandlers<S> {
  /// Creates handlers from a backend-neutral Agent Pool port.
  pub fn new(store: Arc<S>) -> Self {
    Self { store }
  }
}

#[async_trait]
impl<S> CommandTransaction<CreateAgentPoolCommand> for S
where
  S: AgentPoolStore + 'static,
{
  type Error = ApplicationError;

  async fn commit_command(&self, command: CreateAgentPoolCommand) -> Result<AgentPoolCommandOutcome, Self::Error> {
    let outcome = self
      .create_agent_pool(CreateAgentPool {
        id: command.id,
        name: command.name,
        definition: command.definition,
        idempotency_key: command.idempotency_key,
        published_at: command.published_at,
      })
      .await?;
    Ok(AgentPoolCommandOutcome {
      disposition: outcome.disposition.into(),
      pool: outcome.pool.into(),
    })
  }
}

#[async_trait]
impl<S> CommandHandler<CreateAgentPoolCommand> for AgentPoolHandlers<S>
where
  S: AgentPoolStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(&self, command: CreateAgentPoolCommand) -> Result<AgentPoolCommandOutcome, Self::Error> {
    self.store.commit_command(command).await
  }
}

#[async_trait]
impl<S> CommandTransaction<PublishAgentPoolVersionCommand> for S
where
  S: AgentPoolStore + 'static,
{
  type Error = ApplicationError;

  async fn commit_command(
    &self,
    command: PublishAgentPoolVersionCommand,
  ) -> Result<AgentPoolCommandOutcome, Self::Error> {
    let outcome = self
      .publish_agent_pool_version(PublishAgentPoolVersion {
        id: command.id,
        expected_current_version: command.expected_current_version,
        definition: command.definition,
        idempotency_key: command.idempotency_key,
        published_at: command.published_at,
      })
      .await?;
    Ok(AgentPoolCommandOutcome {
      disposition: outcome.disposition.into(),
      pool: outcome.pool.into(),
    })
  }
}

#[async_trait]
impl<S> CommandHandler<PublishAgentPoolVersionCommand> for AgentPoolHandlers<S>
where
  S: AgentPoolStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(
    &self,
    command: PublishAgentPoolVersionCommand,
  ) -> Result<AgentPoolCommandOutcome, Self::Error> {
    self.store.commit_command(command).await
  }
}

#[async_trait]
impl<S> CommandTransaction<DeleteAgentPoolCommand> for S
where
  S: AgentPoolStore + 'static,
{
  type Error = ApplicationError;

  async fn commit_command(
    &self,
    command: DeleteAgentPoolCommand,
  ) -> Result<DeleteAgentPoolCommandOutcome, Self::Error> {
    let outcome = self
      .delete_agent_pool(DeleteAgentPool {
        id: command.id,
        expected_current_version: command.expected_current_version,
        idempotency_key: command.idempotency_key,
        deleted_at: command.deleted_at,
      })
      .await?;
    Ok(DeleteAgentPoolCommandOutcome {
      disposition: outcome.disposition.into(),
      pool_id: outcome.pool_id,
    })
  }
}

#[async_trait]
impl<S> CommandHandler<DeleteAgentPoolCommand> for AgentPoolHandlers<S>
where
  S: AgentPoolStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(
    &self,
    command: DeleteAgentPoolCommand,
  ) -> Result<DeleteAgentPoolCommandOutcome, Self::Error> {
    self.store.commit_command(command).await
  }
}

#[async_trait]
impl<S> QueryHandler<GetAgentPoolQuery> for AgentPoolHandlers<S>
where
  S: AgentPoolStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_query(&self, query: GetAgentPoolQuery) -> Result<AgentPoolProjection, Self::Error> {
    Ok(
      self
        .store
        .agent_pool_version(query.pool_id, query.version)
        .await?
        .into(),
    )
  }
}

#[async_trait]
impl<S> QueryHandler<ListAgentPoolsQuery> for AgentPoolHandlers<S>
where
  S: AgentPoolStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_query(&self, query: ListAgentPoolsQuery) -> Result<AgentPoolPageProjection, Self::Error> {
    let page = self
      .store
      .list_agent_pools(ListAgentPools::new(query.after, query.limit)?)
      .await?;
    Ok(AgentPoolPageProjection {
      pools: page.pools.into_iter().map(AgentPoolProjection::from).collect(),
      next_cursor: page.next_cursor,
    })
  }
}

impl From<octacity_server_store::PublishedAgentPool> for AgentPoolProjection {
  fn from(pool: octacity_server_store::PublishedAgentPool) -> Self {
    let drain_state = match pool.definition.drain_state {
      PoolDrainState::Accepting => AgentPoolDrainStateProjection::Accepting,
      PoolDrainState::GracefulDrain => AgentPoolDrainStateProjection::GracefulDrain,
      PoolDrainState::ForcedDrain => AgentPoolDrainStateProjection::ForcedDrain,
      PoolDrainState::Drained => AgentPoolDrainStateProjection::Drained,
    };
    let admission_policy = match pool.definition.admission_policy {
      PoolAdmissionPolicy::Any => AgentPoolAdmissionPolicyProjection::Any,
      PoolAdmissionPolicy::Allowlist { platforms } => AgentPoolAdmissionPolicyProjection::Allowlist {
        platforms: platforms
          .into_iter()
          .map(|platform| AgentPlatformProjection {
            operating_system: platform.operating_system().to_owned(),
            architecture: platform.architecture().to_owned(),
          })
          .collect(),
      },
    };
    Self {
      id: pool.id,
      name: pool.name.into_inner(),
      version: pool.version,
      enabled: pool.definition.enabled,
      drain_state,
      admission_policy,
      concurrency_limit: pool.definition.concurrency_limit,
      fairness_policy: match pool.definition.fairness_policy {
        octacity_server_store::PoolFairnessPolicy::PriorityFifo => AgentPoolFairnessPolicyProjection::PriorityFifo,
        octacity_server_store::PoolFairnessPolicy::ConfigurationFair => {
          AgentPoolFairnessPolicyProjection::ConfigurationFair
        }
      },
      static_capacity_limit: pool.definition.static_capacity_limit,
      published_at: pool.published_at,
    }
  }
}
