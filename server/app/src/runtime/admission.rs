//! Disposable, bounded ingress overload control.
//!
//! These counters are intentionally process-local. They protect one replica,
//! but never authorize a mutation or participate in coordination correctness.

use std::{
  collections::HashMap,
  net::SocketAddr,
  sync::{Arc, Mutex},
  time::{Duration, Instant},
};

use axum::{
  Json, Router,
  extract::{ConnectInfo, Request, State},
  http::{HeaderValue, StatusCode, header},
  middleware::{self, Next},
  response::{IntoResponse, Response},
};
use octacity_observability::{HttpMethod, HttpRoute, classify_http_response, record_http_request};
use octacity_protocol::{COORDINATOR_PROTOCOL_VERSION, CoordinatorErrorResponse};
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use crate::config::AdmissionConfig;

const AGENT_REQUEST_ID_HEADER: &str = "idempotency-key";
const HEARTBEAT_SUFFIX: &str = "/heartbeat";
const REQUEST_ID_HEADER: &str = "x-request-id";

#[derive(Clone)]
pub(super) struct IngressAdmission {
  management: RateLimiter,
  expensive_management: RateLimiter,
  agent: RateLimiter,
  agent_heartbeat: RateLimiter,
  webhook: RateLimiter,
}

impl IngressAdmission {
  pub(super) fn new(config: AdmissionConfig) -> Self {
    let policy = |capacity| RatePolicy {
      capacity,
      window: config.window(),
      tracked_identities: config.tracked_identities(),
    };
    Self {
      management: RateLimiter::new(policy(config.management_requests())),
      expensive_management: RateLimiter::new(policy(config.expensive_management_requests())),
      agent: RateLimiter::new(policy(config.agent_requests())),
      agent_heartbeat: RateLimiter::new(policy(config.agent_heartbeats())),
      webhook: RateLimiter::new(policy(config.webhook_requests())),
    }
  }

  pub(super) fn protect_management(&self, router: Router) -> Router {
    router.layer(middleware::from_fn_with_state(self.clone(), management_admission))
  }

  pub(super) fn protect_agent(&self, router: Router) -> Router {
    router.layer(middleware::from_fn_with_state(self.clone(), agent_admission))
  }

  pub(super) fn protect_webhook(&self, router: Router) -> Router {
    router.layer(middleware::from_fn_with_state(self.clone(), webhook_admission))
  }
}

#[derive(Clone, Copy)]
struct RatePolicy {
  capacity: u32,
  window: Duration,
  tracked_identities: usize,
}

#[derive(Clone)]
struct RateLimiter {
  policy: RatePolicy,
  windows: Arc<Mutex<HashMap<String, RequestWindow>>>,
}

#[derive(Clone, Copy)]
struct RequestWindow {
  started: Instant,
  requests: u32,
}

enum Admission {
  Allowed,
  Limited { retry_after: Duration },
}

impl RateLimiter {
  fn new(policy: RatePolicy) -> Self {
    debug_assert!(policy.capacity > 0);
    debug_assert!(!policy.window.is_zero());
    debug_assert!(policy.tracked_identities > 0);
    Self {
      policy,
      windows: Arc::new(Mutex::new(HashMap::new())),
    }
  }

  fn admit(&self, key: String) -> Admission {
    self.admit_at(key, Instant::now())
  }

  fn admit_at(&self, key: String, now: Instant) -> Admission {
    let mut windows = self.windows.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(window) = windows.get_mut(&key) {
      return admit_existing(window, self.policy, now);
    }
    if windows.len() == self.policy.tracked_identities {
      windows.retain(|_, window| now.saturating_duration_since(window.started) < self.policy.window);
      if windows.len() == self.policy.tracked_identities
        && let Some(oldest) = windows
          .iter()
          .min_by_key(|(_, window)| window.started)
          .map(|(key, _)| key.clone())
      {
        windows.remove(&oldest);
      }
    }
    windows.insert(
      key,
      RequestWindow {
        started: now,
        requests: 1,
      },
    );
    Admission::Allowed
  }
}

fn admit_existing(window: &mut RequestWindow, policy: RatePolicy, now: Instant) -> Admission {
  let elapsed = now.saturating_duration_since(window.started);
  if elapsed >= policy.window {
    *window = RequestWindow {
      started: now,
      requests: 1,
    };
    return Admission::Allowed;
  }
  if window.requests < policy.capacity {
    window.requests += 1;
    Admission::Allowed
  } else {
    Admission::Limited {
      retry_after: policy.window.saturating_sub(elapsed),
    }
  }
}

