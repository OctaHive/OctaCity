//! REST request tracing using the shared cross-product vocabulary.

use std::time::Duration;

use octacity_observability::{HttpMethod, HttpRoute, TraceSpan, classify_http_response, record_http_request};

pub(super) fn request_span(request_id: &str, method: &str, matched_route: Option<&str>) -> tracing::Span {
  let route = matched_route.unwrap_or("<unmatched>");
  tracing::info_span!(
    TraceSpan::ServerHttpRequest.as_str(),
    request.id = request_id,
    http.request.method = method,
    http.route = route,
  )
}

pub(super) fn record_response(method: &str, route: HttpRoute, status_code: u16, elapsed: Duration) {
  record_http_request(
    HttpMethod::from_token(method),
    route,
    classify_http_response(status_code),
    status_code,
    elapsed,
  );
}

pub(super) fn route_group(matched_route: Option<&str>) -> HttpRoute {
  if matched_route.is_some_and(|route| route.starts_with("/health/")) {
    HttpRoute::Health
  } else {
    HttpRoute::Management
  }
}
