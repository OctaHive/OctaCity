use super::*;

#[test]
fn webhook_delivery_is_durable_before_adapter_execution() {
  run(async {
    let mut fixture = fixture();
    fixture.context.configuration.definition.triggers.allowed = BTreeSet::from([TriggerKind::External]);
    let acceptance = Arc::new(RecordingStore::default());
    let trigger_engine = Arc::new(ManualTriggerService::new(
      acceptance.clone(),
      Arc::new(StaticContext(fixture.context.clone())),
      Arc::new(RecordingResolver::succeed("0123456789abcdef")),
    ));
    let integration_id = id::<IntegrationId>(90);
    let verifier = Arc::new(StaticWebhookVerifier {
      calls: AtomicUsize::new(0),
      event: AuthenticatedWebhookEvent {
        integration_id,
        delivery_id: TriggerIdentity::new("provider-delivery-1").unwrap(),
        event_kind: TriggerEventKind::new("push").unwrap(),
        repository_id: fixture.context.repository.id,
        reference: Some(SourceReference::new("refs/heads/main").unwrap()),
        revision: Some(ImmutableRevision::new("0123456789abcdef").unwrap()),
        provider_time: Some(time(100)),
        actor_display_name: Some("Builder".to_owned()),
        metadata: BTreeMap::from([("provider".to_owned(), "example".to_owned())]),
      },
    });
    let store = Arc::new(StaticExternalStore {
      integration: Mutex::new(WebhookIntegrationRecord {
        integration_id,
        trigger: fixture.command.trigger,
        target: fixture.command.target,
        enabled: true,
        definition: UnmanagedWebhookDefinition {
          adapter_id: "example".to_owned(),
          adapter_sha256: DIGEST.to_owned(),
          verification_material_handle: "secret:webhook".to_owned(),
          verification_headers: BTreeSet::from(["x-signature".to_owned()]),
          repository_id: fixture.context.repository.id,
          event_kind: TriggerEventKind::new("push").unwrap(),
          parameters: fixture.command.parameters,
          priority: fixture.command.priority,
        },
      }),
      deliveries: Mutex::new(StaticDeliveryState::default()),
    });
    let ingress = WebhookIngressService::new(store.clone(), store.clone());
    let delivery = || AcceptWebhookDeliveryCommand {
      integration_id,
      headers: BTreeMap::from([("x-signature".to_owned(), "valid".to_owned())]),
      body: br#"{"event":"push"}"#.to_vec(),
      received_at: time(200),
    };

    let first = ingress.handle_command(delivery()).await.unwrap();
    let second = ingress.handle_command(delivery()).await.unwrap();

    assert_eq!(first.disposition, ApplicationDisposition::Applied);
    assert_eq!(second.disposition, ApplicationDisposition::Applied);
    assert_ne!(first.delivery_id, second.delivery_id);
    assert_eq!(store.deliveries.lock().unwrap().deliveries.len(), 2);
    assert_eq!(acceptance.calls(), 0, "ingress must not evaluate a Trigger inline");
    assert_eq!(
      verifier.calls.load(Ordering::SeqCst),
      0,
      "ingress must return only after durable admission"
    );

    let first_worker = WebhookDeliveryWorker::new(
      store.clone(),
      store.clone(),
      trigger_engine.clone(),
      verifier.clone(),
      DurableRetryPolicy::new(3, 10, 100).unwrap(),
    );
    let normalized = first_worker
      .run_once(WorkerOwner::new("webhook:test-1").unwrap(), time(210), time(300), 10)
      .await
      .unwrap();
    assert_eq!(normalized.normalized, 1);
    assert_eq!(normalized.duplicates, 1);
    drop(first_worker);

    let restarted_worker = WebhookDeliveryWorker::new(
      store.clone(),
      store,
      trigger_engine,
      verifier.clone(),
      DurableRetryPolicy::new(3, 10, 100).unwrap(),
    );
    let evaluated = restarted_worker
      .run_once(WorkerOwner::new("webhook:test-2").unwrap(), time(220), time(300), 10)
      .await
      .unwrap();
    assert_eq!(evaluated.evaluated, 1);
    assert_eq!(acceptance.calls(), 1, "one provider identity must create one Build");
    assert_eq!(verifier.calls.load(Ordering::SeqCst), 2);
  });
}

