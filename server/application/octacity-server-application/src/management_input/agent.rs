use super::*;

impl ManagementInputFactory {
  /// Creates a typed Agent Pool-create command from a strict definition document.
  pub fn create_agent_pool(
    &self,
    id: Uuid,
    name: String,
    definition: Value,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<CreateAgentPoolCommand, ManagementInputError> {
    Ok(CreateAgentPoolCommand {
      id: identifier(id, "agent pool id")?,
      name: parse(&name, "agent pool name")?,
      definition: decode(definition, "agent pool definition")?,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      published_at: timestamp(now_unix_ms)?,
    })
  }

  /// Creates a typed Agent Pool-version command.
  pub fn publish_agent_pool(
    &self,
    id: &str,
    expected_version: u64,
    definition: Value,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<PublishAgentPoolVersionCommand, ManagementInputError> {
    Ok(PublishAgentPoolVersionCommand {
      id: parse(id, "agent pool id")?,
      expected_current_version: version(expected_version, "agent pool version")?,
      definition: decode(definition, "agent pool definition")?,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      published_at: timestamp(now_unix_ms)?,
    })
  }

  /// Creates a typed exact Agent Pool-version query.
  pub fn get_agent_pool(&self, id: &str, version_value: u64) -> Result<GetAgentPoolQuery, ManagementInputError> {
    Ok(GetAgentPoolQuery {
      pool_id: parse(id, "agent pool id")?,
      version: version(version_value, "agent pool version")?,
    })
  }

  /// Creates a typed bounded current Agent Pool-list query.
  pub fn list_agent_pools(&self, after: Option<&str>, limit: u16) -> Result<ListAgentPoolsQuery, ManagementInputError> {
    Ok(ListAgentPoolsQuery {
      after: optional_parse(after, "agent pool cursor")?,
      limit,
    })
  }

  /// Creates a typed guarded Agent Pool-delete command.
  pub fn delete_agent_pool(
    &self,
    id: &str,
    expected_version: u64,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<DeleteAgentPoolCommand, ManagementInputError> {
    Ok(DeleteAgentPoolCommand {
      id: parse(id, "agent pool id")?,
      expected_current_version: version(expected_version, "agent pool version")?,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      deleted_at: timestamp(now_unix_ms)?,
    })
  }

  /// Creates a typed exact Agent query.
  pub fn get_agent(&self, id: &str) -> Result<GetAgentQuery, ManagementInputError> {
    Ok(GetAgentQuery {
      agent_id: parse(id, "agent id")?,
    })
  }

  /// Creates a typed bounded Agent-list query.
  pub fn list_agents(&self, after: Option<&str>, limit: u16) -> Result<ListAgentsQuery, ManagementInputError> {
    Ok(ListAgentsQuery {
      after: optional_parse(after, "agent cursor")?,
      limit,
    })
  }

  /// Creates a typed guarded Agent Pool-reassignment command.
  pub fn reassign_agent_pool(
    &self,
    agent_id: &str,
    expected_version: u64,
    target_pool_id: &str,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<ReassignAgentPoolCommand, ManagementInputError> {
    Ok(ReassignAgentPoolCommand {
      agent_id: parse(agent_id, "agent id")?,
      expected_version: version(expected_version, "agent version")?,
      target_pool_id: parse(target_pool_id, "agent pool id")?,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      reassigned_at: timestamp(now_unix_ms)?,
    })
  }

  /// Creates a typed guarded Agent-drain command.
  pub fn drain_agent(
    &self,
    agent_id: &str,
    expected_version: u64,
    forced: bool,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<DrainAgentCommand, ManagementInputError> {
    Ok(DrainAgentCommand {
      agent_id: parse(agent_id, "agent id")?,
      expected_version: version(expected_version, "agent version")?,
      mode: if forced {
        AgentDrainMode::Forced
      } else {
        AgentDrainMode::Graceful
      },
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      requested_at: timestamp(now_unix_ms)?,
    })
  }
}
