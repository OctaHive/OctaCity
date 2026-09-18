//! Versioned management REST adapter.
//!
//! REST DTOs and transport concerns stay in this crate and map onto typed
//! application commands and queries.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::{sync::Arc, time::Instant};

use axum::{
  Json, Router,
  extract::{MatchedPath, Request, State},
  http::{HeaderName, HeaderValue, StatusCode},
  middleware::{self, Next},
  response::Response,
  routing::get,
};
use serde::Serialize;
use tracing::Instrument as _;
use uuid::Uuid;

mod long_poll;
mod telemetry;
pub mod v1;

pub use long_poll::JobEventNotificationHub;

static REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

type ReadinessProbe = Arc<dyn Fn() -> bool + Send + Sync>;

#[derive(Clone)]
struct RequestId(String);

/// Builds the management HTTP router owned by the REST adapter.
///
/// The composition root supplies process readiness as a small callback; HTTP
/// paths, response DTOs, status codes, headers, and tracing remain here.
pub fn management_router(readiness: impl Fn() -> bool + Send + Sync + 'static) -> Router {
  management_router_with_metadata(
    readiness,
    v1::OperationalMetadata::trusted_network(false, false, false, false),
  )
}

/// Builds management health and operational routes with deployment metadata.
pub fn management_router_with_metadata(
  readiness: impl Fn() -> bool + Send + Sync + 'static,
  metadata: v1::OperationalMetadata,
) -> Router {
  health_routes(readiness, metadata).layer(middleware::from_fn(request_context))
}

/// Builds the management HTTP router with all section-4 application routes.
pub fn management_router_with_application(
  readiness: impl Fn() -> bool + Send + Sync + 'static,
  application: v1::ManagementApplication,
) -> Router {
  management_router_with_application_and_metadata(
    readiness,
    application,
    v1::OperationalMetadata::trusted_network(false, false, false, false),
  )
}

/// Builds the complete management router with deployment operational metadata.
pub fn management_router_with_application_and_metadata(
  readiness: impl Fn() -> bool + Send + Sync + 'static,
  application: v1::ManagementApplication,
  metadata: v1::OperationalMetadata,
) -> Router {
  health_routes(readiness, metadata)
    .merge(v1::management_routes(application))
    .layer(middleware::from_fn(request_context))
}

#[derive(Clone)]
struct ManagementState {
  readiness: ReadinessProbe,
  metadata: Arc<v1::OperationalMetadata>,
}

fn health_routes(readiness: impl Fn() -> bool + Send + Sync + 'static, metadata: v1::OperationalMetadata) -> Router {
  Router::new()
    .route("/health/live", get(liveness))
    .route("/health/ready", get(readiness_handler))
    .route("/api/v1/operations/metadata", get(operational_metadata))
    .with_state(ManagementState {
      readiness: Arc::new(readiness) as ReadinessProbe,
      metadata: Arc::new(metadata),
    })
}

#[derive(Serialize)]
struct HealthResponse {
  status: &'static str,
}

async fn liveness() -> Json<HealthResponse> {
  Json(HealthResponse { status: "live" })
}

async fn readiness_handler(State(state): State<ManagementState>) -> (StatusCode, Json<HealthResponse>) {
  if (state.readiness)() {
    (StatusCode::OK, Json(HealthResponse { status: "ready" }))
  } else {
    (
      StatusCode::SERVICE_UNAVAILABLE,
      Json(HealthResponse { status: "unavailable" }),
    )
  }
}

async fn operational_metadata(State(state): State<ManagementState>) -> Json<v1::OperationalMetadata> {
  Json((*state.metadata).clone())
}

async fn request_context(mut request: Request, next: Next) -> Response {
  let request_id = Uuid::new_v4().to_string();
  let request_id_header = HeaderValue::from_str(&request_id).expect("UUID is always a valid header value");
  let method = request.method().as_str().to_owned();
  let matched_route = request.extensions().get::<MatchedPath>().map(MatchedPath::as_str);
  let span = telemetry::request_span(&request_id, &method, matched_route);
  request.extensions_mut().insert(RequestId(request_id.clone()));
  async move {
    let started = Instant::now();
    let mut response = next.run(request).await;
    response
      .headers_mut()
      .insert(REQUEST_ID_HEADER.clone(), request_id_header);
    telemetry::record_response(response.status().as_u16(), started.elapsed());
    response
  }
  .instrument(span)
  .await
}

#[cfg(test)]
mod tests {
  use axum::{body::Body, http::Request};
  use tower::ServiceExt as _;

  use super::*;

  #[tokio::test]
  async fn readiness_reports_unavailable_without_affecting_liveness() {
    let router = management_router(|| false);

    let readiness = router
      .clone()
      .oneshot(Request::builder().uri("/health/ready").body(Body::empty()).unwrap())
      .await
      .unwrap();
    assert_eq!(readiness.status(), StatusCode::SERVICE_UNAVAILABLE);
    let request_id = readiness.headers().get(&REQUEST_ID_HEADER).unwrap().to_str().unwrap();
    assert!(Uuid::parse_str(request_id).is_ok());
    let body = axum::body::to_bytes(readiness.into_body(), 128).await.unwrap();
    assert_eq!(body.as_ref(), br#"{"status":"unavailable"}"#);

    let liveness = router
      .oneshot(Request::builder().uri("/health/live").body(Body::empty()).unwrap())
      .await
      .unwrap();
    assert_eq!(liveness.status(), StatusCode::OK);
  }
}
