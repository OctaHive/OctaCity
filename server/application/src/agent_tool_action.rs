//! Authenticated server ingress for one lease-bound blocked tool action.

use std::sync::Arc;

use async_trait::async_trait;
use octacity_protocol::{
  AgentCredentialToken, AuthorizeToolActionRequest, FactoryToolActionDecisionSourceV3, FactoryToolActionDecisionV3,
  FactoryToolActionDispositionV3, FactoryToolActionSummaryV3,
};
use octacity_server_domain::{AttemptNumber, JobId, Timestamp};
use octacity_server_store::{JobExecutionStore, LeaseAccess, StoreError};
use thiserror::Error;

use crate::{AgentOperation, AgentRegistrationError, AgentRegistrationUseCases, agent_lease};

/// Complete transport-independent input for one blocked action.
#[derive(Clone)]
pub struct AgentToolActionInput {
  /// Lease identity selected by the route.
  pub route_lease_id: String,
  /// Shared bounded request body.
  pub request: AuthorizeToolActionRequest,
  /// Current registration bearer retained by the Agent coordinator adapter.
  pub credential: AgentCredentialToken,
  /// Server-observed request arrival time.
  pub observed_at_unix_ms: i64,
}

impl std::fmt::Debug for AgentToolActionInput {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("AgentToolActionInput")
      .field("route_lease_id", &self.route_lease_id)
      .field("request", &self.request)
      .field("observed_at_unix_ms", &self.observed_at_unix_ms)
      .finish_non_exhaustive()
  }
}

/// Authoritatively verified lease and redacted proposal passed to server-owned policy.
///
/// [`AgentToolActionService`] verifies the caller-presented fence against the
/// current store state before an authorizer observes this value.
pub struct LeaseBoundToolActionInput {
  /// Current lease access bound to the authenticated registration.
  pub lease: LeaseAccess,
  /// Job bound to that lease.
  pub job_id: JobId,
  /// Attempt bound to that lease.
  pub attempt: AttemptNumber,
  /// Server-observed time used for expiry and policy checks.
  pub observed_at: Timestamp,
  /// Digest of the exact proposal retained by the Agent broker.
  pub proposal_sha256: String,
  /// Bounded secret-free proposal characteristics.
  pub proposal_summary: FactoryToolActionSummaryV3,
}

/// Server-owned policy operation invoked only after registration and fence authorization.
#[async_trait]
pub trait LeaseBoundToolActionAuthorizer: Send + Sync {
  /// Returns a digest-bound decision without exposing provider credentials.
  async fn authorize(
    &self,
    input: LeaseBoundToolActionInput,
  ) -> Result<FactoryToolActionDecisionV3, AgentToolActionError>;
}

/// Safe default used when no Factory tool policy/provider has been wired.
pub struct FailClosedToolActionAuthorizer;

#[async_trait]
impl LeaseBoundToolActionAuthorizer for FailClosedToolActionAuthorizer {
  async fn authorize(
    &self,
    input: LeaseBoundToolActionInput,
  ) -> Result<FactoryToolActionDecisionV3, AgentToolActionError> {
    FactoryToolActionDecisionV3::new(
      input.proposal_sha256,
      FactoryToolActionDispositionV3::Deny,
      FactoryToolActionDecisionSourceV3::FailClosed,
      None,
    )
    .map_err(|_| AgentToolActionError::Unavailable)
  }
}

/// Application boundary used by the Agent protocol adapter.
#[async_trait]
pub trait AgentToolActionUseCases: Send + Sync {
  /// Authorizes one exact blocked action for the current fenced lease.
  async fn authorize_tool_action(
    &self,
    input: AgentToolActionInput,
  ) -> Result<FactoryToolActionDecisionV3, AgentToolActionError>;
}

