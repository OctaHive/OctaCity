use octacity_server_domain::EntityKind;
use octacity_server_factory::{
  FactoryClaim, FactoryClaimFence, FactoryDigest, FactoryKey, FactoryRunId, FactoryWipUsage,
};
use octacity_server_store::{
  AuditActorKind, ClaimFactoryRun, ClaimFactoryRunOutcome, ClaimFactoryRuns, ClaimedFactoryRun, FactoryAuditFact,
  FactoryRunClaimRecord, MutationDisposition, StoreError, StoreInputError, StoreOperation,
};
use sqlx::{PgPool, Postgres, Transaction};

use super::{conflict, insert_audit, insert_claim, lock_run, read_audit, read_claim};
use crate::database::unavailable;

pub(crate) async fn claim(pool: &PgPool, request: ClaimFactoryRun) -> Result<ClaimFactoryRunOutcome, StoreError> {
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let locked = lock_run(&mut transaction, request.run_id).await?;
  if let Some(existing) = read_claim(&mut transaction, request.record.id).await? {
    let audit = read_audit(&mut transaction, request.audit.id).await?;
    if existing != request.record || audit.as_ref() != Some(&request.audit) || locked.claim_id != Some(existing.id) {
      return Err(conflict());
    }
    transaction.commit().await.map_err(unavailable)?;
    return Ok(ClaimFactoryRunOutcome {
      disposition: MutationDisposition::Replayed,
      record: existing,
    });
  }
  validate_claim(&mut transaction, &locked, &request).await?;
  insert_claim(&mut transaction, request.run_id, &request.record).await?;
  insert_audit(&mut transaction, &request.audit).await?;
  update_current_claim(&mut transaction, request.run_id, request.record.id).await?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(ClaimFactoryRunOutcome {
    disposition: MutationDisposition::Applied,
    record: request.record,
  })
}

pub(crate) async fn claim_batch(
  pool: &PgPool,
  request: ClaimFactoryRuns,
) -> Result<Vec<ClaimedFactoryRun>, StoreError> {
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let run_ids: Vec<uuid::Uuid> = sqlx::query_scalar(
    "SELECT run.id FROM factory_runs AS run \
     JOIN factory_run_current AS current ON current.run_id = run.id \
     LEFT JOIN factory_run_claims AS claim ON claim.id = current.claim_id \
     WHERE run.visible AND run.state NOT IN ('rejected', 'cancelled', 'completed') \
       AND (current.claim_id IS NULL OR claim.expires_at <= to_timestamp($1::double precision / 1000.0) \
         OR (claim.owner = $2 \
           AND claim.claimed_at = to_timestamp($1::double precision / 1000.0) \
           AND claim.expires_at = to_timestamp($3::double precision / 1000.0))) \
     ORDER BY run.updated_at, run.id \
     FOR UPDATE OF run SKIP LOCKED LIMIT $4",
  )
  .bind(request.observed_at.unix_millis())
  .bind(request.owner.as_str())
  .bind(request.claim_expires_at.unix_millis())
  .bind(i64::from(request.limit.get()))
  .fetch_all(&mut *transaction)
  .await
  .map_err(unavailable)?;

  let mut claimed = Vec::with_capacity(run_ids.len());
  for raw_run_id in run_ids {
    let run_id = FactoryRunId::from_uuid(raw_run_id).map_err(|_| StoreError::Unavailable)?;
    let locked = lock_run(&mut transaction, run_id).await?;
    let expected_version = locked.run.version();
    let wip_usage = wip_usage(&mut transaction, &locked).await?;
    if let Some(current_id) = locked.claim_id {
      let current = read_claim(&mut transaction, current_id)
        .await?
        .ok_or(StoreError::Unavailable)?;
      if current.owner == request.owner
        && current.claim.claimed_at() == request.observed_at
        && current.claim.expires_at() == request.claim_expires_at
      {
        claimed.push(ClaimedFactoryRun {
          run_id,
          disposition: MutationDisposition::Replayed,
          expected_version,
          record: current,
          wip_usage,
        });
        continue;
      }
    }
    let record = batch_claim(run_id, expected_version, &request)?;
    let audit = batch_claim_audit(run_id, &request, record.id);
    let direct = ClaimFactoryRun {
      run_id,
      expected_version,
      record: record.clone(),
      audit: audit.clone(),
    };
    validate_claim(&mut transaction, &locked, &direct).await?;
    insert_claim(&mut transaction, run_id, &record).await?;
    insert_audit(&mut transaction, &audit).await?;
    update_current_claim(&mut transaction, run_id, record.id).await?;
    claimed.push(ClaimedFactoryRun {
      run_id,
      disposition: MutationDisposition::Applied,
      expected_version,
      record,
      wip_usage,
    });
  }
  transaction.commit().await.map_err(unavailable)?;
  Ok(claimed)
}

