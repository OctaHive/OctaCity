use octacity_server_domain::{BuildId, EntityKind, RetentionHoldVersion, Timestamp};
use octacity_server_store::{
  BuildResultRetentionState, BuildResultVisibility, BuildRetentionDeadlines, GetBuildResultRetention,
  MutationDisposition, PlaceBuildResultHold, ReleaseBuildResultHold, ReleaseBuildResultHoldError,
  RetentionAuditIdentity, RetentionHold, RetentionHoldMutationOutcome, RetentionHoldState, StoreError,
};
use serde::Serialize;
use serde_json::json;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
  database::unavailable,
  mutation::{MutationFacts, MutationIdentity, MutationKind, MutationStart, decode_outcome, encode_outcome},
};

const MANAGEMENT_ACTOR: &str = "unauthenticated_management";

#[derive(Serialize)]
struct PlaceFingerprint<'a> {
  build_id: BuildId,
  reason: &'a str,
  expires_at: Option<Timestamp>,
  actor_identity: Option<&'a str>,
}

#[derive(Serialize)]
struct ReleaseFingerprint<'a> {
  build_id: BuildId,
  expected_version: RetentionHoldVersion,
  actor_identity: Option<&'a str>,
}

#[derive(FromRow)]
struct BuildRetentionRow {
  build_id: Uuid,
  metadata_deadline_millis: i64,
  log_deadline_millis: i64,
  artifact_deadline_millis: i64,
  report_deadline_millis: i64,
  metadata_visible: bool,
  logs_visible: bool,
  artifacts_visible: bool,
  reports_visible: bool,
}

#[derive(FromRow)]
struct HoldRow {
  build_id: Uuid,
  version: i64,
  reason: String,
  created_at_millis: i64,
  expires_at_millis: Option<i64>,
  expired_at_millis: Option<i64>,
  released_at_millis: Option<i64>,
  actor_kind: String,
  actor_identity: Option<String>,
  request_identity: String,
  release_actor_kind: Option<String>,
  release_actor_identity: Option<String>,
  release_request_identity: Option<String>,
}

