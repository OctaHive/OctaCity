use super::*;

/// Application service for bounded durable public webhook admission.
pub struct WebhookIngressService {
  integrations: Arc<dyn WebhookIntegrationReader>,
  deliveries: Arc<dyn WebhookDeliveryAdmissionStore>,
}

impl WebhookIngressService {
  /// Creates ingress over the durable delivery port.
  #[must_use]
  pub fn new(
    integrations: Arc<dyn WebhookIntegrationReader>,
    deliveries: Arc<dyn WebhookDeliveryAdmissionStore>,
  ) -> Self {
    Self {
      integrations,
      deliveries,
    }
  }
}

#[async_trait]
impl CommandHandler<AcceptWebhookDeliveryCommand> for WebhookIngressService {
  type Error = WebhookDeliveryError;

  async fn handle_command(
    &self,
    command: AcceptWebhookDeliveryCommand,
  ) -> Result<WebhookDeliveryAccepted, Self::Error> {
    let integration = self.integrations.webhook_integration(command.integration_id).await?;
    let headers = integration
      .definition
      .verification_headers
      .iter()
      .filter_map(|name| command.headers.get(name).map(|value| (name.clone(), value.clone())))
      .collect::<BTreeMap<_, _>>();
    if headers.len() != integration.definition.verification_headers.len() {
      return Err(WebhookDeliveryError::AuthenticationFailed);
    }
    let delivery_id = WebhookDeliveryId::generate();
    let disposition = self
      .deliveries
      .enqueue_webhook_delivery(EnqueueWebhookDelivery {
        delivery_id,
        integration_id: command.integration_id,
        headers,
        body: command.body,
        received_at: command.received_at,
      })
      .await?
      .into();
    Ok(WebhookDeliveryAccepted {
      delivery_id,
      disposition,
    })
  }
}
