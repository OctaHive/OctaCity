use std::{collections::BTreeMap, sync::Arc};

use octacity_server_domain::{Timestamp, TriggerIdentity};
use octacity_server_store::{
  ClaimManagedWebhookOperations, ClaimWebhookDeliveries, CompleteWebhookDelivery, FailManagedWebhookOperation,
  FailWebhookDelivery, ManagedWebhookOperationStore, NormalizedWebhookEvent, RecordManagedWebhookRegistration,
  RecordWebhookEvent, RecordWebhookEventOutcome, StoreError, SuppressWebhookDelivery, TriggerCause, TriggerMetadata,
  WebhookDeliveryWork, WebhookDeliveryWorkStore, WebhookFailureCode, WebhookIntegrationReader, WorkerOwner,
};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::{
  ApplicationFailure, AuthenticatedWebhookEvent, DurableRetryPolicy, ManagedWebhookRegistrationRequest,
  ManualSourceSelection, ManualTriggerCommand, ManualTriggerError, ManualTriggerService, RevisionResolutionError,
  VerifyWebhookDelivery, WebhookCallbackOrigin, WebhookDeliveryVerifier, WebhookManagementProvider,
  WebhookVerificationFailure,
  diagnostic::bounded_diagnostic,
  external_trigger::{durable_webhook_diagnostic, ensure_managed_result, webhook_failure_code},
};

/// Counts produced by one bounded webhook delivery pass.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WebhookDeliveryBatchOutcome {
  /// Durable receipts acquired by this replica.
  pub claimed: usize,
  /// Authenticated events newly normalized.
  pub normalized: usize,
  /// Repeated provider identities completed without another Trigger.
  pub duplicates: usize,
  /// Normalized events evaluated through the Trigger Engine.
  pub evaluated: usize,
  /// Receipts terminated because their integration was disabled.
  pub suppressed: usize,
  /// Transient adapter attempts scheduled for another pass.
  pub retries_scheduled: usize,
  /// Permanent or retry-exhausted receipts moved to dead letter.
  pub dead_letters: usize,
}

/// Counts produced by one bounded managed-registration retry pass.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ManagedWebhookBatchOutcome {
  /// Durable operations acquired by this replica.
  pub claimed: usize,
  /// Provider operations completed and committed.
  pub completed: usize,
  /// Transient provider failures scheduled for retry.
  pub retries_scheduled: usize,
  /// Permanent or retry-exhausted operations retained as dead letters.
  pub dead_letters: usize,
}

/// Restart-safe executor for durable managed-provider operations.
pub struct ManagedWebhookRegistrationWorker {
  store: Arc<dyn ManagedWebhookOperationStore>,
  provider: Arc<dyn WebhookManagementProvider>,
  callback_origin: WebhookCallbackOrigin,
  retry_policy: DurableRetryPolicy,
}

impl ManagedWebhookRegistrationWorker {
  /// Creates a worker over durable operation state and a replaceable provider host.
  pub fn new(
    store: Arc<dyn ManagedWebhookOperationStore>,
    provider: Arc<dyn WebhookManagementProvider>,
    callback_origin: WebhookCallbackOrigin,
    retry_policy: DurableRetryPolicy,
  ) -> Self {
    Self {
      store,
      provider,
      callback_origin,
      retry_policy,
    }
  }

