use std::sync::Arc;

use async_trait::async_trait;
use octacity_protocol::{AgentCredentialKind, AgentCredentialToken};
use octacity_server_domain::{EnrollmentCredentialId, PoolId, PoolVersion, Timestamp};
use octacity_server_store::{
  AgentCredentialStore, CredentialSecret, ExpectedAgentPlatform, IssueAgentEnrollment,
  MutationDisposition as StoreMutationDisposition,
};

use crate::{ApplicationError, Command, CommandHandler, MutationDisposition};

/// Issues one replay-safe single-use Agent enrollment credential.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssueAgentEnrollmentCommand {
  /// Full enrollment token; debug formatting always redacts its secret.
  pub credential: AgentCredentialToken,
  /// Pool selected for the Agent's initial registration.
  pub pool_id: PoolId,
  /// Exact Pool policy version captured by enrollment.
  pub pool_version: PoolVersion,
  /// Optional exact platform restriction.
  pub expected_platform: ExpectedAgentPlatform,
  /// Authoritative issue time.
  pub issued_at: Timestamp,
  /// Exclusive expiry.
  pub expires_at: Timestamp,
}

impl Command for IssueAgentEnrollmentCommand {
  type Outcome = IssueAgentEnrollmentCommandOutcome;
}

/// One-time management response containing the credential an Agent can consume.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssueAgentEnrollmentCommandOutcome {
  /// Whether the durable issuance applied or replayed.
  pub disposition: MutationDisposition,
  /// Enrollment token whose secret is never persisted by the server.
  pub credential: AgentCredentialToken,
  /// Bound Pool identity.
  pub pool_id: PoolId,
  /// Bound Pool version.
  pub pool_version: PoolVersion,
  /// Exclusive expiry.
  pub expires_at: Timestamp,
}

/// Store-backed Agent enrollment command handler.
pub struct AgentEnrollmentHandler<S> {
  store: Arc<S>,
}

impl<S> AgentEnrollmentHandler<S> {
  /// Creates the handler from the credential lifecycle port.
  pub fn new(store: Arc<S>) -> Self {
    Self { store }
  }
}

#[async_trait]
impl<S> CommandHandler<IssueAgentEnrollmentCommand> for AgentEnrollmentHandler<S>
where
  S: AgentCredentialStore + 'static,
{
  type Error = ApplicationError;

  async fn handle_command(
    &self,
    command: IssueAgentEnrollmentCommand,
  ) -> Result<IssueAgentEnrollmentCommandOutcome, Self::Error> {
    if command.credential.kind() != AgentCredentialKind::Enrollment {
      return Err(ApplicationError::invalid());
    }
    let credential_id = command
      .credential
      .credential_id()
      .parse::<EnrollmentCredentialId>()
      .map_err(|_| ApplicationError::invalid())?;
    let outcome = self
      .store
      .issue_agent_enrollment(IssueAgentEnrollment::new(
        credential_id,
        CredentialSecret::from_bytes(*command.credential.secret()),
        command.pool_id,
        command.pool_version,
        command.expected_platform,
        command.issued_at,
        command.expires_at,
      )?)
      .await?;
    Ok(IssueAgentEnrollmentCommandOutcome {
      disposition: match outcome.disposition {
        StoreMutationDisposition::Applied => MutationDisposition::Applied,
        StoreMutationDisposition::Replayed => MutationDisposition::Replayed,
      },
      credential: command.credential,
      pool_id: outcome.pool_id,
      pool_version: outcome.pool_version,
      expires_at: outcome.expires_at,
    })
  }
}
