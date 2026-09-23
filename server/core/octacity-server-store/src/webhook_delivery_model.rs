use std::{collections::BTreeMap, fmt, num::NonZeroU16, str::FromStr};

use octacity_server_domain::{
  ImmutableRevision, IntegrationId, RepositoryId, SourceReference, Timestamp, TriggerIdentity,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use uuid::Uuid;

use crate::{StoreError, StoreInputError, StoreOperation, TriggerEventKind, WorkerOwner};

/// Maximum webhook deliveries acquired by one durable worker pass.
pub const MAX_WEBHOOK_DELIVERY_BATCH_SIZE: u16 = 100;
/// Maximum raw body bytes retained until authentication finishes.
pub const MAX_STORED_WEBHOOK_BODY_BYTES: usize = 1024 * 1024;
/// Maximum safe diagnostic bytes retained for one dead letter.
pub const MAX_WEBHOOK_DIAGNOSTIC_BYTES: usize = 4096;
/// Maximum opaque metadata entries retained from one authenticated event.
pub const MAX_WEBHOOK_METADATA_ENTRIES: usize = 32;
/// Maximum UTF-8 bytes in one normalized metadata key.
pub const MAX_WEBHOOK_METADATA_NAME_BYTES: usize = 256;
/// Maximum bytes in one normalized metadata value or actor display name.
pub const MAX_WEBHOOK_VALUE_BYTES: usize = 4096;
/// Maximum UTF-8 bytes retained for one allowlisted transport header value.
pub const MAX_WEBHOOK_HEADER_VALUE_BYTES: usize = 4096;

/// Server-owned identity of one raw webhook receipt.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WebhookDeliveryId(Uuid);

impl WebhookDeliveryId {
  /// Generates a fresh receipt identity at the ingress boundary.
  #[must_use]
  pub fn generate() -> Self {
    Self(Uuid::new_v4())
  }

  /// Constructs a receipt identity from a non-nil UUID.
  pub fn from_uuid(value: Uuid) -> Result<Self, StoreInputError> {
    if value.is_nil() {
      Err(StoreInputError::InvalidWebhookDelivery)
    } else {
      Ok(Self(value))
    }
  }

  /// Returns the UUID representation used by persistence adapters.
  #[must_use]
  pub const fn as_uuid(self) -> Uuid {
    self.0
  }
}

impl fmt::Display for WebhookDeliveryId {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(formatter, "{}", self.0.hyphenated())
  }
}

impl FromStr for WebhookDeliveryId {
  type Err = StoreInputError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    let parsed = Uuid::parse_str(value).map_err(|_| StoreInputError::InvalidWebhookDelivery)?;
    if parsed.hyphenated().to_string() != value {
      return Err(StoreInputError::InvalidWebhookDelivery);
    }
    Self::from_uuid(parsed)
  }
}

impl Serialize for WebhookDeliveryId {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.collect_str(self)
  }
}

impl<'de> Deserialize<'de> for WebhookDeliveryId {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    String::deserialize(deserializer)?.parse().map_err(D::Error::custom)
  }
}

/// Exact allowlisted raw delivery durably admitted by public ingress.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnqueueWebhookDelivery {
  /// Server-generated receipt identity.
  pub delivery_id: WebhookDeliveryId,
  /// Callback integration selected by the route.
  pub integration_id: IntegrationId,
  /// Only the configured verification headers.
  pub headers: BTreeMap<String, String>,
  /// Exact bounded request bytes.
  pub body: Vec<u8>,
  /// Server observation time.
  pub received_at: Timestamp,
}

impl EnqueueWebhookDelivery {
  /// Revalidates persisted raw data at the store boundary.
  pub fn validate(&self) -> Result<(), StoreError> {
    let headers_valid = !self.headers.is_empty()
      && self.headers.len() <= crate::MAX_WEBHOOK_VERIFICATION_HEADERS
      && self.headers.iter().all(|(name, value)| {
        !name.is_empty()
          && name.len() <= crate::MAX_WEBHOOK_HEADER_NAME_BYTES
          && value.len() <= MAX_WEBHOOK_HEADER_VALUE_BYTES
          && !value.chars().any(char::is_control)
      });
    if !headers_valid || self.body.len() > MAX_STORED_WEBHOOK_BODY_BYTES {
      return Err(StoreError::invalid(
        StoreOperation::EnqueueWebhookDelivery,
        StoreInputError::InvalidWebhookDelivery,
      ));
    }
    Ok(())
  }
}

