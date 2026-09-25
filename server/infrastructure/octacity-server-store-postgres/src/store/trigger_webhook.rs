use async_trait::async_trait;
use octacity_server_store::*;

use super::PostgresStore;

#[async_trait]
impl TriggerDefinitionStore for PostgresStore {
  async fn require_enabled_trigger(
    &self,
    trigger: TriggerDefinitionRef,
    kind: TriggerKind,
    target: TriggerTarget,
  ) -> Result<(), StoreError> {
    crate::trigger_query::require_enabled(&self.pool, trigger, kind, target).await
  }
}

#[async_trait]
impl WebhookConfigurationStore for PostgresStore {
  async fn create_unmanaged_webhook(
    &self,
    request: CreateUnmanagedWebhook,
  ) -> Result<UnmanagedWebhookMutationOutcome, StoreError> {
    crate::external_trigger::create(&self.pool, request).await
  }

  async fn create_managed_webhook(
    &self,
    request: CreateManagedWebhook,
  ) -> Result<ManagedWebhookMutationOutcome, StoreError> {
    crate::external_trigger::create_managed(&self.pool, request).await
  }
}

#[async_trait]
impl ManagedWebhookRegistrationStore for PostgresStore {
  async fn managed_webhook(
    &self,
    integration_id: octacity_server_domain::IntegrationId,
  ) -> Result<ManagedWebhookRecord, StoreError> {
    crate::external_trigger::read_managed(&self.pool, integration_id).await
  }
}

#[async_trait]
impl ManagedWebhookOperationStore for PostgresStore {
  async fn enqueue_managed_webhook_operation(
    &self,
    request: octacity_server_store::EnqueueManagedWebhookOperation,
  ) -> Result<MutationDisposition, StoreError> {
    crate::external_trigger::enqueue_managed_operation(&self.pool, request).await
  }

  async fn claim_managed_webhook_operations(
    &self,
    request: octacity_server_store::ClaimManagedWebhookOperations,
  ) -> Result<Vec<octacity_server_store::ManagedWebhookOperationClaim>, StoreError> {
    crate::external_trigger::claim_managed_operations(&self.pool, request).await
  }

  async fn record_managed_webhook_registration(
    &self,
    request: RecordManagedWebhookRegistration,
  ) -> Result<ManagedWebhookMutationOutcome, StoreError> {
    crate::external_trigger::record_managed(&self.pool, request).await
  }

  async fn fail_managed_webhook_operation(
    &self,
    request: octacity_server_store::FailManagedWebhookOperation,
  ) -> Result<(), StoreError> {
    crate::external_trigger::fail_managed_operation(&self.pool, request).await
  }
}

#[async_trait]
impl WebhookIntegrationReader for PostgresStore {
  async fn webhook_integration(
    &self,
    integration_id: octacity_server_domain::IntegrationId,
  ) -> Result<WebhookIntegrationRecord, StoreError> {
    crate::external_trigger::read(&self.pool, integration_id).await
  }
}

#[async_trait]
impl WebhookDeliveryAdmissionStore for PostgresStore {
  async fn enqueue_webhook_delivery(
    &self,
    request: octacity_server_store::EnqueueWebhookDelivery,
  ) -> Result<octacity_server_store::MutationDisposition, StoreError> {
    crate::external_trigger::enqueue_delivery(&self.pool, request).await
  }
}

#[async_trait]
impl WebhookDeliveryWorkStore for PostgresStore {
  async fn claim_webhook_deliveries(
    &self,
    request: octacity_server_store::ClaimWebhookDeliveries,
  ) -> Result<Vec<octacity_server_store::WebhookDeliveryClaim>, StoreError> {
    crate::external_trigger::claim_deliveries(&self.pool, request).await
  }

  async fn record_webhook_event(
    &self,
    request: octacity_server_store::RecordWebhookEvent,
  ) -> Result<octacity_server_store::RecordWebhookEventOutcome, StoreError> {
    crate::external_trigger::record_event(&self.pool, request).await
  }

  async fn fail_webhook_delivery(&self, request: octacity_server_store::FailWebhookDelivery) -> Result<(), StoreError> {
    crate::external_trigger::fail_delivery(&self.pool, request).await
  }

  async fn complete_webhook_delivery(
    &self,
    request: octacity_server_store::CompleteWebhookDelivery,
  ) -> Result<octacity_server_store::MutationDisposition, StoreError> {
    crate::external_trigger::complete_delivery(&self.pool, request).await
  }

  async fn suppress_webhook_delivery(
    &self,
    request: SuppressWebhookDelivery,
  ) -> Result<MutationDisposition, StoreError> {
    crate::external_trigger::suppress_delivery(&self.pool, request).await
  }
}

#[async_trait]
impl WebhookDeliveryQueryStore for PostgresStore {
  async fn webhook_delivery(
    &self,
    delivery_id: octacity_server_store::WebhookDeliveryId,
  ) -> Result<octacity_server_store::WebhookDeliveryDiagnostic, StoreError> {
    crate::external_trigger::delivery_diagnostic(&self.pool, delivery_id).await
  }
}
