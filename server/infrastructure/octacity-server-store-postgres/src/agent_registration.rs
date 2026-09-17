use octacity_server_domain::{
  AgentId, AgentName, EntityKind, PoolId, PoolVersion, RegistrationCredentialId, Timestamp,
};
use octacity_server_store::{
  AgentRegistrationOutcome, AgentRegistrationProof, CredentialDigest, MutationDisposition, RegisterAgent,
  RegistrationEpoch, StoreError, StoreOperation,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction, types::Json};
use uuid::Uuid;

use crate::{
  database::{classify, number, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};

pub(crate) async fn execute(pool: &PgPool, request: RegisterAgent) -> Result<AgentRegistrationOutcome, StoreError> {
  request.validate()?;
  let fingerprint = RequestFingerprint::from(&request);
  let identity = MutationIdentity::new(
    MutationKind::RegisterAgent,
    request.credential_id.to_string(),
    request.registered_at,
    EntityKind::AgentRegistration,
    &fingerprint,
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay(outcome),
  };

  let authority = match &request.proof {
    AgentRegistrationProof::Enrollment {
      credential_id,
      credential,
    } => {
      let enrollment = lock_enrollment(&mut transaction, credential_id.as_uuid()).await?;
      if enrollment.consumed
        || enrollment.revoked
        || enrollment.expires_at <= request.registered_at.unix_millis()
        || !digest(&enrollment.credential_hash)?.matches(credential.digest())
        || !platform_matches(
          enrollment.expected_operating_system.as_deref(),
          enrollment.expected_architecture.as_deref(),
          &request,
        )
      {
        return Err(StoreError::CredentialRejected);
      }
      let existing_agent = lock_agent(&mut transaction, request.agent_id.as_uuid()).await?;
      let (epoch, create_agent) = if let Some(agent) = existing_agent {
        if agent.name != request.agent_name.as_str()
          || agent.pool_id != enrollment.pool_id
          || agent.pool_version != enrollment.pool_version
          || agent.platform_operating_system.as_deref() != Some(request.platform.operating_system())
          || agent.platform_architecture.as_deref() != Some(request.platform.architecture())
        {
          return Err(StoreError::CredentialRejected);
        }
        let greatest: i64 =
          sqlx::query_scalar("SELECT COALESCE(MAX(epoch), 0) FROM agent_registrations WHERE agent_id = $1")
            .bind(request.agent_id.as_uuid())
            .fetch_one(&mut *transaction)
            .await
            .map_err(unavailable)?;
        let greatest = u64::try_from(greatest).map_err(|_| StoreError::Unavailable)?;
        sqlx::query(
          "UPDATE agent_registrations SET revoked_at = to_timestamp($1::double precision / 1000.0) \
           WHERE agent_id = $2 AND revoked_at IS NULL",
        )
        .bind(request.registered_at.unix_millis())
        .bind(request.agent_id.as_uuid())
        .execute(&mut *transaction)
        .await
        .map_err(unavailable)?;
        (
          RegistrationEpoch::new(greatest.checked_add(1).ok_or(StoreError::Unavailable)?)
            .map_err(|_| StoreError::Unavailable)?,
          false,
        )
      } else {
        (
          RegistrationEpoch::new(1).map_err(|source| StoreError::InvalidInput {
            operation: StoreOperation::RegisterAgent,
            source,
          })?,
          true,
        )
      };
      RegistrationAuthority {
        pool_id: PoolId::from_uuid(enrollment.pool_id).map_err(|_| StoreError::Unavailable)?,
        pool_version: pool_version(enrollment.pool_version)?,
        epoch,
        consumed_enrollment: Some(*credential_id),
        create_agent,
      }
    }
    AgentRegistrationProof::Registration {
      credential_id,
      epoch,
      credential,
    } => {
      let previous = lock_registration(&mut transaction, credential_id.as_uuid()).await?;
      if previous.agent_id != request.agent_id.as_uuid()
        || previous.epoch != number(epoch.get(), StoreOperation::RegisterAgent)?
        || previous.revoked
        || previous.expires_at <= request.registered_at.unix_millis()
        || !digest(&previous.credential_hash)?.matches(credential.digest())
        || previous.agent_name != request.agent_name.as_str()
        || previous.platform_operating_system.as_deref() != Some(request.platform.operating_system())
        || previous.platform_architecture.as_deref() != Some(request.platform.architecture())
      {
        return Err(StoreError::CredentialRejected);
      }
      let next = epoch.get().checked_add(1).ok_or(StoreError::Unavailable)?;
      sqlx::query(
        "UPDATE agent_registrations SET revoked_at = to_timestamp($1::double precision / 1000.0) WHERE id = $2",
      )
      .bind(request.registered_at.unix_millis())
      .bind(credential_id.as_uuid())
      .execute(&mut *transaction)
      .await
      .map_err(unavailable)?;
      RegistrationAuthority {
        pool_id: PoolId::from_uuid(previous.pool_id).map_err(|_| StoreError::Unavailable)?,
        pool_version: pool_version(previous.pool_version)?,
        epoch: RegistrationEpoch::new(next).map_err(|source| StoreError::InvalidInput {
          operation: StoreOperation::RegisterAgent,
          source,
        })?,
        consumed_enrollment: None,
        create_agent: false,
      }
    }
  };

  persist_agent_and_registration(&mut transaction, &request, authority).await?;
  let outcome = AgentRegistrationOutcome {
    disposition: MutationDisposition::Applied,
    credential_id: request.credential_id,
    agent_id: request.agent_id,
    registration_epoch: authority.epoch,
    pool_id: authority.pool_id,
    pool_version: authority.pool_version,
    expires_at: request.expires_at,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    MutationFacts {
      actor_kind: "agent",
      actor_identity: Some(request.agent_id.to_string()),
      target_identity: request.credential_id.to_string(),
      safe_metadata: json!({
        "agent_id": request.agent_id,
        "registration_epoch": authority.epoch,
        "pool_id": authority.pool_id,
        "pool_version": authority.pool_version,
        "expires_at": request.expires_at,
      }),
      outbox_payload: json!({
        "agent_id": request.agent_id,
        "registration_id": request.credential_id,
        "registration_epoch": authority.epoch,
        "pool_id": authority.pool_id,
      }),
    },
    encode_outcome(&StoredOutcome::from(&outcome))?,
  )
  .await?;
  Ok(outcome)
}

async fn persist_agent_and_registration(
  transaction: &mut Transaction<'_, Postgres>,
  request: &RegisterAgent,
  authority: RegistrationAuthority,
) -> Result<(), StoreError> {
  let pool_version = number(authority.pool_version.get(), StoreOperation::RegisterAgent)?;
  if authority.create_agent {
    sqlx::query(
      "INSERT INTO agents \
         (id, name, pool_id, pool_version, state, inventory, version, created_at, updated_at, \
          platform_operating_system, platform_architecture) \
       VALUES ($1, $2, $3, $4, 'online', $5, 1, to_timestamp($6::double precision / 1000.0), \
               to_timestamp($6::double precision / 1000.0), $7, $8)",
    )
    .bind(request.agent_id.as_uuid())
    .bind(request.agent_name.as_str())
    .bind(authority.pool_id.as_uuid())
    .bind(pool_version)
    .bind(Json(request.inventory.clone()))
    .bind(request.registered_at.unix_millis())
    .bind(request.platform.operating_system())
    .bind(request.platform.architecture())
    .execute(&mut **transaction)
    .await
    .map_err(|error| classify(error, EntityKind::Agent))?;
  } else {
    sqlx::query(
      "UPDATE agents SET inventory = $1, state = 'online', version = version + 1, \
         updated_at = to_timestamp($2::double precision / 1000.0) WHERE id = $3",
    )
    .bind(Json(request.inventory.clone()))
    .bind(request.registered_at.unix_millis())
    .bind(request.agent_id.as_uuid())
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
  }

  sqlx::query(
    "INSERT INTO agent_registrations \
       (id, agent_id, epoch, credential_hash, inventory, registered_at, expires_at) \
     VALUES ($1, $2, $3, $4, $5, to_timestamp($6::double precision / 1000.0), \
             to_timestamp($7::double precision / 1000.0))",
  )
  .bind(request.credential_id.as_uuid())
  .bind(request.agent_id.as_uuid())
  .bind(number(authority.epoch.get(), StoreOperation::RegisterAgent)?)
  .bind(request.credential.digest().as_bytes().as_slice())
  .bind(Json(request.inventory.clone()))
  .bind(request.registered_at.unix_millis())
  .bind(request.expires_at.unix_millis())
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::AgentRegistration))?;

  if let Some(enrollment_id) = authority.consumed_enrollment {
    let updated = sqlx::query(
      "UPDATE agent_enrollment_credentials SET consumed_registration_id = $1, \
         consumed_at = to_timestamp($2::double precision / 1000.0) \
       WHERE id = $3 AND consumed_registration_id IS NULL AND revoked_at IS NULL",
    )
    .bind(request.credential_id.as_uuid())
    .bind(request.registered_at.unix_millis())
    .bind(enrollment_id.as_uuid())
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
    if updated.rows_affected() != 1 {
      return Err(StoreError::CredentialRejected);
    }
  }
  Ok(())
}