#[test]
fn disabled_integrations_terminally_suppress_queued_and_new_deliveries() {
  run(async {
    let mut fixture = fixture();
    fixture.context.configuration.definition.triggers.allowed = BTreeSet::from([TriggerKind::External]);
    let acceptance = Arc::new(RecordingStore::default());
    let trigger_engine = Arc::new(ManualTriggerService::new(
      acceptance.clone(),
      Arc::new(StaticContext(fixture.context.clone())),
      Arc::new(RecordingResolver::succeed("0123456789abcdef")),
    ));
    let integration_id = id::<IntegrationId>(88);
    let verifier = Arc::new(StaticWebhookVerifier {
      calls: AtomicUsize::new(0),
      event: AuthenticatedWebhookEvent {
        integration_id,
        delivery_id: TriggerIdentity::new("provider-delivery-disabled").unwrap(),
        event_kind: TriggerEventKind::new("push").unwrap(),
        repository_id: fixture.context.repository.id,
        reference: None,
        revision: Some(ImmutableRevision::new("0123456789abcdef").unwrap()),
        provider_time: None,
        actor_display_name: None,
        metadata: BTreeMap::new(),
      },
    });
    let store = Arc::new(StaticExternalStore {
      integration: Mutex::new(WebhookIntegrationRecord {
        integration_id,
        trigger: fixture.command.trigger,
        target: fixture.command.target,
        enabled: true,
        definition: UnmanagedWebhookDefinition {
          adapter_id: "example".to_owned(),
          adapter_sha256: DIGEST.to_owned(),
          verification_material_handle: "secret:webhook".to_owned(),
          verification_headers: BTreeSet::from(["x-signature".to_owned()]),
          repository_id: fixture.context.repository.id,
          event_kind: TriggerEventKind::new("push").unwrap(),
          parameters: fixture.command.parameters,
          priority: fixture.command.priority,
        },
      }),
      deliveries: Mutex::new(StaticDeliveryState::default()),
    });
    let ingress = WebhookIngressService::new(store.clone(), store.clone());
    let delivery = |received_at| AcceptWebhookDeliveryCommand {
      integration_id,
      headers: BTreeMap::from([("x-signature".to_owned(), "valid".to_owned())]),
      body: b"delivery".to_vec(),
      received_at,
    };

    let queued_before_delete = ingress.handle_command(delivery(time(100))).await.unwrap();
    store.integration.lock().unwrap().enabled = false;
    let received_after_delete = ingress.handle_command(delivery(time(110))).await.unwrap();
    let outcome = WebhookDeliveryWorker::new(
      store.clone(),
      store.clone(),
      trigger_engine,
      verifier.clone(),
      DurableRetryPolicy::new(3, 10, 100).unwrap(),
    )
    .run_once(WorkerOwner::new("webhook:disabled").unwrap(), time(120), time(200), 10)
    .await
    .unwrap();

    assert_eq!(outcome.suppressed, 2);
    assert_eq!(verifier.calls.load(Ordering::SeqCst), 0);
    assert_eq!(acceptance.calls(), 0);
    for delivery_id in [queued_before_delete.delivery_id, received_after_delete.delivery_id] {
      assert_eq!(
        store.webhook_delivery(delivery_id).await.unwrap().state,
        octacity_server_store::WebhookDeliveryState::Suppressed
      );
    }
  });
}

