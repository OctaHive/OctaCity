//! Accepted Issue projections reuse Reporter policy, worker claims and native ordering.
use crate::{ApplicationError, FactoryReporterRetryPolicy};
use octacity_server_domain::Timestamp;
use octacity_server_factory::{BudgetLimit, FactoryClaim, FactoryDigest};
use octacity_server_store::{FactoryOutboxState, IssuePushProjection};

/// Immutable installed delivery bounds; profile replacement changes the writer binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueReporterDeliveryPolicy {
  /// Existing finite outbox retry contract.
  pub retry: FactoryReporterRetryPolicy,
  /// Total time from acceptance, never renewed by a retry or process restart.
  pub window_millis: u64,
  /// Maximum resources for each independently fenced provider dispatch.
  pub budget: BudgetLimit,
}
impl IssueReporterDeliveryPolicy {
  /// Hard total delivery window ceiling, independent of a dispatch claim or retry delay.
  pub const MAX_WINDOW_MILLIS: u64 = 3_600_000;
  /// Validates explicit installed bounds without authorizing a provider call.
  pub fn validate(self) -> Result<(), ApplicationError> {
    self.retry.validate()?;
    if self.window_millis == 0 || self.window_millis > Self::MAX_WINDOW_MILLIS {
      return Err(ApplicationError::invalid());
    }
    Ok(())
  }
  /// Original expiry derived from immutable acceptance and profile policy.
  pub fn deadline(self, accepted_at: Timestamp) -> Result<Timestamp, ApplicationError> {
    self.validate()?;
    accepted_at
      .unix_millis()
      .checked_add(self.window_millis as i64)
      .and_then(|n| Timestamp::from_unix_millis(n).ok())
      .ok_or_else(ApplicationError::invalid)
  }
}
/// An independently claimed accepted metadata effect; no execution authority is supplied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssueStatusProjectionRequest {
  /// Original owner used to partition the native ordering bucket.
  pub project_id: octacity_server_domain::ProjectId,
  /// Stable original logical effect.
  pub operation_id: FactoryDigest,
  /// Live Reporter worker ownership.
  pub claim: FactoryClaim,
  /// Original installed resource ceiling.
  pub budget: BudgetLimit,
  /// Minimum of original policy expiry and this claim's expiry.
  pub deadline: Timestamp,
}
/// Checks accepted publication and current outbox ownership before constructing a request.
pub fn accepted_issue_status_request(
  projection: &IssuePushProjection,
  policy: IssueReporterDeliveryPolicy,
  at: Timestamp,
) -> Result<IssueStatusProjectionRequest, ApplicationError> {
  let row = &projection.record;
  let claim = row.claim.ok_or_else(ApplicationError::invalid)?;
  let original = projection
    .intent
    .projection_record(projection.accepted_at)
    .ok_or_else(ApplicationError::invalid)?;
  let deadline = policy.deadline(projection.accepted_at)?.min(claim.expires_at());
  if row.attempt >= policy.retry.max_attempts || at >= deadline {
    return Err(ApplicationError::ReporterRejected(
      octacity_server_store::FactoryOutboxFailure::Exhausted,
    ));
  }
  if row.state != FactoryOutboxState::Claimed
    || row.subject != original.subject
    || row.operation_id != original.operation_id
    || row.input_digest != original.input_digest
    || row.kind != original.kind
    || claim.verify_fence(claim.fence(), at).is_err()
  {
    return Err(ApplicationError::invalid());
  }
  Ok(IssueStatusProjectionRequest {
    project_id: projection.intent.project_id,
    operation_id: row.operation_id,
    claim,
    budget: policy.budget,
    deadline,
  })
}
