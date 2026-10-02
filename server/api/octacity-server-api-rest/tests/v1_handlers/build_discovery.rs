use super::*;

const PROJECT_ID: &str = "11111111-1111-4111-8111-111111111111";
const CONFIGURATION_ID: &str = "44444444-4444-4444-8444-444444444444";
const NEXT_CURSOR: &str = "MToxNzAwMDAwMDAwNTAwOjk5OTk5OTk5LTk5OTktNDk5OS04OTk5LTk5OTk5OTk5OTk5OQ";

#[tokio::test]
async fn maps_filtered_build_pages_and_exclusive_cursors() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );

  let first = routes
    .clone()
    .oneshot(empty_request(
      "GET",
      &format!("/api/v1/projects/{PROJECT_ID}/builds?configuration_id={CONFIGURATION_ID}&state=succeeded&limit=1"),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(first.status(), StatusCode::OK);
  let first: serde_json::Value =
    serde_json::from_slice(&to_bytes(first.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(
    first,
    serde_json::from_str::<serde_json::Value>(include_str!("../../fixtures/v1/build-summary-page.json")).unwrap()
  );

  let following = routes
    .oneshot(empty_request(
      "GET",
      &format!(
        "/api/v1/projects/{PROJECT_ID}/builds?configuration_id={CONFIGURATION_ID}&state=succeeded&after={NEXT_CURSOR}&limit=1"
      ),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(following.status(), StatusCode::OK);
  let following: serde_json::Value =
    serde_json::from_slice(&to_bytes(following.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(following, serde_json::json!({"items": [], "next_cursor": null}));

  let queries = application.build_list_queries.lock().unwrap();
  assert_eq!(queries.len(), 2);
  assert_eq!(queries[0].project_id().to_string(), PROJECT_ID);
  assert_eq!(
    queries[0].filter().configuration_id.unwrap().to_string(),
    CONFIGURATION_ID
  );
  assert_eq!(serde_json::to_value(queries[0].filter().state).unwrap(), "succeeded");
  assert_eq!(queries[0].limit(), 1);
  assert!(queries[0].after().is_none());
  assert_eq!(queries[1].after().unwrap().encode(), NEXT_CURSOR);
}

#[tokio::test]
async fn maps_queued_builds_with_created_attempts() {
  let application = Arc::new(RecordingApplication::queued_build_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );

  let response = routes
    .oneshot(empty_request(
      "GET",
      &format!("/api/v1/projects/{PROJECT_ID}/builds?state=queued"),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(body["items"][0]["state"], "queued");
  assert_eq!(body["items"][0]["current_attempt_state"], "created");
}

#[tokio::test]
async fn rejects_invalid_build_filters_and_pagination_before_dispatch() {
  let application = Arc::new(RecordingApplication::default());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );

  for query in [
    "configuration_id=not-an-id",
    "state=unknown",
    "after=not-a-build-cursor",
    "limit=0",
    "limit=201",
    "limit=1&offset=1",
  ] {
    let response = routes
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("/api/v1/projects/{PROJECT_ID}/builds?{query}"),
        None,
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{query}");
  }
  assert!(application.calls.lock().unwrap().is_empty());
}
