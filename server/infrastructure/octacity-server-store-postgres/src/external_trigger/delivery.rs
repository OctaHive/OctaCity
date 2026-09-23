use super::*;

pub(crate) async fn enqueue_delivery(
  pool: &sqlx::PgPool,
  request: EnqueueWebhookDelivery,
) -> Result<MutationDisposition, StoreError> {
  request.validate()?;
  let result = sqlx::query(
    "INSERT INTO webhook_deliveries \
       (id, integration_id, headers, body, received_at, next_attempt_at) \
     VALUES ($1, $2, $3, $4, to_timestamp($5::double precision / 1000.0), \
       to_timestamp($5::double precision / 1000.0)) \
     ON CONFLICT (id) DO NOTHING",
  )
  .bind(request.delivery_id.as_uuid())
  .bind(request.integration_id.as_uuid())
  .bind(Json(&request.headers))
  .bind(request.body)
  .bind(request.received_at.unix_millis())
  .execute(pool)
  .await
  .map_err(|error| classify(error, EntityKind::Integration))?;
  Ok(if result.rows_affected() == 1 {
    MutationDisposition::Applied
  } else {
    MutationDisposition::Replayed
  })
}

pub(crate) async fn claim_deliveries(
  pool: &sqlx::PgPool,
  request: ClaimWebhookDeliveries,
) -> Result<Vec<WebhookDeliveryClaim>, StoreError> {
  let rows = sqlx::query_as::<_, DeliveryClaimRow>(
    "WITH available AS (\
       SELECT id FROM webhook_deliveries \
       WHERE state IN ('pending_verification', 'retry_scheduled', 'pending_trigger') \
         AND next_attempt_at <= to_timestamp($1::double precision / 1000.0) \
         AND (claim_expires_at IS NULL OR claim_expires_at <= to_timestamp($1::double precision / 1000.0)) \
       ORDER BY next_attempt_at, received_at, id FOR UPDATE SKIP LOCKED LIMIT $2\
     ), claimed AS (\
       UPDATE webhook_deliveries AS delivery SET claim_owner = $3, \
         claim_expires_at = to_timestamp($4::double precision / 1000.0), \
         verification_attempts = CASE \
           WHEN delivery.state IN ('pending_verification', 'retry_scheduled') \
             THEN delivery.verification_attempts + 1 \
           ELSE delivery.verification_attempts END, \
         trigger_attempts = CASE \
           WHEN delivery.state = 'pending_trigger' THEN delivery.trigger_attempts + 1 \
           ELSE delivery.trigger_attempts END \
       FROM available WHERE delivery.id = available.id \
       RETURNING delivery.*\
     ) SELECT claimed.id, claimed.integration_id, claimed.state, claimed.headers, claimed.body, \
       FLOOR(EXTRACT(EPOCH FROM claimed.received_at) * 1000)::BIGINT AS received_at_millis, \
       claimed.verification_attempts, claimed.trigger_attempts, normalized.event AS normalized_event \
     FROM claimed LEFT JOIN webhook_normalized_events AS normalized \
       ON normalized.canonical_delivery_id = claimed.id \
     ORDER BY claimed.next_attempt_at, claimed.received_at, claimed.id",
  )
  .bind(request.observed_at.unix_millis())
  .bind(i64::from(request.limit.get()))
  .bind(request.owner.as_str())
  .bind(request.claim_expires_at.unix_millis())
  .fetch_all(pool)
  .await
  .map_err(unavailable)?;
  rows
    .into_iter()
    .map(|row| decode_delivery_claim(row, &request))
    .collect()
}

