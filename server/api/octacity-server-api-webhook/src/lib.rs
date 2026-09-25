//! Durable public webhook HTTP ingress.
//!
//! The adapter preserves exact bounded body bytes and transport headers until
//! a typed application handler has committed a durable delivery receipt.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::{
  collections::BTreeMap,
  sync::Arc,
  time::{Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
  Json, Router,
  body::Bytes,
  extract::{DefaultBodyLimit, Path, Request, State},
  http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
  middleware::{self, Next},
  response::{IntoResponse, Response},
  routing::post,
};
use octacity_observability::{HttpMethod, HttpRoute, TraceSpan, classify_http_response, record_http_request};
use octacity_server_application::{
  AcceptWebhookDeliveryCommand, CommandHandler, WebhookDeliveryError, WebhookDeliveryFailure,
};
use serde::Serialize;
use tracing::Instrument as _;
use uuid::Uuid;

/// Maximum exact raw body accepted by public webhook ingress.
pub const MAX_WEBHOOK_DELIVERY_BYTES: usize = octacity_server_application::MAX_WEBHOOK_DELIVERY_BYTES;
const MAX_TRANSPORT_HEADERS: usize = 128;
static REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

type DeliveryAccept = dyn CommandHandler<AcceptWebhookDeliveryCommand, Error = WebhookDeliveryError>;

/// Type-erased application capability consumed by public webhook ingress.
pub struct WebhookApplication(Arc<DeliveryAccept>);

impl WebhookApplication {
  /// Erases one typed delivery handler without exposing infrastructure to HTTP.
  pub fn new<H>(handler: Arc<H>) -> Self
  where
    H: CommandHandler<AcceptWebhookDeliveryCommand, Error = WebhookDeliveryError> + 'static,
  {
    Self(handler)
  }
}

/// Builds the bounded durable public webhook router.
pub fn webhook_router(application: WebhookApplication) -> Router {
  Router::new()
    .route("/webhooks/v1/integrations/{integration_id}", post(accept_delivery))
    .layer(DefaultBodyLimit::max(MAX_WEBHOOK_DELIVERY_BYTES))
    .with_state(Arc::new(application))
    .layer(middleware::from_fn(request_context))
}

async fn request_context(request: Request, next: Next) -> Response {
  let request_id = Uuid::new_v4().to_string();
  let request_id_header = HeaderValue::from_str(&request_id).expect("UUID is always a valid header value");
  let method = request.method().as_str().to_owned();
  let span = tracing::info_span!(
    TraceSpan::ServerHttpRequest.as_str(),
    request.id = request_id,
    http.request.method = method,
    http.route = "/webhooks/v1/integrations/{integration_id}",
  );
  async move {
    let started = Instant::now();
    let mut response = next.run(request).await;
    response
      .headers_mut()
      .insert(REQUEST_ID_HEADER.clone(), request_id_header);
    let status = response.status().as_u16();
    record_http_request(
      HttpMethod::from_token(&method),
      HttpRoute::Webhook,
      classify_http_response(status),
      status,
      started.elapsed(),
    );
    response
  }
  .instrument(span)
  .await
}

#[derive(Serialize)]
struct DeliveryResponse {
  delivery_id: String,
  disposition: octacity_server_application::MutationDisposition,
}

async fn accept_delivery(
  State(application): State<Arc<WebhookApplication>>,
  Path(integration_id): Path<String>,
  headers: HeaderMap,
  body: Bytes,
) -> Result<(StatusCode, Json<DeliveryResponse>), WebhookApiError> {
  let received_at_unix_ms = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .ok()
    .and_then(|duration| i64::try_from(duration.as_millis()).ok())
    .ok_or_else(WebhookApiError::unavailable)?;
  let headers = normalized_headers(&headers)?;
  let command =
    AcceptWebhookDeliveryCommand::from_transport(&integration_id, headers, body.to_vec(), received_at_unix_ms)
      .map_err(|_| WebhookApiError::invalid())?;
  let outcome = application
    .0
    .handle_command(command)
    .await
    .map_err(WebhookApiError::application)?;
  let response = DeliveryResponse {
    delivery_id: outcome.delivery_id.to_string(),
    disposition: outcome.disposition,
  };
  Ok((StatusCode::ACCEPTED, Json(response)))
}

fn normalized_headers(headers: &HeaderMap) -> Result<BTreeMap<String, String>, WebhookApiError> {
  if headers.len() > MAX_TRANSPORT_HEADERS {
    return Err(WebhookApiError::invalid());
  }
  let mut normalized = BTreeMap::new();
  for name in headers.keys() {
    let values = headers.get_all(name);
    let mut values = values.iter();
    let value = values
      .next()
      .and_then(|value| value.to_str().ok())
      .ok_or_else(WebhookApiError::invalid)?;
    if values.next().is_some() || value.len() > octacity_server_application::MAX_WEBHOOK_HEADER_VALUE_BYTES {
      return Err(WebhookApiError::invalid());
    }
    normalized.insert(name.as_str().to_owned(), value.to_owned());
  }
  Ok(normalized)
}