/// Registration/fence guard in front of server-owned tool policy and signals.
pub struct AgentToolActionService {
  registrations: Arc<dyn AgentRegistrationUseCases>,
  leases: Arc<dyn JobExecutionStore>,
  authorizer: Arc<dyn LeaseBoundToolActionAuthorizer>,
}

impl AgentToolActionService {
  /// Constructs the service from current-registration authority and policy.
  #[must_use]
  pub fn new(
    registrations: Arc<dyn AgentRegistrationUseCases>,
    leases: Arc<dyn JobExecutionStore>,
    authorizer: Arc<dyn LeaseBoundToolActionAuthorizer>,
  ) -> Self {
    Self {
      registrations,
      leases,
      authorizer,
    }
  }
}

#[async_trait]
impl AgentToolActionUseCases for AgentToolActionService {
  async fn authorize_tool_action(
    &self,
    input: AgentToolActionInput,
  ) -> Result<FactoryToolActionDecisionV3, AgentToolActionError> {
    input
      .request
      .validate()
      .map_err(|_| AgentToolActionError::InvalidRequest)?;
    let context = agent_lease::authorize_operation(
      self.registrations.as_ref(),
      AgentOperation::ToolAction,
      &input.route_lease_id,
      &input.request.lease,
      &input.request.registration_id,
      input.credential,
      input.observed_at_unix_ms,
    )
    .await
    .map_err(AgentToolActionError::from)?;
    self
      .leases
      .authorize_tool_action_lease(
        context.lease.access,
        context.lease.job_id,
        context.lease.attempt,
        context.observed_at,
      )
      .await
      .map_err(AgentToolActionError::from)?;
    self
      .authorizer
      .authorize(LeaseBoundToolActionInput {
        lease: context.lease.access,
        job_id: context.lease.job_id,
        attempt: context.lease.attempt,
        observed_at: context.observed_at,
        proposal_sha256: input.request.proposal_sha256,
        proposal_summary: input.request.proposal_summary,
      })
      .await
  }
}

/// Stable failures understood by the Agent tool-action adapter.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AgentToolActionError {
  /// The request body, route binding, proposal, or timestamp is invalid.
  #[error("invalid protected tool-action request")]
  InvalidRequest,
  /// The registration bearer was rejected.
  #[error("agent credential rejected")]
  CredentialRejected,
  /// The registration or Lease no longer owns the Job.
  #[error("lease is fenced")]
  Fenced,
  /// The current Lease reached its authoritative deadline.
  #[error("lease is expired")]
  Expired,
  /// The server cannot safely issue a disposition.
  #[error("protected tool-action authorization unavailable")]
  Unavailable,
}

impl From<AgentRegistrationError> for AgentToolActionError {
  fn from(value: AgentRegistrationError) -> Self {
    match value {
      AgentRegistrationError::InvalidRequest => Self::InvalidRequest,
      AgentRegistrationError::CredentialRejected => Self::CredentialRejected,
      AgentRegistrationError::Fenced => Self::Fenced,
      AgentRegistrationError::Unavailable => Self::Unavailable,
    }
  }
}

impl From<agent_lease::AuthorizationError> for AgentToolActionError {
  fn from(value: agent_lease::AuthorizationError) -> Self {
    match value {
      agent_lease::AuthorizationError::InvalidRequest => Self::InvalidRequest,
      agent_lease::AuthorizationError::Registration(error) => error.into(),
    }
  }
}

impl From<StoreError> for AgentToolActionError {
  fn from(value: StoreError) -> Self {
    match value {
      StoreError::Fenced { .. } => Self::Fenced,
      StoreError::Expired { .. } => Self::Expired,
      StoreError::InvalidInput { .. } => Self::InvalidRequest,
      StoreError::NotFound { .. }
      | StoreError::Conflict { .. }
      | StoreError::Duplicate { .. }
      | StoreError::EventGap { .. }
      | StoreError::EventsMissing { .. }
      | StoreError::CredentialRejected
      | StoreError::Unavailable => Self::Unavailable,
    }
  }
}
