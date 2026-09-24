use super::*;
use crate::{GetBuildResultRetentionQuery, PlaceBuildResultHoldCommand, ReleaseBuildResultHoldCommand};
use octacity_server_store::{RetentionHoldReason, RetentionRequestIdentity};

impl ManagementInputFactory {
  /// Creates a typed Build Result retention query.
  pub fn get_build_result_retention(
    &self,
    build_id: &str,
    observed_at_unix_ms: i64,
  ) -> Result<GetBuildResultRetentionQuery, ManagementInputError> {
    Ok(GetBuildResultRetentionQuery {
      build_id: parse(build_id, "build id")?,
      observed_at: timestamp(observed_at_unix_ms)?,
    })
  }

  /// Creates a typed permanent or time-bounded Build Result hold command.
  pub fn place_build_result_hold(
    &self,
    build_id: &str,
    reason: String,
    expires_at_unix_ms: Option<i64>,
    request_identity: String,
    idempotency_key: &str,
    placed_at_unix_ms: i64,
  ) -> Result<PlaceBuildResultHoldCommand, ManagementInputError> {
    let placed_at = timestamp(placed_at_unix_ms)?;
    let expires_at = expires_at_unix_ms.map(timestamp).transpose()?;
    Ok(PlaceBuildResultHoldCommand {
      build_id: parse(build_id, "build id")?,
      reason: RetentionHoldReason::new(reason)
        .map_err(|_| ManagementInputError::Invalid("Build Result retention hold"))?,
      expires_at,
      actor_identity: None,
      request_identity: RetentionRequestIdentity::new(request_identity)
        .map_err(|_| ManagementInputError::Invalid("Build Result retention hold"))?,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      placed_at,
    })
  }

  /// Creates a typed Build Result hold release command.
  pub fn release_build_result_hold(
    &self,
    build_id: &str,
    expected_version: u64,
    request_identity: String,
    idempotency_key: &str,
    released_at_unix_ms: i64,
  ) -> Result<ReleaseBuildResultHoldCommand, ManagementInputError> {
    Ok(ReleaseBuildResultHoldCommand {
      build_id: parse(build_id, "build id")?,
      expected_version: version(expected_version, "retention hold version")?,
      actor_identity: None,
      request_identity: RetentionRequestIdentity::new(request_identity)
        .map_err(|_| ManagementInputError::Invalid("Build Result retention release"))?,
      idempotency_key: parse(idempotency_key, "idempotency key")?,
      released_at: timestamp(released_at_unix_ms)?,
    })
  }
}
