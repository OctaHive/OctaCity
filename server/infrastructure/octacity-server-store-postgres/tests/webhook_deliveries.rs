#[path = "support/authoritative_fixture.rs"]
mod authoritative_fixture;
mod support;

use std::collections::{BTreeMap, BTreeSet};

use authoritative_fixture::seed_authoritative_prerequisites;
use octacity_server_domain::{IntegrationId, Timestamp, TriggerId, TriggerIdentity, TriggerVersion};
use octacity_server_store::{
  ClaimManagedWebhookOperations, ClaimWebhookDeliveries, CompleteWebhookDelivery, CreateManagedWebhook,
  CreateTriggerDefinition, CreateUnmanagedWebhook, EnqueueWebhookDelivery, FailManagedWebhookOperation,
  FailWebhookDelivery, IdempotencyKey, ManagedWebhookDefinition, ManagedWebhookOperation,
  ManagedWebhookOperationStore as _, ManagedWebhookRegistration, ManagedWebhookRegistrationStatus,
  ManagedWebhookRegistrationStore as _, NormalizedWebhookEvent, RecordManagedWebhookRegistration, RecordWebhookEvent,
  RecordWebhookEventOutcome, SuppressWebhookDelivery, TriggerEventKind, TriggerKind, UnmanagedWebhookDefinition,
  WebhookConfigurationStore as _, WebhookDeliveryAdmissionStore as _, WebhookDeliveryId,
  WebhookDeliveryQueryStore as _, WebhookDeliveryState, WebhookDeliveryWork, WebhookDeliveryWorkStore as _,
  WebhookFailureCode, WorkerOwner, testing::authoritative_store_contract_fixture,
};
use octacity_server_store_postgres::PostgresStore;
use support::TestDatabase;

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn normalized_identity_and_retry_state_survive_store_replacement() {
  let database = TestDatabase::migrated().await;
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let integration_id = IntegrationId::from_uuid(uuid::Uuid::from_u128(9_001)).unwrap();
  let trigger_id = TriggerId::from_uuid(uuid::Uuid::from_u128(9_002)).unwrap();
  store
    .create_unmanaged_webhook(CreateUnmanagedWebhook {
      integration_id,
      trigger: CreateTriggerDefinition {
        id: trigger_id,
        version: TriggerVersion::INITIAL,
        configuration_id: fixture.request.build.configuration_id,
        configuration_version: fixture.request.build.configuration_version,
        kind: TriggerKind::External,
        enabled: true,
        definition: serde_json::json!({}),
        idempotency_key: IdempotencyKey::new("webhook-delivery-integration").unwrap(),
        created_at: time(10),
      },
      definition: UnmanagedWebhookDefinition {
        adapter_id: "fixture".to_owned(),
        adapter_sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_owned(),
        verification_material_handle: "secret:webhook".to_owned(),
        verification_headers: BTreeSet::from(["x-signature".to_owned()]),
        repository_id: fixture.request.build.repository_id,
        event_kind: TriggerEventKind::new("push").unwrap(),
        parameters: BTreeMap::new(),
        priority: 0,
      },
      idempotency_key: IdempotencyKey::new("webhook-delivery-integration").unwrap(),
      created_at: time(10),
    })
    .await
    .unwrap();

  let canonical = WebhookDeliveryId::from_uuid(uuid::Uuid::from_u128(9_003)).unwrap();
  let duplicate = WebhookDeliveryId::from_uuid(uuid::Uuid::from_u128(9_004)).unwrap();
  let failing = WebhookDeliveryId::from_uuid(uuid::Uuid::from_u128(9_005)).unwrap();
  for delivery_id in [canonical, duplicate, failing] {
    store
      .enqueue_webhook_delivery(EnqueueWebhookDelivery {
        delivery_id,
        integration_id,
        headers: BTreeMap::from([("x-signature".to_owned(), "proof".to_owned())]),
        body: b"exact body".to_vec(),
        received_at: time(20),
      })
      .await
      .unwrap();
  }

  let owner = WorkerOwner::new("webhook:primary").unwrap();
  let claims = store
    .claim_webhook_deliveries(ClaimWebhookDeliveries::new(owner.clone(), time(30), time(40), 3).unwrap())
    .await
    .unwrap();
  assert_eq!(claims.len(), 3);
  assert!(
    claims
      .iter()
      .all(|claim| matches!(claim.work, WebhookDeliveryWork::Verify { .. }))
  );
  let event = NormalizedWebhookEvent {
    integration_id,
    provider_delivery_id: TriggerIdentity::new("provider-delivery-1").unwrap(),
    event_kind: TriggerEventKind::new("push").unwrap(),
    repository_id: fixture.request.build.repository_id,
    reference: None,
    revision: None,
    provider_time: Some(time(15)),
    actor_display_name: None,
    metadata: BTreeMap::new(),
  };
  assert_eq!(
    store
      .record_webhook_event(RecordWebhookEvent {
        delivery_id: canonical,
        owner: owner.clone(),
        event: event.clone(),
        recorded_at: time(35),
      })
      .await
      .unwrap(),
    RecordWebhookEventOutcome::Recorded
  );
  assert_eq!(
    store
      .record_webhook_event(RecordWebhookEvent {
        delivery_id: duplicate,
        owner: owner.clone(),
        event,
        recorded_at: time(35),
      })
      .await
      .unwrap(),
    RecordWebhookEventOutcome::Duplicate
  );
  store
    .fail_webhook_delivery(FailWebhookDelivery {
      delivery_id: failing,
      owner,
      code: WebhookFailureCode::Unavailable,
      diagnostic: "webhook verifier is temporarily unavailable".to_owned(),
      failed_at: time(35),
      retry_at: Some(time(50)),
    })
    .await
    .unwrap();

  let replacement = PostgresStore::new(database.pool.clone());
  let owner = WorkerOwner::new("webhook:replacement").unwrap();
  let claims = replacement
    .claim_webhook_deliveries(ClaimWebhookDeliveries::new(owner.clone(), time(50), time(60), 3).unwrap())
    .await
    .unwrap();
  assert_eq!(claims.len(), 2);
  let trigger_claim = claims.iter().find(|claim| claim.delivery_id == canonical).unwrap();
  assert!(matches!(trigger_claim.work, WebhookDeliveryWork::Trigger { .. }));
  replacement
    .complete_webhook_delivery(CompleteWebhookDelivery {
      delivery_id: canonical,
      owner: owner.clone(),
      completed_at: time(55),
    })
    .await
    .unwrap();
  replacement
    .fail_webhook_delivery(FailWebhookDelivery {
      delivery_id: failing,
      owner,
      code: WebhookFailureCode::Unavailable,
      diagnostic: "webhook verifier is temporarily unavailable".to_owned(),
      failed_at: time(55),
      retry_at: None,
    })
    .await
    .unwrap();

  let canonical_diagnostic = replacement.webhook_delivery(canonical).await.unwrap();
  assert_eq!(canonical_diagnostic.state, WebhookDeliveryState::Completed);
  assert_eq!(canonical_diagnostic.verification_attempts, 1);
  let duplicate_diagnostic = replacement.webhook_delivery(duplicate).await.unwrap();
  assert_eq!(duplicate_diagnostic.state, WebhookDeliveryState::Completed);
  let failure_diagnostic = replacement.webhook_delivery(failing).await.unwrap();
  assert_eq!(failure_diagnostic.state, WebhookDeliveryState::DeadLetter);
  assert_eq!(failure_diagnostic.verification_attempts, 2);
  assert_eq!(failure_diagnostic.failure_code, Some(WebhookFailureCode::Unavailable));
  assert_eq!(
    failure_diagnostic.diagnostic.as_deref(),
    Some("webhook verifier is temporarily unavailable")
  );

  let suppressed = WebhookDeliveryId::from_uuid(uuid::Uuid::from_u128(9_006)).unwrap();
  replacement
    .enqueue_webhook_delivery(EnqueueWebhookDelivery {
      delivery_id: suppressed,
      integration_id,
      headers: BTreeMap::from([("x-signature".to_owned(), "proof".to_owned())]),
      body: b"exact body".to_vec(),
      received_at: time(70),
    })
    .await
    .unwrap();
  let owner = WorkerOwner::new("webhook:suppressor").unwrap();
  let claims = replacement
    .claim_webhook_deliveries(ClaimWebhookDeliveries::new(owner.clone(), time(70), time(80), 1).unwrap())
    .await
    .unwrap();
  assert_eq!(claims.len(), 1);
  replacement
    .fail_webhook_delivery(FailWebhookDelivery {
      delivery_id: suppressed,
      owner,
      code: WebhookFailureCode::Unavailable,
      diagnostic: "temporary failure before disablement".to_owned(),
      failed_at: time(75),
      retry_at: Some(time(80)),
    })
    .await
    .unwrap();
  let owner = WorkerOwner::new("webhook:suppressor-after-retry").unwrap();
  let claims = replacement
    .claim_webhook_deliveries(ClaimWebhookDeliveries::new(owner.clone(), time(80), time(90), 1).unwrap())
    .await
    .unwrap();
  assert_eq!(claims.len(), 1);
  replacement
    .suppress_webhook_delivery(SuppressWebhookDelivery {
      delivery_id: suppressed,
      owner,
      suppressed_at: time(85),
    })
    .await
    .unwrap();
  let suppressed_diagnostic = replacement.webhook_delivery(suppressed).await.unwrap();
  assert_eq!(suppressed_diagnostic.state, WebhookDeliveryState::Suppressed);
  assert_eq!(suppressed_diagnostic.failure_code, None);
  assert_eq!(suppressed_diagnostic.diagnostic, None);

  database.cleanup().await;
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable PostgreSQL service"]
async fn managed_registration_retry_survives_store_replacement() {
  let database = TestDatabase::migrated().await;
  let fixture = authoritative_store_contract_fixture();
  seed_authoritative_prerequisites(&database.pool, &fixture)
    .await
    .unwrap();
  let store = PostgresStore::new(database.pool.clone());
  let integration_id = IntegrationId::from_uuid(uuid::Uuid::from_u128(9_101)).unwrap();
  let trigger_id = TriggerId::from_uuid(uuid::Uuid::from_u128(9_102)).unwrap();
  let idempotency_key = IdempotencyKey::new("managed-registration-create").unwrap();
  store
    .create_managed_webhook(CreateManagedWebhook {
      integration_id,
      trigger: CreateTriggerDefinition {
        id: trigger_id,
        version: TriggerVersion::INITIAL,
        configuration_id: fixture.request.build.configuration_id,
        configuration_version: fixture.request.build.configuration_version,
        kind: TriggerKind::External,
        enabled: true,
        definition: serde_json::json!({}),
        idempotency_key: idempotency_key.clone(),
        created_at: time(10),
      },
      definition: ManagedWebhookDefinition {
        delivery: UnmanagedWebhookDefinition {
          adapter_id: "fixture".to_owned(),
          adapter_sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_owned(),
          verification_material_handle: "secret:webhook".to_owned(),
          verification_headers: BTreeSet::from(["x-signature".to_owned()]),
          repository_id: fixture.request.build.repository_id,
          event_kind: TriggerEventKind::new("push").unwrap(),
          parameters: BTreeMap::new(),
          priority: 0,
        },
        administration_credential_handle: "secret:webhook-administration".to_owned(),
      },
      idempotency_key: idempotency_key.clone(),
      created_at: time(10),
    })
    .await
    .unwrap();

  let owner = WorkerOwner::new("managed:primary").unwrap();
  let claim = store
    .claim_managed_webhook_operations(ClaimManagedWebhookOperations::new(owner, time(20), time(25), 1).unwrap())
    .await
    .unwrap()
    .pop()
    .unwrap();
  assert_eq!(claim.attempt, 1);
  assert_eq!(claim.idempotency_key, idempotency_key);
  let stale = store
    .fail_managed_webhook_operation(FailManagedWebhookOperation {
      integration_id,
      operation: ManagedWebhookOperation::Create,
      idempotency_key: idempotency_key.clone(),
      owner: WorkerOwner::new("managed:stale").unwrap(),
      code: WebhookFailureCode::Unavailable,
      diagnostic: "stale worker result".to_owned(),
      failed_at: time(20),
      retry_at: Some(time(30)),
    })
    .await
    .unwrap_err();
  assert!(matches!(stale, octacity_server_store::StoreError::Conflict { .. }));
  store
    .fail_managed_webhook_operation(FailManagedWebhookOperation {
      integration_id,
      operation: ManagedWebhookOperation::Create,
      idempotency_key: idempotency_key.clone(),
      owner: claim.owner,
      code: WebhookFailureCode::Unavailable,
      diagnostic: "provider requested retry".to_owned(),
      failed_at: time(20),
      retry_at: Some(time(30)),
    })
    .await
    .unwrap();

  let replacement = PostgresStore::new(database.pool.clone());
  let claim = replacement
    .claim_managed_webhook_operations(
      ClaimManagedWebhookOperations::new(WorkerOwner::new("managed:replacement").unwrap(), time(30), time(40), 1)
        .unwrap(),
    )
    .await
    .unwrap()
    .pop()
    .unwrap();
  assert_eq!(claim.attempt, 2);
  assert_eq!(claim.idempotency_key, idempotency_key);
  replacement
    .record_managed_webhook_registration(RecordManagedWebhookRegistration {
      integration_id,
      operation: ManagedWebhookOperation::Create,
      registration: ManagedWebhookRegistration {
        registration_id: "remote-registration-01".to_owned(),
        status: ManagedWebhookRegistrationStatus::Active,
        callback_url: format!("https://hooks.example.test/webhooks/v1/integrations/{integration_id}"),
      },
      idempotency_key,
      owner: claim.owner,
      observed_at: time(35),
    })
    .await
    .unwrap();
  assert_eq!(
    replacement
      .managed_webhook(integration_id)
      .await
      .unwrap()
      .registration
      .unwrap()
      .status,
    ManagedWebhookRegistrationStatus::Active
  );

  database.cleanup().await;
}

fn time(value: i64) -> Timestamp {
  Timestamp::from_unix_millis(value).unwrap()
}