pub(crate) async fn read(
  pool: &PgPool,
  query: GetBuildResultRetention,
) -> Result<BuildResultRetentionState, StoreError> {
  let mut transaction = pool.begin().await.map_err(unavailable)?;
  sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
    .execute(&mut *transaction)
    .await
    .map_err(unavailable)?;
  let build = sqlx::query_as::<_, BuildRetentionRow>(build_state_sql(false))
    .bind(query.build_id.as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(unavailable)?
    .ok_or(StoreError::NotFound {
      entity: EntityKind::Build,
    })?;
  let hold = sqlx::query_as::<_, HoldRow>(latest_hold_sql(false))
    .bind(query.build_id.as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(unavailable)?;
  let state = project_state(build, hold, query.observed_at)?;
  transaction.commit().await.map_err(unavailable)?;
  Ok(state)
}

pub(crate) async fn place(
  pool: &PgPool,
  request: PlaceBuildResultHold,
) -> Result<RetentionHoldMutationOutcome, StoreError> {
  let identity = MutationIdentity::new_with_request_identity(
    MutationKind::PlaceBuildResultHold,
    request.idempotency_key.to_string(),
    request.request_identity.as_str().to_owned(),
    request.placed_at,
    EntityKind::RetentionHold,
    &PlaceFingerprint {
      build_id: request.build_id,
      reason: request.reason.as_str(),
      expires_at: request.expires_at,
      actor_identity: request.actor_identity.as_ref().map(|identity| identity.as_str()),
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return replay(outcome),
  };
  request.validate()?;
  let build = lock_build(&mut transaction, request.build_id).await?;
  if !visibility(&build).complete() {
    return Err(StoreError::Conflict {
      entity: EntityKind::RetentionHold,
    });
  }
  let latest = lock_latest_hold(&mut transaction, request.build_id).await?;
  if latest
    .as_ref()
    .is_some_and(|hold| hold_state(hold, request.placed_at) == RetentionHoldState::Active)
  {
    return Err(StoreError::Conflict {
      entity: EntityKind::RetentionHold,
    });
  }
  if latest
    .as_ref()
    .and_then(terminal_transition_millis)
    .is_some_and(|terminal_at| request.placed_at.unix_millis() < terminal_at)
  {
    return Err(StoreError::Conflict {
      entity: EntityKind::RetentionHold,
    });
  }
  let version = match latest {
    Some(hold) => retention_version(hold.version)?
      .next()
      .map_err(|_| StoreError::Unavailable)?,
    None => RetentionHoldVersion::INITIAL,
  };
  sqlx::query(
    "INSERT INTO build_result_retention_holds \
       (build_id, version, reason, created_at, expires_at, expired_at, released_at, actor_kind, actor_identity, \
        request_identity) \
     VALUES ($1, $2, $3, to_timestamp($4::double precision / 1000.0), \
       to_timestamp($5::double precision / 1000.0), NULL, NULL, $6, $7, $8)",
  )
  .bind(request.build_id.as_uuid())
  .bind(to_i64(version.get())?)
  .bind(request.reason.as_str())
  .bind(request.placed_at.unix_millis())
  .bind(request.expires_at.map(Timestamp::unix_millis))
  .bind(MANAGEMENT_ACTOR)
  .bind(request.actor_identity.as_ref().map(|identity| identity.as_str()))
  .bind(request.request_identity.as_str())
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let hold = HoldRow {
    build_id: request.build_id.as_uuid(),
    version: to_i64(version.get())?,
    reason: request.reason.into_string(),
    created_at_millis: request.placed_at.unix_millis(),
    expires_at_millis: request.expires_at.map(Timestamp::unix_millis),
    expired_at_millis: None,
    released_at_millis: None,
    actor_kind: MANAGEMENT_ACTOR.to_owned(),
    actor_identity: request.actor_identity.map(|identity| identity.into_string()),
    request_identity: request.request_identity.into_string(),
    release_actor_kind: None,
    release_actor_identity: None,
    release_request_identity: None,
  };
  let outcome = RetentionHoldMutationOutcome {
    disposition: MutationDisposition::Applied,
    retention: project_state(build, Some(hold), request.placed_at)?,
  };
  commit(
    transaction,
    &identity,
    &outcome,
    outcome
      .retention
      .hold
      .as_ref()
      .and_then(|hold| hold.creation_audit.actor_identity.clone()),
  )
  .await?;
  Ok(outcome)
}

pub(crate) async fn release(
  pool: &PgPool,
  request: ReleaseBuildResultHold,
) -> Result<RetentionHoldMutationOutcome, ReleaseBuildResultHoldError> {
  let identity = MutationIdentity::new_with_request_identity(
    MutationKind::ReleaseBuildResultHold,
    request.idempotency_key.to_string(),
    request.request_identity.as_str().to_owned(),
    request.released_at,
    EntityKind::RetentionHold,
    &ReleaseFingerprint {
      build_id: request.build_id,
      expected_version: request.expected_version,
      actor_identity: request.actor_identity.as_ref().map(|identity| identity.as_str()),
    },
  )?;
  let mut transaction = match crate::mutation::begin(pool, &identity).await? {
    MutationStart::Fresh(transaction) => transaction,
    MutationStart::Replay(outcome) => return Ok(replay(outcome)?),
  };
  let build = lock_build(&mut transaction, request.build_id).await?;
  let latest = lock_latest_hold(&mut transaction, request.build_id)
    .await?
    .ok_or(StoreError::NotFound {
      entity: EntityKind::RetentionHold,
    })?;
  let current_version = retention_version(latest.version)?;
  if current_version != request.expected_version {
    return Err(ReleaseBuildResultHoldError::PreconditionFailed);
  }
  if request.released_at.unix_millis() < latest.created_at_millis
    || hold_state(&latest, request.released_at) != RetentionHoldState::Active
  {
    return Err(
      StoreError::Conflict {
        entity: EntityKind::RetentionHold,
      }
      .into(),
    );
  }
  let version = current_version.next().map_err(|_| StoreError::Unavailable)?;
  sqlx::query(
    "INSERT INTO build_result_retention_holds \
       (build_id, version, reason, created_at, expires_at, expired_at, released_at, actor_kind, actor_identity, request_identity, \
        release_actor_kind, release_actor_identity, release_request_identity) \
     VALUES ($1, $2, $3, to_timestamp($4::double precision / 1000.0), \
       to_timestamp($5::double precision / 1000.0), NULL, to_timestamp($6::double precision / 1000.0), \
       $7, $8, $9, $10, $11, $12)",
  )
  .bind(request.build_id.as_uuid())
  .bind(to_i64(version.get())?)
  .bind(&latest.reason)
  .bind(latest.created_at_millis)
  .bind(latest.expires_at_millis)
  .bind(request.released_at.unix_millis())
  .bind(&latest.actor_kind)
  .bind(&latest.actor_identity)
  .bind(&latest.request_identity)
  .bind(MANAGEMENT_ACTOR)
  .bind(request.actor_identity.as_ref().map(|identity| identity.as_str()))
  .bind(request.request_identity.as_str())
  .execute(&mut *transaction)
  .await
  .map_err(unavailable)?;
  let hold = HoldRow {
    version: to_i64(version.get())?,
    released_at_millis: Some(request.released_at.unix_millis()),
    release_actor_kind: Some(MANAGEMENT_ACTOR.to_owned()),
    release_actor_identity: request
      .actor_identity
      .as_ref()
      .map(|identity| identity.as_str().to_owned()),
    release_request_identity: Some(request.request_identity.as_str().to_owned()),
    ..latest
  };
  let outcome = RetentionHoldMutationOutcome {
    disposition: MutationDisposition::Applied,
    retention: project_state(build, Some(hold), request.released_at)?,
  };
  commit(
    transaction,
    &identity,
    &outcome,
    request.actor_identity.map(|identity| identity.into_string()),
  )
  .await?;
  Ok(outcome)
}

async fn commit(
  transaction: Transaction<'_, Postgres>,
  identity: &MutationIdentity,
  outcome: &RetentionHoldMutationOutcome,
  actor_identity: Option<String>,
) -> Result<(), StoreError> {
  let hold = outcome.retention.hold.as_ref().ok_or(StoreError::Unavailable)?;
  crate::mutation::commit(
    transaction,
    identity,
    MutationFacts {
      actor_kind: MANAGEMENT_ACTOR,
      actor_identity,
      target_identity: outcome.retention.build_id.to_string(),
      safe_metadata: json!({
        "version": hold.version.get(),
        "state": hold.state,
        "expires_at": hold.expires_at.map(Timestamp::unix_millis),
      }),
      outbox_payload: json!({
        "build_id": outcome.retention.build_id,
        "version": hold.version.get(),
        "state": hold.state,
      }),
    },
    encode_outcome(outcome)?,
  )
  .await
}

fn replay(value: serde_json::Value) -> Result<RetentionHoldMutationOutcome, StoreError> {
  let mut outcome: RetentionHoldMutationOutcome = decode_outcome(value)?;
  outcome.disposition = MutationDisposition::Replayed;
  Ok(outcome)
}

async fn lock_build(
  transaction: &mut Transaction<'_, Postgres>,
  build_id: BuildId,
) -> Result<BuildRetentionRow, StoreError> {
  sqlx::query_as::<_, BuildRetentionRow>(build_state_sql(true))
    .bind(build_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(unavailable)?
    .ok_or(StoreError::NotFound {
      entity: EntityKind::Build,
    })
}

async fn lock_latest_hold(
  transaction: &mut Transaction<'_, Postgres>,
  build_id: BuildId,
) -> Result<Option<HoldRow>, StoreError> {
  sqlx::query_as::<_, HoldRow>(latest_hold_sql(true))
    .bind(build_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(unavailable)
}

fn project_state(
  build: BuildRetentionRow,
  hold: Option<HoldRow>,
  observed_at: Timestamp,
) -> Result<BuildResultRetentionState, StoreError> {
  let build_id = BuildId::from_uuid(build.build_id).map_err(|_| StoreError::Unavailable)?;
  Ok(BuildResultRetentionState {
    build_id,
    deadlines: BuildRetentionDeadlines {
      metadata: timestamp(build.metadata_deadline_millis)?,
      logs: timestamp(build.log_deadline_millis)?,
      artifacts: timestamp(build.artifact_deadline_millis)?,
      reports: timestamp(build.report_deadline_millis)?,
    },
    visibility: visibility(&build),
    hold: hold.map(|row| project_hold(row, observed_at)).transpose()?,
  })
}

fn project_hold(row: HoldRow, observed_at: Timestamp) -> Result<RetentionHold, StoreError> {
  let state = hold_state(&row, observed_at);
  Ok(RetentionHold {
    build_id: BuildId::from_uuid(row.build_id).map_err(|_| StoreError::Unavailable)?,
    version: retention_version(row.version)?,
    reason: row.reason,
    created_at: timestamp(row.created_at_millis)?,
    expires_at: row.expires_at_millis.map(timestamp).transpose()?,
    released_at: row.released_at_millis.map(timestamp).transpose()?,
    state,
    creation_audit: RetentionAuditIdentity {
      actor_kind: row.actor_kind,
      actor_identity: row.actor_identity,
      request_identity: row.request_identity,
    },
    release_audit: match (row.release_actor_kind, row.release_request_identity) {
      (Some(actor_kind), Some(request_identity)) => Some(RetentionAuditIdentity {
        actor_kind,
        actor_identity: row.release_actor_identity,
        request_identity,
      }),
      (None, None) => None,
      _ => return Err(StoreError::Unavailable),
    },
  })
}

fn hold_state(row: &HoldRow, observed_at: Timestamp) -> RetentionHoldState {
  if row.released_at_millis.is_some() {
    RetentionHoldState::Released
  } else if row.expired_at_millis.is_some()
    || row
      .expires_at_millis
      .is_some_and(|expiry| expiry <= observed_at.unix_millis())
  {
    RetentionHoldState::Expired
  } else {
    RetentionHoldState::Active
  }
}

const fn visibility(row: &BuildRetentionRow) -> BuildResultVisibility {
  BuildResultVisibility {
    metadata: row.metadata_visible,
    logs: row.logs_visible,
    artifacts: row.artifacts_visible,
    reports: row.reports_visible,
  }
}

const fn build_state_sql(for_update: bool) -> &'static str {
  if for_update {
    "SELECT id AS build_id, FLOOR(EXTRACT(EPOCH FROM metadata_retention_until) * 1000)::BIGINT AS metadata_deadline_millis, \
       FLOOR(EXTRACT(EPOCH FROM log_retention_until) * 1000)::BIGINT AS log_deadline_millis, \
       FLOOR(EXTRACT(EPOCH FROM artifact_retention_until) * 1000)::BIGINT AS artifact_deadline_millis, \
       FLOOR(EXTRACT(EPOCH FROM report_retention_until) * 1000)::BIGINT AS report_deadline_millis, \
       metadata_visible, logs_visible, artifacts_visible, reports_visible FROM builds WHERE id = $1 FOR UPDATE"
  } else {
    "SELECT id AS build_id, FLOOR(EXTRACT(EPOCH FROM metadata_retention_until) * 1000)::BIGINT AS metadata_deadline_millis, \
       FLOOR(EXTRACT(EPOCH FROM log_retention_until) * 1000)::BIGINT AS log_deadline_millis, \
       FLOOR(EXTRACT(EPOCH FROM artifact_retention_until) * 1000)::BIGINT AS artifact_deadline_millis, \
       FLOOR(EXTRACT(EPOCH FROM report_retention_until) * 1000)::BIGINT AS report_deadline_millis, \
       metadata_visible, logs_visible, artifacts_visible, reports_visible FROM builds WHERE id = $1"
  }
}

const fn latest_hold_sql(for_update: bool) -> &'static str {
  if for_update {
    "SELECT build_id, version, reason, FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM expires_at) * 1000)::BIGINT AS expires_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM expired_at) * 1000)::BIGINT AS expired_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM released_at) * 1000)::BIGINT AS released_at_millis, actor_kind, actor_identity, \
       request_identity, release_actor_kind, release_actor_identity, release_request_identity \
       FROM build_result_retention_holds WHERE build_id = $1 ORDER BY version DESC LIMIT 1 FOR UPDATE"
  } else {
    "SELECT build_id, version, reason, FLOOR(EXTRACT(EPOCH FROM created_at) * 1000)::BIGINT AS created_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM expires_at) * 1000)::BIGINT AS expires_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM expired_at) * 1000)::BIGINT AS expired_at_millis, \
       FLOOR(EXTRACT(EPOCH FROM released_at) * 1000)::BIGINT AS released_at_millis, actor_kind, actor_identity, \
       request_identity, release_actor_kind, release_actor_identity, release_request_identity \
       FROM build_result_retention_holds WHERE build_id = $1 ORDER BY version DESC LIMIT 1"
  }
}

/// Returns whether the latest hold is active and durably versions a due expiry.
pub(crate) async fn active_for_retention(
  transaction: &mut Transaction<'_, Postgres>,
  build_id: BuildId,
  observed_at: Timestamp,
) -> Result<bool, StoreError> {
  let Some(latest) = lock_latest_hold(transaction, build_id).await? else {
    return Ok(false);
  };
  if latest.released_at_millis.is_some() || latest.expired_at_millis.is_some() {
    return Ok(false);
  }
  let Some(expires_at) = latest.expires_at_millis else {
    return Ok(true);
  };
  if expires_at > observed_at.unix_millis() {
    return Ok(true);
  }

  let version = retention_version(latest.version)?
    .next()
    .map_err(|_| StoreError::Unavailable)?;
  sqlx::query(
    "INSERT INTO build_result_retention_holds \
       (build_id, version, reason, created_at, expires_at, expired_at, released_at, actor_kind, actor_identity, \
        request_identity, release_actor_kind, release_actor_identity, release_request_identity) \
     VALUES ($1, $2, $3, to_timestamp($4::double precision / 1000.0), \
       to_timestamp($5::double precision / 1000.0), to_timestamp($5::double precision / 1000.0), NULL, \
       $6, $7, $8, NULL, NULL, NULL)",
  )
  .bind(latest.build_id)
  .bind(to_i64(version.get())?)
  .bind(&latest.reason)
  .bind(latest.created_at_millis)
  .bind(expires_at)
  .bind(&latest.actor_kind)
  .bind(&latest.actor_identity)
  .bind(&latest.request_identity)
  .execute(&mut **transaction)
  .await
  .map_err(unavailable)?;
  Ok(false)
}

fn terminal_transition_millis(row: &HoldRow) -> Option<i64> {
  row.released_at_millis.or(row.expired_at_millis)
}

fn retention_version(value: i64) -> Result<RetentionHoldVersion, StoreError> {
  u64::try_from(value)
    .ok()
    .and_then(|value| RetentionHoldVersion::new(value).ok())
    .ok_or(StoreError::Unavailable)
}

fn timestamp(value: i64) -> Result<Timestamp, StoreError> {
  Timestamp::from_unix_millis(value).map_err(|_| StoreError::Unavailable)
}

fn to_i64(value: u64) -> Result<i64, StoreError> {
  i64::try_from(value).map_err(|_| StoreError::Unavailable)
}
