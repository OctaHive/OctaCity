use super::*;
#[tokio::test]
async fn issue_timeline_http_is_bounded_metadata_only_and_usable_without_factory() {
  let router = issue_router(Arc::new(TrustedNetworkManagementPolicy)).await;
  let created = document(
    router
      .clone()
      .oneshot(json_request("POST", &root(), "activity", None, &draft().to_string()))
      .await
      .unwrap(),
  )
  .await;
  let issue = created["resource"]["id"].as_str().unwrap();
  let path = format!("{}/{issue}/timeline", root());
  let response = router
    .clone()
    .oneshot(empty_request("GET", &format!("{path}?limit=1"), None))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers()["cache-control"], "no-store");
  let first = document(response).await;
  assert_eq!(first["items"].as_array().unwrap().len(), 1);
  assert_eq!(first["items"][0]["kind"], "revision");
  assert_eq!(first["boundary"]["issue_version"], 1);
  assert_eq!(first["source_unavailable"], false);
  let cursor = first["next_cursor"].as_str().unwrap();
  let last = document(
    router
      .clone()
      .oneshot(empty_request("GET", &format!("{path}?limit=1&after={cursor}"), None))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(last["items"][0]["kind"], "status");
  assert!(last["next_cursor"].is_null());
  assert_eq!(last["boundary"], first["boundary"]);
  assert!(!first.to_string().contains("Manual draft"));
  for query in [
    "limit=0",
    "limit=201",
    "kinds=unknown",
    "kinds=",
    "extra=1",
    "after=bad",
  ] {
    assert_eq!(
      router
        .clone()
        .oneshot(empty_request("GET", &format!("{path}?{query}"), None))
        .await
        .unwrap()
        .status(),
      StatusCode::BAD_REQUEST,
      "{query}"
    );
  }
  let filtered = document(
    router
      .clone()
      .oneshot(empty_request("GET", &format!("{path}?kinds=status&limit=1"), None))
      .await
      .unwrap(),
  )
  .await;
  assert_eq!(filtered["items"][0]["kind"], "status");
  assert!(filtered["next_cursor"].is_null());
}
#[test]
fn issue_activity_openapi_declares_closed_bounded_independent_observations() {
  let doc = openapi_document();
  let schemas = &doc["components"]["schemas"];
  for name in ["IssueTimelinePage", "IssueTimelineBoundary", "IssueResultPage"] {
    assert_eq!(schemas[name]["additionalProperties"], false, "{name}");
  }
  assert_eq!(schemas["IssueTimelinePage"]["properties"]["items"]["maxItems"], 200);
  assert_eq!(schemas["IssueResultPage"]["properties"]["items"]["maxItems"], 100);
  assert!(schemas["IssueResultPage"]["properties"].get("accepted").is_none());
  assert_eq!(
    schemas["IssueResultCoverage"]["enum"],
    serde_json::json!(["partial", "stale", "unavailable"])
  );
  for (suffix, name, limit) in [
    ("timeline", "listIssueTimeline", 200),
    ("results", "listIssueResults", 100),
  ] {
    let operation = &doc["paths"][format!("/api/v1/projects/{{project_id}}/issues/{{issue_id}}/{suffix}")]["get"];
    assert_eq!(operation["operationId"], name);
    assert_eq!(
      operation["responses"]["200"]["headers"]["Cache-Control"]["schema"]["const"],
      "no-store"
    );
    let parameters = operation["parameters"].as_array().unwrap();
    assert_eq!(
      parameters.iter().find(|p| p["name"] == "limit").unwrap()["schema"]["maximum"],
      limit
    );
    assert_eq!(
      parameters.iter().find(|p| p["name"] == "after").unwrap()["schema"]["maxLength"],
      2048
    );
  }
}

#[tokio::test]
async fn issue_results_http_rejects_unbounded_or_incomplete_selection_before_dispatch() {
  let peer = Arc::new(RecordingApplication::default());
  let router =
    management_router_with_application(|| true, recording_management_application(peer.clone(), peer.clone()));
  let path = format!("{}/11111111-1111-4111-8111-111111111111/results", root());
  for query in [
    "collection=evidence",
    "work_id=11111111-1111-4111-8111-111111111111",
    "work_id=11111111-1111-4111-8111-111111111111&collection=evidence&limit=101",
    "work_id=11111111-1111-4111-8111-111111111111&collection=evidence&after=bad",
  ] {
    assert_eq!(
      router
        .clone()
        .oneshot(empty_request("GET", &format!("{path}?{query}"), None))
        .await
        .unwrap()
        .status(),
      StatusCode::BAD_REQUEST
    );
  }
  assert!(peer.calls.lock().unwrap().is_empty());
}