#[test]
fn transient_adapter_failures_survive_restart_and_end_in_a_safe_dead_letter() {
  run(async {
    let mut fixture = fixture();
    fixture.context.configuration.definition.triggers.allowed = BTreeSet::from([TriggerKind::External]);
    let acceptance = Arc::new(RecordingStore::default());
    let integration_id = id::<IntegrationId>(89);
    let store = Arc::new(StaticExternalStore {
      integration: Mutex::new(WebhookIntegrationRecord {
        integration_id,
        trigger: fixture.command.trigger,
        target: fixture.command.target,
        enabled: true,
        definition: UnmanagedWebhookDefinition {
          adapter_id: "unavailable".to_owned(),
          adapter_sha256: DIGEST.to_owned(),
          verification_material_handle: "secret:webhook".to_owned(),
          verification_headers: BTreeSet::from(["x-signature".to_owned()]),
          repository_id: fixture.context.repository.id,
          event_kind: TriggerEventKind::new("push").unwrap(),
          parameters: fixture.command.parameters,
          priority: fixture.command.priority,
        },
      }),
      deliveries: Mutex::new(StaticDeliveryState::default()),
    });
    let verifier = Arc::new(UnavailableVerifier(AtomicUsize::new(0)));
    let trigger_engine = Arc::new(ManualTriggerService::new(
      acceptance.clone(),
      Arc::new(StaticContext(fixture.context)),
      Arc::new(RecordingResolver::succeed("0123456789abcdef")),
    ));
    let accepted = WebhookIngressService::new(store.clone(), store.clone())
      .handle_command(AcceptWebhookDeliveryCommand {
        integration_id,
        headers: BTreeMap::from([("x-signature".to_owned(), "valid".to_owned())]),
        body: b"delivery".to_vec(),
        received_at: time(100),
      })
      .await
      .unwrap();

    let policy = DurableRetryPolicy::new(2, 10, 100).unwrap();
    let first = WebhookDeliveryWorker::new(
      store.clone(),
      store.clone(),
      trigger_engine.clone(),
      verifier.clone(),
      policy,
    )
    .run_once(WorkerOwner::new("webhook:retry-1").unwrap(), time(200), time(300), 1)
    .await
    .unwrap();
    assert_eq!(first.retries_scheduled, 1);

    let exhausted = WebhookDeliveryWorker::new(store.clone(), store.clone(), trigger_engine, verifier.clone(), policy)
      .run_once(WorkerOwner::new("webhook:retry-2").unwrap(), time(220), time(300), 1)
      .await
      .unwrap();
    assert_eq!(exhausted.dead_letters, 1);
    let diagnostic = store.webhook_delivery(accepted.delivery_id).await.unwrap();
    assert_eq!(
      diagnostic.failure_code,
      Some(octacity_server_store::WebhookFailureCode::Unavailable)
    );
    assert_eq!(
      diagnostic.diagnostic.as_deref(),
      Some("webhook provider failure (rate_limited): provider asked the server to retry")
    );
    assert_eq!(
      diagnostic.state,
      octacity_server_store::WebhookDeliveryState::DeadLetter
    );
    assert_eq!(verifier.0.load(Ordering::SeqCst), 2);
    assert_eq!(acceptance.calls(), 0);
  });
}

struct StaticExternalStore {
  integration: Mutex<WebhookIntegrationRecord>,
  deliveries: Mutex<StaticDeliveryState>,
}

#[derive(Default)]
struct StaticDeliveryState {
  deliveries: BTreeMap<octacity_server_store::WebhookDeliveryId, StaticDelivery>,
  normalized: BTreeMap<
    String,
    (
      octacity_server_store::WebhookDeliveryId,
      octacity_server_store::NormalizedWebhookEvent,
    ),
  >,
}

struct StaticDelivery {
  request: octacity_server_store::EnqueueWebhookDelivery,
  state: StaticDeliveryPhase,
  attempts: u16,
  trigger_attempts: u16,
  last_failure: Option<(octacity_server_store::WebhookFailureCode, String)>,
}

enum StaticDeliveryPhase {
  PendingVerification,
  RetryScheduled,
  PendingTrigger(octacity_server_store::NormalizedWebhookEvent),
  Completed,
  Suppressed,
  DeadLetter,
}

#[async_trait]
impl WebhookIntegrationReader for StaticExternalStore {
  async fn webhook_integration(&self, integration_id: IntegrationId) -> Result<WebhookIntegrationRecord, StoreError> {
    let integration = self.integration.lock().unwrap();
    assert_eq!(integration_id, integration.integration_id);
    Ok(integration.clone())
  }
}

#[async_trait]
impl WebhookDeliveryAdmissionStore for StaticExternalStore {
  async fn enqueue_webhook_delivery(
    &self,
    request: octacity_server_store::EnqueueWebhookDelivery,
  ) -> Result<StoreDisposition, StoreError> {
    request.validate()?;
    self.deliveries.lock().unwrap().deliveries.insert(
      request.delivery_id,
      StaticDelivery {
        request,
        state: StaticDeliveryPhase::PendingVerification,
        attempts: 0,
        trigger_attempts: 0,
        last_failure: None,
      },
    );
    Ok(StoreDisposition::Applied)
  }
}

