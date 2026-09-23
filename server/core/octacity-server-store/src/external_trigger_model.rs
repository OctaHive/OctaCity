use std::{
  collections::{BTreeMap, BTreeSet},
  num::NonZeroU16,
};

use octacity_server_domain::{IntegrationId, RepositoryId, Timestamp};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
  CreateTriggerDefinition, IdempotencyKey, MutationDisposition, StoreError, StoreInputError, StoreOperation,
  TriggerDefinitionRef, TriggerEventKind, TriggerKind, TriggerTarget, WebhookFailureCode, WorkerOwner,
  model::require_bounded_json_object,
};

/// Maximum managed provider operations acquired by one worker pass.
pub const MAX_MANAGED_WEBHOOK_OPERATION_BATCH_SIZE: u16 = 32;

/// Maximum number of transport headers an integration may expose to its verifier.
pub const MAX_WEBHOOK_VERIFICATION_HEADERS: usize = 32;
/// Maximum UTF-8 bytes in an installed webhook adapter identity.
pub const MAX_WEBHOOK_ADAPTER_ID_BYTES: usize = 256;
/// Maximum UTF-8 bytes in a protected webhook material handle.
pub const MAX_WEBHOOK_MATERIAL_HANDLE_BYTES: usize = 256;
/// Maximum ASCII bytes in one allowlisted webhook header name.
pub const MAX_WEBHOOK_HEADER_NAME_BYTES: usize = 256;
/// Maximum UTF-8 bytes in a provider-owned registration identity.
pub const MAX_WEBHOOK_REGISTRATION_ID_BYTES: usize = 256;
/// Maximum UTF-8 bytes in a provider callback URL.
pub const MAX_WEBHOOK_CALLBACK_URL_BYTES: usize = 4096;
/// Exact lowercase hexadecimal length of a SHA-256 executable digest.
pub const WEBHOOK_ADAPTER_SHA256_BYTES: usize = 64;

/// Validates and canonicalizes caller-supplied webhook header names.
pub fn canonical_webhook_headers(headers: Vec<String>) -> Result<BTreeSet<String>, StoreInputError> {
  if headers.is_empty() || headers.len() > MAX_WEBHOOK_VERIFICATION_HEADERS {
    return Err(StoreInputError::InvalidWebhookDefinition);
  }
  let original_len = headers.len();
  let headers = headers.into_iter().collect::<BTreeSet<_>>();
  if headers.len() != original_len || headers.iter().any(|name| !valid_webhook_header_name(name)) {
    return Err(StoreInputError::InvalidWebhookDefinition);
  }
  Ok(headers)
}

/// Immutable provider-neutral delivery configuration shared by unmanaged and managed integrations.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UnmanagedWebhookDefinition {
  /// Operator-installed adapter identity.
  pub adapter_id: String,
  /// Lowercase SHA-256 pin for the selected adapter executable.
  pub adapter_sha256: String,
  /// Logical protected-material handle resolved only by the adapter host.
  pub verification_material_handle: String,
  /// Lowercase transport header names forwarded to the verifier.
  pub verification_headers: BTreeSet<String>,
  /// Repository identity an authenticated normalized event must name.
  pub repository_id: RepositoryId,
  /// Provider-neutral event kind this Trigger accepts.
  pub event_kind: TriggerEventKind,
  /// Build parameters supplied after successful authentication.
  pub parameters: BTreeMap<String, Value>,
  /// Durable ready-queue priority copied to materialized root Jobs.
  pub priority: i64,
}

/// Protected configuration used only for managed provider administration.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedWebhookDefinition {
  /// Delivery authentication and Trigger matching configuration.
  pub delivery: UnmanagedWebhookDefinition,
  /// Host-owned handle for provider administration credentials.
  pub administration_credential_handle: String,
}

impl std::fmt::Debug for ManagedWebhookDefinition {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("ManagedWebhookDefinition")
      .field("delivery", &self.delivery)
      .field("administration_credential_handle", &"<redacted>")
      .finish()
  }
}

impl ManagedWebhookDefinition {
  /// Revalidates both delivery configuration and the protected administration handle.
  pub fn validate(&self) -> Result<(), StoreInputError> {
    self.delivery.validate()?;
    if self.administration_credential_handle.is_empty()
      || self.administration_credential_handle.len() > MAX_WEBHOOK_MATERIAL_HANDLE_BYTES
      || self.administration_credential_handle.trim() != self.administration_credential_handle
      || self.administration_credential_handle.chars().any(char::is_control)
    {
      return Err(StoreInputError::InvalidWebhookDefinition);
    }
    Ok(())
  }
}

