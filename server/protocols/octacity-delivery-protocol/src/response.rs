use crate::*;
use serde::{Deserialize, Serialize};

/// Correlated secret-free provider observation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
  /// Exact negotiated version.
  pub protocol_version: u16,
  /// Exact original process request correlation.
  pub request_id: String,
  /// Qualified observation or classified failure, never a Flow transition.
  pub outcome: Outcome,
}
/// Delivery facts; successful publication does not imply merge or promotion.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", content = "payload", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
  /// No branch or review exists for this exact delivery. Observe performs no mutation.
  Absent,
  /// Exact independently observed branch/review facts.
  Observed(Box<DeliveryReceipt>),
  /// Owned review exists but requires metadata repair before publication is complete.
  RepairRequired(Box<DeliveryReceipt>),
  /// Original operation stopped; no absent publication was created.
  Acknowledged {
    /// Original delivery identity.
    delivery_id: String,
  },
  /// Closed classified failure with no raw provider diagnostic or private URL.
  Failure(Failure),
}
/// Bounded provider-independent failure vocabulary for retry/control policy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
  /// Invalid scope or content, rejected before mutation.
  InvalidRequest,
  /// Installation cannot satisfy this capability.
  Unsupported,
  /// Accepted candidate, ancestry or remote identity conflicts.
  Conflict,
  /// Credential or installation policy requires operator intervention.
  Permanent,
  /// Remote unavailability or unknown response; observe before retry.
  Transient,
  /// Original operation was cancelled.
  Cancelled,
  /// Framing or normalized provider response violated its contract.
  ProtocolFault,
}
/// Failure projection intentionally has no free-form diagnostic field.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Failure {
  /// Provider-independent classification.
  pub class: FailureClass,
  /// Optional retry hint; consuming policy still enforces its own bounds.
  pub retry_after_millis: Option<u32>,
}
/// External review state observed from the provider, not inferred from a model.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewState {
  /// Candidate has been delivered for human review.
  Open,
  /// Review was closed externally without merging this candidate.
  Closed,
  /// External merge was observed; this protocol cannot request a merge.
  MergedExternally,
}
/// Bounded external review identity; private provider URLs never cross this seam.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewReference {
  /// Opaque provider identity, not a URL.
  pub identity: String,
  /// Externally observed state.
  pub state: ReviewState,
  /// Exact remote review head.
  pub head: String,
  /// Exact observed review base; external merge may advance it beyond the accepted base.
  pub base: String,
}
/// Exact publication checkpoint suitable for an append-only node result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryReceipt {
  /// Stable original logical delivery identity.
  pub delivery_id: String,
  /// Exact original repository scope.
  pub repository_id: String,
  /// Exact accepted original base.
  pub base_revision: String,
  /// Exact accepted candidate.
  pub candidate_revision: String,
  /// Exact captured ChangeSet manifest identity.
  pub changeset_digest: String,
  /// Exact accepted Decision/control proof.
  pub decision_digest: String,
  /// Immutable operator provider profile, excluding rotated credential contents.
  pub configuration_digest: String,
  /// Original pinned target policy.
  pub target: TargetPolicy,
  /// Actual published candidate branch, distinct from review state.
  pub branch_reference: String,
  /// Actual current remote branch head.
  pub remote_head: String,
  /// Existing review, or None for an observed partially completed delivery.
  pub review: Option<ReviewReference>,
}
impl Response {
  /// Checks exact scope and typed outcomes before the owner accepts external facts.
  pub fn validate_for(&self, request: &Request) -> Result<(), ProtocolError> {
    request.validate()?;
    common::require_correlation(
      self.protocol_version,
      &self.request_id,
      request.protocol_version,
      &request.request_id,
    )
    .map_err(|_| ProtocolError::Invalid)?;
    match (&request.command, &self.outcome) {
      (_, Outcome::Failure(failure)) if failure.retry_after_millis.is_none_or(|delay| delay <= 3_600_000) => Ok(()),
      (Command::Observe(_), Outcome::Absent) => Ok(()),
      (Command::Observe(intent), Outcome::Observed(receipt)) => receipt.validate_for(intent),
      (Command::Observe(intent), Outcome::RepairRequired(receipt)) => {
        receipt.validate_for(intent)?;
        if receipt
          .review
          .as_ref()
          .is_none_or(|review| review.state != ReviewState::Open)
        {
          return Err(ProtocolError::Invalid);
        }
        Ok(())
      }
      (Command::Publish(intent), Outcome::Observed(receipt)) => {
        receipt.validate_for(intent)?;
        if receipt
          .review
          .as_ref()
          .is_none_or(|review| review.state == ReviewState::Closed)
        {
          return Err(ProtocolError::Invalid);
        }
        Ok(())
      }
      (Command::Cancel { target_delivery_id }, Outcome::Acknowledged { delivery_id })
        if delivery_id == target_delivery_id =>
      {
        Ok(())
      }
      _ => Err(ProtocolError::Invalid),
    }
  }
}
impl DeliveryReceipt {
  /// Validates actual head and stable identities without silently substituting a candidate.
  pub fn validate_for(&self, intent: &DeliveryIntent) -> Result<(), ProtocolError> {
    intent.validate()?;
    if self.delivery_id != intent.delivery_id
      || self.repository_id != intent.repository.repository_id
      || self.base_revision != intent.base_revision
      || self.candidate_revision != intent.candidate_revision
      || self.changeset_digest != intent.changeset_digest
      || self.configuration_digest != intent.configuration_digest
      || self.decision_digest != intent.decision_digest
      || self.target != intent.target
      || self.branch_reference != intent.target.candidate_reference
      || self.remote_head != intent.candidate_revision
    {
      return Err(ProtocolError::Invalid);
    }
    if let Some(review) = &self.review {
      field(&review.identity)?;
      field(&review.base)?;
      if !review
        .identity
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte))
        || review.head != intent.candidate_revision
        || (review.state != ReviewState::MergedExternally && review.base != intent.base_revision)
      {
        return Err(ProtocolError::Invalid);
      }
    }
    Ok(())
  }
}
