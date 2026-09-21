//! Deterministic in-memory Agent-credential store implementation.

use std::collections::BTreeMap;

use async_trait::async_trait;
use octacity_protocol::AgentInventory;
use octacity_server_domain::{
  AgentId, AgentName, EnrollmentCredentialId, EntityKind, PoolId, PoolVersion, RegistrationCredentialId, Timestamp,
};

use crate::{
  AgentCredentialStore, AgentCredentialTarget, AgentPlatform, AgentRegistrationOutcome, AgentRegistrationProof,
  AuthenticateAgentRegistration, AuthenticatedAgentRegistration, CredentialDigest, ExpectedAgentPlatform,
  IssueAgentEnrollment, IssueAgentEnrollmentOutcome, MutationDisposition, RegisterAgent, RegistrationEpoch,
  RevokeAgentCredential, StoreError, StoreOperation,
  testing::{InMemoryStore, RegistrationEligibility, ensure_evidence_available, record_evidence},
};

#[derive(Clone, Default)]
pub(crate) struct CredentialMemoryState {
  enrollments: BTreeMap<EnrollmentCredentialId, EnrollmentRecord>,
  registrations: BTreeMap<RegistrationCredentialId, RegistrationRecord>,
  agents: BTreeMap<AgentId, AgentRecord>,
  current_registration: BTreeMap<AgentId, RegistrationCredentialId>,
}

