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
  http::{HeaderName, HeaderValue, StatusCode, header},
  middleware::{self, Next},
  response::Response,
  routing::get,
};
use serde::Serialize;
use tracing::Instrument as _;
use uuid::Uuid;

use octacity_server_application::{ManagementRequestContext, ManagementRequestId};

mod long_poll;
mod telemetry;
pub mod v1;

pub use long_poll::JobEventNotificationHub;

static REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

type ReadinessProbe = Arc<dyn Fn() -> bool + Send + Sync>;

#[derive(Clone)]
struct RequestId(String);

#[derive(Clone)]
struct ManagementRequest(RequestId, ManagementRequestContext);

/// Builds the management HTTP router owned by the REST adapter.
///
/// The composition root supplies process readiness as a small callback; HTTP
/// paths, response DTOs, status codes, headers, and tracing remain here.
pub fn management_router(readiness: impl Fn() -> bool + Send + Sync + 'static) -> Router {
  health_routes(readiness).layer(middleware::from_fn(request_context))
}

/// Builds the management HTTP router with all section-4 application routes.
pub fn management_router_with_application(
  readiness: impl Fn() -> bool + Send + Sync + 'static,
  application: v1::ManagementApplication,
) -> Router {
  application_routes(readiness, application).layer(middleware::from_fn(request_context))
}

/// Builds the complete management router with a Prometheus operational endpoint.
pub fn management_router_with_application_and_metrics(
  readiness: impl Fn() -> bool + Send + Sync + 'static,
  application: v1::ManagementApplication,
  metrics: impl Fn() -> String + Clone + Send + Sync + 'static,
) -> Router {
  application_routes(readiness, application)
    .merge(metrics_route(metrics))
    .layer(middleware::from_fn(request_context))
}

fn application_routes(
  readiness: impl Fn() -> bool + Send + Sync + 'static,
  application: v1::ManagementApplication,
) -> Router {
  health_routes(readiness).merge(v1::management_routes(application))
}