pub(crate) async fn record_event(
  pool: &sqlx::PgPool,
  request: RecordWebhookEvent,
) -> Result<RecordWebhookEventOutcome, StoreError> {
  request.validate()?;
  let event_value = serde_json::to_value(&request.event).map_err(|_| StoreError::Unavailable)?;
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  let receipt_integration: Option<uuid::Uuid> = sqlx::query_scalar(
    "SELECT integration_id FROM webhook_deliveries \
     WHERE id = $1 AND state IN ('pending_verification', 'retry_scheduled') AND claim_owner = $2 \
       AND claim_expires_at > to_timestamp($3::double precision / 1000.0) FOR UPDATE",
  )
  .bind(request.delivery_id.as_uuid())
  .bind(request.owner.as_str())
  .bind(request.recorded_at.unix_millis())
  .fetch_optional(&mut *transaction)
  .await
  .map_err(unavailable)?;
  if receipt_integration != Some(request.event.integration_id.as_uuid()) {
    return Err(StoreError::Conflict {
      entity: EntityKind::Integration,
    });
  }

  let inserted = sqlx::query(
    "INSERT INTO webhook_normalized_events \
       (integration_id, provider_delivery_id, canonical_delivery_id, event, recorded_at) \
     VALUES ($1, $2, $3, $4, to_timestamp($5::double precision / 1000.0)) \
     ON CONFLICT (integration_id, provider_delivery_id) DO NOTHING",
  )
  .bind(request.event.integration_id.as_uuid())
  .bind(request.event.provider_delivery_id.as_str())
  .bind(request.delivery_id.as_uuid())
  .bind(Json(&event_value))
  .bind(request.recorded_at.unix_millis())
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?
  .rows_affected()
    == 1;

  let outcome = if inserted {
    sqlx::query(
      "UPDATE webhook_deliveries SET state = 'pending_trigger', provider_delivery_id = $1, \
         headers = '{}'::jsonb, body = ''::bytea, failure_code = NULL, diagnostic = NULL, \
         next_attempt_at = to_timestamp($2::double precision / 1000.0), claim_owner = NULL, claim_expires_at = NULL \
       WHERE id = $3",
    )
    .bind(request.event.provider_delivery_id.as_str())
    .bind(request.recorded_at.unix_millis())
    .bind(request.delivery_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
    RecordWebhookEventOutcome::Recorded
  } else {
    let existing: Json<Value> = sqlx::query_scalar(
      "SELECT event FROM webhook_normalized_events WHERE integration_id = $1 AND provider_delivery_id = $2",
    )
    .bind(request.event.integration_id.as_uuid())
    .bind(request.event.provider_delivery_id.as_str())
    .fetch_one(&mut *transaction)
    .await
    .map_err(unavailable)?;
    if existing.0 == event_value {
      sqlx::query(
        "UPDATE webhook_deliveries SET state = 'completed', provider_delivery_id = $1, \
           headers = '{}'::jsonb, body = ''::bytea, completed_at = to_timestamp($2::double precision / 1000.0), \
           next_attempt_at = NULL, claim_owner = NULL, claim_expires_at = NULL WHERE id = $3",
      )
      .bind(request.event.provider_delivery_id.as_str())
      .bind(request.recorded_at.unix_millis())
      .bind(request.delivery_id.as_uuid())
      .execute(&mut *transaction)
      .await
      .map_err(unavailable)?;
      RecordWebhookEventOutcome::Duplicate
    } else {
      sqlx::query(
        "UPDATE webhook_deliveries SET state = 'dead_letter', provider_delivery_id = $1, \
           headers = '{}'::jsonb, body = ''::bytea, failure_code = 'delivery_identity_conflict', \
           diagnostic = 'provider delivery identity conflicts with the recorded event', \
           completed_at = to_timestamp($2::double precision / 1000.0), next_attempt_at = NULL, \
           claim_owner = NULL, claim_expires_at = NULL \
         WHERE id = $3",
      )
      .bind(request.event.provider_delivery_id.as_str())
      .bind(request.recorded_at.unix_millis())
      .bind(request.delivery_id.as_uuid())
      .execute(&mut *transaction)
      .await
      .map_err(unavailable)?;
      RecordWebhookEventOutcome::IdentityConflict
    }
  };
  transaction.commit().await.map_err(unavailable)?;
  Ok(outcome)
}

pub(crate) async fn fail_delivery(pool: &sqlx::PgPool, request: FailWebhookDelivery) -> Result<(), StoreError> {
  request.validate()?;
  let result = sqlx::query(
    "UPDATE webhook_deliveries SET state = CASE \
         WHEN $3::bigint IS NULL THEN 'dead_letter' \
         WHEN state = 'pending_trigger' THEN 'pending_trigger' \
         ELSE 'retry_scheduled' END, failure_code = $1, diagnostic = $2, \
       next_attempt_at = CASE WHEN $3::bigint IS NULL THEN NULL ELSE to_timestamp($3::double precision / 1000.0) END, \
       completed_at = CASE WHEN $3::bigint IS NULL THEN to_timestamp($4::double precision / 1000.0) ELSE NULL END, \
       headers = CASE WHEN $3::bigint IS NULL THEN '{}'::jsonb ELSE headers END, \
       body = CASE WHEN $3::bigint IS NULL THEN ''::bytea ELSE body END, \
       claim_owner = NULL, claim_expires_at = NULL \
     WHERE id = $5 AND state IN ('pending_verification', 'retry_scheduled', 'pending_trigger') AND claim_owner = $6 \
       AND claim_expires_at > to_timestamp($7::double precision / 1000.0)",
  )
  .bind(failure_code(request.code))
  .bind(request.diagnostic)
  .bind(request.retry_at.map(Timestamp::unix_millis))
  .bind(request.failed_at.unix_millis())
  .bind(request.delivery_id.as_uuid())
  .bind(request.owner.as_str())
  .bind(request.failed_at.unix_millis())
  .execute(pool)
  .await
  .map_err(unavailable)?;
  if result.rows_affected() == 1 {
    Ok(())
  } else {
    Err(StoreError::Conflict {
      entity: EntityKind::Integration,
    })
  }
}

pub(crate) async fn complete_delivery(
  pool: &sqlx::PgPool,
  request: CompleteWebhookDelivery,
) -> Result<MutationDisposition, StoreError> {
  let result = sqlx::query(
    "UPDATE webhook_deliveries SET state = 'completed', completed_at = to_timestamp($1::double precision / 1000.0), \
       next_attempt_at = NULL, claim_owner = NULL, claim_expires_at = NULL \
     WHERE id = $2 AND state = 'pending_trigger' AND claim_owner = $3 \
       AND claim_expires_at > to_timestamp($1::double precision / 1000.0)",
  )
  .bind(request.completed_at.unix_millis())
  .bind(request.delivery_id.as_uuid())
  .bind(request.owner.as_str())
  .execute(pool)
  .await
  .map_err(unavailable)?;
  if result.rows_affected() == 1 {
    Ok(MutationDisposition::Applied)
  } else {
    Err(StoreError::Conflict {
      entity: EntityKind::Integration,
    })
  }
}

pub(crate) async fn suppress_delivery(
  pool: &sqlx::PgPool,
  request: SuppressWebhookDelivery,
) -> Result<MutationDisposition, StoreError> {
  let result = sqlx::query(
    "UPDATE webhook_deliveries SET state = 'suppressed', completed_at = to_timestamp($1::double precision / 1000.0), \
       next_attempt_at = NULL, headers = '{}'::jsonb, body = ''::bytea, failure_code = NULL, diagnostic = NULL, \
       claim_owner = NULL, claim_expires_at = NULL \
     WHERE id = $2 AND state IN ('pending_verification', 'retry_scheduled', 'pending_trigger') AND claim_owner = $3 \
       AND claim_expires_at > to_timestamp($1::double precision / 1000.0)",
  )
  .bind(request.suppressed_at.unix_millis())
  .bind(request.delivery_id.as_uuid())
  .bind(request.owner.as_str())
  .execute(pool)
  .await
  .map_err(unavailable)?;
  if result.rows_affected() == 1 {
    Ok(MutationDisposition::Applied)
  } else {
    Err(StoreError::Conflict {
      entity: EntityKind::Integration,
    })
  }
}

pub(crate) async fn delivery_diagnostic(
  pool: &sqlx::PgPool,
  delivery_id: WebhookDeliveryId,
) -> Result<WebhookDeliveryDiagnostic, StoreError> {
  let row = sqlx::query_as::<_, DeliveryDiagnosticRow>(
    "SELECT integration_id, state, verification_attempts, trigger_attempts, provider_delivery_id, failure_code, diagnostic, \
       FLOOR(EXTRACT(EPOCH FROM next_attempt_at) * 1000)::BIGINT AS next_attempt_at_millis \
     FROM webhook_deliveries WHERE id = $1",
  )
  .bind(delivery_id.as_uuid())
  .fetch_optional(pool)
  .await
  .map_err(unavailable)?
  .ok_or(StoreError::NotFound {
    entity: EntityKind::Integration,
  })?;
  Ok(WebhookDeliveryDiagnostic {
    delivery_id,
    integration_id: IntegrationId::from_uuid(row.integration_id).map_err(|_| StoreError::Unavailable)?,
    state: delivery_state(&row.state)?,
    verification_attempts: u16::try_from(row.verification_attempts).map_err(|_| StoreError::Unavailable)?,
    trigger_attempts: u16::try_from(row.trigger_attempts).map_err(|_| StoreError::Unavailable)?,
    provider_delivery_id: row
      .provider_delivery_id
      .map(TriggerIdentity::new)
      .transpose()
      .map_err(|_| StoreError::Unavailable)?,
    failure_code: row.failure_code.as_deref().map(parse_failure_code).transpose()?,
    diagnostic: row.diagnostic,
    next_attempt_at: row.next_attempt_at_millis.map(timestamp).transpose()?,
  })
}
