use super::*;

#[tokio::test]
async fn maps_filters_cursor_and_freshness_without_backend_details() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );
  let target = concat!(
    "/api/v1/projects/11111111-1111-4111-8111-111111111111/build-logs/search",
    "?query=error%5BE0425%5D&mode=literal",
    "&build_id=99999999-9999-4999-8999-999999999999",
    "&attempt_id=aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
    "&job_id=bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
    "&stream=stderr&occurred_from_unix_ms=1699999999000&occurred_through_unix_ms=1700000001000&limit=1"
  );
  let first = routes
    .clone()
    .oneshot(empty_request("GET", target, None))
    .await
    .unwrap();
  assert_eq!(first.status(), StatusCode::OK);
  let first: serde_json::Value =
    serde_json::from_slice(&to_bytes(first.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(
    first,
    serde_json::from_str::<serde_json::Value>(include_str!("../../fixtures/v1/build-log-search-page.json")).unwrap()
  );
  for forbidden in ["bucket", "object_key", "tsvector", "postgres_rank"] {
    assert!(!first.to_string().contains(forbidden));
  }

  let cursor = first["next_cursor"].as_str().unwrap();
  let second = routes
    .oneshot(empty_request(
      "GET",
      &format!(
        "/api/v1/projects/11111111-1111-4111-8111-111111111111/build-logs/search?query=error%5BE0425%5D&mode=literal&after={cursor}&limit=1"
      ),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(second.status(), StatusCode::OK);
  let second: serde_json::Value =
    serde_json::from_slice(&to_bytes(second.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(second["items"], serde_json::json!([]));
  assert_eq!(second["next_cursor"], serde_json::Value::Null);
  assert_eq!(second["freshness"]["indexed_through"], 17);
  assert_eq!(second["freshness"]["committed_through"], 19);
  assert_eq!(second["freshness"]["caught_up"], false);

  let queries = application.log_search_queries.lock().unwrap();
  assert_eq!(queries.len(), 2);
  let first_query = &queries[0].search;
  assert_eq!(first_query.text, "error[E0425]");
  assert_eq!(first_query.limit, 1);
  assert!(first_query.build_id.is_some());
  assert!(first_query.attempt_id.is_some());
  assert!(first_query.job_id.is_some());
  assert_eq!(first_query.stream, Some(BuildLogStream::Stderr));
  assert_eq!(first_query.occurred_from.unwrap().unix_millis(), 1_699_999_999_000);
  assert_eq!(first_query.occurred_through.unwrap().unix_millis(), 1_700_000_001_000);
  assert_eq!(
    queries[1].search.after.unwrap().occurred_at.unix_millis(),
    1_700_000_000_000
  );
}

#[tokio::test]
async fn rejects_unsupported_modes_and_oversized_queries_before_dispatch() {
  let application = Arc::new(RecordingApplication::default());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );
  for target in [
    "/api/v1/projects/11111111-1111-4111-8111-111111111111/build-logs/search?query=error&mode=regex".to_owned(),
    format!(
      "/api/v1/projects/11111111-1111-4111-8111-111111111111/build-logs/search?query={}&mode=literal",
      "x".repeat(1_025)
    ),
  ] {
    let response = routes
      .clone()
      .oneshot(empty_request("GET", &target, None))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  }
  assert!(application.calls.lock().unwrap().is_empty());
}