fn metrics_route(metrics: impl Fn() -> String + Clone + Send + Sync + 'static) -> Router {
  Router::new().route(
    "/metrics",
    get(move || {
      let metrics = metrics.clone();
      async move {
        (
          [(header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
          metrics(),
        )
      }
    }),
  )
}

#[derive(Clone)]
struct ManagementState {
  readiness: ReadinessProbe,
}

fn health_routes(readiness: impl Fn() -> bool + Send + Sync + 'static) -> Router {
  Router::new()
    .route("/health/live", get(liveness))
    .route("/health/ready", get(readiness_handler))
    .with_state(ManagementState {
      readiness: Arc::new(readiness) as ReadinessProbe,
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

async fn request_context(mut request: Request, next: Next) -> Response {
  let request_uuid = Uuid::new_v4();
  let management_request_id =
    ManagementRequestId::new(request_uuid).expect("a generated UUID is a valid management request ID");
  let request_id = request_uuid.to_string();
  let request_id_header = HeaderValue::from_str(&request_id).expect("UUID is always a valid header value");
  let method = request.method().clone();
  let matched_route = request
    .extensions()
    .get::<MatchedPath>()
    .map(|matched_path| matched_path.as_str().to_owned());
  let route_group = telemetry::route_group(matched_route.as_deref());
  let span = telemetry::request_span(&request_id, method.as_str(), matched_route.as_deref());
  request.extensions_mut().insert(RequestId(request_id.clone()));
  if is_management_operation(&method, matched_route.as_deref()) {
    request.extensions_mut().insert(ManagementRequest(
      RequestId(request_id.clone()),
      ManagementRequestContext::trusted_network(management_request_id),
    ));
  }
  async move {
    let started = Instant::now();
    let mut response = next.run(request).await;
    response
      .headers_mut()
      .insert(REQUEST_ID_HEADER.clone(), request_id_header);
    telemetry::record_response(
      method.as_str(),
      route_group,
      response.status().as_u16(),
      started.elapsed(),
    );
    response
  }
  .instrument(span)
  .await
}

fn is_management_operation(method: &axum::http::Method, matched_route: Option<&str>) -> bool {
  let Some(path) = matched_route else {
    return false;
  };
  let method = if method == axum::http::Method::HEAD {
    "GET"
  } else {
    method.as_str()
  };
  v1::MANAGEMENT_OPERATIONS
    .iter()
    .any(|operation| operation.method == method && operation.path == path)
}

#[cfg(test)]
mod tests {
  use std::str::FromStr as _;

  use axum::{
    Router,
    body::Body,
    extract::{Extension, Request as AxumRequest},
    http::{Method, Request as HttpRequest},
    middleware,
    routing::{delete, get, post},
  };
  use octacity_server_application::{ManagementActorKind, ManagementIngress};
  use tower::ServiceExt as _;

  use super::*;

  #[tokio::test]
  async fn readiness_reports_unavailable_without_affecting_liveness() {
    let router = management_router(|| false);

    let readiness = router
      .clone()
      .oneshot(HttpRequest::builder().uri("/health/ready").body(Body::empty()).unwrap())
      .await
      .unwrap();
    assert_eq!(readiness.status(), StatusCode::SERVICE_UNAVAILABLE);
    let request_id = readiness.headers().get(&REQUEST_ID_HEADER).unwrap().to_str().unwrap();
    assert!(Uuid::parse_str(request_id).is_ok());
    let body = axum::body::to_bytes(readiness.into_body(), 128).await.unwrap();
    assert_eq!(body.as_ref(), br#"{"status":"unavailable"}"#);

    let liveness = router
      .oneshot(HttpRequest::builder().uri("/health/live").body(Body::empty()).unwrap())
      .await
      .unwrap();
    assert_eq!(liveness.status(), StatusCode::OK);
  }

  #[tokio::test]
  async fn metrics_route_returns_prometheus_text_without_application_state() {
    let response = metrics_route(|| "octacity_fixture_total 1\n".to_owned())
      .oneshot(HttpRequest::builder().uri("/metrics").body(Body::empty()).unwrap())
      .await
      .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
      response.headers()[header::CONTENT_TYPE],
      "text/plain; version=0.0.4; charset=utf-8"
    );
    let body = axum::body::to_bytes(response.into_body(), 1_024).await.unwrap();
    assert_eq!(body.as_ref(), b"octacity_fixture_total 1\n");
  }

  #[tokio::test]
  async fn every_management_operation_receives_the_canonical_trusted_network_context() {
    for operation in v1::MANAGEMENT_OPERATIONS {
      let method_router = match operation.method {
        "GET" => get(management_context_probe),
        "POST" => post(management_context_probe),
        "DELETE" => delete(management_context_probe),
        other => panic!("unsupported management fixture method {other}"),
      };
      let router = Router::new()
        .route(operation.path, method_router)
        .layer(middleware::from_fn(request_context));
      let response = router
        .clone()
        .oneshot(
          HttpRequest::builder()
            .method(Method::from_str(operation.method).unwrap())
            .uri(concrete_path(operation.path))
            .header(header::AUTHORIZATION, "Bearer untrusted")
            .header(header::COOKIE, "session=untrusted")
            .header("x-octacity-client-kind", "automation")
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();

      assert_eq!(response.status(), StatusCode::NO_CONTENT, "{}", operation.operation_id);
      assert!(Uuid::parse_str(response.headers()[&REQUEST_ID_HEADER].to_str().unwrap()).is_ok());
      if operation.method == "GET" {
        let response = router
          .oneshot(
            HttpRequest::builder()
              .method(Method::HEAD)
              .uri(concrete_path(operation.path))
              .body(Body::empty())
              .unwrap(),
          )
          .await
          .unwrap();
        assert_eq!(
          response.status(),
          StatusCode::NO_CONTENT,
          "HEAD {}",
          operation.operation_id
        );
      }
    }
  }

  #[tokio::test]
  async fn health_and_non_management_routes_do_not_receive_management_context() {
    for path in ["/health/live", "/metrics", "/api/v1/openapi.json", "/agent/v1/leases"] {
      let router = Router::new()
        .route(path, get(non_management_context_probe))
        .layer(middleware::from_fn(request_context));
      let response = router
        .oneshot(HttpRequest::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();

      assert_eq!(response.status(), StatusCode::NO_CONTENT, "{path}");
      assert!(response.headers().contains_key(&REQUEST_ID_HEADER));
    }
  }

  async fn management_context_probe(
    Extension(ManagementRequest(request_id, context)): Extension<ManagementRequest>,
  ) -> StatusCode {
    assert_eq!(context.actor().kind(), ManagementActorKind::UnauthenticatedManagement);
    assert_eq!(context.actor().identity(), None);
    assert_eq!(context.security_scope().as_str(), "trusted-network");
    assert_eq!(context.request_id().to_string(), request_id.0);
    assert_eq!(context.attributes().ingress(), ManagementIngress::TrustedNetwork);
    assert_eq!(context.attributes().client_kind(), None);
    StatusCode::NO_CONTENT
  }

  async fn non_management_context_probe(request: AxumRequest) -> StatusCode {
    assert!(request.extensions().get::<RequestId>().is_some());
    assert!(request.extensions().get::<ManagementRequest>().is_none());
    StatusCode::NO_CONTENT
  }

  fn concrete_path(template: &str) -> String {
    let mut path = template.to_owned();
    while let Some(start) = path.find('{') {
      let end = path[start..].find('}').expect("route parameter must close") + start;
      path.replace_range(start..=end, "fixture");
    }
    path
  }
}