async fn lock_enrollment(
  transaction: &mut Transaction<'_, Postgres>,
  credential_id: Uuid,
) -> Result<EnrollmentRow, StoreError> {
  sqlx::query_as(
    "SELECT credential_hash, pool_id, pool_version, expected_operating_system, expected_architecture, \
       consumed_registration_id IS NOT NULL AS consumed, revoked_at IS NOT NULL AS revoked, \
       (extract(epoch FROM expires_at) * 1000)::bigint AS expires_at \
     FROM agent_enrollment_credentials WHERE id = $1 FOR UPDATE",
  )
  .bind(credential_id)
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::CredentialRejected)
}

async fn lock_registration(
  transaction: &mut Transaction<'_, Postgres>,
  credential_id: Uuid,
) -> Result<RegistrationRow, StoreError> {
  sqlx::query_as(
    "SELECT registration.agent_id, registration.epoch, registration.credential_hash, \
       registration.revoked_at IS NOT NULL AS revoked, \
       (extract(epoch FROM registration.expires_at) * 1000)::bigint AS expires_at, \
       agent.name AS agent_name, agent.pool_id, agent.pool_version, \
       agent.platform_operating_system, agent.platform_architecture \
     FROM agent_registrations AS registration \
     JOIN agents AS agent ON agent.id = registration.agent_id \
     WHERE registration.id = $1 FOR UPDATE OF registration, agent",
  )
  .bind(credential_id)
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::CredentialRejected)
}

