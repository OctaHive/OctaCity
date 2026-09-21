use octacity_server_domain::{EntityKind, JobId, LeaseId};
use octacity_server_job::JobSpecSigner;
use octacity_server_store::{
  ClaimExpiredLeases, ExpiredLeaseClaim, LeaseRecoveryAction, MutationDisposition, RecoverExpiredLease,
  RecoverExpiredLeaseOutcome, StoreError,
};
use serde::Serialize;
use serde_json::json;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{
  database::unavailable,
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
  state::{attempt_state, build_state},
};

const WORK_KIND: &str = "lease_expiry";

#[derive(FromRow)]
struct ClaimRow {
  lease_id: Uuid,
  job_id: Uuid,
}

pub(crate) async fn claim(pool: &PgPool, request: ClaimExpiredLeases) -> Result<Vec<ExpiredLeaseClaim>, StoreError> {
  let rows: Vec<ClaimRow> = sqlx::query_as(
    "WITH candidates AS ( \
       SELECT lease.id AS lease_id, lease.job_id \
       FROM leases AS lease \
       LEFT JOIN worker_claims AS claim \
         ON claim.work_kind = $1 AND claim.work_identity = lease.id::text \
       WHERE lease.state IN ('active', 'cancellation_requested', 'drain_requested') \
         AND lease.expires_at <= to_timestamp($2::double precision / 1000.0) \
         AND (claim.work_identity IS NULL OR claim.expires_at <= to_timestamp($2::double precision / 1000.0)) \
       ORDER BY lease.expires_at, lease.id \
       LIMIT $3 FOR UPDATE OF lease SKIP LOCKED \
     ), claimed AS ( \
       INSERT INTO worker_claims (work_kind, work_identity, owner, claimed_at, expires_at) \
       SELECT $1, candidate.lease_id::text, $4, \
              to_timestamp($2::double precision / 1000.0), to_timestamp($5::double precision / 1000.0) \
       FROM candidates AS candidate \
       ON CONFLICT (work_kind, work_identity) DO UPDATE \
         SET owner = EXCLUDED.owner, claimed_at = EXCLUDED.claimed_at, expires_at = EXCLUDED.expires_at, completed_at = NULL \
         WHERE worker_claims.expires_at <= to_timestamp($2::double precision / 1000.0) \
       RETURNING work_identity \
     ) \
     SELECT candidate.lease_id, candidate.job_id FROM candidates AS candidate \
     JOIN claimed ON claimed.work_identity = candidate.lease_id::text \
     ORDER BY candidate.lease_id",
  )
  .bind(WORK_KIND)
  .bind(request.observed_at.unix_millis())
  .bind(i64::from(request.limit.get()))
  .bind(request.owner.as_str())
  .bind(request.claim_expires_at.unix_millis())
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;
  rows
    .into_iter()
    .map(|row| {
      Ok(ExpiredLeaseClaim {
        lease_id: LeaseId::from_uuid(row.lease_id).map_err(|_| StoreError::Unavailable)?,
        job_id: JobId::from_uuid(row.job_id).map_err(|_| StoreError::Unavailable)?,
        owner: request.owner.clone(),
        claim_expires_at: request.claim_expires_at,
      })
    })
    .collect()
}

#[derive(Serialize)]
struct RecoveryFingerprint {
  lease_id: LeaseId,
  job_id: JobId,
}

#[derive(FromRow)]
struct RecoveryRow {
  job_id: Uuid,
  lease_state: String,
  job_state: String,
  infrastructure_requeues: i64,
  max_attempts: i64,
  retries_infrastructure: bool,
  attempt_id: Uuid,
  build_id: Uuid,
  cancelled: bool,
}

