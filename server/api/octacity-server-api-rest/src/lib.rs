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

mod telemetry;

static REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

type ReadinessProbe = Arc<dyn Fn() -> bool + Send + Sync>;

/// Builds the management HTTP router owned by the REST adapter.
///
/// The composition root supplies process readiness as a small callback; HTTP
/// paths, response DTOs, status codes, headers, and tracing remain here.
pub fn management_router(readiness: impl Fn() -> bool + Send + Sync + 'static) -> Router {
  Router::new()
    .route("/health/live", get(liveness))
    .route("/health/ready", get(readiness_handler))
    .with_state(Arc::new(readiness) as ReadinessProbe)
    .layer(middleware::from_fn(request_context))
}

#[derive(Serialize)]
struct HealthResponse {
  status: &'static str,
}

async fn liveness() -> Json<HealthResponse> {
  Json(HealthResponse { status: "live" })
}

async fn readiness_handler(State(readiness): State<ReadinessProbe>) -> (StatusCode, Json<HealthResponse>) {
  if readiness() {
    (StatusCode::OK, Json(HealthResponse { status: "ready" }))
  } else {
    (
      StatusCode::SERVICE_UNAVAILABLE,
      Json(HealthResponse { status: "unavailable" }),
    )
  }
}

async fn request_context(request: Request, next: Next) -> Response {
  let request_id = Uuid::new_v4().to_string();
  let request_id_header = HeaderValue::from_str(&request_id).expect("UUID is always a valid header value");
  let method = request.method().as_str().to_owned();
  let matched_route = request.extensions().get::<MatchedPath>().map(MatchedPath::as_str);
  let span = telemetry::request_span(&request_id, &method, matched_route);
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
