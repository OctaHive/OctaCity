use octacity_server_domain::EntityKind;
use octacity_server_store::{AgentCredentialTarget, MutationDisposition, RevokeAgentCredential, StoreError};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;

use crate::{
  database::unavailable,
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};

pub(crate) async fn execute(pool: &PgPool, request: RevokeAgentCredential) -> Result<MutationDisposition, StoreError> {
  let (kind, key, entity, target_identity) = match request.target {
    AgentCredentialTarget::Enrollment(credential_id) => (
      MutationKind::RevokeAgentEnrollment,
      credential_id.to_string(),
      EntityKind::AgentEnrollmentCredential,
      credential_id.to_string(),
    ),
    AgentCredentialTarget::Registration(credential_id) => (
      MutationKind::RevokeAgentRegistration,
      credential_id.to_string(),
      EntityKind::AgentRegistration,
      credential_id.to_string(),
    ),
  };
  let identity = MutationIdentity::new(
    kind,
    key,
    request.revoked_at,
    entity,
    &TargetFingerprint {
      target_identity: &target_identity,
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => {
      let _: StoredOutcome = decode_outcome(outcome)?;
      return Ok(MutationDisposition::Replayed);
    }
  };

  let updated = match request.target {
    AgentCredentialTarget::Enrollment(credential_id) => sqlx::query(
      "UPDATE agent_enrollment_credentials \
         SET revoked_at = COALESCE(revoked_at, to_timestamp($1::double precision / 1000.0)) \
         WHERE id = $2",
    )
    .bind(request.revoked_at.unix_millis())
    .bind(credential_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?,
    AgentCredentialTarget::Registration(credential_id) => sqlx::query(
      "UPDATE agent_registrations \
         SET revoked_at = COALESCE(revoked_at, to_timestamp($1::double precision / 1000.0)) \
         WHERE id = $2",
    )
    .bind(request.revoked_at.unix_millis())
    .bind(credential_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?,
  };
  if updated.rows_affected() != 1 {
    return Err(StoreError::NotFound { entity });
  }

  crate::mutation::commit(
    transaction,
    &identity,
    MutationFacts {
      actor_kind: "unauthenticated_management",
      actor_identity: None,
      target_identity: target_identity.clone(),
      safe_metadata: json!({"credential_identity": target_identity}),
      outbox_payload: json!({"credential_identity": target_identity}),
    },
    encode_outcome(&StoredOutcome {})?,
  )
  .await?;
  Ok(MutationDisposition::Applied)
}

#[derive(Serialize)]
struct TargetFingerprint<'a> {
  target_identity: &'a str,
}

#[derive(Deserialize, Serialize)]
struct StoredOutcome {}