#[async_trait]
impl WebhookDeliveryWorkStore for StaticExternalStore {
  async fn claim_webhook_deliveries(
    &self,
    request: octacity_server_store::ClaimWebhookDeliveries,
  ) -> Result<Vec<octacity_server_store::WebhookDeliveryClaim>, StoreError> {
    let mut state = self.deliveries.lock().unwrap();
    let mut claims = Vec::new();
    for delivery in state.deliveries.values_mut().take(usize::from(request.limit.get())) {
      let work = match &delivery.state {
        StaticDeliveryPhase::PendingVerification | StaticDeliveryPhase::RetryScheduled => {
          delivery.attempts += 1;
          octacity_server_store::WebhookDeliveryWork::Verify {
            headers: delivery.request.headers.clone(),
            body: delivery.request.body.clone(),
          }
        }
        StaticDeliveryPhase::PendingTrigger(event) => {
          delivery.trigger_attempts += 1;
          octacity_server_store::WebhookDeliveryWork::Trigger { event: event.clone() }
        }
        StaticDeliveryPhase::Completed | StaticDeliveryPhase::Suppressed | StaticDeliveryPhase::DeadLetter => continue,
      };
      claims.push(octacity_server_store::WebhookDeliveryClaim {
        delivery_id: delivery.request.delivery_id,
        integration_id: delivery.request.integration_id,
        received_at: delivery.request.received_at,
        verification_attempt: delivery.attempts,
        trigger_attempt: delivery.trigger_attempts,
        work,
        owner: request.owner.clone(),
        claim_expires_at: request.claim_expires_at,
      });
    }
    Ok(claims)
  }

  async fn record_webhook_event(
    &self,
    request: octacity_server_store::RecordWebhookEvent,
  ) -> Result<octacity_server_store::RecordWebhookEventOutcome, StoreError> {
    let mut state = self.deliveries.lock().unwrap();
    let key = format!(
      "{}:{}",
      request.event.integration_id, request.event.provider_delivery_id
    );
    let outcome = match state.normalized.get(&key) {
      None => {
        state
          .normalized
          .insert(key, (request.delivery_id, request.event.clone()));
        octacity_server_store::RecordWebhookEventOutcome::Recorded
      }
      Some((_, event)) if event == &request.event => octacity_server_store::RecordWebhookEventOutcome::Duplicate,
      Some(_) => octacity_server_store::RecordWebhookEventOutcome::IdentityConflict,
    };
    state.deliveries.get_mut(&request.delivery_id).unwrap().state = match outcome {
      octacity_server_store::RecordWebhookEventOutcome::Recorded => StaticDeliveryPhase::PendingTrigger(request.event),
      octacity_server_store::RecordWebhookEventOutcome::Duplicate
      | octacity_server_store::RecordWebhookEventOutcome::IdentityConflict => StaticDeliveryPhase::Completed,
    };
    Ok(outcome)
  }

  async fn complete_webhook_delivery(
    &self,
    request: octacity_server_store::CompleteWebhookDelivery,
  ) -> Result<StoreDisposition, StoreError> {
    self
      .deliveries
      .lock()
      .unwrap()
      .deliveries
      .get_mut(&request.delivery_id)
      .unwrap()
      .state = StaticDeliveryPhase::Completed;
    Ok(StoreDisposition::Applied)
  }

  async fn suppress_webhook_delivery(
    &self,
    request: octacity_server_store::SuppressWebhookDelivery,
  ) -> Result<StoreDisposition, StoreError> {
    let mut deliveries = self.deliveries.lock().unwrap();
    let delivery = deliveries.deliveries.get_mut(&request.delivery_id).unwrap();
    delivery.state = StaticDeliveryPhase::Suppressed;
    delivery.last_failure = None;
    Ok(StoreDisposition::Applied)
  }

  async fn fail_webhook_delivery(&self, request: octacity_server_store::FailWebhookDelivery) -> Result<(), StoreError> {
    let mut state = self.deliveries.lock().unwrap();
    let delivery = state.deliveries.get_mut(&request.delivery_id).unwrap();
    delivery.last_failure = Some((request.code, request.diagnostic));
    if request.retry_at.is_none() {
      delivery.state = StaticDeliveryPhase::DeadLetter;
    } else if !matches!(delivery.state, StaticDeliveryPhase::PendingTrigger(_)) {
      delivery.state = StaticDeliveryPhase::RetryScheduled;
    }
    Ok(())
  }
}