/// Provider-neutral state of one managed remote registration.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedWebhookRegistrationStatus {
  /// Remote registration exists and is enabled.
  Active,
  /// Remote registration exists but is disabled.
  Disabled,
  /// Remote registration is absent.
  Missing,
}

/// Secret-free normalized state returned by a managed provider adapter.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedWebhookRegistration {
  /// Opaque remote registration identity.
  pub registration_id: String,
  /// Current normalized lifecycle state.
  pub status: ManagedWebhookRegistrationStatus,
  /// Callback URL observed by the provider.
  pub callback_url: String,
}

impl ManagedWebhookRegistration {
  /// Revalidates bounded provider-neutral registration data.
  pub fn validate(&self) -> Result<(), StoreInputError> {
    let valid = |value: &str, maximum: usize| {
      !value.is_empty() && value.len() <= maximum && value.trim() == value && !value.chars().any(char::is_control)
    };
    if !valid(&self.registration_id, MAX_WEBHOOK_REGISTRATION_ID_BYTES)
      || !valid(&self.callback_url, MAX_WEBHOOK_CALLBACK_URL_BYTES)
    {
      return Err(StoreInputError::InvalidWebhookDefinition);
    }
    Ok(())
  }
}

/// Managed provider operation whose normalized result is being committed.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedWebhookOperation {
  /// Create the remote registration.
  Create,
  /// Observe the current remote registration.
  Observe,
  /// Rotate its verification material.
  Rotate,
  /// Remove the remote registration.
  Delete,
}

impl std::fmt::Debug for UnmanagedWebhookDefinition {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter
      .debug_struct("UnmanagedWebhookDefinition")
      .field("adapter_id", &self.adapter_id)
      .field("adapter_sha256", &self.adapter_sha256)
      .field("verification_material_handle", &"<redacted>")
      .field("verification_headers", &self.verification_headers)
      .field("repository_id", &self.repository_id)
      .field("event_kind", &self.event_kind)
      .field("parameters", &self.parameters)
      .field("priority", &self.priority)
      .finish()
  }
}

impl UnmanagedWebhookDefinition {
  /// Revalidates provider selection, header allowlist, and bounded Build input.
  pub fn validate(&self) -> Result<(), StoreInputError> {
    let valid_text = |value: &str, maximum: usize| {
      !value.is_empty() && value.len() <= maximum && value.trim() == value && !value.chars().any(char::is_control)
    };
    if !valid_text(&self.adapter_id, MAX_WEBHOOK_ADAPTER_ID_BYTES)
      || self.adapter_sha256.len() != WEBHOOK_ADAPTER_SHA256_BYTES
      || !self
        .adapter_sha256
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
      || !valid_text(&self.verification_material_handle, MAX_WEBHOOK_MATERIAL_HANDLE_BYTES)
      || self.verification_headers.is_empty()
      || self.verification_headers.len() > MAX_WEBHOOK_VERIFICATION_HEADERS
      || self
        .verification_headers
        .iter()
        .any(|name| !valid_webhook_header_name(name))
      || self
        .parameters
        .values()
        .any(|value| !matches!(value, Value::String(_) | Value::Bool(_) | Value::Number(_)))
    {
      return Err(StoreInputError::InvalidWebhookDefinition);
    }
    let parameters = serde_json::to_value(&self.parameters).map_err(|_| StoreInputError::InvalidWebhookDefinition)?;
    require_bounded_json_object(&parameters)?;
    Ok(())
  }
}

