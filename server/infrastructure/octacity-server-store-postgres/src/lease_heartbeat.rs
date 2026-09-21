use octacity_server_domain::{AttemptNumber, EntityKind};
use octacity_server_store::{LeaseHeartbeatOutcome, RenewLease, StoreError, StoreOperation};
use serde_json::json;
use sqlx::PgPool;

use crate::{
  database::{classify, unavailable},
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};

pub(crate) async fn execute(pool: &PgPool, request: RenewLease) -> Result<LeaseHeartbeatOutcome, StoreError> {
  request.validate()?;
  let digest_input = json!({
    "attempt": request.attempt,
    "fence": request.lease.fence.expose(),
    "job_id": request.job_id,
    "lease_id": request.lease.lease_id,
    "agent_id": request.lease.agent_id,
    "registration_epoch": request.lease.registration_epoch.get(),
  });
  let identity = MutationIdentity::new(
    MutationKind::RenewLease,
    request.idempotency_key.to_string(),
    request.observed_at,
    EntityKind::Lease,
    &digest_input,
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return decode_outcome(outcome),
  };
  let row = match crate::lease::load(&mut transaction, request.lease, StoreOperation::RenewLease).await {
    Ok(row) => row,
    Err(StoreError::Fenced { .. } | StoreError::Expired { .. }) => {
      transaction.rollback().await.map_err(unavailable)?;
      return Ok(LeaseHeartbeatOutcome::Fenced);
    }
    Err(error) => return Err(error),
  };
  let attempt = AttemptNumber::new(u64::try_from(row.attempt_number).map_err(|_| StoreError::Unavailable)?)
    .map_err(|_| StoreError::Unavailable)?;
  if row.job_id != request.job_id.as_uuid() || attempt != request.attempt {
    transaction.rollback().await.map_err(unavailable)?;
    return Ok(LeaseHeartbeatOutcome::Fenced);
  }
  if let Err(error) = crate::lease::require_current(&row, request.lease, request.observed_at) {
    transaction.rollback().await.map_err(unavailable)?;
    return match error {
      StoreError::Fenced { .. } | StoreError::Expired { .. } => Ok(LeaseHeartbeatOutcome::Fenced),
      error => Err(error),
    };
  }

  let outcome = match row.state.as_str() {
    "active" => {
      renew_deadline(&mut transaction, &request).await?;
      LeaseHeartbeatOutcome::Continue {
        expires_at: request.expires_at,
      }
    }
    "cancellation_requested" => LeaseHeartbeatOutcome::Cancel,
    "drain_requested" => {
      renew_deadline(&mut transaction, &request).await?;
      LeaseHeartbeatOutcome::Drain {
        expires_at: request.expires_at,
      }
    }
    _ => {
      transaction.rollback().await.map_err(unavailable)?;
      return Ok(LeaseHeartbeatOutcome::Fenced);
    }
  };
  let updated = sqlx::query(
    "UPDATE agents SET last_seen_at = to_timestamp($1::double precision / 1000.0), \
       updated_at = GREATEST(updated_at, to_timestamp($1::double precision / 1000.0)) WHERE id = $2",
  )
  .bind(request.observed_at.unix_millis())
  .bind(request.lease.agent_id.as_uuid())
  .execute(&mut *transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Agent))?;
  if updated.rows_affected() != 1 {
    return Err(StoreError::Unavailable);
  }
  crate::mutation::commit(
    transaction,
    &identity,
    facts(&request, outcome),
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

async fn renew_deadline(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  request: &RenewLease,
) -> Result<(), StoreError> {
  let updated = sqlx::query(
    "UPDATE leases SET expires_at = to_timestamp($1::double precision / 1000.0), version = version + 1 \
     WHERE id = $2",
  )
  .bind(request.expires_at.unix_millis())
  .bind(request.lease.lease_id.as_uuid())
  .execute(&mut **transaction)
  .await
  .map_err(|error| classify(error, EntityKind::Lease))?;
  if updated.rows_affected() != 1 {
    return Err(StoreError::Unavailable);
  }
  Ok(())
}

fn facts(request: &RenewLease, outcome: LeaseHeartbeatOutcome) -> MutationFacts {
  let directive = match outcome {
    LeaseHeartbeatOutcome::Continue { .. } => "continue",
    LeaseHeartbeatOutcome::Cancel => "cancel",
    LeaseHeartbeatOutcome::Fenced => "fenced",
    LeaseHeartbeatOutcome::Drain { .. } => "drain",
  };
  MutationFacts {
    actor_kind: "agent",
    actor_identity: Some(request.lease.agent_id.to_string()),
    target_identity: request.lease.lease_id.to_string(),
    safe_metadata: json!({
      "directive": directive,
      "job_id": request.job_id,
      "registration_epoch": request.lease.registration_epoch.get(),
    }),
    outbox_payload: json!({
      "directive": directive,
      "job_id": request.job_id,
      "lease_id": request.lease.lease_id,
      "schema_version": 1,
    }),
  }
}