async fn lock_agent(
  transaction: &mut Transaction<'_, Postgres>,
  agent_id: Uuid,
) -> Result<Option<AgentRow>, StoreError> {
  sqlx::query_as(
    "SELECT name, pool_id, pool_version, platform_operating_system, platform_architecture \
     FROM agents WHERE id = $1 FOR UPDATE",
  )
  .bind(agent_id)
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)
}

fn platform_matches(expected_os: Option<&str>, expected_arch: Option<&str>, request: &RegisterAgent) -> bool {
  match (expected_os, expected_arch) {
    (None, None) => true,
    (Some(os), Some(architecture)) => {
      os == request.platform.operating_system() && architecture == request.platform.architecture()
    }
    _ => false,
  }
}

fn digest(bytes: &[u8]) -> Result<CredentialDigest, StoreError> {
  let bytes: [u8; 32] = bytes.try_into().map_err(|_| StoreError::Unavailable)?;
  Ok(CredentialDigest::from_bytes(bytes))
}

fn pool_version(value: i64) -> Result<PoolVersion, StoreError> {
  let value = u64::try_from(value).map_err(|_| StoreError::Unavailable)?;
  PoolVersion::new(value).map_err(|_| StoreError::Unavailable)
}

fn replay(value: Value) -> Result<AgentRegistrationOutcome, StoreError> {
  let stored: StoredOutcome = decode_outcome(value)?;
  Ok(AgentRegistrationOutcome {
    disposition: MutationDisposition::Replayed,
    credential_id: stored.credential_id,
    agent_id: stored.agent_id,
    registration_epoch: stored.registration_epoch,
    pool_id: stored.pool_id,
    pool_version: stored.pool_version,
    expires_at: stored.expires_at,
  })
}