async fn management_admission(State(admission): State<IngressAdmission>, request: Request, next: Next) -> Response {
  let started = Instant::now();
  let path = request.uri().path();
  if management_probe(path) {
    return next.run(request).await;
  }
  let key = peer_key(&request);
  if let Admission::Limited { retry_after } = admission.management.admit(key.clone()) {
    record_rejection(&request, HttpRoute::Management, started.elapsed());
    return management_limited(retry_after);
  }
  if expensive_management_route(path)
    && let Admission::Limited { retry_after } = admission.expensive_management.admit(key)
  {
    record_rejection(&request, HttpRoute::Management, started.elapsed());
    return management_limited(retry_after);
  }
  next.run(request).await
}

async fn agent_admission(State(admission): State<IngressAdmission>, request: Request, next: Next) -> Response {
  let started = Instant::now();
  let limiter = if agent_heartbeat_route(request.uri().path()) {
    &admission.agent_heartbeat
  } else {
    &admission.agent
  };
  let key = credential_key(&request).unwrap_or_else(|| peer_key(&request));
  match limiter.admit(key) {
    Admission::Allowed => next.run(request).await,
    Admission::Limited { retry_after } => {
      record_rejection(&request, HttpRoute::Agent, started.elapsed());
      agent_limited(&request, retry_after)
    }
  }
}

async fn webhook_admission(State(admission): State<IngressAdmission>, request: Request, next: Next) -> Response {
  let started = Instant::now();
  let key = peer_key(&request);
  match admission.webhook.admit(key) {
    Admission::Allowed => next.run(request).await,
    Admission::Limited { retry_after } => {
      record_rejection(&request, HttpRoute::Webhook, started.elapsed());
      webhook_limited(retry_after)
    }
  }
}

fn record_rejection(request: &Request, route: HttpRoute, elapsed: Duration) {
  let status = StatusCode::TOO_MANY_REQUESTS.as_u16();
  record_http_request(
    HttpMethod::from_token(request.method().as_str()),
    route,
    classify_http_response(status),
    status,
    elapsed,
  );
}

fn management_probe(path: &str) -> bool {
  matches!(path, "/health/live" | "/health/ready" | "/metrics")
}

fn expensive_management_route(path: &str) -> bool {
  path == "/api/v1/audit-facts"
    || path.ends_with("/build-logs/search")
    || path.starts_with("/api/v1/jobs/") && path.ends_with("/events")
}

fn agent_heartbeat_route(path: &str) -> bool {
  path.starts_with("/api/v1/leases/") && path.ends_with(HEARTBEAT_SUFFIX)
}

fn peer_key(request: &Request) -> String {
  request
    .extensions()
    .get::<ConnectInfo<SocketAddr>>()
    .map_or_else(|| "peer:unknown".to_owned(), |peer| format!("peer:{}", peer.0.ip()))
}

fn credential_key(request: &Request) -> Option<String> {
  let credential = request.headers().get(header::AUTHORIZATION)?.to_str().ok()?;
  Some(digest_key(&[b"agent", credential.as_bytes()]))
}

fn digest_key(parts: &[&[u8]]) -> String {
  let mut digest = Sha256::new();
  for part in parts {
    digest.update((part.len() as u64).to_be_bytes());
    digest.update(part);
  }
  hex::encode(digest.finalize())
}

#[derive(Serialize)]
struct ManagementErrorBody {
  code: &'static str,
  message: &'static str,
  request_id: String,
}

#[derive(Serialize)]
struct WebhookErrorBody {
  code: &'static str,
  message: &'static str,
}

fn management_limited(retry_after: Duration) -> Response {
  let request_id = Uuid::new_v4().to_string();
  let mut response = (
    StatusCode::TOO_MANY_REQUESTS,
    Json(ManagementErrorBody {
      code: "rate_limited",
      message: "request rate limit exceeded",
      request_id: request_id.clone(),
    }),
  )
    .into_response();
  response.headers_mut().insert(
    REQUEST_ID_HEADER,
    HeaderValue::from_str(&request_id).expect("UUID is always a valid header value"),
  );
  insert_retry_after(&mut response, retry_after);
  response
}

