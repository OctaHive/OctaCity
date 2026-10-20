use octacity_server_factory::{FactoryClaim, FactoryClaimFence, FactoryDigest, FactoryKey};
use octacity_server_store::{
  AuditActorKind, ClaimFactoryOutbox, ClaimedFactoryOutbox, FactoryAuditFact, FactoryOutboxRecord,
  FactoryOutboxSettlement, FactoryOutboxState, SettleFactoryOutbox, StoreError, StoreInputError, StoreOperation,
};
use sqlx::{PgPool, Postgres, Transaction};

use super::{OutboxRow, conflict, decode_outbox, insert_audit, insert_outbox, invalid};
use crate::database::unavailable;

pub(crate) async fn claim_outbox(
  pool: &PgPool,
  request: ClaimFactoryOutbox,
) -> Result<Vec<ClaimedFactoryOutbox>, StoreError> {
  // A restarted worker must not dispatch a partial legacy publication batch.
  // Work-only consumers remain independent of Issue recovery.
  if (request.kinds.is_empty()
    || request
      .kinds
      .iter()
      .any(|kind| kind.as_str() == "issue.status.projection"))
    && crate::issue_reporter::has_pending_publications(pool)
      .await
      .map_err(unavailable)?
  {
    return Err(StoreError::Unavailable);
  }
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let operation_ids: Vec<Vec<u8>> = sqlx::query_scalar(
    "WITH latest AS ( \
       SELECT DISTINCT ON (operation_id) operation_id, kind, state, available_at, claim_expires_at, recorded_at, id \
       FROM factory_outbox_records \
       ORDER BY operation_id, attempt DESC, \
         CASE state WHEN 'pending' THEN 0 WHEN 'claimed' THEN 1 ELSE 2 END DESC, recorded_at DESC, id DESC \
     ) \
     SELECT operation_id FROM latest \
     WHERE ($3::text[] IS NULL OR kind = ANY($3)) AND ( \
       (state = 'pending' AND available_at <= to_timestamp($1::double precision / 1000.0)) \
        OR (state = 'claimed' AND claim_expires_at <= to_timestamp($1::double precision / 1000.0))) \
     ORDER BY available_at, operation_id LIMIT $2",
  )
  .bind(request.observed_at.unix_millis())
  .bind(i64::from(request.limit.get()))
  .bind((!request.kinds.is_empty()).then(|| request.kinds.iter().map(FactoryKey::as_str).collect::<Vec<_>>()))
  .fetch_all(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let mut claimed = Vec::with_capacity(operation_ids.len());
  for operation_id in operation_ids {
    let operation_id = super::digest(&operation_id)?;
    let Some(initial) = latest(&mut transaction, operation_id).await? else {
      continue;
    };
    lock_subject(&mut transaction, initial.subject).await?;
    let Some(current) = latest(&mut transaction, operation_id).await? else {
      continue;
    };
    let is_pending = current.state == FactoryOutboxState::Pending && current.available_at <= request.observed_at;
    let is_expired = current.state == FactoryOutboxState::Claimed
      && current
        .claim
        .is_some_and(|claim| claim.expires_at() <= request.observed_at);
    if !is_pending && !is_expired {
      continue;
    }
    let pending = if is_expired {
      let retry = FactoryOutboxRecord::retry(&current, request.observed_at, request.observed_at).ok_or_else(|| {
        invalid(
          StoreOperation::ClaimFactoryOutbox,
          StoreInputError::InvalidFactoryRunTransition,
        )
      })?;
      insert_outbox(&mut transaction, &retry).await?;
      retry
    } else {
      current
    };
    let attempt = pending.attempt.to_be_bytes();
    let observed = request.observed_at.unix_millis().to_be_bytes();
    let expires = request.claim_expires_at.unix_millis().to_be_bytes();
    let fence = FactoryClaimFence::new(FactoryDigest::sha256(
      "octacity.factory.outbox-fence.v1",
      &[
        &pending.operation_id.as_bytes(),
        request.owner.as_str().as_bytes(),
        &attempt,
        &observed,
        &expires,
      ],
    ));
    let claim = FactoryClaim::new(fence, request.observed_at, request.claim_expires_at)
      .map_err(|_| invalid(StoreOperation::ClaimFactoryOutbox, StoreInputError::InvalidWorkerClaim))?;
    let record = FactoryOutboxRecord::claimed(
      &pending,
      request.owner.clone(),
      claim,
      pending.attempt,
      request.observed_at,
    );
    insert_outbox(&mut transaction, &record).await?;
    if let Some(audit) = worker_audit(&request.owner, &record, "factory.outbox.claimed", "accepted") {
      insert_audit(&mut transaction, &audit).await?;
    }
    claimed.push(ClaimedFactoryOutbox { record });
  }
  transaction.commit().await.map_err(unavailable)?;
  Ok(claimed)
}

pub(crate) async fn settle_outbox(
  pool: &PgPool,
  request: SettleFactoryOutbox,
) -> Result<FactoryOutboxRecord, StoreError> {
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let current = latest(&mut transaction, request.operation_id)
    .await?
    .ok_or(StoreError::NotFound {
      entity: octacity_server_domain::EntityKind::FactoryRun,
    })?;
  lock_subject(&mut transaction, current.subject).await?;
  let current = latest(&mut transaction, request.operation_id)
    .await?
    .ok_or(StoreError::Unavailable)?;
  if (matches!(
    (current.state, request.settlement),
    (FactoryOutboxState::Delivered, FactoryOutboxSettlement::Delivered)
      | (FactoryOutboxState::Failed, FactoryOutboxSettlement::Failed)
  ) || matches!(request.settlement, FactoryOutboxSettlement::Rejected(reason) if current.state == FactoryOutboxState::Failed && current.failure == Some(reason)))
    && current.owner.as_ref() == Some(&request.owner)
    && current.claim.is_some_and(|claim| claim.fence() == request.fence)
  {
    transaction.commit().await.map_err(unavailable)?;
    return Ok(current);
  }
  if current.state != FactoryOutboxState::Claimed || current.owner.as_ref() != Some(&request.owner) {
    return Err(conflict());
  }
  current
    .claim
    .ok_or_else(conflict)?
    .verify_fence(request.fence, request.observed_at)
    .map_err(|_| conflict())?;
  let (next, outcome) = match request.settlement {
    FactoryOutboxSettlement::Delivered => (
      FactoryOutboxRecord::terminal(&current, FactoryOutboxState::Delivered, request.observed_at),
      "delivered",
    ),
    FactoryOutboxSettlement::Rejected(reason) => (
      FactoryOutboxRecord::rejected(&current, reason, request.observed_at),
      "rejected",
    ),
    FactoryOutboxSettlement::Failed => (
      FactoryOutboxRecord::terminal(&current, FactoryOutboxState::Failed, request.observed_at),
      "failed",
    ),
    FactoryOutboxSettlement::RetryAt(available_at) if available_at >= request.observed_at => (
      FactoryOutboxRecord::retry(&current, available_at, request.observed_at).ok_or_else(|| {
        invalid(
          StoreOperation::SettleFactoryOutbox,
          StoreInputError::InvalidFactoryRunTransition,
        )
      })?,
      "retry_scheduled",
    ),
    FactoryOutboxSettlement::RetryAt(_) => {
      return Err(invalid(
        StoreOperation::SettleFactoryOutbox,
        StoreInputError::InvalidFactoryRunTransition,
      ));
    }
  };
  insert_outbox(&mut transaction, &next).await?;
  if let Some(audit) = worker_audit(&request.owner, &next, "factory.outbox.settled", outcome) {
    insert_audit(&mut transaction, &audit).await?;
  }
  transaction.commit().await.map_err(unavailable)?;
  Ok(next)
}

pub(crate) async fn latest(
  transaction: &mut Transaction<'_, Postgres>,
  operation_id: FactoryDigest,
) -> Result<Option<FactoryOutboxRecord>, StoreError> {
  sqlx::query_as::<_, OutboxRow>(
    "SELECT id, operation_id, run_id, issue, kind, input_digest, state, failure, attempt, \
            FLOOR(EXTRACT(EPOCH FROM available_at) * 1000)::BIGINT AS available_at_millis, \
            FLOOR(EXTRACT(EPOCH FROM recorded_at) * 1000)::BIGINT AS recorded_at_millis, \
            owner, fence, FLOOR(EXTRACT(EPOCH FROM claimed_at) * 1000)::BIGINT AS claimed_at_millis, \
            FLOOR(EXTRACT(EPOCH FROM claim_expires_at) * 1000)::BIGINT AS claim_expires_at_millis \
     FROM factory_outbox_records WHERE operation_id = $1 \
     ORDER BY attempt DESC, CASE state WHEN 'pending' THEN 0 WHEN 'claimed' THEN 1 ELSE 2 END DESC, \
              recorded_at DESC, id DESC LIMIT 1",
  )
  .bind(operation_id.as_bytes().as_slice())
  .fetch_optional(&mut **transaction)
  .await
  .map_err(unavailable)?
  .map(decode_outbox)
  .transpose()
}

fn worker_audit(
  owner: &FactoryKey,
  record: &FactoryOutboxRecord,
  operation: &str,
  outcome: &str,
) -> Option<FactoryAuditFact> {
  Some(FactoryAuditFact::new(
    record.subject.run_id()?,
    AuditActorKind::Worker,
    Some(FactoryDigest::sha256(
      "octacity.factory.audit-worker.v1",
      &[owner.as_str().as_bytes()],
    )),
    FactoryKey::new(operation).expect("static Factory key is valid"),
    record.operation_id,
    FactoryKey::new(outcome).expect("static Factory key is valid"),
    record.recorded_at,
  ))
}

// Lock the accepted owner, never create a Run for an Issue-owned effect.
pub(crate) async fn lock_subject(
  tx: &mut Transaction<'_, Postgres>,
  subject: octacity_server_store::FactoryOutboxSubject,
) -> Result<(), StoreError> {
  match subject {
    octacity_server_store::FactoryOutboxSubject::Run(run) => {
      sqlx::query("SELECT id FROM factory_runs WHERE id=$1 FOR UPDATE")
        .bind(run.as_uuid())
        .execute(&mut **tx)
        .await
        .map_err(unavailable)?;
    }
    octacity_server_store::FactoryOutboxSubject::Issue(issue) => {
      sqlx::query("SELECT id FROM issues WHERE project_id=$1 AND id=$2 FOR NO KEY UPDATE")
        .bind(issue.project_id.as_uuid())
        .bind(issue.issue_id.as_uuid())
        .execute(&mut **tx)
        .await
        .map_err(unavailable)?;
    }
  }
  Ok(())
}
