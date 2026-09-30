use super::*;

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

    let pending = management_command(&service, protected).await.unwrap();
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

    let created = management_command(
      &service,
      managed_create_command(&fixture, id::<IntegrationId>(93), id::<TriggerId>(94), time(200)),
    )
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

    let replayed = management_command(
      &service,
      managed_create_command(&fixture, id::<IntegrationId>(95), id::<TriggerId>(96), time(300)),
    )
    .await
    .unwrap();
    assert_eq!(replayed.integration_id, first_integration);
    assert_eq!(replayed.disposition, ApplicationDisposition::Replayed);
    assert_eq!(
      provider.calls().len(),
      2,
      "a lost REST response must replay local state"
    );

    macro_rules! exercise_operation {
      ($command:ident, $key:literal, $expected:expr) => {{
        let outcome = management_command(
          &service,
          $command {
            integration_id: first_integration,
            idempotency_key: $key.parse().unwrap(),
            observed_at: time(400),
          },
        )
        .await
        .unwrap();
        assert_eq!(
          outcome.registration.unwrap().status,
          ManagedWebhookRegistrationStatus::Active
        );
        assert_eq!(outcome.integration_id, first_integration);
        let completed = worker
          .run_once(
            WorkerOwner::new(format!("webhook:managed-{}", $key)).unwrap(),
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
          $expected.into()
        );
      }};
    }
    exercise_operation!(
      ObserveManagedWebhookRegistrationCommand,
      "observe-managed",
      ManagedWebhookRegistrationStatus::Active
    );
    exercise_operation!(
      RotateManagedWebhookRegistrationCommand,
      "rotate-managed",
      ManagedWebhookRegistrationStatus::Active
    );
    exercise_operation!(
      DeleteManagedWebhookRegistrationCommand,
      "delete-managed",
      ManagedWebhookRegistrationStatus::Missing
    );
    assert!(!store.state.lock().unwrap().record.as_ref().unwrap().enabled);

    let unsupported_store = Arc::new(ManagedExternalStore::default());
    let unsupported_service = managed_service(
      unsupported_store.clone(),
      Arc::new(ManagedProviderFixture::new(
        Some(ManagedWebhookOperation::Create),
        false,
      )),
    );
    let unsupported = management_command(
      &unsupported_service,
      managed_create_command(&fixture, id::<IntegrationId>(97), id::<TriggerId>(98), time(500)),
    )
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
    _request: octacity_server_store::ManagementMutation<CreateUnmanagedWebhook>,
  ) -> Result<UnmanagedWebhookMutationOutcome, StoreError> {
    Err(StoreError::Unavailable)
  }

  async fn create_managed_webhook(
    &self,
    request: octacity_server_store::ManagementMutation<CreateManagedWebhook>,
  ) -> Result<ManagedWebhookMutationOutcome, StoreError> {
    let (request, _audit) = request.into_parts();
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
    request: octacity_server_store::ManagementMutation<EnqueueManagedWebhookOperation>,
  ) -> Result<StoreDisposition, StoreError> {
    let (request, _audit) = request.into_parts();
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
