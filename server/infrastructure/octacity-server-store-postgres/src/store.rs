use async_trait::async_trait;
use octacity_server_domain::ProjectId;
use octacity_server_store::{
  AcceptTrigger, AcceptTriggerOutcome, AgentCredentialStore, AgentRegistrationOutcome, AppendJobEvents,
  AppendJobEventsOutcome, AuthenticateAgentRegistration, AuthenticatedAgentRegistration, AuthoritativeStore,
  CompletionDisposition, IssueAgentEnrollment, IssueAgentEnrollmentOutcome, JobClaim, JobClaimOutcome, JobCompletion,
  LogIndexPosition, LogIndexWorkStore, MutationDisposition, RegisterAgent, RevokeAgentCredential, StoreError,
};
use sqlx::PgPool;

/// PostgreSQL adapter for the backend-neutral authoritative store interface.
#[derive(Clone)]
pub struct PostgresStore {
  pool: PgPool,
}

impl PostgresStore {
  /// Creates an adapter backed by a migrated PostgreSQL pool.
  #[must_use]
  pub fn new(pool: PgPool) -> Self {
    Self { pool }
  }
}

#[async_trait]
impl AuthoritativeStore for PostgresStore {
  async fn accept_trigger(&self, request: AcceptTrigger) -> Result<AcceptTriggerOutcome, StoreError> {
    crate::accept_trigger::execute(&self.pool, request).await
  }

  async fn claim_ready_job(&self, request: JobClaim) -> Result<JobClaimOutcome, StoreError> {
    crate::job_claim::execute(&self.pool, request).await
  }

  async fn append_job_events(&self, request: AppendJobEvents) -> Result<AppendJobEventsOutcome, StoreError> {
    crate::job_events::execute(&self.pool, request).await
  }

  async fn complete_job(&self, request: JobCompletion) -> Result<CompletionDisposition, StoreError> {
    crate::job_completion::execute(&self.pool, request).await
  }
}

#[async_trait]
impl LogIndexWorkStore for PostgresStore {
  async fn committed_log_index_position(&self, project_id: ProjectId) -> Result<Option<LogIndexPosition>, StoreError> {
    let position: Option<i64> =
      sqlx::query_scalar("SELECT committed_through FROM log_index_project_positions WHERE project_id = $1")
        .bind(project_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(crate::database::unavailable)?;
    position
      .map(|position| {
        u64::try_from(position)
          .ok()
          .and_then(|position| LogIndexPosition::new(position).ok())
          .ok_or(StoreError::Unavailable)
      })
      .transpose()
  }
}

#[async_trait]
impl AgentCredentialStore for PostgresStore {
  async fn issue_agent_enrollment(
    &self,
    request: IssueAgentEnrollment,
  ) -> Result<IssueAgentEnrollmentOutcome, StoreError> {
    crate::agent_enrollment::execute(&self.pool, request).await
  }

  async fn register_agent(&self, request: RegisterAgent) -> Result<AgentRegistrationOutcome, StoreError> {
    crate::agent_registration::execute(&self.pool, request).await
  }

  async fn authenticate_agent_registration(
    &self,
    request: AuthenticateAgentRegistration,
  ) -> Result<AuthenticatedAgentRegistration, StoreError> {
    crate::agent_credential_auth::execute(&self.pool, request).await
  }

  async fn revoke_agent_credential(&self, request: RevokeAgentCredential) -> Result<MutationDisposition, StoreError> {
    crate::agent_credential_revocation::execute(&self.pool, request).await
  }
}
