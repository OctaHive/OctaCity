use std::{str::FromStr as _, sync::Arc, time::Duration};

use async_trait::async_trait;
use octacity_protocol::{
  AgentCredentialKind, AgentCredentialToken, AgentInventory, COORDINATOR_PROTOCOL_VERSION, PlatformArchitecture,
  PlatformOs, RegisterAgentRequest,
};
use octacity_server_domain::{AgentId, AgentName, PoolId, RegistrationCredentialId, Timestamp};
use octacity_server_store::{
  AgentCredentialStore, AgentPlatform, AgentRegistrationProof, AuthenticateAgentRegistration, CredentialSecret,
  FreshRegistrationCredential, RegisterAgent, RegistrationEpoch, RegistrationValidity, StoreError,
};
use thiserror::Error;
use uuid::Uuid;

const AGENT_ID_NAMESPACE: Uuid = Uuid::from_u128(0x348f_a4a5_2f19_4b45_a349_1666_f2a5_16db);
const REGISTRATION_ID_NAMESPACE: Uuid = Uuid::from_u128(0xa04a_d14c_131a_46a9_8ee4_1634_189e_c0f1);
/// Complete transport-independent input for one registration call.
#[derive(Clone, Debug)]
pub struct AgentRegistrationInput {
  /// Shared versioned wire request.
  pub request: RegisterAgentRequest,
  /// Enrollment or current-registration bearer.
  pub credential: AgentCredentialToken,
  /// Server-observed Unix time in milliseconds.
  pub observed_at_unix_ms: i64,
}

/// Successful application registration result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentRegistrationOutcome {
  /// Opaque identity of the fresh registration epoch.
  pub registration_id: String,
  /// Monotonic internal epoch used by fenced store operations.
  pub registration_epoch: RegistrationEpoch,
}

/// Agent action that must be authorized by the current registration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentOperation {
  /// Long-poll for an eligible job.
  Poll,
  /// Renew or control an active lease.
  Heartbeat,
  /// Append durable attempt events.
  AppendEvents,
  /// Submit bounded diagnostic telemetry.
  Telemetry,
  /// Begin or finish an output upload.
  Upload,
  /// Begin or revoke remote-cache authority.
  Cache,
  /// Commit terminal job state.
  Complete,
}

/// Input shared by every post-registration Agent operation.
#[derive(Clone, Debug)]
pub struct AuthorizeAgentInput {
  /// Operation being protected.
  pub operation: AgentOperation,
  /// Registration identity carried by the shared protocol DTO.
  pub registration_id: String,
  /// Bearer sent on the independently authenticated Agent ingress.
  pub credential: AgentCredentialToken,
  /// Server-observed Unix time in milliseconds.
  pub observed_at_unix_ms: i64,
}

/// Non-secret authority proven for the current registration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizedAgent {
  /// Stable server identity of the Agent.
  agent_id: AgentId,
  /// Current process registration epoch.
  registration_epoch: RegistrationEpoch,
  /// Pool selected by the current registration.
  pool_id: PoolId,
  /// Static capacity used to bound advisory heartbeat snapshots.
  host_capacity: octacity_protocol::HostCapacity,
  /// Protected operation for which this authority was minted.
  operation: AgentOperation,
}

impl AuthorizedAgent {
  #[cfg(test)]
  pub(crate) fn for_test(
    agent_id: AgentId,
    registration_epoch: RegistrationEpoch,
    pool_id: PoolId,
    host_capacity: octacity_protocol::HostCapacity,
    operation: AgentOperation,
  ) -> Self {
    Self {
      agent_id,
      registration_epoch,
      pool_id,
      host_capacity,
      operation,
    }
  }

  /// Returns the stable server identity of the authorized Agent.
  #[must_use]
  pub const fn agent_id(&self) -> AgentId {
    self.agent_id
  }

  /// Returns the current process registration epoch.
  #[must_use]
  pub const fn registration_epoch(&self) -> RegistrationEpoch {
    self.registration_epoch
  }

  /// Returns the Pool selected by the current registration.
  #[must_use]
  pub const fn pool_id(&self) -> PoolId {
    self.pool_id
  }

  /// Returns the exact operation authorized by this proof.
  #[must_use]
  pub const fn operation(&self) -> AgentOperation {
    self.operation
  }