async fn validate_claim(
  transaction: &mut Transaction<'_, Postgres>,
  locked: &super::LockedRun,
  request: &ClaimFactoryRun,
) -> Result<(), StoreError> {
  if request.expected_version != locked.run.version() {
    return Err(conflict());
  }
  if let Some(current_id) = locked.claim_id {
    let current = read_claim(transaction, current_id)
      .await?
      .ok_or(StoreError::Unavailable)?;
    if current.claim.expires_at() > request.record.claim.claimed_at() {
      return Err(conflict());
    }
  }
  let canonical = FactoryRunClaimRecord::new(request.run_id, request.record.owner.clone(), request.record.claim);
  let canonical_audit = FactoryAuditFact::new(
    request.audit.run_id,
    request.audit.actor_kind,
    request.audit.actor_identity_digest,
    request.audit.operation.clone(),
    request.audit.request_identity_digest,
    request.audit.outcome.clone(),
    request.audit.recorded_at,
  );
  if request.record != canonical
    || request.record.claim.claimed_at() < locked.admitted_at
    || request.audit.run_id != request.run_id
    || request.audit.recorded_at != request.record.claim.claimed_at()
    || request.audit != canonical_audit
  {
    return Err(StoreError::InvalidInput {
      operation: StoreOperation::ClaimFactoryRun,
      source: StoreInputError::InvalidFactoryRunClaim,
    });
  }
  Ok(())
}

async fn update_current_claim(
  transaction: &mut Transaction<'_, Postgres>,
  run_id: FactoryRunId,
  claim_id: FactoryDigest,
) -> Result<(), StoreError> {
  let updated = sqlx::query("UPDATE factory_run_current SET claim_id = $1 WHERE run_id = $2")
    .bind(claim_id.as_bytes().as_slice())
    .bind(run_id.as_uuid())
    .execute(&mut **transaction)
    .await
    .map_err(unavailable)?;
  if updated.rows_affected() != 1 {
    return Err(StoreError::NotFound {
      entity: EntityKind::FactoryRun,
    });
  }
  Ok(())
}

fn batch_claim(
  run_id: FactoryRunId,
  version: octacity_server_factory::FactoryRunVersion,
  request: &ClaimFactoryRuns,
) -> Result<FactoryRunClaimRecord, StoreError> {
  let version = version.get().to_be_bytes();
  let observed = request.observed_at.unix_millis().to_be_bytes();
  let expires = request.claim_expires_at.unix_millis().to_be_bytes();
  let fence = FactoryClaimFence::new(FactoryDigest::sha256(
    "octacity.factory.reconciliation-fence.v1",
    &[
      run_id.as_uuid().as_bytes(),
      request.owner.as_str().as_bytes(),
      &version,
      &observed,
      &expires,
    ],
  ));
  let claim =
    FactoryClaim::new(fence, request.observed_at, request.claim_expires_at).map_err(|_| StoreError::InvalidInput {
      operation: StoreOperation::ClaimFactoryRuns,
      source: StoreInputError::InvalidFactoryRunClaim,
    })?;
  Ok(FactoryRunClaimRecord::new(run_id, request.owner.clone(), claim))
}

fn batch_claim_audit(run_id: FactoryRunId, request: &ClaimFactoryRuns, claim_id: FactoryDigest) -> FactoryAuditFact {
  FactoryAuditFact::new(
    run_id,
    AuditActorKind::Worker,
    Some(FactoryDigest::sha256(
      "octacity.factory.audit-worker.v1",
      &[request.owner.as_str().as_bytes()],
    )),
    FactoryKey::new("factory.claimed").expect("static Factory key is valid"),
    claim_id,
    FactoryKey::new("accepted").expect("static Factory key is valid"),
    request.observed_at,
  )
}

async fn wip_usage(
  transaction: &mut Transaction<'_, Postgres>,
  locked: &super::LockedRun,
) -> Result<FactoryWipUsage, StoreError> {
  let rows = sqlx::query_as::<_, (serde_json::Value,)>(
    "SELECT checkpoint.lifecycle FROM factory_runs AS run \
     JOIN factory_run_current AS current ON current.run_id = run.id \
     JOIN factory_lifecycle_checkpoints AS checkpoint ON checkpoint.id = current.lifecycle_checkpoint_id \
     WHERE run.factory_configuration_id = $1 AND run.factory_configuration_version = $2 \
       AND run.visible AND run.state NOT IN ('rejected', 'cancelled', 'completed')",
  )
  .bind(locked.run.configuration().id().as_uuid())
  .bind(i64::try_from(locked.run.configuration().version().get()).map_err(|_| StoreError::Unavailable)?)
  .fetch_all(&mut **transaction)
  .await
  .map_err(unavailable)?;
  let active_runs = u32::try_from(rows.len()).map_err(|_| StoreError::Unavailable)?;
  let mut active_stages = 0_u32;
  for (document,) in rows {
    let document: super::LifecycleDocument = serde_json::from_value(document).map_err(|_| StoreError::Unavailable)?;
    active_stages = active_stages.saturating_add(document.progress.active_stage_count());
  }
  Ok(FactoryWipUsage::new(active_runs, active_stages))
}
