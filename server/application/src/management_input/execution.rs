use super::*;

impl ManagementInputFactory {
  /// Creates a typed bounded operator-attention scope from transport identities.
  pub fn operator_attention_scope(
    &self,
    build_ids: &[String],
    agent_ids: &[String],
    pool_ids: &[String],
    include_critical_conditions: bool,
    occurred_from_unix_ms: Option<i64>,
    occurred_through_unix_ms: Option<i64>,
  ) -> Result<crate::OperatorAttentionScopeInput, ManagementInputError> {
    let builds = build_ids
      .iter()
      .map(|id| parse(id, "build id").map(crate::OperatorAttentionTarget::Build));
    let agents = agent_ids
      .iter()
      .map(|id| parse(id, "agent id").map(crate::OperatorAttentionTarget::Agent));
    let pools = pool_ids
      .iter()
      .map(|id| parse(id, "agent pool id").map(crate::OperatorAttentionTarget::AgentPool));
    let targets = builds.chain(agents).chain(pools).collect::<Result<Vec<_>, _>>()?;
    crate::OperatorAttentionScopeInput::try_new(
      targets,
      include_critical_conditions,
      occurred_from_unix_ms,
      occurred_through_unix_ms,
    )
    .map_err(|_| ManagementInputError::Invalid("operator attention scope"))
  }

  /// Creates a typed Build read query.
  pub fn get_build(&self, id: &str) -> Result<GetBuildQuery, ManagementInputError> {
    Ok(GetBuildQuery {
      build_id: parse(id, "build id")?,
    })
  }

  /// Creates a typed bounded newest-first Build discovery query for one Project.
  pub fn list_project_builds(
    &self,
    project_id: &str,
    configuration_id: Option<&str>,
    state: Option<&str>,
    cursor: Option<&str>,
    limit: u16,
  ) -> Result<ListProjectBuildsQuery, ManagementInputError> {
    let state = state.map(parse_build_state).transpose()?;
    let after = cursor
      .map(crate::BuildPageCursor::decode)
      .transpose()
      .map_err(|_| ManagementInputError::Invalid("build cursor"))?;
    ListProjectBuildsQuery::try_new(
      parse(project_id, "project id")?,
      crate::BuildListFilter {
        configuration_id: optional_parse(configuration_id, "build configuration id")?,
        state,
      },
      after,
      limit,
    )
    .map_err(|_| ManagementInputError::Invalid("build page"))
  }

  /// Creates a typed Attempt read query.
  pub fn get_attempt(&self, id: &str) -> Result<GetAttemptQuery, ManagementInputError> {
    Ok(GetAttemptQuery {
      attempt_id: parse(id, "attempt id")?,
    })
  }

  /// Creates a typed Job diagnostic query.
  pub fn get_job(&self, id: &str) -> Result<GetJobQuery, ManagementInputError> {
    Ok(GetJobQuery {
      job_id: parse(id, "job id")?,
    })
  }

  /// Creates a typed idempotent Build cancellation command.
  pub fn cancel_build(
    &self,
    id: &str,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<CancelBuildCommand, ManagementInputError> {
    Ok(CancelBuildCommand {
      build_id: parse(id, "build id")?,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      requested_at: timestamp(now_unix_ms)?,
    })
  }

  /// Creates a typed idempotent Build retry command.
  pub fn retry_build(
    &self,
    id: &str,
    idempotency_key: &str,
    now_unix_ms: i64,
  ) -> Result<RetryBuildCommand, ManagementInputError> {
    Ok(RetryBuildCommand {
      build_id: parse(id, "build id")?,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      requested_at: timestamp(now_unix_ms)?,
    })
  }

  /// Creates a typed bounded Job-event long-poll query.
  pub fn read_job_events(
    &self,
    job_id: &str,
    after_sequence: u64,
    limit: u16,
    wait: Duration,
  ) -> Result<ReadJobEventsQuery, ManagementInputError> {
    if wait > MAX_JOB_EVENT_WAIT {
      return Err(ManagementInputError::Invalid("job event wait"));
    }
    let job_id = parse(job_id, "job id")?;
    octacity_server_store::ReadJobEvents::validate_page_size(limit)
      .map_err(|_| ManagementInputError::Invalid("job event page"))?;
    Ok(ReadJobEventsQuery {
      job_id,
      after_sequence,
      limit,
      wait,
    })
  }
}

fn parse_build_state(value: &str) -> Result<octacity_server_orchestrator::BuildState, ManagementInputError> {
  decode(Value::String(value.to_owned()), "build state")
}