#[async_trait]
impl WebhookDeliveryQueryStore for StaticExternalStore {
  async fn webhook_delivery(
    &self,
    delivery_id: octacity_server_store::WebhookDeliveryId,
  ) -> Result<octacity_server_store::WebhookDeliveryDiagnostic, StoreError> {
    let state = self.deliveries.lock().unwrap();
    let delivery = state.deliveries.get(&delivery_id).ok_or(StoreError::NotFound {
      entity: EntityKind::Integration,
    })?;
    let delivery_state = match &delivery.state {
      StaticDeliveryPhase::PendingVerification => octacity_server_store::WebhookDeliveryState::PendingVerification,
      StaticDeliveryPhase::RetryScheduled => octacity_server_store::WebhookDeliveryState::RetryScheduled,
      StaticDeliveryPhase::PendingTrigger(_) => octacity_server_store::WebhookDeliveryState::PendingTrigger,
      StaticDeliveryPhase::Completed => octacity_server_store::WebhookDeliveryState::Completed,
      StaticDeliveryPhase::Suppressed => octacity_server_store::WebhookDeliveryState::Suppressed,
      StaticDeliveryPhase::DeadLetter => octacity_server_store::WebhookDeliveryState::DeadLetter,
    };
    Ok(octacity_server_store::WebhookDeliveryDiagnostic {
      delivery_id,
      integration_id: delivery.request.integration_id,
      state: delivery_state,
      verification_attempts: delivery.attempts,
      trigger_attempts: delivery.trigger_attempts,
      provider_delivery_id: None,
      failure_code: delivery.last_failure.as_ref().map(|value| value.0),
      diagnostic: delivery.last_failure.as_ref().map(|value| value.1.clone()),
      next_attempt_at: None,
    })
  }
}

struct StaticWebhookVerifier {
  calls: AtomicUsize,
  event: AuthenticatedWebhookEvent,
}

struct UnavailableVerifier(AtomicUsize);

#[async_trait]
impl WebhookDeliveryVerifier for UnavailableVerifier {
  async fn verify(
    &self,
    _delivery: VerifyWebhookDelivery,
  ) -> Result<AuthenticatedWebhookEvent, WebhookVerificationError> {
    self.0.fetch_add(1, Ordering::SeqCst);
    Err(WebhookVerificationError::provider(
      octacity_server_application::WebhookVerificationFailure::Unavailable,
      "rate_limited".to_owned(),
      "provider asked the server to retry".to_owned(),
      Some(250),
    ))
  }
}

