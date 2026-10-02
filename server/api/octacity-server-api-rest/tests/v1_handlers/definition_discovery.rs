use super::*;

const PROJECT_ID: &str = "11111111-1111-4111-8111-111111111111";
const MANUAL_TRIGGER_ID: &str = "88888888-8888-4888-8888-888888888888";

#[tokio::test]
async fn gets_one_exact_manual_trigger_definition_version() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );

  let response = routes
    .oneshot(empty_request(
      "GET",
      &format!("/api/v1/trigger-definitions/manual/{MANUAL_TRIGGER_ID}/versions/1"),
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(
    body,
    serde_json::json!({
      "trigger_id": MANUAL_TRIGGER_ID,
      "version": 1,
      "configuration_id": "44444444-4444-4444-8444-444444444444",
      "configuration_version": 1,
      "enabled": true,
      "definition": {"reason": "operator"},
      "created_at_unix_ms": 1_700_000_000_400_i64
    })
  );
}

#[tokio::test]
async fn maps_all_current_definition_pages_and_exclusive_cursors() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );
  let cases = [
    (
      "pipelines",
      "22222222-2222-4222-8222-222222222222",
      include_str!("../../fixtures/v1/pipeline-summary-page.json"),
    ),
    (
      "repositories",
      "33333333-3333-4333-8333-333333333333",
      include_str!("../../fixtures/v1/repository-summary-page.json"),
    ),
    (
      "build-configurations",
      "44444444-4444-4444-8444-444444444444",
      include_str!("../../fixtures/v1/build-configuration-summary-page.json"),
    ),
    (
      "trigger-definitions",
      "88888888-8888-4888-8888-888888888888",
      include_str!("../../fixtures/v1/trigger-definition-summary-page.json"),
    ),
  ];

  for (collection, cursor, fixture) in cases {
    let first = routes
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("/api/v1/projects/{PROJECT_ID}/{collection}?limit=1"),
        None,
      ))
      .await
      .unwrap();
    assert_eq!(first.status(), StatusCode::OK, "{collection}");
    let first: serde_json::Value =
      serde_json::from_slice(&to_bytes(first.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(first, serde_json::from_str::<serde_json::Value>(fixture).unwrap());

    let following = routes
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("/api/v1/projects/{PROJECT_ID}/{collection}?after={cursor}&limit=1"),
        None,
      ))
      .await
      .unwrap();
    assert_eq!(following.status(), StatusCode::OK, "{collection}");
    let following: serde_json::Value =
      serde_json::from_slice(&to_bytes(following.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(following, serde_json::json!({"items": [], "next_cursor": null}));
  }

  assert_eq!(
    *application.calls.lock().unwrap(),
    [
      "list_project_pipelines",
      "list_project_pipelines",
      "list_project_repositories",
      "list_project_repositories",
      "list_project_build_configurations",
      "list_project_build_configurations",
      "list_project_trigger_definitions",
      "list_project_trigger_definitions",
    ]
  );
}

#[tokio::test]
async fn rejects_invalid_definition_pagination_before_dispatch() {
  let application = Arc::new(RecordingApplication::default());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );

  for query in ["after=not-a-stable-id", "limit=0", "limit=201", "limit=1&offset=1"] {
    let response = routes
      .clone()
      .oneshot(empty_request(
        "GET",
        &format!("/api/v1/projects/{PROJECT_ID}/pipelines?{query}"),
        None,
      ))
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{query}");
  }
  assert!(application.calls.lock().unwrap().is_empty());
}
