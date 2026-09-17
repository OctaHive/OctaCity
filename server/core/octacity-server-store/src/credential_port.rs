use async_trait::async_trait;

use crate::{
  AgentRegistrationOutcome, AuthenticateAgentRegistration, AuthenticatedAgentRegistration, IssueAgentEnrollment,
  IssueAgentEnrollmentOutcome, MutationDisposition, RegisterAgent, RevokeAgentCredential, StoreError,
};

/// Backend-neutral deep module for the complete Agent credential lifecycle.
///
/// Mutation methods are atomic. Exact replays return their original logical
/// outcome; consumed, expired, revoked, or superseded credentials all use one
/// non-disclosing rejection.
#[async_trait]
pub trait AgentCredentialStore: Send + Sync {
  /// Issues one short-lived enrollment credential bound to a Pool and platform policy.
  async fn issue_agent_enrollment(
    &self,
    request: IssueAgentEnrollment,
  ) -> Result<IssueAgentEnrollmentOutcome, StoreError>;

  /// Consumes enrollment or current registration authority and creates a fresh epoch.
  async fn register_agent(&self, request: RegisterAgent) -> Result<AgentRegistrationOutcome, StoreError>;

  /// Authenticates one unexpired, unrevoked, current registration credential.
  async fn authenticate_agent_registration(
    &self,
    request: AuthenticateAgentRegistration,
  ) -> Result<AuthenticatedAgentRegistration, StoreError>;

  /// Revokes one enrollment or registration credential idempotently.
  async fn revoke_agent_credential(&self, request: RevokeAgentCredential) -> Result<MutationDisposition, StoreError>;
}