#[async_trait]
impl WebhookDeliveryVerifier for StaticWebhookVerifier {
  async fn verify(
    &self,
    delivery: VerifyWebhookDelivery,
  ) -> Result<AuthenticatedWebhookEvent, WebhookVerificationError> {
    assert_eq!(delivery.body, br#"{"event":"push"}"#);
    assert_eq!(delivery.headers["x-signature"], "valid");
    self.calls.fetch_add(1, Ordering::SeqCst);
    Ok(self.event.clone())
  }
}

#[test]
fn managed_webhook_lifecycle_converges_after_lost_responses_and_reports_unsupported_operations() {
  run(async {
    let fixture = fixture();
    let store = Arc::new(ManagedExternalStore::default());
    let provider = Arc::new(ManagedProviderFixture::new(None, true));
    let service = managed_service(store.clone(), provider.clone());
    let first_integration = id::<IntegrationId>(91);
    let first_trigger = id::<TriggerId>(92);
    let protected = managed_create_command(&fixture, first_integration, first_trigger, time(100));
    let debug = format!("{protected:?}");
    assert!(!debug.contains(&protected.verification_material_handle));
    assert!(!debug.contains(&protected.administration_credential_handle));

    let pending = service.handle_command(protected).await.unwrap();
    assert_eq!(pending.integration_id, first_integration);
    assert_eq!(pending.registration, None);
    assert!(provider.calls().is_empty(), "commands must not race the durable worker");

    let worker = ManagedWebhookRegistrationWorker::new(
      store.clone(),
      provider.clone(),
      WebhookCallbackOrigin::new("https://hooks.example.test").unwrap(),
      DurableRetryPolicy::new(3, 10, 100).unwrap(),
    );
    let retried = worker
      .run_once(
        WorkerOwner::new("webhook:managed-first").unwrap(),
        time(110),
        time(200),
        1,
      )
      .await
      .unwrap();
    assert_eq!(retried.claimed, 1);
    assert_eq!(retried.retries_scheduled, 1);
    let retried = worker
      .run_once(
        WorkerOwner::new("webhook:managed-retry").unwrap(),
        time(120),
        time(200),
        1,
      )
      .await
      .unwrap();
    assert_eq!(retried.completed, 1);

    let created = service
      .handle_command(managed_create_command(
        &fixture,
        id::<IntegrationId>(93),
        id::<TriggerId>(94),
        time(200),
      ))
      .await
      .unwrap();
    assert_eq!(created.integration_id, first_integration);
    assert_eq!(created.trigger.id, first_trigger);
    assert_eq!(
      created.registration.unwrap().status,
      ManagedWebhookRegistrationStatus::Active
    );
    assert_eq!(created.disposition, ApplicationDisposition::Replayed);
    assert_eq!(provider.calls().len(), 2);
    assert!(provider.calls().iter().all(|call| call.1 == first_integration));

    let replayed = service
      .handle_command(managed_create_command(
        &fixture,
        id::<IntegrationId>(95),
        id::<TriggerId>(96),
        time(300),
      ))
      .await
      .unwrap();
    assert_eq!(replayed.integration_id, first_integration);
    assert_eq!(replayed.disposition, ApplicationDisposition::Replayed);
    assert_eq!(
      provider.calls().len(),
      2,
      "a lost REST response must replay local state"
    );

    for (operation, key, expected) in [
      (
        ManagedWebhookOperation::Observe,
        "observe-managed",
        ManagedWebhookRegistrationStatus::Active,
      ),
      (
        ManagedWebhookOperation::Rotate,
        "rotate-managed",
        ManagedWebhookRegistrationStatus::Active,
      ),
      (
        ManagedWebhookOperation::Delete,
        "delete-managed",
        ManagedWebhookRegistrationStatus::Missing,
      ),
    ] {
      let outcome = service
        .handle_command(ManageWebhookRegistrationCommand {
          integration_id: first_integration,
          operation,
          idempotency_key: key.parse().unwrap(),
          observed_at: time(400),
        })
        .await
        .unwrap();
      assert_eq!(
        outcome.registration.unwrap().status,
        ManagedWebhookRegistrationStatus::Active
      );
      assert_eq!(outcome.integration_id, first_integration);
      let completed = worker
        .run_once(
          WorkerOwner::new(format!("webhook:managed-{key}")).unwrap(),
          time(410),
          time(500),
          1,
        )
        .await
        .unwrap();
      assert_eq!(completed.completed, 1);
      assert_eq!(
        store
          .state
          .lock()
          .unwrap()
          .record
          .as_ref()
          .unwrap()
          .registration
          .as_ref()
          .unwrap()
          .status,
        expected.into()
      );
    }
    assert!(!store.state.lock().unwrap().record.as_ref().unwrap().enabled);

    let unsupported_store = Arc::new(ManagedExternalStore::default());
    let unsupported = managed_service(
      unsupported_store.clone(),
      Arc::new(ManagedProviderFixture::new(
        Some(ManagedWebhookOperation::Create),
        false,
      )),
    )
    .handle_command(managed_create_command(
      &fixture,
      id::<IntegrationId>(97),
      id::<TriggerId>(98),
      time(500),
    ))
    .await
    .unwrap_err();
    assert_eq!(unsupported.classification(), ApplicationFailure::CapabilityUnavailable);
    assert!(unsupported_store.state.lock().unwrap().record.is_none());
  });
}

fn managed_service(
  store: Arc<ManagedExternalStore>,
  provider: Arc<ManagedProviderFixture>,
) -> WebhookManagementService {
  WebhookManagementService::new(
    store.clone(),
    store.clone(),
    store,
    provider,
    Some(WebhookCallbackOrigin::new("https://hooks.example.test").unwrap()),
  )
}

fn managed_create_command(
  fixture: &Fixture,
  integration_id: IntegrationId,
  trigger_id: TriggerId,
  created_at: Timestamp,
) -> CreateManagedWebhookCommand {
  CreateManagedWebhookCommand {
    integration_id,
    trigger_id,
    configuration_id: fixture.context.configuration.id,
    configuration_version: fixture.context.configuration.version,
    enabled: true,
    adapter_id: "fixture".to_owned(),
    adapter_sha256: DIGEST.to_owned(),
    verification_material_handle: "secret:webhook-verification".to_owned(),
    verification_headers: vec!["x-signature".to_owned()],
    administration_credential_handle: "secret:provider-administration".to_owned(),
    repository_id: fixture.context.repository.id,
    event_kind: TriggerEventKind::new("push").unwrap(),
    parameters: BTreeMap::new(),
    priority: 0,
    idempotency_key: "managed-create".parse().unwrap(),
    created_at,
  }
}

#[derive(Default)]
struct ManagedExternalStore {
  state: Mutex<ManagedExternalState>,
}

#[derive(Default)]
struct ManagedExternalState {
  create_key: Option<IdempotencyKey>,
  record: Option<ManagedWebhookRecord>,
  operation_outcomes: BTreeMap<String, ManagedWebhookMutationOutcome>,
  pending: Option<PendingManagedOperation>,
}

struct PendingManagedOperation {
  operation: ManagedWebhookOperation,
  idempotency_key: IdempotencyKey,
  attempt: u16,
  due_at: Option<Timestamp>,
}

#[async_trait]
impl WebhookConfigurationStore for ManagedExternalStore {
  async fn create_unmanaged_webhook(
    &self,
    _request: CreateUnmanagedWebhook,
  ) -> Result<UnmanagedWebhookMutationOutcome, StoreError> {
    Err(StoreError::Unavailable)
  }

