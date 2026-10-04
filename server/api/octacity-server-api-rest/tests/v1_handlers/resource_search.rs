use super::*;

const SEARCH_CURSOR: &str = "eyJ2ZXJzaW9uIjoxLCJxdWVyeSI6ImFscGhhIiwia2luZHMiOlsicHJvamVjdCIsImJ1aWxkIiwiYWdlbnQiLCJhZ2VudF9wb29sIl0sInBvc2l0aW9uIjp7InJhbmsiOiJuYW1lX3ByZWZpeCIsImtpbmQiOiJidWlsZCIsIm5vcm1hbGl6ZWRfbGFiZWwiOiJhbHBoYSByZWxlYXNlIiwicmVzb3VyY2UiOnsia2luZCI6ImJ1aWxkIiwiaWQiOiI5OTk5OTk5OS05OTk5LTQ5OTktODk5OS05OTk5OTk5OTk5OTkifX19";

#[tokio::test]
async fn maps_typed_results_and_exclusive_search_cursors() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );

  let first = routes
    .clone()
    .oneshot(empty_request("GET", "/api/v1/search?query=%20ALPHA%20&limit=2", None))
    .await
    .unwrap();
  assert_eq!(first.status(), StatusCode::OK);
  let first: serde_json::Value =
    serde_json::from_slice(&to_bytes(first.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(
    first,
    serde_json::from_str::<serde_json::Value>(include_str!("../../fixtures/v1/resource-search-page.json")).unwrap()
  );

  let following = routes
    .oneshot(empty_request(
      "GET",
      &format!("/api/v1/search?query=alpha&after={SEARCH_CURSOR}&limit=2"),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(following.status(), StatusCode::OK);
  let following: serde_json::Value =
    serde_json::from_slice(&to_bytes(following.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(following, serde_json::json!({"items": [], "next_cursor": null}));

  let queries = application.resource_search_queries.lock().unwrap();
  assert_eq!(queries.len(), 2);
  assert_eq!(queries[0].normalized_query(), "alpha");
  assert_eq!(
    queries[0].kinds(),
    &octacity_server_application::ResourceSearchKinds::all()
  );
  assert_eq!(queries[0].limit(), 2);
  assert!(queries[0].after().is_none());
  assert_eq!(queries[1].after().unwrap().encode(), SEARCH_CURSOR);
}

#[tokio::test]
async fn maps_the_optional_resource_kind_allowlist() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );

  let response = routes
    .oneshot(empty_request(
      "GET",
      "/api/v1/search?query=alpha&kinds=build&kinds=agent_pool&limit=1",
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(body["items"].as_array().unwrap().len(), 1);
  assert_eq!(body["items"][0]["kind"], "build");
  assert_eq!(body["next_cursor"], serde_json::Value::Null);

  let queries = application.resource_search_queries.lock().unwrap();
  assert_eq!(
    queries[0].kinds(),
    &octacity_server_application::ResourceSearchKinds::try_only([
      octacity_server_application::ResourceSearchKind::Build,
      octacity_server_application::ResourceSearchKind::AgentPool,
    ])
    .unwrap()
  );
}

#[tokio::test]
async fn rejects_invalid_search_query_kinds_cursor_and_page_bounds_before_dispatch() {
  let application = Arc::new(RecordingApplication::default());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );
  let oversized_query = "a".repeat(octacity_server_application::MAX_RESOURCE_SEARCH_QUERY_BYTES + 1);
  let oversized_cursor = "a".repeat(octacity_server_application::MAX_RESOURCE_SEARCH_CURSOR_BYTES + 1);
  let cases = [
    "/api/v1/search?limit=1".to_owned(),
    "/api/v1/search?query=%20%20".to_owned(),
    format!("/api/v1/search?query={oversized_query}"),
    "/api/v1/search?query=alpha&kinds=".to_owned(),
    "/api/v1/search?query=alpha&kinds=unknown".to_owned(),
    "/api/v1/search?query=alpha&kinds=build,build".to_owned(),
    "/api/v1/search?query=alpha&kinds=project&kinds=build&kinds=agent&kinds=agent_pool&kinds=build".to_owned(),
    "/api/v1/search?query=alpha&query=beta".to_owned(),
    "/api/v1/search?query=alpha&after=not-a-cursor".to_owned(),
    format!("/api/v1/search?query=alpha&after={oversized_cursor}"),
    format!("/api/v1/search?query=beta&after={SEARCH_CURSOR}"),
    format!("/api/v1/search?query=alpha&kinds=project&after={SEARCH_CURSOR}"),
    "/api/v1/search?query=alpha&limit=0".to_owned(),
    "/api/v1/search?query=alpha&limit=1&limit=2".to_owned(),
    format!(
      "/api/v1/search?query=alpha&limit={}",
      octacity_server_application::MAX_RESOURCE_SEARCH_RESULT_PAGE_SIZE + 1
    ),
    "/api/v1/search?query=alpha&offset=1".to_owned(),
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