fn valid_webhook_header_name(name: &str) -> bool {
  !name.is_empty()
    && name.len() <= MAX_WEBHOOK_HEADER_NAME_BYTES
    && name
      .bytes()
      .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Atomic creation of an unmanaged integration and its immutable external Trigger.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateUnmanagedWebhook {
  /// Server-owned public integration identity.
  pub integration_id: IntegrationId,
  /// Exact external Trigger created with the integration.
  pub trigger: CreateTriggerDefinition,
  /// Provider-neutral verification and matching configuration.
  pub definition: UnmanagedWebhookDefinition,
  /// Stable replay identity for the complete mutation.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative creation time.
  pub created_at: Timestamp,
}

impl CreateUnmanagedWebhook {
  /// Revalidates the complete atomic request at the persistence seam.
  pub fn validate(&self) -> Result<(), StoreError> {
    self.trigger.validate()?;
    if self.trigger.kind != TriggerKind::External {
      return Err(StoreError::InvalidInput {
        operation: StoreOperation::CreateUnmanagedWebhook,
        source: StoreInputError::InvalidWebhookDefinition,
      });
    }
    self.definition.validate().map_err(|source| StoreError::InvalidInput {
      operation: StoreOperation::CreateUnmanagedWebhook,
      source,
    })
  }
}

/// Durable delivery configuration shared by unmanaged and managed integrations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebhookIntegrationRecord {
  /// Server-owned public integration identity.
  pub integration_id: IntegrationId,
  /// Exact immutable Trigger selected by the integration.
  pub trigger: TriggerDefinitionRef,
  /// Exact immutable Build Configuration selected by the Trigger.
  pub target: TriggerTarget,
  /// Whether new deliveries may be evaluated.
  pub enabled: bool,
  /// Provider-neutral verification and matching configuration.
  pub definition: UnmanagedWebhookDefinition,
}

/// Durable result of creating an unmanaged integration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UnmanagedWebhookMutationOutcome {
  /// Whether this mutation was newly applied or replayed.
  pub disposition: MutationDisposition,
  /// Server-owned integration identity.
  pub integration_id: IntegrationId,
  /// Exact immutable external Trigger identity.
  pub trigger: TriggerDefinitionRef,
}

/// Atomic reservation of one managed integration before invoking its provider adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateManagedWebhook {
  /// Server-owned public integration identity selected on the first attempt.
  pub integration_id: IntegrationId,
  /// Exact external Trigger created with the integration.
  pub trigger: CreateTriggerDefinition,
  /// Provider-neutral delivery and protected administration configuration.
  pub definition: ManagedWebhookDefinition,
  /// Stable replay identity for local and remote creation.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative first-attempt time.
  pub created_at: Timestamp,
}

impl CreateManagedWebhook {
  /// Revalidates the complete reservation at the persistence seam.
  pub fn validate(&self) -> Result<(), StoreError> {
    self.trigger.validate()?;
    if self.trigger.kind != TriggerKind::External {
      return Err(StoreError::InvalidInput {
        operation: StoreOperation::CreateManagedWebhook,
        source: StoreInputError::InvalidWebhookDefinition,
      });
    }
    self.definition.validate().map_err(|source| StoreError::InvalidInput {
      operation: StoreOperation::CreateManagedWebhook,
      source,
    })
  }
}

/// Durable managed integration, including protected handles used only by the adapter host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedWebhookRecord {
  /// Server-owned public integration identity.
  pub integration_id: IntegrationId,
  /// Exact immutable external Trigger.
  pub trigger: TriggerDefinitionRef,
  /// Exact immutable Build Configuration selected by the Trigger.
  pub target: TriggerTarget,
  /// Whether authenticated deliveries may be evaluated.
  pub enabled: bool,
  /// Provider-neutral protected configuration.
  pub definition: ManagedWebhookDefinition,
  /// Last durably observed remote state, absent while creation is pending.
  pub registration: Option<ManagedWebhookRegistration>,
}

/// Durable result of reserving or updating a managed integration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManagedWebhookMutationOutcome {
  /// Whether this local mutation was applied or replayed.
  pub disposition: MutationDisposition,
  /// Server-owned integration identity.
  pub integration_id: IntegrationId,
  /// Exact immutable external Trigger identity.
  pub trigger: TriggerDefinitionRef,
  /// Current normalized remote state, absent while creation is pending.
  pub registration: Option<ManagedWebhookRegistration>,
}

/// Commits one normalized provider result after a managed operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordManagedWebhookRegistration {
  /// Managed integration being updated.
  pub integration_id: IntegrationId,
  /// Provider operation that produced the state.
  pub operation: ManagedWebhookOperation,
  /// Secret-free normalized remote registration.
  pub registration: ManagedWebhookRegistration,
  /// Stable operation replay identity supplied to the provider adapter.
  pub idempotency_key: IdempotencyKey,
  /// Claim owner required to fence worker completion.
  pub owner: WorkerOwner,
  /// Server observation time for this provider result.
  pub observed_at: Timestamp,
}