  /// Claims and advances one bounded batch at explicit authoritative times.
  pub async fn run_once(
    &self,
    owner: WorkerOwner,
    observed_at: Timestamp,
    claim_expires_at: Timestamp,
    limit: u16,
  ) -> Result<ManagedWebhookBatchOutcome, WebhookWorkerError> {
    let claims = self
      .store
      .claim_managed_webhook_operations(ClaimManagedWebhookOperations::new(
        owner,
        observed_at,
        claim_expires_at,
        limit,
      )?)
      .await?;
    let mut outcome = ManagedWebhookBatchOutcome {
      claimed: claims.len(),
      ..ManagedWebhookBatchOutcome::default()
    };
    for claim in claims {
      let integration = claim.integration;
      let callback_url = self.callback_origin.callback_url(integration.integration_id);
      let result = self
        .provider
        .manage_registration(ManagedWebhookRegistrationRequest {
          adapter_id: integration.definition.delivery.adapter_id,
          adapter_sha256: integration.definition.delivery.adapter_sha256,
          operation: claim.operation,
          integration_id: integration.integration_id,
          idempotency_key: claim.idempotency_key.clone(),
          callback_url: callback_url.clone(),
          administration_credential_handle: integration.definition.administration_credential_handle,
        })
        .await;
      match result {
        Ok(registration) if ensure_managed_result(&registration, &callback_url, claim.operation).is_ok() => {
          self
            .store
            .record_managed_webhook_registration(RecordManagedWebhookRegistration {
              integration_id: integration.integration_id,
              operation: claim.operation,
              registration: registration.into(),
              idempotency_key: claim.idempotency_key,
              owner: claim.owner,
              observed_at,
            })
            .await?;
          outcome.completed += 1;
        }
        result => {
          let (classification, diagnostic, retry_hint) = match result {
            Ok(_) => (
              WebhookVerificationFailure::InvalidResponse,
              "managed webhook provider returned an invalid registration".to_owned(),
              None,
            ),
            Err(error) if error.classification() == WebhookVerificationFailure::Cancelled => {
              return Err(WebhookWorkerError::AdapterCancelled);
            }
            Err(error) => (
              error.classification(),
              durable_webhook_diagnostic(&error),
              error.retry_after_milliseconds(),
            ),
          };
          let retry_at = (classification == WebhookVerificationFailure::Unavailable)
            .then(|| {
              self
                .retry_policy
                .retry_at_with_hint(claim.attempt, observed_at, retry_hint)
            })
            .flatten();
          self
            .store
            .fail_managed_webhook_operation(FailManagedWebhookOperation {
              integration_id: integration.integration_id,
              operation: claim.operation,
              idempotency_key: claim.idempotency_key,
              owner: claim.owner,
              code: webhook_failure_code(classification),
              diagnostic,
              failed_at: observed_at,
              retry_at,
            })
            .await?;
          if retry_at.is_some() {
            outcome.retries_scheduled += 1;
          } else {
            outcome.dead_letters += 1;
          }
        }
      }
    }
    Ok(outcome)
  }
}

/// Restart-safe executor for raw webhook authentication and normalized Trigger work.
pub struct WebhookDeliveryWorker {
  integrations: Arc<dyn WebhookIntegrationReader>,
  work: Arc<dyn WebhookDeliveryWorkStore>,
  triggers: Arc<ManualTriggerService>,
  verifier: Arc<dyn WebhookDeliveryVerifier>,
  retry_policy: DurableRetryPolicy,
}

impl WebhookDeliveryWorker {
  /// Creates a worker over durable delivery state and replaceable adapters.
  pub fn new(
    integrations: Arc<dyn WebhookIntegrationReader>,
    work: Arc<dyn WebhookDeliveryWorkStore>,
    triggers: Arc<ManualTriggerService>,
    verifier: Arc<dyn WebhookDeliveryVerifier>,
    retry_policy: DurableRetryPolicy,
  ) -> Self {
    Self {
      integrations,
      work,
      triggers,
      verifier,
      retry_policy,
    }
  }