#[derive(Serialize)]
struct ErrorBody {
  code: &'static str,
  message: &'static str,
}

struct WebhookApiError {
  status: StatusCode,
  code: &'static str,
  message: &'static str,
}

impl WebhookApiError {
  const fn invalid() -> Self {
    Self {
      status: StatusCode::BAD_REQUEST,
      code: "invalid_request",
      message: "webhook delivery is invalid",
    }
  }

  const fn unavailable() -> Self {
    Self {
      status: StatusCode::SERVICE_UNAVAILABLE,
      code: "unavailable",
      message: "webhook delivery cannot be processed",
    }
  }

  fn application(error: WebhookDeliveryError) -> Self {
    match error.classification() {
      WebhookDeliveryFailure::Unauthorized => Self {
        status: StatusCode::UNAUTHORIZED,
        code: "authentication_failed",
        message: "webhook delivery authentication failed",
      },
      WebhookDeliveryFailure::NotFound => Self {
        status: StatusCode::NOT_FOUND,
        code: "not_found",
        message: "webhook integration was not found",
      },
      WebhookDeliveryFailure::Invalid => Self::invalid(),
      WebhookDeliveryFailure::Unavailable => Self::unavailable(),
    }
  }
}

impl IntoResponse for WebhookApiError {
  fn into_response(self) -> Response {
    (
      self.status,
      Json(ErrorBody {
        code: self.code,
        message: self.message,
      }),
    )
      .into_response()
  }
}

#[cfg(test)]
mod tests {
  use std::sync::atomic::{AtomicUsize, Ordering};

  use async_trait::async_trait;
  use axum::{body::Body, http::Request};
  use tower::ServiceExt as _;

  use super::*;

  struct InspectingHandler(AtomicUsize);

  struct AcceptingHandler;

  #[async_trait]
  impl CommandHandler<AcceptWebhookDeliveryCommand> for InspectingHandler {
    type Error = WebhookDeliveryError;

    async fn handle_command(
      &self,
      command: AcceptWebhookDeliveryCommand,
    ) -> Result<octacity_server_application::WebhookDeliveryAccepted, Self::Error> {
      assert_eq!(command.body, b"exact\r\nbody\0bytes");
      assert_eq!(command.headers["x-signature"], "proof");
      self.0.fetch_add(1, Ordering::SeqCst);
      Err(WebhookDeliveryError::AuthenticationFailed)
    }
  }

  #[async_trait]
  impl CommandHandler<AcceptWebhookDeliveryCommand> for AcceptingHandler {
    type Error = WebhookDeliveryError;

    async fn handle_command(
      &self,
      _command: AcceptWebhookDeliveryCommand,
    ) -> Result<octacity_server_application::WebhookDeliveryAccepted, Self::Error> {
      Ok(octacity_server_application::WebhookDeliveryAccepted {
        delivery_id: "22222222-2222-4222-8222-222222222222".parse().unwrap(),
        disposition: octacity_server_application::MutationDisposition::Applied,
      })
    }
  }

  #[tokio::test]
  async fn returns_accepted_only_after_the_application_commits_a_receipt() {
    let response = webhook_router(WebhookApplication::new(Arc::new(AcceptingHandler)))
      .oneshot(
        Request::builder()
          .method("POST")
          .uri("/webhooks/v1/integrations/11111111-1111-4111-8111-111111111111")
          .body(Body::from("delivery"))
          .unwrap(),
      )
      .await
      .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
  }

  #[tokio::test]
  async fn preserves_exact_body_and_headers_until_typed_authentication_handler() {
    let handler = Arc::new(InspectingHandler(AtomicUsize::new(0)));
    let response = webhook_router(WebhookApplication::new(handler.clone()))
      .oneshot(
        Request::builder()
          .method("POST")
          .uri("/webhooks/v1/integrations/11111111-1111-4111-8111-111111111111")
          .header("x-signature", "proof")
          .body(Body::from(&b"exact\r\nbody\0bytes"[..]))
          .unwrap(),
      )
      .await
      .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(handler.0.load(Ordering::SeqCst), 1);
  }

  #[tokio::test]
  async fn rejects_oversized_bodies_before_the_application_handler() {
    let handler = Arc::new(InspectingHandler(AtomicUsize::new(0)));
    let response = webhook_router(WebhookApplication::new(handler.clone()))
      .oneshot(
        Request::builder()
          .method("POST")
          .uri("/webhooks/v1/integrations/11111111-1111-4111-8111-111111111111")
          .body(Body::from(vec![0; MAX_WEBHOOK_DELIVERY_BYTES + 1]))
          .unwrap(),
      )
      .await
      .unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(handler.0.load(Ordering::SeqCst), 0);
  }
}