  async fn create_managed_webhook(
    &self,
    request: CreateManagedWebhook,
  ) -> Result<ManagedWebhookMutationOutcome, StoreError> {
    let mut state = self.state.lock().unwrap();
    if let Some(record) = &state.record {
      if state.create_key.as_ref() != Some(&request.idempotency_key) {
        return Err(StoreError::Conflict {
          entity: EntityKind::Integration,
        });
      }
      return Ok(ManagedWebhookMutationOutcome {
        disposition: StoreDisposition::Replayed,
        integration_id: record.integration_id,
        trigger: record.trigger,
        registration: record.registration.clone(),
      });
    }
    request.validate()?;
    let trigger = TriggerDefinitionRef {
      id: request.trigger.id,
      version: request.trigger.version,
    };
    state.create_key = Some(request.idempotency_key.clone());
    state.record = Some(ManagedWebhookRecord {
      integration_id: request.integration_id,
      trigger,
      target: TriggerTarget {
        configuration_id: request.trigger.configuration_id,
        configuration_version: request.trigger.configuration_version,
      },
      enabled: request.trigger.enabled,
      definition: request.definition,
      registration: None,
    });
    state.pending = Some(PendingManagedOperation {
      operation: ManagedWebhookOperation::Create,
      idempotency_key: request.idempotency_key.clone(),
      attempt: 0,
      due_at: Some(request.created_at),
    });
    Ok(ManagedWebhookMutationOutcome {
      disposition: StoreDisposition::Applied,
      integration_id: request.integration_id,
      trigger,
      registration: None,
    })
  }
}

#[async_trait]
impl ManagedWebhookRegistrationStore for ManagedExternalStore {
  async fn managed_webhook(&self, integration_id: IntegrationId) -> Result<ManagedWebhookRecord, StoreError> {
    self
      .state
      .lock()
      .unwrap()
      .record
      .clone()
      .filter(|record| record.integration_id == integration_id)
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Integration,
      })
  }
}

#[async_trait]
impl ManagedWebhookOperationStore for ManagedExternalStore {
  async fn enqueue_managed_webhook_operation(
    &self,
    request: EnqueueManagedWebhookOperation,
  ) -> Result<StoreDisposition, StoreError> {
    let mut state = self.state.lock().unwrap();
    if state.pending.is_some() {
      return Ok(StoreDisposition::Replayed);
    }
    state.pending = Some(PendingManagedOperation {
      operation: request.operation,
      idempotency_key: request.idempotency_key,
      attempt: 0,
      due_at: Some(request.requested_at),
    });
    Ok(StoreDisposition::Applied)
  }

  async fn claim_managed_webhook_operations(
    &self,
    request: octacity_server_store::ClaimManagedWebhookOperations,
  ) -> Result<Vec<ManagedWebhookOperationClaim>, StoreError> {
    let mut state = self.state.lock().unwrap();
    let Some(mut pending) = state.pending.take() else {
      return Ok(Vec::new());
    };
    if pending.due_at.is_none_or(|due_at| due_at > request.observed_at) {
      state.pending = Some(pending);
      return Ok(Vec::new());
    }
    pending.attempt += 1;
    let claim = ManagedWebhookOperationClaim {
      integration: state.record.clone().ok_or(StoreError::NotFound {
        entity: EntityKind::Integration,
      })?,
      operation: pending.operation,
      idempotency_key: pending.idempotency_key.clone(),
      attempt: pending.attempt,
      owner: request.owner,
      claim_expires_at: request.claim_expires_at,
    };
    state.pending = Some(pending);
    Ok(vec![claim])
  }