#[derive(Clone, Copy)]
struct RegistrationAuthority {
  pool_id: PoolId,
  pool_version: PoolVersion,
  epoch: RegistrationEpoch,
  consumed_enrollment: Option<octacity_server_domain::EnrollmentCredentialId>,
  create_agent: bool,
}

#[derive(sqlx::FromRow)]
struct EnrollmentRow {
  credential_hash: Vec<u8>,
  pool_id: Uuid,
  pool_version: i64,
  expected_operating_system: Option<String>,
  expected_architecture: Option<String>,
  consumed: bool,
  revoked: bool,
  expires_at: i64,
}

#[derive(sqlx::FromRow)]
struct RegistrationRow {
  agent_id: Uuid,
  epoch: i64,
  credential_hash: Vec<u8>,
  revoked: bool,
  expires_at: i64,
  agent_name: String,
  pool_id: Uuid,
  pool_version: i64,
  platform_operating_system: Option<String>,
  platform_architecture: Option<String>,
}

#[derive(sqlx::FromRow)]
struct AgentRow {
  name: String,
  pool_id: Uuid,
  pool_version: i64,
  platform_operating_system: Option<String>,
  platform_architecture: Option<String>,
}

#[derive(Serialize)]
struct RequestFingerprint {
  credential_hash: [u8; 32],
  agent_id: AgentId,
  agent_name: AgentName,
  proof: ProofFingerprint,
  platform: octacity_server_store::AgentPlatform,
  inventory: Value,
  expires_at: Timestamp,
}

impl From<&RegisterAgent> for RequestFingerprint {
  fn from(request: &RegisterAgent) -> Self {
    let proof = match &request.proof {
      AgentRegistrationProof::Enrollment {
        credential_id,
        credential,
      } => ProofFingerprint::Enrollment {
        credential_id: *credential_id,
        credential_hash: credential.digest().as_bytes(),
      },
      AgentRegistrationProof::Registration {
        credential_id,
        epoch,
        credential,
      } => ProofFingerprint::Registration {
        credential_id: *credential_id,
        epoch: *epoch,
        credential_hash: credential.digest().as_bytes(),
      },
    };
    Self {
      credential_hash: request.credential.digest().as_bytes(),
      agent_id: request.agent_id,
      agent_name: request.agent_name.clone(),
      proof,
      platform: request.platform.clone(),
      inventory: request.inventory.clone(),
      expires_at: request.expires_at,
    }
  }
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
enum ProofFingerprint {
  Enrollment {
    credential_id: octacity_server_domain::EnrollmentCredentialId,
    credential_hash: [u8; 32],
  },
  Registration {
    credential_id: RegistrationCredentialId,
    epoch: RegistrationEpoch,
    credential_hash: [u8; 32],
  },
}

#[derive(Deserialize, Serialize)]
struct StoredOutcome {
  credential_id: RegistrationCredentialId,
  agent_id: AgentId,
  registration_epoch: RegistrationEpoch,
  pool_id: PoolId,
  pool_version: PoolVersion,
  expires_at: Timestamp,
}

impl From<&AgentRegistrationOutcome> for StoredOutcome {
  fn from(value: &AgentRegistrationOutcome) -> Self {
    Self {
      credential_id: value.credential_id,
      agent_id: value.agent_id,
      registration_epoch: value.registration_epoch,
      pool_id: value.pool_id,
      pool_version: value.pool_version,
      expires_at: value.expires_at,
    }
  }
}