pub(crate) async fn recover(
  pool: &PgPool,
  signer: &JobSpecSigner,
  request: RecoverExpiredLease,
) -> Result<RecoverExpiredLeaseOutcome, StoreError> {
  let identity = MutationIdentity::new(
    MutationKind::RecoverExpiredLease,
    request.claim.lease_id.to_string(),
    request.recovered_at,
    EntityKind::Lease,
    &RecoveryFingerprint {
      lease_id: request.claim.lease_id,
      job_id: request.claim.job_id,
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return decode_outcome(outcome),
  };
  let owns_claim: Option<String> = sqlx::query_scalar(
    "SELECT owner FROM worker_claims WHERE work_kind = $1 AND work_identity = $2 \
       AND owner = $3 AND expires_at > to_timestamp($4::double precision / 1000.0) FOR UPDATE",
  )
  .bind(WORK_KIND)
  .bind(request.claim.lease_id.to_string())
  .bind(request.claim.owner.as_str())
  .bind(request.recovered_at.unix_millis())
  .fetch_optional(&mut *transaction)
  .await
  .map_err(unavailable)?;
  if owns_claim.is_none() || request.recovered_at >= request.claim.claim_expires_at {
    return Err(StoreError::Conflict {
      entity: EntityKind::Lease,
    });
  }

  let row: RecoveryRow = sqlx::query_as(
    "SELECT lease.job_id, lease.state AS lease_state, job.state AS job_state, job.infrastructure_requeues, \
            configuration.retry_max_attempts AS max_attempts, \
            configuration.retries_infrastructure, \
            attempt.id AS attempt_id, build.id AS build_id, \
            EXISTS (SELECT 1 FROM build_cancellations WHERE build_id = build.id) AS cancelled \
     FROM leases AS lease \
     JOIN jobs AS job ON job.id = lease.job_id \
     JOIN attempts AS attempt ON attempt.id = job.attempt_id \
     JOIN builds AS build ON build.id = attempt.build_id \
     JOIN build_configuration_versions AS configuration \
       ON configuration.build_configuration_id = build.build_configuration_id \
      AND configuration.version = build.build_configuration_version \
     WHERE lease.id = $1 FOR UPDATE OF lease, job, attempt, build",
  )
  .bind(request.claim.lease_id.as_uuid())
  .fetch_optional(&mut *transaction)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Lease,
  })?;
  if row.job_id != request.claim.job_id.as_uuid() {
    return Err(StoreError::Conflict {
      entity: EntityKind::Lease,
    });
  }

  let current = matches!(
    row.lease_state.as_str(),
    "active" | "cancellation_requested" | "drain_requested"
  );
  let (action, requeues) = if !current {
    (
      LeaseRecoveryAction::AlreadyRecovered,
      u16_requeues(row.infrastructure_requeues)?,
    )
  } else if matches!(row.job_state.as_str(), "leased" | "running")
    && !row.cancelled
    && row.retries_infrastructure
    && row.infrastructure_requeues < row.max_attempts.saturating_sub(1)
  {
    expire_lease(&mut transaction, request.claim.lease_id, request.recovered_at).await?;
    let requeues = row.infrastructure_requeues + 1;
    let updated = sqlx::query(
      "UPDATE jobs SET state = 'ready', infrastructure_requeues = $1, version = version + 1, \
         updated_at = to_timestamp($2::double precision / 1000.0) WHERE id = $3",
    )
    .bind(requeues)
    .bind(request.recovered_at.unix_millis())
    .bind(row.job_id)
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
    if updated.rows_affected() != 1 {
      return Err(StoreError::Unavailable);
    }
    enqueue(&mut transaction, row.job_id, request.recovered_at).await?;
    (LeaseRecoveryAction::Requeued, u16_requeues(requeues)?)
  } else {
    expire_lease(&mut transaction, request.claim.lease_id, request.recovered_at).await?;
    let (terminal_state, completion_kind, failure_class, action) = if row.cancelled {
      ("cancelled", "cancelled", None, LeaseRecoveryAction::Cancelled)
    } else {
      ("failed", "failed", Some("infrastructure"), LeaseRecoveryAction::Failed)
    };
    let updated = sqlx::query(
      "UPDATE jobs SET state = $1, version = version + 1, \
         updated_at = to_timestamp($2::double precision / 1000.0) \
       WHERE id = $3 AND state IN ('leased', 'running', 'cancelling')",
    )
    .bind(terminal_state)
    .bind(request.recovered_at.unix_millis())
    .bind(row.job_id)
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
    if updated.rows_affected() != 1 {
      return Err(StoreError::Unavailable);
    }
    let applied = crate::job_completion::orchestration::reconcile_and_apply(
      &mut transaction,
      signer,
      row.attempt_id,
      row.build_id,
      request.recovered_at,
    )
    .await?;
    let final_sequence: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(sequence), 0) FROM job_events WHERE job_id = $1")
      .bind(row.job_id)
      .fetch_one(&mut *transaction)
      .await
      .map_err(unavailable)?;
    sqlx::query(
      "INSERT INTO job_completions \
         (job_id, completion_id, lease_id, final_sequence, kind, failure_class, ready_job_ids, skipped_job_ids, \
          attempt_state, build_state, completed_at) \
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, \
               to_timestamp($11::double precision / 1000.0)) ON CONFLICT (job_id) DO NOTHING",
    )
    .bind(row.job_id)
    .bind(format!("lease-expiry:{}", request.claim.lease_id))
    .bind(request.claim.lease_id.as_uuid())
    .bind(final_sequence)
    .bind(completion_kind)
    .bind(failure_class)
    .bind(&applied.ready_jobs)
    .bind(&applied.skipped_jobs)
    .bind(attempt_state(applied.attempt_state))
    .bind(build_state(applied.build_state))
    .bind(request.recovered_at.unix_millis())
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
    (action, u16_requeues(row.infrastructure_requeues)?)
  };

  let completed = sqlx::query(
    "UPDATE worker_claims SET completed_at = to_timestamp($1::double precision / 1000.0), \
       expires_at = to_timestamp($1::double precision / 1000.0) \
     WHERE work_kind = $2 AND work_identity = $3 AND owner = $4",
  )
  .bind(request.recovered_at.unix_millis())
  .bind(WORK_KIND)
  .bind(request.claim.lease_id.to_string())
  .bind(request.claim.owner.as_str())
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?;
  if completed.rows_affected() != 1 {
    return Err(StoreError::Unavailable);
  }
  let outcome = RecoverExpiredLeaseOutcome {
    disposition: if action == LeaseRecoveryAction::AlreadyRecovered {
      MutationDisposition::Replayed
    } else {
      MutationDisposition::Applied
    },
    lease_id: request.claim.lease_id,
    job_id: request.claim.job_id,
    action,
    infrastructure_requeues: requeues,
  };
  crate::mutation::commit(
    transaction,
    &identity,
    MutationFacts {
      actor_kind: "worker",
      actor_identity: Some(request.claim.owner.as_str().to_owned()),
      target_identity: request.claim.lease_id.to_string(),
      safe_metadata: json!({"action": format!("{action:?}"), "job_id": request.claim.job_id, "requeues": requeues}),
      outbox_payload: json!({"action": format!("{action:?}"), "job_id": request.claim.job_id, "lease_id": request.claim.lease_id}),
    },
    encode_outcome(&outcome)?,
  )
  .await?;
  Ok(outcome)
}