  async fn record_managed_webhook_registration(
    &self,
    request: RecordManagedWebhookRegistration,
  ) -> Result<ManagedWebhookMutationOutcome, StoreError> {
    let mut state = self.state.lock().unwrap();
    let key = format!("{:?}:{}", request.operation, request.idempotency_key);
    if let Some(outcome) = state.operation_outcomes.get(&key) {
      let mut outcome = outcome.clone();
      outcome.disposition = StoreDisposition::Replayed;
      return Ok(outcome);
    }
    let record = state.record.as_mut().ok_or(StoreError::NotFound {
      entity: EntityKind::Integration,
    })?;
    record.registration = Some(request.registration.clone());
    if request.operation == ManagedWebhookOperation::Delete {
      record.enabled = false;
    }
    let outcome = ManagedWebhookMutationOutcome {
      disposition: StoreDisposition::Applied,
      integration_id: record.integration_id,
      trigger: record.trigger,
      registration: record.registration.clone(),
    };
    state.pending = None;
    state.operation_outcomes.insert(key, outcome.clone());
    Ok(outcome)
  }

  async fn fail_managed_webhook_operation(&self, request: FailManagedWebhookOperation) -> Result<(), StoreError> {
    request.validate()?;
    let mut state = self.state.lock().unwrap();
    let pending = state.pending.as_mut().ok_or(StoreError::NotFound {
      entity: EntityKind::Integration,
    })?;
    assert_eq!(pending.operation, request.operation);
    assert_eq!(pending.idempotency_key, request.idempotency_key);
    pending.due_at = request.retry_at;
    Ok(())
  }
}

struct ManagedProviderFixture {
  unsupported: Option<ManagedWebhookOperation>,
  fail_first_create: bool,
  create_attempts: AtomicUsize,
  calls: Mutex<Vec<(ManagedWebhookOperation, IntegrationId, IdempotencyKey)>>,
}

impl ManagedProviderFixture {
  fn new(unsupported: Option<ManagedWebhookOperation>, fail_first_create: bool) -> Self {
    Self {
      unsupported,
      fail_first_create,
      create_attempts: AtomicUsize::new(0),
      calls: Mutex::default(),
    }
  }

  fn calls(&self) -> Vec<(ManagedWebhookOperation, IntegrationId, IdempotencyKey)> {
    self.calls.lock().unwrap().clone()
  }
}

#[async_trait]
impl WebhookManagementProvider for ManagedProviderFixture {
  async fn validate_configuration(
    &self,
    _adapter_id: &str,
    _adapter_sha256: &str,
  ) -> Result<(), WebhookVerificationError> {
    Ok(())
  }

  async fn validate_managed_operation(
    &self,
    _adapter_id: &str,
    _adapter_sha256: &str,
    operation: ManagedWebhookOperation,
  ) -> Result<(), WebhookVerificationError> {
    if self.unsupported == Some(operation) {
      Err(WebhookVerificationError::Unsupported)
    } else {
      Ok(())
    }
  }

  async fn manage_registration(
    &self,
    request: ManagedWebhookRegistrationRequest,
  ) -> Result<ManagedWebhookRegistration, WebhookVerificationError> {
    self.calls.lock().unwrap().push((
      request.operation,
      request.integration_id,
      request.idempotency_key.clone(),
    ));
    if request.operation == ManagedWebhookOperation::Create
      && self.fail_first_create
      && self.create_attempts.fetch_add(1, Ordering::SeqCst) == 0
    {
      return Err(WebhookVerificationError::Unavailable);
    }
    Ok(ManagedWebhookRegistration {
      registration_id: "remote-registration-01".to_owned(),
      status: if request.operation == ManagedWebhookOperation::Delete {
        ManagedWebhookRegistrationStatus::Missing
      } else {
        ManagedWebhookRegistrationStatus::Active
      },
      callback_url: request.callback_url,
    })
  }
}