  /// Returns the registered capacity used to validate heartbeat snapshots.
  #[must_use]
  pub const fn host_capacity(&self) -> &octacity_protocol::HostCapacity {
    &self.host_capacity
  }
}

/// Stable application failures understood by the Agent HTTP adapter.
#[derive(Debug, Error)]
pub enum AgentRegistrationError {
  /// The shared DTO or a server-controlled registration value was invalid.
  #[error("invalid agent registration request")]
  InvalidRequest,
  /// Enrollment or registration authority was invalid without disclosing why.
  #[error("agent credential rejected")]
  CredentialRejected,
  /// A superseded registration attempted a protected operation.
  #[error("agent registration is fenced")]
  Fenced,
  /// The authoritative store could not safely complete the operation.
  #[error("agent registration store unavailable")]
  Unavailable,
}

/// Application boundary used by the independently authenticated Agent adapter.
#[async_trait]
pub trait AgentRegistrationUseCases: Send + Sync {
  /// Registers a process and returns a fresh opaque epoch identity.
  async fn register(&self, input: AgentRegistrationInput) -> Result<AgentRegistrationOutcome, AgentRegistrationError>;

  /// Proves that a protected operation belongs to the current registration.
  async fn authorize(&self, input: AuthorizeAgentInput) -> Result<AuthorizedAgent, AgentRegistrationError>;
}

/// Store-backed registration and current-epoch authorization service.
pub struct AgentRegistrationService<S> {
  store: Arc<S>,
  registration_lifetime: Duration,
}

impl<S> AgentRegistrationService<S> {
  /// Creates the service with an explicit server-controlled credential lifetime.
  pub fn new(store: Arc<S>, registration_lifetime: Duration) -> Result<Self, AgentRegistrationError> {
    if registration_lifetime.is_zero() || registration_lifetime.as_millis() > i64::MAX as u128 {
      return Err(AgentRegistrationError::InvalidRequest);
    }
    Ok(Self {
      store,
      registration_lifetime,
    })
  }
}

