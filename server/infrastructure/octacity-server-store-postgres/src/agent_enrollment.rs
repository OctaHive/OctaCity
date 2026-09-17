use octacity_server_domain::{EnrollmentCredentialId, EntityKind, PoolId, PoolVersion, Timestamp};
use octacity_server_store::{
  ExpectedAgentPlatform, IssueAgentEnrollment, IssueAgentEnrollmentOutcome, MutationDisposition, StoreError,
  StoreOperation,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;

use crate::{
  database::{classify, number, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};

pub(crate) async fn execute(
  pool: &PgPool,
  request: IssueAgentEnrollment,
) -> Result<IssueAgentEnrollmentOutcome, StoreError> {
  request.validate()?;
  let digest = request.credential.digest().as_bytes();
  let fingerprint = RequestFingerprint {
    credential_hash: digest,
    pool_id: request.pool_id,
    pool_version: request.pool_version,
    expected_platform: request.expected_platform.clone(),
    expires_at: request.expires_at,
  };
  let identity = MutationIdentity::new(
    MutationKind::IssueAgentEnrollment,
    request.credential_id.to_string(),
    request.issued_at,
    EntityKind::AgentEnrollmentCredential,
    &fingerprint,
  )?;
  let pool_version = number(request.pool_version.get(), StoreOperation::IssueAgentEnrollment)?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay(outcome),
  };

  let pool_exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pools WHERE id = $1 AND version = $2)")
    .bind(request.pool_id.as_uuid())
    .bind(pool_version)
    .fetch_one(&mut *transaction)
    .await
    .map_err(unavailable)?;
  if !pool_exists {
    return Err(StoreError::NotFound {
      entity: EntityKind::Pool,
    });
  }

  let (expected_os, expected_architecture) = match &request.expected_platform {
    ExpectedAgentPlatform::Any => (None, None),
    ExpectedAgentPlatform::Exact(platform) => (Some(platform.operating_system()), Some(platform.architecture())),
  };
  sqlx::query(
    "INSERT INTO agent_enrollment_credentials \
       (id, credential_hash, pool_id, pool_version, expected_operating_system, expected_architecture, \
        issued_at, expires_at) \
     VALUES ($1, $2, $3, $4, $5, $6, to_timestamp($7::double precision / 1000.0), \
             to_timestamp($8::double precision / 1000.0))",
  )
  .bind(request.credential_id.as_uuid())
  .bind(digest.as_slice())
  .bind(request.pool_id.as_uuid())
  .bind(pool_version)
  .bind(expected_os)
  .bind(expected_architecture)
  .bind(request.issued_at.unix_millis())
  .bind(request.expires_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(|error| classify(error, EntityKind::AgentEnrollmentCredential))?;

  let outcome = IssueAgentEnrollmentOutcome {
    disposition: MutationDisposition::Applied,
    credential_id: request.credential_id,
    pool_id: request.pool_id,
    pool_version: request.pool_version,
    expires_at: request.expires_at,
  };
  let stored = StoredOutcome::from(&outcome);
  crate::mutation::commit(
    transaction,
    &identity,
    MutationFacts {
      actor_kind: "unauthenticated_management",
      actor_identity: None,
      target_identity: request.credential_id.to_string(),
      safe_metadata: json!({
        "pool_id": request.pool_id,
        "pool_version": request.pool_version,
        "expires_at": request.expires_at,
      }),
      outbox_payload: json!({
        "credential_id": request.credential_id,
        "pool_id": request.pool_id,
        "pool_version": request.pool_version,
      }),
    },
    encode_outcome(&stored)?,
  )
  .await?;
  Ok(outcome)
}

fn replay(value: serde_json::Value) -> Result<IssueAgentEnrollmentOutcome, StoreError> {
  let stored: StoredOutcome = decode_outcome(value)?;
  Ok(IssueAgentEnrollmentOutcome {
    disposition: MutationDisposition::Replayed,
    credential_id: stored.credential_id,
    pool_id: stored.pool_id,
    pool_version: stored.pool_version,
    expires_at: stored.expires_at,
  })
}

#[derive(Serialize)]
struct RequestFingerprint {
  credential_hash: [u8; 32],
  pool_id: PoolId,
  pool_version: PoolVersion,
  expected_platform: ExpectedAgentPlatform,
  expires_at: Timestamp,
}

#[derive(Deserialize, Serialize)]
struct StoredOutcome {
  credential_id: EnrollmentCredentialId,
  pool_id: PoolId,
  pool_version: PoolVersion,
  expires_at: Timestamp,
}

impl From<&IssueAgentEnrollmentOutcome> for StoredOutcome {
  fn from(value: &IssueAgentEnrollmentOutcome) -> Self {
    Self {
      credential_id: value.credential_id,
      pool_id: value.pool_id,
      pool_version: value.pool_version,
      expires_at: value.expires_at,
    }
  }
}
