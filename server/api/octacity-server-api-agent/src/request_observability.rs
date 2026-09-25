//! Correlated diagnostic context for authenticated Agent HTTP requests.

use std::time::Instant;

use axum::{
  extract::{MatchedPath, Request},
  http::{HeaderName, HeaderValue},
  middleware::Next,
  response::Response,
};
use octacity_observability::{HttpMethod, HttpRoute, TraceSpan, classify_http_response, record_http_request};
use tracing::Instrument as _;
use uuid::Uuid;

static REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

pub(super) async fn request_context(request: Request, next: Next) -> Response {
  let request_id = Uuid::new_v4().to_string();
  let response_request_id = HeaderValue::from_str(&request_id).expect("UUID is always a valid header value");
  let method = request.method().as_str().to_owned();
  let route = request
    .extensions()
    .get::<MatchedPath>()
    .map_or("<unmatched>", MatchedPath::as_str)
    .to_owned();
  let span = tracing::info_span!(
    TraceSpan::ServerHttpRequest.as_str(),
    request.id = request_id.as_str(),
    http.request.method = method.as_str(),
    http.route = route.as_str(),
  );
  async move {
    let started = Instant::now();
    let mut response = next.run(request).await;
    response
      .headers_mut()
      .insert(REQUEST_ID_HEADER.clone(), response_request_id);
    let status = response.status().as_u16();
    record_http_request(
      HttpMethod::from_token(&method),
      HttpRoute::Agent,
      classify_http_response(status),
      status,
      started.elapsed(),
    );
    response
  }
  .instrument(span)
  .await
}
