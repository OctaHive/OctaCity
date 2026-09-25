use super::*;

#[tokio::test]
async fn audit_reads_are_bounded_and_represent_unauthenticated_management_honestly() {
  let application = Arc::new(RecordingApplication::successful_workflow());
  let routes = management_router_with_application(
    || true,
    recording_management_application(Arc::clone(&application), Arc::clone(&application)),
  );

  let response = routes
    .clone()
    .oneshot(empty_request(
      "GET",
      "/api/v1/audit-facts?actor_kind=unauthenticated_management&operation=cancel-build&limit=1",
      None,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: serde_json::Value =
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
  assert_eq!(body["items"][0]["actor"]["kind"], "unauthenticated_management");
  assert_eq!(body["items"][0]["actor"]["identity"], serde_json::Value::Null);
  assert_eq!(body["items"][0]["request_identity"], "cancel-build:cancel-17");
  assert!(body["items"][0].get("request_body").is_none());
  assert_eq!(application.calls.lock().unwrap().as_slice(), ["list_audit_facts"]);

  let oversized = routes
    .oneshot(empty_request("GET", "/api/v1/audit-facts?limit=201", None))
    .await
    .unwrap();
  assert_eq!(oversized.status(), StatusCode::BAD_REQUEST);
}
