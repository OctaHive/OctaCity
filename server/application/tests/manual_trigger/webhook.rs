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

#[test]
fn unauthenticated_deliveries_never_reach_the_trigger_engine() {
  run(async {
    let mut fixture = fixture();
    fixture.context.configuration.definition.triggers.allowed = BTreeSet::from([TriggerKind::External]);
    let acceptance = Arc::new(RecordingStore::default());
    let integration_id = id::<IntegrationId>(87);
    let store = Arc::new(StaticExternalStore {
      integration: Mutex::new(WebhookIntegrationRecord {
        integration_id,
        trigger: fixture.command.trigger,
        target: fixture.command.target,
        enabled: true,
        definition: UnmanagedWebhookDefinition {
          adapter_id: "rejecting".to_owned(),
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
    let missing_proof = ingress
      .handle_command(AcceptWebhookDeliveryCommand {
        integration_id,
        headers: BTreeMap::new(),
        body: b"delivery".to_vec(),
        received_at: time(100),
      })
      .await;
    assert!(matches!(
      missing_proof,
      Err(octacity_server_application::WebhookDeliveryError::AuthenticationFailed)
    ));
    assert!(store.deliveries.lock().unwrap().deliveries.is_empty());

    let accepted = ingress
      .handle_command(AcceptWebhookDeliveryCommand {
        integration_id,
        headers: BTreeMap::from([("x-signature".to_owned(), "invalid".to_owned())]),
        body: b"delivery".to_vec(),
        received_at: time(110),
      })
      .await
      .unwrap();
    let trigger_engine = Arc::new(ManualTriggerService::new(
      acceptance.clone(),
      Arc::new(StaticContext(fixture.context)),
      Arc::new(RecordingResolver::succeed("0123456789abcdef")),
    ));
    let outcome = WebhookDeliveryWorker::new(
      store.clone(),
      store.clone(),
      trigger_engine,
      Arc::new(RejectingWebhookVerifier),
      DurableRetryPolicy::new(3, 10, 100).unwrap(),
    )
    .run_once(WorkerOwner::new("webhook:reject").unwrap(), time(120), time(200), 1)
    .await
    .unwrap();

    assert_eq!(outcome.dead_letters, 1);
    assert_eq!(outcome.retries_scheduled, 0);
    assert_eq!(acceptance.calls(), 0);
    let diagnostic = store.webhook_delivery(accepted.delivery_id).await.unwrap();
    assert_eq!(
      diagnostic.failure_code,
      Some(octacity_server_store::WebhookFailureCode::AuthenticationFailed)
    );
    assert_eq!(
      diagnostic.state,
      octacity_server_store::WebhookDeliveryState::DeadLetter
    );
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

struct RejectingWebhookVerifier;

#[async_trait]
impl WebhookDeliveryVerifier for RejectingWebhookVerifier {
  async fn verify(
    &self,
    _delivery: VerifyWebhookDelivery,
  ) -> Result<AuthenticatedWebhookEvent, WebhookVerificationError> {
    Err(WebhookVerificationError::AuthenticationFailed)
  }
}

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

#[path = "webhook/managed.rs"]
mod managed;