/// Bounded request to acquire due webhook delivery work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimWebhookDeliveries {
  /// Process instance acquiring work.
  pub owner: WorkerOwner,
  /// Authoritative time used for due work and stale claims.
  pub observed_at: Timestamp,
  /// Exclusive deadline after which another replica may reclaim work.
  pub claim_expires_at: Timestamp,
  /// Positive bounded number of receipts.
  pub limit: NonZeroU16,
}

impl ClaimWebhookDeliveries {
  /// Validates the ownership window and batch bound.
  pub fn new(
    owner: WorkerOwner,
    observed_at: Timestamp,
    claim_expires_at: Timestamp,
    limit: u16,
  ) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit)
      .filter(|value| value.get() <= MAX_WEBHOOK_DELIVERY_BATCH_SIZE)
      .ok_or_else(|| {
        StoreError::invalid(
          StoreOperation::ClaimWebhookDeliveries,
          StoreInputError::InvalidWorkerClaim,
        )
      })?;
    if claim_expires_at <= observed_at {
      return Err(StoreError::invalid(
        StoreOperation::ClaimWebhookDeliveries,
        StoreInputError::InvalidWorkerClaim,
      ));
    }
    Ok(Self {
      owner,
      observed_at,
      claim_expires_at,
      limit,
    })
  }
}

/// Current processing phase of a claimed receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebhookDeliveryWork {
  /// The raw request still needs adapter authentication and normalization.
  Verify {
    /// Allowlisted transport headers.
    headers: BTreeMap<String, String>,
    /// Exact request body.
    body: Vec<u8>,
  },
  /// Authentication completed and Trigger evaluation remains outstanding.
  Trigger {
    /// Durable provider-neutral event.
    event: NormalizedWebhookEvent,
  },
}

/// One webhook receipt exclusively owned until its claim deadline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebhookDeliveryClaim {
  /// Server-owned receipt identity.
  pub delivery_id: WebhookDeliveryId,
  /// Integration selected at ingress.
  pub integration_id: IntegrationId,
  /// Server receipt time preserved for Trigger causality.
  pub received_at: Timestamp,
  /// Number of adapter verification attempts including this claim.
  pub verification_attempt: u16,
  /// Number of Trigger-evaluation attempts including this claim.
  pub trigger_attempt: u16,
  /// Work phase selected from durable state.
  pub work: WebhookDeliveryWork,
  /// Claim owner required by subsequent mutations.
  pub owner: WorkerOwner,
  /// Exclusive claim deadline.
  pub claim_expires_at: Timestamp,
}

/// Authenticated provider-neutral repository event retained durably.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NormalizedWebhookEvent {
  /// Integration that authenticated the provider delivery.
  pub integration_id: IntegrationId,
  /// Provider delivery identity scoped to the integration.
  pub provider_delivery_id: TriggerIdentity,
  /// Provider-neutral event kind.
  pub event_kind: TriggerEventKind,
  /// Repository named by the authenticated event.
  pub repository_id: RepositoryId,
  /// Optional mutable source reference.
  pub reference: Option<SourceReference>,
  /// Optional immutable revision.
  pub revision: Option<ImmutableRevision>,
  /// Optional provider observation time.
  pub provider_time: Option<Timestamp>,
  /// Optional display-only actor name.
  pub actor_display_name: Option<String>,
  /// Bounded secret-free provider metadata.
  pub metadata: BTreeMap<String, String>,
}

impl NormalizedWebhookEvent {
  /// Revalidates bounded normalized provider data before persistence.
  pub fn validate(&self) -> Result<(), StoreInputError> {
    if self.metadata.len() > MAX_WEBHOOK_METADATA_ENTRIES
      || self.metadata.iter().any(|(name, value)| {
        name.is_empty()
          || name.len() > MAX_WEBHOOK_METADATA_NAME_BYTES
          || name.chars().any(char::is_control)
          || value.len() > MAX_WEBHOOK_VALUE_BYTES
          || value.chars().any(char::is_control)
      })
      || self.actor_display_name.as_ref().is_some_and(|value| {
        value.is_empty() || value.len() > MAX_WEBHOOK_VALUE_BYTES || value.chars().any(char::is_control)
      })
    {
      return Err(StoreInputError::InvalidWebhookDelivery);
    }
    Ok(())
  }
}

/// Outcome of committing an authenticated normalized event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordWebhookEventOutcome {
  /// This receipt became the canonical Trigger work item.
  Recorded,
  /// The same normalized provider identity was already recorded.
  Duplicate,
  /// The provider reused one delivery identity for different normalized data.
  IdentityConflict,
}

/// Commits a normalized event while completing the adapter phase.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordWebhookEvent {
  /// Claimed raw receipt.
  pub delivery_id: WebhookDeliveryId,
  /// Claim owner returned by the store.
  pub owner: WorkerOwner,
  /// Authenticated normalized event.
  pub event: NormalizedWebhookEvent,
  /// Authoritative completion time for the adapter phase.
  pub recorded_at: Timestamp,
}