async fn expire_lease(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  lease_id: LeaseId,
  recovered_at: octacity_server_domain::Timestamp,
) -> Result<(), StoreError> {
  sqlx::query(
    "UPDATE leases SET state = 'expired', version = version + 1, \
       completed_at = to_timestamp($1::double precision / 1000.0) WHERE id = $2",
  )
  .bind(recovered_at.unix_millis())
  .bind(lease_id.as_uuid())
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  Ok(())
}

async fn enqueue(
  transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  job_id: Uuid,
  enqueued_at: octacity_server_domain::Timestamp,
) -> Result<(), StoreError> {
  sqlx::query(
    "INSERT INTO ready_queue_entries \
       (job_id, priority, enqueued_at, project_id, build_configuration_id, build_configuration_version, \
        allowed_pool_ids, requirements) \
     SELECT job.id, build.priority, to_timestamp($1::double precision / 1000.0), build.project_id, \
            build.build_configuration_id, build.build_configuration_version, job.allowed_pool_ids, job.requirements \
     FROM jobs AS job JOIN attempts AS attempt ON attempt.id = job.attempt_id \
     JOIN builds AS build ON build.id = attempt.build_id WHERE job.id = $2",
  )
  .bind(enqueued_at.unix_millis())
  .bind(job_id)
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  crate::ready_queue_notification::notify_after_commit(transaction).await?;
  Ok(())
}

fn u16_requeues(value: i64) -> Result<u16, StoreError> {
  u16::try_from(value).map_err(|_| StoreError::Unavailable)
}