#[async_trait]
impl<S> AgentRegistrationUseCases for AgentRegistrationService<S>
where
  S: AgentCredentialStore + 'static,
{
  async fn register(&self, input: AgentRegistrationInput) -> Result<AgentRegistrationOutcome, AgentRegistrationError> {
    input
      .request
      .validate()
      .map_err(|_| AgentRegistrationError::InvalidRequest)?;
    if input.request.protocol_version != COORDINATOR_PROTOCOL_VERSION {
      return Err(AgentRegistrationError::InvalidRequest);
    }
    let registered_at = timestamp(input.observed_at_unix_ms)?;
    let lifetime =
      i64::try_from(self.registration_lifetime.as_millis()).map_err(|_| AgentRegistrationError::InvalidRequest)?;
    let expires_at = timestamp(
      input
        .observed_at_unix_ms
        .checked_add(lifetime)
        .ok_or(AgentRegistrationError::InvalidRequest)?,
    )?;
    let agent_id = stable_agent_id(&input.request.inventory.agent_id)?;
    let agent_name =
      AgentName::new(input.request.inventory.agent_id.clone()).map_err(|_| AgentRegistrationError::InvalidRequest)?;
    let platform = platform(&input.request.inventory)?;
    let registration_id = stable_registration_id(&input.credential, &input.request.request_id)?;
    let credential = CredentialSecret::from_bytes(*input.credential.secret());
    let proof = match input.credential.kind() {
      AgentCredentialKind::Enrollment => AgentRegistrationProof::Enrollment {
        credential_id: input
          .credential
          .credential_id()
          .parse()
          .map_err(|_| AgentRegistrationError::CredentialRejected)?,
        credential: CredentialSecret::from_bytes(*input.credential.secret()),
      },
      AgentCredentialKind::Registration => {
        let credential_id = input
          .credential
          .credential_id()
          .parse()
          .map_err(|_| AgentRegistrationError::CredentialRejected)?;
        let current = self
          .store
          .authenticate_agent_registration(AuthenticateAgentRegistration {
            credential_id,
            credential: CredentialSecret::from_bytes(*input.credential.secret()),
            authenticated_at: registered_at,
          })
          .await
          .map_err(map_store_error)?;
        if current.agent_id != agent_id {
          return Err(AgentRegistrationError::CredentialRejected);
        }
        AgentRegistrationProof::Registration {
          credential_id,
          epoch: current.registration_epoch,
          credential: CredentialSecret::from_bytes(*input.credential.secret()),
        }
      }
    };
    let request = RegisterAgent::new(
      FreshRegistrationCredential {
        id: registration_id,
        secret: credential,
      },
      agent_id,
      agent_name,
      proof,
      platform,
      input.request.inventory,
      RegistrationValidity {
        registered_at,
        expires_at,
      },
    )
    .map_err(map_store_error)?;
    let outcome = self.store.register_agent(request).await.map_err(map_store_error)?;
    Ok(AgentRegistrationOutcome {
      registration_id: outcome.credential_id.to_string(),
      registration_epoch: outcome.registration_epoch,
    })
  }

  async fn authorize(&self, input: AuthorizeAgentInput) -> Result<AuthorizedAgent, AgentRegistrationError> {
    let credential_id =
      RegistrationCredentialId::from_str(&input.registration_id).map_err(|_| AgentRegistrationError::Fenced)?;
    if input.credential.kind() != AgentCredentialKind::Registration
      || input.credential.credential_id() != input.registration_id
    {
      return Err(AgentRegistrationError::Fenced);
    }
    let authenticated_at = timestamp(input.observed_at_unix_ms).map_err(|_| AgentRegistrationError::Fenced)?;
    let authenticated = self
      .store
      .authenticate_agent_registration(AuthenticateAgentRegistration {
        credential_id,
        credential: CredentialSecret::from_bytes(*input.credential.secret()),
        authenticated_at,
      })
      .await
      .map_err(|error| match error {
        StoreError::Unavailable => AgentRegistrationError::Unavailable,
        _ => AgentRegistrationError::Fenced,
      })?;
    Ok(AuthorizedAgent {
      agent_id: authenticated.agent_id,
      registration_epoch: authenticated.registration_epoch,
      pool_id: authenticated.pool_id,
      host_capacity: authenticated.host_capacity,
      operation: input.operation,
    })
  }
}

pub(crate) fn stable_agent_id(agent_id: &str) -> Result<AgentId, AgentRegistrationError> {
  AgentId::from_uuid(Uuid::new_v5(&AGENT_ID_NAMESPACE, agent_id.as_bytes()))
    .map_err(|_| AgentRegistrationError::InvalidRequest)
}

fn stable_registration_id(
  presented: &AgentCredentialToken,
  request_id: &str,
) -> Result<RegistrationCredentialId, AgentRegistrationError> {
  let kind = match presented.kind() {
    AgentCredentialKind::Enrollment => "enrollment",
    AgentCredentialKind::Registration => "registration",
  };
  let identity = format!("{kind}:{}:{request_id}", presented.credential_id());
  RegistrationCredentialId::from_uuid(Uuid::new_v5(&REGISTRATION_ID_NAMESPACE, identity.as_bytes()))
    .map_err(|_| AgentRegistrationError::InvalidRequest)
}

fn platform(inventory: &AgentInventory) -> Result<AgentPlatform, AgentRegistrationError> {
  let operating_system = match inventory.host_platform.os {
    PlatformOs::Linux => "linux",
    PlatformOs::Windows => "windows",
    PlatformOs::Macos => "macos",
  };
  let architecture = match inventory.host_platform.architecture {
    PlatformArchitecture::Amd64 => "amd64",
    PlatformArchitecture::Arm64 => "arm64",
  };
  AgentPlatform::new(operating_system, architecture).map_err(|_| AgentRegistrationError::InvalidRequest)
}

fn timestamp(value: i64) -> Result<Timestamp, AgentRegistrationError> {
  Timestamp::from_unix_millis(value).map_err(|_| AgentRegistrationError::InvalidRequest)
}

fn map_store_error(error: StoreError) -> AgentRegistrationError {
  match error {
    StoreError::CredentialRejected => AgentRegistrationError::CredentialRejected,
    StoreError::Unavailable => AgentRegistrationError::Unavailable,
    _ => AgentRegistrationError::InvalidRequest,
  }
}
