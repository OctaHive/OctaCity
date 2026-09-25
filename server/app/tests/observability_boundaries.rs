use std::sync::Arc;

use async_trait::async_trait;
use axum::{body::Body, http::Request};
use metrics_exporter_prometheus::PrometheusBuilder;
use octacity_server_api_rest::management_router;
use octacity_server_api_webhook::{MAX_WEBHOOK_DELIVERY_BYTES, WebhookApplication, webhook_router};
use octacity_server_application::{
  AcceptWebhookDeliveryCommand, CommandHandler, MutationDisposition, WebhookDeliveryAccepted, WebhookDeliveryError,
};
use tower::ServiceExt as _;

struct AcceptingWebhookHandler;

#[async_trait]
impl CommandHandler<AcceptWebhookDeliveryCommand> for AcceptingWebhookHandler {
  type Error = WebhookDeliveryError;

  async fn handle_command(
    &self,
    _command: AcceptWebhookDeliveryCommand,
  ) -> Result<WebhookDeliveryAccepted, Self::Error> {
    Ok(WebhookDeliveryAccepted {
      delivery_id: "22222222-2222-4222-8222-222222222222".parse().unwrap(),
      disposition: MutationDisposition::Applied,
    })
  }
}

#[tokio::test]
async fn real_http_boundaries_export_success_and_rejection() {
  let handle = PrometheusBuilder::new()
    .with_recommended_naming(true)
    .install_recorder()
    .unwrap();

  let management = management_router(|| true)
    .oneshot(Request::builder().uri("/health/live").body(Body::empty()).unwrap())
    .await
    .unwrap();
  let webhook = webhook_router(WebhookApplication::new(Arc::new(AcceptingWebhookHandler)));
  let path = "/webhooks/v1/integrations/11111111-1111-4111-8111-111111111111";
  let accepted = webhook
    .clone()
    .oneshot(Request::post(path).body(Body::from("delivery")).unwrap())
    .await
    .unwrap();
  let rejected = webhook
    .oneshot(
      Request::post(path)
        .body(Body::from(vec![0; MAX_WEBHOOK_DELIVERY_BYTES + 1]))
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(management.status(), axum::http::StatusCode::OK);
  assert_eq!(accepted.status(), axum::http::StatusCode::ACCEPTED);
  assert_eq!(rejected.status(), axum::http::StatusCode::PAYLOAD_TOO_LARGE);
  let rendered = handle.render();
  assert!(rendered.contains("octacity_server_http_requests_total"), "{rendered}");
  assert!(rendered.contains("http_request_method=\"GET\""), "{rendered}");
  assert!(rendered.contains("http_request_method=\"POST\""), "{rendered}");
  assert!(rendered.contains("http_route_group=\"health\""), "{rendered}");
  assert!(rendered.contains("http_route_group=\"webhook\""), "{rendered}");
  assert!(rendered.contains("outcome=\"success\""), "{rendered}");
  assert!(rendered.contains("outcome=\"rejected\""), "{rendered}");
}
