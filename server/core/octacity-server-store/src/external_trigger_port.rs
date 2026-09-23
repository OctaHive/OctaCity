use async_trait::async_trait;
use octacity_server_domain::IntegrationId;

use crate::{
  ClaimManagedWebhookOperations, ClaimWebhookDeliveries, CompleteWebhookDelivery, CreateManagedWebhook,
  CreateUnmanagedWebhook, EnqueueManagedWebhookOperation, EnqueueWebhookDelivery, FailManagedWebhookOperation,
  FailWebhookDelivery, ManagedWebhookMutationOutcome, ManagedWebhookOperationClaim, ManagedWebhookRecord,
  MutationDisposition, RecordManagedWebhookRegistration, RecordWebhookEvent, RecordWebhookEventOutcome, StoreError,
  SuppressWebhookDelivery, UnmanagedWebhookMutationOutcome, WebhookDeliveryClaim, WebhookDeliveryDiagnostic,
  WebhookDeliveryId, WebhookIntegrationRecord,
};

/// Atomic persistence for webhook integration creation.
#[async_trait]
pub trait WebhookConfigurationStore: Send + Sync {
  /// Atomically creates one integration and its external Trigger definition.
  async fn create_unmanaged_webhook(
    &self,
    request: CreateUnmanagedWebhook,
  ) -> Result<UnmanagedWebhookMutationOutcome, StoreError>;

  /// Atomically reserves one managed integration before the provider call.
  async fn create_managed_webhook(
    &self,
    request: CreateManagedWebhook,
  ) -> Result<ManagedWebhookMutationOutcome, StoreError>;
}

/// Persistence for protected managed-registration state.
#[async_trait]
pub trait ManagedWebhookRegistrationStore: Send + Sync {
  /// Reads protected managed configuration for one adapter operation.
  async fn managed_webhook(&self, integration_id: IntegrationId) -> Result<ManagedWebhookRecord, StoreError>;
}

/// Durable work queue and atomic result commit for managed-provider operations.
#[async_trait]
pub trait ManagedWebhookOperationStore: Send + Sync {
  /// Durably enqueues an idempotent operation before provider execution.
  async fn enqueue_managed_webhook_operation(
    &self,
    request: EnqueueManagedWebhookOperation,
  ) -> Result<MutationDisposition, StoreError>;

  /// Claims a bounded batch of due or abandoned operations.
  async fn claim_managed_webhook_operations(
    &self,
    request: ClaimManagedWebhookOperations,
  ) -> Result<Vec<ManagedWebhookOperationClaim>, StoreError>;

  /// Idempotently commits normalized state returned by the provider adapter.
  async fn record_managed_webhook_registration(
    &self,
    request: RecordManagedWebhookRegistration,
  ) -> Result<ManagedWebhookMutationOutcome, StoreError>;

  /// Schedules a transient retry or retains a final dead letter.
  async fn fail_managed_webhook_operation(&self, request: FailManagedWebhookOperation) -> Result<(), StoreError>;
}

/// Reads immutable webhook integration configuration.
#[async_trait]
pub trait WebhookIntegrationReader: Send + Sync {
  /// Reads delivery authentication and Trigger configuration selected by a public callback identity.
  async fn webhook_integration(&self, integration_id: IntegrationId) -> Result<WebhookIntegrationRecord, StoreError>;
}

/// Durably admits raw public webhook deliveries.
#[async_trait]
pub trait WebhookDeliveryAdmissionStore: Send + Sync {
  /// Durably admits exact bounded raw delivery data before adapter execution.
  async fn enqueue_webhook_delivery(&self, request: EnqueueWebhookDelivery) -> Result<MutationDisposition, StoreError>;
}

/// Claims and advances durable webhook delivery work.
#[async_trait]
pub trait WebhookDeliveryWorkStore: Send + Sync {
  /// Claims due raw-verification or normalized Trigger work.
  async fn claim_webhook_deliveries(
    &self,
    request: ClaimWebhookDeliveries,
  ) -> Result<Vec<WebhookDeliveryClaim>, StoreError>;

  /// Atomically deduplicates and records one authenticated normalized event.
  async fn record_webhook_event(&self, request: RecordWebhookEvent) -> Result<RecordWebhookEventOutcome, StoreError>;

  /// Schedules a transient retry or records a final dead letter.
  async fn fail_webhook_delivery(&self, request: FailWebhookDelivery) -> Result<(), StoreError>;

  /// Completes one normalized receipt after idempotent Trigger evaluation.
  async fn complete_webhook_delivery(
    &self,
    request: CompleteWebhookDelivery,
  ) -> Result<MutationDisposition, StoreError>;

  /// Terminates one owned receipt without adapter or Trigger work when its integration is disabled.
  async fn suppress_webhook_delivery(
    &self,
    request: SuppressWebhookDelivery,
  ) -> Result<MutationDisposition, StoreError>;
}

/// Reads secret-free webhook delivery diagnostics.
#[async_trait]
pub trait WebhookDeliveryQueryStore: Send + Sync {
  /// Reads lifecycle and dead-letter diagnostics for one receipt.
  async fn webhook_delivery(&self, delivery_id: WebhookDeliveryId) -> Result<WebhookDeliveryDiagnostic, StoreError>;
}