/// Durable request to execute one idempotent managed-provider operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnqueueManagedWebhookOperation {
  /// Managed integration being synchronized.
  pub integration_id: IntegrationId,
  /// Idempotent provider operation.
  pub operation: ManagedWebhookOperation,
  /// Stable identity passed unchanged to every retry.
  pub idempotency_key: IdempotencyKey,
  /// Authoritative time at which the operation became durable.
  pub requested_at: Timestamp,
}

/// Bounded request to claim due managed-provider operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimManagedWebhookOperations {
  /// Process instance acquiring work.
  pub owner: WorkerOwner,
  /// Authoritative due-work and stale-claim time.
  pub observed_at: Timestamp,
  /// Exclusive deadline after which another replica may reclaim work.
  pub claim_expires_at: Timestamp,
  /// Positive bounded claim count.
  pub limit: NonZeroU16,
}

impl ClaimManagedWebhookOperations {
  /// Validates the ownership window and batch bound.
  pub fn new(
    owner: WorkerOwner,
    observed_at: Timestamp,
    claim_expires_at: Timestamp,
    limit: u16,
  ) -> Result<Self, StoreError> {
    let limit = NonZeroU16::new(limit)
      .filter(|value| value.get() <= MAX_MANAGED_WEBHOOK_OPERATION_BATCH_SIZE)
      .ok_or_else(|| {
        StoreError::invalid(
          StoreOperation::ClaimManagedWebhookOperations,
          StoreInputError::InvalidWorkerClaim,
        )
      })?;
    if claim_expires_at <= observed_at {
      return Err(StoreError::invalid(
        StoreOperation::ClaimManagedWebhookOperations,
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

/// One protected managed-provider operation exclusively claimed by a worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedWebhookOperationClaim {
  /// Complete protected integration configuration.
  pub integration: ManagedWebhookRecord,
  /// Provider lifecycle operation.
  pub operation: ManagedWebhookOperation,
  /// Stable provider replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Number of attempts including this claim.
  pub attempt: u16,
  /// Claim owner required for failure recording.
  pub owner: WorkerOwner,
  /// Exclusive claim deadline.
  pub claim_expires_at: Timestamp,
}

/// Records retry scheduling or final dead-letter state for managed work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FailManagedWebhookOperation {
  /// Managed integration whose operation failed.
  pub integration_id: IntegrationId,
  /// Failed provider lifecycle operation.
  pub operation: ManagedWebhookOperation,
  /// Stable provider replay identity.
  pub idempotency_key: IdempotencyKey,
  /// Claim owner required to fence worker failure.
  pub owner: WorkerOwner,
  /// Stable safe failure class.
  pub code: WebhookFailureCode,
  /// Bounded secret-free diagnostic.
  pub diagnostic: String,
  /// Authoritative failure time.
  pub failed_at: Timestamp,
  /// Next attempt time for a transient failure; `None` creates a dead letter.
  pub retry_at: Option<Timestamp>,
}

impl FailManagedWebhookOperation {
  /// Revalidates safe diagnostics and retry ordering.
  pub fn validate(&self) -> Result<(), StoreError> {
    if self.diagnostic.is_empty()
      || self.diagnostic.len() > crate::MAX_WEBHOOK_DIAGNOSTIC_BYTES
      || self.diagnostic.chars().any(char::is_control)
      || self.retry_at.is_some_and(|retry_at| retry_at <= self.failed_at)
    {
      return Err(StoreError::invalid(
        StoreOperation::FailManagedWebhookOperation,
        StoreInputError::InvalidWebhookDefinition,
      ));
    }
    Ok(())
  }
}

impl RecordManagedWebhookRegistration {
  /// Revalidates the provider result before opening a transaction.
  pub fn validate(&self) -> Result<(), StoreError> {
    self
      .registration
      .validate()
      .map_err(|source| StoreError::InvalidInput {
        operation: StoreOperation::RecordManagedWebhookRegistration,
        source,
      })?;
    if (self.operation == ManagedWebhookOperation::Create
      && self.registration.status == ManagedWebhookRegistrationStatus::Missing)
      || (self.operation == ManagedWebhookOperation::Delete
        && self.registration.status != ManagedWebhookRegistrationStatus::Missing)
    {
      return Err(StoreError::InvalidInput {
        operation: StoreOperation::RecordManagedWebhookRegistration,
        source: StoreInputError::InvalidWebhookDefinition,
      });
    }
    Ok(())
  }
}
