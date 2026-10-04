use super::*;

const BUILD_ID: &str = "99999999-9999-4999-8999-999999999999";
const AGENT_ID: &str = "88888888-8888-4888-8888-888888888888";
const POOL_ID: &str = "77777777-7777-4777-8777-777777777777";

#[tokio::test]
async fn maps_the_explicit_bounded_attention_scope_and_safe_page() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );
  let response = routes
    .oneshot(empty_request(
      "GET",
      &format!(
        "/api/v1/operator-attention?build_ids={BUILD_ID}&agent_ids={AGENT_ID}&pool_ids={POOL_ID}&include_critical_conditions=true&occurred_from_unix_ms=1700000000000&occurred_through_unix_ms=1700000010000&limit=25"
      ),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(
    body,
    serde_json::from_str::<serde_json::Value>(include_str!("../../fixtures/v1/operator-attention-page.json")).unwrap()
  );
  let encoded = serde_json::to_string(&body).unwrap();
  for forbidden in ["recipient", "unread", "read_at", "request_identity", "raw_event"] {
    assert!(!encoded.contains(forbidden), "response exposed {forbidden}");
  }

  let queries = application.operator_attention_queries.lock().unwrap();
  assert_eq!(queries.len(), 1);
  let query = &queries[0];
  assert_eq!(query.limit(), 25);
  assert!(query.scope().includes_critical_conditions());
  assert_eq!(query.scope().occurred_from_unix_ms(), Some(1_700_000_000_000));
  assert_eq!(query.scope().occurred_through_unix_ms(), Some(1_700_000_010_000));
  assert_eq!(query.scope().targets().len(), 3);
}

#[tokio::test]
async fn accepts_an_explicit_critical_only_scope() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );
  let response = routes
    .oneshot(empty_request(
      "GET",
      "/api/v1/operator-attention?include_critical_conditions=true",
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let queries = application.operator_attention_queries.lock().unwrap();
  assert!(queries[0].scope().targets().is_empty());
  assert_eq!(queries[0].limit(), 50);
}

#[tokio::test]
async fn rejects_invalid_scope_time_cursor_and_page_bounds_before_dispatch() {
  let application = Arc::new(RecordingApplication::default());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );
  let oversized_cursor = "a".repeat(octacity_server_application::MAX_OPERATOR_ATTENTION_CURSOR_BYTES + 1);
  let oversized_scope = (0..=octacity_server_application::MAX_OPERATOR_ATTENTION_SCOPE_TARGETS)
    .map(|index| format!("build_ids={}", uuid::Uuid::from_u128(index as u128 + 1)))
    .collect::<Vec<_>>()
    .join("&");
  let cases = [
    "/api/v1/operator-attention".to_owned(),
    "/api/v1/operator-attention?build_ids=not-a-uuid".to_owned(),
    format!("/api/v1/operator-attention?build_ids={BUILD_ID}&build_ids={BUILD_ID}"),
    format!("/api/v1/operator-attention?{oversized_scope}"),
    format!("/api/v1/operator-attention?build_ids={BUILD_ID}&occurred_from_unix_ms=2&occurred_through_unix_ms=1"),
    format!("/api/v1/operator-attention?build_ids={BUILD_ID}&after=not-a-cursor"),
    format!("/api/v1/operator-attention?build_ids={BUILD_ID}&after={oversized_cursor}"),
    format!("/api/v1/operator-attention?build_ids={BUILD_ID}&limit=0"),
    format!(
      "/api/v1/operator-attention?build_ids={BUILD_ID}&limit={}",
      octacity_server_application::MAX_OPERATOR_ATTENTION_RESULT_PAGE_SIZE + 1
    ),
    format!("/api/v1/operator-attention?build_ids={BUILD_ID}&limit=1&limit=2"),
    format!("/api/v1/operator-attention?build_ids={BUILD_ID}&offset=1"),
  ];

  for target in cases {
    let response = routes
      .clone()
      .oneshot(empty_request("GET", &target, None))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{target}");
  }
  assert!(application.calls.lock().unwrap().is_empty());
}