impl RecordWebhookEvent {
  /// Revalidates the authenticated event at the persistence seam.
  pub fn validate(&self) -> Result<(), StoreError> {
    self.event.validate().map_err(|source| StoreError::InvalidInput {
      operation: StoreOperation::RecordWebhookEvent,
      source,
    })
  }
}

/// Stable secret-free reason retained for a failed adapter attempt.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookFailureCode {
  /// Provider proof did not authenticate.
  AuthenticationFailed,
  /// Installed adapter or immutable integration configuration is invalid.
  InvalidConfiguration,
  /// Adapter operation is not supported.
  Unsupported,
  /// Provider rejected the operation permanently.
  Permanent,
  /// Provider operation was cooperatively cancelled before completion.
  Cancelled,
  /// Adapter dependency was temporarily unavailable.
  Unavailable,
  /// Adapter returned a malformed or mismatched response.
  InvalidResponse,
  /// Authenticated event conflicts with immutable integration configuration.
  NormalizedEventMismatch,
  /// A provider identity was reused for different normalized data.
  DeliveryIdentityConflict,
  /// A normalized event could not be evaluated into a Build.
  TriggerEvaluationFailed,
}

/// Records retry scheduling or final dead-letter state for one owned receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FailWebhookDelivery {
  /// Claimed raw receipt.
  pub delivery_id: WebhookDeliveryId,
  /// Claim owner returned by the store.
  pub owner: WorkerOwner,
  /// Stable safe failure class.
  pub code: WebhookFailureCode,
  /// Bounded secret-free diagnostic.
  pub diagnostic: String,
  /// Authoritative failure time.
  pub failed_at: Timestamp,
  /// Next verification time for a transient failure; `None` creates a dead letter.
  pub retry_at: Option<Timestamp>,
}

impl FailWebhookDelivery {
  /// Revalidates the safe diagnostic and optional future retry time.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.diagnostic.is_empty()
      || self.diagnostic.len() > MAX_WEBHOOK_DIAGNOSTIC_BYTES
      || self.diagnostic.chars().any(char::is_control)
      || self.retry_at.is_some_and(|retry_at| retry_at <= self.failed_at)
    {
      return Err(StoreError::invalid(
        StoreOperation::FailWebhookDelivery,
        StoreInputError::InvalidWebhookDelivery,
      ));
    }
    Ok(())
  }
}

/// Marks one normalized receipt complete after idempotent Trigger evaluation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteWebhookDelivery {
  /// Canonical normalized receipt.
  pub delivery_id: WebhookDeliveryId,
  /// Claim owner returned by the store.
  pub owner: WorkerOwner,
  /// Authoritative completion time.
  pub completed_at: Timestamp,
}

/// Terminates one owned receipt because its integration was disabled.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuppressWebhookDelivery {
  /// Receipt that must not reach provider verification or Trigger evaluation.
  pub delivery_id: WebhookDeliveryId,
  /// Claim owner returned by the store.
  pub owner: WorkerOwner,
  /// Authoritative suppression time.
  pub suppressed_at: Timestamp,
}

/// Secret-free durable diagnostic projection for one receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebhookDeliveryDiagnostic {
  /// Server-owned receipt identity.
  pub delivery_id: WebhookDeliveryId,
  /// Callback integration identity.
  pub integration_id: IntegrationId,
  /// Current durable lifecycle state.
  pub state: WebhookDeliveryState,
  /// Number of adapter verification attempts.
  pub verification_attempts: u16,
  /// Number of normalized Trigger-evaluation attempts.
  pub trigger_attempts: u16,
  /// Authenticated provider identity, when available.
  pub provider_delivery_id: Option<TriggerIdentity>,
  /// Last stable failure classification, when present.
  pub failure_code: Option<WebhookFailureCode>,
  /// Last bounded safe diagnostic, when present.
  pub diagnostic: Option<String>,
  /// Next retry time for transient adapter failure.
  pub next_attempt_at: Option<Timestamp>,
}

/// Durable lifecycle state exposed by delivery diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebhookDeliveryState {
  /// Waiting for adapter authentication.
  PendingVerification,
  /// Waiting for a bounded transient retry.
  RetryScheduled,
  /// Normalized and waiting for Trigger evaluation.
  PendingTrigger,
  /// Completed or identified as an exact duplicate.
  Completed,
  /// Intentionally discarded because the selected integration is disabled.
  Suppressed,
  /// Permanently failed with safe retained diagnostics.
  DeadLetter,
}
