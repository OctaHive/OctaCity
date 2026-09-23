use super::*;

impl ManagementInputFactory {
  /// Creates a typed Build read query.
  pub fn get_build(&self, id: &str) -> Result<GetBuildQuery, ManagementInputError> {
    Ok(GetBuildQuery {
      build_id: parse(id, "build id")?,
    })
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
    octacity_server_store::ReadJobEvents::new(job_id, after_sequence, limit)
      .map_err(|_| ManagementInputError::Invalid("job event page"))?;
    Ok(ReadJobEventsQuery {
      job_id,
      after_sequence,
      limit,
      wait,
    })
  }
}