  /// Claims and advances one bounded batch at explicit authoritative times.
  pub async fn run_once(
    &self,
    owner: WorkerOwner,
    observed_at: Timestamp,
    claim_expires_at: Timestamp,
    limit: u16,
  ) -> Result<WebhookDeliveryBatchOutcome, WebhookWorkerError> {
    let claims = self
      .work
      .claim_webhook_deliveries(ClaimWebhookDeliveries::new(
        owner,
        observed_at,
        claim_expires_at,
        limit,
      )?)
      .await?;
    let mut outcome = WebhookDeliveryBatchOutcome {
      claimed: claims.len(),
      ..WebhookDeliveryBatchOutcome::default()
    };
    for claim in claims {
      let integration = self.integrations.webhook_integration(claim.integration_id).await?;
      if !integration.enabled {
        self
          .work
          .suppress_webhook_delivery(SuppressWebhookDelivery {
            delivery_id: claim.delivery_id,
            owner: claim.owner,
            suppressed_at: observed_at,
          })
          .await?;
        outcome.suppressed += 1;
        continue;
      }
      match claim.work {
        WebhookDeliveryWork::Verify { headers, body } => {
          let verification = self
            .verifier
            .verify(VerifyWebhookDelivery {
              delivery_id: claim.delivery_id,
              integration_id: claim.integration_id,
              adapter_id: integration.definition.adapter_id.clone(),
              adapter_sha256: integration.definition.adapter_sha256.clone(),
              verification_material_handle: integration.definition.verification_material_handle.clone(),
              headers,
              body,
            })
            .await;
          match verification {
            Ok(event) if event_matches(&event, &integration) => {
              let recorded = self
                .work
                .record_webhook_event(RecordWebhookEvent {
                  delivery_id: claim.delivery_id,
                  owner: claim.owner,
                  event: event.into(),
                  recorded_at: observed_at,
                })
                .await?;
              match recorded {
                RecordWebhookEventOutcome::Recorded => outcome.normalized += 1,
                RecordWebhookEventOutcome::Duplicate => outcome.duplicates += 1,
                RecordWebhookEventOutcome::IdentityConflict => outcome.dead_letters += 1,
              }
            }
            Ok(_) => {
              self
                .dead_letter(
                  claim.delivery_id,
                  claim.owner,
                  observed_at,
                  WebhookFailureCode::NormalizedEventMismatch,
                  "authenticated event does not match immutable integration configuration",
                )
                .await?;
              outcome.dead_letters += 1;
            }
            Err(error) => {
              let classification = error.classification();
              if classification == WebhookVerificationFailure::Cancelled {
                return Err(WebhookWorkerError::AdapterCancelled);
              }
              let code = webhook_failure_code(classification);
              let retry_at = if classification == WebhookVerificationFailure::Unavailable {
                self.retry_policy.retry_at_with_hint(
                  claim.verification_attempt,
                  observed_at,
                  error.retry_after_milliseconds(),
                )
              } else {
                None
              };
              self
                .work
                .fail_webhook_delivery(FailWebhookDelivery {
                  delivery_id: claim.delivery_id,
                  owner: claim.owner,
                  code,
                  diagnostic: durable_webhook_diagnostic(&error),
                  failed_at: observed_at,
                  retry_at,
                })
                .await?;
              if retry_at.is_some() {
                outcome.retries_scheduled += 1;
              } else {
                outcome.dead_letters += 1;
              }
            }
          }
        }
        WebhookDeliveryWork::Trigger { event } => {
          if !normalized_event_matches(&event, &integration) {
            self
              .dead_letter(
                claim.delivery_id,
                claim.owner,
                observed_at,
                WebhookFailureCode::NormalizedEventMismatch,
                "persisted webhook event does not match immutable integration configuration",
              )
              .await?;
            outcome.dead_letters += 1;
            continue;
          }
          match evaluate_event(&self.triggers, integration, event, claim.received_at, observed_at).await {
            Ok(()) => {
              self
                .work
                .complete_webhook_delivery(CompleteWebhookDelivery {
                  delivery_id: claim.delivery_id,
                  owner: claim.owner,
                  completed_at: observed_at,
                })
                .await?;
              outcome.evaluated += 1;
            }
            Err(ManualTriggerError::Revision(RevisionResolutionError::Cancelled)) => {
              return Err(WebhookWorkerError::AdapterCancelled);
            }
            Err(error) => {
              let retry_at = (error.classification() == ApplicationFailure::Unavailable)
                .then(|| self.retry_policy.retry_at(claim.trigger_attempt, observed_at))
                .flatten();
              self
                .work
                .fail_webhook_delivery(FailWebhookDelivery {
                  delivery_id: claim.delivery_id,
                  owner: claim.owner,
                  code: WebhookFailureCode::TriggerEvaluationFailed,
                  diagnostic: bounded_diagnostic(
                    &error.to_string(),
                    octacity_server_store::MAX_WEBHOOK_DIAGNOSTIC_BYTES,
                    "webhook Trigger evaluation failed",
                  ),
                  failed_at: observed_at,
                  retry_at,
                })
                .await?;
              if retry_at.is_some() {
                outcome.retries_scheduled += 1;
              } else {
                outcome.dead_letters += 1;
              }
            }
          }
        }
      }
    }
    Ok(outcome)
  }