fn agent_limited(request: &Request, retry_after: Duration) -> Response {
  let request_id = request
    .headers()
    .get(AGENT_REQUEST_ID_HEADER)
    .and_then(|value| value.to_str().ok())
    .unwrap_or("unknown-request");
  let mut response = (
    StatusCode::TOO_MANY_REQUESTS,
    Json(CoordinatorErrorResponse {
      protocol_version: COORDINATOR_PROTOCOL_VERSION,
      request_id: request_id.to_owned(),
      code: "rate_limited".to_owned(),
      message: "agent request rate limit exceeded".to_owned(),
      retryable: true,
      retry_after_ms: Some(duration_milliseconds_ceil(retry_after)),
    }),
  )
    .into_response();
  insert_retry_after(&mut response, retry_after);
  response
}

fn webhook_limited(retry_after: Duration) -> Response {
  let request_id = Uuid::new_v4().to_string();
  let mut response = (
    StatusCode::TOO_MANY_REQUESTS,
    Json(WebhookErrorBody {
      code: "rate_limited",
      message: "webhook delivery rate limit exceeded",
    }),
  )
    .into_response();
  response.headers_mut().insert(
    REQUEST_ID_HEADER,
    HeaderValue::from_str(&request_id).expect("UUID is always a valid header value"),
  );
  insert_retry_after(&mut response, retry_after);
  response
}

fn insert_retry_after(response: &mut Response, retry_after: Duration) {
  let seconds = retry_after
    .as_secs()
    .saturating_add(u64::from(retry_after.subsec_nanos() > 0))
    .max(1);
  let value = HeaderValue::from_str(&seconds.to_string()).expect("duration seconds are a valid header value");
  response.headers_mut().insert(header::RETRY_AFTER, value);
}

fn duration_milliseconds_ceil(duration: Duration) -> u64 {
  let milliseconds = duration
    .as_millis()
    .saturating_add(u128::from(!duration.subsec_nanos().is_multiple_of(1_000_000)))
    .max(1)
    .min(u128::from(u64::MAX));
  milliseconds as u64
}

#[cfg(test)]
mod tests {
  use axum::{body::Body, http::Request, routing::post};
  use tower::ServiceExt as _;

  use super::*;

  fn admission(general: u32, expensive: u32, heartbeat: u32) -> IngressAdmission {
    let policy = |capacity| RatePolicy {
      capacity,
      window: Duration::from_secs(1),
      tracked_identities: 2,
    };
    IngressAdmission {
      management: RateLimiter::new(policy(general)),
      expensive_management: RateLimiter::new(policy(expensive)),
      agent: RateLimiter::new(policy(general)),
      agent_heartbeat: RateLimiter::new(policy(heartbeat)),
      webhook: RateLimiter::new(policy(general)),
    }
  }

  fn request(path: &str, peer: [u8; 4]) -> Request<Body> {
    let mut request = Request::post(path).body(Body::empty()).unwrap();
    request
      .extensions_mut()
      .insert(ConnectInfo(SocketAddr::from((peer, 9_999))));
    request
  }

  fn authorized_request(path: &str) -> Request<Body> {
    let mut request = request(path, [127, 0, 0, 1]);
    request.headers_mut().insert(
      header::AUTHORIZATION,
      HeaderValue::from_static("Bearer registration.agent.secret"),
    );
    request
      .headers_mut()
      .insert(AGENT_REQUEST_ID_HEADER, HeaderValue::from_static("request-1"));
    request
  }

  #[test]
  fn limiter_resets_and_bounds_tracked_identity_state() {
    let policy = RatePolicy {
      capacity: 1,
      window: Duration::from_secs(1),
      tracked_identities: 2,
    };
    let limiter = RateLimiter::new(policy);
    let now = Instant::now();
    assert!(matches!(limiter.admit_at("a".to_owned(), now), Admission::Allowed));
    assert!(matches!(
      limiter.admit_at("a".to_owned(), now),
      Admission::Limited { .. }
    ));
    assert!(matches!(limiter.admit_at("b".to_owned(), now), Admission::Allowed));
    assert!(matches!(limiter.admit_at("c".to_owned(), now), Admission::Allowed));
    assert_eq!(limiter.windows.lock().unwrap().len(), 2);
    assert!(matches!(
      limiter.admit_at("c".to_owned(), now + policy.window),
      Admission::Allowed
    ));
  }