#[derive(Clone)]
struct EnrollmentRecord {
  fingerprint: EnrollmentFingerprint,
  outcome: IssueAgentEnrollmentOutcome,
  expected_platform: ExpectedAgentPlatform,
  digest: CredentialDigest,
  consumed_by: Option<RegistrationCredentialId>,
  revoked_at: Option<Timestamp>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EnrollmentFingerprint {
  digest: CredentialDigest,
  pool_id: PoolId,
  pool_version: PoolVersion,
  expected_platform: ExpectedAgentPlatform,
}

#[derive(Clone)]
struct RegistrationRecord {
  fingerprint: RegistrationFingerprint,
  outcome: AgentRegistrationOutcome,
  digest: CredentialDigest,
  revoked_at: Option<Timestamp>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RegistrationFingerprint {
  new_digest: CredentialDigest,
  agent_id: AgentId,
  agent_name: AgentName,
  proof: ProofFingerprint,
  platform: AgentPlatform,
  inventory: AgentInventory,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ProofFingerprint {
  Enrollment {
    credential_id: EnrollmentCredentialId,
    digest: CredentialDigest,
  },
  Registration {
    credential_id: RegistrationCredentialId,
    epoch: RegistrationEpoch,
    digest: CredentialDigest,
  },
}

#[derive(Clone)]
struct AgentRecord {
  name: AgentName,
  pool_id: PoolId,
  pool_version: PoolVersion,
  platform: AgentPlatform,
}

#[async_trait]
impl AgentCredentialStore for InMemoryStore {
  async fn issue_agent_enrollment(
    &self,
    request: IssueAgentEnrollment,
  ) -> Result<IssueAgentEnrollmentOutcome, StoreError> {
    request.validate()?;
    let mut state = self.lock()?;
    if !state.pools.contains_key(&(request.pool_id, request.pool_version)) {
      return Err(StoreError::NotFound {
        entity: EntityKind::Pool,
      });
    }
    let fingerprint = EnrollmentFingerprint {
      digest: request.credential.digest(),
      pool_id: request.pool_id,
      pool_version: request.pool_version,
      expected_platform: request.expected_platform.clone(),
    };
    if let Some(existing) = state.credentials.enrollments.get(&request.credential_id) {
      if existing.fingerprint == fingerprint {
        let mut outcome = existing.outcome.clone();
        outcome.disposition = MutationDisposition::Replayed;
        return Ok(outcome);
      }
      return Err(StoreError::Conflict {
        entity: EntityKind::AgentEnrollmentCredential,
      });
    }
    let outcome = IssueAgentEnrollmentOutcome {
      disposition: MutationDisposition::Applied,
      credential_id: request.credential_id,
      pool_id: request.pool_id,
      pool_version: request.pool_version,
      expires_at: request.expires_at,
    };
    let evidence_identity = format!("issue-agent-enrollment:{}", request.credential_id);
    ensure_evidence_available(&state, &evidence_identity)?;
    state.credentials.enrollments.insert(
      request.credential_id,
      EnrollmentRecord {
        fingerprint,
        outcome: outcome.clone(),
        expected_platform: request.expected_platform,
        digest: request.credential.digest(),
        consumed_by: None,
        revoked_at: None,
      },
    );
    record_evidence(&mut state, evidence_identity);
    Ok(outcome)
  }

  async fn register_agent(&self, request: RegisterAgent) -> Result<AgentRegistrationOutcome, StoreError> {
    request.validate()?;
    let mut state = self.lock()?;
    let fingerprint = registration_fingerprint(&request);
    if let Some(existing) = state.credentials.registrations.get(&request.credential_id) {
      if existing.fingerprint == fingerprint {
        let mut outcome = existing.outcome.clone();
        outcome.disposition = MutationDisposition::Replayed;
        return Ok(outcome);
      }
      return Err(StoreError::Conflict {
        entity: EntityKind::AgentRegistration,
      });
    }

    let plan = match &request.proof {
      AgentRegistrationProof::Enrollment {
        credential_id,
        credential,
      } => {
        let enrollment = state
          .credentials
          .enrollments
          .get(credential_id)
          .ok_or(StoreError::CredentialRejected)?;
        if enrollment.revoked_at.is_some()
          || enrollment.consumed_by.is_some()
          || enrollment.outcome.expires_at <= request.registered_at
          || !enrollment.digest.matches(credential.digest())
          || !enrollment.expected_platform.accepts(&request.platform)
        {
          return Err(StoreError::CredentialRejected);
        }
        let pool_id = enrollment.outcome.pool_id;
        let pool_version = enrollment.outcome.pool_version;
        let (next_epoch, previous_registration) = if let Some(agent) = state.credentials.agents.get(&request.agent_id) {
          if agent.name != request.agent_name
            || agent.pool_id != pool_id
            || agent.pool_version != pool_version
            || agent.platform != request.platform
          {
            return Err(StoreError::CredentialRejected);
          }
          let greatest = state
            .credentials
            .registrations
            .values()
            .filter(|registration| registration.outcome.agent_id == request.agent_id)
            .map(|registration| registration.outcome.registration_epoch.get())
            .max()
            .ok_or(StoreError::Unavailable)?;
          let current = state.credentials.current_registration.get(&request.agent_id).copied();
          if current.is_some_and(|current| !state.credentials.registrations.contains_key(&current)) {
            return Err(StoreError::Unavailable);
          }
          (greatest.checked_add(1).ok_or(StoreError::Unavailable)?, current)
        } else {
          (1, None)
        };
        RegistrationPlan {
          pool_id,
          pool_version,
          epoch: epoch(next_epoch)?,
          enrollment_to_consume: Some(*credential_id),
          registration_to_revoke: previous_registration,
        }
      }
      AgentRegistrationProof::Registration {
        credential_id,
        epoch,
        credential,
      } => {
        let current = state
          .credentials
          .current_registration
          .get(&request.agent_id)
          .copied()
          .ok_or(StoreError::CredentialRejected)?;
        let previous = state
          .credentials
          .registrations
          .get(credential_id)
          .ok_or(StoreError::CredentialRejected)?;
        let agent = state
          .credentials
          .agents
          .get(&request.agent_id)
          .ok_or(StoreError::CredentialRejected)?;
        if current != *credential_id
          || previous.outcome.agent_id != request.agent_id
          || previous.outcome.registration_epoch != *epoch
          || previous.revoked_at.is_some()
          || previous.outcome.expires_at <= request.registered_at
          || !previous.digest.matches(credential.digest())
          || agent.name != request.agent_name
          || agent.platform != request.platform
        {
          return Err(StoreError::CredentialRejected);
        }
        let next = epoch.get().checked_add(1).ok_or(StoreError::Unavailable)?;
        RegistrationPlan {
          pool_id: agent.pool_id,
          pool_version: agent.pool_version,
          epoch: self::epoch(next)?,
          enrollment_to_consume: None,
          registration_to_revoke: Some(*credential_id),
        }
      }
    };

    let evidence_identity = format!("register-agent:{}", request.credential_id);
    ensure_evidence_available(&state, &evidence_identity)?;
    if let Some(credential_id) = plan.enrollment_to_consume {
      state
        .credentials
        .enrollments
        .get_mut(&credential_id)
        .expect("registration plan validated enrollment")
        .consumed_by = Some(request.credential_id);
    }
    if let Some(credential_id) = plan.registration_to_revoke {
      let previous_identity = {
        let previous = state
          .credentials
          .registrations
          .get_mut(&credential_id)
          .expect("registration plan validated previous registration");
        previous.revoked_at = Some(request.registered_at);
        (previous.outcome.agent_id, previous.outcome.registration_epoch)
      };
      if let Some(eligibility) = state.registrations.get_mut(&previous_identity) {
        eligibility.revoked = true;
      }
    }
    state
      .credentials
      .agents
      .entry(request.agent_id)
      .or_insert_with(|| AgentRecord {
        name: request.agent_name.clone(),
        pool_id: plan.pool_id,
        pool_version: plan.pool_version,
        platform: request.platform.clone(),
      });
    let outcome = AgentRegistrationOutcome {
      disposition: MutationDisposition::Applied,
      credential_id: request.credential_id,
      agent_id: request.agent_id,
      registration_epoch: plan.epoch,
      pool_id: plan.pool_id,
      pool_version: plan.pool_version,
      expires_at: request.expires_at,
    };
    state.credentials.registrations.insert(
      request.credential_id,
      RegistrationRecord {
        fingerprint,
        outcome: outcome.clone(),
        digest: request.credential.digest(),
        revoked_at: None,
      },
    );
    state
      .credentials
      .current_registration
      .insert(request.agent_id, request.credential_id);
    state.registrations.insert(
      (request.agent_id, plan.epoch),
      RegistrationEligibility {
        pool_id: plan.pool_id,
        pool_version: plan.pool_version,
        expires_at: request.expires_at,
        revoked: false,
        inventory: Some(request.inventory.clone()),
      },
    );
    record_evidence(&mut state, evidence_identity);
    Ok(outcome)
  }

  async fn authenticate_agent_registration(
    &self,
    request: AuthenticateAgentRegistration,
  ) -> Result<AuthenticatedAgentRegistration, StoreError> {
    let state = self.lock()?;
    let registration = state
      .credentials
      .registrations
      .get(&request.credential_id)
      .ok_or(StoreError::CredentialRejected)?;
    if registration.revoked_at.is_some()
      || registration.outcome.expires_at <= request.authenticated_at
      || !registration.digest.matches(request.credential.digest())
      || state
        .credentials
        .current_registration
        .get(&registration.outcome.agent_id)
        != Some(&request.credential_id)
    {
      return Err(StoreError::CredentialRejected);
    }
    Ok(AuthenticatedAgentRegistration {
      agent_id: registration.outcome.agent_id,
      registration_epoch: registration.outcome.registration_epoch,
      pool_id: registration.outcome.pool_id,
      pool_version: registration.outcome.pool_version,
      host_capacity: state
        .registrations
        .get(&(registration.outcome.agent_id, registration.outcome.registration_epoch))
        .and_then(|registration| registration.inventory.as_ref())
        .map(|inventory| inventory.host_capacity.clone())
        .ok_or(StoreError::Unavailable)?,
      expires_at: registration.outcome.expires_at,
    })
  }

  async fn revoke_agent_credential(&self, request: RevokeAgentCredential) -> Result<MutationDisposition, StoreError> {
    let mut state = self.lock()?;
    let evidence = match request.target {
      AgentCredentialTarget::Enrollment(credential_id) => {
        let record = state
          .credentials
          .enrollments
          .get(&credential_id)
          .ok_or(StoreError::NotFound {
            entity: EntityKind::AgentEnrollmentCredential,
          })?;
        if record.revoked_at.is_some() {
          return Ok(MutationDisposition::Replayed);
        }
        format!("revoke-agent-enrollment:{credential_id}")
      }
      AgentCredentialTarget::Registration(credential_id) => {
        let record = state
          .credentials
          .registrations
          .get(&credential_id)
          .ok_or(StoreError::NotFound {
            entity: EntityKind::AgentRegistration,
          })?;
        if record.revoked_at.is_some() {
          return Ok(MutationDisposition::Replayed);
        }
        format!("revoke-agent-registration:{credential_id}")
      }
    };
    ensure_evidence_available(&state, &evidence)?;
    match request.target {
      AgentCredentialTarget::Enrollment(credential_id) => {
        state
          .credentials
          .enrollments
          .get_mut(&credential_id)
          .expect("revocation target was validated")
          .revoked_at = Some(request.revoked_at);
      }
      AgentCredentialTarget::Registration(credential_id) => {
        let record = state
          .credentials
          .registrations
          .get_mut(&credential_id)
          .expect("revocation target was validated");
        record.revoked_at = Some(request.revoked_at);
        let agent_id = record.outcome.agent_id;
        let registration_epoch = record.outcome.registration_epoch;
        state.credentials.current_registration.remove(&agent_id);
        if let Some(eligibility) = state.registrations.get_mut(&(agent_id, registration_epoch)) {
          eligibility.revoked = true;
        }
      }
    }
    record_evidence(&mut state, evidence);
    Ok(MutationDisposition::Applied)
  }
}

struct RegistrationPlan {
  pool_id: PoolId,
  pool_version: PoolVersion,
  epoch: RegistrationEpoch,
  enrollment_to_consume: Option<EnrollmentCredentialId>,
  registration_to_revoke: Option<RegistrationCredentialId>,
}

fn registration_fingerprint(request: &RegisterAgent) -> RegistrationFingerprint {
  let proof = match &request.proof {
    AgentRegistrationProof::Enrollment {
      credential_id,
      credential,
    } => ProofFingerprint::Enrollment {
      credential_id: *credential_id,
      digest: credential.digest(),
    },
    AgentRegistrationProof::Registration {
      credential_id,
      epoch,
      credential,
    } => ProofFingerprint::Registration {
      credential_id: *credential_id,
      epoch: *epoch,
      digest: credential.digest(),
    },
  };
  RegistrationFingerprint {
    new_digest: request.credential.digest(),
    agent_id: request.agent_id,
    agent_name: request.agent_name.clone(),
    proof,
    platform: request.platform.clone(),
    inventory: request.inventory.clone(),
  }
}

fn epoch(value: u64) -> Result<RegistrationEpoch, StoreError> {
  RegistrationEpoch::new(value).map_err(|source| StoreError::InvalidInput {
    operation: StoreOperation::RegisterAgent,
    source,
  })
}