  async fn dead_letter(
    &self,
    delivery_id: octacity_server_store::WebhookDeliveryId,
    owner: WorkerOwner,
    failed_at: Timestamp,
    code: WebhookFailureCode,
    diagnostic: &str,
  ) -> Result<(), StoreError> {
    self
      .work
      .fail_webhook_delivery(FailWebhookDelivery {
        delivery_id,
        owner,
        code,
        diagnostic: diagnostic.to_owned(),
        failed_at,
        retry_at: None,
      })
      .await
  }
}

async fn evaluate_event(
  triggers: &ManualTriggerService,
  integration: octacity_server_store::WebhookIntegrationRecord,
  event: NormalizedWebhookEvent,
  received_at: Timestamp,
  accepted_at: Timestamp,
) -> Result<(), ManualTriggerError> {
  let source = match (event.revision.clone(), event.reference.clone()) {
    (Some(revision), _) => ManualSourceSelection::ExactRevision(revision),
    (None, Some(reference)) => ManualSourceSelection::Reference(reference),
    (None, None) => ManualSourceSelection::DefaultReference,
  };
  let source_time = event.provider_time.unwrap_or(received_at);
  let mut metadata = event
    .metadata
    .into_iter()
    .map(|(name, value)| (name, Value::String(value)))
    .collect::<BTreeMap<_, _>>();
  if let Some(actor) = event.actor_display_name {
    metadata.insert("actor_display_name".to_owned(), Value::String(actor));
  }
  let deduplication_identity = scoped_delivery_identity(integration.integration_id, &event.provider_delivery_id)?;
  triggers
    .accept_external(
      ManualTriggerCommand {
        trigger: integration.trigger,
        target: integration.target,
        deduplication_identity,
        source,
        parameters: integration.definition.parameters,
        priority: integration.definition.priority,
        observed_at: source_time,
      },
      accepted_at,
      TriggerCause::External {
        integration_id: integration.integration_id,
        repository_id: event.repository_id,
        event_kind: event.event_kind,
        reference: event.reference,
        revision: event.revision,
      },
      TriggerMetadata::new(metadata)
        .map_err(|_| ManualTriggerError::Invalid(crate::ManualTriggerInputError::ContextMismatch))?,
    )
    .await?;
  Ok(())
}

fn scoped_delivery_identity(
  integration_id: octacity_server_domain::IntegrationId,
  provider_delivery_id: &TriggerIdentity,
) -> Result<TriggerIdentity, ManualTriggerError> {
  let identity = Uuid::new_v5(&integration_id.as_uuid(), provider_delivery_id.as_str().as_bytes());
  TriggerIdentity::new(format!("webhook:{identity}"))
    .map_err(|_| ManualTriggerError::Invalid(crate::ManualTriggerInputError::ContextMismatch))
}

fn event_matches(
  event: &AuthenticatedWebhookEvent,
  integration: &octacity_server_store::WebhookIntegrationRecord,
) -> bool {
  event.integration_id == integration.integration_id
    && event.repository_id == integration.definition.repository_id
    && event.event_kind == integration.definition.event_kind
}

fn normalized_event_matches(
  event: &NormalizedWebhookEvent,
  integration: &octacity_server_store::WebhookIntegrationRecord,
) -> bool {
  event.integration_id == integration.integration_id
    && event.repository_id == integration.definition.repository_id
    && event.event_kind == integration.definition.event_kind
}

impl From<AuthenticatedWebhookEvent> for NormalizedWebhookEvent {
  fn from(value: AuthenticatedWebhookEvent) -> Self {
    Self {
      integration_id: value.integration_id,
      provider_delivery_id: value.delivery_id,
      event_kind: value.event_kind,
      repository_id: value.repository_id,
      reference: value.reference,
      revision: value.revision,
      provider_time: value.provider_time,
      actor_display_name: value.actor_display_name,
      metadata: value.metadata,
    }
  }
}

/// Failure from one bounded durable webhook worker pass.
#[derive(Debug, Error)]
pub enum WebhookWorkerError {
  /// Durable delivery work could not be claimed or advanced.
  #[error("webhook delivery store failed")]
  Store(#[from] StoreError),
  /// An adapter operation stopped cooperatively with the process cancellation tree.
  #[error("webhook adapter operation was cancelled")]
  AdapterCancelled,
}