  #[tokio::test]
  async fn management_limit_is_per_client_and_returns_stable_retry_guidance() {
    let router = admission(1, 1, 1).protect_management(Router::new().route("/api/v1/projects", post(|| async {})));
    assert_eq!(
      router
        .clone()
        .oneshot(request("/api/v1/projects", [127, 0, 0, 1]))
        .await
        .unwrap()
        .status(),
      StatusCode::OK
    );
    let rejected = router
      .clone()
      .oneshot(request("/api/v1/projects", [127, 0, 0, 1]))
      .await
      .unwrap();
    assert_eq!(rejected.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(rejected.headers()[header::RETRY_AFTER], "1");
    assert!(rejected.headers().contains_key(REQUEST_ID_HEADER));
    let request_id = rejected.headers()[REQUEST_ID_HEADER].to_str().unwrap().to_owned();
    let body = axum::body::to_bytes(rejected.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["code"], "rate_limited");
    assert_eq!(body["request_id"], request_id);
    assert_eq!(
      router
        .oneshot(request("/api/v1/projects", [127, 0, 0, 2]))
        .await
        .unwrap()
        .status(),
      StatusCode::OK
    );
  }

  #[tokio::test]
  async fn expensive_management_routes_have_a_stricter_independent_limit() {
    let router = admission(3, 1, 1)
      .protect_management(Router::new().route("/api/v1/projects/{project_id}/build-logs/search", post(|| async {})));
    let path = "/api/v1/projects/project-1/build-logs/search";
    assert_eq!(
      router
        .clone()
        .oneshot(request(path, [127, 0, 0, 1]))
        .await
        .unwrap()
        .status(),
      StatusCode::OK
    );
    assert_eq!(
      router.oneshot(request(path, [127, 0, 0, 1])).await.unwrap().status(),
      StatusCode::TOO_MANY_REQUESTS
    );
  }

  #[tokio::test]
  async fn agent_traffic_cannot_consume_the_heartbeat_budget() {
    let limits = admission(1, 1, 1);
    let router = limits.protect_agent(
      Router::new()
        .route("/api/v1/leases/{lease_id}/events:append", post(|| async {}))
        .route("/api/v1/leases/{lease_id}/heartbeat", post(|| async {})),
    );
    let events = "/api/v1/leases/lease-1/events:append";
    assert_eq!(
      router
        .clone()
        .oneshot(authorized_request(events))
        .await
        .unwrap()
        .status(),
      StatusCode::OK
    );
    let rejected = router.clone().oneshot(authorized_request(events)).await.unwrap();
    assert_eq!(rejected.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(rejected.headers()[header::RETRY_AFTER], "1");
    let body = axum::body::to_bytes(rejected.into_body(), usize::MAX).await.unwrap();
    let error: CoordinatorErrorResponse = serde_json::from_slice(&body).unwrap();
    error.validate("request-1").unwrap();
    assert_eq!(error.request_id, "request-1");
    assert_eq!(error.code, "rate_limited");
    assert!(error.retryable);
    assert!(error.retry_after_ms.is_some_and(|delay| delay > 0));
    assert_eq!(
      router
        .oneshot(authorized_request("/api/v1/leases/lease-1/heartbeat"))
        .await
        .unwrap()
        .status(),
      StatusCode::OK
    );
  }

  #[tokio::test]
  async fn webhook_limit_is_scoped_by_peer() {
    let router = admission(1, 1, 1)
      .protect_webhook(Router::new().route("/webhooks/v1/integrations/{integration_id}", post(|| async {})));
    let first = "/webhooks/v1/integrations/first";
    assert_eq!(
      router
        .clone()
        .oneshot(request(first, [127, 0, 0, 1]))
        .await
        .unwrap()
        .status(),
      StatusCode::OK
    );
    let rejected = router.clone().oneshot(request(first, [127, 0, 0, 1])).await.unwrap();
    assert_eq!(rejected.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(rejected.headers()[header::RETRY_AFTER], "1");
    assert!(rejected.headers().contains_key(REQUEST_ID_HEADER));
    assert_eq!(
      router
        .clone()
        .oneshot(request("/webhooks/v1/integrations/second", [127, 0, 0, 1],))
        .await
        .unwrap()
        .status(),
      StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
      router
        .oneshot(request("/webhooks/v1/integrations/second", [127, 0, 0, 2],))
        .await
        .unwrap()
        .status(),
      StatusCode::OK
    );
  }

  #[test]
  fn credential_keys_do_not_retain_bearer_material() {
    let mut request = request("/api/v1/agents/agent-1/leases:acquire", [127, 0, 0, 1]);
    request.headers_mut().insert(
      header::AUTHORIZATION,
      HeaderValue::from_static("Bearer registration.agent.highly-secret-material"),
    );
    let key = credential_key(&request).unwrap();
    assert_eq!(key.len(), 64);
    assert!(!key.contains("agent"));
    assert!(!key.contains("secret"));
  }
}
