//! REST request tracing using the shared cross-product vocabulary.

use std::time::Duration;

use octacity_observability::{TraceEvent, TraceSpan};

pub(super) fn request_span(request_id: &str, method: &str, matched_route: Option<&str>) -> tracing::Span {
  let route = matched_route.unwrap_or("<unmatched>");
  tracing::info_span!(
    TraceSpan::ServerHttpRequest.as_str(),
    request.id = request_id,
    http.request.method = method,
    http.route = route,
  )
}

pub(super) fn record_response(status_code: u16, elapsed: Duration) {
  let elapsed_milliseconds = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
  tracing::info!(
    event.name = TraceEvent::ServerRequestCompleted.as_str(),
    http.response.status_code = status_code,
    duration.milliseconds = elapsed_milliseconds,
    "HTTP request completed"
  );
}
